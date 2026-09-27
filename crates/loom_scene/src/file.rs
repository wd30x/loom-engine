//! The scene file format as written by people and AI. Parsed with ron's
//! `IMPLICIT_SOME`, so optional fields are written bare (`mesh: Cube`, not
//! `mesh: Some(Cube)`). Every struct denies unknown fields: ron would
//! otherwise silently ignore a misspelled one.
//!
//! Serializing these structs gives the canonical format
//! (`docs/architecture.md` section 10, "规范格式"): fields in declaration
//! order, absent optional fields and empty lists left out.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) type Triple = (f32, f32, f32);

/// A loom scene file (`docs/architecture.md` section 10). The file is RON;
/// this schema describes the same data as JSON: a RON tuple such as
/// `(1.0, 2.0, 3.0)` is a fixed-length array; an optional field may be left
/// out, and when given is written bare in RON (`mesh: Cube`, not
/// `mesh: Some(Cube)`); an enum such as `mesh` is serde's externally tagged
/// form: a unit variant is its name (`"Cube"`), a variant with fields an
/// object with one key (`{"Capsule": {"radius": 0.35, "height": 1.8}}`).
/// Loading checks more than the structure: the rules are in the field
/// descriptions.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "scene")]
pub(crate) struct SceneFile {
    /// The file format version: 1.
    #[schemars(range(min = 1, max = 1))]
    pub version: u32,
    /// Which game runs the scene, e.g. "souls" or "rogue"; `loom run` picks
    /// the game by it. Not empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    /// Path of the active camera node: names joined by "/" from a root
    /// node. The node needs a camera component.
    pub camera: String,
    /// Ambient light, linear RGB, each component from 0 to 1.
    pub ambient: Triple,
    /// The root nodes, in drawing and checking order.
    pub nodes: Vec<NodeDesc>,
}

/// A scene node. Every number must be finite.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeDesc {
    /// Unique among its siblings.
    #[schemars(pattern("^[a-z][a-z0-9_]*$"))]
    pub name: String,
    /// A name for people to read, e.g. over a boss's health bar. Not
    /// empty; which characters it may have is the game's to say.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub display_name: Option<String>,
    /// Local position in meters, relative to the parent. Default (0, 0, 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Triple>,
    /// YXZ Euler angles in degrees. Not together with `look_at`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Triple>,
    /// A point, in the parent's space, that the node's -Z axis turns to.
    /// Not the node's own position, and not straight above or below it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_at: Option<Triple>,
    /// Every component greater than 0. Default (1, 1, 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Triple>,
    /// At most one of `mesh`, `sprite` and `tilemap`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<MeshKind>,
    /// The mesh color, linear RGB, each component from 0 to 1. Default
    /// (0.8, 0.8, 0.8).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Triple>,
    /// Makes the node a camera, looking along its -Z axis.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraDesc>,
    /// At most one of `mesh`, `sprite` and `tilemap`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sprite: Option<SpriteDesc>,
    /// At most one of `mesh`, `sprite` and `tilemap`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tilemap: Option<TilemapDesc>,
    /// The scene's one light, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<LightDesc>,
    /// The scene's one character, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<CharacterDesc>,
    /// The scene's one orbit camera, if any: on a node with a camera.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orbit_camera: Option<OrbitCameraDesc>,
    /// Numbers for the game to interpret, as `(name, value)` pairs in file
    /// order: names match ^[a-z][a-z0-9_]*$, each given once, values
    /// finite. Which names a game knows, and their rules, are under
    /// `games.<game>.props`. A list, not a map: ron silently keeps the last
    /// of two duplicate map keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub props: Vec<(String, f64)>,
    /// A static collision shape. Not on the character's node; the node may
    /// only rotate around Y (rotation x and z 0, no look_at), and its
    /// ancestors only translate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collider: Option<ColliderKind>,
    /// Child nodes, placed relative to this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeDesc>,
}

/// Static collision shapes, sized and placed by the node's transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub(crate) enum ColliderKind {
    /// Unit cube like `mesh: Cube`.
    Box,
}

/// Mesh shapes. The first three are unit sized; size them with `scale`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
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

/// A camera: `near` and `far` in meters, `0 < near < far`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CameraDesc {
    /// Perspective: vertical field of view in degrees, between 0 and 180.
    /// Exactly one of `fov_y` and `ortho` is given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov_y: Option<f32>,
    /// Orthographic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ortho: Option<OrthoDesc>,
    /// Meters to the near clipping plane.
    pub near: f32,
    /// Meters to the far clipping plane.
    pub far: f32,
}

/// An orthographic projection.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrthoDesc {
    /// World length the view covers vertically, greater than 0; the width
    /// follows the aspect ratio.
    pub height: f32,
}

/// A sprite: one tag of an Aseprite sheet, drawn on the node's XY plane.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SpriteDesc {
    /// Path of an Aseprite json-array sprite sheet, relative to the
    /// working directory. Not empty.
    pub sheet: String,
    /// The tag shown, one of the sheet's. Not empty.
    pub tag: String,
    /// Other tags of the sheet the game may switch the sprite to while it
    /// runs: each once, and not `tag`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternates: Vec<String>,
}

/// A tile map: rows of characters, each drawing a tag of an Aseprite
/// sheet, one cell a unit square on the node's XY plane.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TilemapDesc {
    /// Path of an Aseprite json-array sprite sheet, relative to the
    /// working directory. Not empty.
    pub sheet: String,
    /// Which tag each map character draws; each character once.
    pub legend: Vec<(char, String)>,
    /// One string per row, top row first; one character per cell, every
    /// character in the legend. At least one row, all of the same length,
    /// at least 1.
    pub rows: Vec<String>,
}

/// A directional light, shining along the node's -Z axis.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct LightDesc {
    /// Linear RGB, each component from 0 to 1.
    pub color: Triple,
    /// At least 0.
    pub intensity: f32,
}

/// Makes the node the character the game moves.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CharacterDesc {
    /// Meters per second, greater than 0.
    pub run_speed: f32,
    /// Degrees per second, greater than 0.
    pub turn_speed: f32,
}

/// A camera that orbits the character, as in Dark Souls.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrbitCameraDesc {
    /// Path (from a root) of the node with the `character` component.
    pub target: String,
    /// Meters above the character's feet the camera orbits around.
    pub pivot_height: f32,
    /// Meters from the pivot, greater than 0.
    pub distance: f32,
    /// Starting angle around +Y, in degrees.
    pub yaw: f32,
    /// Starting angle in degrees, between `pitch_min` and `pitch_max`;
    /// positive looks up.
    pub pitch: f32,
    /// Degrees; `-90 < pitch_min < pitch_max < 90`.
    pub pitch_min: f32,
    /// Degrees; see `pitch_min`.
    pub pitch_max: f32,
    /// Radius of the camera's collision sphere, in meters:
    /// `0 < collision_radius < pivot_height`.
    pub collision_radius: f32,
    /// Meters per second the camera moves back out once unblocked, greater
    /// than 0.
    pub return_speed: f32,
}

/// The fields `edit_node`'s `set` may give: the node's own, every one
/// optional, plus `children` so it can be refused with its own message.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodePatch {
    pub name: Option<String>,
    pub display_name: Option<String>,
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
