//! The Round Choice's vote in Co-op. The Host draws the options and owns the result; a client
//! mirrors `RoundVariety` from `RoundChoice`, `RoundVotes` and `RoundPicked`, so its Fog, banner,
//! panels and the Twists' stat entries (Low Gravity feeds its movement prediction) run unchanged.
//! Its Twist's banner, its survival (for its own Scrip and Run Record) and the Round's cue are
//! inferred from the pick, the Round's start and its end. A client's vote goes up as
//! `Request::VoteRound`.

use bevy::prelude::*;

use super::{Heard, Mirror, SeatReturned, Tell, WorldEvent};
use crate::audio::cues::Cue;
use crate::audio::{PlayCue, RoundStartCue};
use crate::net::play::Wire;
use crate::net::protocol::{HOST, ToHost, encode};
use crate::run::gauntlet::{PickRound, RoundVariety};
use crate::run::round::{Round, RoundPhase};

/// Tells everyone the Round Choice when it is drawn or lit, the votes when they change, and the
/// pick once.
pub fn tell_choice(mut tell: Tell, mut mirror: ResMut<Mirror>, variety: Option<Res<RoundVariety>>) {
    let Some(variety) = variety else { return };
    let now = variety.pending.as_ref().map(|pending| (pending.round, pending.options.map(|option| option.wire()), variety.lit));
    if mirror.choice != now {
        mirror.choice = now;
        if let Some((round, options, lit)) = now {
            tell.all(&WorldEvent::RoundChoice { round, options, lit });
        }
    }
    match variety.chosen.as_ref() {
        Some(chosen) if mirror.picked != Some(chosen.round) => {
            mirror.picked = Some(chosen.round);
            tell.all(&WorldEvent::RoundPicked { round: chosen.round, option: chosen.index });
        }
        Some(_) => {}
        None => mirror.picked = None,
    }
    if mirror.votes != variety.votes {
        mirror.votes = variety.votes;
        tell.all(&WorldEvent::RoundVotes(variety.votes));
    }
}

/// Tells a Player who has come back the Round Choice, the votes and the Round's pick.
pub fn catch_up_choice(mut tell: Tell, mut returned: MessageReader<SeatReturned>, variety: Option<Res<RoundVariety>>) {
    let Some(variety) = variety else { return };
    for &SeatReturned { seat } in returned.read() {
        let Some(&(peer, _)) = tell.roster.0.iter().find(|&&(_, held)| held == seat.0) else { continue };
        if let Some(pending) = &variety.pending {
            let options = pending.options.map(|option| option.wire());
            tell.to(peer, &WorldEvent::RoundChoice { round: pending.round, options, lit: variety.lit });
            tell.to(peer, &WorldEvent::RoundVotes(variety.votes));
        } else if let Some(chosen) = &variety.chosen {
            // Only the picked option is left, and it stands for both.
            let options = [chosen.option.wire(); 2];
            tell.to(peer, &WorldEvent::RoundChoice { round: chosen.round, options, lit: false });
            tell.to(peer, &WorldEvent::RoundPicked { round: chosen.round, option: chosen.index });
        }
    }
}

/// A client mirrors the Host's Round Choice, votes and pick.
pub fn hear_choice(
    mut heard: MessageReader<Heard>,
    variety: Option<ResMut<RoundVariety>>,
    round: Option<Res<Round>>,
    real: Res<Time<Real>>,
    standing_in: Option<ResMut<RoundStartCue>>,
    mut cues: MessageWriter<PlayCue>,
) {
    let (Some(mut variety), Some(mut standing_in)) = (variety, standing_in) else { return };
    for Heard(event) in heard.read() {
        match *event {
            WorldEvent::RoundChoice { round, options, lit } => variety.mirror_choice(round, options, lit, real.elapsed_secs()),
            WorldEvent::RoundVotes(votes) => variety.votes = votes,
            WorldEvent::RoundPicked { round: picked, option } => {
                let was_pending = variety.pending.is_some();
                let (now, active) = round.as_ref().map_or((0, false), |round| (round.number, round.phase == RoundPhase::Active));
                variety.mirror_pick(picked, option, now, active);
                if was_pending && let Some(chosen) = variety.chosen.as_ref().filter(|chosen| chosen.round == picked) {
                    standing_in.0 = chosen.option.twist.map(|_| Cue::TwistRoundStart);
                    cues.write(PlayCue::new(Cue::OfferPick, None));
                }
            }
            _ => {}
        }
    }
}

/// Sends the Host the vote the `LocalPlayer` made.
pub fn forward_votes(mut picks: MessageReader<PickRound>, mut wire: ResMut<Wire>) {
    for &PickRound { option } in picks.read() {
        wire.0.send(HOST, encode(&ToHost::Request(super::Request::VoteRound { option: option as u8 })));
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Run;
    use super::*;
    use crate::run::gauntlet::variety::{PendingChoice, RoundOption, Twist, apply_pick, client_rounds, count_votes, light_choice, settle_votes};
    use crate::authority::{Authority, HostSimulation};
    use crate::modifier::Stat;
    use crate::run::gauntlet::arc::RunArc;
    use crate::run::gauntlet::{OfferWait, VoteRound};
    use crate::run::round::RoundEnded;
    use crate::state::RunType;

    fn gauntlet(app: &mut App) {
        app.insert_resource(RunType::Coop)
            .insert_resource(Round::in_progress(4))
            .init_resource::<RoundVariety>()
            .init_resource::<RoundStartCue>()
            .init_resource::<RunArc>()
            .init_resource::<OfferWait>()
            .add_message::<PickRound>()
            .add_message::<VoteRound>()
            .add_message::<PlayCue>()
            .add_systems(
                Update,
                (light_choice, apply_pick, count_votes, settle_votes).chain().in_set(HostSimulation),
            )
            .add_systems(Update, client_rounds.run_if(in_state(Authority::Client)));
    }

    fn draw(run: &mut Run) {
        let twist = RoundOption { twist: Some(Twist::LowGravity), challenge: None };
        run.world(0).resource_mut::<RoundVariety>().pending =
            Some(PendingChoice { round: 5, options: [RoundOption { twist: None, challenge: None }, twist] });
        run.step(10);
    }

    #[test]
    fn a_client_sees_the_round_choice_and_votes_on_it_and_gets_the_result() {
        let mut run = Run::with(1, &gauntlet);
        draw(&mut run);
        for on in 0..2 {
            let variety = run.world(on).resource::<RoundVariety>();
            assert_eq!(variety.pending.as_ref().map(|pending| pending.round), Some(5), "game {on} shows the choice");
            assert!(variety.lit, "game {on}");
        }
        // The client votes for the Twist, and so does the Host.
        run.world(1).write_message(PickRound { option: 1 });
        run.step(10);
        for on in 0..2 {
            assert_eq!(run.world(on).resource::<RoundVariety>().votes, [None, Some(1), None, None], "game {on} shows the vote");
        }
        run.world(0).write_message(PickRound { option: 1 });
        run.step(10);
        for on in 0..2 {
            let variety = run.world(on).resource::<RoundVariety>();
            assert!(variety.pending.is_none(), "game {on}");
            let chosen = variety.chosen.as_ref().unwrap_or_else(|| panic!("game {on} has the pick"));
            assert_eq!((chosen.round, chosen.option.twist), (5, Some(Twist::LowGravity)), "game {on}");
            assert!(
                variety.entries(5, &RoundPhase::Active).iter().any(|entry| entry.stat == Stat::PlayerGravity),
                "game {on} has Low Gravity's entry"
            );
        }
    }

    #[test]
    fn a_tie_between_two_players_goes_to_the_hosts_vote_on_every_machine() {
        let mut run = Run::with(1, &gauntlet);
        draw(&mut run);
        run.world(1).write_message(PickRound { option: 1 });
        run.world(0).write_message(PickRound { option: 0 });
        run.step(10);
        for on in 0..2 {
            let chosen = run.world(on).resource::<RoundVariety>().chosen.clone().expect("picked");
            assert_eq!(chosen.option.twist, None, "game {on}: the Host voted for the safe option");
        }
    }

    #[test]
    fn a_client_infers_the_twists_round_start_and_survival() {
        let mut run = Run::with(1, &gauntlet);
        draw(&mut run);
        run.world(1).write_message(PickRound { option: 1 });
        run.world(0).write_message(PickRound { option: 1 });
        run.step(10);
        run.world(0).insert_resource(Round::in_progress(5));
        run.step(10);
        assert_eq!(run.world(1).resource::<Round>().number, 5);
        assert_eq!(run.world(1).resource::<RoundVariety>().chosen.as_ref().map(|chosen| chosen.round), Some(5));
        run.world(0).write_message(RoundEnded);
        run.world(0).resource_mut::<Round>().phase = RoundPhase::Intermission(30.0);
        run.step(10);
        assert_eq!(run.world(1).resource::<RoundVariety>().twists_survived, 1, "the client counts it for its own Run Record");
    }
}
