//! `loom schema`: the formats an AI writes and reads, described from the
//! types and tables that define them (`docs/architecture.md` section 17).
//! The same content is committed as `schema/loom.schema.json`.

use std::collections::BTreeMap;

use loom_core::props::PropGroup;
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema};
use serde::Serialize;

use crate::state::StateDump;
use crate::trace::Line;

/// The version of this description's own layout.
const LOOM_SCHEMA: u32 = 1;

/// The whole description; the fields in this order.
#[derive(Debug, Serialize)]
pub(crate) struct LoomSchema {
    loom_schema: u32,
    /// The scene file.
    scene: Schema,
    /// Each game, by name.
    games: BTreeMap<&'static str, GameSchema>,
}

/// One game's props, state dump, trace line and events.
#[derive(Debug, Serialize)]
struct GameSchema {
    /// By kind of node.
    props: BTreeMap<&'static str, PropGroup>,
    state_dump: Schema,
    trace_line: Schema,
    event: Schema,
}

/// What the engine writes, as JSON Schema draft 2020-12.
fn written<T: JsonSchema>() -> Schema {
    SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

impl GameSchema {
    fn of<G: JsonSchema, E: JsonSchema>(props: &[(&'static str, PropGroup)]) -> Self {
        Self {
            props: props.iter().copied().collect(),
            state_dump: written::<StateDump<G>>(),
            trace_line: written::<Line<Vec<E>, StateDump<G>>>(),
            event: written::<E>(),
        }
    }
}

impl LoomSchema {
    pub fn new() -> Self {
        Self {
            loom_schema: LOOM_SCHEMA,
            scene: loom_scene::scene_schema(),
            games: BTreeMap::from([
                (
                    "rogue",
                    GameSchema::of::<rogue::GameState, rogue::Event>(&rogue::PROP_GROUPS),
                ),
                (
                    "souls",
                    GameSchema::of::<souls::GameState, souls::Event>(&souls::PROP_GROUPS),
                ),
            ]),
        }
    }

    /// Pretty JSON (2-space indent) with a trailing newline: the committed
    /// file, which a test writes.
    #[cfg(test)]
    pub fn to_pretty(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("schemas are plain JSON values");
        json.push('\n');
        json
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;
    use serde_json::{Value, json};

    use super::*;
    use crate::games::GAMES;

    /// The committed file, from the repository root.
    const COMMITTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schema/loom.schema.json");

    fn schema() -> Value {
        serde_json::to_value(LoomSchema::new()).unwrap()
    }

    /// Fails with the first error when `instance` does not fit `schema`.
    fn assert_fits(schema: &Value, instance: &Value) {
        let validator = jsonschema::validator_for(schema).unwrap();
        if let Some(e) = validator.iter_errors(instance).next() {
            panic!("{e} at {}: {instance}", e.instance_path());
        }
    }

    #[test]
    fn committed_schema_file_is_the_generated_one() {
        let generated = LoomSchema::new().to_pretty();
        if std::env::var_os("LOOM_UPDATE_SCHEMA").is_some() {
            std::fs::write(COMMITTED, &generated).unwrap();
        }
        let committed = std::fs::read_to_string(COMMITTED).unwrap_or_default();
        assert!(
            committed == generated,
            "schema/loom.schema.json is out of date: rerun with LOOM_UPDATE_SCHEMA=1"
        );
        assert!(generated.ends_with("}\n") && !generated.contains('\r'));
        // The same code gives the same bytes.
        assert_eq!(LoomSchema::new().to_pretty(), generated);
    }

    #[test]
    fn schema_lists_its_parts_in_order_and_every_game_by_name() {
        let pretty = LoomSchema::new().to_pretty();
        let at = |key: &str| pretty.find(&format!("\"{key}\":")).unwrap();
        assert!(at("loom_schema") < at("scene") && at("scene") < at("games"));
        let schema = schema();
        assert_eq!(schema["loom_schema"], 1);
        let games: Vec<&str> = schema["games"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(games, GAMES);
        for game in GAMES {
            let part = &schema["games"][game]["state_dump"];
            assert_eq!(
                part["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
        }
        let souls = serde_json::to_string(&LoomSchema::new().games["souls"]).unwrap();
        let at = |key: &str| souls.find(&format!("\"{key}\":")).unwrap();
        assert!(at("props") < at("state_dump"));
        assert!(at("state_dump") < at("trace_line") && at("trace_line") < at("event"));
    }

    #[test]
    fn props_come_from_the_tables_that_check_them() {
        let schema = schema();
        let group = |game: &str, kind: &str| schema["games"][game]["props"][kind].clone();
        let prop = |game: &str, kind: &str, name: &str| {
            group(game, kind)["props"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["name"] == name)
                .cloned()
                .unwrap_or_else(|| panic!("no {name}"))
        };
        let character = group("souls", "character");
        assert_eq!(character["props"].as_array().unwrap().len(), 50);
        assert_eq!(
            group("souls", "node")["props"].as_array().unwrap().len(),
            95
        );
        assert_eq!(
            prop("souls", "character", "stamina_max"),
            json!({"name": "stamina_max", "default": 100.0, "rule": {"kind": "positive"},
                   "unit": "stamina points", "description": "The most stamina the character has."})
        );
        assert_eq!(
            prop("souls", "character", "lock_pitch_down")["rule"],
            json!({"kind": "below", "min": 0.0, "max": 90.0})
        );
        assert_eq!(
            prop("souls", "node", "swing_arc")["rule"],
            json!({"kind": "arc"})
        );
        assert_eq!(prop("souls", "node", "lock_height")["default"], Value::Null);
        assert_eq!(prop("souls", "node", "revive_ticks")["default"], 180.0);
        for kind in ["player", "zombie"] {
            let hp = prop("rogue", kind, "hp");
            assert_eq!(hp["default"], Value::Null);
            assert_eq!(hp["rule"], json!({"kind": "whole", "min": 1, "max": 1000}));
        }
        // In the order they are checked.
        let names = |game: &str, kind: &str| -> Vec<String> {
            group(game, kind)["props"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["name"].as_str().unwrap().to_owned())
                .collect()
        };
        let node = names("souls", "node");
        assert_eq!(node[..3], ["lock_height", "hurt_radius", "hurt_height"]);
        assert_eq!(node[16], "swing_high");
        assert_eq!(node[17], "enemy_speed");
        assert_eq!(
            node[25..28],
            ["sweep_damage", "sweep_impact", "sweep_startup_ticks"]
        );
        assert_eq!(node[68], "combo_track_speed");
        assert_eq!(
            node[69..72],
            ["lunge_distance", "combo_hits", "combo_gap_ticks"]
        );
        assert_eq!(
            node[72..75],
            [
                "approach_min_range",
                "approach_weight_per_meter",
                "approach_ticks"
            ]
        );
        assert_eq!(node[75..77], ["approach_weight_max", "phase2_at"]);
        assert_eq!(node[83], "roar_reach");
        assert_eq!(node[84], "spin_damage");
        assert_eq!(node[94], "spin_track_speed");
        assert_eq!(
            prop("souls", "node", "phase2_at")["rule"],
            json!({"kind": "between", "min": 0.0, "max": 1.0})
        );
        assert_eq!(prop("souls", "node", "spin_arc")["default"], 360.0);
        assert_eq!(
            prop("souls", "node", "enemy_face_angle")["rule"],
            json!({"kind": "positive_at_most", "max": 180.0})
        );
        assert_eq!(prop("souls", "node", "cleave_arc")["default"], 20.0);
        let character_names = names("souls", "character");
        assert_eq!(character_names[..2], ["stamina_max", "stamina_regen"]);
        assert_eq!(character_names[49], "respawn_ticks");
        assert_eq!(names("rogue", "zombie"), ["hp", "attack", "speed"]);
        assert_eq!(
            character["constraints"][2],
            "lock_range <= lock_break_range"
        );
        assert_eq!(
            group("souls", "node")["requires"][2],
            "poise, stagger_ticks, poise_reset_ticks, revive_ticks: need hp"
        );
        assert_eq!(
            group("souls", "node")["constraints"][9],
            "an enemy has no collider in its subtree"
        );
        assert_eq!(group("rogue", "player")["requires"], json!([]));
    }

    #[test]
    fn prop_texts_are_single_clean_lines() {
        // Written by hand as one line each, unlike the `///` descriptions:
        // a string continued with `\\` instead of `\` keeps a backslash,
        // the line end and the next line's indent.
        let schema = schema();
        let mut texts: Vec<String> = Vec::new();
        for (game, groups) in schema["games"].as_object().unwrap() {
            for (kind, group) in groups["props"].as_object().unwrap() {
                for key in ["requires", "constraints"] {
                    for text in group[key].as_array().unwrap() {
                        texts.push(format!("{game}.{kind}.{key}: {}", text.as_str().unwrap()));
                    }
                }
                for prop in group["props"].as_array().unwrap() {
                    for key in ["unit", "description"] {
                        let text = prop[key].as_str().unwrap();
                        texts.push(format!("{game}.{kind}.{}.{key}: {text}", prop["name"]));
                    }
                }
            }
        }
        assert!(texts.len() > 2 * (50 + 95 + 2 + 3), "{}", texts.len());
        for text in &texts {
            assert!(
                !text.contains('\\') && !text.contains('\n') && !text.contains("  "),
                "{text:?}"
            );
        }
    }

    /// One souls event of each type (hits by the character, a swinger and an
    /// enemy's move, with and without health);
    /// the match stops compiling when a type is added without one here.
    fn every_souls_event() -> Vec<souls::Event> {
        use souls::{
            Action, Buffered, DropReason, EndReason, Event, HitAttack, Input, LockReason, Outcome,
        };
        let events = vec![
            Event::Hit {
                by: "player".to_owned(),
                target: "dummy_b".to_owned(),
                attack: HitAttack::Heavy,
                combo: 0,
                damage: Some(35.0),
                hp: Some(5.0),
                result: Outcome::Stagger,
            },
            Event::Hit {
                by: "stake".to_owned(),
                target: "box".to_owned(),
                attack: HitAttack::Swing,
                combo: 0,
                damage: None,
                hp: None,
                result: Outcome::Hit,
            },
            Event::Dodged {
                by: "stake".to_owned(),
                action: Action::Roll,
                action_tick: 3,
            },
            Event::InputDropped {
                input: Input::Lock,
                reason: DropReason::NoTarget,
                buffered: false,
            },
            Event::BufferReplaced {
                old: Buffered::Attack,
                new: Buffered::Roll,
            },
            Event::ActionStart {
                action: Action::Attack,
                combo: 1,
                buffered: true,
            },
            Event::ActionEnd {
                action: Action::Sprint,
                reason: EndReason::Interrupted,
            },
            Event::Lock {
                target: None,
                reason: LockReason::TargetDied,
            },
            Event::Revive {
                node: "dummy_a".to_owned(),
            },
            Event::Respawn {
                position: Vec3::new(0.0, 0.0, -1.5),
            },
            Event::EnemyMove {
                by: "boss".to_owned(),
                r#move: souls::EnemyChoice::Approach,
                distance: 6.5,
                candidates: vec![souls::EnemyChoice::Lunge, souls::EnemyChoice::Approach],
            },
            Event::Hit {
                by: "boss".to_owned(),
                target: "player".to_owned(),
                attack: HitAttack::Combo,
                combo: 0,
                damage: Some(20.0),
                hp: Some(80.0),
                result: Outcome::Hit,
            },
            Event::Stage {
                by: "boss".to_owned(),
                stage: 2,
            },
            Event::Hit {
                by: "boss".to_owned(),
                target: "player".to_owned(),
                attack: HitAttack::Roar,
                combo: 0,
                damage: Some(30.0),
                hp: Some(50.0),
                result: Outcome::Stagger,
            },
        ];
        for e in &events {
            match e {
                Event::Hit { .. }
                | Event::Dodged { .. }
                | Event::InputDropped { .. }
                | Event::BufferReplaced { .. }
                | Event::ActionStart { .. }
                | Event::ActionEnd { .. }
                | Event::Lock { .. }
                | Event::Revive { .. }
                | Event::Respawn { .. }
                | Event::EnemyMove { .. }
                | Event::Stage { .. } => {}
            }
        }
        events
    }

    #[test]
    fn every_event_type_of_both_games_fits_its_event_schema() {
        let schema = schema();
        let souls = &schema["games"]["souls"]["event"];
        let events = every_souls_event();
        let mut types: Vec<String> = Vec::new();
        for e in &events {
            let e = serde_json::to_value(e).unwrap();
            assert_fits(souls, &e);
            types.push(e["type"].as_str().unwrap().to_owned());
        }
        types.sort();
        types.dedup();
        assert_eq!(types.len(), 11);
        assert_eq!(souls["oneOf"].as_array().unwrap().len(), 11);

        let rogue = &schema["games"]["rogue"]["event"];
        let e = rogue::Event::Hit {
            by: "z1".to_owned(),
            target: "player".to_owned(),
            damage: 1,
            hp: 9,
            result: rogue::HitResult::Hit,
        };
        match e {
            rogue::Event::Hit { .. } => {}
        }
        assert_fits(rogue, &serde_json::to_value(&e).unwrap());

        // And what does not fit: a misspelled type, a missing field.
        let validator = jsonschema::validator_for(souls).unwrap();
        let hit = serde_json::to_value(&events[0]).unwrap();
        assert!(validator.is_valid(&hit));
        let mut misspelled = hit.clone();
        misspelled["type"] = json!("hits");
        assert!(!validator.is_valid(&misspelled));
        let mut missing = hit;
        missing.as_object_mut().unwrap().remove("damage");
        assert!(!validator.is_valid(&missing));
    }
}
