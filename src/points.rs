use bevy::prelude::*;

use crate::state::GameState;
use crate::tuning::*;
use crate::zombie::{Attack, HitZone, Pay, ZombieHit, ZombieKilled};

/// Points paid for one hit a Zombie survives.
pub fn points_for(_hit: ZombieHit) -> u64 {
    POINTS_PER_HIT
}

/// Kill Points for a kill: by hit zone, a flat amount for the Knife, nothing if Unpaid. A Burn
/// kill pays by the zone it's dealt to, the torso.
pub fn kill_points(kill: &ZombieKilled) -> u64 {
    match (kill.pay, kill.attack, kill.zone) {
        (Pay::Unpaid, _, _) => 0,
        (Pay::Paid, Attack::Knife, _) => POINTS_KNIFE_KILL,
        (Pay::Paid, Attack::Bullet | Attack::Burn, HitZone::Limb) => POINTS_LIMB_KILL,
        (Pay::Paid, Attack::Bullet | Attack::Burn, HitZone::Torso) => POINTS_TORSO_KILL,
        (Pay::Paid, Attack::Bullet | Attack::Burn, HitZone::Neck) => POINTS_NECK_KILL,
        (Pay::Paid, Attack::Bullet | Attack::Burn, HitZone::Head) => POINTS_HEADSHOT_KILL,
    }
}

#[derive(Resource, Default)]
pub struct Points {
    /// What the Player can spend.
    pub balance: u64,
    /// Points Earned: everything gained this Run, before spending.
    pub earned: u64,
}

impl Points {
    fn award(&mut self, amount: u64) {
        self.balance = self.balance.saturating_add(amount);
        self.earned = self.earned.saturating_add(amount);
    }

    /// Takes Points for a purchase. The purchase flow checks affordability first.
    pub fn spend(&mut self, amount: u64) {
        self.balance = self.balance.saturating_sub(amount);
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
                    earned: 0,
                };
            })
            .add_systems(Update, earn);
    }
}

fn earn(
    mut hits: MessageReader<ZombieHit>,
    mut kills: MessageReader<ZombieKilled>,
    mut points: ResMut<Points>,
    mut awarded: MessageWriter<PointsAwarded>,
) {
    let amounts = hits.read().map(|hit| points_for(*hit)).chain(kills.read().map(kill_points));
    for amount in amounts.filter(|&amount| amount > 0) {
        points.award(amount);
        awarded.write(PointsAwarded(amount));
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
    fn unpaid_kills_pay_nothing() {
        for zone in [HitZone::Limb, HitZone::Torso, HitZone::Neck, HitZone::Head] {
            for attack in [Attack::Bullet, Attack::Knife] {
                assert_eq!(kill_points(&kill(Pay::Unpaid, zone, attack)), 0);
            }
        }
    }
}
