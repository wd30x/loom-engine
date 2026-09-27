//! The game: a map of cells, some of them walls, a player who moves one cell
//! per key press in eight directions, and zombies that chase the player in
//! a straight line, bite when adjacent and die into corpses. The rules and
//! their order are fixed by `docs/architecture.md` section 15; changing
//! them changes the state dump. All simulation arithmetic is on integers.

use glam::Vec3;
use loom_core::SimClock;
use loom_core::debug::DebugDraw;
use loom_core::input::{ActionMap, ActionState};
use loom_scene::{Node, NodeId, Scene, SceneError};
use serde::Serialize;

const BINDINGS: &str = include_str!("bindings.ron");

/// The move actions with their `(dc, dr)` step, in the order they are
/// checked each tick (and zombies try their neighbours). Rows grow
/// downwards, so north is `dr = -1`.
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

/// The sprite tag every character switches to when it dies.
const CORPSE: &str = "corpse";

/// Energy a zombie spends per action; it gains `speed` per turn.
const ACTION_COST: i32 = 100;

/// Props of the player and of a zombie, in the order they are checked.
const PLAYER_PROPS: [&str; 2] = ["hp", "attack"];
const ZOMBIE_PROPS: [&str; 3] = ["hp", "attack", "speed"];

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
    #[error("{0:?} needs a sprite")]
    NoSprite(String),
    #[error("{0:?} must only be translated: no rotation or look_at, scale (1, 1, 1)")]
    NotOnlyTranslated(String),
    #[error(
        "{name:?} at ({x}, {y}) is not on a map cell: x - map.x and map.y - y must be whole \
         numbers inside the {width}x{height} map"
    )]
    OffGrid {
        name: String,
        x: f32,
        y: f32,
        width: usize,
        height: usize,
    },
    #[error("{name:?} starts on a wall at column {column}, row {row}")]
    StartsOnWall { name: String, column: i32, row: i32 },
    #[error("{name:?} starts on the same cell as {other:?}")]
    SameCell { name: String, other: String },
    #[error(
        "the player must be in front of the map: its z ({player_z}) must be greater than the \
         map's ({map_z})"
    )]
    BehindMap { player_z: f32, map_z: f32 },
    #[error(
        "{name:?} must be between the map and the player: its z ({z}) must be greater than the \
         map's ({map_z}) and less than the player's ({player_z})"
    )]
    ZombieDepth {
        name: String,
        z: f32,
        map_z: f32,
        player_z: f32,
    },
    #[error("{name:?} must have the same z as {first:?} ({first_z}), not {z}")]
    ZombieDepthMismatch {
        name: String,
        first: String,
        first_z: f32,
        z: f32,
    },
    #[error("{0:?}: its sprite needs \"corpse\" in alternates, shown when it dies")]
    NoCorpse(String),
    #[error("{name:?}: prop {prop} is missing")]
    MissingProp { name: String, prop: &'static str },
    #[error("{name:?}: prop {prop} must be a whole number from 1 to 1000, got {value}")]
    BadProp {
        name: String,
        prop: &'static str,
        value: f64,
    },
    #[error("{name:?}: unknown prop {prop:?} (known: {known})")]
    UnknownProp {
        name: String,
        prop: String,
        known: String,
    },
}

/// The game state in the state dump (`game`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameState {
    /// Turns taken so far: moves, attacks and waits.
    pub turn: u64,
    pub player: PlayerState,
    /// In scene pre-order; dead ones stay, with `hp` 0, on their corpse.
    pub zombies: Vec<ZombieState>,
    /// Every message so far, oldest first.
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlayerState {
    /// `[column, row]`; row 0 is the map's top row.
    pub cell: [i32; 2],
    /// 0 when dead.
    pub hp: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZombieState {
    /// The node name.
    pub name: String,
    /// `[column, row]`.
    pub cell: [i32; 2],
    /// 0 when dead.
    pub hp: i32,
    /// Gained `speed` per turn, spent 100 per action.
    pub energy: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// The turn the message belongs to (after that turn was counted).
    pub turn: u64,
    pub text: String,
}

/// One event of a tick, for traces and `loom serve`'s `step`
/// (`docs/architecture.md` section 15, "事件"); serialized as an object
/// whose first field is `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// The player bumped into a zombie, or a zombie bit the player: one
    /// per `You hit`, `You kill` or `bites you` message.
    Hit {
        /// The attacker's node path.
        by: String,
        /// The node path of the one hit.
        target: String,
        /// The attacker's attack.
        damage: i32,
        /// The target's hp after the hit.
        hp: i32,
        result: HitResult,
    },
}

/// What a hit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HitResult {
    Hit,
    /// Down to 0 hp.
    Death,
}

/// What does not change after loading, per zombie; parallel to
/// `GameState::zombies`.
#[derive(Debug, Clone, Copy)]
struct ZombieStats {
    node: NodeId,
    attack: i32,
    speed: i32,
}

#[derive(Debug)]
pub struct Game {
    clock: SimClock,
    scene: Scene,
    player: NodeId,
    player_attack: i32,
    zombies: Vec<ZombieStats>,
    /// The map node's position: cell `(c, r)` is centered at
    /// `(map.x + c, map.y - r)`.
    map_origin: Vec3,
    width: i32,
    height: i32,
    /// `width * height` flags, row by row: whether the cell is a wall.
    walls: Vec<bool>,
    state: GameState,
    /// The events of the latest tick, in the order they happened.
    events: Vec<Event>,
}

/// The map as `Game::new` checks characters against it.
struct Grid<'a> {
    origin: Vec3,
    width: usize,
    height: usize,
    walls: &'a [bool],
}

impl Grid<'_> {
    /// The cell of the node at `position`, or why it is not on one.
    fn cell_of(&self, name: &str, position: Vec3) -> Result<[i32; 2], GameError> {
        let (c, r) = (position.x - self.origin.x, self.origin.y - position.y);
        let on_grid = c.fract() == 0.0
            && r.fract() == 0.0
            && (0.0..self.width as f32).contains(&c)
            && (0.0..self.height as f32).contains(&r);
        if !on_grid {
            return Err(GameError::OffGrid {
                name: name.to_owned(),
                x: position.x,
                y: position.y,
                width: self.width,
                height: self.height,
            });
        }
        let (column, row) = (c as usize, r as usize);
        let cell = [cell_index(column), cell_index(row)];
        if self.walls[row * self.width + column] {
            return Err(GameError::StartsOnWall {
                name: name.to_owned(),
                column: cell[0],
                row: cell[1],
            });
        }
        Ok(cell)
    }
}

fn cell_index(n: usize) -> i32 {
    i32::try_from(n).expect("a map fits in memory, so in i32 cells")
}

impl Game {
    /// Parses scene source text and starts a game on it.
    pub fn from_source(src: &str) -> Result<Self, GameError> {
        Self::new(Scene::parse(src)?)
    }

    /// Takes over `scene` after checking everything section 15 requires of
    /// it, in its order: the map, the player, then each zombie.
    pub fn new(scene: Scene) -> Result<Self, GameError> {
        let map = root_with(&scene, "map", |n| n.tilemap.is_some()).ok_or(GameError::NoMap)?;
        only_translated(&scene, map)?;
        let tilemap = scene
            .node(map)
            .tilemap
            .as_ref()
            .expect("root_with checked the map has a tilemap");
        let walls: Vec<bool> = tilemap
            .rows
            .iter()
            .flat_map(|row| row.chars().map(|ch| ch == WALL))
            .collect();
        let grid = Grid {
            origin: scene.transform(map).position,
            width: tilemap.rows[0].chars().count(),
            height: tilemap.rows.len(),
            walls: &walls,
        };
        let zombie_ids: Vec<NodeId> = scene
            .roots()
            .iter()
            .copied()
            .filter(|&id| is_zombie_name(&scene.node(id).name))
            .collect();

        let player =
            root_with(&scene, "player", |n| n.sprite.is_some()).ok_or(GameError::NoPlayer)?;
        only_translated(&scene, player)?;
        let player_position = scene.transform(player).position;
        let player_cell = grid.cell_of("player", player_position)?;
        if zombie_ids.is_empty() && player_position.z <= grid.origin.z {
            return Err(GameError::BehindMap {
                player_z: player_position.z,
                map_z: grid.origin.z,
            });
        }
        needs_corpse(&scene, player)?;
        let [player_hp, player_attack] = props(scene.node(player), PLAYER_PROPS)?;

        // Characters placed so far, for the same-cell check.
        let mut placed = vec![("player".to_owned(), player_cell)];
        let mut zombies = Vec::with_capacity(zombie_ids.len());
        let mut zombie_states = Vec::with_capacity(zombie_ids.len());
        for &id in &zombie_ids {
            let node = scene.node(id);
            let name = node.name.clone();
            if node.sprite.is_none() {
                return Err(GameError::NoSprite(name));
            }
            only_translated(&scene, id)?;
            let position = scene.transform(id).position;
            let cell = grid.cell_of(&name, position)?;
            if let Some((other, _)) = placed.iter().find(|(_, c)| *c == cell) {
                return Err(GameError::SameCell {
                    name,
                    other: other.clone(),
                });
            }
            if !(grid.origin.z < position.z && position.z < player_position.z) {
                return Err(GameError::ZombieDepth {
                    name,
                    z: position.z,
                    map_z: grid.origin.z,
                    player_z: player_position.z,
                });
            }
            let first = zombie_ids[0];
            let first_z = scene.transform(first).position.z;
            if position.z != first_z {
                return Err(GameError::ZombieDepthMismatch {
                    name,
                    first: scene.node(first).name.clone(),
                    first_z,
                    z: position.z,
                });
            }
            needs_corpse(&scene, id)?;
            let [hp, attack, speed] = props(node, ZOMBIE_PROPS)?;
            placed.push((name.clone(), cell));
            zombies.push(ZombieStats {
                node: id,
                attack,
                speed,
            });
            zombie_states.push(ZombieState {
                name,
                cell,
                hp,
                energy: 0,
            });
        }

        let (width, height) = (cell_index(grid.width), cell_index(grid.height));
        let map_origin = grid.origin;
        Ok(Self {
            clock: SimClock::new(),
            scene,
            player,
            player_attack,
            zombies,
            map_origin,
            width,
            height,
            walls,
            state: GameState {
                turn: 0,
                player: PlayerState {
                    cell: player_cell,
                    hp: player_hp,
                },
                zombies: zombie_states,
                messages: Vec::new(),
            },
            events: Vec::new(),
        })
    }

    /// Advances one tick: the first move (or wait) pressed this very tick,
    /// in [`MOVES`] order, is the player's turn; then every living zombie
    /// gains energy and acts while it has enough. Holding a key does not
    /// repeat it; a dead player ignores all input.
    pub fn tick(&mut self, input: &ActionState) {
        self.clock.step();
        self.events.clear();
        if self.state.player.hp == 0 {
            return;
        }
        let step = MOVES
            .iter()
            .find(|(action, _, _)| input.just_pressed(action))
            .map(|&(_, dc, dr)| (dc, dr));
        let step = match step {
            Some(step) => Some(step),
            None if input.just_pressed(WAIT) => None,
            None => return,
        };
        if let Some((dc, dr)) = step {
            let [c, r] = self.state.player.cell;
            let target = [c + dc, r + dr];
            if let Some(z) = self.living_zombie_at(target) {
                self.state.turn += 1;
                self.player_attacks(z);
            } else if !self.is_floor(target) {
                // Bumping into a wall or the map edge takes no turn.
                return;
            } else {
                self.state.turn += 1;
                self.state.player.cell = target;
                self.place(self.player, target);
            }
        } else {
            self.state.turn += 1;
        }
        for z in 0..self.zombies.len() {
            // A zombie killed this turn does not act.
            if self.state.zombies[z].hp == 0 {
                continue;
            }
            self.state.zombies[z].energy += self.zombies[z].speed;
            while self.state.zombies[z].energy >= ACTION_COST && self.state.player.hp > 0 {
                self.state.zombies[z].energy -= ACTION_COST;
                self.zombie_acts(z);
            }
        }
    }

    /// The player hits zombie `z`; at 0 hp it dies into a corpse drawn
    /// halfway between the map and the living zombies.
    fn player_attacks(&mut self, z: usize) {
        let zombie = &mut self.state.zombies[z];
        zombie.hp = (zombie.hp - self.player_attack).max(0);
        let (dead, name, hp) = (zombie.hp == 0, zombie.name.clone(), zombie.hp);
        self.hit(self.player, self.zombies[z].node, self.player_attack, hp);
        if !dead {
            self.message(format!("You hit {name}."));
            return;
        }
        self.message(format!("You kill {name}."));
        let node = self.zombies[z].node;
        self.show_corpse(node);
        let map_z = self.map_origin.z;
        let transform = self.scene.transform_mut(node);
        transform.position.z = (map_z + transform.position.z) * 0.5;
    }

    /// Zombie `z` bites the player if adjacent, or else steps to the
    /// neighbour closest to the player (Chebyshev distance, then squared
    /// Euclidean), staying put when none is closer: no path finding.
    fn zombie_acts(&mut self, z: usize) {
        let [pc, pr] = self.state.player.cell;
        let [zc, zr] = self.state.zombies[z].cell;
        let key = |[c, r]: [i32; 2]| {
            let (dc, dr) = (c - pc, r - pr);
            (dc.abs().max(dr.abs()), dc * dc + dr * dr)
        };
        if key([zc, zr]).0 == 1 {
            let player = &mut self.state.player;
            player.hp = (player.hp - self.zombies[z].attack).max(0);
            let (dead, hp) = (player.hp == 0, player.hp);
            let name = self.state.zombies[z].name.clone();
            self.hit(
                self.zombies[z].node,
                self.player,
                self.zombies[z].attack,
                hp,
            );
            self.message(format!("{name} bites you."));
            if dead {
                self.message("You die.".to_owned());
                self.show_corpse(self.player);
            }
            return;
        }
        let mut best = [zc, zr];
        let mut best_key = key(best);
        for &(_, dc, dr) in &MOVES {
            let n = [zc + dc, zr + dr];
            if !self.is_floor(n) || n == [pc, pr] || self.living_zombie_at(n).is_some() {
                continue;
            }
            if key(n) < best_key {
                best = n;
                best_key = key(n);
            }
        }
        if best != [zc, zr] {
            self.state.zombies[z].cell = best;
            self.place(self.zombies[z].node, best);
        }
    }

    /// A hit by the node `by` on `target` for `damage`, leaving it `hp`.
    fn hit(&mut self, by: NodeId, target: NodeId, damage: i32, hp: i32) {
        self.events.push(Event::Hit {
            by: self.scene.path(by),
            target: self.scene.path(target),
            damage,
            hp,
            result: if hp == 0 {
                HitResult::Death
            } else {
                HitResult::Hit
            },
        });
    }

    fn message(&mut self, text: String) {
        self.state.messages.push(Message {
            turn: self.state.turn,
            text,
        });
    }

    fn show_corpse(&mut self, node: NodeId) {
        self.scene
            .set_sprite_tag(node, CORPSE)
            .expect("Game::new checked every character declares a corpse alternate");
    }

    /// Moves `node` onto `cell`, keeping its z.
    fn place(&mut self, node: NodeId, [c, r]: [i32; 2]) {
        let (x, y) = (self.map_origin.x + c as f32, self.map_origin.y - r as f32);
        let transform = self.scene.transform_mut(node);
        transform.position = Vec3::new(x, y, transform.position.z);
    }

    /// The index of the living zombie on `cell`, if any.
    fn living_zombie_at(&self, cell: [i32; 2]) -> Option<usize> {
        self.state
            .zombies
            .iter()
            .position(|z| z.hp > 0 && z.cell == cell)
    }

    /// Whether `[c, r]` is inside the map and not a wall.
    fn is_floor(&self, [c, r]: [i32; 2]) -> bool {
        if !(0..self.width).contains(&c) || !(0..self.height).contains(&r) {
            return false;
        }
        let index = usize::try_from(r * self.width + c).expect("checked to be inside the map");
        !self.walls[index]
    }

    /// The events of the latest tick, in the order they happened; none
    /// before the first tick.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// The debug draw categories: none yet (`docs/architecture.md`
    /// section 16).
    pub fn debug_categories() -> &'static [&'static str] {
        &[]
    }

    /// Draws nothing: rogue has no debug draw categories.
    pub fn debug_draw(&self, _categories: &[&str], _draw: &mut DebugDraw) {}

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    /// The scene with every character node where the state says it is.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn state(&self) -> GameState {
        self.state.clone()
    }
}

/// `zombie` or `zombie_...`.
fn is_zombie_name(name: &str) -> bool {
    name == "zombie" || name.starts_with("zombie_")
}

/// The root node named `name`, if it satisfies `has`.
fn root_with(scene: &Scene, name: &str, has: impl Fn(&Node) -> bool) -> Option<NodeId> {
    scene.find(name).filter(|&id| {
        let node = scene.node(id);
        node.parent.is_none() && has(node)
    })
}

fn only_translated(scene: &Scene, id: NodeId) -> Result<(), GameError> {
    let t = scene.transform(id);
    if t.rotation != glam::Quat::IDENTITY || t.scale != Vec3::ONE {
        return Err(GameError::NotOnlyTranslated(scene.path(id)));
    }
    Ok(())
}

fn needs_corpse(scene: &Scene, id: NodeId) -> Result<(), GameError> {
    let sprite = scene
        .node(id)
        .sprite
        .as_ref()
        .expect("checked to have a sprite before");
    // tags()[0] is the sprite's own tag; the rest are its alternates.
    if !sprite.tags()[1..].iter().any(|t| t == CORPSE) {
        return Err(GameError::NoCorpse(scene.path(id)));
    }
    Ok(())
}

/// The `known` props of `node`, in that order, each a whole number from 1
/// to 1000. Missing and invalid ones are reported first (in `known` order),
/// then any unknown name (in file order).
fn props<const N: usize>(node: &Node, known: [&'static str; N]) -> Result<[i32; N], GameError> {
    let name = || node.name.clone();
    let mut values = [0; N];
    for (value, prop) in values.iter_mut().zip(known) {
        let Some(v) = node.prop(prop) else {
            return Err(GameError::MissingProp { name: name(), prop });
        };
        if v.fract() != 0.0 || !(1.0..=1000.0).contains(&v) {
            return Err(GameError::BadProp {
                name: name(),
                prop,
                value: v,
            });
        }
        *value = v as i32;
    }
    if let Some((prop, _)) = node.props().find(|(p, _)| !known.contains(p)) {
        return Err(GameError::UnknownProp {
            name: name(),
            prop: prop.to_owned(),
            known: known.join(", "),
        });
    }
    Ok(values)
}
