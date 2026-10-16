//! Animation clips of a scene's animated model nodes, and posing a
//! skeleton by a clip (`docs/architecture.md` section 19, "骨骼和动画片段").
//! Clip files are read and checked when the scene is loaded, whether or
//! not it is rendered; sampling is `loom_scene::Clip`'s.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use glam::{Mat4, Quat, Vec3};
use loom_core::debug::DebugDraw;
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

    /// The clip `animation` names `name`; [`Clips::load`] read it.
    fn clip(&self, animation: &Animation, name: &str) -> &Clip {
        let path = animation
            .clip_path(name)
            .expect("Scene::play_clip only plays declared clips");
        self.clips
            .get(path)
            .expect("Clips::load read every clip of the scene")
    }

    /// The model-space pose of `skeleton` by `animation` at tick `tau`
    /// ([`Animation::offsets`], [`pose`]).
    pub fn pose(&self, skeleton: &Skeleton, animation: &Animation, tau: u64) -> Vec<Mat4> {
        pose(skeleton, |bone| {
            animation.offsets(|name| self.clip(animation, name), bone, tau)
        })
    }
}

/// Reads and checks the clip file at `path`.
fn load_file(path: &Path) -> Result<Clip> {
    let src = std::fs::read_to_string(path).context("reading the file")?;
    Ok(Clip::parse(&src)?)
}

/// The model-space transform of every joint of `skeleton`, in its order,
/// with each named bone moved by the offset `offsets` gives it (rotation
/// `q`, move `d`; an unnamed joint keeps its bind pose). The bone gets the
/// local rotation `R * (B⁻¹ * q * B)` and the local translation
/// `T + B_p⁻¹ * d`, so with its parent still it turns by `q` about its own
/// position along the model's axes and moves by `d`.
pub(crate) fn pose(skeleton: &Skeleton, offsets: impl Fn(&str) -> (Quat, Vec3)) -> Vec<Mat4> {
    let mut model: Vec<Mat4> = Vec::with_capacity(skeleton.joints.len());
    for joint in &skeleton.joints {
        let (q, d) = joint
            .name
            .as_deref()
            .map_or((Quat::IDENTITY, Vec3::ZERO), &offsets);
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
}

/// Where a node hanging on joint `joint` of `skeleton` is placed, in model
/// space, when the skeleton is in `pose` ([`pose`]; `None`: the bind pose):
/// `A = translate(P_j's position) * rotate(P_j's rotation * P0_j's
/// rotation⁻¹)`, with `P0` the bind pose, so its offset is along the
/// model's axes (`docs/architecture.md` section 19, "挂在骨骼上").
pub(crate) fn anchor(skeleton: &Skeleton, pose: Option<&[Mat4]>, joint: usize) -> Mat4 {
    let bind = self::pose(skeleton, |_| (Quat::IDENTITY, Vec3::ZERO));
    let (_, bind_rotation, bind_at) = bind[joint].to_scale_rotation_translation();
    match pose {
        None => Mat4::from_translation(bind_at),
        Some(pose) => {
            let (_, rotation, at) = pose[joint].to_scale_rotation_translation();
            Mat4::from_rotation_translation(rotation * bind_rotation.inverse(), at)
        }
    }
}

/// The joint matrices of a pose ([`pose`]) of `skeleton`: each joint's
/// model-space transform times its inverse bind matrix.
pub(crate) fn joint_matrices(skeleton: &Skeleton, pose: &[Mat4]) -> Vec<Mat4> {
    pose.iter()
        .zip(&skeleton.joints)
        .map(|(m, j)| *m * j.inverse_bind)
        .collect()
}

/// The size of a joint's cross in the `skeleton` debug draw (each of its
/// three segments), meters.
const JOINT_CROSS: f32 = 0.03;
/// The `skeleton` debug draw's color.
const SKELETON_COLOR: Vec3 = Vec3::new(1.0, 0.8, 0.0);

/// The `skeleton` debug draw of a pose ([`pose`]) of `skeleton` placed by
/// `world` (`docs/architecture.md` section 19): a cross at every joint, a
/// segment from every joint to its parent joint.
pub(crate) fn draw_skeleton(skeleton: &Skeleton, pose: &[Mat4], world: Mat4, draw: &mut DebugDraw) {
    let at = |m: &Mat4| world.transform_point3(m.w_axis.truncate());
    for (joint, m) in skeleton.joints.iter().zip(pose) {
        draw.cross(at(m), JOINT_CROSS, SKELETON_COLOR);
        if let Some(p) = joint.parent {
            draw.line(at(&pose[p]), at(m), SKELETON_COLOR);
        }
    }
}

#[cfg(test)]
#[path = "anim_tests.rs"]
pub(crate) mod tests;
