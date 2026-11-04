//! Leaving a Co-op Run: the local side. The Pause menu's "Leave Run" confirm writes [`LeaveRun`]
//! for the network layer to carry to the Host, and the Host removes a leaver's Player when a
//! [`PlayerLeft`] arrives. Nothing here talks to a Room; that comes with the netcode.

use bevy::prelude::*;

use crate::authority::{Authority, HostSimulation};
use crate::player::{Player, PlayerName, Seat};
use crate::state::{GameState, RunType};

/// What the confirm asks, for a joiner.
pub const LEAVE_JOINER_TEXT: &str = "Leave the Run? You can't rejoin.";
/// What the confirm asks, for the Host.
pub const LEAVE_HOST_TEXT: &str = "End the Run for everyone?";

/// This machine's Player left the Co-op Run: by the Pause menu's confirm, or by closing the window.
/// The network layer sends it to the Host. A joiner's gives up their Seat for good; the Host's ends
/// the Run for everyone (`ends_run`).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaveRun {
    pub ends_run: bool,
}

/// A joiner has left the Run for good, as the Host hears it: their Player is removed, and
/// teammates are told "*name* left". The network layer writes it.
#[allow(dead_code, reason = "nothing hears a joiner leave until the netcode lands")]
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct PlayerLeft {
    pub seat: Seat,
}

/// Closing the window mid-Run counts as Leave Run. The Record is saved by `records`.
fn leave_on_close(authority: Res<State<Authority>>, mut leaves: MessageWriter<LeaveRun>) {
    leaves.write(LeaveRun {
        ends_run: *authority.get() == Authority::Host,
    });
}

/// Removes the Player of each Seat that left.
fn remove_leavers(
    mut commands: Commands,
    mut left: MessageReader<PlayerLeft>,
    players: Query<(Entity, &Seat, Option<&PlayerName>), With<Player>>,
) {
    for &PlayerLeft { seat } in left.read() {
        for (player, _, name) in players.iter().filter(|(_, held, _)| **held == seat) {
            let name = name.map_or("Player", |name| name.0.as_str());
            info!("{name} left");
            commands.entity(player).despawn();
        }
    }
}

pub struct LeaveRunPlugin;

impl Plugin for LeaveRunPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LeaveRun>()
            .add_message::<PlayerLeft>()
            .add_systems(
                Update,
                remove_leavers
                    .in_set(HostSimulation)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Last,
                leave_on_close
                    .run_if(on_message::<AppExit>)
                    .run_if(in_state(GameState::Playing))
                    .run_if(resource_equals(RunType::Coop)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaver_loses_their_player_and_the_others_stay() {
        let mut app = App::new();
        app.add_message::<PlayerLeft>().add_systems(Update, remove_leavers);
        let host = app.world_mut().spawn((Player, Seat(0))).id();
        let joiner = app.world_mut().spawn((Player, Seat(1), PlayerName("Ann".into()))).id();
        app.world_mut().write_message(PlayerLeft { seat: Seat(1) });
        app.update();
        assert!(app.world().get_entity(joiner).is_err());
        assert!(app.world().get_entity(host).is_ok());
    }
}
