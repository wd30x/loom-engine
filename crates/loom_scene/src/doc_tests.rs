//! The canonical format and the edits of [`SceneDoc`]
//! (`docs/architecture.md` sections 10 and 13).

use std::collections::BTreeMap;

use super::*;

const SCENES: [(&str, &str); 6] = [
    (
        "souls demo",
        include_str!("../../../games/souls/src/demo.scene.ron"),
    ),
    (
        "g2_3 demo",
        include_str!("../../../games/souls/src/g2_3_demo.scene.ron"),
    ),
    (
        "g3_3 demo",
        include_str!("../../../games/souls/src/g3_3_demo.scene.ron"),
    ),
    ("rogue", include_str!("../../../assets/scenes/rogue.ron")),
    ("boss", include_str!("../../../assets/scenes/boss.ron")),
    (
        "tiles preview",
        include_str!("../../../assets/scenes/tiles_preview.ron"),
    ),
];

/// Equal, and equal bit for bit: `Debug` tells `-0.0` from `0.0`.
fn assert_same_scene(a: &Scene, b: &Scene, what: &str) {
    assert!(a == b, "{what}");
    assert_eq!(format!("{a:?}"), format!("{b:?}"), "{what}");
}

#[test]
fn every_repository_scene_reads_back_the_same_from_its_canonical_text() {
    for (what, src) in SCENES {
        let canonical = SceneDoc::parse(src).unwrap().to_ron();
        assert_same_scene(
            &Scene::parse(&canonical).unwrap(),
            &Scene::parse(src).unwrap(),
            what,
        );
    }
}

#[test]
fn canonical_text_of_every_repository_scene_is_idempotent() {
    for (what, src) in SCENES {
        let once = SceneDoc::parse(src).unwrap().to_ron();
        let twice = SceneDoc::parse(&once).unwrap().to_ron();
        assert_eq!(once, twice, "{what}");
        assert!(once.ends_with(")\n") && !once.contains('\r'), "{what}");
    }
}

#[test]
fn canonical_text_orders_fields_drops_comments_and_empty_lists() {
    let src = r#"// A comment that is lost.
(
    version: 1, game: "souls", camera: "cam",
    ambient: (0.2, 0.2, 0.25),
    nodes: [
        (name: "player", collider: Box, props: [("hp", 100), ("poise", 0.5)],
            mesh: Capsule(radius: 0.35, height: 1.8), position: (0.0, 0.0, 0.0),
            children: [(name: "visor", scale: (1.0, 1.0, 1.0), mesh: Cube, color: (0.1, 0.2, 0.3))]),
        (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0), rotation: (-15.0, 0, 0)),
        (name: "hero", sprite: (sheet: "tiles.json", tag: "player", alternates: [])),
    ],
)"#;
    assert_eq!(
        SceneDoc::parse(src).unwrap().to_ron(),
        r#"(
    version: 1,
    game: "souls",
    camera: "cam",
    ambient: (0.2, 0.2, 0.25),
    nodes: [
        (
            name: "player",
            position: (0.0, 0.0, 0.0),
            mesh: Capsule(
                radius: 0.35,
                height: 1.8,
            ),
            props: [
                ("hp", 100.0),
                ("poise", 0.5),
            ],
            collider: Box,
            children: [
                (
                    name: "visor",
                    scale: (1.0, 1.0, 1.0),
                    mesh: Cube,
                    color: (0.1, 0.2, 0.3),
                ),
            ],
        ),
        (
            name: "cam",
            rotation: (-15.0, 0.0, 0.0),
            camera: (
                fov_y: 60.0,
                near: 0.1,
                far: 100.0,
            ),
        ),
        (
            name: "hero",
            sprite: (
                sheet: "tiles.json",
                tag: "player",
            ),
        ),
    ],
)
"#
    );
}

// ---------------------------------------------------------------- edits

/// A small scene: a camera, and `box` with a child `lid`.
const SMALL: &str = r#"(version: 1, camera: "cam", ambient: (0.2, 0.2, 0.2), nodes: [
    (name: "cam", camera: (fov_y: 60.0, near: 0.1, far: 100.0)),
    (name: "box", position: (1.0, 0.0, 0.0), mesh: Cube, props: [("b", 2), ("a", 1)],
        children: [(name: "lid", mesh: Cube)]),
])"#;

fn small() -> SceneDoc {
    SceneDoc::parse(SMALL).unwrap()
}

fn no_props() -> BTreeMap<String, Option<f64>> {
    BTreeMap::new()
}

/// The canonical RON of the node at `path`, without children.
fn ron_of(doc: &SceneDoc, path: &str) -> String {
    doc.node_ron(path).unwrap().0
}

fn edit_error(set: Option<&str>, unset: &[&str], props: &[(&str, Option<f64>)]) -> String {
    let unset: Vec<String> = unset.iter().map(|s| (*s).to_owned()).collect();
    let props = props.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect();
    small().edit_node("box", set, &unset, &props).unwrap_err().0
}

#[test]
fn set_replaces_fields_and_unset_removes_them() {
    let mut doc = small();
    doc.edit_node(
        "box",
        Some("position: (3.0, 0.5, -1.0), color: (1.0, 0.0, 0.0)"),
        &["mesh".to_owned()],
        &no_props(),
    )
    .unwrap();
    assert_eq!(
        ron_of(&doc, "box"),
        "(\n    name: \"box\",\n    position: (3.0, 0.5, -1.0),\n    color: (1.0, 0.0, 0.0),\n    \
         props: [\n        (\"b\", 2.0),\n        (\"a\", 1.0),\n    ],\n)\n"
    );
    // The child is kept.
    assert_eq!(doc.node_ron("box").unwrap().1, ["lid"]);
    // props in set replace the whole list.
    doc.edit_node("box", Some("props: []"), &[], &no_props())
        .unwrap();
    assert!(!ron_of(&doc, "box").contains("props"));
}

#[test]
fn props_merge_by_name_in_name_order() {
    let mut doc = small();
    let props = [
        ("c".to_owned(), Some(3.0)),
        ("b".to_owned(), None),
        ("a".to_owned(), Some(-1.5)),
    ]
    .into_iter()
    .collect();
    doc.edit_node("box", None, &[], &props).unwrap();
    // a changed in place, b removed, c added last.
    assert!(
        ron_of(&doc, "box")
            .contains("props: [\n        (\"a\", -1.5),\n        (\"c\", 3.0),\n    ],"),
        "{}",
        ron_of(&doc, "box")
    );
    assert_eq!(
        edit_error(None, &[], &[("zz", None)]),
        r#"prop "zz" is not on the node "box""#
    );
}

#[test]
fn renaming_a_node_moves_its_path() {
    let mut doc = small();
    doc.edit_node("box", Some(r#"name: "crate""#), &[], &no_props())
        .unwrap();
    assert!(doc.node_ron("crate/lid").is_ok());
    assert_eq!(doc.node_ron("box").unwrap_err().0, r#"no node at "box""#);
}

#[test]
fn edit_node_rejects_bad_fields_with_exact_messages() {
    assert_eq!(
        edit_error(None, &[], &[]),
        "edit_node needs set, unset or props"
    );
    assert!(
        edit_error(Some("positon: (1.0, 0.0, 0.0)"), &[], &[])
            .starts_with("set: 1:1: Unexpected field named `positon`"),
        "{}",
        edit_error(Some("positon: (1.0, 0.0, 0.0)"), &[], &[])
    );
    assert_eq!(
        edit_error(Some("mesh: Cube, mesh: Sphere"), &[], &[]),
        "set: 1:13: Unexpected duplicate field named `mesh`"
    );
    assert_eq!(
        edit_error(Some("position: (1.0, 0.0)"), &[], &[]),
        "set: 1:20: Expected a tuple of size 3 but found 2 elements instead"
    );
    assert_eq!(
        edit_error(Some("children: []"), &[], &[]),
        "set cannot give children: use add_node and remove_node"
    );
    assert_eq!(edit_error(None, &["name"], &[]), "unset cannot remove name");
    assert_eq!(
        edit_error(None, &["props"], &[]),
        "unset cannot remove props: set props: [] instead"
    );
    assert_eq!(
        edit_error(None, &["children"], &[]),
        "unset cannot remove children: use remove_node"
    );
    assert!(edit_error(None, &["meshh"], &[]).starts_with(r#"unset: unknown field "meshh""#));
    assert_eq!(
        edit_error(None, &["mesh", "mesh"], &[]),
        r#"unset lists "mesh" twice"#
    );
    assert_eq!(
        edit_error(Some("mesh: Sphere"), &["mesh"], &[]),
        "mesh is both in set and in unset"
    );
    assert_eq!(
        small()
            .edit_node("nope/lid", Some("mesh: Cube"), &[], &no_props())
            .unwrap_err()
            .0,
        r#"no node at "nope/lid""#
    );
}

#[test]
fn add_node_inserts_at_the_start_the_middle_or_the_end() {
    let names = |doc: &SceneDoc| -> Vec<String> {
        let scene = doc.build().unwrap();
        scene
            .roots()
            .iter()
            .map(|&id| scene.node(id).name.clone())
            .collect()
    };
    let mut doc = small();
    doc.add_node(None, Some(0), r#"(name: "first")"#).unwrap();
    doc.add_node(
        None,
        Some(2),
        r#"(name: "middle", children: [(name: "kid")])"#,
    )
    .unwrap();
    doc.add_node(None, None, r#"(name: "last")"#).unwrap();
    assert_eq!(names(&doc), ["first", "cam", "middle", "box", "last"]);
    assert_eq!(doc.node_ron("middle").unwrap().1, ["kid"]);
    doc.add_node(Some("box"), Some(0), r#"(name: "handle")"#)
        .unwrap();
    assert_eq!(doc.node_ron("box").unwrap().1, ["handle", "lid"]);
    assert_eq!(
        doc.add_node(Some("box"), Some(3), r#"(name: "x")"#)
            .unwrap_err()
            .0,
        r#"index 3 is past the end: "box" has 2 children"#
    );
    assert!(
        doc.add_node(None, None, r#"(name: "x", colour: (1.0, 0.0, 0.0))"#)
            .unwrap_err()
            .0
            .starts_with("ron: ")
    );
}

#[test]
fn remove_node_takes_the_whole_subtree() {
    let mut doc = small();
    doc.remove_node("box").unwrap();
    let scene = doc.build().unwrap();
    assert_eq!(scene.len(), 1);
    assert_eq!(
        doc.remove_node("box/lid").unwrap_err().0,
        r#"no node at "box""#
    );
    assert_eq!(
        small().remove_node("box/nope").unwrap_err().0,
        r#"no node at "box/nope""#
    );
}

#[test]
fn edit_scene_sets_camera_and_ambient_but_not_game_or_version() {
    let mut doc = small();
    doc.edit_scene("ambient: (0.5, 0.25, 0.125)").unwrap();
    assert_eq!(doc.build().unwrap().ambient(), Vec3::new(0.5, 0.25, 0.125));
    for field in [r#"game: "rogue""#, "version: 2", "nodes: []"] {
        let name = field.split(':').next().unwrap();
        assert_eq!(
            doc.edit_scene(field).unwrap_err().0,
            format!("edit_scene cannot change {name}")
        );
    }
    // A camera that names no camera node fails at build time.
    doc.edit_scene(r#"camera: "box""#).unwrap();
    assert_eq!(
        doc.build().unwrap_err().to_string(),
        r#"camera "box" names a node without a camera component"#
    );
}

#[test]
fn edits_that_break_a_scene_rule_fail_to_build_with_the_load_message() {
    // Two siblings named alike.
    let mut doc = small();
    doc.add_node(None, None, r#"(name: "box")"#).unwrap();
    assert_eq!(
        doc.build().unwrap_err().to_string(),
        Scene::parse(&doc.to_ron()).unwrap_err().to_string()
    );
    assert!(
        doc.build()
            .unwrap_err()
            .to_string()
            .contains("another node with the same parent has the same name")
    );
    // A collider on the character.
    let mut doc = small();
    doc.edit_node(
        "box",
        Some("character: (run_speed: 4.0, turn_speed: 720.0), collider: Box"),
        &["mesh".to_owned()],
        &no_props(),
    )
    .unwrap();
    assert_eq!(
        doc.build().unwrap_err().to_string(),
        r#"node "box": character and collider cannot be on the same node"#
    );
}
