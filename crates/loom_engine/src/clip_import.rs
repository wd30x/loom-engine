//! `loom clip`: one glTF animation of a model, sampled at every tick and
//! written as a loom clip file (`docs/architecture.md` section 19, "glTF
//! 动画转片段 `loom clip`"). Offsets are the inverse of the ones clips give
//! (`crate::anim::pose`), and keyframes are thinned to the fewest a fixed
//! greedy pass allows within the precision below.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use glam::{EulerRot, Quat, Vec3};
use gltf::animation::util::ReadOutputs;
use gltf::animation::{Interpolation, Property};
use loom_scene::{BoneDesc, ClipFile, Keyframe, euler_degrees};
use serde::Serialize;

use crate::model::{buffer_data, import};
use crate::output::ensure_parent_dir;

/// Degrees a thinned rotation may be off at any tick.
const ROTATION_TOLERANCE: f32 = 0.1;
/// Meters a thinned translation may be off at any tick.
const TRANSLATION_TOLERANCE: f32 = 0.0005;
/// Allowed difference of an animated scale from the bind pose's.
const SCALE_TOLERANCE: f32 = 1e-4;
/// Ticks a second.
const TICKS_PER_SECOND: f32 = 60.0;

/// `loom clip`'s report.
#[derive(Debug, Serialize)]
pub(crate) struct ClipReport {
    pub status: &'static str,
    pub path: String,
    pub animation: String,
    pub out: String,
    pub length: u64,
    /// Bones written.
    pub bones: usize,
    /// Keyframes written, over every channel.
    pub keyframes: usize,
}

/// `loom clip <path> --animation <name> --out <out> [--looping]`.
pub(crate) fn convert(
    path: &Path,
    animation: &str,
    out: &Path,
    looping: bool,
) -> Result<ClipReport> {
    let shown = path.display().to_string();
    let bytes = std::fs::read(path)
        .context("reading the file")
        .with_context(|| format!("model {shown}"))?;
    let file = to_clip(&bytes, animation, looping).with_context(|| format!("model {shown}"))?;
    let text = format!(
        "// Converted by `loom clip` from {shown}, animation {animation:?}.\n{}",
        file.to_ron()
    );
    ensure_parent_dir(out)?;
    std::fs::write(out, text).with_context(|| format!("writing clip {}", out.display()))?;
    Ok(ClipReport {
        status: "ok",
        path: shown,
        animation: animation.to_owned(),
        out: out.display().to_string(),
        length: file.length,
        bones: file.bones.len(),
        keyframes: file
            .bones
            .values()
            .map(|b| {
                b.rotation
                    .iter()
                    .chain(&b.translation)
                    .map(Vec::len)
                    .sum::<usize>()
            })
            .sum(),
    })
}

/// One channel of the animation: a property of a joint (by skeleton
/// index), keyframed.
struct Channel {
    joint: usize,
    property: Property,
    interpolation: Interpolation,
    times: Vec<f32>,
    /// Three or four components; rotations are `[x, y, z, w]`.
    values: Vec<[f32; 4]>,
}

impl Channel {
    /// The value at `t` seconds by glTF's rules: held before the first and
    /// after the last keyframe; STEP holds each keyframe until the next,
    /// LINEAR interpolates (rotations along the shortest arc).
    fn sample(&self, t: f32) -> [f32; 4] {
        let last = self.times.len() - 1;
        if t <= self.times[0] {
            return self.values[0];
        }
        if t >= self.times[last] {
            return self.values[last];
        }
        let i = self.times.iter().rposition(|&k| k <= t).unwrap_or(0);
        let (a, b) = (self.values[i], self.values[i + 1]);
        if self.interpolation == Interpolation::Step {
            return a;
        }
        let s = (t - self.times[i]) / (self.times[i + 1] - self.times[i]);
        if self.property == Property::Rotation {
            Quat::from_array(a).slerp(Quat::from_array(b), s).to_array()
        } else {
            let (a, b) = (glam::Vec4::from_array(a), glam::Vec4::from_array(b));
            a.lerp(b, s).to_array()
        }
    }
}

/// The clip of the animation `name` of the `.glb` in `bytes`.
pub(crate) fn to_clip(bytes: &[u8], name: &str, looping: bool) -> Result<ClipFile> {
    let imported = import(bytes)?;
    let skinned = match imported.skinned {
        Ok(skinned) => skinned,
        Err(why) => bail!("{why}"),
    };
    let skeleton = &skinned.skeleton;
    let gltf = gltf::Gltf::from_slice(bytes).context("not a valid glTF binary")?;
    let blob = gltf.blob.as_deref();
    let Some(animation) = gltf.animations().find(|a| a.name() == Some(name)) else {
        let names: Vec<String> = gltf
            .animations()
            .filter_map(|a| a.name().map(|n| format!("{n:?}")))
            .collect();
        bail!(
            "the model has no animation {name:?}: it has {}",
            list(&names)
        );
    };

    let mut channels = Vec::new();
    for c in animation.channels() {
        let label = || format!("animation {name:?} channel {}", c.index());
        let node = c.target().node();
        let Some(joint) = skeleton.joints.iter().position(|j| j.node == node.index()) else {
            let named = node.name().map(|n| format!(" ({n:?})")).unwrap_or_default();
            bail!(
                "{} moves node {}{named}, which is not a joint of the skin",
                label(),
                node.index()
            );
        };
        if skeleton.joints[joint].name.is_none() {
            bail!(
                "{} moves joint node {}, which has no name",
                label(),
                node.index()
            );
        }
        let interpolation = c.sampler().interpolation();
        if interpolation == Interpolation::CubicSpline {
            bail!(
                "{} uses CUBICSPLINE interpolation, which loom does not support",
                label()
            );
        }
        let reader = c.reader(|b| buffer_data(b, blob));
        let times: Vec<f32> = reader
            .read_inputs()
            .with_context(|| format!("{} has no keyframe times", label()))?
            .collect();
        let values: Vec<[f32; 4]> = match reader
            .read_outputs()
            .with_context(|| format!("{} has no keyframe values", label()))?
        {
            ReadOutputs::Translations(v) => v.map(|[x, y, z]| [x, y, z, 0.0]).collect(),
            ReadOutputs::Scales(v) => v.map(|[x, y, z]| [x, y, z, 0.0]).collect(),
            ReadOutputs::Rotations(v) => v.into_f32().collect(),
            ReadOutputs::MorphTargetWeights(_) => {
                bail!(
                    "{} animates morph target weights, which loom does not support",
                    label()
                )
            }
        };
        if times.is_empty() || times.len() != values.len() {
            bail!(
                "{} has {} keyframe times for {} values",
                label(),
                times.len(),
                values.len()
            );
        }
        channels.push(Channel {
            joint,
            property: c.target().property(),
            interpolation,
            times,
            values,
        });
    }

    let seconds = channels
        .iter()
        .flat_map(|c| c.times.last())
        .fold(0.0_f32, |a, &b| a.max(b));
    // ceil, forgiving float noise such as 2.0000002 s.
    let length = ((f64::from(seconds) * 60.0 - 1e-3).ceil().max(1.0)) as u64;

    // Each moved joint's offsets at every tick.
    let moved: BTreeMap<usize, (bool, bool)> = channels.iter().fold(BTreeMap::new(), |mut m, c| {
        let entry = m.entry(c.joint).or_insert((false, false));
        match c.property {
            Property::Rotation => entry.0 = true,
            Property::Translation => entry.1 = true,
            _ => {}
        }
        m
    });
    let mut series: BTreeMap<usize, (Vec<Quat>, Vec<Vec3>)> = BTreeMap::new();
    for k in 0..=length {
        let t = k as f32 / TICKS_PER_SECOND;
        for &j in moved.keys() {
            let joint = &skeleton.joints[j];
            let (mut r, mut tr, mut sc) = (joint.rotation, joint.translation, joint.scale);
            for c in channels.iter().filter(|c| c.joint == j) {
                let v = c.sample(t);
                match c.property {
                    Property::Rotation => r = Quat::from_array(v).normalize(),
                    Property::Translation => tr = Vec3::new(v[0], v[1], v[2]),
                    Property::Scale => sc = Vec3::new(v[0], v[1], v[2]),
                    Property::MorphTargetWeights => {}
                }
            }
            if (sc - joint.scale).abs().max_element() > SCALE_TOLERANCE {
                bail!(
                    "animation {name:?} scales joint {:?} at tick {k}, but clips have no scale",
                    joint.name.as_deref().unwrap_or_default()
                );
            }
            let b = joint.bind_rotation;
            let q = b * joint.rotation.inverse() * r * b.inverse();
            let d = joint.parent_bind_rotation * (tr - joint.translation);
            let entry = series.entry(j).or_default();
            entry.0.push(q);
            entry.1.push(d);
        }
    }

    let mut bones = BTreeMap::new();
    for (j, (rotations, translations)) in series {
        let (has_rotation, has_translation) = moved[&j];
        let rotation = (has_rotation
            && rotations
                .iter()
                .any(|q| angle(*q, Quat::IDENTITY) > ROTATION_TOLERANCE))
        .then(|| thin_rotations(&rotations));
        let translation = (has_translation
            && translations
                .iter()
                .any(|d| d.length() > TRANSLATION_TOLERANCE))
        .then(|| thin_translations(&translations));
        if rotation.is_none() && translation.is_none() {
            continue;
        }
        let bone = skeleton.joints[j].name.clone().expect("checked above");
        bones.insert(
            bone,
            BoneDesc {
                rotation,
                translation,
            },
        );
    }
    Ok(ClipFile {
        length,
        looping,
        bones,
    })
}

/// `"a", "b" and "c"`; `none` for none.
fn list(names: &[String]) -> String {
    match names {
        [] => "none".to_owned(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// Degrees between two rotations.
fn angle(a: Quat, b: Quat) -> f32 {
    2.0 * a.dot(b).abs().min(1.0).acos().to_degrees()
}

/// `v` rounded to `places` decimals, without a negative zero.
fn round(v: f32, places: i32) -> f32 {
    let scale = 10f64.powi(places);
    let r = (f64::from(v) * scale).round() / scale;
    if r == 0.0 { 0.0 } else { r as f32 }
}

/// A rotation as the clip file writes it: YXZ degrees, 2 decimals.
fn euler_of(q: Quat) -> (f32, f32, f32) {
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    (
        round(x.to_degrees(), 2),
        round(y.to_degrees(), 2),
        round(z.to_degrees(), 2),
    )
}

/// The ticks the greedy pass keeps out of `0..=last`: from each kept tick,
/// the furthest next one for which `fits(from, to)` holds (the ticks
/// between interpolate close enough), at least the one after it.
fn thin(last: usize, fits: impl Fn(usize, usize) -> bool) -> Vec<usize> {
    let mut kept = vec![0];
    let mut i = 0;
    while i < last {
        let mut j = i + 1;
        while j < last && fits(i, j + 1) {
            j += 1;
        }
        kept.push(j);
        i = j;
    }
    kept
}

/// The fraction of the way tick `m` is from `i` to `j`, as clips sample.
fn fraction(i: usize, j: usize, m: usize) -> f32 {
    (m - i) as f32 / (j - i) as f32
}

fn thin_rotations(q: &[Quat]) -> Vec<Keyframe> {
    let written: Vec<(f32, f32, f32)> = q.iter().map(|&q| euler_of(q)).collect();
    let back: Vec<Quat> = written.iter().map(|&e| euler_degrees(e.into())).collect();
    let fits = |i: usize, j: usize| {
        (i + 1..j)
            .all(|m| angle(back[i].slerp(back[j], fraction(i, j, m)), q[m]) <= ROTATION_TOLERANCE)
    };
    thin(q.len() - 1, fits)
        .into_iter()
        .map(|k| (k as u64, written[k]))
        .collect()
}

fn thin_translations(d: &[Vec3]) -> Vec<Keyframe> {
    let written: Vec<(f32, f32, f32)> = d
        .iter()
        .map(|v| (round(v.x, 4), round(v.y, 4), round(v.z, 4)))
        .collect();
    let back: Vec<Vec3> = written.iter().map(|&w| w.into()).collect();
    let fits = |i: usize, j: usize| {
        (i + 1..j).all(|m| {
            back[i].lerp(back[j], fraction(i, j, m)).distance(d[m]) <= TRANSLATION_TOLERANCE
        })
    };
    thin(d.len() - 1, fits)
        .into_iter()
        .map(|k| (k as u64, written[k]))
        .collect()
}

#[cfg(test)]
#[path = "clip_import_tests.rs"]
mod tests;
