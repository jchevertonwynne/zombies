//! Bleeding Out and coming back, in Co-op. A Co-op Last Stand that runs out is a Bleed Out: the
//! Player keeps their Points, every other Player loses a slice of theirs, and the Player waits to
//! Respawn with no body, watching a teammate. At the start of the next Round they Respawn beside a
//! teammate who is up, with the starting loadout and no Perks. The Run is over the moment no
//! Player is up.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::equipment::{Equipment, FragLimits};
use crate::explosive::world_filter;
use crate::last_stand::{LastStand, go_down, self_revive};
use crate::modifier::Modifiers;
use crate::perk::Perks;
use crate::player::{Health, LocalPlayer, Locomotion, Player};
use crate::points::Points;
use crate::round::{Round, RoundStarted};
use crate::state::{GameState, RunType, world_runs};
use crate::target::is_up;
use crate::tuning::*;
use crate::weapon::{Knife, Loadout};

/// A Player waiting to Respawn: they have no body, no name tag, can't be targeted or hurt, and
/// don't hold any Zone open. It stays until the next Round starts.
#[derive(Component)]
pub struct AwaitingRespawn;

/// The Player's Co-op Last Stand ran out with no Revive.
#[derive(Message, Debug, Clone, Copy)]
pub struct BledOut {
    pub player: Entity,
}

/// The Player came back at the start of a Round, beside a teammate.
#[derive(Message, Debug, Clone, Copy)]
pub struct Respawned {
    #[allow(dead_code, reason = "read by the HUD and audio once they react to Respawns")]
    pub player: Entity,
}

/// Run condition: the `LocalPlayer` isn't waiting to Respawn, so their Weapon can fire.
pub fn not_waiting(waiting: Query<(), (With<LocalPlayer>, With<AwaitingRespawn>)>) -> bool {
    waiting.is_empty()
}

/// What a Bleed Out takes from a teammate holding `balance` Points: `BLEED_OUT_TEAM_LOSS_FRACTION`
/// of them, rounded down.
pub fn bleed_out_loss(balance: u64) -> u64 {
    (balance as f64 * f64::from(BLEED_OUT_TEAM_LOSS_FRACTION)).floor() as u64
}

/// A Respawned Player's Points, given the Round they come back in: raised to
/// `RESPAWN_POINTS_FLOOR` from Round `RESPAWN_FLOOR_FROM_ROUND`.
pub fn respawn_points(balance: u64, round: u32) -> u64 {
    if round >= RESPAWN_FLOOR_FROM_ROUND { balance.max(RESPAWN_POINTS_FLOOR) } else { balance }
}

pub struct RespawnPlugin;

impl Plugin for RespawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BledOut>().add_message::<Respawned>().add_systems(
            Update,
            (
                (
                    bleed_out.after(self_revive),
                    respawn,
                    game_over_when_none_up.after(go_down).after(bleed_out),
                )
                    .chain()
                    .in_set(HostSimulation),
                watch_teammate,
            )
                .run_if(world_runs),
        );
    }
}

/// A Player whose Last Stand ran out loses their Last Stand, body and Weapons, and every other
/// Player loses `BLEED_OUT_TEAM_LOSS_FRACTION` of their Points. They keep their own.
fn bleed_out(
    mut commands: Commands,
    mut bled_out: MessageReader<BledOut>,
    mut players: Query<(Entity, &mut Points), With<Player>>,
) {
    for &BledOut { player } in bled_out.read() {
        for (entity, mut points) in &mut players {
            if entity != player {
                let loss = bleed_out_loss(points.balance);
                points.spend(loss);
            }
        }
        // Gone from the world until the next Round: no body to hit, no Last Stand to Revive.
        commands.entity(player).remove::<LastStand>().insert((AwaitingRespawn, ColliderDisabled));
    }
}

/// At the start of a Round every Player waiting to Respawn comes back beside a teammate who is up:
/// full Health, the Knife, the starting Weapon and Equipment, no Perks, and the Points floor.
fn respawn(
    mut started: MessageReader<RoundStarted>,
    mut commands: Commands,
    spatial: SpatialQuery,
    round: Res<Round>,
    mut players: Query<
        (
            Entity,
            &mut Transform,
            &mut Health,
            &mut Loadout,
            &mut Knife,
            &mut Equipment,
            &mut Perks,
            &mut Points,
            &mut Locomotion,
            &Modifiers,
            Has<AwaitingRespawn>,
            Has<LastStand>,
        ),
        With<Player>,
    >,
    mut respawned: MessageWriter<Respawned>,
) {
    if started.read().count() == 0 {
        return;
    }
    let anchors: Vec<Transform> = players
        .iter()
        .filter(|(.., awaiting, last_stand)| is_up(*last_stand, *awaiting))
        .map(|(_, transform, ..)| *transform)
        .collect();
    let Some(&beside) = anchors.first() else { return };
    for (entity, mut transform, mut health, mut loadout, mut knife, mut equipment, mut perks, mut points, mut locomotion, modifiers, awaiting, _) in
        &mut players
    {
        if !awaiting {
            continue;
        }
        let spot = respawn_spot(&beside, &spatial);
        transform.translation = spot;
        // The body's physics position moves with it, or the old one wins when it's enabled again.
        commands.entity(entity).insert(Position(spot));
        transform.rotation = beside.rotation;
        health.current = health.max;
        loadout.replace(Loadout::new(STARTING_WEAPON));
        *knife = Knife::default();
        *equipment = Equipment { frag_limits: FragLimits::resolve(&modifiers.0), ..default() };
        perks.0.clear();
        points.balance = respawn_points(points.balance, round.number);
        *locomotion = Locomotion::default();
        commands.entity(entity).remove::<(AwaitingRespawn, ColliderDisabled)>();
        respawned.write(Respawned { player: entity });
    }
}

/// Where a Player comes back beside the teammate at `beside`: the first of right, left, behind and
/// in front of them, `RESPAWN_BESIDE_DISTANCE` away, where the Player's capsule fits clear of the
/// World and there's floor under it. If none does, the teammate's own spot.
fn respawn_spot(beside: &Transform, spatial: &SpatialQuery) -> Vec3 {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    // A little slimmer than the Player, so standing on the floor or against a wall isn't a hit.
    let capsule = Collider::capsule(PLAYER_RADIUS - 0.05, PLAYER_CAPSULE_LENGTH);
    let filter = world_filter();
    [beside.right(), beside.left(), beside.back(), beside.forward()]
        .into_iter()
        .map(|side| beside.translation + *side * RESPAWN_BESIDE_DISTANCE)
        .find(|&spot| {
            // Nothing solid between the teammate and the spot, so it isn't through a wall.
            let between = spot - beside.translation;
            Dir3::new(between).is_ok_and(|dir| spatial.cast_ray(beside.translation, dir, between.length(), true, &filter).is_none())
                && spatial.shape_intersections(&capsule, spot, Quat::IDENTITY, &filter).is_empty()
                && spatial.cast_ray(spot, Dir3::NEG_Y, half_height + RESPAWN_FLOOR_REACH, true, &filter).is_some()
        })
        .unwrap_or(beside.translation)
}

/// Co-op Game Over comes the moment no Player is up, whatever Last Stand time is left, unless one
/// is still to Self-Revive. Solo's Game Over is the Down itself (see `go_down`).
pub(crate) fn game_over_when_none_up(
    players: Query<(Option<&LastStand>, Has<AwaitingRespawn>), With<Player>>,
    run: Res<RunType>,
    mut next: ResMut<NextState<GameState>>,
) {
    // A Player on their way to a Self-Revive (a spare's solo Last Stand) is still in the Run.
    let in_play = |(stand, awaiting): (Option<&LastStand>, bool)| {
        is_up(stand.is_some(), awaiting) || (!awaiting && stand.is_some_and(|stand| !stand.is_coop()))
    };
    if run.is_coop() && !players.is_empty() && !players.iter().any(in_play) {
        next.set(GameState::GameOver);
    }
}

/// A `LocalPlayer` waiting to Respawn watches the first teammate who is up: their body follows
/// the teammate from behind and above, and the camera on it looks wherever they turn.
fn watch_teammate(
    mut watcher: Single<&mut Transform, (With<LocalPlayer>, With<AwaitingRespawn>)>,
    up: Query<(&Transform, Has<LastStand>, Has<AwaitingRespawn>), (With<Player>, Without<LocalPlayer>)>,
) {
    let Some(followed) = up.iter().find(|&(_, last_stand, awaiting)| is_up(last_stand, awaiting)).map(|(at, ..)| *at) else {
        return;
    };
    let behind = watcher.back() * SPECTATE_BEHIND_DISTANCE;
    watcher.translation = followed.translation + behind + Vec3::Y * SPECTATE_LIFT;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bleed_out_takes_ten_percent_rounded_down() {
        assert_eq!(bleed_out_loss(0), 0);
        assert_eq!(bleed_out_loss(1_000), 100);
        assert_eq!(bleed_out_loss(2_555), 255);
        assert_eq!(bleed_out_loss(9), 0);
    }

    #[test]
    fn the_points_floor_starts_at_round_seven() {
        assert_eq!(respawn_points(300, 6), 300);
        assert_eq!(respawn_points(300, 7), 1_500);
        assert_eq!(respawn_points(4_000, 12), 4_000);
    }
}
