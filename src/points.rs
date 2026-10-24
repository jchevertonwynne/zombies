use bevy::prelude::*;

use crate::state::GameState;
use crate::tuning::*;
use crate::zombie::{HitKind, ZombieHit};

/// Points paid for one hit on a Zombie.
pub fn points_for(hit: ZombieHit) -> u32 {
    match (hit.killed, hit.kind) {
        (false, _) => POINTS_PER_HIT,
        (true, HitKind::Body) => POINTS_BODY_KILL,
        (true, HitKind::Head) => POINTS_HEADSHOT_KILL,
        (true, HitKind::Knife) => POINTS_KNIFE_KILL,
    }
}

#[derive(Resource, Default)]
pub struct Points(pub u32);

#[derive(Message)]
pub struct PointsEarned(pub u32);

pub struct PointsPlugin;

impl Plugin for PointsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Points>()
            .add_message::<PointsEarned>()
            .add_systems(OnEnter(GameState::Playing), |mut points: ResMut<Points>| {
                points.0 = 0
            })
            .add_systems(Update, earn);
    }
}

fn earn(
    mut hits: MessageReader<ZombieHit>,
    mut points: ResMut<Points>,
    mut earned: MessageWriter<PointsEarned>,
) {
    for hit in hits.read() {
        let amount = points_for(*hit);
        points.0 += amount;
        earned.write(PointsEarned(amount));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(kind: HitKind, killed: bool) -> ZombieHit {
        ZombieHit { kind, killed }
    }

    #[test]
    fn any_non_lethal_hit_pays_ten() {
        for kind in [HitKind::Body, HitKind::Head, HitKind::Knife] {
            assert_eq!(points_for(hit(kind, false)), 10);
        }
    }

    #[test]
    fn kills_pay_by_how_they_landed() {
        assert_eq!(points_for(hit(HitKind::Body, true)), 60);
        assert_eq!(points_for(hit(HitKind::Head, true)), 100);
        assert_eq!(points_for(hit(HitKind::Knife, true)), 130);
    }
}
