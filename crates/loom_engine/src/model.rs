//! glTF models of a scene's model nodes (`docs/architecture.md` section
//! 19): `.glb` files, read and checked when the scene is loaded, whether or
//! not it is rendered, merged into one mesh each and uploaded to the
//! renderer on first draw. An animated node draws its model's skinned mesh
//! instead, posed by its playing clip (`crate::anim`). Also `loom model`,
//! which describes one file.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use glam::{Mat3, Mat4, Quat, Vec3};
use loom_core::debug::DebugDraw;
use loom_render::{JointRange, MeshId, RenderError, Renderer, SkinnedVertex, Vertex};
use loom_scene::Scene;
use serde::Serialize;

use crate::anim::{self, Clips};

/// A model merged into one mesh, in its bind pose.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModelMesh {
    pub vertices: Vec<Vertex>,
    /// Three a triangle, counter-clockwise seen from the front.
    pub indices: Vec<u32>,
}

/// What a `.glb` holds, as `loom model` reports it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Imported {
    pub mesh: ModelMesh,
    /// The skinned mesh, or why the model has none: only animated nodes
    /// need one, so only they turn the reason into an error.
    pub skinned: Result<Skinned, String>,
    pub materials: Vec<MaterialInfo>,
    pub skins: Vec<SkinInfo>,
    pub animations: Vec<AnimationInfo>,
}

/// A model's mesh for skinning: every mesh of the default scene, not
/// placed by its node (glTF's rule for skinned meshes: the joint matrices
/// place it), and the skeleton of its one skin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Skinned {
    /// Joint indices into [`Skeleton::joints`]; weights sum to 1.
    pub vertices: Vec<SkinnedVertex>,
    /// Three a triangle, counter-clockwise seen from the front.
    pub indices: Vec<u32>,
    pub skeleton: Skeleton,
}

/// The joints of a skin, a parent before its children.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Skeleton {
    pub joints: Vec<Joint>,
}

/// One joint: its glTF node's local transform, and its bind pose in model
/// space (the default scene's space).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Joint {
    pub name: Option<String>,
    /// The joint's glTF node index.
    pub node: usize,
    /// The parent joint's index, which is lower; `None` when the parent
    /// node is not a joint of the skin (or there is none).
    pub parent: Option<usize>,
    /// Without a parent joint: the parent node's model-space transform
    /// (identity for a root node); the identity otherwise.
    pub outer: Mat4,
    /// The local translation, rotation and scale (`T_j`, `R_j`).
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    /// The bind pose's model-space rotation (`B_j`).
    pub bind_rotation: Quat,
    /// The parent's bind-pose model-space rotation (`B_p`): the parent
    /// joint's, or `outer`'s.
    pub parent_bind_rotation: Quat,
    /// From the skin; the identity if the skin has none.
    pub inverse_bind: Mat4,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct MaterialInfo {
    pub name: Option<String>,
    /// `baseColorFactor` without its alpha, linear RGB.
    pub color: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SkinInfo {
    pub name: Option<String>,
    /// In the skin's order.
    pub joints: Vec<JointInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct JointInfo {
    pub name: Option<String>,
    /// The parent node's name if the parent is a joint of the same skin.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct AnimationInfo {
    pub name: Option<String>,
    /// The last keyframe time of any channel.
    pub seconds: f32,
    pub channels: usize,
}

/// Reads a `.glb` from `bytes` by the import rules of section 19.
pub(crate) fn import(bytes: &[u8]) -> Result<Imported> {
    let gltf = gltf::Gltf::from_slice(bytes).context("not a valid glTF binary")?;
    let version = &gltf.document.as_json().asset.version;
    if version != "2.0" {
        bail!("not glTF 2.0: the asset version is {version:?}");
    }
    let blob = gltf.blob.as_deref();
    for buffer in gltf.buffers() {
        if !matches!(buffer.source(), gltf::buffer::Source::Bin) {
            bail!("buffer {} is not stored in the .glb", buffer.index());
        }
    }
    if let Some(accessor) = gltf.accessors().find(|a| a.sparse().is_some()) {
        bail!("accessor {} is sparse", accessor.index());
    }
    for m in gltf.materials() {
        let pbr = m.pbr_metallic_roughness();
        let textured = pbr.base_color_texture().is_some()
            || pbr.metallic_roughness_texture().is_some()
            || m.normal_texture().is_some()
            || m.occlusion_texture().is_some()
            || m.emissive_texture().is_some();
        if textured {
            bail!("material {} uses a texture", material_label(&m));
        }
    }

    let mut mesh = ModelMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let scene = gltf.default_scene().or_else(|| gltf.scenes().next());
    let nodes = scene_nodes(scene.as_ref());
    let mut found = false;
    for (node, world) in &nodes {
        if let Some(m) = node.mesh() {
            found = true;
            add_mesh(&mut mesh, &m, *world, blob)?;
        }
    }
    if !found {
        bail!("the default scene has no mesh");
    }

    let materials = gltf
        .materials()
        .map(|m| {
            let [r, g, b, _] = m.pbr_metallic_roughness().base_color_factor();
            MaterialInfo {
                name: m.name().map(str::to_owned),
                color: [r, g, b],
            }
        })
        .collect();
    let parents: BTreeMap<usize, usize> = gltf
        .nodes()
        .flat_map(|p| p.children().map(move |c| (c.index(), p.index())))
        .collect();
    let skinned = skinned(&gltf, blob, &nodes, &parents).map_err(|e| format!("{e:#}"));
    let skins = gltf
        .skins()
        .map(|skin| {
            let joints: Vec<_> = skin.joints().collect();
            SkinInfo {
                name: skin.name().map(str::to_owned),
                joints: joints
                    .iter()
                    .map(|j| JointInfo {
                        name: j.name().map(str::to_owned),
                        parent: parents
                            .get(&j.index())
                            .and_then(|p| joints.iter().find(|o| o.index() == *p))
                            .and_then(|p| p.name().map(str::to_owned)),
                    })
                    .collect(),
            }
        })
        .collect();
    let animations = gltf
        .animations()
        .map(|a| {
            let mut seconds = 0.0_f32;
            for channel in a.channels() {
                let reader = channel.reader(|b| buffer_data(b, blob));
                if let Some(inputs) = reader.read_inputs() {
                    seconds = inputs.fold(seconds, f32::max);
                }
            }
            AnimationInfo {
                name: a.name().map(str::to_owned),
                seconds,
                channels: a.channels().count(),
            }
        })
        .collect();
    Ok(Imported {
        mesh,
        skinned,
        materials,
        skins,
        animations,
    })
}

/// A material by index, and by name if it has one.
fn material_label(m: &gltf::Material<'_>) -> String {
    match (m.index(), m.name()) {
        (Some(i), Some(name)) => format!("{i} ({name:?})"),
        (Some(i), None) => i.to_string(),
        (None, _) => "(default)".to_owned(),
    }
}

/// The bytes of `buffer`: only the `.glb`'s own binary chunk (checked
/// before any is read).
pub(crate) fn buffer_data<'a>(
    buffer: gltf::Buffer<'_>,
    blob: Option<&'a [u8]>,
) -> Option<&'a [u8]> {
    match buffer.source() {
        gltf::buffer::Source::Bin => blob,
        gltf::buffer::Source::Uri(_) => None,
    }
}

/// Every node of `scene` with its world transform, depth first with
/// children in order: a node after its parent.
fn scene_nodes<'a>(scene: Option<&gltf::Scene<'a>>) -> Vec<(gltf::Node<'a>, Mat4)> {
    let mut out = Vec::new();
    let mut stack: Vec<(gltf::Node<'a>, Mat4)> = scene
        .iter()
        .flat_map(|s| s.nodes())
        .map(|n| (n, Mat4::IDENTITY))
        .collect();
    stack.reverse();
    while let Some((node, parent)) = stack.pop() {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        let children: Vec<_> = node.children().map(|c| (c, world)).collect();
        stack.extend(children.into_iter().rev());
        out.push((node, world));
    }
    out
}

/// A primitive's data, checked: positions, normals (as many), indices
/// (three a triangle, in range) and the material's color.
struct Primitive {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    color: [f32; 3],
}

/// Primitive `p` of mesh `m` as "mesh M primitive P", for errors.
fn primitive_label(m: &gltf::Mesh<'_>, p: &gltf::Primitive<'_>) -> String {
    format!("mesh {} primitive {}", m.index(), p.index())
}

fn read_primitive(
    m: &gltf::Mesh<'_>,
    p: &gltf::Primitive<'_>,
    blob: Option<&[u8]>,
) -> Result<Primitive> {
    let label = || primitive_label(m, p);
    if p.mode() != gltf::mesh::Mode::Triangles {
        bail!("{} is not triangles", label());
    }
    let reader = p.reader(|b| buffer_data(b, blob));
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .with_context(|| format!("{} has no POSITION", label()))?
        .collect();
    let normals: Vec<[f32; 3]> = reader
        .read_normals()
        .with_context(|| format!("{} has no NORMAL", label()))?
        .collect();
    if normals.len() != positions.len() {
        bail!(
            "{} has {} normals for {} positions",
            label(),
            normals.len(),
            positions.len()
        );
    }
    let indices: Vec<u32> = match reader.read_indices() {
        Some(i) => i.into_u32().collect(),
        None => (0..u32::try_from(positions.len())?).collect(),
    };
    if !indices.len().is_multiple_of(3) {
        bail!(
            "{} has {} indices, not a multiple of 3",
            label(),
            indices.len()
        );
    }
    if let Some(i) = indices.iter().find(|&&i| i as usize >= positions.len()) {
        bail!(
            "{} has index {i}, but only {} vertices",
            label(),
            positions.len()
        );
    }
    let color = match p.material().index() {
        Some(_) => {
            let [r, g, b, _] = p.material().pbr_metallic_roughness().base_color_factor();
            [r, g, b]
        }
        None => [1.0; 3],
    };
    Ok(Primitive {
        positions,
        normals,
        indices,
        color,
    })
}

/// Adds every primitive of `m`, placed by `world`, to `out`.
fn add_mesh(
    out: &mut ModelMesh,
    m: &gltf::Mesh<'_>,
    world: Mat4,
    blob: Option<&[u8]>,
) -> Result<()> {
    let normal_matrix = Mat3::from_mat4(world).inverse().transpose();
    // A mirroring transform turns the winding around (glTF 2.0, 3.7.4).
    let mirrored = Mat3::from_mat4(world).determinant() < 0.0;
    for p in m.primitives() {
        let Primitive {
            positions,
            normals,
            indices,
            color,
        } = read_primitive(m, &p, blob)?;
        let base = u32::try_from(out.vertices.len()).context("too many vertices")?;
        out.vertices
            .extend(positions.iter().zip(&normals).map(|(p, n)| {
                Vertex {
                    position: world.transform_point3(Vec3::from_array(*p)).to_array(),
                    normal: (normal_matrix * Vec3::from_array(*n))
                        .normalize_or_zero()
                        .to_array(),
                    color,
                }
            }));
        for &[a, b, c] in indices.as_chunks::<3>().0 {
            let t = if mirrored { [a, c, b] } else { [a, b, c] };
            out.indices.extend(t.map(|i| base + i));
        }
    }
    Ok(())
}

/// The skinned mesh and skeleton of a model with exactly one skin
/// (`docs/architecture.md` section 19, "蒙皮"). `nodes` is the default
/// scene's nodes with their world transforms ([`scene_nodes`]), `parents`
/// every node's parent.
fn skinned(
    gltf: &gltf::Gltf,
    blob: Option<&[u8]>,
    nodes: &[(gltf::Node<'_>, Mat4)],
    parents: &BTreeMap<usize, usize>,
) -> Result<Skinned> {
    let skins: Vec<_> = gltf.skins().collect();
    let skin = match skins.as_slice() {
        [skin] => skin,
        [] => bail!("the model has no skin"),
        _ => bail!("the model has {} skins, not one", skins.len()),
    };
    let worlds: BTreeMap<usize, Mat4> = nodes.iter().map(|(n, w)| (n.index(), *w)).collect();
    let joint_nodes: Vec<gltf::Node<'_>> = skin.joints().collect();
    // Vertices index the joints with u16s.
    if joint_nodes.len() > usize::from(u16::MAX) + 1 {
        bail!("the skin has {} joints, more than 65536", joint_nodes.len());
    }
    let skin_index = |node: usize| joint_nodes.iter().position(|j| j.index() == node);
    let inverse_binds: Vec<Mat4> = match skin
        .reader(|b| buffer_data(b, blob))
        .read_inverse_bind_matrices()
    {
        Some(m) => m.map(|m| Mat4::from_cols_array_2d(&m)).collect(),
        None => vec![Mat4::IDENTITY; joint_nodes.len()],
    };
    if inverse_binds.len() != joint_nodes.len() {
        bail!(
            "the skin has {} inverse bind matrices for {} joints",
            inverse_binds.len(),
            joint_nodes.len()
        );
    }
    // Parents before children: by the number of joint ancestors, then in
    // skin order.
    let parent_joint = |i: usize| {
        parents
            .get(&joint_nodes[i].index())
            .and_then(|&p| skin_index(p))
    };
    let depth = |mut i: usize| {
        let mut depth = 0;
        while let Some(p) = parent_joint(i) {
            depth += 1;
            i = p;
            if depth > joint_nodes.len() {
                break;
            }
        }
        depth
    };
    let mut order: Vec<usize> = (0..joint_nodes.len()).collect();
    order.sort_by_key(|&i| depth(i));
    let mut sorted_of = vec![0; joint_nodes.len()];
    for (sorted, &i) in order.iter().enumerate() {
        sorted_of[i] = sorted;
    }
    let mut joints: Vec<Joint> = Vec::with_capacity(order.len());
    for &i in &order {
        let node = &joint_nodes[i];
        let world = *worlds
            .get(&node.index())
            .with_context(|| format!("joint {} is not in the default scene", node.index()))?;
        let (translation, rotation, scale) = node.transform().decomposed();
        let parent = parent_joint(i).map(|p| sorted_of[p]);
        let outer = match (parent, parents.get(&node.index())) {
            (None, Some(p)) => *worlds
                .get(p)
                .expect("a node in the default scene has its parent there too"),
            _ => Mat4::IDENTITY,
        };
        let parent_bind_rotation = match parent {
            Some(p) => joints[p].bind_rotation,
            None => outer.to_scale_rotation_translation().1,
        };
        joints.push(Joint {
            name: node.name().map(str::to_owned),
            node: node.index(),
            parent,
            outer,
            translation: Vec3::from_array(translation),
            rotation: Quat::from_array(rotation),
            scale: Vec3::from_array(scale),
            bind_rotation: world.to_scale_rotation_translation().1,
            parent_bind_rotation,
            inverse_bind: inverse_binds[i],
        });
    }

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (node, _) in nodes {
        let Some(m) = node.mesh() else { continue };
        if node.skin().is_none_or(|s| s.index() != skin.index()) {
            bail!("node {} has a mesh but not the skin", node.index());
        }
        for p in m.primitives() {
            let label = || primitive_label(&m, &p);
            let data = read_primitive(&m, &p, blob)?;
            if p.get(&gltf::Semantic::Joints(1)).is_some() {
                bail!("{} has JOINTS_1: at most 4 joints a vertex", label());
            }
            let reader = p.reader(|b| buffer_data(b, blob));
            let joint_sets: Vec<[u16; 4]> = reader
                .read_joints(0)
                .with_context(|| format!("{} has no JOINTS_0", label()))?
                .into_u16()
                .collect();
            let weight_sets: Vec<[f32; 4]> = reader
                .read_weights(0)
                .with_context(|| format!("{} has no WEIGHTS_0", label()))?
                .into_f32()
                .collect();
            let count = data.positions.len();
            if joint_sets.len() != count || weight_sets.len() != count {
                bail!(
                    "{} has {} JOINTS_0 and {} WEIGHTS_0 for {count} positions",
                    label(),
                    joint_sets.len(),
                    weight_sets.len()
                );
            }
            let base = u32::try_from(vertices.len()).context("too many vertices")?;
            for (v, ((position, normal), (js, ws))) in data
                .positions
                .iter()
                .zip(&data.normals)
                .zip(joint_sets.iter().zip(&weight_sets))
                .enumerate()
            {
                if let Some(&j) = js.iter().find(|&&j| usize::from(j) >= joints.len()) {
                    bail!(
                        "{} has joint index {j}, but the skin has {} joints",
                        label(),
                        joints.len()
                    );
                }
                let sum: f32 = ws.iter().sum();
                if sum.is_nan() || sum <= 0.0 {
                    bail!("{} vertex {v} has weights that sum to 0", label());
                }
                vertices.push(SkinnedVertex {
                    position: *position,
                    normal: *normal,
                    color: data.color,
                    joints: js.map(|j| {
                        u16::try_from(sorted_of[usize::from(j)])
                            .expect("a skin has at most 65536 joints, checked above")
                    }),
                    weights: ws.map(|w| w / sum),
                });
            }
            indices.extend(data.indices.iter().map(|i| base + i));
        }
    }
    Ok(Skinned {
        vertices,
        indices,
        skeleton: Skeleton { joints },
    })
}

/// Reads and imports the `.glb` at `path`.
pub(crate) fn load_file(path: &Path) -> Result<Imported> {
    let bytes = std::fs::read(path).context("reading the file")?;
    import(&bytes)
}

/// One loaded model: its meshes, and the renderer's copies once drawn.
#[derive(Debug)]
struct Loaded {
    mesh: ModelMesh,
    id: Option<MeshId>,
    skinned: Result<Skinned, String>,
    skinned_id: Option<MeshId>,
}

/// Every model a scene uses, by path as written in the scene, and the
/// clips of its animated nodes.
#[derive(Debug, Default)]
pub(crate) struct Models {
    models: BTreeMap<String, Loaded>,
    clips: Clips,
}

impl Models {
    /// Reads each model the scene's model nodes use, once per path
    /// (relative to the working directory), and checks it; for an animated
    /// node, also that the model can be skinned, and its clips
    /// ([`Clips::load`]). Errors name the node and the file, e.g. `node
    /// "boss": model assets/models/humanoid.glb: mesh 0 primitive 2 is not
    /// triangles`.
    pub fn load(scene: &Scene) -> Result<Self> {
        let mut models: BTreeMap<String, Loaded> = BTreeMap::new();
        let mut clips = Clips::default();
        for &id in scene.model_nodes() {
            let model = scene
                .node(id)
                .model
                .as_ref()
                .expect("Scene::model_nodes only returns nodes with a model");
            let context = || format!("node {:?}: model {}", scene.path(id), model.path);
            if !models.contains_key(&model.path) {
                let imported = load_file(Path::new(&model.path)).with_context(context)?;
                models.insert(
                    model.path.clone(),
                    Loaded {
                        mesh: imported.mesh,
                        id: None,
                        skinned: imported.skinned,
                        skinned_id: None,
                    },
                );
            }
            let Some(animation) = &model.animation else {
                continue;
            };
            let skinned = match &models[&model.path].skinned {
                Ok(skinned) => skinned,
                Err(why) => return Err(anyhow::anyhow!("{why}")).with_context(context),
            };
            clips.load(scene, id, animation, &skinned.skeleton)?;
        }
        // Nodes hung on a bone of their parent's model.
        for id in scene.ids() {
            let Some(bone) = &scene.node(id).bone else {
                continue;
            };
            let parent = scene
                .node(id)
                .parent
                .expect("Scene::parse checks a node with a bone has a parent");
            let model = scene
                .node(parent)
                .model
                .as_ref()
                .expect("Scene::parse checks a node with a bone has a parent with a model");
            let context = || format!("node {:?}: bone {bone:?}", scene.path(id));
            let skinned = match &models[&model.path].skinned {
                Ok(skinned) => skinned,
                Err(why) => {
                    return Err(anyhow::anyhow!("model {}: {why}", model.path))
                        .with_context(context);
                }
            };
            if !skinned
                .skeleton
                .joints
                .iter()
                .any(|j| j.name.as_deref() == Some(bone.as_str()))
            {
                return Err(anyhow::anyhow!(
                    "model {}: not a joint of the model's skin",
                    model.path
                ))
                .with_context(context);
            }
        }
        Ok(Self { models, clips })
    }

    /// The model-space pose of animated `model` (one [`Models::load`] read)
    /// at `tick`; `None` for a model without an animation.
    pub fn pose(&self, model: &loom_scene::Model, tick: u64) -> Option<Vec<Mat4>> {
        let animation = model.animation.as_ref()?;
        let skinned = self.models[&model.path]
            .skinned
            .as_ref()
            .expect("Models::load checked an animated model can be skinned");
        Some(self.clips.pose(&skinned.skeleton, animation, tick))
    }

    /// Where a node hanging on `bone` of `model` is placed in model space,
    /// with `model` in `pose` ([`Models::pose`]; `None`: its bind pose)
    /// ([`anim::anchor`]). [`Models::load`] checked the bone.
    pub fn anchor(&self, model: &loom_scene::Model, pose: Option<&[Mat4]>, bone: &str) -> Mat4 {
        let skeleton = &self.models[&model.path]
            .skinned
            .as_ref()
            .expect("Models::load checked a model with bones hung on it can be skinned")
            .skeleton;
        let joint = skeleton
            .joints
            .iter()
            .position(|j| j.name.as_deref() == Some(bone))
            .expect("Models::load checked the bone is a joint of the skin");
        anim::anchor(skeleton, pose, joint)
    }

    /// What to draw for `model`, a model [`Models::load`] read, placed by
    /// `world`: its mesh, uploaded on first use; for an animated model, its
    /// skinned mesh posed by `pose` ([`Models::pose`]) and the range of its
    /// joint matrices, which are appended to `joints`, and with `skeleton`
    /// its bones in that same pose.
    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        model: &loom_scene::Model,
        world: Mat4,
        pose: Option<&[Mat4]>,
        joints: &mut Vec<Mat4>,
        skeleton: Option<&mut DebugDraw>,
    ) -> Result<(MeshId, Option<JointRange>), RenderError> {
        let loaded = self
            .models
            .get_mut(&model.path)
            .expect("Models::load read every model of the scene");
        let Some(pose) = pose else {
            if let Some(id) = loaded.id {
                return Ok((id, None));
            }
            let id = renderer.create_mesh(&loaded.mesh.vertices, &loaded.mesh.indices)?;
            loaded.id = Some(id);
            return Ok((id, None));
        };
        let skinned = loaded
            .skinned
            .as_ref()
            .expect("Models::load checked an animated model can be skinned");
        let id = match loaded.skinned_id {
            Some(id) => id,
            None => {
                let id = renderer.create_skinned_mesh(&skinned.vertices, &skinned.indices)?;
                loaded.skinned_id = Some(id);
                id
            }
        };
        if let Some(draw) = skeleton {
            anim::draw_skeleton(&skinned.skeleton, pose, world, draw);
        }
        let first = u32::try_from(joints.len()).expect("far fewer than 2^32 joints");
        joints.extend(anim::joint_matrices(&skinned.skeleton, pose));
        let count = u32::try_from(skinned.skeleton.joints.len()).expect("a skin's joints fit u16");
        Ok((id, Some(JointRange { first, count })))
    }
}

/// `loom model` (`docs/architecture.md` section 19).
#[derive(Debug, Serialize)]
pub(crate) struct ModelReport {
    pub status: &'static str,
    pub path: String,
    pub vertices: usize,
    pub triangles: usize,
    pub bounds: Bounds,
    pub materials: Vec<MaterialInfo>,
    pub skins: Vec<SkinInfo>,
    pub animations: Vec<AnimationInfo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// `loom model <path>`: reads the file by the import rules and describes it.
pub(crate) fn describe(path: &Path) -> Result<ModelReport> {
    let imported = load_file(path).with_context(|| format!("model {}", path.display()))?;
    let (min, max) = imported.mesh.vertices.iter().fold(
        (Vec3::INFINITY, Vec3::NEG_INFINITY),
        |(min, max), v| {
            let p = Vec3::from_array(v.position);
            (min.min(p), max.max(p))
        },
    );
    Ok(ModelReport {
        status: "ok",
        path: path.display().to_string(),
        vertices: imported.mesh.vertices.len(),
        triangles: imported.mesh.indices.len() / 3,
        bounds: Bounds {
            min: min.to_array(),
            max: max.to_array(),
        },
        materials: imported.materials,
        skins: imported.skins,
        animations: imported.animations,
    })
}

#[cfg(test)]
#[path = "model_tests.rs"]
pub(crate) mod tests;
