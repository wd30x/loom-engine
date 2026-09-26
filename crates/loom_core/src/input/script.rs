use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{ActionMap, ActionSet, InputError};
use crate::time::TICK_HZ;

/// The only script format version so far.
const VERSION: u32 = 1;

/// An input script: per-tick action changes, used both for replay
/// (`--input`) and recording (`--record`). Records actions, not keys, so
/// rebinding keys does not invalidate recordings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputScript {
    pub version: u32,
    pub tick_hz: u32,
    pub events: Vec<InputEvent>,
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

impl InputScript {
    /// An empty script in the current version and tick rate.
    pub fn new(events: Vec<InputEvent>) -> Self {
        Self {
            version: VERSION,
            tick_hz: TICK_HZ,
            events,
        }
    }

    /// Parses and fully validates a script against `map`. Errors name the
    /// offending event as `events[i]` together with its tick; the caller adds
    /// the file path.
    pub fn parse(src: &str, map: &ActionMap) -> Result<Self, InputError> {
        let script: Self = ron::from_str(src)?;
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
        Ok(script)
    }

    /// Canonical text form: one event per line, `\n` line endings on every
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
    tick: u64,
    applied: usize,
    current: ActionSet,
}

impl ScriptPlayer {
    pub fn new(script: InputScript) -> Self {
        Self {
            events: script.events,
            tick: 0,
            applied: 0,
            current: ActionSet::new(),
        }
    }

    /// Advances to the next tick and returns its action set.
    pub fn next_tick(&mut self) -> ActionSet {
        self.tick += 1;
        while let Some(event) = self
            .events
            .get(self.applied)
            .filter(|e| e.tick <= self.tick)
        {
            if event.down {
                self.current.insert(event.action.clone());
            } else {
                self.current.remove(&event.action);
            }
            self.applied += 1;
        }
        self.current.clone()
    }

    /// Number of events in the script.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Number of events that took effect so far.
    pub fn applied(&self) -> usize {
        self.applied
    }
}

/// Records per-tick action sets as an input script.
#[derive(Debug, Clone, Default)]
pub struct InputRecorder {
    tick: u64,
    previous: ActionSet,
    events: Vec<InputEvent>,
}

impl InputRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call once per tick with that tick's action set.
    pub fn observe(&mut self, actions: &ActionSet) {
        self.tick += 1;
        let tick = self.tick;
        for action in actions.difference(&self.previous) {
            self.events.push(InputEvent {
                tick,
                action: action.clone(),
                down: true,
            });
        }
        for action in self.previous.difference(actions) {
            self.events.push(InputEvent {
                tick,
                action: action.clone(),
                down: false,
            });
        }
        self.previous.clone_from(actions);
    }

    /// The recording, events sorted by `(tick, action)`. Actions still held
    /// at the end get no release event.
    pub fn finish(mut self) -> InputScript {
        self.events
            .sort_by(|a, b| (a.tick, &a.action).cmp(&(b.tick, &b.action)));
        InputScript::new(self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ActionMap {
        ActionMap::parse(
            r#"[(action: "move_forward", keys: [KeyW]), (action: "move_right", keys: [KeyD]), (action: "jump", keys: [])]"#,
        )
        .unwrap()
    }

    fn script(events: &str) -> String {
        format!("(version: 1, tick_hz: 60, events: [{events}])")
    }

    /// Parses `events`, expects a failure, and checks that the message names
    /// the event index and tick.
    fn rejected(events: &str, index: usize, tick: u64) -> InputError {
        let err = InputScript::parse(&script(events), &map()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(&format!("events[{index}]")), "{msg}");
        assert!(msg.contains(&format!("tick {tick}")), "{msg}");
        err
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
    fn replay_applies_events_at_their_own_tick() {
        let parsed =
            InputScript::parse(&script(r#"(tick: 3, action: "jump", down: true)"#), &map())
                .unwrap();
        let mut player = ScriptPlayer::new(parsed);
        assert!(player.next_tick().is_empty());
        assert!(player.next_tick().is_empty());
        assert_eq!(player.applied(), 0);
        assert!(player.next_tick().contains("jump"));
        assert!(player.next_tick().contains("jump"));
        assert_eq!((player.applied(), player.event_count()), (1, 1));
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
        assert_eq!(recorder.finish(), InputScript::new(sorted));
    }

    #[test]
    fn recorded_script_serializes_to_the_canonical_bytes() {
        let mut recorder = InputRecorder::new();
        let forward_right = ActionSet::from(["move_forward".to_owned(), "move_right".to_owned()]);
        let forward = ActionSet::from(["move_forward".to_owned()]);
        for tick in 1..=40 {
            recorder.observe(if tick <= 30 { &forward_right } else { &forward });
        }
        let text = recorder.finish().to_ron();
        assert_eq!(
            text,
            "(\n    version: 1,\n    tick_hz: 60,\n    events: [\n        \
             (tick: 1, action: \"move_forward\", down: true),\n        \
             (tick: 1, action: \"move_right\", down: true),\n        \
             (tick: 31, action: \"move_right\", down: false),\n    ],\n)\n"
        );
        assert!(!text.contains('\r'));
        assert_eq!(InputScript::parse(&text, &map()).unwrap().events.len(), 3);
    }
}
