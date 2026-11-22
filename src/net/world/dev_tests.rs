//! Developer mode over the wire: a client's Cheat request is judged by the Host, who taints the
//! Run for everyone, and the Host's setting (and Seat-kept Cheats) reach and survive Rejoins.

use super::downs::coop;
use super::tests::Run;
use super::*;
use crate::economy::points::PointsPlugin;
use crate::net::away::{Reach, Rejoin};
use crate::run::dev::{CHEAT_POINTS, Cheat, Cheats, DevPlugin, SetDevMode};
use crate::storage::records::{RunTainted, TaintRun};

fn with_dev(app: &mut App) {
    coop(app);
    app.add_plugins((PointsPlugin, DevPlugin))
        .init_resource::<RunTainted>()
        .add_message::<TaintRun>()
        .add_systems(Update, |mut asked: MessageReader<TaintRun>, mut tainted: ResMut<RunTainted>| {
            if asked.read().count() > 0 {
                tainted.0 = true;
            }
        });
}

/// A Co-op Run of the Host and `clients`, with Developer mode set to `dev` on every machine.
fn run(clients: u8, dev: DevMode) -> Run {
    let mut run = Run::with(clients, &with_dev);
    run.step(5);
    for on in 0..=usize::from(clients) {
        run.world(on).resource_mut::<DevAccess>().0 = dev;
    }
    run.step(5);
    run
}

fn ask(run: &mut Run, on: usize) {
    let player = run.player(on, on as u8);
    run.world(on).write_message(CheatRequest { player, cheat: Cheat::Points });
    run.step(20);
}

fn tainted(run: &mut Run, on: usize) -> bool {
    run.world(on).resource::<RunTainted>().0
}

#[test]
fn a_clients_cheat_goes_through_when_developer_mode_is_on_and_taints_everyone() {
    let mut run = run(2, DevMode::On);
    run.give_points(1, 0);
    run.step(5);
    for on in 0..3 {
        assert!(!tainted(&mut run, on), "opening the panel alone taints nothing");
    }
    ask(&mut run, 1);
    assert_eq!(run.balance(1), CHEAT_POINTS);
    for on in 0..3 {
        assert!(tainted(&mut run, on), "game {on}");
    }
}

#[test]
fn a_request_the_setting_forbids_is_dropped_silently() {
    for (dev, on) in [(DevMode::Off, 1), (DevMode::HostOnly, 1), (DevMode::Off, 0)] {
        let mut run = run(1, dev);
        run.give_points(on as u8, 0);
        run.step(5);
        ask(&mut run, on);
        assert_eq!(run.balance(on as u8), 0, "{dev:?} game {on}");
        for game in 0..2 {
            assert!(!tainted(&mut run, game), "{dev:?} game {game}");
        }
    }
}

#[test]
fn host_only_still_lets_the_host_cheat() {
    let mut run = run(1, DevMode::HostOnly);
    run.give_points(0, 0);
    run.step(5);
    ask(&mut run, 0);
    assert_eq!(run.balance(0), CHEAT_POINTS);
    assert!(tainted(&mut run, 1));
}

#[test]
fn the_hosts_change_of_setting_reaches_every_client() {
    let mut run = run(2, DevMode::On);
    run.world(0).write_message(SetDevMode(DevMode::HostOnly));
    run.step(20);
    for on in 0..3 {
        assert_eq!(run.world(on).resource::<DevAccess>().0, DevMode::HostOnly, "game {on}");
    }
}

#[test]
fn off_takes_every_players_cheats_away_and_a_rejoiner_hears_the_setting_and_keeps_theirs() {
    let mut run = run(2, DevMode::On);
    ask(&mut run, 1);
    let guest = run.player(0, 1);
    assert_eq!(run.world(0).get::<Cheats>(guest).unwrap().points_given, CHEAT_POINTS);

    // A Player whose way breaks keeps their Cheats, by Seat, and hears the setting on return.
    run.cut(1);
    run.step(2);
    run.world(0).write_message(SetDevMode(DevMode::HostOnly));
    run.step(40);
    assert_eq!(run.world(1).resource::<Rejoin>().reach, Reach::Up);
    run.world(0).write_message(RoundStarted);
    run.step(30);
    assert_eq!(run.world(1).resource::<DevAccess>().0, DevMode::HostOnly, "the Rejoiner hears the setting");
    assert!(tainted(&mut run, 1), "and that the Run is Tainted");
    let guest = run.player(0, 1);
    assert_eq!(run.world(0).get::<Cheats>(guest).unwrap().points_given, CHEAT_POINTS, "kept by Seat");

    run.world(0).write_message(SetDevMode(DevMode::Off));
    run.step(20);
    assert!(run.world(0).get::<Cheats>(guest).is_none());
    assert!(tainted(&mut run, 0), "the Run stays Tainted");
    assert_eq!(run.world(2).resource::<DevAccess>().0, DevMode::Off);
}
