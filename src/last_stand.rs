//! The Down and the Last Stand. Every Down takes every Perk (unless a stat entry keeps some). Holding Quick Revive, a Down starts
//! a Last Stand: the Player crawls with Mustang & Sally and can't be hurt, then Self-Revives.
//! Without it (or a spare Self-Revive a stat entry gives), the Run is over.

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::buy::buy;
use crate::modifier::{Modifier, Modifiers, Stat, resolve_count};
use crate::perk::{Drinking, Perk, Perks, drink, kept_on_down};
use crate::player::{Eye, Health, LocalPlayer, Player, take_hits};
use crate::state::{GameState, world_runs};
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

/// The Player is in Last Stand: crawling, untouchable, earning nothing and buying nothing. Their
/// own Weapons wait here until the Self-Revive.
#[derive(Component)]
pub struct LastStand {
    elapsed: f32,
    saved: Option<Loadout>,
}

impl LastStand {
    /// Starts a Last Stand, swapping the Player's Weapons for Mustang & Sally.
    pub fn begin(loadout: &mut Loadout) -> Self {
        Self {
            elapsed: 0.0,
            saved: Some(std::mem::replace(loadout, last_stand_loadout())),
        }
    }

    /// Advance the Last Stand. True once it's over.
    pub fn advance(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        self.elapsed >= LAST_STAND_SECS
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
/// and a Perk mid-drink with its Points.
pub fn go_down(
    mut commands: Commands,
    mut players: Query<
        (Entity, &mut Health, &mut Perks, &mut Modifiers, &mut Loadout, &mut SparesUsed),
        (With<Player>, Without<LastStand>),
    >,
    mut downs: MessageWriter<Down>,
    mut next: ResMut<NextState<GameState>>,
) {
    for (entity, mut health, mut perks, mut modifiers, mut loadout, mut spares) in &mut players {
        if health.current > 0.0 {
            continue;
        }
        let outcome = down_outcome(&perks, spares_left(&modifiers.0, spares.0));
        if outcome == DownOutcome::LastStand && !perks.has(Perk::QuickRevive) {
            spares.0 = spares.0.saturating_add(1);
        }
        let keep = kept_on_down(&modifiers.0);
        perks.lose_on_down(keep, &mut modifiers);
        commands.entity(entity).remove::<Drinking>();
        downs.write(Down { player: entity, outcome });
        match outcome {
            DownOutcome::GameOver => next.set(GameState::GameOver),
            DownOutcome::LastStand => {
                health.current = 0.0;
                commands.entity(entity).insert(LastStand::begin(&mut loadout));
            }
        }
    }
}

/// The end of a Last Stand: 100 Health, the old Weapons back, no Points taken.
fn self_revive(
    mut commands: Commands,
    mut players: Query<(Entity, &mut LastStand, &mut Loadout, &mut Health), With<Player>>,
    mut revived: MessageWriter<SelfRevived>,
    time: Res<Time>,
) {
    for (entity, mut stand, mut loadout, mut health) in &mut players {
        if !stand.advance(time.delta_secs()) {
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
    fn the_last_stand_lasts_ten_seconds() {
        let mut stand = LastStand::begin(&mut Loadout::new(WeaponId::M1911));
        assert!(!stand.advance(LAST_STAND_SECS - 0.1));
        assert!(stand.advance(0.1));
    }
}
