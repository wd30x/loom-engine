//! Debug draw selection and conversion (`docs/architecture.md` section 16):
//! which of the game's categories are on, and the game's lines as the
//! renderer's.

use loom_core::debug::DebugLine;
use loom_render::LineDraw;

/// A bad `--debug-draw` value or serve `debug_draw` list: an argument
/// error (exit code 2), whose message is the whole report.
#[derive(Debug)]
pub(crate) struct DebugDrawError(pub String);

impl std::fmt::Display for DebugDrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DebugDrawError {}

/// The categories `names` selects out of `game`'s `categories`, in the
/// game's order. Every name must be one of them, and none given twice.
pub(crate) fn select(
    game: &str,
    categories: &'static [&'static str],
    names: &[&str],
) -> Result<Vec<&'static str>, DebugDrawError> {
    for (i, name) in names.iter().enumerate() {
        if !categories.contains(name) {
            return Err(DebugDrawError(format!(
                "unknown debug draw category {name:?}: {game} has {}",
                list(categories)
            )));
        }
        if names[..i].contains(name) {
            return Err(DebugDrawError(format!(
                "debug draw category {name:?} is given twice"
            )));
        }
    }
    Ok(categories
        .iter()
        .copied()
        .filter(|c| names.contains(c))
        .collect())
}

/// `--debug-draw` as given: `None` without a value (every category), else
/// the comma-separated names.
pub(crate) fn select_cli(
    game: &str,
    categories: &'static [&'static str],
    value: Option<&str>,
) -> Result<Vec<&'static str>, DebugDrawError> {
    match value {
        None => Ok(categories.to_vec()),
        Some(value) => select(game, categories, &value.split(',').collect::<Vec<_>>()),
    }
}

/// `"a", "b" and "c"`; `none` for no categories.
fn list(categories: &[&str]) -> String {
    let quoted: Vec<String> = categories.iter().map(|c| format!("{c:?}")).collect();
    match quoted.as_slice() {
        [] => "none".to_owned(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// The game's lines as the renderer draws them.
pub(crate) fn line_draws(lines: &[DebugLine]) -> Vec<LineDraw> {
    lines
        .iter()
        .map(|l| LineDraw {
            a: l.a,
            b: l.b,
            color: l.color,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOULS: [&str; 4] = ["hurtboxes", "swings", "colliders", "lock"];

    #[test]
    fn debug_draw_without_a_value_selects_every_category() {
        assert_eq!(select_cli("souls", &SOULS, None).unwrap(), SOULS);
        assert!(select_cli("rogue", &[], None).unwrap().is_empty());
    }

    #[test]
    fn debug_draw_names_come_back_in_the_game_order() {
        assert_eq!(
            select_cli("souls", &SOULS, Some("lock,hurtboxes")).unwrap(),
            ["hurtboxes", "lock"]
        );
        assert!(select("souls", &SOULS, &[]).unwrap().is_empty());
    }

    #[test]
    fn unknown_empty_and_repeated_debug_draw_names_are_rejected_with_exact_messages() {
        let err = |value| select_cli("souls", &SOULS, Some(value)).unwrap_err().0;
        assert_eq!(
            err("lock,hitboxes"),
            r#"unknown debug draw category "hitboxes": souls has "hurtboxes", "swings", "colliders" and "lock""#
        );
        assert_eq!(
            err(""),
            r#"unknown debug draw category "": souls has "hurtboxes", "swings", "colliders" and "lock""#
        );
        assert_eq!(
            err("lock,swings,lock"),
            r#"debug draw category "lock" is given twice"#
        );
        assert_eq!(
            select_cli("rogue", &[], Some("lock")).unwrap_err().0,
            r#"unknown debug draw category "lock": rogue has none"#
        );
    }
}
