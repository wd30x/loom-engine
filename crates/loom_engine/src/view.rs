//! Scene to renderer: builds the [`Frame`] for the current scene state. The
//! renderer and the scene crate know nothing of each other; this is the only
//! place that knows both. Also `loom view`, which renders a scene once
//! without simulating it.

use std::time::Instant;

use anyhow::{Context, Result};
use glam::{Mat4, Vec2, Vec3};
use loom_core::hud::{GLYPH_CELL, Hud, HudItem, HudView, glyph_cell};
use loom_render::{
    DirectionalLight, DrawItem, DrawMesh, Frame, HudDraw, JointRange, LineDraw, MeshId, Projection,
    RenderError, Renderer, SpriteDraw, TextureId, view_projection,
};
use loom_scene::{Model, NodeId, Scene};

use crate::ViewArgs;
use crate::model::Models;
use crate::output::ensure_parent_dir;
use crate::report::ViewReport;
use crate::run::millis_since;
use crate::sheet::Sheets;

/// A game's HUD for a view: what [`render_scene`] draws over the scene.
pub(crate) type HudFn<'a> = &'a dyn Fn(&HudView, &mut Hud);

/// Draws `scene` from its camera at game tick `tick` (which poses the
/// animated models), with the HUD `hud` lays out (if any) over it and the
/// debug `lines` on top. `sheets` and `models` must have been loaded for
/// `scene` ([`Sheets::load`], [`Models::load`]) and only ever be drawn
/// with `renderer`. Returns `false` if the frame was skipped (zero render
/// size).
pub(crate) fn render_scene(
    renderer: &mut Renderer,
    scene: &Scene,
    sheets: &mut Sheets,
    models: &mut Models,
    lines: &[LineDraw],
    hud: Option<HudFn<'_>>,
    tick: u64,
) -> Result<bool, RenderError> {
    let mut joints = Vec::new();
    let draws = mesh_draws(scene, |model| {
        models.draw(renderer, model, tick, &mut joints)
    })?;
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
            let font = sheets.font(renderer).transpose()?;
            hud_draws(items.items(), height, font, |sheet, tag| {
                sheets.hud_image(renderer, sheet, tag).transpose()
            })?
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
        joints: &joints,
        sprites: &sprites,
        hud: &hud,
        lines,
    })
}

/// HUD items in HUD units to draws in pixels of a frame `height` pixels
/// high: text as one quad a character from the `font` texture, an image as
/// one quad of the texture and rectangle `image(sheet, tag)` gives. Text
/// without a font, and an image `image` gives `None` for (not in the game's
/// HUD sheets), is left out with an error in the log.
fn hud_draws(
    items: &[HudItem],
    height: u32,
    font: Option<TextureId>,
    mut image: impl FnMut(&str, &str) -> Result<Option<(TextureId, [u32; 4])>, RenderError>,
) -> Result<Vec<HudDraw>, RenderError> {
    let px = height as f32 / loom_core::hud::HUD_HEIGHT;
    let [cell_w, cell_h] = GLYPH_CELL;
    let mut draws = Vec::with_capacity(items.len());
    for item in items {
        match item {
            &HudItem::Rect { min, size, color } => draws.push(HudDraw::Rect {
                min: min * px,
                size: size * px,
                color,
            }),
            &HudItem::Disc {
                center,
                radius,
                color,
            } => draws.push(HudDraw::Disc {
                center: center * px,
                radius: radius * px,
                color,
            }),
            HudItem::Text {
                at,
                scale,
                color,
                text,
                shadow,
            } => {
                let Some(texture) = font else {
                    log::error!("HUD text {text:?} is not drawn: there is no font");
                    continue;
                };
                // Every glyph's shadow, one glyph pixel right and down,
                // then every glyph over them.
                let layers = shadow
                    .map(|shadow| (Vec2::splat(*scale), shadow))
                    .into_iter()
                    .chain([(Vec2::ZERO, *color)]);
                for (offset, color) in layers {
                    for (i, c) in text.chars().enumerate() {
                        let [col, row] = glyph_cell(c);
                        let x = at.x + (i as f32) * cell_w as f32 * scale;
                        draws.push(HudDraw::Image {
                            texture,
                            min: (Vec2::new(x, at.y) + offset) * px,
                            // The glyph, without the blank column and row.
                            size: Vec2::new((cell_w - 1) as f32, (cell_h - 1) as f32) * *scale * px,
                            rect: [col * cell_w, row * cell_h, cell_w - 1, cell_h - 1],
                            color,
                        });
                    }
                }
            }
            &HudItem::Image {
                sheet,
                tag,
                at,
                scale,
                color,
            } => {
                let Some((texture, rect)) = image(sheet, tag)? else {
                    log::error!(
                        "HUD image {tag:?} of {sheet} is not drawn: it is not in the game's HUD sheets"
                    );
                    continue;
                };
                draws.push(HudDraw::Image {
                    texture,
                    min: at * px,
                    size: Vec2::new(rect[2] as f32, rect[3] as f32) * scale * px,
                    rect,
                    color,
                });
            }
        }
    }
    Ok(draws)
}

/// One draw per drawn node with a mesh or a model ([`Scene::is_drawn`]),
/// in scene pre-order; `model` gives the mesh of a model, and its joint
/// range if it is animated.
fn mesh_draws(
    scene: &Scene,
    mut model: impl FnMut(&Model) -> Result<(MeshId, Option<JointRange>), RenderError>,
) -> Result<Vec<DrawItem>, RenderError> {
    let mut draws = Vec::new();
    for id in scene.ids().filter(|&id| scene.is_drawn(id)) {
        let node = scene.node(id);
        let (mesh, color, joints) = match (&node.mesh, &node.model) {
            (Some(mesh), _) => (mesh_kind(mesh.kind).into(), mesh.color, None),
            (None, Some(m)) => {
                let (mesh, joints) = model(m)?;
                (DrawMesh::Id(mesh), m.color, joints)
            }
            (None, None) => continue,
        };
        draws.push(DrawItem {
            mesh,
            model: scene.world_matrix(id),
            color,
            joints,
        });
    }
    Ok(draws)
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
/// `game`, nor what a game requires) and without ticking, once for each of
/// `--ticks` (which only poses the animated models), and saves the frames
/// left to right as one screenshot. The scene, its sheets, models and
/// clips are checked before the GPU is touched. `started` is the process
/// start, for `renderer_init_ms`.
pub(crate) fn view(args: &ViewArgs, started: Instant) -> Result<ViewReport> {
    let path = &args.scene;
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading scene {}", path.display()))?;
    let scene = Scene::parse(&src).with_context(|| format!("invalid scene {}", path.display()))?;
    // No game, so no HUD sheets.
    let mut sheets =
        Sheets::load(&scene, &[]).with_context(|| format!("invalid scene {}", path.display()))?;
    let mut models =
        Models::load(&scene).with_context(|| format!("invalid scene {}", path.display()))?;
    let (width, height) = (args.size.width, args.size.height);
    let mut renderer = Renderer::new_headless(width, height)?;
    let renderer_init_ms = millis_since(started);
    let ticks = &args.ticks.0;
    let frames = u32::try_from(ticks.len()).expect("at most 16 ticks");
    let strip_width = width
        .checked_mul(frames)
        .context("the frames are too wide together")?;
    let mut strip = image::RgbaImage::new(strip_width, height);
    for (i, &tick) in (0u32..).zip(ticks) {
        render_scene(
            &mut renderer,
            &scene,
            &mut sheets,
            &mut models,
            &[],
            None,
            tick,
        )?;
        let pixels = renderer.read_pixels().context("reading back the frame")?;
        let frame = image::RgbaImage::from_raw(width, height, pixels)
            .context("readback size does not match the render size")?;
        image::imageops::replace(&mut strip, &frame, i64::from(i * width), 0);
    }
    let path_shown = args.screenshot.display().to_string();
    ensure_parent_dir(&args.screenshot)?;
    strip
        .save_with_format(&args.screenshot, image::ImageFormat::Png)
        .with_context(|| format!("writing screenshot {path_shown}"))?;
    log::info!("screenshot saved to {path_shown}");
    let adapter = renderer.adapter_summary();
    renderer.shutdown();
    Ok(ViewReport {
        status: "ok",
        mode: "view",
        screenshot: path_shown,
        size: [width, height],
        ticks: ticks.clone(),
        scene: path.display().to_string(),
        adapter: adapter.into(),
        renderer_init_ms,
    })
}

#[cfg(test)]
mod tests {
    use glam::{Vec2, Vec4};

    use super::*;

    /// The font's and every image's stand-in texture in these tests.
    fn font() -> Option<TextureId> {
        Some(TextureId::from_index(0))
    }

    /// `hud_draws` with the test font and one declared image: `flask` of
    /// `icons.json`, 16x16 at (16, 0) in texture 1.
    fn draws(items: &[HudItem], height: u32) -> Vec<HudDraw> {
        hud_draws(items, height, font(), |sheet, tag| {
            Ok((sheet == "icons.json" && tag == "flask")
                .then(|| (TextureId::from_index(1), [16, 0, 16, 16])))
        })
        .unwrap()
    }

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
            draws(&items, 720),
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
            draws(&items, 360)[0],
            HudDraw::Rect {
                min: Vec2::new(20.0, 20.0),
                size: Vec2::new(150.0, 6.0),
                color: Vec4::ONE,
            }
        );
    }

    fn glyph(x: f32, y: f32, scale: f32, rect: [u32; 4], color: Vec4) -> HudDraw {
        HudDraw::Image {
            texture: TextureId::from_index(0),
            min: Vec2::new(x, y),
            size: Vec2::new(5.0, 7.0) * scale,
            rect,
            color,
        }
    }

    #[test]
    fn text_is_one_glyph_quad_a_character_six_units_apart() {
        let color = Vec4::new(0.9, 0.8, 0.7, 1.0);
        let items = [HudItem::Text {
            at: Vec2::new(80.0, 657.0),
            scale: 2.0,
            color,
            text: "A1é".to_owned(),
            shadow: None,
        }];
        // 'A' (65) is in column 1, row 2; '1' (49) in column 1, row 1; 'é'
        // is drawn as '?' (63): column 15, row 1. Each glyph is the 5x7 at
        // its 6x8 cell's top left.
        assert_eq!(
            draws(&items, 720),
            [
                glyph(80.0, 657.0, 2.0, [6, 16, 5, 7], color),
                glyph(92.0, 657.0, 2.0, [6, 8, 5, 7], color),
                glyph(104.0, 657.0, 2.0, [90, 8, 5, 7], color),
            ]
        );
        // Twice the pixels at twice the height.
        assert_eq!(
            draws(&items, 1440)[1],
            glyph(184.0, 1314.0, 4.0, [6, 8, 5, 7], color)
        );
        let empty = [HudItem::Text {
            at: Vec2::ZERO,
            scale: 2.0,
            color,
            text: String::new(),
            shadow: Some(Vec4::ZERO),
        }];
        assert!(draws(&empty, 720).is_empty());
    }

    #[test]
    fn shadowed_text_draws_every_shadow_first_one_glyph_pixel_down_right() {
        let color = Vec4::new(0.9, 0.8, 0.7, 1.0);
        let shadow = Vec4::new(0.0, 0.0, 0.0, 0.75);
        let items = [HudItem::Text {
            at: Vec2::new(80.0, 657.0),
            scale: 2.0,
            color,
            text: "AB".to_owned(),
            shadow: Some(shadow),
        }];
        // 'A' is in column 1, row 2; 'B' in column 2, row 2.
        let (a, b) = ([6, 16, 5, 7], [12, 16, 5, 7]);
        assert_eq!(
            draws(&items, 720),
            [
                glyph(82.0, 659.0, 2.0, a, shadow),
                glyph(94.0, 659.0, 2.0, b, shadow),
                glyph(80.0, 657.0, 2.0, a, color),
                glyph(92.0, 657.0, 2.0, b, color),
            ]
        );
        // Twice the pixels at twice the height: 4 pixels off.
        let big = draws(&items, 1440);
        assert_eq!(big[0], glyph(164.0, 1318.0, 4.0, a, shadow));
        assert_eq!(big[2], glyph(160.0, 1314.0, 4.0, a, color));
    }

    #[test]
    fn image_is_its_tags_first_frame_scaled() {
        let color = Vec4::new(0.35, 0.35, 0.35, 1.0);
        let items = [HudItem::Image {
            sheet: "icons.json",
            tag: "flask",
            at: Vec2::new(40.0, 648.0),
            scale: 2.0,
            color,
        }];
        assert_eq!(
            draws(&items, 720),
            [HudDraw::Image {
                texture: TextureId::from_index(1),
                min: Vec2::new(40.0, 648.0),
                size: Vec2::new(32.0, 32.0),
                rect: [16, 0, 16, 16],
                color,
            }]
        );
    }

    #[test]
    fn undeclared_images_and_text_without_a_font_are_left_out() {
        let rect = HudItem::Rect {
            min: Vec2::ZERO,
            size: Vec2::ONE,
            color: Vec4::ONE,
        };
        let items = [
            HudItem::Image {
                sheet: "icons.json",
                tag: "flsk",
                at: Vec2::ZERO,
                scale: 2.0,
                color: Vec4::ONE,
            },
            rect.clone(),
            HudItem::Image {
                sheet: "other.json",
                tag: "flask",
                at: Vec2::ZERO,
                scale: 2.0,
                color: Vec4::ONE,
            },
        ];
        assert_eq!(draws(&items, 720).len(), 1);
        let text = [
            HudItem::Text {
                at: Vec2::ZERO,
                scale: 2.0,
                color: Vec4::ONE,
                text: "0".to_owned(),
                shadow: Some(Vec4::ZERO),
            },
            rect,
        ];
        let no_font = hud_draws(&text, 720, None, |_, _| Ok(None)).unwrap();
        assert_eq!(no_font.len(), 1);
        assert!(matches!(no_font[0], HudDraw::Rect { .. }));
    }

    #[test]
    fn model_nodes_draw_their_mesh_id_at_their_world_matrix_tinted_in_pre_order() {
        let scene = Scene::parse(
            r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
                (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
                (name: "a", position: (1.0, 0.0, 0.0), mesh: Cube, children: [
                    (name: "k", position: (0.0, 2.0, 0.0), model: "k.glb",
                        color: (0.5, 1.0, 1.0)),
                ]),
                (name: "j", model: "j.glb"),
            ])"#,
        )
        .unwrap();
        let mut asked = Vec::new();
        let draws = mesh_draws(&scene, |model| {
            asked.push(model.path.clone());
            let id = MeshId::from_index(if model.path == "k.glb" { 7 } else { 3 });
            let joints = (model.path == "j.glb").then_some(JointRange { first: 4, count: 2 });
            Ok((id, joints))
        })
        .unwrap();
        assert_eq!(asked, ["k.glb", "j.glb"]);
        assert_eq!(
            draws,
            [
                DrawItem {
                    mesh: DrawMesh::Kind(loom_render::MeshKind::Cube),
                    model: Mat4::from_translation(Vec3::X),
                    color: Vec3::splat(0.8),
                    joints: None,
                },
                DrawItem {
                    mesh: DrawMesh::Id(MeshId::from_index(7)),
                    model: Mat4::from_translation(Vec3::new(1.0, 2.0, 0.0)),
                    color: Vec3::new(0.5, 1.0, 1.0),
                    joints: None,
                },
                DrawItem {
                    mesh: DrawMesh::Id(MeshId::from_index(3)),
                    model: Mat4::IDENTITY,
                    color: Vec3::ONE,
                    joints: Some(JointRange { first: 4, count: 2 }),
                },
            ]
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
            mesh_draws(scene, |_| unreachable!("the scene has no model"))
                .unwrap()
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
