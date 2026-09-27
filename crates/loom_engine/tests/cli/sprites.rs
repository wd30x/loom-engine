//! Sprites and sheets: `loom view`, tags, alternates, missing sheets.

use serde_json::Value;

use super::common::{path_str, report, run_loom, test_dir, write_file};
use super::scene::MINIMAL_SCENE;

/// The project's tile sheet as an absolute path with forward slashes, so it
/// can be written into a RON string on any platform.
fn tiles_sheet() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/art/tiles.json")
        .display()
        .to_string()
        .replace('\\', "/")
}

/// A 2D scene (no character) whose tilemap `map` uses `sheet` and draws
/// `'.'` with `floor_tag`.
fn tilemap_scene(sheet: &str, floor_tag: &str) -> String {
    format!(
        r#"(
    version: 1,
    camera: "cam",
    ambient: (1.0, 1.0, 1.0),
    nodes: [
        (name: "cam", position: (0.0, 0.0, 10.0), camera: (ortho: (height: 4.0), near: 0.1, far: 100.0)),
        (name: "map", tilemap: (sheet: "{sheet}", legend: [('.', "{floor_tag}")], rows: [".."])),
    ],
)
"#
    )
}

/// [`MINIMAL_SCENE`] plus a sprite node `badge` showing `tag` of the tile sheet.
fn scene_with_sprite(tag: &str) -> String {
    let sprite = format!(
        "        (name: \"badge\", position: (0.0, 2.0, 0.0), sprite: (sheet: \"{}\", tag: \"{tag}\")),\n    ],",
        tiles_sheet()
    );
    MINIMAL_SCENE.replacen("\n    ],", &format!("\n{sprite}"), 1)
}

#[test]
fn view_without_screenshot_is_an_argument_error() {
    let out = run_loom(&["view"]);
    let r = report(&out);
    assert_eq!(out.code, 2);
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains("--screenshot"), "{error}");
}

#[test]
fn view_with_a_misspelled_tag_fails_before_rendering() {
    let dir = test_dir("view_with_a_misspelled_tag_fails_before_rendering");
    let scene = write_file(
        &dir,
        "map.scene.ron",
        &tilemap_scene(&tiles_sheet(), "flor"),
    );
    let shot = dir.join("shot.png");
    let out = run_loom(&[
        "view",
        "--scene",
        path_str(&scene),
        "--screenshot",
        path_str(&shot),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    let expected = format!(
        "node map: tilemap: sheet {} has no tag \"flor\"",
        tiles_sheet()
    );
    assert!(
        error.contains(&expected),
        "error should contain {expected:?}: {error}"
    );
    assert!(!shot.exists(), "no screenshot is written");
}

#[test]
fn view_with_a_missing_sheet_is_a_runtime_error() {
    let dir = test_dir("view_with_a_missing_sheet_is_a_runtime_error");
    let missing = dir
        .join("nowhere.json")
        .display()
        .to_string()
        .replace('\\', "/");
    let scene = write_file(&dir, "map.scene.ron", &tilemap_scene(&missing, "floor"));
    let out = run_loom(&[
        "view",
        "--scene",
        path_str(&scene),
        "--screenshot",
        path_str(&dir.join("shot.png")),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    let error = r["error"].as_str().expect("error is a string");
    for part in ["node map: tilemap", missing.as_str()] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn headless_run_without_screenshot_still_checks_sprite_tags() {
    let dir = test_dir("headless_run_without_screenshot_still_checks_sprite_tags");
    let scene = write_file(&dir, "badge.scene.ron", &scene_with_sprite("playr"));
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    let error = r["error"].as_str().expect("error is a string");
    for part in [
        path_str(&scene),
        "node badge: sprite",
        "has no tag \"playr\"",
    ] {
        assert!(
            error.contains(part),
            "error should contain {part:?}: {error}"
        );
    }
}

#[test]
fn headless_run_accepts_a_scene_with_a_valid_sprite() {
    let dir = test_dir("headless_run_accepts_a_scene_with_a_valid_sprite");
    let scene = write_file(&dir, "badge.scene.ron", &scene_with_sprite("player"));
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 0, "{r}");
    assert_eq!(r["adapter"], Value::Null);
}

#[test]
fn headless_run_checks_sprite_alternates_against_the_sheet() {
    let dir = test_dir("headless_run_checks_sprite_alternates_against_the_sheet");
    let src = scene_with_sprite("player").replacen(
        "tag: \"player\")",
        "tag: \"player\", alternates: [\"corpes\"])",
        1,
    );
    assert!(src.contains("corpes"));
    let scene = write_file(&dir, "badge.scene.ron", &src);
    let out = run_loom(&[
        "run",
        "--headless",
        "--frames",
        "1",
        "--scene",
        path_str(&scene),
    ]);
    let r = report(&out);
    assert_eq!(out.code, 1, "{r}");
    assert_eq!(
        r["error"],
        format!(
            "invalid scene {}: node badge: sprite: sheet {} has no tag \"corpes\"",
            path_str(&scene),
            tiles_sheet()
        )
    );
}

#[test]
fn view_without_scene_is_an_argument_error() {
    let dir = test_dir("view_without_scene_is_an_argument_error");
    let shot = dir.join("shot.png");
    let out = run_loom(&["view", "--screenshot", path_str(&shot)]);
    let r = report(&out);
    assert_eq!(out.code, 2, "{r}");
    assert_eq!(r["status"], "error");
    let error = r["error"].as_str().expect("error is a string");
    assert!(error.contains("--scene"), "{error}");
    assert!(!shot.exists());
}
