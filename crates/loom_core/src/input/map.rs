use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::{ActionSet, InputError, Key, canonical_axis_value};

/// A bindings file: `(actions: [...], axes: [...])`, both optional.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingsFile {
    #[serde(default)]
    actions: Vec<Binding>,
    #[serde(default)]
    axes: Vec<AxisBindingFile>,
}

/// One entry of `actions`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    action: String,
    keys: Vec<Key>,
}

/// One entry of `axes`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AxisBindingFile {
    axis: String,
    keys: Vec<(Key, f32)>,
    #[serde(default)]
    mouse: Option<MouseAxis>,
    #[serde(default)]
    mouse_scale: Option<f32>,
}

/// A direction of raw mouse motion: right is +X, down is +Y.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MouseAxis {
    X,
    Y,
}

/// A validated axis binding.
#[derive(Debug, Clone, PartialEq)]
struct AxisBinding {
    axis: String,
    keys: Vec<(Key, f32)>,
    mouse: Option<(MouseAxis, f32)>,
}

/// Validated action and axis bindings, parsed from RON such as
/// `(actions: [(action: "roll", keys: [Space])], axes: [(axis: "move_x", keys: [(KeyD, 1.0)])])`.
///
/// Lists, not maps: ron silently keeps the last of two duplicate map keys,
/// which would hide a duplicated action or axis.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionMap {
    /// In file order; every action name is unique.
    bindings: Vec<(String, Vec<Key>)>,
    /// In file order; every axis name is unique and no action's name.
    axes: Vec<AxisBinding>,
}

impl ActionMap {
    pub fn parse(src: &str) -> Result<Self, InputError> {
        // `mouse: X` rather than `mouse: Some(X)`.
        let options = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let file: BindingsFile = options.from_str(src)?;

        let mut seen = BTreeSet::new();
        let mut bindings = Vec::with_capacity(file.actions.len());
        for Binding { action, keys } in file.actions {
            if !is_valid_name(&action) {
                return Err(InputError::InvalidActionName(action));
            }
            if !seen.insert(action.clone()) {
                return Err(InputError::DuplicateAction(action));
            }
            let mut unique = BTreeSet::new();
            if let Some(&key) = keys.iter().find(|&&k| !unique.insert(k)) {
                return Err(InputError::RepeatedKey { action, key });
            }
            bindings.push((action, keys));
        }

        let mut axes: Vec<AxisBinding> = Vec::with_capacity(file.axes.len());
        for AxisBindingFile {
            axis,
            keys,
            mouse,
            mouse_scale,
        } in file.axes
        {
            if !is_valid_name(&axis) {
                return Err(InputError::InvalidAxisName(axis));
            }
            if seen.contains(&axis) {
                return Err(InputError::AxisNamedLikeAction(axis));
            }
            if axes.iter().any(|a| a.axis == axis) {
                return Err(InputError::DuplicateAxis(axis));
            }
            let mut unique = BTreeSet::new();
            if let Some(&(key, _)) = keys.iter().find(|(k, _)| !unique.insert(*k)) {
                return Err(InputError::RepeatedAxisKey { axis, key });
            }
            if let Some(&(key, _)) = keys.iter().find(|(_, v)| !v.is_finite()) {
                return Err(InputError::NonFiniteKeyValue { axis, key });
            }
            let mouse = match (mouse, mouse_scale) {
                (None, None) => None,
                (Some(_), Some(scale)) if !scale.is_finite() => {
                    return Err(InputError::NonFiniteMouseScale(axis));
                }
                (Some(direction), Some(scale)) => Some((direction, scale)),
                _ => return Err(InputError::MouseWithoutScale(axis)),
            };
            axes.push(AxisBinding { axis, keys, mouse });
        }
        Ok(Self { bindings, axes })
    }

    pub fn contains(&self, action: &str) -> bool {
        self.bindings.iter().any(|(a, _)| a == action)
    }

    pub fn contains_axis(&self, axis: &str) -> bool {
        self.axes.iter().any(|a| a.axis == axis)
    }

    /// Action names in file order.
    /// Whether any axis reads mouse motion; the window captures the mouse
    /// only then (`docs/architecture.md` section 9).
    pub fn uses_mouse(&self) -> bool {
        self.axes.iter().any(|a| a.mouse.is_some())
    }

    pub fn actions(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|(a, _)| a.as_str())
    }

    /// Axis names in file order.
    pub fn axes(&self) -> impl Iterator<Item = &str> {
        self.axes.iter().map(|a| a.axis.as_str())
    }

    /// The actions bound to at least one of `keys`.
    pub fn actions_for(&self, keys: &BTreeSet<Key>) -> ActionSet {
        self.bindings
            .iter()
            .filter(|(_, bound)| bound.iter().any(|k| keys.contains(k)))
            .map(|(a, _)| a.clone())
            .collect()
    }

    /// Axis values for active `keys` and accumulated raw mouse motion
    /// `(dx, dy)`: the sum of the active keys' values plus the motion times
    /// `mouse_scale`. Only non-zero values are returned, and never `-0.0`.
    pub fn axes_for(&self, keys: &BTreeSet<Key>, mouse: (f64, f64)) -> BTreeMap<String, f32> {
        let mut values = BTreeMap::new();
        for binding in &self.axes {
            let mut value = 0.0f32;
            for (key, key_value) in &binding.keys {
                if keys.contains(key) {
                    value += key_value;
                }
            }
            if let Some((direction, scale)) = binding.mouse {
                let motion = match direction {
                    MouseAxis::X => mouse.0,
                    MouseAxis::Y => mouse.1,
                };
                value += motion as f32 * scale;
            }
            let value = canonical_axis_value(value);
            if value != 0.0 {
                values.insert(binding.axis.clone(), value);
            }
        }
        values
    }
}

/// `^[a-z][a-z0-9_]*$`
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(list: &str) -> String {
        format!("(actions: [{list}])")
    }

    fn axes(list: &str) -> String {
        format!("(axes: [{list}])")
    }

    #[test]
    fn uses_mouse_only_when_an_axis_reads_the_mouse() {
        let keys_only = axes(r#"(axis: "move_x", keys: [(KeyD, 1.0)])"#);
        assert!(!ActionMap::parse(&keys_only).unwrap().uses_mouse());
        let no_axes = actions(r#"(action: "wait", keys: [Period])"#);
        assert!(!ActionMap::parse(&no_axes).unwrap().uses_mouse());
        let mouse = axes(
            r#"(axis: "move_x", keys: [(KeyD, 1.0)]), (axis: "look_x", keys: [], mouse: X, mouse_scale: 0.01)"#,
        );
        assert!(ActionMap::parse(&mouse).unwrap().uses_mouse());
    }

    #[test]
    fn numpad_and_period_keys_parse_by_their_winit_names() {
        let src = actions(
            r#"(action: "step", keys: [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, Period])"#,
        );
        let map = ActionMap::parse(&src).unwrap();
        for key in [
            Key::Numpad1,
            Key::Numpad2,
            Key::Numpad3,
            Key::Numpad4,
            Key::Numpad5,
            Key::Numpad6,
            Key::Numpad7,
            Key::Numpad8,
            Key::Numpad9,
            Key::Period,
        ] {
            let pressed = map.actions_for(&BTreeSet::from([key]));
            assert!(pressed.contains("step"), "{key:?}");
        }
    }

    #[test]
    fn action_map_rejects_a_duplicate_action() {
        let src = actions(r#"(action: "jump", keys: [Space]), (action: "jump", keys: [KeyJ])"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(
            matches!(err, InputError::DuplicateAction(ref a) if a == "jump"),
            "{err}"
        );
    }

    #[test]
    fn action_map_rejects_an_invalid_action_name() {
        for name in ["Move", "1a", "", "move-left", "_x", "a b"] {
            let src = actions(&format!(r#"(action: "{name}", keys: [])"#));
            let err = ActionMap::parse(&src).unwrap_err();
            assert!(
                matches!(err, InputError::InvalidActionName(_)),
                "{name:?}: {err}"
            );
        }
    }

    #[test]
    fn action_map_rejects_a_repeated_key_within_one_action() {
        let src = actions(r#"(action: "jump", keys: [Space, KeyJ, Space])"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(
            matches!(
                err,
                InputError::RepeatedKey {
                    key: Key::Space,
                    ..
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn action_map_rejects_an_unknown_key_name() {
        let err = ActionMap::parse(&actions(r#"(action: "jump", keys: [KeyQQ])"#)).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
        assert!(err.to_string().contains("KeyQQ"), "{err}");
    }

    #[test]
    fn action_map_rejects_a_misspelled_field() {
        let err = ActionMap::parse(&actions(r#"(action: "jump", key: [Space])"#)).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
    }

    #[test]
    fn action_map_accepts_one_key_bound_to_two_actions() {
        let map = ActionMap::parse(&actions(
            r#"(action: "jump", keys: [Space]), (action: "confirm", keys: [Space, Enter]), (action: "scripted", keys: [])"#,
        ))
        .unwrap();
        assert_eq!(
            map.actions().collect::<Vec<_>>(),
            ["jump", "confirm", "scripted"]
        );
        let active = map.actions_for(&BTreeSet::from([Key::Space]));
        assert_eq!(
            active,
            ActionSet::from(["confirm".to_owned(), "jump".to_owned()])
        );
        assert!(map.contains("scripted"));
        assert!(!map.contains("missing"));
    }

    #[test]
    fn bindings_file_with_actions_and_axes_parses() {
        let map = ActionMap::parse(
            r#"// comment
            (
                actions: [(action: "roll", keys: [Space])],
                axes: [
                    (axis: "move_x", keys: [(KeyD, 1.0), (KeyA, -1)]),
                    (axis: "look_x", keys: [(KeyL, 0.04)], mouse: X, mouse_scale: 0.0025),
                ],
            )"#,
        )
        .unwrap();
        assert_eq!(map.actions().collect::<Vec<_>>(), ["roll"]);
        assert_eq!(map.axes().collect::<Vec<_>>(), ["move_x", "look_x"]);
        assert!(map.contains_axis("look_x") && !map.contains_axis("roll"));
        assert!(!map.contains("move_x"));
    }

    #[test]
    fn bindings_file_may_omit_both_lists() {
        let map = ActionMap::parse("()").unwrap();
        assert_eq!(map.actions().count() + map.axes().count(), 0);
        let only_axes = ActionMap::parse(&axes(r#"(axis: "move_x", keys: [])"#)).unwrap();
        assert_eq!(only_axes.axes().collect::<Vec<_>>(), ["move_x"]);
    }

    #[test]
    fn old_list_format_is_rejected() {
        let err = ActionMap::parse(r#"[(action: "jump", keys: [Space])]"#).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
    }

    #[test]
    fn axis_with_the_name_of_an_action_is_rejected() {
        let src = r#"(actions: [(action: "turn", keys: [])], axes: [(axis: "turn", keys: [])])"#;
        let err = ActionMap::parse(src).unwrap_err();
        assert!(
            matches!(err, InputError::AxisNamedLikeAction(ref a) if a == "turn"),
            "{err}"
        );
    }

    #[test]
    fn axis_defined_twice_is_rejected() {
        let src = axes(r#"(axis: "move_x", keys: []), (axis: "move_x", keys: [(KeyD, 1.0)])"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(
            matches!(err, InputError::DuplicateAxis(ref a) if a == "move_x"),
            "{err}"
        );
    }

    #[test]
    fn axis_with_an_invalid_name_is_rejected() {
        let err = ActionMap::parse(&axes(r#"(axis: "Move", keys: [])"#)).unwrap_err();
        assert!(matches!(err, InputError::InvalidAxisName(_)), "{err}");
    }

    #[test]
    fn axis_listing_one_key_twice_is_rejected() {
        let src = axes(r#"(axis: "move_x", keys: [(KeyD, 1.0), (KeyA, -1.0), (KeyD, 0.5)])"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(
            matches!(err, InputError::RepeatedAxisKey { key: Key::KeyD, .. }),
            "{err}"
        );
    }

    #[test]
    fn axis_key_value_that_is_not_finite_is_rejected() {
        for value in ["inf", "-inf", "NaN"] {
            let src = axes(&format!(
                r#"(axis: "move_x", keys: [(KeyD, 1.0), (KeyA, {value})])"#
            ));
            let err = ActionMap::parse(&src).unwrap_err();
            assert!(
                matches!(err, InputError::NonFiniteKeyValue { key: Key::KeyA, .. }),
                "{value}: {err}"
            );
        }
    }

    #[test]
    fn mouse_scale_that_is_not_finite_is_rejected() {
        let src = axes(r#"(axis: "look_x", keys: [], mouse: X, mouse_scale: inf)"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(matches!(err, InputError::NonFiniteMouseScale(_)), "{err}");
    }

    #[test]
    fn mouse_without_mouse_scale_is_rejected() {
        let src = axes(r#"(axis: "look_x", keys: [], mouse: X)"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(matches!(err, InputError::MouseWithoutScale(_)), "{err}");
    }

    #[test]
    fn mouse_scale_without_mouse_is_rejected() {
        let src = axes(r#"(axis: "look_x", keys: [], mouse_scale: 0.5)"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(matches!(err, InputError::MouseWithoutScale(_)), "{err}");
    }

    #[test]
    fn axis_binding_with_an_unknown_field_is_rejected() {
        let src = axes(r#"(axis: "look_x", keys: [], mouse: X, mouse_scale: 0.5, invert: true)"#);
        let err = ActionMap::parse(&src).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
        assert!(err.to_string().contains("invert"), "{err}");
    }
}
