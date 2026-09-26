use std::collections::BTreeSet;

use serde::Deserialize;

use super::{ActionSet, InputError, Key};

/// One entry of a bindings file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    action: String,
    keys: Vec<Key>,
}

/// Validated action bindings, parsed from a RON list such as
/// `[(action: "move_forward", keys: [KeyW, ArrowUp])]`.
///
/// A list, not a map: ron silently keeps the last of two duplicate map keys,
/// which would hide a duplicated action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMap {
    /// In file order; every action name is unique.
    bindings: Vec<(String, Vec<Key>)>,
}

impl ActionMap {
    pub fn parse(src: &str) -> Result<Self, InputError> {
        let raw: Vec<Binding> = ron::from_str(src)?;
        let mut seen = BTreeSet::new();
        let mut bindings = Vec::with_capacity(raw.len());
        for Binding { action, keys } in raw {
            if !is_valid_action_name(&action) {
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
        Ok(Self { bindings })
    }

    pub fn contains(&self, action: &str) -> bool {
        self.bindings.iter().any(|(a, _)| a == action)
    }

    /// Action names in file order.
    pub fn actions(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|(a, _)| a.as_str())
    }

    /// The actions bound to at least one of `keys`.
    pub fn actions_for(&self, keys: &BTreeSet<Key>) -> ActionSet {
        self.bindings
            .iter()
            .filter(|(_, bound)| bound.iter().any(|k| keys.contains(k)))
            .map(|(a, _)| a.clone())
            .collect()
    }
}

/// `^[a-z][a-z0-9_]*$`
fn is_valid_action_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_map_rejects_a_duplicate_action() {
        let src = r#"[(action: "jump", keys: [Space]), (action: "jump", keys: [KeyJ])]"#;
        let err = ActionMap::parse(src).unwrap_err();
        assert!(
            matches!(err, InputError::DuplicateAction(ref a) if a == "jump"),
            "{err}"
        );
    }

    #[test]
    fn action_map_rejects_an_invalid_action_name() {
        for name in ["Move", "1a", "", "move-left", "_x", "a b"] {
            let src = format!(r#"[(action: "{name}", keys: [])]"#);
            let err = ActionMap::parse(&src).unwrap_err();
            assert!(
                matches!(err, InputError::InvalidActionName(_)),
                "{name:?}: {err}"
            );
        }
    }

    #[test]
    fn action_map_rejects_a_repeated_key_within_one_action() {
        let src = r#"[(action: "jump", keys: [Space, KeyJ, Space])]"#;
        let err = ActionMap::parse(src).unwrap_err();
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
        let err = ActionMap::parse(r#"[(action: "jump", keys: [KeyQQ])]"#).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
        assert!(err.to_string().contains("KeyQQ"), "{err}");
    }

    #[test]
    fn action_map_rejects_a_misspelled_field() {
        let err = ActionMap::parse(r#"[(action: "jump", key: [Space])]"#).unwrap_err();
        assert!(matches!(err, InputError::Syntax(_)), "{err}");
    }

    #[test]
    fn action_map_accepts_one_key_bound_to_two_actions() {
        let map = ActionMap::parse(
            r#"[(action: "jump", keys: [Space]), (action: "confirm", keys: [Space, Enter]), (action: "scripted", keys: [])]"#,
        )
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
}
