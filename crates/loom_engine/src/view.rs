//! Scene to renderer: builds the [`Frame`] for the current scene state. The
//! renderer and the scene crate know nothing of each other; this is the only
//! place that knows both. Also `loom view`, which renders a scene once
//! without simulating it.

use std::time::Instant;

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use loom_core::hud::{Hud, HudItem, HudView};
use loom_render::{
    DirectionalLight, DrawItem, Frame, HudDraw, LineDraw, Projection, RenderError, Renderer,
    SpriteDraw, view_projection,
};
use loom_scene::{NodeId, Scene};

use crate::ViewArgs;
use crate::report::ViewReport;
use crate::run::{millis_since, save_screenshot};
use crate::sheet::Sheets;

/// A game's HUD for a view: what [`render_scene`] draws over the scene.
pub(crate) type HudFn<'a> = &'a dyn Fn(&HudView, &mut Hud);

/// Draws `scene` from its camera, with the HUD `hud` lays out (if any) over
/// it and the debug `lines` on top. `sheets` must have been loaded for
/// `scene` ([`Sheets::load`]) and only ever be drawn with `renderer`.
/// Returns `false` if the frame was skipped (zero render size).
pub(crate) fn render_scene(
    renderer: &mut Renderer,
    scene: &Scene,
    sheets: &mut Sheets,
    lines: &[LineDraw],
    hud: Option<HudFn<'_>>,
) -> Result<bool, RenderError> {
    let draws = mesh_draws(scene);
    let sprites = sprite_draws(renderer, scene, sheets)?;

    let camera_id = scene.camera();
    let camera = scene
        .node(camera_id)
        .camera
        .expect("Scene::parse guarantees the camera node has a camera");
    let light = scene.light().map(|id| {
        let light = scene
            .node(id)
            .light
            .expect("Scene::light only returns nodes with a light");
        DirectionalLight {
            direction: scene
                .world_matrix(id)
                .transform_vector3(Vec3::NEG_Z)
                .normalize(),
            color: light.color,
            intensity: light.intensity,
        }
    });

    let view = scene.world_matrix(camera_id).inverse();
    let projection = projection(camera.projection);
    let (width, height) = renderer.size();
    let hud = match hud {
        Some(hud) if width > 0 && height > 0 => {
            let aspect = width as f32 / height as f32;
            let view_proj = view_projection(view, projection, aspect, camera.near, camera.far);
            let mut items = Hud::new();
            hud(&HudView::new(width, height, view_proj), &mut items);
            hud_draws(items.items(), height)
        }
        _ => Vec::new(),
    };
    renderer.render(&Frame {
        view,
        projection,
        near: camera.near,
        far: camera.far,
        ambient: scene.ambient(),
        light,
        draws: &draws,
        sprites: &sprites,
        hud: &hud,
        lines,
    })
}

/// HUD items in HUD units to pixels of a frame `height` pixels high.
fn hud_draws(items: &[HudItem], height: u32) -> Vec<HudDraw> {
    let scale = height as f32 / loom_core::hud::HUD_HEIGHT;
    items
        .iter()
        .map(|&item| match item {
            HudItem::Rect { min, size, color } => HudDraw::Rect {
                min: min * scale,
                size: size * scale,
                color,
            },
            HudItem::Disc {
                center,
                radius,
                color,
            } => HudDraw::Disc {
                center: center * scale,
                radius: radius * scale,
                color,
            },
        })
        .collect()
}

/// One draw per drawn node with a mesh ([`Scene::is_drawn`]), in scene
/// pre-order.
fn mesh_draws(scene: &Scene) -> Vec<DrawItem> {
    scene
        .ids()
        .filter(|&id| scene.is_drawn(id))
        .filter_map(|id| {
            let mesh = scene.node(id).mesh?;
            Some(DrawItem {
                mesh: mesh_kind(mesh.kind),
                model: scene.world_matrix(id),
                color: mesh.color,
            })
        })
        .collect()
}

/// The sprite and tilemap nodes to draw ([`Scene::is_drawn`]), in scene
/// pre-order.
fn drawn_sprite_nodes(scene: &Scene) -> Vec<NodeId> {
    scene
        .sprite_nodes()
        .iter()
        .copied()
        .filter(|&id| scene.is_drawn(id))
        .collect()
}

/// One draw per drawn sprite node, and one per cell of each drawn tilemap
/// node (row by row, each row left to right), in scene pre-order.
fn sprite_draws(
    renderer: &mut Renderer,
    scene: &Scene,
    sheets: &mut Sheets,
) -> Result<Vec<SpriteDraw>, RenderError> {
    let mut sprites = Vec::new();
    for id in drawn_sprite_nodes(scene) {
        let node = scene.node(id);
        let world = scene.world_matrix(id);
        if let Some(sprite) = &node.sprite {
            // The current tag: the game may have switched it (e.g. to a corpse).
            let (texture, rect) = sheets.draw(renderer, &sprite.sheet, sprite.tag())?;
            sprites.push(SpriteDraw {
                texture,
                model: world,
                rect,
            });
        }
        if let Some(tilemap) = &node.tilemap {
            for (r, row) in tilemap.rows.iter().enumerate() {
                for (c, ch) in row.chars().enumerate() {
                    let tag = tilemap
                        .tag(ch)
                        .expect("Scene::parse checks every tilemap character is in the legend");
                    let (texture, rect) = sheets.draw(renderer, &tilemap.sheet, tag)?;
                    // Cell (r, c) is centered at (c, -r, 0): row 0 on top.
                    let offset = Vec3::new(c as f32, -(r as f32), 0.0);
                    sprites.push(SpriteDraw {
                        texture,
                        model: world * Mat4::from_translation(offset),
                        rect,
                    });
                }
            }
        }
    }
    Ok(sprites)
}

fn projection(projection: loom_scene::Projection) -> Projection {
    match projection {
        loom_scene::Projection::Perspective { fov_y_degrees } => Projection::Perspective {
            fov_y: fov_y_degrees.to_radians(),
        },
        loom_scene::Projection::Orthographic { height } => Projection::Orthographic { height },
    }
}

fn mesh_kind(kind: loom_scene::MeshKind) -> loom_render::MeshKind {
    match kind {
        loom_scene::MeshKind::Cube => loom_render::MeshKind::Cube,
        loom_scene::MeshKind::Plane => loom_render::MeshKind::Plane,
        loom_scene::MeshKind::Sphere => loom_render::MeshKind::Sphere,
        loom_scene::MeshKind::Capsule { radius, height } => {
            loom_render::MeshKind::Capsule { radius, height }
        }
    }
}

/// `loom view`: renders the scene as loaded, without a game (so it needs no
/// `game`, nor what a game requires) and without ticking, and saves a
/// screenshot. The scene and its sheets are checked before the GPU is
/// touched. `started` is the process start, for `renderer_init_ms`.
pub(crate) fn view(args: &ViewArgs, started: Instant) -> Result<ViewReport> {
    let path = &args.scene;
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading scene {}", path.display()))?;
    let scene = Scene::parse(&src).with_context(|| format!("invalid scene {}", path.display()))?;
    let mut sheets =
        Sheets::load(&scene).with_context(|| format!("invalid scene {}", path.display()))?;
    let mut renderer = Renderer::new_headless(args.size.width, args.size.height)?;
    let renderer_init_ms = millis_since(started);
    render_scene(&mut renderer, &scene, &mut sheets, &[], None)?;
    let screenshot = save_screenshot(&renderer, &args.screenshot)?;
    let adapter = renderer.adapter_summary();
    renderer.shutdown();
    Ok(ViewReport {
        status: "ok",
        mode: "view",
        screenshot,
        size: [args.size.width, args.size.height],
        scene: path.display().to_string(),
        adapter: adapter.into(),
        renderer_init_ms,
    })
}

#[cfg(test)]
mod tests {
    use glam::{Vec2, Vec4};

    use super::*;

    #[test]
    fn hud_units_scale_to_pixels_by_the_frame_height() {
        let items = [
            HudItem::Rect {
                min: Vec2::new(40.0, 40.0),
                size: Vec2::new(300.0, 12.0),
                color: Vec4::ONE,
            },
            HudItem::Disc {
                center: Vec2::new(640.0, 360.0),
                radius: 5.0,
                color: Vec4::ZERO,
            },
        ];
        // 720 high: one unit a pixel.
        assert_eq!(
            hud_draws(&items, 720),
            [
                HudDraw::Rect {
                    min: Vec2::new(40.0, 40.0),
                    size: Vec2::new(300.0, 12.0),
                    color: Vec4::ONE,
                },
                HudDraw::Disc {
                    center: Vec2::new(640.0, 360.0),
                    radius: 5.0,
                    color: Vec4::ZERO,
                },
            ]
        );
        // 360 high: half a pixel a unit.
        assert_eq!(
            hud_draws(&items, 360)[0],
            HudDraw::Rect {
                min: Vec2::new(20.0, 20.0),
                size: Vec2::new(150.0, 6.0),
                color: Vec4::ONE,
            }
        );
    }

    #[test]
    fn hidden_mesh_nodes_and_their_subtrees_are_not_drawn() {
        let mut scene = Scene::parse(
            r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
                (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
                (name: "a", mesh: Cube, color: (1.0, 0.0, 0.0), children: [
                    (name: "b", mesh: Cube, color: (0.0, 1.0, 0.0)),
                ]),
                (name: "c", mesh: Cube, color: (0.0, 0.0, 1.0)),
            ])"#,
        )
        .unwrap();
        let colors = |scene: &Scene| {
            mesh_draws(scene)
                .iter()
                .map(|d| d.color)
                .collect::<Vec<_>>()
        };
        assert_eq!(colors(&scene), [Vec3::X, Vec3::Y, Vec3::Z]);
        let a = scene.find("a").unwrap();
        scene.set_visible(a, false);
        assert_eq!(colors(&scene), [Vec3::Z]);
        scene.set_visible(a, true);
        assert_eq!(colors(&scene), [Vec3::X, Vec3::Y, Vec3::Z]);
    }

    #[test]
    fn hidden_sprite_and_tilemap_nodes_are_not_drawn() {
        let mut scene = Scene::parse(include_str!("../../../assets/scenes/rogue.ron")).unwrap();
        let names = |scene: &Scene| {
            drawn_sprite_nodes(scene)
                .into_iter()
                .map(|id| scene.path(id))
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&scene), ["map", "player", "zombie", "zombie_2"]);
        for hidden in ["map", "zombie"] {
            let id = scene.find(hidden).unwrap();
            scene.set_visible(id, false);
        }
        assert_eq!(names(&scene), ["player", "zombie_2"]);
    }
}
