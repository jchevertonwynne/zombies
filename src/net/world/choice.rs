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
use crate::run::gauntlet::variety::Standing;
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

/// The most often the Challenge's kills are told, in a second.
pub const CHALLENGE_TELL_PER_SEC: f32 = 4.0;

/// The Challenge as clients are told it: its standing, kills and Blitz's seconds left.
fn challenge_event(variety: &RoundVariety, horde: u32) -> Option<WorldEvent> {
    let run = variety.chosen.as_ref()?.challenge.as_ref()?;
    Some(WorldEvent::Challenge {
        standing: run.standing.index(),
        kills: run.kills.min(u32::from(u16::MAX)) as u16,
        horde: horde.min(u32::from(u16::MAX)) as u16,
        blitz_left_ds: run.left_ds(horde),
    })
}

/// Tells everyone how the Challenge stands when it changes: at once when the standing does, and
/// the kills at most `CHALLENGE_TELL_PER_SEC` times a second.
pub fn tell_challenge(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    variety: Option<Res<RoundVariety>>,
    round: Option<Res<Round>>,
    time: Res<Time>,
) {
    let Some(variety) = variety else { return };
    let horde = round.map_or(0, |round| round.horde());
    let Some(event) = challenge_event(&variety, horde) else {
        mirror.challenge = None;
        return;
    };
    let WorldEvent::Challenge { standing, kills, horde, blitz_left_ds } = event else { return };
    let now = (standing, kills, horde, blitz_left_ds.is_some());
    if mirror.challenge == Some(now) {
        return;
    }
    let at = time.elapsed_secs();
    let standing_changed = mirror.challenge.is_none_or(|(told, ..)| told != standing);
    if !standing_changed && mirror.challenge_told.is_some_and(|told| at - told < 1.0 / CHALLENGE_TELL_PER_SEC) {
        return;
    }
    mirror.challenge = Some(now);
    mirror.challenge_told = Some(at);
    tell.all(&event);
}

/// Tells a Player who has come back the Round Choice, the votes and the Round's pick.
pub fn catch_up_choice(
    mut tell: Tell,
    mut returned: MessageReader<SeatReturned>,
    variety: Option<Res<RoundVariety>>,
    round: Option<Res<Round>>,
) {
    let Some(variety) = variety else { return };
    let horde = round.map_or(0, |round| round.horde());
    for &SeatReturned { seat } in returned.read() {
        let Some(&(peer, _)) = tell.roster.0.iter().find(|&&(_, held)| held == seat.0) else { continue };
        if let Some(pending) = &variety.pending {
            let options = pending.options.map(|option| option.wire());
            tell.to(peer, &WorldEvent::RoundChoice { round: pending.round, options, lit: variety.lit });
            tell.to(peer, &WorldEvent::RoundVotes(variety.votes));
        } else if let Some(chosen) = &variety.chosen {
            let options = chosen.options.map(|option| option.wire());
            tell.to(peer, &WorldEvent::RoundChoice { round: chosen.round, options, lit: false });
            tell.to(peer, &WorldEvent::RoundPicked { round: chosen.round, option: chosen.index });
            if let Some(event) = challenge_event(&variety, horde) {
                tell.to(peer, &event);
            }
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
            WorldEvent::Challenge { standing, kills, horde, blitz_left_ds } => {
                let standing = Standing::from_index(standing);
                if let Some(cue) = variety.mirror_challenge(standing, u32::from(kills), u32::from(horde), blitz_left_ds) {
                    cues.write(PlayCue::new(cue, None));
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
    use crate::run::gauntlet::variety::{ChallengeKind, Standing, blitz_limit_secs, PendingChoice, RoundOption, Twist, apply_pick, client_rounds, count_votes, light_choice, settle_votes};
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

    fn pick_with_challenge(run: &mut Run, challenge: ChallengeKind) {
        let twist = RoundOption { twist: Some(Twist::LowGravity), challenge: None };
        let safe = RoundOption { twist: None, challenge: Some(challenge) };
        run.world(0).resource_mut::<RoundVariety>().pending = Some(PendingChoice { round: 5, options: [safe, twist] });
        run.step(10);
        run.world(1).write_message(PickRound { option: 0 });
        run.world(0).write_message(PickRound { option: 0 });
        run.step(10);
    }

    fn challenge_of(run: &mut Run, on: usize) -> (Standing, u32, Option<f32>) {
        let variety = run.world(on).resource::<RoundVariety>();
        let challenge = variety.chosen.as_ref().and_then(|chosen| chosen.challenge.as_ref()).expect("a Challenge");
        (challenge.standing, challenge.kills, challenge.time_left(0))
    }

    #[test]
    fn a_client_sees_the_challenge_kills_and_blitz_clock_and_its_cues() {
        let mut run = Run::with(1, &gauntlet);
        pick_with_challenge(&mut run, ChallengeKind::Blitz);
        {
            let host = run.world(0);
            let mut variety = host.resource_mut::<RoundVariety>();
            let challenge = variety.chosen.as_mut().unwrap().challenge.as_mut().unwrap();
            challenge.kills = 7;
            challenge.tick(1.0, 20, true);
        }
        run.step(40);
        let (standing, kills, left) = challenge_of(&mut run, 1);
        assert_eq!((standing, kills), (Standing::Active, 7), "the client shows the team's kills");
        assert!(left.is_some_and(|left| left < blitz_limit_secs(20)), "and Blitz's clock: {left:?}");
        // A breach and then a Met reach the client with their cues.
        run.world(0).resource_mut::<RoundVariety>().chosen.as_mut().unwrap().challenge.as_mut().unwrap().standing = Standing::Broken;
        run.step(10);
        assert_eq!(challenge_of(&mut run, 1).0, Standing::Broken, "the client shows the breach");
    }

    #[test]
    fn a_client_counts_a_met_challenge_for_its_own_run_record() {
        let mut run = Run::with(1, &gauntlet);
        pick_with_challenge(&mut run, ChallengeKind::ColdSteel);
        run.world(0).resource_mut::<RoundVariety>().chosen.as_mut().unwrap().challenge.as_mut().unwrap().standing = Standing::Met;
        run.step(10);
        assert_eq!(challenge_of(&mut run, 1).0, Standing::Met);
        assert_eq!(run.world(1).resource::<RoundVariety>().challenges_met, 1);
    }

    #[test]
    fn the_challenge_kills_are_told_at_most_four_times_a_second() {
        let mut run = Run::with(1, &gauntlet);
        pick_with_challenge(&mut run, ChallengeKind::ColdSteel);
        let mut told = 0;
        let mut last = run.world(0).resource::<Mirror>().challenge;
        for kill in 1..=crate::tuning::FIXED_HZ {
            run.world(0).resource_mut::<RoundVariety>().chosen.as_mut().unwrap().challenge.as_mut().unwrap().kills = kill;
            run.step(1);
            let now = run.world(0).resource::<Mirror>().challenge;
            if now != last {
                told += 1;
                last = now;
            }
        }
        assert!((2..=5).contains(&told), "{told} tellings in a second");
    }

    #[test]
    fn a_returning_player_is_told_both_options_the_pick_and_the_challenge() {
        let mut run = Run::with(1, &gauntlet);
        pick_with_challenge(&mut run, ChallengeKind::ColdSteel);
        run.world(0).resource_mut::<RoundVariety>().chosen.as_mut().unwrap().challenge.as_mut().unwrap().kills = 4;
        run.step(20);
        // The client forgets everything, as a fresh game does.
        {
            let client = run.world(1);
            let mut variety = client.resource_mut::<RoundVariety>();
            variety.chosen = None;
            variety.pending = None;
        }
        run.world(0).write_message(crate::players::respawn::SeatReturned { seat: crate::players::player::Seat(1) });
        run.step(10);
        let host = run.world(0).resource::<RoundVariety>().chosen.clone().unwrap();
        let client = run.world(1).resource::<RoundVariety>().chosen.clone().expect("told the pick");
        assert_eq!(client.options, host.options, "both options, not the picked one twice");
        assert_ne!(client.options[0], client.options[1]);
        assert_eq!(challenge_of(&mut run, 1).1, 4, "and the Challenge's kills");
    }
}
