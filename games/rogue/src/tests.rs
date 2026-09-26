use std::collections::BTreeSet;

use glam::Vec3;
use loom_core::input::{ActionState, Key, TickInput};

use super::*;

/// Where the test maps sit, so cell positions are not just the node's.
const MAP_ORIGIN: (f32, f32) = (10.0, 5.0);

/// A scene whose map (at `MAP_ORIGIN`, z 0) has `rows` and whose player
/// node is at `(x, y, z)`; `map_extra` and `player_extra` are appended to
/// the two nodes.
fn scene_src(rows: &[&str], (x, y, z): (f32, f32, f32), extra: (&str, &str)) -> String {
    let rows: Vec<String> = rows.iter().map(|r| format!("{r:?}")).collect();
    format!(
        r####"(
    version: 1,
    game: "rogue",
    camera: "cam",
    ambient: (1.0, 1.0, 1.0),
    nodes: [
        (name: "cam", position: (0.0, 0.0, 10.0), camera: (ortho: (height: 8.0), near: 0.1, far: 100.0)),
        (name: "map", position: ({mx:?}, {my:?}, 0.0){map_extra},
            tilemap: (sheet: "tiles.json", legend: [('#', "wall"), ('.', "floor")], rows: [{rows}])),
        (name: "player", position: ({x:?}, {y:?}, {z:?}){player_extra},
            sprite: (sheet: "tiles.json", tag: "player")),
    ],
)"####,
        mx = MAP_ORIGIN.0,
        my = MAP_ORIGIN.1,
        rows = rows.join(", "),
        map_extra = extra.0,
        player_extra = extra.1,
    )
}

/// The node position of cell `(c, r)` at player height 1.
fn cell_position(c: i32, r: i32) -> (f32, f32, f32) {
    (MAP_ORIGIN.0 + c as f32, MAP_ORIGIN.1 - r as f32, 1.0)
}

/// A game on `rows` with the player on cell `(c, r)` at z 1.
fn game_at(rows: &[&str], c: i32, r: i32) -> Game {
    Game::from_source(&scene_src(rows, cell_position(c, r), ("", ""))).unwrap()
}

fn error_of(src: &str) -> String {
    match Game::from_source(src) {
        Ok(_) => panic!("the scene should be rejected:\n{src}"),
        Err(e) => e.to_string(),
    }
}

/// Runs one tick with exactly `actions` held.
fn tick(game: &mut Game, input: &mut ActionState, actions: &[&str]) {
    input.advance(TickInput {
        actions: actions.iter().map(|a| a.to_string()).collect(),
        ..TickInput::default()
    });
    game.tick(input);
}

/// Presses `actions` for one tick, then releases everything for one.
fn press(game: &mut Game, input: &mut ActionState, actions: &[&str]) {
    tick(game, input, actions);
    tick(game, input, &[]);
}

fn player_position(game: &Game) -> Vec3 {
    let id = game.scene().find("player").unwrap();
    game.scene().transform(id).position
}

const OPEN: [&str; 5] = [".....", ".....", ".....", ".....", "....."];

/// Walls north (1, 1) and east (2, 2) of the cell (1, 2), whose north-east
/// neighbour (2, 1) is floor; the map edge is all floor.
const CORNER: [&str; 5] = [".....", ".#...", "..#..", ".....", "....."];

#[test]
fn bindings_are_the_nine_actions_on_numpad_and_vi_keys() {
    let map = bindings();
    let mut names: Vec<&str> = map.actions().collect();
    names.sort_unstable();
    let mut expected: Vec<&str> = MOVES.iter().map(|m| m.0).chain([WAIT]).collect();
    expected.sort_unstable();
    assert_eq!(names, expected);
    assert_eq!(map.axes().count(), 0);
    assert!(!map.uses_mouse());
    let table = [
        ("move_n", Key::Numpad8, Key::KeyK),
        ("move_ne", Key::Numpad9, Key::KeyU),
        ("move_e", Key::Numpad6, Key::KeyL),
        ("move_se", Key::Numpad3, Key::KeyN),
        ("move_s", Key::Numpad2, Key::KeyJ),
        ("move_sw", Key::Numpad1, Key::KeyB),
        ("move_w", Key::Numpad4, Key::KeyH),
        ("move_nw", Key::Numpad7, Key::KeyY),
        ("wait", Key::Numpad5, Key::Period),
    ];
    for (action, numpad, vi) in table {
        for key in [numpad, vi] {
            let actions = map.actions_for(&BTreeSet::from([key]));
            assert_eq!(actions, BTreeSet::from([action.to_owned()]), "{key:?}");
        }
    }
}

#[test]
fn each_direction_moves_one_cell_and_takes_one_turn() {
    for (action, dc, dr) in MOVES {
        let mut game = game_at(&OPEN, 2, 2);
        let mut input = ActionState::new();
        tick(&mut game, &mut input, &[action]);
        let (c, r) = (2 + dc, 2 + dr);
        assert_eq!(game.state().player.cell, [c, r], "{action}");
        assert_eq!(game.state().turn, 1, "{action}");
        let (x, y, z) = cell_position(c, r);
        assert_eq!(player_position(&game), Vec3::new(x, y, z), "{action}");
    }
}

#[test]
fn waiting_takes_a_turn_in_place() {
    let mut game = game_at(&OPEN, 2, 2);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &[WAIT]);
    assert_eq!(game.state().player.cell, [2, 2]);
    assert_eq!(game.state().turn, 1);
}

#[test]
fn bumping_into_a_wall_neither_moves_nor_takes_a_turn() {
    let mut game = game_at(&CORNER, 1, 2);
    let mut input = ActionState::new();
    press(&mut game, &mut input, &["move_n"]);
    press(&mut game, &mut input, &["move_e"]);
    assert_eq!(game.state().player.cell, [1, 2]);
    assert_eq!(game.state().turn, 0);
    let (x, y, z) = cell_position(1, 2);
    assert_eq!(player_position(&game), Vec3::new(x, y, z));
}

#[test]
fn map_edge_blocks_like_a_wall() {
    let mut game = game_at(&OPEN, 0, 0);
    let mut input = ActionState::new();
    for action in ["move_n", "move_nw", "move_w", "move_ne", "move_sw"] {
        press(&mut game, &mut input, &[action]);
    }
    assert_eq!(game.state().player.cell, [0, 0]);
    assert_eq!(game.state().turn, 0);
    let mut game = game_at(&OPEN, 4, 4);
    for action in ["move_s", "move_se", "move_e"] {
        press(&mut game, &mut input, &[action]);
    }
    assert_eq!(game.state().player.cell, [4, 4]);
    assert_eq!(game.state().turn, 0);
}

#[test]
fn diagonal_step_squeezes_between_two_walls() {
    let mut game = game_at(&CORNER, 1, 2);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &["move_ne"]);
    assert_eq!(game.state().player.cell, [2, 1]);
    assert_eq!(game.state().turn, 1);
}

#[test]
fn holding_a_key_moves_once_and_pressing_again_moves_again() {
    let mut game = game_at(&OPEN, 0, 2);
    let mut input = ActionState::new();
    for _ in 0..10 {
        tick(&mut game, &mut input, &["move_e"]);
    }
    assert_eq!(game.state().player.cell, [1, 2]);
    assert_eq!(game.state().turn, 1);
    tick(&mut game, &mut input, &[]);
    tick(&mut game, &mut input, &["move_e"]);
    assert_eq!(game.state().player.cell, [2, 2]);
    assert_eq!(game.state().turn, 2);
    assert_eq!(game.clock().tick(), 12);
}

#[test]
fn simultaneous_presses_take_only_the_first_in_table_order() {
    let mut game = game_at(&OPEN, 2, 2);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &["move_s", "move_n", WAIT]);
    assert_eq!(game.state().player.cell, [2, 1]);
    assert_eq!(game.state().turn, 1);
    // The other presses are dropped, not queued.
    tick(&mut game, &mut input, &["move_s", "move_n", WAIT]);
    assert_eq!(game.state().player.cell, [2, 1]);
    assert_eq!(game.state().turn, 1);
}

#[test]
fn scene_without_a_map_tilemap_is_rejected() {
    let src = scene_src(&OPEN, cell_position(0, 0), ("", ""));
    let no_tilemap = src.replace("(name: \"map\"", "(name: \"ground\"");
    assert_eq!(
        error_of(&no_tilemap),
        "the scene needs a root node named \"map\" with a tilemap"
    );
}

#[test]
fn scene_without_a_player_sprite_is_rejected() {
    let src = scene_src(&OPEN, cell_position(0, 0), ("", ""));
    let no_sprite = src.replace(
        "sprite: (sheet: \"tiles.json\", tag: \"player\")",
        "mesh: Cube",
    );
    assert_eq!(
        error_of(&no_sprite),
        "the scene needs a root node named \"player\" with a sprite"
    );
}

#[test]
fn rotated_or_scaled_map_and_player_are_rejected() {
    let expected = |path: &str| {
        format!("\"{path}\" must only be translated: no rotation or look_at, scale (1, 1, 1)")
    };
    let rotated_map = scene_src(
        &OPEN,
        cell_position(0, 0),
        (", rotation: (0.0, 0.0, 90.0)", ""),
    );
    assert_eq!(error_of(&rotated_map), expected("map"));
    let scaled_player = scene_src(&OPEN, cell_position(0, 0), ("", ", scale: (2.0, 2.0, 1.0)"));
    assert_eq!(error_of(&scaled_player), expected("player"));
}

#[test]
fn player_off_the_cell_grid_is_rejected() {
    let between = scene_src(&OPEN, (10.5, 3.0, 1.0), ("", ""));
    assert_eq!(
        error_of(&between),
        "the player at (10.5, 3) is not on a map cell: x - map.x and map.y - y must be whole \
         numbers inside the 5x5 map"
    );
    let outside = scene_src(&OPEN, (15.0, 5.0, 1.0), ("", ""));
    assert_eq!(
        error_of(&outside),
        "the player at (15, 5) is not on a map cell: x - map.x and map.y - y must be whole \
         numbers inside the 5x5 map"
    );
}

#[test]
fn player_starting_on_a_wall_is_rejected() {
    let src = scene_src(&CORNER, cell_position(2, 2), ("", ""));
    assert_eq!(
        error_of(&src),
        "the player starts on a wall at column 2, row 2"
    );
}

#[test]
fn player_not_in_front_of_the_map_is_rejected() {
    let (x, y, _) = cell_position(0, 0);
    let src = scene_src(&OPEN, (x, y, 0.0), ("", ""));
    assert_eq!(
        error_of(&src),
        "the player must be in front of the map: its z (0) must be greater than the map's (0)"
    );
}

#[test]
fn state_serializes_as_turn_and_player_cell() {
    let game = game_at(&OPEN, 3, 2);
    let json = serde_json::to_string(&game.state()).unwrap();
    assert_eq!(json, r#"{"turn":0,"player":{"cell":[3,2]}}"#);
}

#[test]
fn project_rogue_scene_starts_on_column_three_row_two() {
    let game = Game::from_source(include_str!("../../../assets/scenes/rogue.ron")).unwrap();
    assert_eq!(game.clock().tick(), 0);
    assert_eq!(game.state().player.cell, [3, 2]);
    assert_eq!(game.state().turn, 0);
    assert_eq!(game.scene().game(), Some("rogue"));
}
