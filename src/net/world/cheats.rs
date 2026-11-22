//! A Player's Cheat toggles over the wire (ADR 0010). The Host holds every Player's `Cheats` and
//! judges every request; it tells each client the toggles of its own Player, so that client can
//! resolve what is its own (its ammo, and the prices on its prompts) and show its panel. A Player
//! who comes back is told theirs again.

use std::collections::HashMap;

use bevy::prelude::*;

use super::{Heard, Prediction, SeatReturned, Tell, WorldEvent, release_events};
use crate::net::remote::Remote;
use crate::players::player::{LocalPlayer, Player, Seat as PlayerSeat};
use crate::run::dev::Cheats;
use crate::state::{GameState, world_runs};

pub fn add(app: &mut App) {
    app.add_systems(
        Update,
        (tell_cheats, catch_up_cheats.after(tell_cheats))
            .after(crate::authority::HostSimulation)
            .run_if(resource_exists::<super::Roster>)
            .run_if(resource_exists::<super::NetIds>)
            .run_if(in_state(GameState::Playing))
            .run_if(world_runs),
    )
    .add_systems(
        Update,
        hear_cheats
            .after(release_events)
            .run_if(resource_exists::<Prediction>)
            .run_if(resource_exists::<super::NetIds>)
            .run_if(in_state(GameState::Playing))
            .run_if(world_runs),
    );
}

/// Tells each remote Player their toggles when they change.
fn tell_cheats(
    mut tell: Tell,
    mut told: Local<HashMap<u8, u8>>,
    players: Query<(&PlayerSeat, Option<&Cheats>, &Remote), With<Player>>,
) {
    for (seat, cheats, remote) in &players {
        let now = Cheats::toggles(cheats);
        if told.get(&seat.0).copied().unwrap_or(0) != now {
            told.insert(seat.0, now);
            tell.to(remote.peer, &WorldEvent::Cheats(now));
        }
    }
}

/// Tells a Player who has come back the toggles the Host kept for their Seat.
fn catch_up_cheats(
    mut tell: Tell,
    mut returned: MessageReader<SeatReturned>,
    players: Query<(&PlayerSeat, Option<&Cheats>), With<Player>>,
) {
    for &SeatReturned { seat } in returned.read() {
        let Some(&(peer, _)) = tell.roster.0.iter().find(|&&(_, held)| held == seat.0) else { continue };
        let toggles = players.iter().find(|(held, _)| held.0 == seat.0).map_or(0, |(_, cheats)| Cheats::toggles(cheats));
        if toggles != 0 {
            tell.to(peer, &WorldEvent::Cheats(toggles));
        }
    }
}

/// The toggles the Host says the `LocalPlayer` has: held on their Player as `Cheats`, so the stats
/// this machine resolves read them.
fn hear_cheats(mut heard: MessageReader<Heard>, mut commands: Commands, local: Query<(Entity, Option<&Cheats>), With<LocalPlayer>>) {
    let Ok((player, held)) = local.single() else { return };
    for Heard(event) in heard.read() {
        let WorldEvent::Cheats(toggles) = *event else { continue };
        if Cheats::toggles(held) == toggles {
            continue;
        }
        match Cheats::from_toggles(toggles) {
            Some(cheats) => commands.entity(player).insert(cheats),
            None => commands.entity(player).remove::<Cheats>(),
        };
    }
}
