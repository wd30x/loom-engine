//! Scene to renderer: builds the [`Frame`] for the current scene state. The
//! renderer and the scene crate know nothing of each other; this is the only
//! place that knows both. Also `loom view`, which renders a scene once
//! without simulating it.

use std::time::Instant;

use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use loom_render::{
    DirectionalLight, DrawItem, Frame, Projection, RenderError, Renderer, SpriteDraw,
};
use loom_scene::Scene;

use crate::ViewArgs;
use crate::report::ViewReport;
use crate::run::{millis_since, save_screenshot};
use crate::sheet::Sheets;

/// Draws `scene` from its camera. `sheets` must have been loaded for `scene`
/// ([`Sheets::load`]) and only ever be drawn with `renderer`. Returns
/// `false` if the frame was skipped (zero render size).
pub(crate) fn render_scene(
    renderer: &mut Renderer,
    scene: &Scene,
    sheets: &mut Sheets,
) -> Result<bool, RenderError> {
    let draws: Vec<DrawItem> = scene
        .ids()
        .filter_map(|id| {
            let mesh = scene.node(id).mesh?;
            Some(DrawItem {
                mesh: mesh_kind(mesh.kind),
                model: scene.world_matrix(id),
                color: mesh.color,
            })
        })
        .collect();
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

    renderer.render(&Frame {
        view: scene.world_matrix(camera_id).inverse(),
        projection: projection(camera.projection),
        near: camera.near,
        far: camera.far,
        ambient: scene.ambient(),
        light,
        draws: &draws,
        sprites: &sprites,
    })
}

/// One draw per sprite node, and one per cell of each tilemap node (row by
/// row, each row left to right), in scene pre-order.
fn sprite_draws(
    renderer: &mut Renderer,
    scene: &Scene,
    sheets: &mut Sheets,
) -> Result<Vec<SpriteDraw>, RenderError> {
    let mut sprites = Vec::new();
    for &id in scene.sprite_nodes() {
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
    render_scene(&mut renderer, &scene, &mut sheets)?;
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
