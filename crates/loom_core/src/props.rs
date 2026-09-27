//! Describing a game's props (`docs/architecture.md` section 17): each game
//! keeps one table of [`PropSpec`]s that both checks the props of a scene
//! and is written out by `loom schema`.

use serde::Serialize;

/// How a prop's value is checked. Serialized as `{"kind": …, …}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropRule {
    /// Greater than 0.
    Positive,
    /// At least `min`.
    AtLeast { min: f64 },
    /// A whole number from `min`, up to `max` if given, both included.
    Whole {
        min: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<u32>,
    },
    /// At least `min` and less than `max`.
    Below { min: f64, max: f64 },
    /// An angle in degrees, greater than 0 and at most 360.
    Arc,
    /// Greater than 0 and at most `max`.
    PositiveAtMost { max: f64 },
    /// Any number: checked only against other props (see the group's
    /// constraints).
    Any,
}

impl PropRule {
    /// Whether `value` passes this rule.
    pub fn allows(self, value: f64) -> bool {
        match self {
            Self::Positive => value > 0.0,
            Self::AtLeast { min } => value >= min,
            Self::Whole { min, max } => {
                value.fract() == 0.0
                    && value >= f64::from(min)
                    && max.is_none_or(|max| value <= f64::from(max))
            }
            Self::Below { min, max } => (min..max).contains(&value),
            Self::Arc => value > 0.0 && value <= 360.0,
            Self::PositiveAtMost { max } => value > 0.0 && value <= max,
            Self::Any => true,
        }
    }
}

/// What a value must be, as error messages say it after "must be", e.g.
/// "a whole number from 1 to 1000".
impl std::fmt::Display for PropRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::Positive => write!(f, "greater than 0"),
            Self::AtLeast { min } => write!(f, "at least {min}"),
            Self::Whole { min, max: None } => write!(f, "a whole number of at least {min}"),
            Self::Whole {
                min,
                max: Some(max),
            } => write!(f, "a whole number from {min} to {max}"),
            Self::Below { min, max } => write!(f, "at least {min} and less than {max}"),
            Self::Arc => write!(f, "greater than 0 and at most 360"),
            Self::PositiveAtMost { max } => write!(f, "greater than 0 and at most {max}"),
            Self::Any => write!(f, "a number"),
        }
    }
}

/// One prop a game understands.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PropSpec {
    pub name: &'static str,
    /// The value used when the prop is left out; `None` when it has none
    /// (it must be given, or giving it turns something on).
    pub default: Option<f64>,
    pub rule: PropRule,
    /// What the number counts, in English, e.g. "meters", "ticks".
    pub unit: &'static str,
    /// What the prop does, in English.
    pub description: &'static str,
}

/// The props of one kind of node, as `loom schema` writes them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PropGroup {
    /// In the order they are checked.
    pub props: &'static [PropSpec],
    /// Which props only go with another one, in words.
    pub requires: &'static [&'static str],
    /// Rules across props, in words, e.g. "lock_range <= lock_break_range".
    pub constraints: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_allows_exactly_its_range() {
        let cases: [(PropRule, &[f64], &[f64]); 7] = [
            (PropRule::Positive, &[0.001, 5.0], &[0.0, -1.0]),
            (PropRule::AtLeast { min: 0.0 }, &[0.0, 0.5], &[-0.001]),
            (
                PropRule::Whole { min: 1, max: None },
                &[1.0, 7.0],
                &[0.0, 1.5],
            ),
            (
                PropRule::Whole {
                    min: 1,
                    max: Some(1000),
                },
                &[1.0, 1000.0],
                &[0.0, 1001.0, 2.5],
            ),
            (
                PropRule::Below {
                    min: 0.0,
                    max: 90.0,
                },
                &[0.0, 89.9],
                &[90.0, -0.1],
            ),
            (PropRule::Arc, &[0.1, 360.0], &[0.0, 360.1]),
            (
                PropRule::PositiveAtMost { max: 180.0 },
                &[0.1, 180.0],
                &[0.0, 180.1],
            ),
        ];
        for (rule, ok, bad) in cases {
            for &v in ok {
                assert!(rule.allows(v), "{rule:?} {v}");
            }
            for &v in bad {
                assert!(!rule.allows(v), "{rule:?} {v}");
            }
        }
        assert!(PropRule::Any.allows(-1e9));
    }

    #[test]
    fn rules_read_as_the_error_messages_say_them() {
        let texts = [
            (PropRule::Positive, "greater than 0"),
            (PropRule::AtLeast { min: 0.0 }, "at least 0"),
            (
                PropRule::Whole { min: 1, max: None },
                "a whole number of at least 1",
            ),
            (
                PropRule::Whole {
                    min: 1,
                    max: Some(1000),
                },
                "a whole number from 1 to 1000",
            ),
            (
                PropRule::Below {
                    min: 0.0,
                    max: 90.0,
                },
                "at least 0 and less than 90",
            ),
            (PropRule::Arc, "greater than 0 and at most 360"),
            (
                PropRule::PositiveAtMost { max: 180.0 },
                "greater than 0 and at most 180",
            ),
        ];
        for (rule, text) in texts {
            assert_eq!(rule.to_string(), text);
        }
    }

    #[test]
    fn rules_serialize_with_their_kind_and_only_given_bounds() {
        let json = |rule: PropRule| serde_json::to_string(&rule).unwrap();
        assert_eq!(json(PropRule::Positive), r#"{"kind":"positive"}"#);
        assert_eq!(
            json(PropRule::AtLeast { min: 0.0 }),
            r#"{"kind":"at_least","min":0.0}"#
        );
        assert_eq!(
            json(PropRule::Whole { min: 1, max: None }),
            r#"{"kind":"whole","min":1}"#
        );
        assert_eq!(
            json(PropRule::Whole {
                min: 1,
                max: Some(1000)
            }),
            r#"{"kind":"whole","min":1,"max":1000}"#
        );
        assert_eq!(
            json(PropRule::Below {
                min: 0.0,
                max: 90.0
            }),
            r#"{"kind":"below","min":0.0,"max":90.0}"#
        );
        assert_eq!(json(PropRule::Arc), r#"{"kind":"arc"}"#);
        assert_eq!(
            json(PropRule::PositiveAtMost { max: 180.0 }),
            r#"{"kind":"positive_at_most","max":180.0}"#
        );
        assert_eq!(json(PropRule::Any), r#"{"kind":"any"}"#);
    }
}
