//! Every game `loom` can run, picked by the scene's top-level `game`
//! (`docs/architecture.md` section 1). An enum, not a trait: `loom_engine`
//! is the one crate that knows them all, and adding a game makes the
//! compiler point at every `match` to extend.

use anyhow::{Result, bail};
use loom_core::SimClock;
use loom_core::debug::DebugDraw;
use loom_core::input::{ActionMap, ActionState};
use loom_scene::Scene;
use serde::Serialize;

/// Game names, sorted: the order the error message lists them in.
const GAMES: [&str; 2] = ["rogue", "souls"];

/// Both games are boxed, so the enum's size does not depend on what they
/// hold (clippy::large_enum_variant).
#[derive(Debug)]
pub(crate) enum AnyGame {
    Souls(Box<souls::Game>),
    Rogue(Box<rogue::Game>),
}

/// The state dump's `game`: the inner game's state, unwrapped, so a souls
/// dump is byte for byte what it was before there were two games.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum AnyGameState {
    Souls(souls::GameState),
    Rogue(rogue::GameState),
}

/// The events of one tick (`docs/architecture.md` section 6, "轨迹"): the
/// inner game's list, unwrapped, so each event is the game's own object.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(untagged)]
pub(crate) enum AnyEvents<'a> {
    Souls(&'a [souls::Event]),
    Rogue(&'a [rogue::Event]),
}

/// One event, owned: for collecting events over several ticks.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum AnyEvent {
    Souls(souls::Event),
    Rogue(rogue::Event),
}

impl AnyEvents<'_> {
    pub fn len(&self) -> usize {
        match self {
            Self::Souls(events) => events.len(),
            Self::Rogue(events) => events.len(),
        }
    }

    /// Owned copies of the events, in order.
    pub fn to_vec(self) -> Vec<AnyEvent> {
        match self {
            Self::Souls(events) => events.iter().cloned().map(AnyEvent::Souls).collect(),
            Self::Rogue(events) => events.iter().cloned().map(AnyEvent::Rogue).collect(),
        }
    }
}

impl AnyGame {
    /// Starts the game `scene` names. Errors are the scene's: a missing or
    /// unknown `game`, or what that game requires of its scene.
    pub fn new(scene: Scene) -> Result<Self> {
        let list = || {
            GAMES
                .iter()
                .map(|g| format!("{g:?}"))
                .collect::<Vec<_>>()
                .join(" and ")
        };
        match scene.game() {
            Some("souls") => Ok(Self::Souls(Box::new(souls::Game::new(scene)?))),
            Some("rogue") => Ok(Self::Rogue(Box::new(rogue::Game::new(scene)?))),
            Some(other) => bail!("unknown game {other:?}: the games are {}", list()),
            None => bail!(
                "the scene does not say which game runs it: add {} at the top level",
                GAMES
                    .iter()
                    .map(|g| format!("game: {g:?}"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
        }
    }

    /// The built-in souls demo scene, used without `--scene`.
    pub fn builtin() -> Result<Self> {
        Ok(Self::Souls(Box::new(
            souls::Game::new(souls::demo_scene())?,
        )))
    }

    /// The name the scene's `game` gives this game.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Souls(_) => "souls",
            Self::Rogue(_) => "rogue",
        }
    }

    pub fn bindings(&self) -> ActionMap {
        match self {
            Self::Souls(_) => souls::bindings(),
            Self::Rogue(_) => rogue::bindings(),
        }
    }

    pub fn tick(&mut self, input: &ActionState) {
        match self {
            Self::Souls(game) => game.tick(input),
            Self::Rogue(game) => game.tick(input),
        }
    }

    /// The game's debug draw categories, in its order.
    pub fn debug_categories(&self) -> &'static [&'static str] {
        match self {
            Self::Souls(_) => souls::Game::debug_categories(),
            Self::Rogue(_) => rogue::Game::debug_categories(),
        }
    }

    /// Draws the `categories` given (the game's names) for the current
    /// state. Changes nothing.
    pub fn debug_draw(&self, categories: &[&str], draw: &mut DebugDraw) {
        match self {
            Self::Souls(game) => game.debug_draw(categories, draw),
            Self::Rogue(game) => game.debug_draw(categories, draw),
        }
    }

    /// The events of the latest tick; none before the first.
    pub fn events(&self) -> AnyEvents<'_> {
        match self {
            Self::Souls(game) => AnyEvents::Souls(game.events()),
            Self::Rogue(game) => AnyEvents::Rogue(game.events()),
        }
    }

    pub fn clock(&self) -> SimClock {
        match self {
            Self::Souls(game) => game.clock(),
            Self::Rogue(game) => game.clock(),
        }
    }

    pub fn scene(&self) -> &Scene {
        match self {
            Self::Souls(game) => game.scene(),
            Self::Rogue(game) => game.scene(),
        }
    }

    pub fn state(&self) -> AnyGameState {
        match self {
            Self::Souls(game) => AnyGameState::Souls(game.state()),
            Self::Rogue(game) => AnyGameState::Rogue(game.state()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOULS: &str = include_str!("../../../games/souls/src/demo.scene.ron");
    const ROGUE: &str = include_str!("../../../assets/scenes/rogue.ron");

    fn error_of(src: &str) -> String {
        let scene = Scene::parse(src).expect("the test scene parses");
        match AnyGame::new(scene) {
            Ok(game) => panic!("expected an error, got a {} game", game.name()),
            Err(e) => format!("{e:#}"),
        }
    }

    #[test]
    fn scene_without_a_game_names_both_games() {
        let src = SOULS.replace("    game: \"souls\",", "");
        assert_ne!(src, SOULS);
        assert_eq!(
            error_of(&src),
            "the scene does not say which game runs it: add game: \"rogue\" or game: \"souls\" \
             at the top level"
        );
    }

    #[test]
    fn unknown_game_name_lists_the_games() {
        let src = ROGUE.replace("game: \"rogue\"", "game: \"rouge\"");
        assert_eq!(
            error_of(&src),
            "unknown game \"rouge\": the games are \"rogue\" and \"souls\""
        );
    }

    #[test]
    fn scene_game_picks_souls_or_rogue() {
        let souls = AnyGame::new(Scene::parse(SOULS).unwrap()).unwrap();
        assert!(matches!(souls, AnyGame::Souls(_)));
        assert_eq!(souls.name(), "souls");
        let rogue = AnyGame::new(Scene::parse(ROGUE).unwrap()).unwrap();
        assert!(matches!(rogue, AnyGame::Rogue(_)));
        assert_eq!(rogue.name(), "rogue");
        assert!(rogue.bindings().contains("move_ne"));
    }
}
