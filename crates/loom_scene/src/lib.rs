//! Scenes: parsing and validating scene files, and the runtime node tree.
//! See `docs/architecture.md` section 10.
//!
//! A scene is a tree of named nodes. Each node has a local transform and may
//! carry optional components (`mesh`, `camera`, `light`, `character`,
//! `orbit_camera`, `collider`). Nodes are stored in
//! one array in pre-order (a parent before its children, children in file
//! order) and addressed by [`NodeId`] (an index) or by path (`arena/pillar`).
//! The tree is simulation state: its structure is fixed after parsing, only
//! transforms change.

mod file;

use std::collections::BTreeSet;

use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use serde::Serialize;

pub use file::MeshKind;
use file::{ColliderKind, NodeDesc, OrbitCameraDesc, SceneFile, Triple};

/// The only scene file version so far.
const VERSION: u32 = 1;
/// Color of a mesh node without `color`.
const DEFAULT_COLOR: Vec3 = Vec3::splat(0.8);

#[derive(Debug, thiserror::Error)]
pub enum SceneError {
    /// RON syntax error, unknown field, ... Includes the line and column.
    #[error(transparent)]
    Syntax(#[from] ron::error::SpannedError),
    #[error("unsupported scene version {0}, expected 1")]
    UnsupportedVersion(u32),
    #[error("ambient: {0}")]
    Ambient(Problem),
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
    #[error("light intensity must be at least 0")]
    NegativeIntensity,
    #[error("a second light; the scene already has one at {0:?}")]
    SecondLight(String),
    #[error("capsule must satisfy radius > 0 and height >= 2 * radius")]
    CapsuleShape,
    #[error("character run_speed and turn_speed must be greater than 0")]
    CharacterSpeed,
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
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

/// Looks along the node's -Z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Vertical field of view in degrees.
    pub fov_y_degrees: f32,
    pub near: f32,
    pub far: f32,
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
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub transform: Transform,
    pub mesh: Option<Mesh>,
    pub camera: Option<Camera>,
    pub light: Option<Light>,
    pub character: Option<Character>,
    pub orbit_camera: Option<OrbitCamera>,
    pub collider: Option<Collider>,
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
    ambient: Vec3,
}

impl Scene {
    /// Parses a scene file and applies every validation rule of
    /// `docs/architecture.md` section 10. Semantic errors name the node by
    /// path; the caller adds the file path.
    pub fn parse(src: &str) -> Result<Self, SceneError> {
        let options = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let file: SceneFile = options.from_str(src)?;
        if file.version != VERSION {
            return Err(SceneError::UnsupportedVersion(file.version));
        }
        let ambient = color(file.ambient, "ambient").map_err(SceneError::Ambient)?;

        let mut builder = Builder {
            nodes: Vec::new(),
            light: None,
            character: None,
            orbit_camera: None,
            colliders: Vec::new(),
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
            ambient,
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
            });
            scene.orbit_camera = Some(id);
        }
        scene.camera = match scene.find(&file.camera) {
            None => return Err(SceneError::CameraNotFound(file.camera)),
            Some(id) if scene.node(id).camera.is_none() => {
                return Err(SceneError::NotACamera(file.camera));
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

    /// `parent world matrix * local matrix`, computed along the parent chain.
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

    /// Ambient light, linear RGB.
    pub fn ambient(&self) -> Vec3 {
        self.ambient
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
            if node.collider.is_some() {
                if let Some(ancestor) = moving_ancestor {
                    return Err(fail(Problem::ColliderAncestor(ancestor.to_owned())));
                }
                self.colliders.push(id);
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
    let position = finite(desc.position.unwrap_or((0.0, 0.0, 0.0)), "position")?;
    let scale = finite(desc.scale.unwrap_or((1.0, 1.0, 1.0)), "scale")?;
    if scale.cmple(Vec3::ZERO).any() {
        return Err(Problem::ScaleNotPositive);
    }
    let rotation = match (desc.rotation, desc.look_at) {
        (Some(_), Some(_)) => return Err(Problem::RotationAndLookAt),
        (Some(degrees), None) => {
            let d = finite(degrees, "rotation")?;
            Quat::from_euler(
                EulerRot::YXZ,
                d.y.to_radians(),
                d.x.to_radians(),
                d.z.to_radians(),
            )
        }
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
        None => DEFAULT_COLOR,
    };
    let camera = match &desc.camera {
        Some(c) => {
            if ![c.fov_y, c.near, c.far].iter().all(|v| v.is_finite()) {
                return Err(Problem::NotFinite("camera"));
            }
            if !(c.fov_y > 0.0 && c.fov_y < 180.0) {
                return Err(Problem::FovOutOfRange);
            }
            if !(0.0 < c.near && c.near < c.far) {
                return Err(Problem::NearFar);
            }
            Some(Camera {
                fov_y_degrees: c.fov_y,
                near: c.near,
                far: c.far,
            })
        }
        None => None,
    };
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
            Some(Character {
                run_speed: c.run_speed,
                turn_speed_degrees: c.turn_speed,
            })
        }
        None => None,
    };
    if let Some(o) = &desc.orbit_camera {
        validate_orbit_camera(o, camera.is_some())?;
    }
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
        parent,
        children: Vec::new(),
        transform: Transform {
            position,
            rotation,
            scale,
        },
        mesh: desc.mesh.map(|kind| Mesh {
            kind,
            color: mesh_color,
        }),
        camera,
        light,
        character,
        orbit_camera: None,
        collider,
    })
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
