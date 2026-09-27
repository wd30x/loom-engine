//! The HUD: its report field and the HUD font errors.

use std::fs;

use super::common::{repo_root, report, run_loom, run_loom_in, test_dir};

#[test]
fn hud_report_is_true_unless_no_hud_is_given() {
    // The renders themselves are checked by smoke (H11): this needs no GPU.
    for (flag, expected) in [(None, true), (Some("--no-hud"), false)] {
        let mut args = vec!["run", "--headless", "--frames", "1"];
        args.extend(flag);
        let out = run_loom(&args);
        let r = report(&out);
        assert_eq!(out.code, 0, "{r}");
        assert_eq!(r["hud"], expected, "{flag:?}");
    }
}

/// The error `loom run` reports in `dir`, with `extra` arguments.
fn run_error_in(dir: &std::path::Path, extra: &[&str]) -> String {
    let mut args = vec!["run", "--headless", "--frames", "1"];
    args.extend_from_slice(extra);
    let out = run_loom_in(dir, &args);
    assert_eq!(out.code, 1, "{}", out.stdout);
    let r = report(&out);
    assert_eq!(r["status"], "error");
    r["error"].as_str().expect("error is a string").to_owned()
}

#[test]
fn missing_hud_font_fails_the_run_even_without_the_hud() {
    let dir = test_dir("font_missing");
    for extra in [&[][..], &["--no-hud"]] {
        let error = run_error_in(&dir, extra);
        assert!(
            error.starts_with("the HUD font assets/fonts/loom_5x7.json: reading the JSON: "),
            "{error}"
        );
    }
}

#[test]
fn hud_font_of_the_wrong_size_fails_the_run_even_without_the_hud() {
    // The 80x16 tile sheet in the font's place.
    let dir = test_dir("font_size");
    let fonts = dir.join("assets/fonts");
    fs::create_dir_all(&fonts).expect("the fonts directory can be created");
    let art = repo_root().join("assets/art");
    fs::copy(art.join("tiles.json"), fonts.join("loom_5x7.json")).expect("copying tiles.json");
    fs::copy(art.join("tiles.png"), fonts.join("tiles.png")).expect("copying tiles.png");
    for extra in [&[][..], &["--no-hud"]] {
        assert_eq!(
            run_error_in(&dir, extra),
            "the HUD font assets/fonts/loom_5x7.json: the image is 80x16, but the glyph layout needs 96x48"
        );
    }
}
