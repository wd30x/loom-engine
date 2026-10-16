//! A scene file as a document to edit and write back
//! (`docs/architecture.md` section 10, "规范格式", and section 13,
//! "运行中编辑"). Edits change only the document; [`SceneDoc::build`]
//! validates it like a freshly loaded file.

use std::collections::BTreeMap;

use ron::extensions::Extensions;
use ron::ser::PrettyConfig;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::file::{NodeDesc, NodePatch, SceneFile, ScenePatch};
use crate::{Scene, SceneError};

/// A failed edit: the path, a field or a RON fragment is wrong. The message
/// is the whole explanation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EditError(pub String);

fn fail<T>(message: impl Into<String>) -> Result<T, EditError> {
    Err(EditError(message.into()))
}

/// The optional fields of a node, in canonical order: what `unset` takes.
const OPTIONAL_FIELDS: [&str; 17] = [
    "display_name",
    "position",
    "rotation",
    "look_at",
    "scale",
    "bone",
    "mesh",
    "model",
    "animation",
    "color",
    "camera",
    "sprite",
    "tilemap",
    "light",
    "character",
    "orbit_camera",
    "collider",
];

/// The parsed file structure of a scene: what the file says, before
/// validation, with the edits made to it. Serializes as the file does (as
/// JSON: what [`crate::scene_schema`] describes).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SceneDoc {
    file: SceneFile,
}

fn options() -> ron::Options {
    ron::Options::default().with_default_extension(Extensions::IMPLICIT_SOME)
}

/// Canonical RON of `value`: ron's pretty printer, 4-space indent, bare
/// optional values, no struct names, `\n` line ends, a final newline.
fn canonical(value: &impl Serialize) -> String {
    let config = PrettyConfig::new()
        .indentor("    ".to_owned())
        .new_line("\n".to_owned());
    let mut text = options()
        .to_string_pretty(value, config)
        .expect("scene file structs contain only plain data");
    text.push('\n');
    text
}

/// Parses `(<fragment>)` as `T`: the fields of a `set`. Positions in the
/// message are those of the fragment.
fn fields<T: DeserializeOwned>(fragment: &str) -> Result<T, EditError> {
    options()
        .from_str(&format!("({fragment})"))
        .or_else(|e: ron::error::SpannedError| {
            let (line, col) = (e.span.start.line, e.span.start.col);
            // The added "(" shifts the first line by one column.
            let col = if line == 1 {
                col.saturating_sub(1).max(1)
            } else {
                col
            };
            // ron names the struct; the fields are the node's or the scene's.
            let message = e.code.to_string().replace(" in `NodePatch`", "");
            let message = message.replace(" in `ScenePatch`", "");
            fail(format!("set: {line}:{col}: {message}"))
        })
}

impl SceneDoc {
    /// Parses scene file text (RON syntax and field names only; nothing
    /// is validated yet).
    pub fn parse(src: &str) -> Result<Self, SceneError> {
        Ok(Self {
            file: options().from_str(src)?,
        })
    }

    /// The canonical text of the document.
    pub fn to_ron(&self) -> String {
        canonical(&self.file)
    }

    /// Validates the document by every rule of loading a scene file and
    /// builds the scene.
    pub fn build(&self) -> Result<Scene, SceneError> {
        Scene::from_file(&self.file)
    }

    /// The node at `path` in canonical RON, without its children, and the
    /// names of its children in order.
    pub fn node_ron(&self, path: &str) -> Result<(String, Vec<String>), EditError> {
        let node = self.node(path)?;
        let children = node.children.iter().map(|c| c.name.clone()).collect();
        let alone = NodeDesc {
            children: Vec::new(),
            ..node.clone()
        };
        Ok((canonical(&alone), children))
    }

    /// `edit_node`: sets the fields of `set` (a RON field list), removes
    /// the optional fields of `unset`, then merges `props` (a value sets or
    /// adds one, `None` removes one), in name order.
    pub fn edit_node(
        &mut self,
        path: &str,
        set: Option<&str>,
        unset: &[String],
        props: &BTreeMap<String, Option<f64>>,
    ) -> Result<(), EditError> {
        if set.is_none() && unset.is_empty() && props.is_empty() {
            return fail("edit_node needs set, unset or props");
        }
        let patch: Option<NodePatch> = set.map(fields).transpose()?;
        if let Some(patch) = &patch {
            if patch.children.is_some() {
                return fail("set cannot give children: use add_node and remove_node");
            }
            for field in unset {
                if patch_has(patch, field) {
                    return fail(format!("{field} is both in set and in unset"));
                }
            }
        }
        for (i, field) in unset.iter().enumerate() {
            match field.as_str() {
                "name" => return fail("unset cannot remove name"),
                "props" => return fail("unset cannot remove props: set props: [] instead"),
                "children" => return fail("unset cannot remove children: use remove_node"),
                f if !OPTIONAL_FIELDS.contains(&f) => {
                    return fail(format!(
                        "unset: unknown field {f:?}, expected one of {}",
                        OPTIONAL_FIELDS.join(", ")
                    ));
                }
                f if unset[..i].iter().any(|u| u == f) => {
                    return fail(format!("unset lists {f:?} twice"));
                }
                _ => {}
            }
        }
        let node = self.node_mut(path)?;
        if let Some(p) = patch {
            apply(node, p);
        }
        for field in unset {
            clear(node, field);
        }
        for (name, value) in props {
            let at = node.props.iter().position(|(n, _)| n == name);
            match (value, at) {
                (Some(v), Some(i)) => node.props[i].1 = *v,
                (Some(v), None) => node.props.push((name.clone(), *v)),
                (None, Some(i)) => {
                    node.props.remove(i);
                }
                (None, None) => {
                    return fail(format!("prop {name:?} is not on the node {path:?}"));
                }
            }
        }
        Ok(())
    }

    /// `add_node`: the node `ron` (with any children) under `parent` (a
    /// root without one), at `index` among its siblings or last.
    pub fn add_node(
        &mut self,
        parent: Option<&str>,
        index: Option<usize>,
        ron: &str,
    ) -> Result<(), EditError> {
        let node: NodeDesc = options()
            .from_str(ron)
            .or_else(|e| fail(format!("ron: {e}")))?;
        let siblings = match parent {
            Some(path) => &mut self.node_mut(path)?.children,
            None => &mut self.file.nodes,
        };
        let index = index.unwrap_or(siblings.len());
        if index > siblings.len() {
            let under = match parent {
                Some(path) => format!("{path:?} has"),
                None => "the root has".to_owned(),
            };
            return fail(format!(
                "index {index} is past the end: {under} {} children",
                siblings.len()
            ));
        }
        siblings.insert(index, node);
        Ok(())
    }

    /// `remove_node`: the node at `path` and its whole subtree.
    pub fn remove_node(&mut self, path: &str) -> Result<(), EditError> {
        let (parent, name) = match path.rsplit_once('/') {
            Some((parent, name)) => (Some(parent), name),
            None => (None, path),
        };
        let siblings = match parent {
            Some(parent) => &mut self.node_mut(parent)?.children,
            None => &mut self.file.nodes,
        };
        match siblings.iter().position(|n| n.name == name) {
            Some(i) => {
                siblings.remove(i);
                Ok(())
            }
            None => fail(format!("no node at {path:?}")),
        }
    }

    /// `edit_scene`: sets the top-level `camera` and `ambient` given in
    /// `set`.
    pub fn edit_scene(&mut self, set: &str) -> Result<(), EditError> {
        let patch: ScenePatch = fields(set)?;
        for (given, field) in [
            (patch.version.is_some(), "version"),
            (patch.game.is_some(), "game"),
            (patch.nodes.is_some(), "nodes"),
        ] {
            if given {
                return fail(format!("edit_scene cannot change {field}"));
            }
        }
        if let Some(camera) = patch.camera {
            self.file.camera = camera;
        }
        if let Some(ambient) = patch.ambient {
            self.file.ambient = ambient;
        }
        Ok(())
    }

    fn node(&self, path: &str) -> Result<&NodeDesc, EditError> {
        let mut names = path.split('/');
        let first = names.next().unwrap_or_default();
        let mut node = self.file.nodes.iter().find(|n| n.name == first);
        for name in names {
            node = node.and_then(|n| n.children.iter().find(|c| c.name == name));
        }
        node.map_or_else(|| fail(format!("no node at {path:?}")), Ok)
    }

    fn node_mut(&mut self, path: &str) -> Result<&mut NodeDesc, EditError> {
        let mut names = path.split('/');
        let first = names.next().unwrap_or_default();
        let mut node = self.file.nodes.iter_mut().find(|n| n.name == first);
        for name in names {
            node = node.and_then(|n| n.children.iter_mut().find(|c| c.name == name));
        }
        node.map_or_else(|| fail(format!("no node at {path:?}")), Ok)
    }
}

/// Whether `set` gave `field`.
fn patch_has(p: &NodePatch, field: &str) -> bool {
    match field {
        "name" => p.name.is_some(),
        "display_name" => p.display_name.is_some(),
        "position" => p.position.is_some(),
        "rotation" => p.rotation.is_some(),
        "look_at" => p.look_at.is_some(),
        "scale" => p.scale.is_some(),
        "bone" => p.bone.is_some(),
        "mesh" => p.mesh.is_some(),
        "model" => p.model.is_some(),
        "animation" => p.animation.is_some(),
        "color" => p.color.is_some(),
        "camera" => p.camera.is_some(),
        "sprite" => p.sprite.is_some(),
        "tilemap" => p.tilemap.is_some(),
        "light" => p.light.is_some(),
        "character" => p.character.is_some(),
        "orbit_camera" => p.orbit_camera.is_some(),
        "props" => p.props.is_some(),
        "collider" => p.collider.is_some(),
        _ => false,
    }
}

/// Replaces every field `p` gives.
fn apply(node: &mut NodeDesc, p: NodePatch) {
    if let Some(name) = p.name {
        node.name = name;
    }
    if let Some(props) = p.props {
        node.props = props;
    }
    macro_rules! set {
        ($($field:ident),*) => {
            $(if p.$field.is_some() {
                node.$field = p.$field;
            })*
        };
    }
    set!(
        display_name,
        position,
        rotation,
        look_at,
        scale,
        bone,
        mesh,
        model,
        animation,
        color,
        camera,
        sprite,
        tilemap,
        light,
        character,
        orbit_camera,
        collider
    );
}

/// Removes the optional field `field` (one of [`OPTIONAL_FIELDS`]).
fn clear(node: &mut NodeDesc, field: &str) {
    match field {
        "display_name" => node.display_name = None,
        "position" => node.position = None,
        "rotation" => node.rotation = None,
        "look_at" => node.look_at = None,
        "scale" => node.scale = None,
        "bone" => node.bone = None,
        "mesh" => node.mesh = None,
        "model" => node.model = None,
        "animation" => node.animation = None,
        "color" => node.color = None,
        "camera" => node.camera = None,
        "sprite" => node.sprite = None,
        "tilemap" => node.tilemap = None,
        "light" => node.light = None,
        "character" => node.character = None,
        "orbit_camera" => node.orbit_camera = None,
        "collider" => node.collider = None,
        _ => unreachable!("unset names are checked against OPTIONAL_FIELDS"),
    }
}
