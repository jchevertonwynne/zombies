use bevy::prelude::*;

use crate::last_stand::LastStand;
use crate::player::Player;
use crate::state::GameState;
use crate::tuning::*;
use crate::zombie::{Attack, HitZone, Pay, ZombieHit, ZombieKilled};

/// Points paid for one hit a Zombie survives.
pub fn points_for(_hit: ZombieHit) -> u64 {
    POINTS_PER_HIT
}

/// Kill Points for a kill: by hit zone, a flat amount for the Knife, nothing if Unpaid (a Trap's,
/// say). A Burn kill pays by the zone it's dealt to, the torso.
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

/// What an award of `amount` really pays: twice as much while Double Points is on.
pub fn award_amount(amount: u64, double: bool) -> u64 {
    if double { amount.saturating_mul(2) } else { amount }
}

#[derive(Resource, Default)]
pub struct Points {
    /// What the Player can spend.
    pub balance: u64,
    /// Points Earned: everything gained this Run, before spending.
    pub earned: u64,
    /// Whether Double Points is on, doubling every award. The Power-ups own it.
    pub double: bool,
}

impl Points {
    /// Pays an award, doubled while Double Points is on, and returns what was paid.
    pub fn award(&mut self, amount: u64) -> u64 {
        let amount = award_amount(amount, self.double);
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

/// Points were awarded (for HUD popups).
#[derive(Message)]
pub struct PointsAwarded(pub u64);

pub struct PointsPlugin;

impl Plugin for PointsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Points>()
            .add_message::<PointsAwarded>()
            .add_systems(OnEnter(GameState::Playing), |mut points: ResMut<Points>| {
                *points = Points {
                    balance: STARTING_POINTS,
                    ..default()
                };
            })
            .add_systems(Update, earn);
    }
}

/// Hits and kills pay, except in Last Stand, which earns nothing.
fn earn(
    mut hits: MessageReader<ZombieHit>,
    mut kills: MessageReader<ZombieKilled>,
    last_stand: Query<(), (With<Player>, With<LastStand>)>,
    mut points: ResMut<Points>,
    mut awarded: MessageWriter<PointsAwarded>,
) {
    let earning = last_stand.is_empty();
    let amounts = hits.read().map(|hit| points_for(*hit)).chain(kills.read().map(kill_points));
    for amount in amounts.filter(|&amount| amount > 0 && earning) {
        let paid = points.award(amount);
        awarded.write(PointsAwarded(paid));
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
        assert_eq!(points_for(ZombieHit), 10);
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
        assert_eq!(award_amount(60, false), 60);
        assert_eq!(award_amount(60, true), 120);
        assert_eq!(award_amount(u64::MAX, true), u64::MAX);

        let mut points = Points {
            double: true,
            ..Points::default()
        };
        assert_eq!(points.award(400), 800);
        assert_eq!((points.balance, points.earned), (800, 800));
    }

    #[test]
    fn nuke_kills_are_unpaid() {
        assert_eq!(crate::zombie::pay_for(Attack::Nuke), Pay::Unpaid);
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
