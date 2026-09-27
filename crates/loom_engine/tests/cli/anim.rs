//! Animation clips (`docs/architecture.md` section 19, "骨骼和动画片段"):
//! `view --ticks` arguments, clip load errors, and the clip schema. The
//! film strip itself needs a GPU: smoke H13 checks its size and report.

use std::fs;

use serde_json::Value;

use super::common::{assert_fits, repo_root, report, run_loom, run_loom_in, test_dir, validator};

fn view_with_ticks(ticks: &str) -> String {
    // `--ticks=` so a value starting with '-' is not taken for an option.
    let ticks = format!("--ticks={ticks}");
    let out = run_loom(&[
        "view",
        "--scene",
        "assets/scenes/anim_preview.ron",
        &ticks,
        "--screenshot",
        "out/never.png",
    ]);
    assert_eq!(out.code, 2, "{}", out.stdout);
    let r = report(&out);
    assert_eq!(r["status"], "error");
    r["error"].as_str().expect("an error").to_owned()
}

#[test]
fn view_ticks_that_are_empty_not_numbers_or_too_many_are_argument_errors() {
    for bad in ["", "0,,15", "0,x", "-1", "1.5"] {
        let error = view_with_ticks(bad);
        assert!(
            error.contains("expected ticks as comma-separated non-negative integers"),
            "{bad:?}: {error}"
        );
    }
    let seventeen = vec!["0"; 17].join(",");
    let error = view_with_ticks(&seventeen);
    assert!(error.contains("at most 16 ticks, got 17"), "{error}");
}

#[test]
fn view_of_an_animated_node_with_a_missing_clip_fails_before_rendering() {
    let dir = test_dir("anim_missing_clip_view");
    let model = repo_root().join("assets/models/humanoid.glb");
    let model = model.display().to_string().replace('\\', "/");
    fs::write(
        dir.join("s.ron"),
        format!(
            r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
            (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
            (name: "k", model: {model:?}, animation: (clips: [("idle", "idle.ron")], play: "idle")),
        ])"#
        ),
    )
    .expect("writing the scene");
    let out = run_loom_in(
        &dir,
        &[
            "view",
            "--scene",
            "s.ron",
            "--ticks",
            "0,30",
            "--screenshot",
            "v.png",
        ],
    );
    assert_eq!(out.code, 1);
    let error = report(&out)["error"].as_str().expect("an error").to_owned();
    assert!(
        error.starts_with(
            "invalid scene s.ron: node \"k\": animation clip \"idle\" idle.ron: reading the file: "
        ),
        "{error}"
    );
    assert!(!dir.join("v.png").exists());
}

#[test]
fn every_repository_clip_fits_the_clip_schema_and_broken_ones_do_not() {
    let clip = validator(&["clip"]);
    let dir = repo_root().join("assets/anims/humanoid");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("assets/anims/humanoid exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ron"))
        .collect();
    files.sort();
    let names: Vec<_> = files
        .iter()
        .map(|p| {
            p.file_name()
                .expect("a file")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["idle.ron", "walk.ron"]);
    let as_json = |text: &str| -> Value {
        let value: ron::Value = ron::from_str(text).expect("the clip is RON");
        serde_json::to_value(&value).expect("RON values are plain data")
    };
    for file in &files {
        let text = fs::read_to_string(file).expect("the clip can be read");
        loom_scene::Clip::parse(&text).expect("the clip loads");
        assert_fits(&clip, &as_json(&text), &file.display().to_string());
    }
    let good = as_json(
        r#"(length: 10, looping: true, bones: {"hips": (rotation: [(0, (1.0, 2.0, 3.0))])})"#,
    );
    assert!(clip.is_valid(&good));
    let scaled =
        as_json(r#"(length: 10, looping: true, bones: {"hips": (scale: [(0, (1.0, 1.0, 1.0))])})"#);
    assert!(!clip.is_valid(&scaled));
    let empty = as_json("(length: 0, looping: true, bones: {})");
    assert!(!clip.is_valid(&empty));
    let flat = as_json(
        r#"(length: 10, looping: true, bones: {"hips": (translation: [(0, (1.0, 2.0))])})"#,
    );
    assert!(!clip.is_valid(&flat));
}

/// The rogue scene with an animated knight, relative to the repository root.
const ROGUE_KNIGHT: &str = "crates/loom_engine/tests/cli/rogue_knight.ron";

/// Runs [`ROGUE_KNIGHT`] headless for 30 ticks with `extra` arguments and
/// returns its state dump and trace.
fn rogue_knight_run(name: &str, extra: &[&str]) -> (String, String) {
    let dir = test_dir(name);
    let (state, trace) = (dir.join("state.json"), dir.join("trace.jsonl"));
    let (state_arg, trace_arg) = (state.display().to_string(), trace.display().to_string());
    let mut args = vec![
        "run",
        "--headless",
        "--frames",
        "30",
        "--scene",
        ROGUE_KNIGHT,
        "--dump-state",
        &state_arg,
        "--trace",
        &trace_arg,
    ];
    args.extend(extra);
    let out = run_loom_in(&repo_root(), &args);
    assert_eq!(out.code, 0, "{}", out.stdout);
    (
        fs::read_to_string(&state).expect("the state dump was written"),
        fs::read_to_string(&trace).expect("the trace was written"),
    )
}

#[test]
fn state_dump_shows_what_animated_nodes_play_and_nothing_on_other_nodes() {
    let (state, _) = rogue_knight_run("anim_state_dump", &[]);
    let dump: Value = serde_json::from_str(&state).expect("the dump is JSON");
    assert_eq!(
        dump["nodes"]["knight"]["animation"],
        serde_json::json!({"clip": "idle", "start": 0, "from": null, "blend": 0})
    );
    let nodes = dump["nodes"].as_object().expect("nodes");
    let animated: Vec<&String> = nodes
        .iter()
        .filter(|(_, node)| node.get("animation").is_some())
        .map(|(path, _)| path)
        .collect();
    assert_eq!(animated, ["knight"]);
    // The key comes after the transform.
    let knight = &state[state.find("\"knight\": {").expect("the knight")..];
    let at = |key: &str| knight.find(key).expect("a key of the knight");
    assert!(at("\"scale\"") < at("\"animation\""));
    assert_fits(
        &validator(&["games", "rogue", "state_dump"]),
        &dump,
        "rogue knight state dump",
    );
}

#[test]
fn drawing_the_skeleton_changes_neither_state_dump_nor_trace() {
    let plain = rogue_knight_run("anim_skeleton_plain", &[]);
    let drawn = rogue_knight_run("anim_skeleton_drawn", &["--debug-draw=skeleton"]);
    assert_eq!(plain, drawn);
}

#[test]
fn view_debug_draw_has_only_the_skeleton() {
    let out = run_loom(&[
        "view",
        "--scene",
        "assets/scenes/anim_preview.ron",
        "--debug-draw=lock",
        "--screenshot",
        "out/never.png",
    ]);
    assert_eq!(out.code, 2, "{}", out.stdout);
    assert_eq!(
        report(&out),
        serde_json::json!({
            "status": "error",
            "error": r#"unknown debug draw category "lock": view has "skeleton""#
        })
    );
}

#[test]
fn clip_of_an_animation_the_model_lacks_exits_1_listing_what_it_has() {
    let dir = test_dir("clip_missing_animation");
    let out_path = dir.join("wave.ron");
    let out = run_loom(&[
        "clip",
        "assets/models/humanoid.glb",
        "--animation",
        "wave",
        "--out",
        &out_path.display().to_string(),
    ]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert_eq!(
        report(&out),
        serde_json::json!({
            "status": "error",
            "error": r#"model assets/models/humanoid.glb: the model has no animation "wave": it has none"#
        })
    );
    assert!(!out_path.exists());
}
