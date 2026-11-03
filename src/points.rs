use bevy::prelude::*;

use crate::last_stand::LastStand;
use crate::modifier::{Modifiers, RunModifiers, Stat, resolve, resolve_points};
use crate::player::{LocalPlayer, Player};
use crate::tuning::*;
use crate::zombie::{Attack, HitZone, Pay, ZombieHit, ZombieKilled};

/// Points paid for one hit a Zombie survives, if the hit pays at all.
pub fn points_for(hit: ZombieHit) -> u64 {
    if hit.pays { POINTS_PER_HIT } else { 0 }
}

/// Kill Points for a kill: by hit zone, a flat amount for the Knife, nothing if Unpaid (a Trap's,
/// say). A Burn kill pays by the zone it's dealt to, the torso, and so does a Paid Ammo Mod kill.
pub fn kill_points(kill: &ZombieKilled) -> u64 {
    match (kill.pay, kill.attack, kill.zone) {
        (Pay::Unpaid, _, _) => 0,
        (Pay::Paid, Attack::Knife, _) => POINTS_KNIFE_KILL,
        (Pay::Paid, _, HitZone::Limb) => POINTS_LIMB_KILL,
        (Pay::Paid, _, HitZone::Torso) => POINTS_TORSO_KILL,
        (Pay::Paid, _, HitZone::Neck) => POINTS_NECK_KILL,
        (Pay::Paid, _, HitZone::Head) => POINTS_HEADSHOT_KILL,
    }
}

/// What an award of `amount` really pays: twice as much while Double Points is on, and `extra`
/// more multiples of it from stat entries, added to Double Points' rather than multiplied.
pub fn award_amount(amount: u64, double: bool, extra: f32) -> u64 {
    let rate = 1.0 + f64::from(extra.max(0.0)) + if double { 1.0 } else { 0.0 };
    if rate.fract() == 0.0 {
        amount.saturating_mul(rate as u64)
    } else {
        // A float-to-int cast saturates, so this never wraps either.
        (amount as f64 * rate).round() as u64
    }
}

/// A Player's Points.
#[derive(Component, Default)]
pub struct Points {
    /// What the Player can spend.
    pub balance: u64,
    /// Points Earned: everything gained this Run, before spending.
    pub earned: u64,
    /// Whether Double Points is on, doubling every award. The Power-ups own it.
    pub double: bool,
    /// Extra multiples of every award, from the Player's `Stat::PointsRate` entries.
    pub extra_rate: f32,
}

impl Points {
    /// Pays an award, doubled while Double Points is on, and returns what was paid.
    pub fn award(&mut self, amount: u64) -> u64 {
        let amount = award_amount(amount, self.double, self.extra_rate);
        self.balance = self.balance.saturating_add(amount);
        self.earned = self.earned.saturating_add(amount);
        amount
    }

    /// Takes Points for a purchase. The purchase flow checks affordability first.
    pub fn spend(&mut self, amount: u64) {
        self.balance = self.balance.saturating_sub(amount);
    }

    /// Gives back Points spent, like a Teddy Bear's refund. It isn't earned, so Points Earned
    /// stays the same.
    pub fn refund(&mut self, amount: u64) {
        self.balance = self.balance.saturating_add(amount);
    }
}

impl Points {
    /// A new Run's Points: the starting balance and nothing Earned.
    pub fn starting(run: &[crate::modifier::Modifier]) -> Self {
        Self {
            balance: resolve_points(Stat::StartingPoints, STARTING_POINTS, run),
            ..default()
        }
    }
}

/// Points paid to `player` by a rule layered on Classic, awarded like any other Points (so none in
/// Last Stand).
#[derive(Message)]
pub struct BonusPoints {
    pub player: Entity,
    pub amount: u64,
}

/// Points were awarded to `player` (for HUD popups).
#[derive(Message)]
pub struct PointsAwarded {
    pub player: Entity,
    pub amount: u64,
}

pub struct PointsPlugin;

impl Plugin for PointsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunModifiers>()
            .add_message::<PointsAwarded>()
            .add_message::<BonusPoints>()
            .add_systems(Update, (resolve_rate, earn).chain());
    }
}

/// Every award's extra multiples follow the Player's stat entries.
fn resolve_rate(mut players: Query<(&Modifiers, &mut Points), With<Player>>) {
    for (modifiers, mut points) in &mut players {
        let extra = resolve(Stat::PointsRate, 0.0, &modifiers.0);
        if points.extra_rate != extra {
            points.extra_rate = extra;
        }
    }
}

/// Hits, kills and bonus Points pay, except in Last Stand, which earns nothing. Hits and kills pay
/// the `LocalPlayer`; a bonus pays the Player it names.
fn earn(
    mut hits: MessageReader<ZombieHit>,
    mut kills: MessageReader<ZombieKilled>,
    mut bonus: MessageReader<BonusPoints>,
    local: Single<Entity, With<LocalPlayer>>,
    mut players: Query<(&mut Points, Has<LastStand>), With<Player>>,
    mut awarded: MessageWriter<PointsAwarded>,
) {
    let local = *local;
    let amounts = hits
        .read()
        .map(|hit| (local, points_for(*hit)))
        .chain(kills.read().map(|kill| (local, kill_points(kill))))
        .chain(bonus.read().map(|bonus| (bonus.player, bonus.amount)));
    for (player, amount) in amounts.filter(|&(_, amount)| amount > 0) {
        let Ok((mut points, last_stand)) = players.get_mut(player) else {
            continue;
        };
        if last_stand {
            continue;
        }
        let paid = points.award(amount);
        awarded.write(PointsAwarded { player, amount: paid });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kill(pay: Pay, zone: HitZone, attack: Attack) -> ZombieKilled {
        ZombieKilled {
            pay,
            headshot: crate::zombie::is_headshot(zone, attack),
            zone,
            attack,
            at: Vec3::ZERO,
        }
    }

    #[test]
    fn any_non_lethal_hit_pays_ten() {
        assert_eq!(points_for(ZombieHit { pays: true, headshot: false }), 10);
        assert_eq!(points_for(ZombieHit { pays: false, headshot: false }), 0);
    }

    #[test]
    fn paid_bullet_kills_pay_by_hit_zone() {
        let paid = |zone| kill_points(&kill(Pay::Paid, zone, Attack::Bullet));
        assert_eq!(paid(HitZone::Limb), 50);
        assert_eq!(paid(HitZone::Torso), 60);
        assert_eq!(paid(HitZone::Neck), 70);
        assert_eq!(paid(HitZone::Head), 100);
    }

    #[test]
    fn knife_kills_pay_130_wherever_they_land() {
        for zone in [HitZone::Limb, HitZone::Torso, HitZone::Neck, HitZone::Head] {
            assert_eq!(kill_points(&kill(Pay::Paid, zone, Attack::Knife)), 130);
        }
    }

    #[test]
    fn burn_kills_are_paid_torso_kills_and_never_headshots() {
        let burn = kill(Pay::Paid, HitZone::Torso, Attack::Burn);
        assert_eq!(kill_points(&burn), 60);
        assert!(!kill(Pay::Paid, HitZone::Head, Attack::Burn).headshot);
    }

    #[test]
    fn double_points_doubles_every_award() {
        assert_eq!(award_amount(60, false, 0.0), 60);
        assert_eq!(award_amount(60, true, 0.0), 120);
        assert_eq!(award_amount(u64::MAX, true, 0.0), u64::MAX);

        let mut points = Points {
            double: true,
            ..Points::default()
        };
        assert_eq!(points.award(400), 800);
        assert_eq!((points.balance, points.earned), (800, 800));
    }

    #[test]
    fn an_extra_rate_adds_to_double_points_rather_than_multiplying() {
        // One extra multiple: ×2, and ×3 (not ×4) during Double Points.
        assert_eq!(award_amount(60, false, 1.0), 120);
        assert_eq!(award_amount(60, true, 1.0), 180);
        assert_eq!(award_amount(10, false, 0.5), 15);
        assert_eq!(award_amount(u64::MAX, true, 1.0), u64::MAX);
    }

    #[test]
    fn nuke_kills_are_unpaid() {
        assert_eq!(Attack::Nuke.pay(), Pay::Unpaid);
        assert_eq!(kill_points(&kill(Pay::Unpaid, HitZone::Head, Attack::Nuke)), 0);
    }

    #[test]
    fn insta_kill_makes_any_damage_lethal_and_kills_still_pay_by_hit_zone() {
        use crate::zombie::dealt_damage;
        assert_eq!(dealt_damage(20.0, false), 20.0);
        assert!(dealt_damage(1.0, true) >= f32::MAX);
        assert_eq!(dealt_damage(0.0, true), 0.0);
        // The kill it makes still pays by where it landed.
        assert_eq!(kill_points(&kill(Pay::Paid, HitZone::Limb, Attack::Bullet)), 50);
        assert_eq!(kill_points(&kill(Pay::Paid, HitZone::Head, Attack::Bullet)), 100);
    }

    #[test]
    fn unpaid_kills_pay_nothing() {
        for zone in [HitZone::Limb, HitZone::Torso, HitZone::Neck, HitZone::Head] {
            for attack in [Attack::Bullet, Attack::Knife, Attack::Trap, Attack::Nuke] {
                assert_eq!(kill_points(&kill(Pay::Unpaid, zone, attack)), 0);
            }
        }
    }
}
