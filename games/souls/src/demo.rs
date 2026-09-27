//! Built-in demo data: the key bindings and the scene used without
//! `--scene`, both embedded in the binary.

use loom_core::input::ActionMap;
use loom_scene::Scene;

const DEMO_BINDINGS: &str = include_str!("demo_bindings.ron");
const DEMO_SCENE: &str = include_str!("demo.scene.ron");

/// The game's key bindings.
pub fn bindings() -> ActionMap {
    ActionMap::parse(DEMO_BINDINGS)
        .expect("the embedded demo bindings are valid (checked by a unit test)")
}

/// The scene used without `--scene`.
pub fn demo_scene() -> Scene {
    Scene::parse(DEMO_SCENE).expect("the embedded demo scene is valid (checked by a unit test)")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use loom_core::input::Key;

    use super::*;
    use crate::{
        ATTACK, Condition, Game, HEAVY, HealthState, LOCK, LOOK_X, LOOK_Y, MOVE_X, MOVE_Y, ROLL,
    };

    #[test]
    fn demo_bindings_define_the_game_axes_roll_both_attacks_and_lock() {
        let map = bindings();
        let mut actions: Vec<_> = map.actions().collect();
        actions.sort_unstable();
        assert_eq!(actions, [ATTACK, HEAVY, LOCK, ROLL]);
        for (key, action) in [
            (Key::Space, ROLL),
            (Key::MouseLeft, ATTACK),
            (Key::MouseRight, HEAVY),
            (Key::MouseMiddle, LOCK),
            (Key::KeyQ, LOCK),
        ] {
            let pressed = map.actions_for(&BTreeSet::from([key]));
            assert_eq!(pressed, BTreeSet::from([action.to_owned()]), "{key:?}");
        }
        let mut axes: Vec<_> = map.axes().collect();
        axes.sort_unstable();
        assert_eq!(axes, [LOOK_X, LOOK_Y, MOVE_X, MOVE_Y]);
    }

    #[test]
    fn built_in_scene_starts_a_game() {
        let scene = demo_scene();
        assert!(scene.character().is_some() && scene.orbit_camera().is_some());
        let game = Game::new(scene).unwrap();
        assert_eq!(game.clock().tick(), 0);
    }

    #[test]
    fn built_in_scene_colliders_are_the_pillars_then_the_wall() {
        let scene = demo_scene();
        let paths: Vec<_> = scene.colliders().iter().map(|&id| scene.path(id)).collect();
        assert_eq!(
            paths,
            [
                "pillars/a",
                "pillars/b",
                "pillars/c",
                "wall",
                "dummy_a/block",
                "dummy_b/block",
                "dummy_c/block"
            ]
        );
    }

    #[test]
    fn built_in_scene_lock_targets_are_the_three_dummies() {
        let game = Game::new(demo_scene()).unwrap();
        assert_eq!(game.target_paths(), ["dummy_a", "dummy_b", "dummy_c"]);
    }

    #[test]
    fn built_in_scene_hurtboxes_are_the_three_dummies_unhit() {
        let game = Game::new(demo_scene()).unwrap();
        let hurtboxes: Vec<_> = game
            .state()
            .hurtboxes
            .iter()
            .map(|h| (h.node.clone(), h.hits, h.last_hit))
            .collect();
        assert_eq!(
            hurtboxes,
            ["dummy_a", "dummy_b", "dummy_c"].map(|n| (n.to_owned(), 0, None))
        );
    }

    #[test]
    fn built_in_scene_dummies_stand_at_full_hp_under_their_hp_bars() {
        let game = Game::new(demo_scene()).unwrap();
        let full = HealthState {
            hp: 100.0,
            state: Condition::Ok,
            state_ticks: 0,
            impact: 0.0,
            impact_wait: 0,
        };
        for h in game.state().hurtboxes {
            assert_eq!(h.health, Some(full), "{}", h.node);
            let scene = game.scene();
            let bar = scene.find(&format!("{}/hp_bar", h.node)).unwrap();
            assert!(scene.node(bar).mesh.is_some() && scene.is_drawn(bar));
        }
    }
}
