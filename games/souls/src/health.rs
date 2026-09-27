//! Health, poise and death of the hurtboxes that have an `hp` prop: what
//! the state dump shows, the countdowns of step 0b and what a hit does
//! (`docs/architecture.md` section 11, "血量、韧性、死亡").

use loom_scene::NodeId;
use serde::Serialize;

/// Whether a hurtbox with health stands, reels or lies dead; serialized in
/// lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Condition {
    Ok,
    /// Staggered: the impacts added up past its poise.
    Stagger,
    Dead,
}

/// A hurtbox's health in the state dump (`game.hurtboxes[i].health`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct HealthState {
    /// From 0 to the full `hp`.
    pub hp: f32,
    pub state: Condition,
    /// Ticks left of the stagger or of death; 0 while `Ok`.
    pub state_ticks: u32,
    /// Impacts added up since the last stagger or reset.
    pub impact: f32,
    /// Ticks left before `impact` goes back to 0; 0 when not waiting.
    pub impact_wait: u32,
}

/// The `hp_bar` child: its scene-file `position.x` and `scale.x`, the bar at
/// full health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HpBar {
    pub id: NodeId,
    pub x0: f32,
    pub width: f32,
}

/// The health props of a hurtbox and its current state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Health {
    pub hp_max: f32,
    pub poise: f32,
    pub stagger_ticks: u32,
    pub poise_reset_ticks: u32,
    pub revive_ticks: u32,
    pub bar: Option<HpBar>,
    pub now: HealthState,
    /// Which way it falls when staggered or dead: the bearing from the
    /// character on the hit that staggered or killed it (0 at load). Not
    /// in the state dump; the pose shows it.
    pub fall: f32,
}

impl Health {
    /// Full health, `Ok`, nothing added up.
    pub fn new(
        hp_max: f32,
        poise: f32,
        [stagger_ticks, poise_reset_ticks, revive_ticks]: [u32; 3],
        bar: Option<HpBar>,
    ) -> Self {
        Self {
            hp_max,
            poise,
            stagger_ticks,
            poise_reset_ticks,
            revive_ticks,
            bar,
            now: HealthState {
                hp: hp_max,
                state: Condition::Ok,
                state_ticks: 0,
                impact: 0.0,
                impact_wait: 0,
            },
            fall: 0.0,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.now.state == Condition::Dead
    }

    /// Step 0b: the stagger or death runs out (reviving at full health),
    /// and the added-up impact is forgotten after `poise_reset_ticks`.
    pub fn count_down(&mut self) {
        let now = &mut self.now;
        if now.state != Condition::Ok {
            now.state_ticks -= 1;
            if now.state_ticks == 0 {
                if now.state == Condition::Dead {
                    now.hp = self.hp_max;
                }
                now.state = Condition::Ok;
            }
        }
        if now.impact_wait > 0 {
            now.impact_wait -= 1;
            if now.impact_wait == 0 {
                now.impact = 0.0;
            }
        }
    }

    /// A hit of `damage` and `impact` from the direction `fall` (see
    /// [`Health::fall`]): dies at 0 health, otherwise staggers once the
    /// impacts add up past the poise; either of those records `fall`.
    /// Returns whether it died.
    pub fn take(&mut self, damage: f32, impact: f32, fall: f32) -> bool {
        let now = &mut self.now;
        now.hp = f32::max(now.hp - damage, 0.0);
        if now.hp == 0.0 {
            now.state = Condition::Dead;
            now.state_ticks = self.revive_ticks;
            now.impact = 0.0;
            now.impact_wait = 0;
            self.fall = fall;
            true
        } else {
            now.impact += impact;
            now.impact_wait = self.poise_reset_ticks;
            if now.impact > self.poise {
                now.state = Condition::Stagger;
                now.state_ticks = self.stagger_ticks;
                now.impact = 0.0;
                self.fall = fall;
            }
            false
        }
    }
}
