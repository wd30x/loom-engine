use std::collections::BTreeSet;

use glam::Vec3;
use loom_core::input::{ActionState, Key, TickInput};

use super::*;

/// Where the test maps sit, so node positions differ from cell numbers.
const MAP_ORIGIN: (f32, f32) = (10.0, 5.0);
const PLAYER_Z: f32 = 1.0;
const ZOMBIE_Z: f32 = 0.5;
const PLAYER_PROPS: &str = r#"("hp", 10), ("attack", 1)"#;

/// The node position of cell `(c, r)` at height `z`.
fn at(c: i32, r: i32, z: f32) -> (f32, f32, f32) {
    (MAP_ORIGIN.0 + c as f32, MAP_ORIGIN.1 - r as f32, z)
}

/// A character node with a `tag` sprite that declares a corpse.
fn actor(name: &str, tag: &str, (x, y, z): (f32, f32, f32), props: &str) -> String {
    format!(
        r#"(name: "{name}", position: ({x:?}, {y:?}, {z:?}),
            sprite: (sheet: "tiles.json", tag: "{tag}", alternates: ["corpse"]), props: [{props}])"#
    )
}

/// The player on cell `(c, r)` with 10 hp and attack 1.
fn player(c: i32, r: i32) -> String {
    actor("player", "player", at(c, r, PLAYER_Z), PLAYER_PROPS)
}

/// A zombie on cell `(c, r)`.
fn zombie(name: &str, c: i32, r: i32, hp: i32, attack: i32, speed: i32) -> String {
    let props = format!(r#"("hp", {hp}), ("attack", {attack}), ("speed", {speed})"#);
    actor(name, "zombie", at(c, r, ZOMBIE_Z), &props)
}

/// A rogue scene: the map (at `MAP_ORIGIN`, z 0) with `rows`, then `nodes`.
fn scene(rows: &[&str], nodes: &[String]) -> String {
    let rows: Vec<String> = rows.iter().map(|r| format!("{r:?}")).collect();
    format!(
        r####"(
    version: 1,
    game: "rogue",
    camera: "cam",
    ambient: (1.0, 1.0, 1.0),
    nodes: [
        (name: "cam", position: (0.0, 0.0, 10.0), camera: (ortho: (height: 8.0), near: 0.1, far: 100.0)),
        (name: "map", position: ({mx:?}, {my:?}, 0.0),
            tilemap: (sheet: "tiles.json", legend: [('#', "wall"), ('.', "floor")], rows: [{rows}])),
        {nodes},
    ],
)"####,
        mx = MAP_ORIGIN.0,
        my = MAP_ORIGIN.1,
        rows = rows.join(", "),
        nodes = nodes.join(",\n        "),
    )
}

fn game(rows: &[&str], nodes: &[String]) -> Game {
    Game::from_source(&scene(rows, nodes)).unwrap()
}

/// A game on `rows` with only the player, on cell `(c, r)`.
fn game_at(rows: &[&str], c: i32, r: i32) -> Game {
    game(rows, &[player(c, r)])
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

fn position(game: &Game, name: &str) -> Vec3 {
    let id = game.scene().find(name).unwrap();
    game.scene().transform(id).position
}

fn sprite_tag(game: &Game, name: &str) -> String {
    let id = game.scene().find(name).unwrap();
    game.scene()
        .node(id)
        .sprite
        .as_ref()
        .unwrap()
        .tag()
        .to_owned()
}

fn messages(game: &Game) -> Vec<(u64, String)> {
    game.state()
        .messages
        .into_iter()
        .map(|m| (m.turn, m.text))
        .collect()
}

fn zombie_cell(game: &Game, index: usize) -> [i32; 2] {
    game.state().zombies[index].cell
}

const OPEN: [&str; 5] = [".....", ".....", ".....", ".....", "....."];

/// Walls north (1, 1) and east (2, 2) of the cell (1, 2), whose north-east
/// neighbour (2, 1) is floor; the map edge is all floor.
const CORNER: [&str; 5] = [".....", ".#...", "..#..", ".....", "....."];

/// One row: movement only east and west.
const CORRIDOR: [&str; 1] = ["...................."];

// ---------------------------------------------------------------- player

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
        let (x, y, z) = at(c, r, PLAYER_Z);
        assert_eq!(position(&game, "player"), Vec3::new(x, y, z), "{action}");
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
    let (x, y, z) = at(1, 2, PLAYER_Z);
    assert_eq!(position(&game, "player"), Vec3::new(x, y, z));
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

// ---------------------------------------------------------------- combat

#[test]
fn bumping_a_zombie_hits_it_and_takes_a_turn_without_moving() {
    // Speed 1: the zombie never gets to act here.
    let mut game = game(&OPEN, &[player(1, 2), zombie("zombie", 2, 2, 3, 1, 1)]);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &["move_e"]);
    let state = game.state();
    assert_eq!(state.zombies[0].hp, 2);
    assert_eq!(state.player.cell, [1, 2]);
    assert_eq!(state.turn, 1);
    assert_eq!(messages(&game), [(1, "You hit zombie.".to_owned())]);
    assert_eq!(sprite_tag(&game, "zombie"), "zombie");
}

#[test]
fn killed_zombie_becomes_a_corpse_that_does_not_act_and_can_be_walked_on() {
    // Speed 100 and adjacent: alive, it would bite this very turn.
    let mut game = game(&OPEN, &[player(1, 2), zombie("zombie", 2, 2, 1, 5, 100)]);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &["move_e"]);
    let state = game.state();
    assert_eq!(state.zombies[0].hp, 0);
    assert_eq!(state.zombies[0].energy, 0);
    assert_eq!(state.player.hp, 10);
    assert_eq!(messages(&game), [(1, "You kill zombie.".to_owned())]);
    assert_eq!(sprite_tag(&game, "zombie"), "corpse");
    let (x, y, _) = at(2, 2, ZOMBIE_Z);
    assert_eq!(
        position(&game, "zombie"),
        Vec3::new(x, y, (0.0 + ZOMBIE_Z) * 0.5)
    );
    // The corpse's cell is floor now.
    press(&mut game, &mut input, &[]);
    tick(&mut game, &mut input, &["move_e"]);
    assert_eq!(game.state().player.cell, [2, 2]);
    assert_eq!(game.state().turn, 2);
    assert_eq!(
        game.state().zombies[0].energy,
        0,
        "dead zombies gain no energy"
    );
}

#[test]
fn living_zombies_walk_over_corpses() {
    // `zombie` dies at (1, 0); `zombie_2` (speed 100) walks from (3, 0)
    // onto its corpse, then bites.
    let nodes = [
        player(0, 0),
        zombie("zombie", 1, 0, 1, 1, 1),
        zombie("zombie_2", 3, 0, 3, 2, 100),
    ];
    let mut game = game(&CORRIDOR, &nodes);
    let mut input = ActionState::new();
    press(&mut game, &mut input, &["move_e"]);
    assert_eq!(zombie_cell(&game, 1), [2, 0]);
    press(&mut game, &mut input, &[WAIT]);
    assert_eq!(zombie_cell(&game, 1), [1, 0]);
    press(&mut game, &mut input, &[WAIT]);
    assert_eq!(zombie_cell(&game, 1), [1, 0]);
    assert_eq!(game.state().player.hp, 8);
}

#[test]
fn zombie_with_speed_67_acts_on_turns_2_3_5_and_6() {
    let mut game = game(
        &CORRIDOR,
        &[player(0, 0), zombie("zombie", 19, 0, 3, 1, 67)],
    );
    let mut input = ActionState::new();
    let mut seen = Vec::new();
    for _ in 0..6 {
        press(&mut game, &mut input, &[WAIT]);
        let z = &game.state().zombies[0];
        seen.push((z.cell[0], z.energy));
    }
    // Energy +67 per turn, -100 per action: 67, 34, 1, 68, 35, 2.
    assert_eq!(
        seen,
        [(19, 67), (18, 34), (17, 1), (17, 68), (16, 35), (15, 2)]
    );
    assert_eq!(game.state().turn, 6);
    let (x, y, z) = at(15, 0, ZOMBIE_Z);
    assert_eq!(position(&game, "zombie"), Vec3::new(x, y, z));
}

#[test]
fn adjacent_zombie_bites_instead_of_moving() {
    // Diagonally adjacent.
    let mut game = game(&OPEN, &[player(1, 1), zombie("zombie", 2, 2, 3, 3, 100)]);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &[WAIT]);
    let state = game.state();
    assert_eq!(state.player.hp, 7);
    assert_eq!(state.zombies[0].cell, [2, 2]);
    assert_eq!(state.zombies[0].energy, 0);
    assert_eq!(messages(&game), [(1, "zombie bites you.".to_owned())]);
}

#[test]
fn player_at_zero_hp_dies_and_ignores_all_further_input() {
    let hp1 = actor(
        "player",
        "player",
        at(2, 2, PLAYER_Z),
        r#"("hp", 1), ("attack", 1)"#,
    );
    let nodes = [
        hp1,
        zombie("zombie", 1, 2, 3, 1, 100),
        zombie("zombie_2", 3, 2, 3, 1, 100),
    ];
    let mut game = game(&OPEN, &nodes);
    let mut input = ActionState::new();
    press(&mut game, &mut input, &[WAIT]);
    let state = game.state();
    assert_eq!(state.player.hp, 0);
    assert_eq!(
        messages(&game),
        [
            (1, "zombie bites you.".to_owned()),
            (1, "You die.".to_owned())
        ]
    );
    assert_eq!(sprite_tag(&game, "player"), "corpse");
    // zombie_2 gained its energy this turn but did not act on it.
    assert_eq!(state.zombies[1].energy, 100);
    // Nothing moves any more.
    for action in ["move_n", "move_e", WAIT] {
        press(&mut game, &mut input, &[action]);
    }
    assert_eq!(game.state(), state);
    let (x, y, z) = at(2, 2, PLAYER_Z);
    assert_eq!(position(&game, "player"), Vec3::new(x, y, z));
    assert_eq!(game.clock().tick(), 8);
}

#[test]
fn zombie_blocked_by_a_wall_stays_put() {
    // Every neighbour of (3, 2) is as far from the player at (0, 2) or
    // farther, except the walls: a straight chase gets stuck.
    let rows = [".....", "..#..", "..#..", "..#..", "....."];
    let mut game = game(&rows, &[player(0, 2), zombie("zombie", 3, 2, 3, 1, 100)]);
    let mut input = ActionState::new();
    for _ in 0..3 {
        press(&mut game, &mut input, &[WAIT]);
    }
    assert_eq!(zombie_cell(&game, 0), [3, 2]);
    assert_eq!(game.state().zombies[0].energy, 0);
}

#[test]
fn zombie_does_not_step_onto_a_living_zombie() {
    // `zombie` acts first and finds (2, 0) taken by `zombie_2`.
    let nodes = [
        player(0, 0),
        zombie("zombie", 3, 0, 3, 1, 100),
        zombie("zombie_2", 2, 0, 3, 1, 100),
    ];
    let mut game = game(&CORRIDOR, &nodes);
    let mut input = ActionState::new();
    press(&mut game, &mut input, &[WAIT]);
    assert_eq!(zombie_cell(&game, 0), [3, 0]);
    assert_eq!(zombie_cell(&game, 1), [1, 0]);
}

#[test]
fn zombies_act_in_scene_order_and_see_earlier_moves() {
    // `zombie` moves first and frees (2, 0) for `zombie_2` the same turn.
    let nodes = [
        player(0, 0),
        zombie("zombie", 2, 0, 3, 1, 100),
        zombie("zombie_2", 3, 0, 3, 1, 100),
    ];
    let mut game = game(&CORRIDOR, &nodes);
    let mut input = ActionState::new();
    press(&mut game, &mut input, &[WAIT]);
    assert_eq!(zombie_cell(&game, 0), [1, 0]);
    assert_eq!(zombie_cell(&game, 1), [2, 0]);
}

#[test]
fn wall_bump_gives_zombies_no_turn() {
    let mut game = game(
        &CORRIDOR,
        &[player(0, 0), zombie("zombie", 9, 0, 3, 1, 100)],
    );
    let mut input = ActionState::new();
    press(&mut game, &mut input, &["move_w"]);
    let state = game.state();
    assert_eq!(state.turn, 0);
    assert_eq!(state.zombies[0].cell, [9, 0]);
    assert_eq!(state.zombies[0].energy, 0);
}

// ---------------------------------------------------------------- events

fn hit(by: &str, target: &str, damage: i32, hp: i32, result: HitResult) -> Event {
    Event::Hit {
        by: by.to_owned(),
        target: target.to_owned(),
        damage,
        hp,
        result,
    }
}

#[test]
fn player_bumping_a_zombie_emits_one_hit_then_the_killing_one() {
    // hp 2, the player's attack 1; speed 1: the zombie never acts.
    let mut game = game(&OPEN, &[player(1, 2), zombie("zombie", 2, 2, 2, 1, 1)]);
    let mut input = ActionState::new();
    assert!(game.events().is_empty());
    tick(&mut game, &mut input, &["move_e"]);
    assert_eq!(
        game.events(),
        [hit("player", "zombie", 1, 1, HitResult::Hit)]
    );
    tick(&mut game, &mut input, &[]);
    assert!(game.events().is_empty());
    tick(&mut game, &mut input, &["move_e"]);
    assert_eq!(
        game.events(),
        [hit("player", "zombie", 1, 0, HitResult::Death)]
    );
    assert_eq!(
        messages(&game),
        [
            (1, "You hit zombie.".to_owned()),
            (2, "You kill zombie.".to_owned())
        ]
    );
}

#[test]
fn zombie_bites_emit_a_hit_each_and_the_killing_one_a_death() {
    // Both adjacent with speed 100, attack 3: the player (hp 10) is bitten
    // to 7 and 4 on turn 1; on turn 2 to 1, then 0.
    let nodes = [
        player(2, 2),
        zombie("zombie", 1, 2, 3, 3, 100),
        zombie("zombie_2", 3, 2, 3, 3, 100),
    ];
    let mut game = game(&OPEN, &nodes);
    let mut input = ActionState::new();
    tick(&mut game, &mut input, &[WAIT]);
    assert_eq!(
        game.events(),
        [
            hit("zombie", "player", 3, 7, HitResult::Hit),
            hit("zombie_2", "player", 3, 4, HitResult::Hit),
        ]
    );
    tick(&mut game, &mut input, &[]);
    tick(&mut game, &mut input, &[WAIT]);
    assert_eq!(
        game.events(),
        [
            hit("zombie", "player", 3, 1, HitResult::Hit),
            hit("zombie_2", "player", 3, 0, HitResult::Death),
        ]
    );
    assert_eq!(messages(&game).last().unwrap().1, "You die.");
}

#[test]
fn walking_waiting_and_bumping_walls_emit_no_events() {
    // The zombie far away steps closer on each turn but never bites.
    let mut game = game(
        &CORRIDOR,
        &[player(0, 0), zombie("zombie", 19, 0, 3, 1, 100)],
    );
    let mut input = ActionState::new();
    for action in ["move_w", "move_e", WAIT] {
        tick(&mut game, &mut input, &[action]);
        assert!(game.events().is_empty(), "{action}");
        tick(&mut game, &mut input, &[]);
        assert!(game.events().is_empty(), "{action}");
    }
    assert_eq!(game.state().turn, 2);
    assert_eq!(zombie_cell(&game, 0), [17, 0]);
}

// ---------------------------------------------------------------- scene errors

/// The OPEN map with the player on (0, 0) and one zombie on (4, 4).
fn valid() -> String {
    scene(&OPEN, &[player(0, 0), zombie("zombie", 4, 4, 3, 1, 67)])
}

/// `valid()` with `from` replaced once by `to`.
fn valid_with(from: &str, to: &str) -> String {
    let src = valid();
    assert!(src.contains(from), "{from}");
    src.replacen(from, to, 1)
}

#[test]
fn scene_without_a_map_tilemap_is_rejected() {
    let src = valid_with("(name: \"map\"", "(name: \"ground\"");
    assert_eq!(
        error_of(&src),
        "the scene needs a root node named \"map\" with a tilemap"
    );
}

#[test]
fn scene_without_a_player_sprite_is_rejected() {
    let src = valid_with("(name: \"player\"", "(name: \"hero\"");
    assert_eq!(
        error_of(&src),
        "the scene needs a root node named \"player\" with a sprite"
    );
}

#[test]
fn zombie_without_a_sprite_is_rejected() {
    let src = scene(
        &OPEN,
        &[
            player(0, 0),
            r#"(name: "zombie_7", position: (14.0, 1.0, 0.5))"#.to_owned(),
        ],
    );
    assert_eq!(error_of(&src), "\"zombie_7\" needs a sprite");
}

#[test]
fn rotated_or_scaled_characters_are_rejected() {
    let expected = |name: &str| {
        format!("\"{name}\" must only be translated: no rotation or look_at, scale (1, 1, 1)")
    };
    let rotated_map = valid_with(
        "(name: \"map\", position: (10.0, 5.0, 0.0)",
        "(name: \"map\", position: (10.0, 5.0, 0.0), rotation: (0.0, 0.0, 90.0)",
    );
    assert_eq!(error_of(&rotated_map), expected("map"));
    let scaled_zombie = valid_with(
        "(name: \"zombie\", position: (14.0, 1.0, 0.5)",
        "(name: \"zombie\", position: (14.0, 1.0, 0.5), scale: (2.0, 2.0, 1.0)",
    );
    assert_eq!(error_of(&scaled_zombie), expected("zombie"));
}

#[test]
fn character_off_the_cell_grid_is_rejected() {
    let between = valid_with("position: (10.0, 5.0, 1.0)", "position: (10.5, 3.0, 1.0)");
    assert_eq!(
        error_of(&between),
        "\"player\" at (10.5, 3) is not on a map cell: x - map.x and map.y - y must be whole \
         numbers inside the 5x5 map"
    );
    let outside = valid_with("position: (14.0, 1.0, 0.5)", "position: (15.0, 1.0, 0.5)");
    assert_eq!(
        error_of(&outside),
        "\"zombie\" at (15, 1) is not on a map cell: x - map.x and map.y - y must be whole \
         numbers inside the 5x5 map"
    );
}

#[test]
fn character_starting_on_a_wall_is_rejected() {
    let src = scene(&CORNER, &[player(2, 2)]);
    assert_eq!(
        error_of(&src),
        "\"player\" starts on a wall at column 2, row 2"
    );
    let src = scene(&CORNER, &[player(0, 0), zombie("zombie", 1, 1, 3, 1, 67)]);
    assert_eq!(
        error_of(&src),
        "\"zombie\" starts on a wall at column 1, row 1"
    );
}

#[test]
fn two_characters_on_one_cell_are_rejected() {
    let src = scene(&OPEN, &[player(2, 2), zombie("zombie", 2, 2, 3, 1, 67)]);
    assert_eq!(
        error_of(&src),
        "\"zombie\" starts on the same cell as \"player\""
    );
    let src = scene(
        &OPEN,
        &[
            player(0, 0),
            zombie("zombie", 3, 3, 3, 1, 67),
            zombie("zombie_2", 3, 3, 3, 1, 67),
        ],
    );
    assert_eq!(
        error_of(&src),
        "\"zombie_2\" starts on the same cell as \"zombie\""
    );
}

#[test]
fn player_not_in_front_of_the_map_is_rejected() {
    let src = scene(
        &OPEN,
        &[actor("player", "player", at(0, 0, 0.0), PLAYER_PROPS)],
    );
    assert_eq!(
        error_of(&src),
        "the player must be in front of the map: its z (0) must be greater than the map's (0)"
    );
}

#[test]
fn zombie_not_between_the_map_and_the_player_is_rejected() {
    let src = valid_with("position: (14.0, 1.0, 0.5)", "position: (14.0, 1.0, 1.0)");
    assert_eq!(
        error_of(&src),
        "\"zombie\" must be between the map and the player: its z (1) must be greater than the \
         map's (0) and less than the player's (1)"
    );
}

#[test]
fn zombies_at_different_depths_are_rejected() {
    let other = actor(
        "zombie_2",
        "zombie",
        at(3, 3, 0.25),
        r#"("hp", 3), ("attack", 1), ("speed", 67)"#,
    );
    let src = scene(
        &OPEN,
        &[player(0, 0), zombie("zombie", 4, 4, 3, 1, 67), other],
    );
    assert_eq!(
        error_of(&src),
        "\"zombie_2\" must have the same z as \"zombie\" (0.5), not 0.25"
    );
}

#[test]
fn character_without_a_corpse_alternate_is_rejected() {
    let src = valid_with(
        "tag: \"player\", alternates: [\"corpse\"]",
        "tag: \"player\"",
    );
    assert_eq!(
        error_of(&src),
        "\"player\": its sprite needs \"corpse\" in alternates, shown when it dies"
    );
}

#[test]
fn missing_prop_is_rejected() {
    let src = valid_with(", (\"speed\", 67)", "");
    assert_eq!(error_of(&src), "\"zombie\": prop speed is missing");
}

#[test]
fn prop_that_is_not_a_whole_number_from_1_to_1000_is_rejected() {
    for (value, shown) in [("0", "0"), ("2.5", "2.5"), ("1001", "1001")] {
        let src = valid_with("(\"hp\", 3)", &format!("(\"hp\", {value})"));
        assert_eq!(
            error_of(&src),
            format!("\"zombie\": prop hp must be a whole number from 1 to 1000, got {shown}")
        );
    }
}

#[test]
fn unknown_prop_is_rejected_listing_the_known_ones() {
    let src = valid_with("(\"attack\", 1)]", "(\"attack\", 1), (\"sped\", 1)]");
    assert_eq!(
        error_of(&src),
        "\"player\": unknown prop \"sped\" (known: hp, attack)"
    );
    let src = valid_with("(\"speed\", 67)", "(\"speed\", 67), (\"range\", 2)");
    assert_eq!(
        error_of(&src),
        "\"zombie\": unknown prop \"range\" (known: hp, attack, speed)"
    );
}

#[test]
fn scene_checks_run_in_the_documented_order() {
    // The player is off the grid and the zombie stands on a wall: the
    // player is checked first.
    let src = scene(
        &CORNER,
        &[
            actor("player", "player", (10.5, 5.0, 1.0), PLAYER_PROPS),
            zombie("zombie", 1, 1, 3, 1, 67),
        ],
    );
    assert!(error_of(&src).starts_with("\"player\" at (10.5, 5)"));
    // One character both rotated and off the grid: translation comes first.
    let src = valid_with(
        "(name: \"zombie\", position: (14.0, 1.0, 0.5)",
        "(name: \"zombie\", position: (14.5, 1.0, 0.5), rotation: (0.0, 0.0, 90.0)",
    );
    assert!(error_of(&src).starts_with("\"zombie\" must only be translated"));
}

// ---------------------------------------------------------------- the project scene

const ROGUE_SCENE: &str = include_str!("../../../assets/scenes/rogue.ron");

#[test]
fn project_rogue_scene_state_at_tick_zero_serializes_as_expected() {
    let game = Game::from_source(ROGUE_SCENE).unwrap();
    assert_eq!(game.clock().tick(), 0);
    assert_eq!(game.scene().game(), Some("rogue"));
    assert_eq!(
        serde_json::to_string(&game.state()).unwrap(),
        concat!(
            r#"{"turn":0,"player":{"cell":[3,2],"hp":10},"#,
            r#""zombies":[{"name":"zombie","cell":[7,2],"hp":3,"energy":0},"#,
            r#"{"name":"zombie_2","cell":[12,7],"hp":3,"energy":0}],"messages":[]}"#
        )
    );
}

#[test]
fn project_scene_prototype_run_ends_in_the_prototype_state() {
    // The planner's prototype: wait x5, move_e x4, move_n, wait x3.
    let mut game = Game::from_source(ROGUE_SCENE).unwrap();
    let mut input = ActionState::new();
    let keys = [
        WAIT, WAIT, WAIT, WAIT, WAIT, "move_e", "move_e", "move_e", "move_e", "move_n", WAIT, WAIT,
        WAIT,
    ];
    for key in keys {
        press(&mut game, &mut input, &[key]);
    }
    assert_eq!(
        serde_json::to_string(&game.state()).unwrap(),
        concat!(
            r#"{"turn":13,"player":{"cell":[4,1],"hp":9},"#,
            r#""zombies":[{"name":"zombie","cell":[4,2],"hp":0,"energy":69},"#,
            r#"{"name":"zombie_2","cell":[5,1],"hp":3,"energy":71}],"#,
            r#""messages":[{"turn":6,"text":"You hit zombie."},{"turn":6,"text":"zombie bites you."},"#,
            r#"{"turn":7,"text":"You hit zombie."},{"turn":8,"text":"You kill zombie."}]}"#
        )
    );
    assert_eq!(position(&game, "player"), Vec3::new(4.0, -1.0, 1.0));
    assert_eq!(position(&game, "zombie"), Vec3::new(4.0, -2.0, 0.25));
    assert_eq!(position(&game, "zombie_2"), Vec3::new(5.0, -1.0, 0.5));
    assert_eq!(sprite_tag(&game, "zombie"), "corpse");
}
