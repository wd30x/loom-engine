//! Scenes: parsing and validating scene files, and the runtime node tree.
//! See `docs/architecture.md` section 10.
//!
//! A scene is a tree of named nodes. Each node has a local transform and may
//! carry optional components (`mesh`, `camera`, `light`, `character`,
//! `orbit_camera`, `collider`, `sprite`, `tilemap`). Nodes are stored in
//! one array in pre-order (a parent before its children, children in file
//! order) and addressed by [`NodeId`] (an index) or by path (`arena/pillar`).
//! The tree is simulation state: its structure is fixed after parsing, only
//! transforms change.

#[cfg(test)]
mod animation_tests;
mod clip;
mod doc;
#[cfg(test)]
mod doc_tests;
mod file;

use std::collections::BTreeSet;

use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use schemars::JsonSchema;
use serde::Serialize;

pub use clip::{BoneDesc, CTICKS, Clip, ClipError, ClipFile, Keyframe, clip_schema};
pub use doc::{EditError, SceneDoc};
pub use file::MeshKind;
use file::{
    AnimationDesc, CameraDesc, ColliderKind, NodeDesc, OrbitCameraDesc, SceneFile, SpriteDesc,
    TilemapDesc, Triple,
};

/// The JSON Schema (draft 2020-12) of a scene file, as `loom schema`
/// writes it under `scene` (`docs/architecture.md` section 17): what a file
/// may contain, the RON read as JSON.
pub fn scene_schema() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<SceneFile>()
}

/// The only scene file version so far.
const VERSION: u32 = 1;
/// Color of a mesh node without `color`.
const DEFAULT_COLOR: Vec3 = Vec3::splat(0.8);
/// A model's default `color`: its own colors, untinted.
const DEFAULT_MODEL_COLOR: Vec3 = Vec3::ONE;

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    /// RON syntax error, unknown field, ... Includes the line and column.
    #[error(transparent)]
    Syntax(#[from] ron::error::SpannedError),
    #[error("unsupported scene version {0}, expected 1")]
    UnsupportedVersion(u32),
    #[error("ambient: {0}")]
    Ambient(Problem),
    /// The top-level `game`.
    #[error("{0}")]
    Game(Problem),
    #[error("camera {0:?} does not name a node")]
    CameraNotFound(String),
    #[error("camera {0:?} names a node without a camera component")]
    NotACamera(String),
    #[error("node {path:?}: {problem}")]
    Node { path: String, problem: Problem },
}

/// What is wrong with a node (or with `ambient`).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Problem {
    #[error("invalid name: must match ^[a-z][a-z0-9_]*$")]
    InvalidName,
    #[error("another node with the same parent has the same name")]
    DuplicateName,
    #[error("display_name must not be empty")]
    EmptyDisplayName,
    #[error("{0} contains a non-finite number")]
    NotFinite(&'static str),
    #[error("scale components must be greater than 0")]
    ScaleNotPositive,
    #[error("{0} components must be between 0 and 1")]
    ColorOutOfRange(&'static str),
    #[error("rotation and look_at cannot both be given")]
    RotationAndLookAt,
    #[error("look_at target equals the node's position")]
    LookAtSelf,
    #[error("look_at direction is parallel to the Y axis")]
    LookAtVertical,
    #[error("camera fov_y must be between 0 and 180 degrees")]
    FovOutOfRange,
    #[error("camera must satisfy 0 < near < far")]
    NearFar,
    #[error("camera needs exactly one of fov_y (perspective) and ortho (orthographic)")]
    CameraProjection,
    #[error("camera ortho height must be greater than 0")]
    OrthoHeight,
    #[error("mesh, model, sprite and tilemap cannot share a node: give at most one of them")]
    DrawableConflict,
    #[error("{0} must not be empty")]
    EmptyField(&'static str),
    #[error("animation needs a model on the same node")]
    AnimationWithoutModel,
    #[error("bone needs a model on the parent node")]
    BoneWithoutParentModel,
    #[error("animation play {0:?} is not one of its clips")]
    AnimationPlayUnknown(String),
    #[error("animation clip {0:?} is given twice")]
    DuplicateClip(String),
    #[error("tilemap rows must not be empty")]
    TilemapNoRows,
    #[error("tilemap rows must all have the same number of characters, at least 1")]
    TilemapRowWidth,
    #[error("tilemap row {row}, column {column}: {ch:?} is not in the legend")]
    TilemapUnknownChar { row: usize, column: usize, ch: char },
    #[error("tilemap legend lists {0:?} more than once")]
    TilemapDuplicateLegend(char),
    #[error("sprite alternates list {0:?} more than once")]
    DuplicateAlternate(String),
    #[error("sprite alternate {0:?} is already the sprite's tag")]
    AlternateIsTag(String),
    #[error("prop name {0:?} is invalid: must match ^[a-z][a-z0-9_]*$")]
    InvalidPropName(String),
    #[error("prop {0:?} is given more than once")]
    DuplicateProp(String),
    #[error("prop {0:?} must be a finite number")]
    PropNotFinite(String),
    #[error("light intensity must be at least 0")]
    NegativeIntensity,
    #[error("a second light; the scene already has one at {0:?}")]
    SecondLight(String),
    #[error("capsule must satisfy radius > 0 and height >= 2 * radius")]
    CapsuleShape,
    #[error("character run_speed and turn_speed must be greater than 0")]
    CharacterSpeed,
    #[error("character capsule radius and height must be greater than 0")]
    CharacterCapsule,
    #[error("a second character; the scene already has one at {0:?}")]
    SecondCharacter(String),
    #[error("a second orbit_camera; the scene already has one at {0:?}")]
    SecondOrbitCamera(String),
    #[error("orbit_camera needs a camera component on the same node")]
    OrbitCameraWithoutCamera,
    #[error("orbit_camera target {0:?} does not name a node")]
    OrbitTargetNotFound(String),
    #[error("orbit_camera target {0:?} names a node without a character component")]
    OrbitTargetNotCharacter(String),
    #[error("orbit_camera distance must be greater than 0")]
    OrbitDistance,
    #[error("orbit_camera must satisfy -90 < pitch_min < pitch_max < 90")]
    OrbitPitchRange,
    #[error("orbit_camera pitch must be between pitch_min and pitch_max")]
    OrbitPitchOutOfRange,
    #[error("orbit_camera collision_radius must satisfy 0 < collision_radius < pivot_height")]
    OrbitCollisionRadius,
    #[error("orbit_camera return_speed must be greater than 0")]
    OrbitReturnSpeed,
    #[error(
        "collider may only rotate around Y: rotation x and z must be 0, and look_at is not allowed"
    )]
    ColliderRotation,
    #[error("collider needs ancestors that only translate, but {0:?} rotates or scales")]
    ColliderAncestor(String),
    #[error("character and collider cannot be on the same node")]
    CharacterAndCollider,
}

/// Index of a node in its [`Scene`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(usize);

impl NodeId {
    pub fn index(self) -> usize {
        self.0
    }
}

/// Local transform, relative to the parent node. Serializes as
/// `{"position": [x, y, z], "rotation": [x, y, z, w], "scale": [x, y, z]}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct Transform {
    /// `[x, y, z]`, in meters.
    #[schemars(with = "[f32; 3]")]
    pub position: Vec3,
    /// A unit quaternion `[x, y, z, w]`.
    #[schemars(with = "[f32; 4]")]
    pub rotation: Quat,
    /// `[x, y, z]`.
    #[schemars(with = "[f32; 3]")]
    pub scale: Vec3,
}

impl Transform {
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mesh {
    pub kind: MeshKind,
    /// Linear RGB.
    pub color: Vec3,
}

/// A glTF model (`docs/architecture.md` section 19). The file is read by
/// `loom_engine`, not here.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Path of the `.glb`, as written; never empty.
    pub path: String,
    /// Linear RGB, multiplied with the model's vertex colors.
    pub color: Vec3,
    /// The model's clips and the one playing; `None` draws the bind pose.
    pub animation: Option<Animation>,
}

/// A model's animation clips and what plays (`docs/architecture.md`
/// section 19, "播放和过渡"). The clip files are read by `loom_engine`, not
/// here. Part of the simulation state: games switch clips with
/// [`Scene::play_clip`].
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// `(name, path)` pairs as written, in file order: at least one, names
    /// and paths not empty, names unique.
    pub clips: Vec<(String, String)>,
    /// The clip playing: one of `clips`' names.
    pub playing: String,
    /// The tick this stretch of it is counted from.
    pub start: u64,
    /// The clip time at `start`, in ctick (hundredths of a tick).
    pub offset: u64,
    /// Playing speed in percent: 100 is as written, 0 holds still.
    pub rate: u32,
    /// What played before the latest switch; `None` until the first.
    pub from: Option<ClipTime>,
    /// The tick the blend from `from` starts at.
    pub blend_start: u64,
    /// Ticks the blend from `from` takes; 0 switches at once.
    pub blend: u64,
}

/// A clip and how its time runs: `offset` ctick at tick `start`, then
/// `rate` ctick a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipTime {
    pub clip: String,
    pub start: u64,
    pub offset: u64,
    pub rate: u32,
}

impl ClipTime {
    /// The clip time at tick `tau`, in ctick: `offset + (tau - start) *
    /// rate`, the ticks before `start` counting as 0; not wrapped.
    pub fn at(&self, tau: u64) -> u64 {
        clip_time(self.start, self.offset, self.rate, tau)
    }
}

fn clip_time(start: u64, offset: u64, rate: u32, tau: u64) -> u64 {
    offset.saturating_add(tau.saturating_sub(start).saturating_mul(u64::from(rate)))
}

/// What [`Scene::play_clip`] switches to: `clip` from tick `start` at
/// clip time `offset` (ctick), `rate` percent fast, blended from what
/// played over `blend` ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Play {
    pub clip: String,
    pub start: u64,
    pub blend: u64,
    pub offset: u64,
    pub rate: u32,
}

impl Play {
    /// `clip` from its beginning at tick `start`, as fast as written,
    /// without a blend.
    pub fn new(clip: &str, start: u64) -> Self {
        Self {
            clip: clip.to_owned(),
            start,
            blend: 0,
            offset: 0,
            rate: 100,
        }
    }

    pub fn blend(self, blend: u64) -> Self {
        Self { blend, ..self }
    }

    pub fn offset(self, offset: u64) -> Self {
        Self { offset, ..self }
    }

    pub fn rate(self, rate: u32) -> Self {
        Self { rate, ..self }
    }
}

impl Animation {
    /// The path of the clip named `name`, if the node declares it.
    pub fn clip_path(&self, name: &str) -> Option<&str> {
        self.clips
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, path)| path.as_str())
    }

    /// Plays on `rate` percent fast from tick `tau`, from the clip time
    /// it has then ([`Scene::set_clip_rate`]).
    pub fn set_rate(&mut self, tau: u64, rate: u32) {
        self.offset = self.time_at(tau);
        self.start = tau;
        self.rate = rate;
    }

    /// The playing clip's time at tick `tau`, in ctick, not wrapped
    /// ([`ClipTime::at`]).
    pub fn time_at(&self, tau: u64) -> u64 {
        clip_time(self.start, self.offset, self.rate, tau)
    }

    /// The offset of `bone` from its bind pose at tick `tau`: the playing
    /// clip's, blended from the `from` clip's while the blend lasts
    /// (`docs/architecture.md` section 19, "画某个 tick 的姿势" and
    /// "播放速度和片段起点"). `clip` gives the clip of each of the node's
    /// clip names. A bone a clip does not list has the identity rotation
    /// and no move in it.
    pub fn offsets<'c>(
        &self,
        clip: impl Fn(&str) -> &'c Clip,
        bone: &str,
        tau: u64,
    ) -> (Quat, Vec3) {
        let sample = |name: &str, c: u64| {
            clip(name)
                .sample(bone, c)
                .unwrap_or((Quat::IDENTITY, Vec3::ZERO))
        };
        let new = sample(&self.playing, self.time_at(tau));
        let Some(from) = &self.from else {
            return new;
        };
        // Ticks into the blend; `None` before it starts.
        let into = tau.checked_sub(self.blend_start);
        if self.blend == 0 || into.is_some_and(|t| t >= self.blend) {
            return new;
        }
        let old = sample(&from.clip, from.at(tau));
        match into {
            None | Some(0) => old,
            Some(t) => {
                let w = t as f32 / self.blend as f32;
                (old.0.slerp(new.0, w), old.1.lerp(new.1, w))
            }
        }
    }
}

/// Why [`Scene::play_clip`] refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AnimationError {
    #[error("node {0:?} has no animation")]
    NoAnimation(String),
    #[error("node {path:?}: the scene declares no animation clip {clip:?}")]
    Undeclared { path: String, clip: String },
}

/// Looks along the node's -Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub projection: Projection,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Projection {
    /// Vertical field of view in degrees.
    Perspective { fov_y_degrees: f32 },
    /// `height` is the world length the view covers vertically; the width
    /// follows the aspect ratio (`docs/architecture.md` section 14).
    Orthographic { height: f32 },
}

/// A 1x1 quad on the node's XY plane, centered at its origin, showing the
/// first frame of its current tag (`docs/architecture.md` section 14).
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    /// Path of an Aseprite json-array sprite sheet, relative to the working
    /// directory. Whether it exists and has every tag is checked by the
    /// engine.
    pub sheet: String,
    /// Every tag the scene declares: `tag`, then `alternates` in file
    /// order. Non-empty and unique.
    tags: Vec<String>,
    /// Index into `tags` of the tag shown now; only
    /// [`Scene::set_sprite_tag`] changes it.
    current: usize,
}

impl Sprite {
    /// The tag shown now: the scene's `tag` until the game switches it.
    pub fn tag(&self) -> &str {
        &self.tags[self.current]
    }

    /// Every tag the sprite may show: the scene's `tag`, then its
    /// `alternates`.
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
}

/// Why [`Scene::set_sprite_tag`] refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpriteTagError {
    #[error("node {0:?} has no sprite")]
    NoSprite(String),
    #[error("node {path:?}: the scene declares no sprite tag {tag:?} (neither tag nor alternates)")]
    Undeclared { path: String, tag: String },
}

/// Why [`Scene::set_mesh_color`] refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MeshColorError {
    #[error("the node {0:?} has no mesh")]
    NoMesh(String),
    #[error("the node {path:?}: mesh color components must be between 0 and 1, got {color}")]
    OutOfRange { path: String, color: Vec3 },
}

/// A grid of 1x1 cells on the node's XY plane: row `r`, column `c` is
/// centered at `(c, -r, 0)`, row 0 on top (`docs/architecture.md` section 14).
#[derive(Debug, Clone, PartialEq)]
pub struct Tilemap {
    /// Path of an Aseprite json-array sprite sheet, as for [`Sprite`].
    pub sheet: String,
    /// Which tag each character draws; characters are unique. A list, not a
    /// map, so it keeps the file's order.
    pub legend: Vec<(char, String)>,
    /// Non-empty, all of the same (non-zero) character count, every
    /// character in `legend`.
    pub rows: Vec<String>,
}

impl Tilemap {
    /// The tag `ch` draws, if it is in the legend.
    pub fn tag(&self, ch: char) -> Option<&str> {
        self.legend
            .iter()
            .find(|(c, _)| *c == ch)
            .map(|(_, tag)| tag.as_str())
    }
}

/// Directional light, shining along the node's -Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    /// Linear RGB.
    pub color: Vec3,
    pub intensity: f32,
}

/// A player character (`docs/architecture.md` section 11).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Character {
    /// Meters per second.
    pub run_speed: f32,
    /// Degrees per second.
    pub turn_speed_degrees: f32,
    /// The capsule's `(radius, height)` in meters, when the scene gives
    /// one.
    pub capsule: Option<(f32, f32)>,
}

/// A camera orbiting a character (`docs/architecture.md` section 11). Always
/// on a node that also has a [`Camera`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    /// The node with the [`Character`] it orbits.
    pub target: NodeId,
    pub pivot_height: f32,
    pub distance: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub pitch_min_degrees: f32,
    pub pitch_max_degrees: f32,
    /// Radius of the camera's collision sphere, in meters.
    pub collision_radius: f32,
    /// Meters per second the camera moves back out once unblocked.
    pub return_speed: f32,
}

/// A static collision shape (`docs/architecture.md` section 11, collision).
/// Its size, rotation (around Y only) and position come from the node's
/// transform; its ancestors only translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collider {
    /// A unit cube like `MeshKind::Cube`, scaled by the node.
    Box,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    /// A name for people to read; never empty.
    pub display_name: Option<String>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub transform: Transform,
    /// A bone of the parent's model the node hangs on, on screen only
    /// (`docs/architecture.md` section 19, "挂在骨骼上"); the parent has a
    /// model. Never empty.
    pub bone: Option<String>,
    pub mesh: Option<Mesh>,
    pub model: Option<Model>,
    pub camera: Option<Camera>,
    pub light: Option<Light>,
    pub character: Option<Character>,
    pub orbit_camera: Option<OrbitCamera>,
    pub collider: Option<Collider>,
    pub sprite: Option<Sprite>,
    pub tilemap: Option<Tilemap>,
    /// Game-defined numbers, in file order; names are unique.
    props: Vec<(String, f64)>,
    /// Runtime only, not in the scene file: a hidden node and its subtree
    /// are not drawn ([`Scene::set_visible`]). `true` at load time.
    pub visible: bool,
}

impl Node {
    /// The prop called `name`, if the node has it.
    pub fn prop(&self, name: &str) -> Option<f64> {
        self.props.iter().find(|(n, _)| n == name).map(|&(_, v)| v)
    }

    /// Every prop, in file order.
    pub fn props(&self) -> impl Iterator<Item = (&str, f64)> {
        self.props.iter().map(|(n, v)| (n.as_str(), *v))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// Pre-order: every parent comes before its children.
    nodes: Vec<Node>,
    roots: Vec<NodeId>,
    camera: NodeId,
    light: Option<NodeId>,
    character: Option<NodeId>,
    orbit_camera: Option<NodeId>,
    /// In pre-order.
    colliders: Vec<NodeId>,
    /// Nodes with a sprite or a tilemap, in pre-order.
    sprite_nodes: Vec<NodeId>,
    /// Nodes with a model, in pre-order.
    model_nodes: Vec<NodeId>,
    ambient: Vec3,
    /// The top-level `game`: which game runs the scene. Never empty.
    game: Option<String>,
}

impl Scene {
    /// Parses a scene file and applies every validation rule of
    /// `docs/architecture.md` section 10. Semantic errors name the node by
    /// path; the caller adds the file path.
    pub fn parse(src: &str) -> Result<Self, SceneError> {
        SceneDoc::parse(src)?.build()
    }

    /// Validates a parsed scene file and builds the scene from it: the
    /// part of [`Scene::parse`] after the RON syntax.
    pub(crate) fn from_file(file: &SceneFile) -> Result<Self, SceneError> {
        if file.version != VERSION {
            return Err(SceneError::UnsupportedVersion(file.version));
        }
        let ambient = color(file.ambient, "ambient").map_err(SceneError::Ambient)?;
        if let Some(game) = &file.game {
            not_empty(game, "game").map_err(SceneError::Game)?;
        }

        let mut builder = Builder {
            nodes: Vec::new(),
            light: None,
            character: None,
            orbit_camera: None,
            colliders: Vec::new(),
            sprite_nodes: Vec::new(),
            model_nodes: Vec::new(),
        };
        let roots = builder.add_siblings(&file.nodes, None, "", None)?;
        let mut scene = Self {
            nodes: builder.nodes,
            roots,
            camera: NodeId(0),
            light: builder.light,
            character: builder.character,
            orbit_camera: None,
            colliders: builder.colliders,
            sprite_nodes: builder.sprite_nodes,
            model_nodes: builder.model_nodes,
            ambient,
            game: file.game.clone(),
        };
        // The target is a path from a root, resolved once the whole tree
        // exists: it may name a node that comes later in the file.
        if let Some((id, desc)) = builder.orbit_camera {
            let fail = |problem| SceneError::Node {
                path: scene.path(id),
                problem,
            };
            let target = match scene.find(&desc.target) {
                None => return Err(fail(Problem::OrbitTargetNotFound(desc.target.clone()))),
                Some(t) if scene.node(t).character.is_none() => {
                    return Err(fail(Problem::OrbitTargetNotCharacter(desc.target.clone())));
                }
                Some(t) => t,
            };
            scene.nodes[id.0].orbit_camera = Some(OrbitCamera {
                target,
                pivot_height: desc.pivot_height,
                distance: desc.distance,
                yaw_degrees: desc.yaw,
                pitch_degrees: desc.pitch,
                pitch_min_degrees: desc.pitch_min,
                pitch_max_degrees: desc.pitch_max,
                collision_radius: desc.collision_radius,
                return_speed: desc.return_speed,
            });
            scene.orbit_camera = Some(id);
        }
        scene.camera = match scene.find(&file.camera) {
            None => return Err(SceneError::CameraNotFound(file.camera.clone())),
            Some(id) if scene.node(id).camera.is_none() => {
                return Err(SceneError::NotACamera(file.camera.clone()));
            }
            Some(id) => id,
        };
        Ok(scene)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// All nodes in pre-order, which is also the draw order.
    pub fn ids(&self) -> impl Iterator<Item = NodeId> + use<> {
        (0..self.nodes.len()).map(NodeId)
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }

    /// The node at `path` (names joined by `/`, starting at a root).
    pub fn find(&self, path: &str) -> Option<NodeId> {
        let mut candidates = self.roots.as_slice();
        let mut found = None;
        for name in path.split('/') {
            let id = *candidates
                .iter()
                .find(|&&id| self.nodes[id.0].name == name)?;
            candidates = &self.nodes[id.0].children;
            found = Some(id);
        }
        found
    }

    /// The path of `id`, e.g. `arena/pillar`.
    pub fn path(&self, id: NodeId) -> String {
        let node = &self.nodes[id.0];
        match node.parent {
            Some(parent) => format!("{}/{}", self.path(parent), node.name),
            None => node.name.clone(),
        }
    }

    pub fn transform(&self, id: NodeId) -> &Transform {
        &self.nodes[id.0].transform
    }

    pub fn transform_mut(&mut self, id: NodeId) -> &mut Transform {
        &mut self.nodes[id.0].transform
    }

    /// Switches the sprite of `id` to `tag`, which must be one the scene
    /// declares for it (its `tag` or one of its `alternates`), so the
    /// renderer only ever meets tags checked at load time.
    pub fn set_sprite_tag(&mut self, id: NodeId, tag: &str) -> Result<(), SpriteTagError> {
        let path = || self.path(id);
        let Some(sprite) = &self.nodes[id.0].sprite else {
            return Err(SpriteTagError::NoSprite(path()));
        };
        let Some(index) = sprite.tags.iter().position(|t| t == tag) else {
            return Err(SpriteTagError::Undeclared {
                path: path(),
                tag: tag.to_owned(),
            });
        };
        if let Some(sprite) = &mut self.nodes[id.0].sprite {
            sprite.current = index;
        }
        Ok(())
    }

    /// Switches the animation of `id` to its clip `clip`, started at tick
    /// `start`, blended from what played over `blend` ticks
    /// (`docs/architecture.md` section 19, "播放和过渡"). The clip must be
    /// one the scene declares for the node, so the engine only ever meets
    /// clips checked at load time; on an error nothing changes. Switching
    /// to the clip already playing restarts it, blended from where it was.
    pub fn play_clip(&mut self, id: NodeId, play: Play) -> Result<(), AnimationError> {
        let path = self.path(id);
        let animation = self.animation_mut(id)?;
        if animation.clip_path(&play.clip).is_none() {
            return Err(AnimationError::Undeclared {
                path,
                clip: play.clip,
            });
        }
        let playing = std::mem::replace(&mut animation.playing, play.clip);
        animation.from = Some(ClipTime {
            clip: playing,
            start: animation.start,
            offset: animation.offset,
            rate: animation.rate,
        });
        animation.start = play.start;
        animation.offset = play.offset;
        animation.rate = play.rate;
        animation.blend_start = play.start;
        animation.blend = play.blend;
        Ok(())
    }

    /// Changes how fast the animation of `id` plays from tick `tau` on,
    /// `rate` percent, without a jump: the clip time at `tau` stays (not
    /// wrapped), and a blend in progress goes on as it was
    /// (`docs/architecture.md` section 19, "播放速度和片段起点"). On an
    /// error nothing changes.
    pub fn set_clip_rate(&mut self, id: NodeId, tau: u64, rate: u32) -> Result<(), AnimationError> {
        self.animation_mut(id)?.set_rate(tau, rate);
        Ok(())
    }

    /// The animation of `id`, to change.
    fn animation_mut(&mut self, id: NodeId) -> Result<&mut Animation, AnimationError> {
        let path = self.path(id);
        self.nodes[id.0]
            .model
            .as_mut()
            .and_then(|m| m.animation.as_mut())
            .ok_or(AnimationError::NoAnimation(path))
    }

    /// Sets the mesh color of `id` (a model's tint for a model node),
    /// drawn from the next frame on. The color follows the scene file's
    /// rule for `color` (finite, every component in `[0, 1]`); on an error
    /// nothing changes.
    pub fn set_mesh_color(&mut self, id: NodeId, color: Vec3) -> Result<(), MeshColorError> {
        let path = || self.path(id);
        let node = &self.nodes[id.0];
        if node.mesh.is_none() && node.model.is_none() {
            return Err(MeshColorError::NoMesh(path()));
        }
        if self::color((color.x, color.y, color.z), "mesh color").is_err() {
            return Err(MeshColorError::OutOfRange {
                path: path(),
                color,
            });
        }
        let node = &mut self.nodes[id.0];
        if let Some(mesh) = &mut node.mesh {
            mesh.color = color;
        }
        if let Some(model) = &mut node.model {
            model.color = color;
        }
        Ok(())
    }

    /// Hides `id` and its whole subtree from drawing, or shows it again.
    /// Only drawing changes: transforms, components and colliders stay.
    pub fn set_visible(&mut self, id: NodeId, visible: bool) {
        self.nodes[id.0].visible = visible;
    }

    /// Whether `id` is drawn: it and every ancestor are visible.
    pub fn is_drawn(&self, id: NodeId) -> bool {
        let mut at = Some(id);
        while let Some(n) = at {
            if !self.nodes[n.0].visible {
                return false;
            }
            at = self.nodes[n.0].parent;
        }
        true
    }

    /// `parent world matrix * local matrix`, computed along the parent chain.
    /// Scene-tree only: it does not include bone poses (a node with a
    /// `bone` is placed as if it hung on its parent directly); rendering
    /// adds those (`docs/architecture.md` section 19, "挂在骨骼上").
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        let node = &self.nodes[id.0];
        let local = node.transform.matrix();
        match node.parent {
            Some(parent) => self.world_matrix(parent) * local,
            None => local,
        }
    }

    /// The active camera node; it always has a camera component.
    pub fn camera(&self) -> NodeId {
        self.camera
    }

    /// The scene's only light, if any.
    pub fn light(&self) -> Option<NodeId> {
        self.light
    }

    /// The scene's only node with a [`Character`], if any.
    pub fn character(&self) -> Option<NodeId> {
        self.character
    }

    /// The scene's only node with an [`OrbitCamera`], if any. It also has a
    /// [`Camera`], but need not be the active camera.
    pub fn orbit_camera(&self) -> Option<NodeId> {
        self.orbit_camera
    }

    /// Every node with a [`Collider`], in pre-order.
    pub fn colliders(&self) -> &[NodeId] {
        &self.colliders
    }

    /// Every node with a [`Sprite`] or a [`Tilemap`], in pre-order.
    pub fn sprite_nodes(&self) -> &[NodeId] {
        &self.sprite_nodes
    }

    /// Every node with a [`Model`], in pre-order.
    pub fn model_nodes(&self) -> &[NodeId] {
        &self.model_nodes
    }

    /// Ambient light, linear RGB.
    pub fn ambient(&self) -> Vec3 {
        self.ambient
    }

    /// The top-level `game`, as written: which game runs the scene
    /// (`docs/architecture.md` sections 1 and 10). Never empty.
    pub fn game(&self) -> Option<&str> {
        self.game.as_deref()
    }
}

/// Rotation that points a node's -Z from `position` towards `target` with +Y
/// up, both in the parent's space. This exact sequence of operations is
/// part of the format: an equivalent formula could differ in the last bit.
/// The caller must ensure `target != position` and that the direction is not
/// parallel to Y.
pub fn look_at_rotation(position: Vec3, target: Vec3) -> Quat {
    let back = (position - target).normalize();
    let right = Vec3::Y.cross(back).normalize();
    let up = back.cross(right);
    Quat::from_mat3(&Mat3::from_cols(right, up, back))
}

struct Builder<'a> {
    nodes: Vec<Node>,
    light: Option<NodeId>,
    character: Option<NodeId>,
    /// Validated except for its target, which is resolved after the build.
    orbit_camera: Option<(NodeId, &'a OrbitCameraDesc)>,
    colliders: Vec<NodeId>,
    sprite_nodes: Vec<NodeId>,
    model_nodes: Vec<NodeId>,
}

impl<'a> Builder<'a> {
    /// Adds `descs` (siblings) and their subtrees in pre-order.
    /// `moving_ancestor` is the path of the first ancestor that does more
    /// than translate, if any (colliders may not have one).
    fn add_siblings(
        &mut self,
        descs: &'a [NodeDesc],
        parent: Option<NodeId>,
        parent_path: &str,
        moving_ancestor: Option<&str>,
    ) -> Result<Vec<NodeId>, SceneError> {
        let mut names = BTreeSet::new();
        let mut ids = Vec::with_capacity(descs.len());
        for desc in descs {
            let path = if parent_path.is_empty() {
                desc.name.clone()
            } else {
                format!("{parent_path}/{}", desc.name)
            };
            let fail = |problem| SceneError::Node {
                path: path.clone(),
                problem,
            };
            if !is_valid_name(&desc.name) {
                return Err(fail(Problem::InvalidName));
            }
            if !names.insert(desc.name.as_str()) {
                return Err(fail(Problem::DuplicateName));
            }
            let id = NodeId(self.nodes.len());
            let node = node_from_desc(desc, parent).map_err(fail)?;
            if node.bone.is_some() && parent.is_none_or(|p| self.nodes[p.0].model.is_none()) {
                return Err(fail(Problem::BoneWithoutParentModel));
            }
            if node.collider.is_some() {
                if let Some(ancestor) = moving_ancestor {
                    return Err(fail(Problem::ColliderAncestor(ancestor.to_owned())));
                }
                self.colliders.push(id);
            }
            if node.sprite.is_some() || node.tilemap.is_some() {
                self.sprite_nodes.push(id);
            }
            if node.model.is_some() {
                self.model_nodes.push(id);
            }
            if node.light.is_some() {
                if let Some(first) = self.light {
                    return Err(fail(Problem::SecondLight(self.path_of(first))));
                }
                self.light = Some(id);
            }
            if node.character.is_some() {
                if let Some(first) = self.character {
                    return Err(fail(Problem::SecondCharacter(self.path_of(first))));
                }
                self.character = Some(id);
            }
            if let Some(orbit) = &desc.orbit_camera {
                if let Some((first, _)) = self.orbit_camera {
                    return Err(fail(Problem::SecondOrbitCamera(self.path_of(first))));
                }
                self.orbit_camera = Some((id, orbit));
            }
            self.nodes.push(node);
            let moving = match moving_ancestor {
                Some(ancestor) => Some(ancestor),
                None if !only_translates(desc) => Some(path.as_str()),
                None => None,
            };
            let children = self.add_siblings(&desc.children, Some(id), &path, moving)?;
            self.nodes[id.0].children = children;
            ids.push(id);
        }
        Ok(ids)
    }

    fn path_of(&self, id: NodeId) -> String {
        let node = &self.nodes[id.0];
        match node.parent {
            Some(parent) => format!("{}/{}", self.path_of(parent), node.name),
            None => node.name.clone(),
        }
    }
}

/// Validates one node's own fields (not its name or children). The
/// `orbit_camera` is validated but left out: its target is resolved later.
fn node_from_desc(desc: &NodeDesc, parent: Option<NodeId>) -> Result<Node, Problem> {
    if desc.display_name.as_deref() == Some("") {
        return Err(Problem::EmptyDisplayName);
    }
    let position = finite(desc.position.unwrap_or((0.0, 0.0, 0.0)), "position")?;
    let scale = finite(desc.scale.unwrap_or((1.0, 1.0, 1.0)), "scale")?;
    if scale.cmple(Vec3::ZERO).any() {
        return Err(Problem::ScaleNotPositive);
    }
    let rotation = match (desc.rotation, desc.look_at) {
        (Some(_), Some(_)) => return Err(Problem::RotationAndLookAt),
        (Some(degrees), None) => euler_degrees(finite(degrees, "rotation")?),
        (None, Some(target)) => {
            let target = finite(target, "look_at")?;
            if target == position {
                return Err(Problem::LookAtSelf);
            }
            let back = (position - target).normalize();
            if Vec3::Y.cross(back).length_squared() < 1e-12 {
                return Err(Problem::LookAtVertical);
            }
            look_at_rotation(position, target)
        }
        (None, None) => Quat::IDENTITY,
    };
    let mesh_color = match desc.color {
        Some(c) => color(c, "color")?,
        None if desc.model.is_some() => DEFAULT_MODEL_COLOR,
        None => DEFAULT_COLOR,
    };
    let camera = desc.camera.as_ref().map(camera).transpose()?;
    let light = match &desc.light {
        Some(l) => {
            if !l.intensity.is_finite() {
                return Err(Problem::NotFinite("light intensity"));
            }
            if l.intensity < 0.0 {
                return Err(Problem::NegativeIntensity);
            }
            Some(Light {
                color: color(l.color, "light color")?,
                intensity: l.intensity,
            })
        }
        None => None,
    };
    if let Some(MeshKind::Capsule { radius, height }) = desc.mesh {
        if !(radius.is_finite() && height.is_finite()) {
            return Err(Problem::NotFinite("mesh"));
        }
        if !(radius > 0.0 && height >= 2.0 * radius) {
            return Err(Problem::CapsuleShape);
        }
    }
    let character = match &desc.character {
        Some(c) => {
            if !(c.run_speed.is_finite() && c.turn_speed.is_finite()) {
                return Err(Problem::NotFinite("character"));
            }
            if !(c.run_speed > 0.0 && c.turn_speed > 0.0) {
                return Err(Problem::CharacterSpeed);
            }
            if let Some(capsule) = c.capsule {
                if !(capsule.radius.is_finite() && capsule.height.is_finite()) {
                    return Err(Problem::NotFinite("character"));
                }
                if !(capsule.radius > 0.0 && capsule.height > 0.0) {
                    return Err(Problem::CharacterCapsule);
                }
            }
            Some(Character {
                run_speed: c.run_speed,
                turn_speed_degrees: c.turn_speed,
                capsule: c.capsule.map(|c| (c.radius, c.height)),
            })
        }
        None => None,
    };
    if let Some(o) = &desc.orbit_camera {
        validate_orbit_camera(o, camera.is_some())?;
    }
    let drawables = [
        desc.mesh.is_some(),
        desc.model.is_some(),
        desc.sprite.is_some(),
        desc.tilemap.is_some(),
    ];
    if drawables.iter().filter(|&&d| d).count() > 1 {
        return Err(Problem::DrawableConflict);
    }
    if desc.model.as_deref() == Some("") {
        return Err(Problem::EmptyField("model"));
    }
    if desc.bone.as_deref() == Some("") {
        return Err(Problem::EmptyField("bone"));
    }
    let animation = desc.animation.as_ref().map(animation).transpose()?;
    if animation.is_some() && desc.model.is_none() {
        return Err(Problem::AnimationWithoutModel);
    }
    let sprite = desc.sprite.as_ref().map(sprite).transpose()?;
    let tilemap = desc.tilemap.as_ref().map(tilemap).transpose()?;
    let props = props(&desc.props)?;
    let collider = match desc.collider {
        Some(ColliderKind::Box) => {
            let rotates_off_y = desc.rotation.is_some_and(|(x, _, z)| x != 0.0 || z != 0.0);
            if rotates_off_y || desc.look_at.is_some() {
                return Err(Problem::ColliderRotation);
            }
            if character.is_some() {
                return Err(Problem::CharacterAndCollider);
            }
            Some(Collider::Box)
        }
        None => None,
    };
    Ok(Node {
        name: desc.name.clone(),
        display_name: desc.display_name.clone(),
        parent,
        children: Vec::new(),
        transform: Transform {
            position,
            rotation,
            scale,
        },
        bone: desc.bone.clone(),
        mesh: desc.mesh.map(|kind| Mesh {
            kind,
            color: mesh_color,
        }),
        model: desc.model.as_ref().map(|path| Model {
            path: path.clone(),
            color: mesh_color,
            animation,
        }),
        camera,
        light,
        character,
        orbit_camera: None,
        collider,
        sprite,
        tilemap,
        props,
        visible: true,
    })
}

fn camera(c: &CameraDesc) -> Result<Camera, Problem> {
    let height = c.ortho.as_ref().map(|o| o.height);
    let numbers = [c.fov_y, height, Some(c.near), Some(c.far)];
    if !numbers.iter().flatten().all(|v| v.is_finite()) {
        return Err(Problem::NotFinite("camera"));
    }
    let projection = match (c.fov_y, height) {
        (Some(fov_y), None) => {
            if !(fov_y > 0.0 && fov_y < 180.0) {
                return Err(Problem::FovOutOfRange);
            }
            Projection::Perspective {
                fov_y_degrees: fov_y,
            }
        }
        (None, Some(height)) => {
            if height <= 0.0 {
                return Err(Problem::OrthoHeight);
            }
            Projection::Orthographic { height }
        }
        _ => return Err(Problem::CameraProjection),
    };
    if !(0.0 < c.near && c.near < c.far) {
        return Err(Problem::NearFar);
    }
    Ok(Camera {
        projection,
        near: c.near,
        far: c.far,
    })
}

/// A rotation from YXZ Euler angles in degrees `(x, y, z)`: a node's
/// `rotation`, and a clip's rotation keyframes.
pub fn euler_degrees(d: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        d.y.to_radians(),
        d.x.to_radians(),
        d.z.to_radians(),
    )
}

fn animation(a: &AnimationDesc) -> Result<Animation, Problem> {
    if a.clips.is_empty() {
        return Err(Problem::EmptyField("animation clips"));
    }
    for (i, (name, path)) in a.clips.iter().enumerate() {
        not_empty(name, "animation clip name")?;
        not_empty(path, "animation clip path")?;
        if a.clips[..i].iter().any(|(n, _)| n == name) {
            return Err(Problem::DuplicateClip(name.clone()));
        }
    }
    if !a.clips.iter().any(|(n, _)| *n == a.play) {
        return Err(Problem::AnimationPlayUnknown(a.play.clone()));
    }
    Ok(Animation {
        clips: a.clips.clone(),
        playing: a.play.clone(),
        start: 0,
        offset: 0,
        rate: 100,
        from: None,
        blend_start: 0,
        blend: 0,
    })
}

fn sprite(s: &SpriteDesc) -> Result<Sprite, Problem> {
    not_empty(&s.sheet, "sprite sheet")?;
    not_empty(&s.tag, "sprite tag")?;
    let mut tags = vec![s.tag.clone()];
    for alternate in &s.alternates {
        not_empty(alternate, "sprite alternate")?;
        if *alternate == s.tag {
            return Err(Problem::AlternateIsTag(alternate.clone()));
        }
        if tags.contains(alternate) {
            return Err(Problem::DuplicateAlternate(alternate.clone()));
        }
        tags.push(alternate.clone());
    }
    Ok(Sprite {
        sheet: s.sheet.clone(),
        tags,
        current: 0,
    })
}

fn props(props: &[(String, f64)]) -> Result<Vec<(String, f64)>, Problem> {
    let mut seen = BTreeSet::new();
    for (name, value) in props {
        if !is_valid_name(name) {
            return Err(Problem::InvalidPropName(name.clone()));
        }
        if !seen.insert(name.as_str()) {
            return Err(Problem::DuplicateProp(name.clone()));
        }
        if !value.is_finite() {
            return Err(Problem::PropNotFinite(name.clone()));
        }
    }
    Ok(props.to_vec())
}

fn tilemap(t: &TilemapDesc) -> Result<Tilemap, Problem> {
    not_empty(&t.sheet, "tilemap sheet")?;
    let mut seen = BTreeSet::new();
    for (ch, tag) in &t.legend {
        if !seen.insert(*ch) {
            return Err(Problem::TilemapDuplicateLegend(*ch));
        }
        not_empty(tag, "tilemap legend tag")?;
    }
    let Some(first) = t.rows.first() else {
        return Err(Problem::TilemapNoRows);
    };
    let width = first.chars().count();
    if width == 0 || t.rows.iter().any(|r| r.chars().count() != width) {
        return Err(Problem::TilemapRowWidth);
    }
    for (row, text) in t.rows.iter().enumerate() {
        for (column, ch) in text.chars().enumerate() {
            if !seen.contains(&ch) {
                return Err(Problem::TilemapUnknownChar { row, column, ch });
            }
        }
    }
    Ok(Tilemap {
        sheet: t.sheet.clone(),
        legend: t.legend.clone(),
        rows: t.rows.clone(),
    })
}

fn not_empty(value: &str, field: &'static str) -> Result<(), Problem> {
    if value.is_empty() {
        Err(Problem::EmptyField(field))
    } else {
        Ok(())
    }
}

/// The node's own transform is at most a translation: no rotation (or all
/// zero), no `look_at`, no scale (or all one).
fn only_translates(desc: &NodeDesc) -> bool {
    desc.rotation.is_none_or(|r| r == (0.0, 0.0, 0.0))
        && desc.look_at.is_none()
        && desc.scale.is_none_or(|s| s == (1.0, 1.0, 1.0))
}

/// Everything about an `orbit_camera` except its target.
fn validate_orbit_camera(o: &OrbitCameraDesc, has_camera: bool) -> Result<(), Problem> {
    if !has_camera {
        return Err(Problem::OrbitCameraWithoutCamera);
    }
    let numbers = [
        o.pivot_height,
        o.distance,
        o.yaw,
        o.pitch,
        o.pitch_min,
        o.pitch_max,
        o.collision_radius,
        o.return_speed,
    ];
    if !numbers.iter().all(|v| v.is_finite()) {
        return Err(Problem::NotFinite("orbit_camera"));
    }
    if o.distance <= 0.0 {
        return Err(Problem::OrbitDistance);
    }
    if !(-90.0 < o.pitch_min && o.pitch_min < o.pitch_max && o.pitch_max < 90.0) {
        return Err(Problem::OrbitPitchRange);
    }
    if !(o.pitch_min <= o.pitch && o.pitch <= o.pitch_max) {
        return Err(Problem::OrbitPitchOutOfRange);
    }
    if !(0.0 < o.collision_radius && o.collision_radius < o.pivot_height) {
        return Err(Problem::OrbitCollisionRadius);
    }
    if o.return_speed <= 0.0 {
        return Err(Problem::OrbitReturnSpeed);
    }
    Ok(())
}

fn finite((x, y, z): Triple, field: &'static str) -> Result<Vec3, Problem> {
    let v = Vec3::new(x, y, z);
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Problem::NotFinite(field))
    }
}

fn color(rgb: Triple, field: &'static str) -> Result<Vec3, Problem> {
    let c = finite(rgb, field)?;
    if c.cmpge(Vec3::ZERO).all() && c.cmple(Vec3::ONE).all() {
        Ok(c)
    } else {
        Err(Problem::ColorOutOfRange(field))
    }
}

/// `^[a-z][a-z0-9_]*$`
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests;
