//! The Down and the Last Stand. Every Down takes every Perk (unless a stat entry keeps some). Solo,
//! holding Quick Revive, a Down starts a Last Stand: the Player crawls with Mustang & Sally and
//! can't be hurt, then Self-Revives. Without it (or a spare Self-Revive a stat entry gives), the
//! Run is over. In Co-op every Down starts a longer Last Stand that costs a slice of Points and
//! ends in a teammate's Revive (see `crate::revive`).

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::buy::buy;
use crate::modifier::{Modifier, Modifiers, Stat, resolve_count};
use crate::perk::{Drinking, Perk, Perks, drink, kept_on_down};
use crate::player::{Eye, Health, LocalPlayer, Player, take_hits};
use crate::points::Points;
use crate::respawn::{AwaitingRespawn, BledOut};
use crate::revive::Reviving;
use crate::state::{GameState, RunType, world_runs};
use crate::tuning::*;
use crate::weapon::{Loadout, WeaponCopy};

/// What a Down leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownOutcome {
    LastStand,
    GameOver,
}

/// The Player went Down. Sent for every Down, whether it starts a Last Stand or ends the Run.
#[derive(Message, Debug, Clone, Copy)]
pub struct Down {
    /// The Player who went Down.
    pub player: Entity,
    #[allow(dead_code, reason = "read by the HUD, audio and Gauntlet once they react to Downs")]
    pub outcome: DownOutcome,
}

/// The Player got back up at the end of a Last Stand.
#[derive(Message, Debug, Clone, Copy)]
pub struct SelfRevived;

/// What a Down leads to, given the Perks held at the moment of it and the spare Self-Revives
/// left (none in Classic): an unused Quick Revive life means a Last Stand, and so does a spare
/// once Quick Revive is gone; otherwise Game Over.
pub fn down_outcome(perks: &Perks, spares_left: u32) -> DownOutcome {
    if perks.has(Perk::QuickRevive) || spares_left > 0 {
        DownOutcome::LastStand
    } else {
        DownOutcome::GameOver
    }
}

/// Spare Self-Revives left: the Player's `Stat::SelfRevives` entries, less those used this Run.
pub fn spares_left(player: &[Modifier], used: u32) -> u32 {
    resolve_count(Stat::SelfRevives, 0, player).saturating_sub(used)
}

/// Spare Self-Revives a Player has used this Run: Last Stands started without Quick Revive.
#[derive(Component, Default)]
pub struct SparesUsed(pub u32);

/// What the Player holds through a Last Stand: Mustang & Sally, with a full Magazine and
/// `LAST_STAND_SPARE_MAGAZINES` more in Reserve Ammo.
pub fn last_stand_loadout() -> Loadout {
    let mut loadout = Loadout::new(LAST_STAND_WEAPON);
    let weapon: &mut WeaponCopy = loadout.held_mut();
    *weapon = WeaponCopy::new(LAST_STAND_WEAPON).upgrade();
    weapon.reserve = weapon.stats().magazine * LAST_STAND_SPARE_MAGAZINES;
    loadout.raise();
    loadout
}

/// What a Co-op Down takes from a Player holding `balance` Points: `LAST_STAND_COOP_POINTS_LOSS_PERCENT`
/// of them, rounded down.
pub fn coop_down_cost(balance: u64) -> u64 {
    balance / 100 * LAST_STAND_COOP_POINTS_LOSS_PERCENT + balance % 100 * LAST_STAND_COOP_POINTS_LOSS_PERCENT / 100
}

/// The Player is in Last Stand: crawling, untouchable, earning nothing and buying nothing. Their
/// own Weapons wait here until the Self-Revive, or a teammate's Revive in Co-op.
#[derive(Component)]
pub struct LastStand {
    elapsed: f32,
    secs: f32,
    saved: Option<Loadout>,
    /// The Points the Down took, which the Revive gives the reviver. Nothing solo.
    cost: u64,
    coop: bool,
}

impl LastStand {
    /// Starts a solo Last Stand, swapping the Player's Weapons for Mustang & Sally.
    pub fn begin(loadout: &mut Loadout) -> Self {
        Self {
            elapsed: 0.0,
            secs: LAST_STAND_SECS,
            saved: Some(std::mem::replace(loadout, last_stand_loadout())),
            cost: 0,
            coop: false,
        }
    }

    /// Starts a Co-op Last Stand for a Down that took `cost` Points.
    pub fn begin_coop(loadout: &mut Loadout, cost: u64) -> Self {
        Self { secs: LAST_STAND_COOP_SECS, cost, coop: true, ..Self::begin(loadout) }
    }

    /// Whether it ends in a teammate's Revive, not a Self-Revive.
    pub fn is_coop(&self) -> bool {
        self.coop
    }

    /// Advance the Last Stand. True once its time is up.
    pub fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        self.elapsed >= self.secs
    }

    #[allow(dead_code, reason = "read by the HUD for a Downed teammate's Bleed Out time")]
    pub fn remaining_secs(&self) -> f32 {
        (self.secs - self.elapsed).max(0.0)
    }

    /// The Points the Down took, handed over once: to whoever Revives the Player.
    pub fn take_cost(&mut self) -> u64 {
        std::mem::take(&mut self.cost)
    }

    /// Gives back the Weapons held at the Down, with the one in hand coming up again.
    pub fn self_revive(&mut self, loadout: &mut Loadout) {
        if let Some(saved) = self.saved.take() {
            *loadout = saved;
            loadout.raise();
        }
    }
}

pub struct LastStandPlugin;

impl Plugin for LastStandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Down>()
            .add_message::<BledOut>()
            .add_message::<SelfRevived>()
            .add_systems(
                Update,
                // A drink that ends in the frame of a Down is still lost, and nothing can be bought
                // in that frame either.
                ((go_down.after(take_hits).after(drink).before(buy), self_revive).in_set(HostSimulation), lower_eye)
                    .run_if(world_runs),
            );
    }
}

/// Health at zero is a Down. It takes every Perk (but any the Player's stat entries say it leaves),
/// and a Perk mid-drink with its Points. In Co-op it always starts a Last Stand, and takes a slice
/// of the Player's Points, unless the Player holds a spare Self-Revive (Undying's): that gives the
/// solo 10 s Last Stand instead, costing nothing, and they get up alone.
pub fn go_down(
    mut commands: Commands,
    mut players: Query<
        (Entity, &mut Health, &mut Perks, &mut Modifiers, &mut Loadout, &mut SparesUsed, &mut Points),
        (With<Player>, Without<LastStand>, Without<AwaitingRespawn>),
    >,
    mut downs: MessageWriter<Down>,
    mut next: ResMut<NextState<GameState>>,
    run: Res<RunType>,
) {
    for (entity, mut health, mut perks, mut modifiers, mut loadout, mut spares, mut points) in &mut players {
        if health.current > 0.0 {
            continue;
        }
        let spare = spares_left(&modifiers.0, spares.0) > 0;
        // A Co-op Down is the long Last Stand for a teammate to Revive, unless a spare Self-Revive
        // turns it into the solo one.
        let coop = run.is_coop() && !spare;
        let outcome = if run.is_coop() { DownOutcome::LastStand } else { down_outcome(&perks, spares_left(&modifiers.0, spares.0)) };
        let uses_spare = if run.is_coop() { spare } else { outcome == DownOutcome::LastStand && !perks.has(Perk::QuickRevive) };
        if uses_spare {
            spares.0 = spares.0.saturating_add(1);
        }
        let keep = kept_on_down(&modifiers.0);
        perks.lose_on_down(keep, &mut modifiers);
        commands.entity(entity).remove::<(Drinking, Reviving)>();
        downs.write(Down { player: entity, outcome });
        match outcome {
            DownOutcome::GameOver => next.set(GameState::GameOver),
            DownOutcome::LastStand => {
                health.current = 0.0;
                let stand = if coop {
                    let cost = coop_down_cost(points.balance);
                    points.spend(cost);
                    LastStand::begin_coop(&mut loadout, cost)
                } else {
                    LastStand::begin(&mut loadout)
                };
                commands.entity(entity).insert(stand);
            }
        }
    }
}

/// The end of a Last Stand: 100 Health, the old Weapons back, no Points taken. A Co-op one that
/// runs out is a Bleed Out instead (see `crate::respawn`).
pub(crate) fn self_revive(
    mut commands: Commands,
    mut players: Query<(Entity, &mut LastStand, &mut Loadout, &mut Health), With<Player>>,
    mut revived: MessageWriter<SelfRevived>,
    mut bled_out: MessageWriter<BledOut>,
    time: Res<Time>,
) {
    for (entity, mut stand, mut loadout, mut health) in &mut players {
        if !stand.advance(time.delta_secs()) {
            continue;
        }
        // A Co-op Last Stand ends in a Revive or a Bleed Out, never a Self-Revive.
        if stand.is_coop() {
            bled_out.write(BledOut { player: entity });
            continue;
        }
        stand.self_revive(&mut loadout);
        health.current = SELF_REVIVE_HEALTH.min(health.max);
        commands.entity(entity).remove::<LastStand>();
        revived.write(SelfRevived);
    }
}

/// The view drops to the floor for the crawl and comes back up after.
pub fn lower_eye(player: Single<Has<LastStand>, With<LocalPlayer>>, mut eye: Single<&mut Eye>, time: Res<Time>) {
    let target = if *player { PLAYER_EYE_HEIGHT - LAST_STAND_EYE_DROP } else { PLAYER_EYE_HEIGHT };
    let step = LAST_STAND_EYE_DROP * 3.0 * time.delta_secs();
    let y = eye.height;
    if y != target {
        eye.height = if y < target { (y + step).min(target) } else { (y - step).max(target) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arsenal::WeaponId;

    #[test]
    fn a_down_with_quick_revive_starts_a_last_stand() {
        let perks = Perks(vec![Perk::Juggernog, Perk::QuickRevive]);
        assert_eq!(down_outcome(&perks, 0), DownOutcome::LastStand);
    }

    #[test]
    fn a_down_without_a_life_is_game_over() {
        assert_eq!(down_outcome(&Perks::default(), 0), DownOutcome::GameOver);
        let perks = Perks(vec![Perk::Juggernog, Perk::SpeedCola, Perk::DoubleTap]);
        assert_eq!(down_outcome(&perks, 0), DownOutcome::GameOver);
    }

    #[test]
    fn a_down_uses_up_the_life_and_every_perk() {
        let mut perks = Perks(Perk::ALL.to_vec());
        let mut modifiers = Modifiers(Perk::ALL.iter().flat_map(|perk| perk.modifiers()).copied().collect());
        assert_eq!(down_outcome(&perks, 0), DownOutcome::LastStand);
        perks.lose_on_down(0, &mut modifiers);
        assert!(perks.0.is_empty());
        assert!(modifiers.0.is_empty());
        // So the next Down, with no Quick Revive bought since, ends the Run.
        assert_eq!(down_outcome(&perks, 0), DownOutcome::GameOver);
    }

    #[test]
    fn spare_self_revives_start_last_stands_without_quick_revive_until_used_up() {
        use crate::modifier::{Effect, Source};
        let two = [Modifier { stat: Stat::SelfRevives, source: Source("A"), effect: Effect::Flat(2.0) }];
        assert_eq!(spares_left(&[], 0), 0);
        assert_eq!(spares_left(&two, 0), 2);
        assert_eq!(spares_left(&two, 2), 0);
        assert_eq!(spares_left(&two, 5), 0);
        let none = Perks::default();
        assert_eq!(down_outcome(&none, spares_left(&two, 1)), DownOutcome::LastStand);
        assert_eq!(down_outcome(&none, spares_left(&two, 2)), DownOutcome::GameOver);
    }

    #[test]
    fn the_last_stand_hands_over_mustang_and_sally_with_two_spare_magazines() {
        let loadout = last_stand_loadout();
        let held = loadout.held();
        assert_eq!(held.stats().name, "Mustang & Sally");
        assert_eq!((held.magazine, held.reserve), (12, 24));
        assert!(loadout.slot(1).is_none());
    }

    #[test]
    fn the_self_revive_gives_back_the_weapons_held_at_the_down() {
        let mut loadout = Loadout::new(WeaponId::M1911);
        loadout.give(WeaponId::Mp40);
        loadout.held_mut().magazine = 7;
        let none: &[crate::modifier::Modifier] = &[];
        loadout.start_reload(none);

        let mut stand = LastStand::begin(&mut loadout);
        assert_eq!(loadout.held().stats().name, "Mustang & Sally");
        loadout.held_mut().magazine = 0;

        stand.self_revive(&mut loadout);
        assert_eq!(loadout.held().id, WeaponId::Mp40);
        assert_eq!(loadout.held().magazine, 7);
        assert_eq!(loadout.slot(0).unwrap().id, WeaponId::M1911);
        // It comes back up with the Reload cancelled.
        assert!(loadout.reload.is_none());
        assert!(loadout.swap.is_some());
    }

    #[test]
    fn a_co_op_down_takes_five_percent_of_the_points_rounded_down() {
        assert_eq!(coop_down_cost(0), 0);
        assert_eq!(coop_down_cost(500), 25);
        assert_eq!(coop_down_cost(10_000), 500);
        assert_eq!(coop_down_cost(1_234), 61);
        assert_eq!(coop_down_cost(19), 0);
    }

    #[test]
    fn a_co_op_last_stand_lasts_forty_five_seconds_and_holds_the_cost_once() {
        let mut stand = LastStand::begin_coop(&mut Loadout::new(WeaponId::M1911), 75);
        assert!(stand.is_coop());
        assert!(!stand.advance(LAST_STAND_COOP_SECS - 0.1));
        assert!(stand.advance(0.1));
        assert_eq!(stand.take_cost(), 75);
        assert_eq!(stand.take_cost(), 0);
    }

    #[test]
    fn the_last_stand_lasts_ten_seconds() {
        let mut stand = LastStand::begin(&mut Loadout::new(WeaponId::M1911));
        assert!(!stand.advance(LAST_STAND_SECS - 0.1));
        assert!(stand.advance(0.1));
    }
}
