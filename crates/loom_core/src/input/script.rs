use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{ActionMap, InputError, TickInput, canonical_axis_value};
use crate::time::TICK_HZ;

/// The only script format version so far.
const VERSION: u32 = 1;

/// An input script: per-tick action and axis changes, used both for replay
/// (`--input`) and recording (`--record`). Records actions and axes, not
/// keys, so rebinding keys does not invalidate recordings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputScript {
    pub version: u32,
    pub tick_hz: u32,
    pub events: Vec<InputEvent>,
    /// Optional when reading (older scripts have none); always written.
    #[serde(default)]
    pub axes: Vec<AxisEvent>,
}

/// `down: true` presses `action`, `down: false` releases it, starting with
/// tick `tick` (1-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputEvent {
    pub tick: u64,
    pub action: String,
    pub down: bool,
}

/// Sets `axis` to `value` from tick `tick` (1-based) until the axis's next
/// event (level semantics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisEvent {
    pub tick: u64,
    pub axis: String,
    pub value: f32,
}

impl InputScript {
    /// A script in the current version and tick rate.
    pub fn new(events: Vec<InputEvent>, axes: Vec<AxisEvent>) -> Self {
        Self {
            version: VERSION,
            tick_hz: TICK_HZ,
            events,
            axes,
        }
    }

    /// Parses and fully validates a script against `map`. Errors name the
    /// offending event as `events[i]` or `axes[i]` together with its tick;
    /// the caller adds the file path. Axis values of `-0.0` become `0.0`.
    pub fn parse(src: &str, map: &ActionMap) -> Result<Self, InputError> {
        let mut script: Self = ron::from_str(src)?;
        if script.version != VERSION {
            return Err(InputError::UnsupportedVersion(script.version));
        }
        if script.tick_hz != TICK_HZ {
            return Err(InputError::TickRateMismatch {
                found: script.tick_hz,
                expected: TICK_HZ,
            });
        }

        let mut held = BTreeSet::new();
        let mut in_tick = BTreeSet::new();
        let mut previous = 0;
        for (index, InputEvent { tick, action, down }) in script.events.iter().enumerate() {
            let (tick, action) = (*tick, action.clone());
            if tick == 0 {
                return Err(InputError::TickZero { index, tick });
            }
            if tick < previous {
                return Err(InputError::DecreasingTick {
                    index,
                    tick,
                    previous,
                });
            }
            if tick != previous {
                in_tick.clear();
                previous = tick;
            }
            if !map.contains(&action) {
                return Err(InputError::UnknownAction {
                    index,
                    tick,
                    action,
                });
            }
            if !in_tick.insert(action.clone()) {
                return Err(InputError::RepeatedInTick {
                    index,
                    tick,
                    action,
                });
            }
            if *down && !held.insert(action.clone()) {
                return Err(InputError::PressWhileHeld {
                    index,
                    tick,
                    action,
                });
            }
            if !*down && !held.remove(&action) {
                return Err(InputError::ReleaseWhileIdle {
                    index,
                    tick,
                    action,
                });
            }
        }

        let mut values: BTreeMap<String, f32> = BTreeMap::new();
        let mut in_tick = BTreeSet::new();
        let mut previous = 0;
        for (index, event) in script.axes.iter_mut().enumerate() {
            event.value = canonical_axis_value(event.value);
            let (tick, axis, value) = (event.tick, event.axis.clone(), event.value);
            if tick == 0 {
                return Err(InputError::AxisTickZero { index, tick });
            }
            if tick < previous {
                return Err(InputError::AxisDecreasingTick {
                    index,
                    tick,
                    previous,
                });
            }
            if tick != previous {
                in_tick.clear();
                previous = tick;
            }
            if !map.contains_axis(&axis) {
                return Err(InputError::UnknownAxis { index, tick, axis });
            }
            if !in_tick.insert(axis.clone()) {
                return Err(InputError::AxisRepeatedInTick { index, tick, axis });
            }
            if !value.is_finite() {
                return Err(InputError::NonFiniteAxisValue { index, tick, axis });
            }
            if value == values.get(&axis).copied().unwrap_or(0.0) {
                return Err(InputError::AxisUnchanged {
                    index,
                    tick,
                    axis,
                    value,
                });
            }
            values.insert(axis, value);
        }
        Ok(script)
    }

    /// Canonical text form: one event per line, `axes` after `events`, `\n` line endings on every
    /// platform, trailing newline.
    pub fn to_ron(&self) -> String {
        let config = ron::ser::PrettyConfig::new().depth_limit(2).new_line("\n");
        let mut text =
            ron::ser::to_string_pretty(self, config).expect("scripts contain only plain data");
        text.push('\n');
        text
    }
}

/// Replays a validated script, one tick at a time.
#[derive(Debug, Clone)]
pub struct ScriptPlayer {
    events: Vec<InputEvent>,
    axes: Vec<AxisEvent>,
    tick: u64,
    /// Entries of `events` and of `axes` that took effect so far.
    applied_events: usize,
    applied_axes: usize,
    current: TickInput,
}

impl ScriptPlayer {
    pub fn new(script: InputScript) -> Self {
        Self {
            events: script.events,
            axes: script.axes,
            tick: 0,
            applied_events: 0,
            applied_axes: 0,
            current: TickInput::default(),
        }
    }

    /// Advances to the next tick and returns its input.
    pub fn next_tick(&mut self) -> TickInput {
        self.tick += 1;
        while let Some(event) = self
            .events
            .get(self.applied_events)
            .filter(|e| e.tick <= self.tick)
        {
            if event.down {
                self.current.actions.insert(event.action.clone());
            } else {
                self.current.actions.remove(&event.action);
            }
            self.applied_events += 1;
        }
        while let Some(event) = self
            .axes
            .get(self.applied_axes)
            .filter(|e| e.tick <= self.tick)
        {
            // Values are canonical (see `parse`), so zero means "no input".
            if event.value == 0.0 {
                self.current.axes.remove(&event.axis);
            } else {
                self.current.axes.insert(event.axis.clone(), event.value);
            }
            self.applied_axes += 1;
        }
        self.current.clone()
    }

    /// Number of events in the script, actions and axes together.
    pub fn event_count(&self) -> usize {
        self.events.len() + self.axes.len()
    }

    /// Number of events (actions and axes) that took effect so far.
    pub fn applied(&self) -> usize {
        self.applied_events + self.applied_axes
    }
}

/// Records per-tick input as an input script.
#[derive(Debug, Clone, Default)]
pub struct InputRecorder {
    tick: u64,
    previous: TickInput,
    events: Vec<InputEvent>,
    axes: Vec<AxisEvent>,
}

impl InputRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call once per tick with that tick's input.
    pub fn observe(&mut self, input: &TickInput) {
        self.tick += 1;
        let tick = self.tick;
        let (actions, previous) = (&input.actions, &self.previous.actions);
        for action in actions.difference(previous) {
            self.events.push(InputEvent {
                tick,
                action: action.clone(),
                down: true,
            });
        }
        for action in previous.difference(actions) {
            self.events.push(InputEvent {
                tick,
                action: action.clone(),
                down: false,
            });
        }
        let changed: BTreeSet<&String> = input
            .axes
            .keys()
            .chain(self.previous.axes.keys())
            .filter(|axis| input.axis(axis) != self.previous.axis(axis))
            .collect();
        for axis in changed {
            self.axes.push(AxisEvent {
                tick,
                axis: axis.clone(),
                value: input.axis(axis),
            });
        }
        self.previous.clone_from(input);
    }

    /// The recording, events sorted by `(tick, action)` and axis events by
    /// `(tick, axis)`. Actions still held and axes still non-zero at the end
    /// get no closing event.
    pub fn finish(mut self) -> InputScript {
        self.events
            .sort_by(|a, b| (a.tick, &a.action).cmp(&(b.tick, &b.action)));
        self.axes
            .sort_by(|a, b| (a.tick, &a.axis).cmp(&(b.tick, &b.axis)));
        InputScript::new(self.events, self.axes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ActionSet;

    fn map() -> ActionMap {
        ActionMap::parse(
            r#"(
                actions: [(action: "move_forward", keys: [KeyW]), (action: "move_right", keys: [KeyD]), (action: "jump", keys: [])],
                axes: [
                    (axis: "move_x", keys: [(KeyD, 1.0)]),
                    (axis: "move_y", keys: [(KeyW, 1.0)]),
                    (axis: "look_x", keys: [], mouse: X, mouse_scale: 0.0025),
                ],
            )"#,
        )
        .unwrap()
    }

    fn script(events: &str) -> String {
        format!("(version: 1, tick_hz: 60, events: [{events}])")
    }

    fn axis_script(axes: &str) -> String {
        format!("(version: 1, tick_hz: 60, events: [], axes: [{axes}])")
    }

    fn actions(names: &[&str]) -> TickInput {
        TickInput {
            actions: names.iter().map(|a| (*a).to_owned()).collect::<ActionSet>(),
            axes: BTreeMap::new(),
        }
    }

    fn axes(values: &[(&str, f32)]) -> TickInput {
        TickInput {
            actions: ActionSet::new(),
            axes: values.iter().map(|(a, v)| ((*a).to_owned(), *v)).collect(),
        }
    }

    /// Parses `src`, expects a failure, and checks that the message names
    /// the entry as `{list}[{index}]` and its tick.
    fn rejected_in(src: &str, list: &str, index: usize, tick: u64) -> InputError {
        let err = InputScript::parse(src, &map()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&format!("{list}[{index}]")), "{msg}");
        assert!(msg.contains(&format!("tick {tick}")), "{msg}");
        err
    }

    fn rejected(events: &str, index: usize, tick: u64) -> InputError {
        rejected_in(&script(events), "events", index, tick)
    }

    fn rejected_axis(axes: &str, index: usize, tick: u64) -> InputError {
        rejected_in(&axis_script(axes), "axes", index, tick)
    }

    #[test]
    fn script_accepts_comments_names_and_trailing_commas() {
        let src = "// a hand-written script\nInputScript(version: 1, tick_hz: 60, events: [\n  (tick: 1, action: \"jump\", down: true), // press\n])";
        let parsed = InputScript::parse(src, &map()).unwrap();
        assert_eq!(parsed.events.len(), 1);
    }

    #[test]
    fn script_rejects_unsupported_version() {
        let src = "(version: 2, tick_hz: 60, events: [])";
        let err = InputScript::parse(src, &map()).unwrap_err();
        assert!(matches!(err, InputError::UnsupportedVersion(2)), "{err}");
    }

    #[test]
    fn script_rejects_mismatched_tick_rate() {
        let src = "(version: 1, tick_hz: 30, events: [])";
        let err = InputScript::parse(src, &map()).unwrap_err();
        assert!(
            matches!(
                err,
                InputError::TickRateMismatch {
                    found: 30,
                    expected: 60
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn script_rejects_tick_zero() {
        let err = rejected(r#"(tick: 0, action: "jump", down: true)"#, 0, 0);
        assert!(matches!(err, InputError::TickZero { .. }), "{err}");
    }

    #[test]
    fn script_rejects_decreasing_ticks() {
        let err = rejected(
            r#"(tick: 5, action: "jump", down: true), (tick: 4, action: "move_right", down: true)"#,
            1,
            4,
        );
        assert!(
            matches!(err, InputError::DecreasingTick { previous: 5, .. }),
            "{err}"
        );
    }

    #[test]
    fn script_rejects_one_action_twice_in_a_tick() {
        let err = rejected(
            r#"(tick: 3, action: "jump", down: true), (tick: 3, action: "jump", down: false)"#,
            1,
            3,
        );
        assert!(matches!(err, InputError::RepeatedInTick { .. }), "{err}");
    }

    #[test]
    fn script_rejects_an_unknown_action() {
        let err = rejected(
            r#"(tick: 1, action: "jump", down: true), (tick: 2, action: "fly", down: true)"#,
            1,
            2,
        );
        assert!(matches!(err, InputError::UnknownAction { .. }), "{err}");
    }

    #[test]
    fn script_rejects_pressing_a_held_action() {
        let err = rejected(
            r#"(tick: 1, action: "jump", down: true), (tick: 7, action: "jump", down: true)"#,
            1,
            7,
        );
        assert!(matches!(err, InputError::PressWhileHeld { .. }), "{err}");
    }

    #[test]
    fn script_rejects_releasing_an_idle_action() {
        let err = rejected(
            r#"(tick: 1, action: "jump", down: true), (tick: 9, action: "move_right", down: false)"#,
            1,
            9,
        );
        assert!(matches!(err, InputError::ReleaseWhileIdle { .. }), "{err}");
    }

    #[test]
    fn script_rejects_an_unknown_field() {
        let src = script(r#"(tik: 1, action: "jump", down: true)"#);
        let err = InputScript::parse(&src, &map()).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
        assert!(err.to_string().contains("tik"), "{err}");
    }

    #[test]
    fn script_without_an_axes_field_has_no_axis_events() {
        let parsed =
            InputScript::parse(&script(r#"(tick: 1, action: "jump", down: true)"#), &map())
                .unwrap();
        assert!(parsed.axes.is_empty());
    }

    #[test]
    fn axis_event_at_tick_zero_is_rejected() {
        let err = rejected_axis(r#"(tick: 0, axis: "move_x", value: 1.0)"#, 0, 0);
        assert!(matches!(err, InputError::AxisTickZero { .. }), "{err}");
    }

    #[test]
    fn axis_events_with_decreasing_ticks_are_rejected() {
        let err = rejected_axis(
            r#"(tick: 5, axis: "move_x", value: 1.0), (tick: 4, axis: "move_y", value: 1.0)"#,
            1,
            4,
        );
        assert!(
            matches!(err, InputError::AxisDecreasingTick { previous: 5, .. }),
            "{err}"
        );
    }

    #[test]
    fn axis_ticks_are_checked_apart_from_action_ticks() {
        // Each list is non-decreasing on its own; together they are not.
        let src = "(version: 1, tick_hz: 60, \
            events: [(tick: 9, action: \"jump\", down: true)], \
            axes: [(tick: 2, axis: \"move_x\", value: 1.0)])";
        assert!(InputScript::parse(src, &map()).is_ok());
    }

    #[test]
    fn one_axis_twice_in_a_tick_is_rejected() {
        let err = rejected_axis(
            r#"(tick: 3, axis: "move_x", value: 1.0), (tick: 3, axis: "move_y", value: 1.0), (tick: 3, axis: "move_x", value: 0.5)"#,
            2,
            3,
        );
        assert!(
            matches!(err, InputError::AxisRepeatedInTick { .. }),
            "{err}"
        );
    }

    #[test]
    fn unknown_axis_is_rejected() {
        let err = rejected_axis(
            r#"(tick: 1, axis: "move_x", value: 1.0), (tick: 2, axis: "jump", value: 1.0)"#,
            1,
            2,
        );
        assert!(matches!(err, InputError::UnknownAxis { .. }), "{err}");
    }

    #[test]
    fn axis_value_that_is_not_finite_is_rejected() {
        for value in ["inf", "-inf", "NaN"] {
            let err = rejected_axis(
                &format!(
                    r#"(tick: 1, axis: "move_x", value: 1.0), (tick: 6, axis: "move_y", value: {value})"#
                ),
                1,
                6,
            );
            assert!(
                matches!(err, InputError::NonFiniteAxisValue { .. }),
                "{value}: {err}"
            );
        }
    }

    #[test]
    fn setting_an_axis_to_its_current_value_is_rejected() {
        let err = rejected_axis(r#"(tick: 4, axis: "move_x", value: 0.0)"#, 0, 4);
        assert!(matches!(err, InputError::AxisUnchanged { .. }), "{err}");
        let err = rejected_axis(
            r#"(tick: 1, axis: "move_x", value: 0.5), (tick: 8, axis: "move_x", value: 0.5)"#,
            1,
            8,
        );
        assert!(matches!(err, InputError::AxisUnchanged { .. }), "{err}");
    }

    #[test]
    fn axis_event_with_an_unknown_field_is_rejected() {
        let src = axis_script(r#"(tick: 1, axis: "move_x", valeu: 1.0)"#);
        let err = InputScript::parse(&src, &map()).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
    }

    #[test]
    fn negative_zero_in_a_script_becomes_positive_zero() {
        let src = axis_script(
            r#"(tick: 1, axis: "move_x", value: 1), (tick: 2, axis: "move_x", value: -0.0)"#,
        );
        let parsed = InputScript::parse(&src, &map()).unwrap();
        assert_eq!(parsed.axes[0].value.to_bits(), 1.0f32.to_bits());
        assert_eq!(parsed.axes[1].value.to_bits(), 0.0f32.to_bits());
        // And `-0.0` as the first value of an axis is "set to its current value".
        let err = rejected_axis(r#"(tick: 1, axis: "move_x", value: -0.0)"#, 0, 1);
        assert!(matches!(err, InputError::AxisUnchanged { .. }), "{err}");
    }

    #[test]
    fn replay_applies_events_at_their_own_tick() {
        let parsed =
            InputScript::parse(&script(r#"(tick: 3, action: "jump", down: true)"#), &map())
                .unwrap();
        let mut player = ScriptPlayer::new(parsed);
        assert!(player.next_tick().actions.is_empty());
        assert!(player.next_tick().actions.is_empty());
        assert_eq!(player.applied(), 0);
        assert!(player.next_tick().actions.contains("jump"));
        assert!(player.next_tick().actions.contains("jump"));
        assert_eq!((player.applied(), player.event_count()), (1, 1));
    }

    #[test]
    fn replayed_axis_keeps_its_value_until_its_next_event() {
        let src = "(version: 1, tick_hz: 60, \
            events: [(tick: 2, action: \"jump\", down: true)], \
            axes: [(tick: 2, axis: \"move_x\", value: 0.5), (tick: 4, axis: \"move_x\", value: -1.0), \
                   (tick: 5, axis: \"look_x\", value: 0.25), (tick: 6, axis: \"move_x\", value: 0.0)])";
        let mut player = ScriptPlayer::new(InputScript::parse(src, &map()).unwrap());
        assert_eq!(player.event_count(), 5);
        let seen: Vec<(f32, f32)> = (0..7)
            .map(|_| {
                let input = player.next_tick();
                (input.axis("move_x"), input.axis("look_x"))
            })
            .collect();
        assert_eq!(
            seen,
            [
                (0.0, 0.0),
                (0.5, 0.0),
                (0.5, 0.0),
                (-1.0, 0.0),
                (-1.0, 0.25),
                (0.0, 0.25),
                (0.0, 0.25),
            ]
        );
        assert_eq!(player.applied(), 5);
    }

    #[test]
    fn recording_a_replay_yields_the_sorted_script() {
        let unsorted = script(concat!(
            r#"(tick: 1, action: "move_right", down: true),"#,
            r#"(tick: 1, action: "move_forward", down: true),"#,
            r#"(tick: 1, action: "jump", down: true),"#,
            r#"(tick: 4, action: "move_right", down: false),"#,
            r#"(tick: 4, action: "jump", down: false),"#,
            r#"(tick: 6, action: "move_forward", down: false),"#,
        ));
        let parsed = InputScript::parse(&unsorted, &map()).unwrap();
        let mut sorted = parsed.events.clone();
        sorted.sort_by(|a, b| (a.tick, &a.action).cmp(&(b.tick, &b.action)));

        let mut player = ScriptPlayer::new(parsed);
        let mut recorder = InputRecorder::new();
        for _ in 0..10 {
            recorder.observe(&player.next_tick());
        }
        assert_eq!(recorder.finish(), InputScript::new(sorted, Vec::new()));
    }

    #[test]
    fn recording_an_axis_replay_yields_the_sorted_axes() {
        let unsorted = axis_script(concat!(
            r#"(tick: 1, axis: "move_y", value: 0.5),"#,
            r#"(tick: 1, axis: "move_x", value: 1.0),"#,
            r#"(tick: 20, axis: "look_x", value: 0.0175),"#,
            r#"(tick: 21, axis: "look_x", value: 0.0),"#,
            r#"(tick: 46, axis: "move_y", value: 0.0),"#,
            r#"(tick: 46, axis: "move_x", value: 0.0),"#,
        ));
        let parsed = InputScript::parse(&unsorted, &map()).unwrap();
        let mut sorted = parsed.axes.clone();
        sorted.sort_by(|a, b| (a.tick, &a.axis).cmp(&(b.tick, &b.axis)));
        assert_ne!(sorted, parsed.axes);

        let mut player = ScriptPlayer::new(parsed);
        let mut recorder = InputRecorder::new();
        for _ in 0..60 {
            recorder.observe(&player.next_tick());
        }
        assert_eq!(recorder.finish(), InputScript::new(Vec::new(), sorted));
    }

    #[test]
    fn recorded_script_serializes_to_the_canonical_bytes() {
        let mut recorder = InputRecorder::new();
        let forward_right = actions(&["move_forward", "move_right"]);
        let forward = actions(&["move_forward"]);
        for tick in 1..=40 {
            recorder.observe(if tick <= 30 { &forward_right } else { &forward });
        }
        let text = recorder.finish().to_ron();
        assert_eq!(
            text,
            "(\n    version: 1,\n    tick_hz: 60,\n    events: [\n        \
             (tick: 1, action: \"move_forward\", down: true),\n        \
             (tick: 1, action: \"move_right\", down: true),\n        \
             (tick: 31, action: \"move_right\", down: false),\n    ],\n    axes: [],\n)\n"
        );
        assert!(!text.contains('\r'));
        assert_eq!(InputScript::parse(&text, &map()).unwrap().events.len(), 3);
    }

    #[test]
    fn recorded_axes_serialize_to_the_canonical_bytes() {
        let mut recorder = InputRecorder::new();
        for tick in 1..=50 {
            let mut values = Vec::new();
            if tick <= 45 {
                values.extend([("move_x", 1.0), ("move_y", 0.5)]);
            }
            if tick == 20 {
                values.push(("look_x", 0.0175));
            }
            recorder.observe(&axes(&values));
        }
        let text = recorder.finish().to_ron();
        assert_eq!(
            text,
            concat!(
                "(\n",
                "    version: 1,\n",
                "    tick_hz: 60,\n",
                "    events: [],\n",
                "    axes: [\n",
                "        (tick: 1, axis: \"move_x\", value: 1.0),\n",
                "        (tick: 1, axis: \"move_y\", value: 0.5),\n",
                "        (tick: 20, axis: \"look_x\", value: 0.0175),\n",
                "        (tick: 21, axis: \"look_x\", value: 0.0),\n",
                "        (tick: 46, axis: \"move_x\", value: 0.0),\n",
                "        (tick: 46, axis: \"move_y\", value: 0.0),\n",
                "    ],\n",
                ")\n",
            )
        );
        let reparsed = InputScript::parse(&text, &map()).unwrap();
        assert_eq!(reparsed.axes[2].value.to_bits(), 0.0175f32.to_bits());
    }
}
