//! The Win over the wire. A client's choice on its Win screen goes to the Host as
//! `Request::WinChoice`, so the Host's wait for every choice (`run::gauntlet::win`) hears it. A
//! client who ends the Run leaves it for good through the leave path, as `ToHost::LeftRun` would:
//! out of the roster, their Player removed and their Seat freed (`PlayerLeft`), never held for
//! Rejoin. The Host's own End the Run reaches every client with its Game Over
//! (`ToPlayer::RunOver { won }`).

use bevy::prelude::*;

use super::{Prediction, Request, Roster, Wire, hear_requests};
use crate::net::protocol::{HOST, ToHost, encode};
use crate::run::gauntlet::win::WinChoice;
use crate::run::leave_run::PlayerLeft;
use crate::state::GameState;

pub fn add(app: &mut App) {
    app.add_systems(
        Update,
        part_winners
            .after(hear_requests)
            .run_if(resource_exists::<Roster>)
            .run_if(in_state(GameState::Playing)),
    )
    // Not only while Playing: End the Run is heard in the frame the Run ends.
    .add_systems(Update, forward_win_choice.run_if(resource_exists::<Prediction>).run_if(resource_exists::<Wire>));
}

/// A client who chose End the Run has left the Run for good, with their Win.
fn part_winners(mut chosen: MessageReader<WinChoice>, mut roster: ResMut<Roster>, mut left: MessageWriter<PlayerLeft>) {
    for choice in chosen.read().filter(|choice| choice.end) {
        let before = roster.0.len();
        roster.0.retain(|&(_, seat)| seat != choice.seat.0);
        if roster.0.len() != before {
            left.write(PlayerLeft { seat: choice.seat });
        }
    }
}

/// Sends the Host this machine's Win choice. On a client every `WinChoice` is its own Player's.
fn forward_win_choice(mut chosen: MessageReader<WinChoice>, mut wire: ResMut<Wire>) {
    for choice in chosen.read() {
        wire.0.send(HOST, encode(&ToHost::Request(Request::WinChoice { end: choice.end })));
    }
}
