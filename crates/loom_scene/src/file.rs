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
    #[serde(default)]
    pub children: Vec<NodeDesc>,
}

/// Built-in unit meshes; size them with `scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MeshKind {
    /// Edge 1, centered at the origin.
    Cube,
    /// 1x1 on the XZ plane, facing +Y.
    Plane,
    /// Diameter 1, centered at the origin.
    Sphere,
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
