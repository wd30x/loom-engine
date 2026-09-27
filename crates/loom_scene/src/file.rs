//! The scene file format as written by people and AI. Parsed with ron's
//! `IMPLICIT_SOME`, so optional fields are written bare (`mesh: Cube`, not
//! `mesh: Some(Cube)`). Every struct denies unknown fields: ron would
//! otherwise silently ignore a misspelled one.
//!
//! Serializing these structs gives the canonical format
//! (`docs/architecture.md` section 10, "规范格式"): fields in declaration
//! order, absent optional fields and empty lists left out.

use serde::{Deserialize, Serialize};

pub(crate) type Triple = (f32, f32, f32);

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SceneFile {
    pub version: u32,
    /// Which game runs the scene; `loom_scene` knows no game names.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// Path of the active camera node.
    pub camera: String,
    /// Ambient light, linear RGB.
    pub ambient: Triple,
    pub nodes: Vec<NodeDesc>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeDesc {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Triple>,
    /// YXZ Euler angles in degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Triple>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_at: Option<Triple>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Triple>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<MeshKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Triple>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDesc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprite: Option<SpriteDesc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tilemap: Option<TilemapDesc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<LightDesc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterDesc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orbit_camera: Option<OrbitCameraDesc>,
    /// Numbers for the game to interpret, in file order. A list, not a map:
    /// ron silently keeps the last of two duplicate map keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<(String, f64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collider: Option<ColliderKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeDesc>,
}

/// Static collision shapes, sized and placed by the node's transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum ColliderKind {
    /// Unit cube like `mesh: Cube`.
    Box,
}

/// Mesh shapes. The first three are unit sized; size them with `scale`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CameraDesc {
    /// Perspective: vertical field of view in degrees. Exactly one of
    /// `fov_y` and `ortho` is given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov_y: Option<f32>,
    /// Orthographic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ortho: Option<OrthoDesc>,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrthoDesc {
    /// World length the view covers vertically; the width follows the
    /// aspect ratio.
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpriteDesc {
    /// Path of an Aseprite json-array sprite sheet.
    pub sheet: String,
    pub tag: String,
    /// Other tags the game may switch the sprite to while it runs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TilemapDesc {
    /// Path of an Aseprite json-array sprite sheet.
    pub sheet: String,
    /// Which tag each map character draws.
    pub legend: Vec<(char, String)>,
    /// One string per row, top row first; one character per cell.
    pub rows: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LightDesc {
    pub color: Triple,
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterDesc {
    /// Meters per second.
    pub run_speed: f32,
    /// Degrees per second.
    pub turn_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
    /// Radius of the camera's collision sphere, in meters.
    pub collision_radius: f32,
    /// Meters per second the camera moves back out once unblocked.
    pub return_speed: f32,
}

/// The fields `edit_node`'s `set` may give: the node's own, every one
/// optional, plus `children` so it can be refused with its own message.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodePatch {
    pub name: Option<String>,
    pub position: Option<Triple>,
    pub rotation: Option<Triple>,
    pub look_at: Option<Triple>,
    pub scale: Option<Triple>,
    pub mesh: Option<MeshKind>,
    pub color: Option<Triple>,
    pub camera: Option<CameraDesc>,
    pub sprite: Option<SpriteDesc>,
    pub tilemap: Option<TilemapDesc>,
    pub light: Option<LightDesc>,
    pub character: Option<CharacterDesc>,
    pub orbit_camera: Option<OrbitCameraDesc>,
    pub props: Option<Vec<(String, f64)>>,
    pub collider: Option<ColliderKind>,
    pub children: Option<ron::Value>,
}

/// The top-level fields `edit_scene`'s `set` may give, plus `version` and
/// `game` so they can be refused with their own message.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenePatch {
    pub camera: Option<String>,
    pub ambient: Option<Triple>,
    pub version: Option<ron::Value>,
    pub game: Option<ron::Value>,
    pub nodes: Option<ron::Value>,
}
