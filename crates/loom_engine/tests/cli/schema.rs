//! `loom schema`: the committed schema, and scenes and state dumps that
//! fit it.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::common::{assert_fits, committed_schema, repo_root, report, run_loom, validator};
use super::rogue::ROGUE_STATE;
use super::run::{SCENARIO_A, SCENARIO_B};
use super::souls_moves::{
    SOULS_ATTACKS_STATE, SOULS_COMBO_HITS_STATE, SOULS_KILL_STATE, SOULS_LOCK_STATE,
    SOULS_MOVES_STATE, SOULS_STAKE_STATE,
};

#[test]
fn loom_schema_prints_the_committed_schema_the_same_on_every_run() {
    let first = run_loom(&["schema"]);
    let second = run_loom(&["schema"]);
    assert_eq!(first.code, 0);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stdout.lines().count(), 1);
    assert!(first.stdout.ends_with("}\n"));
    let r = report(&first);
    assert_eq!(r["status"], "ok");
    assert_eq!(r.as_object().expect("an object").len(), 2);
    assert_eq!(r["schema"], committed_schema());
}

#[test]
fn every_repository_scene_fits_the_scene_schema_and_broken_ones_do_not() {
    let root = repo_root();
    let mut files: Vec<PathBuf> = ["demo", "g2_3_demo", "g3_3_demo"]
        .iter()
        .map(|name| root.join(format!("games/souls/src/{name}.scene.ron")))
        .collect();
    let mut assets: Vec<PathBuf> = fs::read_dir(root.join("assets/scenes"))
        .expect("assets/scenes exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ron"))
        .collect();
    assets.sort();
    assert_eq!(assets.len(), 10, "{assets:?}");
    files.extend(assets);
    let scene = validator(&["scene"]);
    let as_json = |file: &PathBuf| -> Value {
        let text = fs::read_to_string(file).expect("the scene can be read");
        let doc = loom_scene::SceneDoc::parse(&text).expect("the scene parses");
        serde_json::to_value(&doc).expect("a scene is plain data")
    };
    for file in &files {
        assert_fits(&scene, &as_json(file), &file.display().to_string());
    }

    let demo = as_json(&files[0]);
    assert!(scene.is_valid(&demo));
    let mut misspelled = demo.clone();
    misspelled["nodes"][1]["positon"] = serde_json::json!([0.0, 0.0, 0.0]);
    assert!(!scene.is_valid(&misspelled));
    let mut unknown = demo.clone();
    unknown["gravity"] = serde_json::json!(9.8);
    assert!(!scene.is_valid(&unknown));
    let mut flat = demo.clone();
    assert_eq!(flat["nodes"][1]["name"], "player");
    flat["nodes"][1]["position"] = serde_json::json!([0.0, 0.0]);
    assert!(!scene.is_valid(&flat));
    let mut mesh = demo;
    mesh["nodes"][0]["mesh"] = serde_json::json!("Cone");
    assert!(!scene.is_valid(&mesh));
}

#[test]
fn every_expected_state_dump_fits_its_games_state_dump_schema() {
    let dumps = [
        ("souls", "SCENARIO_A", SCENARIO_A),
        ("souls", "SCENARIO_B", SCENARIO_B),
        ("souls", "SOULS_MOVES_STATE", SOULS_MOVES_STATE),
        ("souls", "SOULS_ATTACKS_STATE", SOULS_ATTACKS_STATE),
        ("souls", "SOULS_LOCK_STATE", SOULS_LOCK_STATE),
        ("souls", "SOULS_COMBO_HITS_STATE", SOULS_COMBO_HITS_STATE),
        ("souls", "SOULS_KILL_STATE", SOULS_KILL_STATE),
        ("souls", "SOULS_STAKE_STATE", SOULS_STAKE_STATE),
        ("rogue", "ROGUE_STATE", ROGUE_STATE),
    ];
    let souls = validator(&["games", "souls", "state_dump"]);
    let rogue = validator(&["games", "rogue", "state_dump"]);
    for (game, name, text) in dumps {
        let dump: Value = serde_json::from_str(text).expect("the expected dump is JSON");
        let (own, other) = match game {
            "souls" => (&souls, &rogue),
            _ => (&rogue, &souls),
        };
        assert_fits(own, &dump, name);
        assert!(!other.is_valid(&dump), "{name} fits the other game too");
    }

    // The schema asks for the fields: one taken out no longer fits.
    let mut dump: Value = serde_json::from_str(SOULS_MOVES_STATE).expect("JSON");
    assert!(souls.is_valid(&dump));
    dump["game"]["player"]
        .as_object_mut()
        .expect("the player is an object")
        .remove("stamina");
    assert!(!souls.is_valid(&dump));
}
