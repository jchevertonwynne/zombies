//! Two to four headless games on the real Map, joined by a `Hub` with loss and jitter: the Host
//! steps each remote Player by what their game sends, each client predicts its own Player, and the
//! others show on every client as puppets.

use std::time::Duration;

use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use super::link::Hub;
use super::play::{Prediction, install_client, install_host};
use super::protocol::Peer;
use crate::authority::{Authority, AuthorityPlugin};
use crate::player::{LocalPlayer, MoveInput, Player, Seat, move_player, player_body};
use crate::state::{GameState, Pause, world_runs};
use crate::teammate::{Action, Pose, Shots};
use crate::tuning::*;

/// One machine of the Run: the Map and all `seats` Players, the one in `local` the `LocalPlayer`.
fn machine(local: u8, seats: u8, install: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, PhysicsPlugins::default(), StatesPlugin))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .insert_state(GameState::Playing)
        .insert_state(Authority::Host)
        .add_plugins(AuthorityPlugin)
        .add_sub_state::<Pause>()
        .add_systems(FixedUpdate, move_player.run_if(world_runs))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ))));
    install(&mut app);
    app.finish();
    app.cleanup();
    app.world_mut().run_system_once(crate::map::spawn_map).unwrap();
    for seat in 0..seats {
        let mut player = app.world_mut().spawn(player_body(Seat(seat), "Player", &[]));
        if seat == local {
            player.insert(LocalPlayer);
        }
    }
    app
}

/// A Host and `clients` clients over a network of `delay` ticks each way, losing `loss` of the
/// datagrams and delaying each up to `jitter` more.
struct Run {
    hub: Hub,
    apps: Vec<App>,
    tick: u32,
}

impl Run {
    fn new(clients: u8, delay: u32, loss: f32, jitter: u32) -> Self {
        let mut hub = Hub::new(delay, loss);
        hub.jitter = jitter;
        let seats = clients + 1;
        let host_link = hub.host();
        let links: Vec<_> = (0..clients).map(|_| hub.join()).collect();
        let remotes: Vec<(Peer, u8)> = links.iter().map(|link| (link.peer(), link.peer() as u8)).collect();
        let mut apps = vec![machine(0, seats, |app| install_host(app, host_link, remotes))];
        for link in links {
            let seat = link.peer() as u8;
            apps.push(machine(seat, seats, |app| install_client(app, link)));
        }
        let mut run = Run { hub, apps, tick: 0 };
        run.step(&|_, _| MoveInput::default(), 20);
        run
    }

    /// Runs `ticks` ticks, each client's input being `script(seat, tick)`.
    fn step(&mut self, script: &dyn Fn(u8, u32) -> MoveInput, ticks: u32) {
        for _ in 0..ticks {
            self.tick += 1;
            for seat in 1..self.apps.len() as u8 {
                let input = script(seat, self.tick);
                let world = self.apps[usize::from(seat)].world_mut();
                let mut query = world.query_filtered::<&mut MoveInput, With<LocalPlayer>>();
                *query.single_mut(world).unwrap() = input;
            }
            self.hub.step();
            for app in &mut self.apps {
                app.update();
            }
        }
    }

    /// Where `seat`'s Player is on machine `on`.
    fn at(&mut self, on: usize, seat: u8) -> Vec3 {
        let world = self.apps[on].world_mut();
        let mut query = world.query_filtered::<(&Seat, &Transform), With<Player>>();
        query.iter(world).find(|(held, _)| held.0 == seat).unwrap().1.translation
    }

    fn corrected(&mut self, on: usize) -> f32 {
        self.apps[on].world().resource::<Prediction>().corrected
    }

    fn pose_of(&mut self, on: usize, seat: u8) -> (Pose, u8) {
        let world = self.apps[on].world_mut();
        let mut query = world.query_filtered::<(&Seat, &Pose, &Shots), With<Player>>();
        let (_, pose, shots) = query.iter(world).find(|(held, ..)| held.0 == seat).unwrap();
        (*pose, shots.count())
    }
}

/// A Player's walk: forward and round to the right, a strafe, a jump, then standing still.
fn walk(seat: u8, tick: u32) -> MoveInput {
    let yaw = 0.3 * f32::from(seat) - tick as f32 * 0.004;
    match tick {
        20..120 => MoveInput { axes: Vec2::Y, yaw, ..default() },
        120..180 => MoveInput { axes: Vec2::new(1.0, 1.0), sprint: tick % 40 < 20, yaw, jump: tick == 130, ..default() },
        180..240 => MoveInput { axes: Vec2::NEG_X, yaw, ..default() },
        _ => MoveInput { yaw, ..default() },
    }
}

fn lossy(clients: u8) {
    let mut run = Run::new(clients, 3, 0.05, 2);
    let starts: Vec<Vec3> = (0..=clients).map(|seat| run.at(0, seat)).collect();
    let mut worst = 0.0f32;
    // How far each puppet shown on the first client moves in a tick.
    let mut strides = Vec::new();
    let mut last = run.at(1, 0);
    // Each client's body, as it holds a Weapon at the ready and has fired seven shots.
    let held = Pose { aiming: true, action: Action::Reload, ..default() };
    for _ in 0..400 {
        run.step(&walk, 1);
        for seat in 1..=clients {
            let world = run.apps[usize::from(seat)].world_mut();
            let mut query = world.query_filtered::<(&mut Pose, &mut Shots), With<LocalPlayer>>();
            let (mut pose, mut shots) = query.single_mut(world).unwrap();
            (*pose, *shots) = (held, {
                let mut shots = Shots::default();
                shots.set(7);
                shots
            });
        }
        if run.tick > 60 {
            for on in 1..=usize::from(clients) {
                worst = worst.max(run.corrected(on));
            }
            let now = run.at(1, 0);
            strides.push(now.distance(last));
            last = now;
        }
    }
    run.step(&|seat, tick| walk(seat, tick.max(240)), 100);

    for seat in 1..=clients {
        let on_host = run.at(0, seat);
        let there = on_host - starts[usize::from(seat)];
        assert!(there.xz().length() > 5.0, "seat {seat} only went {there} on the Host");
        let own = run.at(usize::from(seat), seat);
        assert!(own.distance(on_host) < 0.05, "seat {seat} is {own} for itself and {on_host} on the Host");
        // Everyone else sees them where the Host does, and what their body is doing.
        for on in (0..=usize::from(clients)).filter(|&on| on != usize::from(seat)) {
            let seen = run.at(on, seat);
            assert!(seen.distance(on_host) < 0.05, "machine {on} shows seat {seat} at {seen}, the Host at {on_host}");
        }
        for on in 0..=usize::from(clients) {
            if on != usize::from(seat) {
                assert_eq!(run.pose_of(on, seat), (held, 7), "machine {on} was not told how seat {seat} stands");
            }
        }
    }
    // The Host's own Player stands where it was, and shows so.
    for on in 1..=usize::from(clients) {
        assert!(run.at(on, 0).distance(starts[0]) < 0.05);
    }
    assert!(worst < 0.3, "the Host put a Player right by {worst} m");
    let jump = strides.iter().copied().fold(0.0, f32::max);
    assert!(jump < 0.2, "a puppet moved {jump} m in a tick");
}

#[test]
fn two_games_agree_over_a_lossy_network() {
    lossy(1);
}

#[test]
fn four_games_agree_over_a_lossy_network() {
    lossy(3);
}

/// A Player's own Player goes where it is put, and moves on from there by the input not yet heard.
#[test]
fn prediction_runs_ahead_of_the_host_by_the_way_there() {
    let mut run = Run::new(1, 5, 0.0, 0);
    run.step(&|_, _| MoveInput { axes: Vec2::Y, ..default() }, 14);
    let (own, host) = (run.at(1, 1), run.at(0, 1));
    // The Host has heard of 5 ticks fewer, and the Player has gone on by them.
    assert!(own.z < host.z - 0.1, "no ahead: {own} against {host}");
    assert!(run.corrected(1) < 0.05, "put right by {}", run.corrected(1));
}

/// A remote Player whose input stops coming stands still on the Host.
#[test]
fn a_player_gone_quiet_stands_still() {
    let mut run = Run::new(1, 2, 0.0, 0);
    run.step(&|_, _| MoveInput { axes: Vec2::Y, ..default() }, 60);
    run.hub.cut(1);
    run.step(&|_, _| MoveInput { axes: Vec2::Y, ..default() }, 60);
    let first = run.at(0, 1);
    run.step(&|_, _| MoveInput { axes: Vec2::Y, ..default() }, 60);
    assert!(run.at(0, 1).distance(first) < 0.02, "still walking");
}
