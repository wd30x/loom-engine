//! The HUD (`docs/architecture.md` section 11, "HUD"), a child of `game` so
//! it can read the game's own fields: the health and stamina bars at the
//! top left, the boss bars and names at the bottom, the flask at the bottom
//! left, the souls at the bottom right, the lock-on dot. HUD units: the
//! screen is 720 high and `view.width` wide.

use glam::{Vec2, Vec3, Vec4};
use loom_core::hud::{Hud, HudView, text_width};

use super::{Game, hdist};

/// Under every bar: its full length.
pub(super) const BAR_BACK: Vec4 = Vec4::new(0.02, 0.02, 0.02, 0.75);
/// The health left, on the character's bar and the bosses'.
pub(super) const BAR_HP: Vec4 = Vec4::new(0.6, 0.05, 0.05, 1.0);
/// The character's health bar trail, between the back and the health.
pub(super) const BAR_TRAIL: Vec4 = Vec4::new(0.85, 0.7, 0.35, 1.0);
pub(super) const BAR_STAMINA: Vec4 = Vec4::new(0.1, 0.5, 0.15, 1.0);
pub(super) const LOCK_DOT: Vec4 = Vec4::new(1.0, 1.0, 1.0, 0.9);

/// HUD units of bar per hp and per stamina point.
const PER_POINT: f32 = 3.0;
pub(super) const HP_BAR_AT: Vec2 = Vec2::new(40.0, 40.0);
pub(super) const HP_BAR_HEIGHT: f32 = 12.0;
pub(super) const STAMINA_BAR_AT: Vec2 = Vec2::new(40.0, 58.0);
pub(super) const STAMINA_BAR_HEIGHT: f32 = 8.0;
pub(super) const BOSS_BAR_LENGTH: f32 = 700.0;
pub(super) const BOSS_BAR_HEIGHT: f32 = 10.0;
/// How far above the bottom the first boss bar's top is.
pub(super) const BOSS_BAR_FROM_BOTTOM: f32 = 60.0;
/// How much higher each further boss bar is: room for its name.
pub(super) const BOSS_BAR_STEP: f32 = 40.0;
pub(super) const LOCK_DOT_RADIUS: f32 = 5.0;

/// The icons' sheet: a flask and a soul, 16x16 each.
pub(super) const ICONS: &str = "assets/art/souls_hud.json";
pub(super) const FLASK_ICON: &str = "flask";
pub(super) const SOUL_ICON: &str = "soul";
/// HUD units a pixel of the icons and of the font.
pub(super) const HUD_SCALE: f32 = 2.0;
/// The icons are this many HUD units a side.
pub(super) const ICON_SIZE: f32 = 32.0;
/// The flask icon's top-left corner; its count's cell is at `FLASK_TEXT_AT`.
pub(super) const FLASK_ICON_AT: Vec2 = Vec2::new(40.0, 720.0 - 72.0);
pub(super) const FLASK_TEXT_AT: Vec2 = Vec2::new(80.0, 720.0 - 63.0);
/// The flask icon's color with no drink left: a dark, empty flask.
pub(super) const FLASK_EMPTY: Vec4 = Vec4::new(0.35, 0.35, 0.35, 1.0);
/// The counts: drinks and souls.
pub(super) const COUNT_COLOR: Vec4 = Vec4::new(0.95, 0.92, 0.85, 1.0);
/// The souls count ends this far left of the right edge.
pub(super) const SOULS_RIGHT_MARGIN: f32 = 40.0;
/// Between the soul icon's right edge and the count.
pub(super) const SOUL_ICON_GAP: f32 = 8.0;
/// A boss's name, this far above its bar's top.
pub(super) const BOSS_NAME_ABOVE: f32 = 20.0;
pub(super) const BOSS_NAME_COLOR: Vec4 = Vec4::new(0.9, 0.88, 0.8, 1.0);
/// Under every text (the counts and the boss names), so it reads over the
/// pale floor.
pub(super) const TEXT_SHADOW: Vec4 = Vec4::new(0.0, 0.0, 0.0, 0.75);

/// Adds the layers of a bar at `at`, `height` high: each `(length, color)`
/// in order, from the left end, skipping those of length 0.
fn bar(hud: &mut Hud, at: Vec2, height: f32, layers: &[(f32, Vec4)]) {
    for &(length, color) in layers {
        if length > 0.0 {
            hud.rect(at, Vec2::new(length, height), color);
        }
    }
}

impl Game {
    /// The sheets the HUD draws images from, with their tags.
    pub fn hud_sheets() -> &'static [(&'static str, &'static [&'static str])] {
        &[(ICONS, &[FLASK_ICON, SOUL_ICON])]
    }

    /// The HUD of the current state for `view`, in this order: the health
    /// bar (back, trail, health), the stamina bar (back, stamina), a bar
    /// for each boss in pre-order each followed by its name, the flask
    /// icon and count, the soul icon and count, the lock-on dot. Changes
    /// nothing.
    pub fn hud(&self, view: &HudView, hud: &mut Hud) {
        let player = &self.state.player;
        let params = &self.params;

        let full = params.hp * PER_POINT;
        let hp_max = self.health.hp_max;
        bar(
            hud,
            HP_BAR_AT,
            HP_BAR_HEIGHT,
            &[
                (full, BAR_BACK),
                (full * player.hp_trail / hp_max, BAR_TRAIL),
                (full * player.health.hp / hp_max, BAR_HP),
            ],
        );
        let full = params.stamina_max * PER_POINT;
        bar(
            hud,
            STAMINA_BAR_AT,
            STAMINA_BAR_HEIGHT,
            &[
                (full, BAR_BACK),
                (full * player.stamina / params.stamina_max, BAR_STAMINA),
            ],
        );

        let x = (view.width - BOSS_BAR_LENGTH) / 2.0;
        let mut y = view.height - BOSS_BAR_FROM_BOTTOM;
        if !self.health.is_dead() {
            for e in self.enemies.iter().filter(|e| e.boss_bar) {
                let Some(health) = self.hurtboxes[e.hurtbox].health else {
                    continue;
                };
                // Enemies are root nodes: the local position is the world one.
                let at = self.scene.transform(e.id).position;
                if health.is_dead() || hdist(at, player.position) > e.params().aggro_range {
                    continue;
                }
                bar(
                    hud,
                    Vec2::new(x, y),
                    BOSS_BAR_HEIGHT,
                    &[
                        (BOSS_BAR_LENGTH, BAR_BACK),
                        (BOSS_BAR_LENGTH * health.now.hp / health.hp_max, BAR_HP),
                    ],
                );
                if let Some(name) = &self.scene.node(e.id).display_name {
                    let at = Vec2::new(x, y - BOSS_NAME_ABOVE);
                    hud.text(at, HUD_SCALE, BOSS_NAME_COLOR, name.as_str())
                        .shadow(TEXT_SHADOW);
                }
                y -= BOSS_BAR_STEP;
            }
        }

        let flask_color = if player.flask == 0 {
            FLASK_EMPTY
        } else {
            Vec4::ONE
        };
        hud.image(ICONS, FLASK_ICON, FLASK_ICON_AT, HUD_SCALE, flask_color);
        hud.text(
            FLASK_TEXT_AT,
            HUD_SCALE,
            COUNT_COLOR,
            player.flask.to_string(),
        )
        .shadow(TEXT_SHADOW);

        let souls = player.souls.to_string();
        let text_x = view.width - SOULS_RIGHT_MARGIN - text_width(&souls, HUD_SCALE);
        let icon_at = Vec2::new(text_x - SOUL_ICON_GAP - ICON_SIZE, FLASK_ICON_AT.y);
        hud.image(ICONS, SOUL_ICON, icon_at, HUD_SCALE, Vec4::ONE);
        hud.text(
            Vec2::new(text_x, FLASK_TEXT_AT.y),
            HUD_SCALE,
            COUNT_COLOR,
            souls,
        )
        .shadow(TEXT_SHADOW);

        if let Some(t) = self.lock {
            let point = self.target_position(t) + Vec3::new(0.0, self.targets[t].height, 0.0);
            if let Some(center) = view.project(point) {
                hud.disc(center, LOCK_DOT_RADIUS, LOCK_DOT);
            }
        }
    }
}
