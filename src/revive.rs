//! Reviving a teammate in Co-op. A Player holds Use within `REVIVE_REACH` of a teammate in Last
//! Stand for `REVIVE_SECS` (`REVIVE_QUICK_SECS` with Quick Revive) and gets them back up with their
//! Weapons and full Health, and the Points the Down cost.
//!
//! The held Use key is a `UseRequest` every frame, naming the downed Player as the thing used,
//! the way a Barrier's rebuild asks once per Plank. The Host counts the frames it keeps coming:
//! letting go ends the request and resets the hold, and being hit doesn't.

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::buy::{UseRequest, focus};
use crate::controls::{Action, Controls};
use crate::last_stand::{LastStand, go_down};
use crate::perk::{Perk, Perks};
use crate::player::{Health, LocalPlayer, Player};
use crate::points::Points;
use crate::state::{GameState, input_runs, world_runs};
use crate::tuning::*;
use crate::weapon::Loadout;

/// A Player is holding Use over a teammate in Last Stand: the Revive in progress. It's gone the
/// moment they let go, step away, or go Down themselves.
#[derive(Component, Debug)]
pub struct Reviving {
    pub target: Entity,
    elapsed: f32,
    needed: f32,
}

impl Reviving {
    /// How far through the Revive it is, 0 to 1.
    #[allow(dead_code, reason = "read by the HUD once it shows a Revive in progress")]
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.needed).min(1.0)
    }
}

/// The Player got up from a Last Stand because `reviver` Revived them.
#[derive(Message, Debug, Clone, Copy)]
pub struct Revived {
    #[allow(dead_code, reason = "read by the HUD and audio once they react to Revives")]
    pub player: Entity,
    pub reviver: Entity,
}

/// The teammate in Last Stand the `LocalPlayer` can Revive, if any. Their Use key goes to the
/// Revive before any Buyable.
#[derive(Resource, Default)]
pub struct ReviveFocus(pub Option<Entity>);

/// How long a Revive takes for a reviver who does (or doesn't) hold Quick Revive.
pub fn revive_secs(quick_revive: bool) -> f32 {
    if quick_revive { REVIVE_QUICK_SECS } else { REVIVE_SECS }
}

/// The nearest of `downed` (entity and position) within `REVIVE_REACH` of `from`.
pub fn revive_target(from: Vec3, downed: impl IntoIterator<Item = (Entity, Vec3)>) -> Option<Entity> {
    downed
        .into_iter()
        .map(|(entity, at)| (entity, from.distance(at)))
        .filter(|&(_, distance)| distance <= REVIVE_REACH)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(entity, _)| entity)
}

/// Run condition: the `LocalPlayer` isn't Reviving, so their Weapon can fire.
pub fn not_reviving(reviving: Query<(), (With<LocalPlayer>, With<Reviving>)>) -> bool {
    reviving.is_empty()
}

pub struct RevivePlugin;

impl Plugin for RevivePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReviveFocus>()
            .add_message::<Revived>()
            .add_systems(
                Update,
                (
                    // A paused Player's hold is cancelled: no input, no requests.
                    (aim_revive.before(focus), ask_revive.after(focus)).chain().run_if(in_state(GameState::Playing)).run_if(input_runs),
                    revive.in_set(HostSimulation).after(go_down),
                )
                    .run_if(world_runs),
            );
    }
}

/// Picks the teammate in Last Stand within reach of the `LocalPlayer`, who must be up themselves.
fn aim_revive(
    local: Single<(&Transform, Has<LastStand>), With<LocalPlayer>>,
    downed: Query<(Entity, &Transform, &LastStand), With<Player>>,
    mut aim: ResMut<ReviveFocus>,
) {
    let (transform, down) = *local;
    let picked = if down {
        None
    } else {
        revive_target(
            transform.translation,
            downed.iter().filter(|(_, _, stand)| stand.is_coop()).map(|(entity, at, _)| (entity, at.translation)),
        )
    };
    if aim.0 != picked {
        aim.0 = picked;
    }
}

/// Holding Use over that teammate asks to Revive them, every frame it's held.
fn ask_revive(
    controls: Controls,
    aim: Res<ReviveFocus>,
    local: Single<Entity, With<LocalPlayer>>,
    mut requests: MessageWriter<UseRequest>,
) {
    if let Some(target) = aim.0.filter(|_| controls.pressed(Action::Use)) {
        requests.write(UseRequest { player: *local, buyable: target, hold: true });
    }
}

/// Counts each reviver's hold. Whoever's request stops, steps out of reach or goes Down starts
/// over; whoever holds long enough gets the teammate up with their Weapons and full Health, and
/// the Points the Down cost.
pub(crate) fn revive(
    mut commands: Commands,
    mut requests: MessageReader<UseRequest>,
    mut revivers: Query<(Entity, &Transform, &Perks, &mut Points, Option<&mut Reviving>), (With<Player>, Without<LastStand>)>,
    mut downed: Query<(&Transform, &mut LastStand, &mut Loadout, &mut Health), With<Player>>,
    mut revived: MessageWriter<Revived>,
    time: Res<Time>,
) {
    // Each reviver's latest request to use a teammate in Last Stand.
    let holds: Vec<(Entity, Entity)> = requests
        .read()
        .filter(|request| request.hold && downed.contains(request.buyable))
        .map(|request| (request.player, request.buyable))
        .collect();
    let mut done = Vec::new();
    for (entity, transform, perks, mut points, reviving) in &mut revivers {
        let target = holds.iter().rev().find(|(reviver, _)| *reviver == entity).map(|&(_, target)| target);
        let stand = target.filter(|target| !done.contains(target)).and_then(|target| {
            let (at, stand, ..) = downed.get(target).ok()?;
            (stand.is_coop() && transform.translation.distance(at.translation) <= REVIVE_REACH).then_some(target)
        });
        let Some(target) = stand else {
            if reviving.is_some() {
                commands.entity(entity).remove::<Reviving>();
            }
            continue;
        };
        let needed = revive_secs(perks.has(Perk::QuickRevive));
        let elapsed = match &reviving {
            Some(reviving) if reviving.target == target => reviving.elapsed + time.delta_secs(),
            _ => time.delta_secs(),
        };
        if elapsed < needed {
            match reviving {
                Some(mut reviving) if reviving.target == target => {
                    reviving.elapsed = elapsed;
                    reviving.needed = needed;
                }
                _ => {
                    commands.entity(entity).insert(Reviving { target, elapsed, needed });
                }
            }
            continue;
        }
        let Ok((_, mut stand, mut loadout, mut health)) = downed.get_mut(target) else { continue };
        stand.self_revive(&mut loadout);
        health.current = health.max;
        points.refund(stand.take_cost());
        commands.entity(target).remove::<LastStand>();
        commands.entity(entity).remove::<Reviving>();
        revived.write(Revived { player: target, reviver: entity });
        done.push(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_revive_halves_the_time() {
        assert_eq!(revive_secs(false), 3.0);
        assert_eq!(revive_secs(true), 1.5);
    }

    #[test]
    fn only_a_teammate_within_two_metres_can_be_revived_and_the_nearest_is_picked() {
        let (near, far, out) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap(), Entity::from_raw_u32(3).unwrap());
        let downed = [(far, Vec3::new(1.9, 0.0, 0.0)), (near, Vec3::new(0.0, 0.0, 0.5)), (out, Vec3::new(2.1, 0.0, 0.0))];
        assert_eq!(revive_target(Vec3::ZERO, downed), Some(near));
        assert_eq!(revive_target(Vec3::ZERO, [(far, Vec3::new(1.9, 0.0, 0.0)), (out, Vec3::new(2.1, 0.0, 0.0))]), Some(far));
        assert_eq!(revive_target(Vec3::ZERO, [(out, Vec3::new(2.1, 0.0, 0.0))]), None);
    }
}
