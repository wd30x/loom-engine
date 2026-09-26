//! Scene to renderer: builds the [`Frame`] for the current scene state. The
//! renderer and the scene crate know nothing of each other; this is the only
//! place that knows both.

use glam::Vec3;
use loom_render::{DirectionalLight, DrawItem, Frame, RenderError, Renderer};
use loom_scene::Scene;

/// Draws `scene` from its camera. Returns `false` if the frame was skipped
/// (zero render size).
pub(crate) fn render_scene(renderer: &mut Renderer, scene: &Scene) -> Result<bool, RenderError> {
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
        fov_y: camera.fov_y_degrees.to_radians(),
        near: camera.near,
        far: camera.far,
        ambient: scene.ambient(),
        light,
        draws: &draws,
    })
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
