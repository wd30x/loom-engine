use std::collections::BTreeSet;

use super::{ActionMap, ActionSet, Key};

/// Live keyboard input, sampled once per tick.
///
/// An action is active for a tick if one of its keys is held at sampling
/// time, or was pressed since the previous sample. A tap that starts and
/// ends between two samples is therefore active for exactly one tick, no
/// matter how many frames or ticks passed in between.
#[derive(Debug, Clone, Default)]
pub struct InputCollector {
    held: BTreeSet<Key>,
    pressed_since_sample: BTreeSet<Key>,
}

impl InputCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// A key went down. A repeat of an already held key is not a new press.
    pub fn key_down(&mut self, key: Key) {
        if self.held.insert(key) {
            self.pressed_since_sample.insert(key);
        }
    }

    pub fn key_up(&mut self, key: Key) {
        self.held.remove(&key);
    }

    /// Releases every held key, e.g. when the window loses focus and would
    /// not see the key-up events. Presses not yet sampled still count once.
    pub fn release_all(&mut self) {
        self.held.clear();
    }

    /// The action set for the next tick.
    pub fn sample(&mut self, map: &ActionMap) -> ActionSet {
        let mut keys = std::mem::take(&mut self.pressed_since_sample);
        keys.extend(self.held.iter().copied());
        map.actions_for(&keys)
    }
}

/// The actions of the current and the previous tick. This is all the
/// simulation sees of input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionState {
    previous: ActionSet,
    current: ActionSet,
}

impl ActionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves to the next tick with the given active actions.
    pub fn advance(&mut self, next: ActionSet) {
        self.previous = std::mem::replace(&mut self.current, next);
    }

    /// Active this tick.
    pub fn pressed(&self, action: &str) -> bool {
        self.current.contains(action)
    }

    /// Active this tick but not the previous one.
    pub fn just_pressed(&self, action: &str) -> bool {
        self.current.contains(action) && !self.previous.contains(action)
    }

    /// Active the previous tick but not this one.
    pub fn just_released(&self, action: &str) -> bool {
        !self.current.contains(action) && self.previous.contains(action)
    }

    pub fn current(&self) -> &ActionSet {
        &self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ActionMap {
        ActionMap::parse(
            r#"[(action: "forward", keys: [KeyW, ArrowUp]), (action: "jump", keys: [Space])]"#,
        )
        .unwrap()
    }

    fn has(set: &ActionSet, action: &str) -> bool {
        set.contains(action)
    }

    #[test]
    fn tap_between_samples_is_active_for_exactly_one_sample() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::Space);
        input.key_up(Key::Space);
        assert!(has(&input.sample(&map), "jump"));
        assert!(!has(&input.sample(&map), "jump"));
    }

    #[test]
    fn held_key_stays_active_across_samples() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::KeyW);
        for _ in 0..3 {
            assert!(has(&input.sample(&map), "forward"));
        }
    }

    #[test]
    fn key_down_while_held_is_not_a_new_tap() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::Space);
        assert!(has(&input.sample(&map), "jump"));
        input.key_down(Key::Space); // OS key repeat
        input.key_up(Key::Space);
        assert!(!has(&input.sample(&map), "jump"));
    }

    #[test]
    fn releasing_one_of_two_bound_keys_keeps_action_active() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::KeyW);
        input.key_down(Key::ArrowUp);
        assert!(has(&input.sample(&map), "forward"));
        input.key_up(Key::KeyW);
        assert!(has(&input.sample(&map), "forward"));
        input.key_up(Key::ArrowUp);
        assert!(!has(&input.sample(&map), "forward"));
    }

    #[test]
    fn focus_loss_releases_held_keys() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::KeyW);
        assert!(has(&input.sample(&map), "forward"));
        input.release_all();
        assert!(!has(&input.sample(&map), "forward"));
    }

    #[test]
    fn action_state_reports_press_and_release_edges() {
        let jump = || ActionSet::from(["jump".to_owned()]);
        let mut state = ActionState::new();
        // (tick input, pressed, just_pressed, just_released)
        let steps = [
            (ActionSet::new(), false, false, false),
            (jump(), true, true, false),
            (jump(), true, false, false),
            (ActionSet::new(), false, false, true),
            (ActionSet::new(), false, false, false),
        ];
        for (i, (next, pressed, just_pressed, just_released)) in steps.into_iter().enumerate() {
            state.advance(next);
            assert_eq!(state.pressed("jump"), pressed, "tick {}", i + 1);
            assert_eq!(state.just_pressed("jump"), just_pressed, "tick {}", i + 1);
            assert_eq!(state.just_released("jump"), just_released, "tick {}", i + 1);
        }
    }
}
