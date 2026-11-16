//! Each machine earns its own Commendations: the Host's word of the Round and the Elites reaches a
//! client, which logs and pays its own Player.

use super::tests::Run;
use super::*;
use crate::run::gauntlet::arc::{RunArc, clear_boss_rounds, infer_win, win_round};
use crate::run::gauntlet::commendations::{CommendationEarned, RunCommendations, RunLog, earn_commendations, log_run};
use crate::run::gauntlet::progress::GauntletFile;
use crate::run::gauntlet::{ActiveRanks, HeldBoons, UnlockAll};
use crate::state::RunType;
use crate::storage::records::RunWon;

fn gauntlet(app: &mut App) {
    app.insert_resource(RunType::Coop)
        .insert_resource(Round::in_progress(3))
        .init_resource::<HeldPacts>()
        .init_resource::<RunLog>()
        .init_resource::<RunCommendations>()
        .init_resource::<GauntletFile>()
        .init_resource::<RunArc>()
        .init_resource::<RunWon>()
        .init_resource::<ActiveRanks>()
        .init_resource::<UnlockAll>()
        .init_resource::<RunElitesKilled>()
        .add_message::<CommendationEarned>()
        .add_message::<crate::audio::PlayCue>()
        .add_systems(Update, give_boons)
        .add_systems(
            Update,
            (
                clear_boss_rounds.in_set(HostSimulation),
                infer_win.run_if(in_state(Authority::Client)),
                (log_run, earn_commendations).chain(),
            )
                .chain(),
        );
}

/// Every Player holds Boons, as Gauntlet gives them.
fn give_boons(mut commands: Commands, players: Query<Entity, (With<Player>, Without<HeldBoons>)>) {
    for player in &players {
        commands.entity(player).insert(HeldBoons::default());
    }
}

fn earned(run: &mut Run, on: usize) -> Vec<String> {
    run.world(on).resource::<GauntletFile>().commendations.clone()
}

#[test]
fn a_client_earns_the_win_commendations_on_its_own_machine() {
    let mut run = Run::with(1, &gauntlet);
    run.world(0).insert_resource(Round::in_progress(win_round()));
    run.step(10);
    run.world(0).write_message(RoundEnded);
    run.world(0).resource_mut::<Round>().phase = RoundPhase::Intermission(INTERMISSION_SECS);
    run.step(20);
    for on in 0..2 {
        assert!(run.world(on).resource::<RunArc>().won, "game {on} has not won");
        let got = earned(&mut run, on);
        assert!(got.iter().any(|id| id == "win"), "game {on}: {got:?}");
        assert!(got.iter().any(|id| id == "win_no_pact"), "game {on}: {got:?}");
    }
}

#[test]
fn a_client_earns_elites_100_from_the_hosts_count_and_the_host_does_not() {
    let mut run = Run::with(1, &gauntlet);
    run.step(10);
    run.world(0).resource_mut::<RunElitesKilled>().0 = 100;
    run.step(20);
    assert_eq!(earned(&mut run, 1), ["elites_100"]);
    assert!(earned(&mut run, 0).is_empty(), "the Host's own Run counted none");
}
