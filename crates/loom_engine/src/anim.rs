//! Animation clips of a scene's animated model nodes, and posing a
//! skeleton by a clip (`docs/architecture.md` section 19, "骨骼和动画片段").
//! Clip files are read and checked when the scene is loaded, whether or
//! not it is rendered; sampling is `loom_scene::Clip`'s.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use glam::{Mat4, Quat, Vec3};
use loom_scene::{Animation, Clip, NodeId, Scene};

use crate::model::Skeleton;

/// Every clip a scene's animated nodes use, by path as written.
#[derive(Debug, Default)]
pub(crate) struct Clips {
    clips: BTreeMap<String, Clip>,
}

impl Clips {
    /// Reads each clip of node `id`'s `animation` not read yet (relative
    /// to the working directory), and checks that every bone it moves is a
    /// joint of `skeleton`, the node's model's. Errors name the node, the
    /// clip and the file, e.g. `node "knight": animation clip "walk"
    /// assets/anims/humanoid/walk.ron: bone "tail" is not a joint of the
    /// model's skin`.
    pub fn load(
        &mut self,
        scene: &Scene,
        id: NodeId,
        animation: &Animation,
        skeleton: &Skeleton,
    ) -> Result<()> {
        for (name, path) in &animation.clips {
            let context = || format!("node {:?}: animation clip {name:?} {path}", scene.path(id));
            if !self.clips.contains_key(path) {
                let clip = load_file(Path::new(path)).with_context(context)?;
                self.clips.insert(path.clone(), clip);
            }
            let clip = &self.clips[path];
            for bone in clip.bones() {
                if !skeleton
                    .joints
                    .iter()
                    .any(|j| j.name.as_deref() == Some(bone))
                {
                    return Err(anyhow::anyhow!(
                        "bone {bone:?} is not a joint of the model's skin"
                    ))
                    .with_context(context);
                }
            }
        }
        Ok(())
    }

    /// The clip `animation` plays; [`Clips::load`] read it.
    pub fn playing(&self, animation: &Animation) -> &Clip {
        let path = &animation.clips[&animation.playing];
        self.clips
            .get(path)
            .expect("Clips::load read every clip of the scene")
    }
}

/// Reads and checks the clip file at `path`.
fn load_file(path: &Path) -> Result<Clip> {
    let src = std::fs::read_to_string(path).context("reading the file")?;
    Ok(Clip::parse(&src)?)
}

/// The joint matrices of `skeleton` posed by `clip` `t` ticks after it
/// started, in the skeleton's order: each joint's model-space pose times
/// its inverse bind matrix. A bone the clip moves by rotation `q` and move
/// `d` gets the local rotation `R * (B⁻¹ * q * B)` and the local
/// translation `T + B_p⁻¹ * d`, so with its parent still it turns by `q`
/// about its own position along the model's axes and moves by `d`.
pub(crate) fn pose(skeleton: &Skeleton, clip: &Clip, t: u64) -> Vec<Mat4> {
    let mut model: Vec<Mat4> = Vec::with_capacity(skeleton.joints.len());
    for joint in &skeleton.joints {
        let (q, d) = joint
            .name
            .as_deref()
            .and_then(|name| clip.sample(name, t))
            .unwrap_or((Quat::IDENTITY, Vec3::ZERO));
        let rotation = joint.rotation * (joint.bind_rotation.inverse() * q * joint.bind_rotation);
        let translation = joint.translation + joint.parent_bind_rotation.inverse() * d;
        let local = Mat4::from_scale_rotation_translation(joint.scale, rotation, translation);
        let parent = match joint.parent {
            Some(p) => model[p],
            None => joint.outer,
        };
        model.push(parent * local);
    }
    model
        .iter()
        .zip(&skeleton.joints)
        .map(|(m, j)| *m * j.inverse_bind)
        .collect()
}

#[cfg(test)]
#[path = "anim_tests.rs"]
mod tests;
