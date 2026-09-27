//! glTF models of a scene's model nodes (`docs/architecture.md` section
//! 19): `.glb` files, read and checked when the scene is loaded, whether or
//! not it is rendered, merged into one mesh each and uploaded to the
//! renderer on first draw. Also `loom model`, which describes one file.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use glam::{Mat3, Mat4, Vec3};
use loom_render::{MeshId, RenderError, Renderer, Vertex};
use loom_scene::Scene;
use serde::Serialize;

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
    pub materials: Vec<MaterialInfo>,
    pub skins: Vec<SkinInfo>,
    pub animations: Vec<AnimationInfo>,
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
    let mut found = false;
    let scene = gltf.default_scene().or_else(|| gltf.scenes().next());
    let mut stack: Vec<(gltf::Node<'_>, Mat4)> = scene
        .iter()
        .flat_map(|s| s.nodes())
        .map(|n| (n, Mat4::IDENTITY))
        .collect();
    // Depth first, children in order: a node after its parent.
    stack.reverse();
    while let Some((node, parent)) = stack.pop() {
        let world = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
        if let Some(m) = node.mesh() {
            found = true;
            add_mesh(&mut mesh, &m, world, blob)?;
        }
        let children: Vec<_> = node.children().map(|c| (c, world)).collect();
        stack.extend(children.into_iter().rev());
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
fn buffer_data<'a>(buffer: gltf::Buffer<'_>, blob: Option<&'a [u8]>) -> Option<&'a [u8]> {
    match buffer.source() {
        gltf::buffer::Source::Bin => blob,
        gltf::buffer::Source::Uri(_) => None,
    }
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
        let label = || format!("mesh {} primitive {}", m.index(), p.index());
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

/// Reads and imports the `.glb` at `path`.
pub(crate) fn load_file(path: &Path) -> Result<Imported> {
    let bytes = std::fs::read(path).context("reading the file")?;
    import(&bytes)
}

/// One loaded model: its mesh, and the renderer's copy once drawn.
#[derive(Debug)]
struct Loaded {
    mesh: ModelMesh,
    id: Option<MeshId>,
}

/// Every model a scene uses, by path as written in the scene.
#[derive(Debug, Default)]
pub(crate) struct Models {
    models: BTreeMap<String, Loaded>,
}

impl Models {
    /// Reads each model the scene's model nodes use, once per path
    /// (relative to the working directory), and checks it. Errors name
    /// the node and the file, e.g. `node "boss": model
    /// assets/models/humanoid.glb: mesh 0 primitive 2 is not triangles`.
    pub fn load(scene: &Scene) -> Result<Self> {
        let mut models = BTreeMap::new();
        for &id in scene.model_nodes() {
            let model = scene
                .node(id)
                .model
                .as_ref()
                .expect("Scene::model_nodes only returns nodes with a model");
            if models.contains_key(&model.path) {
                continue;
            }
            let imported = load_file(Path::new(&model.path))
                .with_context(|| format!("node {:?}: model {}", scene.path(id), model.path))?;
            models.insert(
                model.path.clone(),
                Loaded {
                    mesh: imported.mesh,
                    id: None,
                },
            );
        }
        Ok(Self { models })
    }

    /// The mesh of the model at `path`, uploading it on first use; `path`
    /// is one [`Models::load`] read.
    pub fn draw(&mut self, renderer: &mut Renderer, path: &str) -> Result<MeshId, RenderError> {
        let model = self
            .models
            .get_mut(path)
            .expect("Models::load read every model of the scene");
        if let Some(id) = model.id {
            return Ok(id);
        }
        let id = renderer.create_mesh(&model.mesh.vertices, &model.mesh.indices)?;
        model.id = Some(id);
        Ok(id)
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
mod tests;
