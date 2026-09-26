use std::collections::{BTreeMap, BTreeSet};

use super::{ActionMap, ActionSet, Key, TickInput};

/// Live keyboard and mouse input, sampled once per tick.
///
/// An action is active for a tick if one of its keys is held at sampling
/// time, or was pressed since the previous sample. A tap that starts and
/// ends between two samples is therefore active for exactly one tick, no
/// matter how many frames or ticks passed in between. Axes use the same
/// keys, plus the raw mouse motion accumulated since the previous sample.
#[derive(Debug, Clone, Default)]
pub struct InputCollector {
    held: BTreeSet<Key>,
    pressed_since_sample: BTreeSet<Key>,
    /// Raw mouse motion `(dx, dy)` since the previous sample.
    mouse: (f64, f64),
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

    /// Raw mouse motion (right and down are positive); adds up until the
    /// next sample.
    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        self.mouse.0 += dx;
        self.mouse.1 += dy;
    }

    /// Releases every held key and drops the accumulated mouse motion, e.g.
    /// when the window loses focus and would not see the key-up events.
    /// Presses not yet sampled still count once.
    pub fn release_all(&mut self) {
        self.held.clear();
        self.mouse = (0.0, 0.0);
    }

    /// The input for the next tick. Clears the presses and the mouse motion
    /// collected since the previous sample.
    pub fn sample(&mut self, map: &ActionMap) -> TickInput {
        let mut keys = std::mem::take(&mut self.pressed_since_sample);
        keys.extend(self.held.iter().copied());
        let mouse = std::mem::take(&mut self.mouse);
        TickInput {
            actions: map.actions_for(&keys),
            axes: map.axes_for(&keys, mouse),
        }
    }
}

/// The input of the current tick, and the actions of the previous one. This
/// is all the simulation sees of input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActionState {
    previous: ActionSet,
    current: TickInput,
}

impl ActionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves to the next tick with the given input.
    pub fn advance(&mut self, next: TickInput) {
        let previous = std::mem::replace(&mut self.current, next);
        self.previous = previous.actions;
    }

    /// Active this tick.
    pub fn pressed(&self, action: &str) -> bool {
        self.current.actions.contains(action)
    }

    /// Active this tick but not the previous one.
    pub fn just_pressed(&self, action: &str) -> bool {
        self.current.actions.contains(action) && !self.previous.contains(action)
    }

    /// Active the previous tick but not this one.
    pub fn just_released(&self, action: &str) -> bool {
        !self.current.actions.contains(action) && self.previous.contains(action)
    }

    /// The value of `axis` this tick; `0.0` without input.
    pub fn axis(&self, axis: &str) -> f32 {
        self.current.axis(axis)
    }

    /// The actions active this tick.
    pub fn current(&self) -> &ActionSet {
        &self.current.actions
    }

    /// This tick's whole input, e.g. for recording.
    pub fn input(&self) -> &TickInput {
        &self.current
    }

    /// This tick's value of every axis in `map` (zero included), by name.
    pub fn axis_values(&self, map: &ActionMap) -> BTreeMap<String, f32> {
        map.axes()
            .map(|axis| (axis.to_owned(), self.axis(axis)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ActionMap {
        ActionMap::parse(
            r#"(
                actions: [(action: "forward", keys: [KeyW, ArrowUp]), (action: "jump", keys: [Space])],
                axes: [
                    (axis: "move_x", keys: [(KeyD, 1.0), (KeyA, -1.0)]),
                    (axis: "look_x", keys: [(KeyL, 0.04)], mouse: X, mouse_scale: 0.0025),
                    (axis: "look_y", keys: [], mouse: Y, mouse_scale: -0.0025),
                ],
            )"#,
        )
        .unwrap()
    }

    fn has(input: &TickInput, action: &str) -> bool {
        input.actions.contains(action)
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
    fn key_axis_is_the_sum_of_its_active_keys() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::KeyD);
        assert_eq!(input.sample(&map).axis("move_x"), 1.0);
        input.key_down(Key::KeyA);
        let both = input.sample(&map);
        assert_eq!(both.axis("move_x").to_bits(), 0.0f32.to_bits());
        assert!(!both.axes.contains_key("move_x"), "zero axes are left out");
        input.key_up(Key::KeyD);
        assert_eq!(input.sample(&map).axis("move_x"), -1.0);
    }

    #[test]
    fn tapped_key_drives_its_axis_for_exactly_one_sample() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::KeyL);
        input.key_up(Key::KeyL);
        assert_eq!(input.sample(&map).axis("look_x"), 0.04);
        assert_eq!(input.sample(&map).axis("look_x"), 0.0);
    }

    #[test]
    fn mouse_motion_goes_to_the_next_sample_only() {
        let map = map();
        let mut input = InputCollector::new();
        input.mouse_motion(3.0, 1.0);
        input.mouse_motion(1.0, 1.0);
        let first = input.sample(&map);
        assert_eq!(first.axis("look_x"), 4.0f32 * 0.0025);
        assert_eq!(first.axis("look_y"), 2.0f32 * -0.0025);
        let second = input.sample(&map);
        assert!(second.axes.is_empty(), "{second:?}");
    }

    #[test]
    fn mouse_motion_adds_to_held_keys() {
        let map = map();
        let mut input = InputCollector::new();
        input.key_down(Key::KeyL);
        input.mouse_motion(2.0, 0.0);
        assert_eq!(input.sample(&map).axis("look_x"), 0.04 + 2.0f32 * 0.0025);
    }

    #[test]
    fn release_all_drops_accumulated_mouse_motion() {
        let map = map();
        let mut input = InputCollector::new();
        input.mouse_motion(5.0, -5.0);
        input.release_all();
        assert!(input.sample(&map).axes.is_empty());
    }

    #[test]
    fn negative_mouse_scale_without_motion_gives_positive_zero() {
        let map = map();
        let mut input = InputCollector::new();
        let sample = input.sample(&map);
        assert_eq!(sample.axis("look_y").to_bits(), 0.0f32.to_bits());
        let mut state = ActionState::new();
        state.advance(sample);
        let values = state.axis_values(&map);
        assert_eq!(values["look_y"].to_bits(), 0.0f32.to_bits());
        // Also when the motion itself is a negative zero.
        input.mouse_motion(-0.0, -0.0);
        let sample = input.sample(&map);
        assert_eq!(sample.axis("look_y").to_bits(), 0.0f32.to_bits());
        assert!(sample.axes.is_empty());
    }

    #[test]
    fn action_state_lists_every_mapped_axis() {
        let map = map();
        let mut state = ActionState::new();
        state.advance(TickInput {
            actions: ActionSet::new(),
            axes: [("move_x".to_owned(), -1.0)].into(),
        });
        assert_eq!(state.axis("move_x"), -1.0);
        assert_eq!(state.axis("unknown"), 0.0);
        let values: Vec<_> = state.axis_values(&map).into_iter().collect();
        assert_eq!(
            values,
            [
                ("look_x".to_owned(), 0.0),
                ("look_y".to_owned(), 0.0),
                ("move_x".to_owned(), -1.0)
            ]
        );
    }

    #[test]
    fn action_state_reports_press_and_release_edges() {
        let jump = || TickInput {
            actions: ActionSet::from(["jump".to_owned()]),
            axes: BTreeMap::new(),
        };
        let mut state = ActionState::new();
        // (tick input, pressed, just_pressed, just_released)
        let steps = [
            (TickInput::default(), false, false, false),
            (jump(), true, true, false),
            (jump(), true, false, false),
            (TickInput::default(), false, false, true),
            (TickInput::default(), false, false, false),
        ];
        for (i, (next, pressed, just_pressed, just_released)) in steps.into_iter().enumerate() {
            state.advance(next);
            assert_eq!(state.pressed("jump"), pressed, "tick {}", i + 1);
            assert_eq!(state.just_pressed("jump"), just_pressed, "tick {}", i + 1);
            assert_eq!(state.just_released("jump"), just_released, "tick {}", i + 1);
        }
    }
}
