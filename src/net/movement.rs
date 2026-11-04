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
use super::zombies::{ZombieId, ZombieIds};
use crate::authority::{Authority, AuthorityPlugin};
use crate::barrier::Barriers;
use crate::hellhound::{HellhoundAssetsParam, HellhoundPlugin, spawn_hellhound};
use crate::nav::NavGraph;
use crate::nova::NovaCrawlerKilled;
use crate::player::PlayerHit;
use crate::power_up::ActivePowerUps;
use crate::special::SpecialCue;
use crate::zombie::rig::{Activity, Shown};
use crate::zombie::{Dying, Gibbed, Rising, SpeedTier, Zombie, ZombieAssetsParam, ZombiePlugin, spawn_zombie};
use crate::player::{LocalPlayer, MoveInput, Player, Seat, move_player, player_body};
use crate::state::{GameState, Pause, world_runs};
use crate::teammate::{Action, Pose, Shots};
use crate::tuning::*;

/// One machine of the Run: the Map and all `seats` Players, the one in `local` the `LocalPlayer`.
pub(super) fn machine(local: u8, seats: u8, install: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, PhysicsPlugins::default(), StatesPlugin))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .insert_state(GameState::Playing)
        .insert_state(Authority::Host)
        .add_plugins(AuthorityPlugin)
        .add_sub_state::<Pause>()
        .add_message::<PlayerHit>()
        .add_message::<SpecialCue>()
        .add_message::<NovaCrawlerKilled>()
        .init_resource::<ActivePowerUps>()
        .insert_resource(NavGraph::new())
        .insert_resource(Barriers::default())
        .add_plugins((ZombiePlugin, HellhoundPlugin))
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
pub(super) struct Run {
    pub(super) hub: Hub,
    pub(super) apps: Vec<App>,
    pub(super) tick: u32,
}

impl Run {
    fn new(clients: u8, delay: u32, loss: f32, jitter: u32) -> Self {
        Self::with(clients, delay, loss, jitter, &|_| {})
    }

    /// The same, with `extra` added to every machine.
    pub(super) fn with(clients: u8, delay: u32, loss: f32, jitter: u32, extra: &dyn Fn(&mut App)) -> Self {
        let mut hub = Hub::new(delay, loss);
        hub.jitter = jitter;
        let seats = clients + 1;
        let host_link = hub.host();
        let links: Vec<_> = (0..clients).map(|_| hub.join()).collect();
        let remotes: Vec<(Peer, u8)> = links.iter().map(|link| (link.peer(), link.peer() as u8)).collect();
        let mut apps = vec![machine(0, seats, |app| {
            extra(app);
            install_host(app, host_link, remotes);
        })];
        for link in links {
            let seat = link.peer() as u8;
            apps.push(machine(seat, seats, |app| {
                extra(app);
                install_client(app, link);
            }));
        }
        let mut run = Run { hub, apps, tick: 0 };
        run.step(&|_, _| MoveInput::default(), 20);
        run
    }

    /// Runs `ticks` ticks, each client's input being `script(seat, tick)`.
    pub(super) fn step(&mut self, script: &dyn Fn(u8, u32) -> MoveInput, ticks: u32) {
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

/// A Zombie or Hellhound as machine `on` has it.
#[derive(Debug, PartialEq)]
struct Puppeted {
    at: Vec3,
    shown: Activity,
    crawler: bool,
    hellhound: bool,
    dying: bool,
}

impl Run {
    /// A Walker standing on the Host's floor at `feet`.
    fn zombie(&mut self, feet: Vec3) -> Entity {
        self.zombie_with(feet, 1000.0)
    }

    /// The same with `health`.
    pub(super) fn zombie_with(&mut self, feet: Vec3, health: f32) -> Entity {
        let world = self.apps[0].world_mut();
        world
            .run_system_once(move |mut commands: Commands, assets: ZombieAssetsParam| {
                spawn_zombie(&mut commands, &assets, feet, health, SpeedTier::Walker, None)
            })
            .unwrap()
    }

    fn hellhound(&mut self, feet: Vec3) -> Entity {
        let world = self.apps[0].world_mut();
        world
            .run_system_once(move |mut commands: Commands, assets: HellhoundAssetsParam| {
                spawn_hellhound(&mut commands, &assets, feet, 1000.0)
            })
            .unwrap()
    }

    /// Every Zombie and Hellhound machine `on` has, by id.
    fn puppets(&mut self, on: usize) -> Vec<(u16, Puppeted)> {
        let world = self.apps[on].world_mut();
        let mut query = world.query::<(&ZombieId, &Transform, &Zombie, Option<&Shown>, Has<Dying>)>();
        let mut found: Vec<_> = query
            .iter(world)
            .map(|(id, transform, zombie, shown, dying)| {
                (
                    id.0,
                    Puppeted {
                        at: transform.translation,
                        shown: shown.map_or(Activity::Moving, |shown| shown.0),
                        crawler: zombie.is_crawler(),
                        hellhound: zombie.is_hellhound(),
                        dying,
                    },
                )
            })
            .collect();
        found.sort_by_key(|(id, _)| *id);
        found
    }
}

pub(super) const IDLE: &dyn Fn(u8, u32) -> MoveInput = &|_, _| MoveInput::default();

/// The Zombies and Hellhounds the Host has are on every client, in the same places, and gone when
/// they are.
fn horde(clients: u8) {
    let mut run = Run::new(clients, 2, 0.05, 1);
    let start = crate::map::PLAYER_START;
    let zombies: Vec<Entity> = (0..5).map(|i| run.zombie(start + Vec3::new(8.0 + 1.5 * i as f32, 0.0, 4.0))).collect();
    let hound = run.hellhound(start + Vec3::new(-8.0, 0.0, 4.0));
    run.step(IDLE, 150);
    // Every machine has the same ids for them.
    let host = run.puppets(0);
    assert_eq!(host.len(), 6);
    for on in 1..=usize::from(clients) {
        let seen = run.puppets(on);
        assert_eq!(seen.len(), 6, "machine {on}");
        assert_eq!(seen.iter().filter(|(_, puppet)| puppet.hellhound).count(), 1);
        for ((mine_id, mine), (their_id, theirs)) in seen.iter().zip(&host) {
            assert_eq!(mine_id, their_id);
            // A hound is running at a Player, so is as far behind as it has run in the time.
            let off = if theirs.hellhound { 1.5 } else { 0.2 };
            assert!(mine.at.distance(theirs.at) < off, "{mine:?} against {theirs:?}");
            assert_eq!(mine.hellhound, theirs.hellhound);
        }
    }
    // The ids go both ways.
    let ids = run.apps[0].world().resource::<ZombieIds>();
    let id = ids.id(zombies[2]).unwrap();
    assert_eq!(ids.entity(id), Some(zombies[2]));
    let on_client = run.apps[1].world().resource::<ZombieIds>();
    assert!(on_client.entity(id).is_some_and(|entity| on_client.id(entity) == Some(id)));

    // The Host's Zombies move and turn, and the puppets follow.
    for tick in 0..120 {
        let world = run.apps[0].world_mut();
        for (i, &zombie) in zombies.iter().enumerate() {
            let mut transform = world.get_mut::<Transform>(zombie).unwrap();
            transform.translation.x -= 0.02;
            transform.rotation = Quat::from_rotation_y(0.01 * (tick + i) as f32);
        }
        run.step(IDLE, 1);
    }
    run.step(IDLE, 40);
    let host = run.puppets(0);
    for on in 1..=usize::from(clients) {
        for ((_, mine), (_, theirs)) in run.puppets(on).iter().zip(&host) {
            // A hound is running at a Player, so is as far behind as it has run in the time.
            let off = if theirs.hellhound { 1.5 } else { 0.2 };
            assert!(mine.at.distance(theirs.at) < off, "{mine:?} against {theirs:?}");
        }
    }

    // One killed falls over, and is gone from every machine in time.
    run.apps[0].world_mut().entity_mut(zombies[0]).insert(Dying::default());
    run.step(IDLE, 40);
    for on in 0..=usize::from(clients) {
        let dead = run.puppets(on).into_iter().filter(|(_, puppet)| puppet.dying).count();
        assert_eq!(dead, 1, "machine {on}");
    }
    run.step(IDLE, 250);
    for on in 0..=usize::from(clients) {
        assert_eq!(run.puppets(on).len(), 5, "machine {on} still has the dead");
    }
    // One that is simply gone is gone.
    run.apps[0].world_mut().entity_mut(hound).despawn();
    run.step(IDLE, 60);
    for on in 0..=usize::from(clients) {
        assert!(run.puppets(on).iter().all(|(_, puppet)| !puppet.hellhound), "machine {on} still has the hound");
    }
}

#[test]
fn zombies_and_hellhounds_appear_move_and_go_on_the_client() {
    horde(1);
}

#[test]
fn zombies_and_hellhounds_appear_move_and_go_on_every_client() {
    horde(3);
}

/// A puppet's rig is told what its Zombie is doing, and a Zombie that loses its legs on the Host
/// crawls here.
#[test]
fn a_puppet_is_posed_as_the_host_has_its_zombie() {
    let mut run = Run::new(1, 2, 0.0, 0);
    let zombie = run.zombie(crate::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0));
    run.step(IDLE, 60);
    assert_eq!(run.puppets(1)[0].1.shown, Activity::Moving);
    run.apps[0].world_mut().entity_mut(zombie).insert(Rising::new(0.5, 0.5, 50.0));
    run.step(IDLE, 30);
    assert_eq!(run.puppets(1)[0].1.shown, Activity::Rising);
    run.apps[0].world_mut().entity_mut(zombie).remove::<Rising>();
    run.apps[0].world_mut().write_message(Gibbed { zombie });
    run.step(IDLE, 30);
    let puppet = &run.puppets(1)[0].1;
    assert!(puppet.crawler && run.puppets(0)[0].1.crawler, "{puppet:?}");
    assert_eq!(puppet.shown, Activity::Moving);
}
