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
    use super::*;
    use crate::{Game, LOOK_X, LOOK_Y, MOVE_X, MOVE_Y};

    #[test]
    fn demo_bindings_define_exactly_the_game_axes() {
        let map = bindings();
        assert_eq!(map.actions().count(), 0);
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
        assert_eq!(paths, ["pillars/a", "pillars/b", "pillars/c", "wall"]);
    }
}
