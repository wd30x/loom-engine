//! The scene file format as written by people and AI. Parsed with ron's
//! `IMPLICIT_SOME`, so optional fields are written bare (`mesh: Cube`, not
//! `mesh: Some(Cube)`). Every struct denies unknown fields: ron would
//! otherwise silently ignore a misspelled one.

use serde::Deserialize;

pub(crate) type Triple = (f32, f32, f32);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SceneFile {
    pub version: u32,
    /// Path of the active camera node.
    pub camera: String,
    /// Ambient light, linear RGB.
    pub ambient: Triple,
    pub nodes: Vec<NodeDesc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeDesc {
    pub name: String,
    pub position: Option<Triple>,
    /// YXZ Euler angles in degrees.
    pub rotation: Option<Triple>,
    pub look_at: Option<Triple>,
    pub scale: Option<Triple>,
    pub mesh: Option<MeshKind>,
    pub color: Option<Triple>,
    pub camera: Option<CameraDesc>,
    pub light: Option<LightDesc>,
    pub character: Option<CharacterDesc>,
    pub orbit_camera: Option<OrbitCameraDesc>,
    pub collider: Option<ColliderKind>,
    #[serde(default)]
    pub children: Vec<NodeDesc>,
}

/// Static collision shapes, sized and placed by the node's transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum ColliderKind {
    /// Unit cube like `mesh: Cube`.
    Box,
}

/// Mesh shapes. The first three are unit sized; size them with `scale`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum MeshKind {
    /// Edge 1, centered at the origin.
    Cube,
    /// 1x1 on the XZ plane, facing +Y.
    Plane,
    /// Diameter 1, centered at the origin.
    Sphere,
    /// A cylinder along Y capped by two half spheres. `height` includes the
    /// caps; the origin is at the bottom, so it spans y in `[0, height]`.
    /// Requires `radius > 0` and `height >= 2 * radius`.
    Capsule { radius: f32, height: f32 },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CameraDesc {
    /// Vertical field of view in degrees.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LightDesc {
    pub color: Triple,
    pub intensity: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterDesc {
    /// Meters per second.
    pub run_speed: f32,
    /// Degrees per second.
    pub turn_speed: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrbitCameraDesc {
    /// Path (from a root) of the node with the `character` component.
    pub target: String,
    pub pivot_height: f32,
    pub distance: f32,
    /// Angles in degrees.
    pub yaw: f32,
    pub pitch: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
}
