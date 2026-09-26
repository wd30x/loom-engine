//! The game: a map of cells, some of them walls, and a player who moves one
//! cell per key press in eight directions. The rules and their order are
//! fixed by `docs/architecture.md` section 15; changing them changes the
//! state dump.

use glam::Vec3;
use loom_core::SimClock;
use loom_core::input::{ActionMap, ActionState};
use loom_scene::{NodeId, Scene, SceneError};
use serde::Serialize;

const BINDINGS: &str = include_str!("bindings.ron");

/// The move actions with their `(dc, dr)` step, in the order they are
/// checked each tick. Rows grow downwards, so north is `dr = -1`.
pub const MOVES: [(&str, i32, i32); 8] = [
    ("move_n", 0, -1),
    ("move_ne", 1, -1),
    ("move_e", 1, 0),
    ("move_se", 1, 1),
    ("move_s", 0, 1),
    ("move_sw", -1, 1),
    ("move_w", -1, 0),
    ("move_nw", -1, -1),
];

/// Stay in place for one turn; checked after every move.
pub const WAIT: &str = "wait";

/// The map character that blocks movement; every other one is floor.
const WALL: char = '#';

/// The game's key bindings: the numpad and vi keys.
pub fn bindings() -> ActionMap {
    ActionMap::parse(BINDINGS).expect("the embedded bindings are valid (checked by a unit test)")
}

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error("the scene needs a root node named \"map\" with a tilemap")]
    NoMap,
    #[error("the scene needs a root node named \"player\" with a sprite")]
    NoPlayer,
    #[error("{0:?} must only be translated: no rotation or look_at, scale (1, 1, 1)")]
    NotOnlyTranslated(String),
    #[error(
        "the player at ({x}, {y}) is not on a map cell: x - map.x and map.y - y must be whole \
         numbers inside the {width}x{height} map"
    )]
    OffGrid {
        x: f32,
        y: f32,
        width: usize,
        height: usize,
    },
    #[error("the player starts on a wall at column {column}, row {row}")]
    StartsOnWall { column: usize, row: usize },
    #[error(
        "the player must be in front of the map: its z ({player_z}) must be greater than the \
         map's ({map_z})"
    )]
    BehindMap { player_z: f32, map_z: f32 },
}

/// The game state in the state dump (`game`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GameState {
    /// Turns taken so far: successful moves plus waits.
    pub turn: u64,
    pub player: PlayerState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlayerState {
    /// `[column, row]`; row 0 is the map's top row.
    pub cell: [i32; 2],
}

#[derive(Debug)]
pub struct Game {
    clock: SimClock,
    scene: Scene,
    player: NodeId,
    /// The map node's position: cell `(c, r)` is centered at
    /// `(map.x + c, map.y - r)`.
    map_origin: Vec3,
    width: i32,
    height: i32,
    /// `width * height` flags, row by row: whether the cell is a wall.
    walls: Vec<bool>,
    state: GameState,
}

impl Game {
    /// Parses scene source text and starts a game on it.
    pub fn from_source(src: &str) -> Result<Self, GameError> {
        Self::new(Scene::parse(src)?)
    }

    /// Takes over `scene`, which needs a root node `map` with a tilemap and
    /// a root node `player` with a sprite, both only translated, the player
    /// on a floor cell of the map and in front of it.
    pub fn new(scene: Scene) -> Result<Self, GameError> {
        let map = root_with(&scene, "map", |n| n.tilemap.is_some()).ok_or(GameError::NoMap)?;
        let player =
            root_with(&scene, "player", |n| n.sprite.is_some()).ok_or(GameError::NoPlayer)?;
        for id in [map, player] {
            let t = scene.transform(id);
            if t.rotation != glam::Quat::IDENTITY || t.scale != Vec3::ONE {
                return Err(GameError::NotOnlyTranslated(scene.path(id)));
            }
        }
        let tilemap = scene
            .node(map)
            .tilemap
            .as_ref()
            .expect("root_with checked the map has a tilemap");
        let rows = tilemap.rows.len();
        let columns = tilemap.rows[0].chars().count();
        let walls: Vec<bool> = tilemap
            .rows
            .iter()
            .flat_map(|row| row.chars().map(|ch| ch == WALL))
            .collect();

        let map_origin = scene.transform(map).position;
        let start = scene.transform(player).position;
        let (c, r) = (start.x - map_origin.x, map_origin.y - start.y);
        let on_grid = c.fract() == 0.0
            && r.fract() == 0.0
            && (0.0..columns as f32).contains(&c)
            && (0.0..rows as f32).contains(&r);
        if !on_grid {
            return Err(GameError::OffGrid {
                x: start.x,
                y: start.y,
                width: columns,
                height: rows,
            });
        }
        let (column, row) = (c as usize, r as usize);
        if walls[row * columns + column] {
            return Err(GameError::StartsOnWall { column, row });
        }
        if start.z <= map_origin.z {
            return Err(GameError::BehindMap {
                player_z: start.z,
                map_z: map_origin.z,
            });
        }
        let size = |n: usize| i32::try_from(n).expect("a map fits in memory, so in i32 cells");
        Ok(Self {
            clock: SimClock::new(),
            scene,
            player,
            map_origin,
            width: size(columns),
            height: size(rows),
            walls,
            state: GameState {
                turn: 0,
                player: PlayerState {
                    cell: [size(column), size(row)],
                },
            },
        })
    }

    /// Advances one tick: takes the first move (or wait) pressed this very
    /// tick, in [`MOVES`] order; holding a key does not repeat it.
    pub fn tick(&mut self, input: &ActionState) {
        self.clock.step();
        let step = MOVES
            .iter()
            .find(|(action, _, _)| input.just_pressed(action))
            .map(|&(_, dc, dr)| (dc, dr));
        let (dc, dr) = match step {
            Some(step) => step,
            None if input.just_pressed(WAIT) => {
                self.state.turn += 1;
                return;
            }
            None => return,
        };
        let [c, r] = self.state.player.cell;
        let (c2, r2) = (c + dc, r + dr);
        if !self.is_floor(c2, r2) {
            // Bumping into a wall or the map edge takes no turn.
            return;
        }
        self.state.player.cell = [c2, r2];
        self.state.turn += 1;
        let transform = self.scene.transform_mut(self.player);
        transform.position = Vec3::new(
            self.map_origin.x + c2 as f32,
            self.map_origin.y - r2 as f32,
            transform.position.z,
        );
    }

    /// Whether `(c, r)` is inside the map and not a wall.
    fn is_floor(&self, c: i32, r: i32) -> bool {
        if !(0..self.width).contains(&c) || !(0..self.height).contains(&r) {
            return false;
        }
        let index = usize::try_from(r * self.width + c).expect("checked to be inside the map");
        !self.walls[index]
    }

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    /// The scene with the player node where the state says it is.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn state(&self) -> GameState {
        self.state
    }
}

/// The root node named `name`, if it satisfies `has`.
fn root_with(scene: &Scene, name: &str, has: impl Fn(&loom_scene::Node) -> bool) -> Option<NodeId> {
    scene.find(name).filter(|&id| {
        let node = scene.node(id);
        node.parent.is_none() && has(node)
    })
}
