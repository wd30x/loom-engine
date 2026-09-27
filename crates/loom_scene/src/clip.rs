//! Animation clips: the RON clip file format and sampling it by tick
//! (`docs/architecture.md` section 19, "骨骼和动画片段"). Pure functions of
//! the file and the tick; combining the bones into joint matrices is
//! `loom_engine`'s.

use std::collections::BTreeMap;

use glam::{Quat, Vec3};
use ron::extensions::Extensions;
use ron::ser::PrettyConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::euler_degrees;

/// A keyframe of a clip file: `(tick, (x, y, z))`.
pub type Keyframe = (u64, (f32, f32, f32));

/// A loom animation clip file (`docs/architecture.md` section 19). The
/// file is RON; this schema describes the same data as JSON: a keyframe
/// `(30, (25.0, 0.0, 0.0))` is a two-item array of the tick and a
/// three-item array; an optional field may be left out, and when given is
/// written bare in RON (`rotation: [...]`, not `rotation: Some([...])`).
/// Loading checks more than the structure: the rules are in the field
/// descriptions.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "clip")]
pub struct ClipFile {
    /// The clip's length in ticks (1/60 s), greater than 0.
    #[schemars(range(min = 1))]
    pub length: u64,
    /// `true`: the clip repeats, tick `length` being tick 0 again; `false`:
    /// it stops at its last tick.
    pub looping: bool,
    /// Bone name (a joint of the model's skin) to its channels; may be
    /// empty (the bind pose). Bones not listed keep their bind pose.
    pub bones: BTreeMap<String, BoneDesc>,
}

/// One bone's channels: at least one of the two. Values are offsets from
/// the bind pose along the model's axes (+X right, +Y up, -Z forward), not
/// the bone's own axes; a parent's motion carries its children's axes.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoneDesc {
    /// `(tick, (x, y, z))` keyframes of YXZ Euler angles in degrees, as a
    /// scene node's `rotation`, about the bone's position. At least one;
    /// ticks strictly increasing, each at most `length`; values finite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Vec<Keyframe>>,
    /// `(tick, (x, y, z))` keyframes of a move in meters. At least one;
    /// ticks strictly increasing, each at most `length`; values finite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translation: Option<Vec<Keyframe>>,
}

impl ClipFile {
    /// The file as RON: ron's pretty printer, 4-space indent, bare optional
    /// values, no struct names, `\n` line ends, a final newline (the scene
    /// files' canonical format).
    pub fn to_ron(&self) -> String {
        let config = PrettyConfig::new()
            .indentor("    ".to_owned())
            .new_line("\n".to_owned());
        let mut text = options()
            .to_string_pretty(self, config)
            .expect("clip files contain only plain data");
        text.push('\n');
        text
    }
}

fn options() -> ron::Options {
    ron::Options::default().with_default_extension(Extensions::IMPLICIT_SOME)
}

/// What is wrong with a clip file.
#[derive(Debug, thiserror::Error)]
pub enum ClipError {
    /// RON syntax error, unknown field, ... Includes the line and column.
    #[error(transparent)]
    Syntax(#[from] ron::error::SpannedError),
    #[error("length must be greater than 0")]
    Length,
    #[error("bone {0:?}: give rotation, translation or both")]
    NoChannel(String),
    #[error("bone {bone:?} {channel}: needs at least one keyframe")]
    NoKeyframe { bone: String, channel: &'static str },
    #[error("bone {bone:?} {channel}: keyframe tick {tick} is not after the one before it")]
    NotIncreasing {
        bone: String,
        channel: &'static str,
        tick: u64,
    },
    #[error("bone {bone:?} {channel}: keyframe tick {tick} is past the length {length}")]
    PastEnd {
        bone: String,
        channel: &'static str,
        tick: u64,
        length: u64,
    },
    #[error("bone {bone:?} {channel}: the keyframe at tick {tick} has a non-finite value")]
    NotFinite {
        bone: String,
        channel: &'static str,
        tick: u64,
    },
}

/// The JSON Schema (draft 2020-12) of a clip file, as `loom schema` writes
/// it under `clip` (`docs/architecture.md` section 17).
pub fn clip_schema() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<ClipFile>()
}

/// A checked clip.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    length: u64,
    looping: bool,
    bones: BTreeMap<String, Channels>,
}

/// One bone's keyframes; an absent channel is empty.
#[derive(Debug, Clone, PartialEq)]
struct Channels {
    /// Each Euler triple already a quaternion.
    rotation: Vec<(u64, Quat)>,
    translation: Vec<(u64, Vec3)>,
}

impl Clip {
    /// Parses and checks a clip file.
    pub fn parse(src: &str) -> Result<Self, ClipError> {
        let file: ClipFile = options().from_str(src)?;
        if file.length == 0 {
            return Err(ClipError::Length);
        }
        let mut bones = BTreeMap::new();
        for (bone, desc) in file.bones {
            if desc.rotation.is_none() && desc.translation.is_none() {
                return Err(ClipError::NoChannel(bone));
            }
            let rotation = keys(&bone, "rotation", desc.rotation, file.length)?;
            let translation = keys(&bone, "translation", desc.translation, file.length)?;
            let channels = Channels {
                rotation: rotation
                    .into_iter()
                    .map(|(t, d)| (t, euler_degrees(d)))
                    .collect(),
                translation,
            };
            bones.insert(bone, channels);
        }
        Ok(Self {
            length: file.length,
            looping: file.looping,
            bones,
        })
    }

    /// Length in ticks, greater than 0.
    pub fn length(&self) -> u64 {
        self.length
    }

    pub fn looping(&self) -> bool {
        self.looping
    }

    /// The bones the clip moves, by name, in name order.
    pub fn bones(&self) -> impl Iterator<Item = &str> {
        self.bones.keys().map(String::as_str)
    }

    /// The offset of `bone` from its bind pose `t` ticks after the clip
    /// started: a rotation and a move along the model's axes (identity
    /// and zero for a channel the bone does not have). `None` for a bone
    /// the clip does not list.
    pub fn sample(&self, bone: &str, t: u64) -> Option<(Quat, Vec3)> {
        let channels = self.bones.get(bone)?;
        let t = if self.looping {
            t % self.length
        } else {
            t.min(self.length)
        };
        let rotation = sample(&channels.rotation, t, Quat::IDENTITY, Quat::slerp);
        let translation = sample(&channels.translation, t, Vec3::ZERO, Vec3::lerp);
        Some((rotation, translation))
    }
}

/// The keyframes of one channel as vectors, checked: at least one, ticks
/// strictly increasing and at most `length`, values finite. Empty for an
/// absent channel.
fn keys(
    bone: &str,
    channel: &'static str,
    keys: Option<Vec<Keyframe>>,
    length: u64,
) -> Result<Vec<(u64, Vec3)>, ClipError> {
    let Some(keys) = keys else {
        return Ok(Vec::new());
    };
    if keys.is_empty() {
        return Err(ClipError::NoKeyframe {
            bone: bone.to_owned(),
            channel,
        });
    }
    let mut out: Vec<(u64, Vec3)> = Vec::with_capacity(keys.len());
    for (tick, value) in keys {
        let bone = bone.to_owned();
        if out.last().is_some_and(|&(before, _)| tick <= before) {
            return Err(ClipError::NotIncreasing {
                bone,
                channel,
                tick,
            });
        }
        if tick > length {
            return Err(ClipError::PastEnd {
                bone,
                channel,
                tick,
                length,
            });
        }
        let value = Vec3::from(value);
        if !value.is_finite() {
            return Err(ClipError::NotFinite {
                bone,
                channel,
                tick,
            });
        }
        out.push((tick, value));
    }
    Ok(out)
}

/// A channel's value at `t`: the first keyframe's before it, the last's
/// after it, `mix(a, b, s)` between two keyframes; `none` without
/// keyframes.
fn sample<T: Copy>(keys: &[(u64, T)], t: u64, none: T, mix: impl Fn(T, T, f32) -> T) -> T {
    let Some(&(first_tick, first)) = keys.first() else {
        return none;
    };
    if t <= first_tick {
        return first;
    }
    // The first keyframe after t; the one before it is at or before t.
    match keys.iter().position(|&(tick, _)| tick > t) {
        None => keys[keys.len() - 1].1,
        Some(i) => {
            let ((t0, a), (t1, b)) = (keys[i - 1], keys[i]);
            if t == t0 {
                // A keyframe's own value, exactly.
                return a;
            }
            let s = (t - t0) as f32 / (t1 - t0) as f32;
            mix(a, b, s)
        }
    }
}

#[cfg(test)]
#[path = "clip_tests.rs"]
mod tests;
