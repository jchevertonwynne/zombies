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
use crate::maps::barrier::Barriers;
use crate::enemies::hellhound::{HellhoundAssetsParam, HellhoundPlugin, spawn_hellhound};
use crate::enemies::nav::NavGraph;
use crate::enemies::hellhound::Telegraph;
use crate::maps::map::SpawnKind;
use crate::enemies::nova::{Burst, GasCloud, NovaAssetsParam, NovaCrawlerKilled, spawn_nova_crawler};
use crate::enemies::sapper::{self, Sapper, SapperAssetsParam};
use crate::enemies::special::{self, Napalm, SpecialAssetsParam, SpecialKind};
use crate::players::player::PlayerHit;
use crate::economy::power_up::ActivePowerUps;
use crate::enemies::special::SpecialCue;
use crate::enemies::zombie::rig::{Activity, Shown};
use crate::enemies::zombie::{Dying, Gibbed, Rising, SpeedTier, Zombie, ZombieAssetsParam, ZombiePlugin, spawn_zombie};
use crate::players::player::{LocalPlayer, Locomotion, MoveInput, Player, Seat, Stance, move_player, player_body};
use crate::state::{GameState, Pause, world_runs};
use crate::players::teammate::{Action, Pose, Shots};
use crate::tuning::*;

/// One machine of the Run: the Map and all `seats` Players, the one in `local` the `LocalPlayer`.
pub(super) fn machine(local: u8, seats: u8, install: impl FnOnce(&mut App)) -> App {
    machine_on(crate::maps::map::MapId::Greybox, local, seats, install)
}

/// The same on `map`.
pub(super) fn machine_on(map: crate::maps::map::MapId, local: u8, seats: u8, install: impl FnOnce(&mut App)) -> App {
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
        .insert_resource(map)
        .insert_resource(NavGraph::new(map))
        .insert_resource(Barriers::default())
        .add_plugins((ZombiePlugin, HellhoundPlugin))
        .add_systems(Startup, (special::load_assets, sapper::load_assets, crate::enemies::nova::load_assets))
        .add_systems(FixedUpdate, move_player.run_if(world_runs))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ))));
    install(&mut app);
    app.finish();
    app.cleanup();
    app.world_mut().run_system_once(crate::maps::map::spawn_map).unwrap();
    for seat in 0..seats {
        let mut player = app.world_mut().spawn(player_body(Seat(seat), "Player", &[], map));
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
        Self::on(crate::maps::map::MapId::Greybox, clients, delay, loss, jitter, extra)
    }

    /// The same on `map`.
    pub(super) fn on(map: crate::maps::map::MapId, clients: u8, delay: u32, loss: f32, jitter: u32, extra: &dyn Fn(&mut App)) -> Self {
        let mut hub = Hub::new(delay, loss);
        hub.jitter = jitter;
        let seats = clients + 1;
        let host_link = hub.host();
        let links: Vec<_> = (0..clients).map(|_| hub.join()).collect();
        let remotes: Vec<(Peer, u8)> = links.iter().map(|link| (link.peer(), link.peer() as u8)).collect();
        let mut apps = vec![machine_on(map, 0, seats, |app| {
            extra(app);
            install_host(app, host_link, remotes);
        })];
        for link in links {
            let seat = link.peer() as u8;
            apps.push(machine_on(map, seat, seats, |app| {
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

impl Run {
    /// `seat`'s `Locomotion` on machine `on`.
    fn locomotion(&mut self, on: usize, seat: u8) -> Locomotion {
        let world = self.apps[on].world_mut();
        let mut query = world.query_filtered::<(&Seat, &Locomotion), With<Player>>();
        query.iter(world).find(|(held, _)| held.0 == seat).unwrap().1.clone()
    }
}

/// Crouch, a crawl on the spot, a stand-up, a sprint and a Dive out of it, landing Prone.
fn low(seat: u8, tick: u32) -> MoveInput {
    let yaw = 0.3 * f32::from(seat);
    if seat != 1 {
        return MoveInput { yaw, ..default() };
    }
    match tick {
        20..60 => MoveInput { axes: Vec2::X, crouch: true, yaw, ..default() },
        60..120 => MoveInput { axes: Vec2::NEG_X, prone: true, yaw, ..default() },
        120..170 => MoveInput { yaw, ..default() },
        170..200 => MoveInput { axes: Vec2::Y, sprint: true, yaw, ..default() },
        200..260 => MoveInput { axes: Vec2::Y, sprint: true, prone: true, yaw, ..default() },
        _ => MoveInput { prone: true, yaw, ..default() },
    }
}

/// The Stance rides the buffered Steps, so the Host replays the same presses to the same Stance
/// and the same place: the client is never put right, the Dive's lunge included, and everyone
/// sees the Host's stepped Stance, the Dive as Prone ∧ airborne.
#[test]
fn stance_and_the_dive_replay_to_the_same_place() {
    let mut run = Run::new(2, 3, 0.0, 0);
    let mut worst = 0.0f32;
    let (mut crouched, mut prone, mut dived, mut seen_diving) = (false, false, false, false);
    while run.tick < 400 {
        run.step(&low, 1);
        worst = worst.max(run.corrected(1));
        let host = run.locomotion(0, 1);
        crouched |= host.stance == Stance::Crouched;
        prone |= host.stance == Stance::Prone && host.grounded();
        dived |= host.diving();
        let (seen, _) = run.pose_of(2, 1);
        seen_diving |= seen.stance == Stance::Prone && seen.airborne;
    }
    assert!(crouched && prone && dived, "the Host had them crouched {crouched}, prone {prone}, diving {dived}");
    assert!(seen_diving, "the other client never saw the Dive");
    assert!(worst < 0.01, "the Host put the Player right by {worst} m");
    let (own, host) = (run.locomotion(1, 1), run.locomotion(0, 1));
    assert_eq!((own.stance, host.stance), (Stance::Prone, Stance::Prone));
    assert!(run.at(1, 1).distance(run.at(0, 1)) < 0.01);
    // Everyone sees them lying where the Host has them.
    for on in [0, 2] {
        assert_eq!(run.pose_of(on, 1).0.stance, Stance::Prone, "machine {on}");
    }
}

/// A ceiling only the client's world has refuses their stand-up there; the Host, with room, stands
/// them, and the next snapshot heals the client's Stance to the Host's.
#[test]
fn a_ceiling_check_divergence_heals_to_the_hosts_stance() {
    let mut run = Run::new(1, 3, 0.0, 0);
    let prone = |_: u8, _: u32| MoveInput { prone: true, ..default() };
    run.step(&prone, 60);
    assert_eq!(run.locomotion(1, 1).stance, Stance::Prone);
    let feet = run.at(1, 1) - Vec3::Y * Stance::Prone.half_height();
    run.apps[1].world_mut().spawn((
        Transform::from_translation(feet + Vec3::Y * 1.3),
        RigidBody::Static,
        Collider::cuboid(1.0, 0.4, 1.0),
        CollisionLayers::new(crate::layers::GameLayer::World, LayerMask::ALL),
    ));
    run.step(&prone, 5);
    run.step(IDLE, 80);
    assert_eq!(run.locomotion(0, 1).stance, Stance::Standing, "the Host had room to stand");
    assert_eq!(run.locomotion(1, 1).stance, Stance::Standing, "the client wasn't healed");
}

/// A Zombie or Hellhound as machine `on` has it.
#[derive(Debug, PartialEq)]
struct Puppeted {
    at: Vec3,
    shown: Activity,
    crawler: bool,
    hellhound: bool,
    dying: bool,
    special: Option<SpecialKind>,
    nova: bool,
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
                        special: zombie.special_kind(),
                        nova: zombie.is_nova_crawler(),
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
    let start = crate::maps::map::PLAYER_START;
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
    let zombie = run.zombie(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0));
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

impl Run {
    /// A Special Zombie of `kind` on the Host's floor at `feet`, dressed as the game does.
    fn special(&mut self, feet: Vec3, kind: SpecialKind) -> Entity {
        let zombie = self.zombie(feet);
        let world = self.apps[0].world_mut();
        world
            .run_system_once(
                move |mut commands: Commands,
                      special: SpecialAssetsParam,
                      sapper: SapperAssetsParam,
                      zombies: Query<&Transform>| {
                    let at = zombies.get(zombie).unwrap().translation;
                    match kind {
                        SpecialKind::Sapper => sapper::dress_puppet(&mut commands, &sapper, zombie, at, true),
                        _ => special::dress_puppet(&mut commands, &special, zombie, kind),
                    }
                },
            )
            .unwrap();
        zombie
    }

    fn nova_crawler(&mut self, floor: Vec3) -> Entity {
        let world = self.apps[0].world_mut();
        world
            .run_system_once(move |mut commands: Commands, assets: NovaAssetsParam| {
                spawn_nova_crawler(&mut commands, &assets, SpawnKind::Riser, floor, 1000.0)
            })
            .unwrap()
    }
}

/// Each Special Zombie on the Host has its own body on a client, and a Zombie that is none has not.
#[test]
fn each_special_zombie_has_its_own_body_on_the_client() {
    let mut run = Run::new(2, 2, 0.0, 0);
    let start = crate::maps::map::PLAYER_START;
    let napalm = run.special(start + Vec3::new(8.0, 0.0, 4.0), SpecialKind::Napalm);
    let shrieker = run.special(start + Vec3::new(9.5, 0.0, 4.0), SpecialKind::Shrieker);
    let sapper = run.special(start + Vec3::new(11.0, 0.0, 4.0), SpecialKind::Sapper);
    let nova = run.nova_crawler(start + Vec3::new(12.5, 0.0, 4.0));
    run.zombie(start + Vec3::new(14.0, 0.0, 4.0));
    run.step(IDLE, 150);
    let ids: Vec<u16> = [napalm, shrieker, sapper, nova]
        .map(|zombie| run.apps[0].world().resource::<ZombieIds>().id(zombie).unwrap())
        .into();
    for on in 1..=2 {
        let seen = run.puppets(on);
        assert_eq!(seen.len(), 5, "machine {on}");
        let kinds: Vec<_> = ids.iter().map(|id| seen.iter().find(|(held, _)| held == id).map(|(_, p)| (p.special, p.nova))).collect();
        assert_eq!(
            kinds,
            [
                Some((Some(SpecialKind::Napalm), false)),
                Some((Some(SpecialKind::Shrieker), false)),
                Some((Some(SpecialKind::Sapper), false)),
                Some((None, true)),
            ],
            "machine {on}"
        );
        assert_eq!(seen.iter().filter(|(_, puppet)| puppet.special.is_none() && !puppet.nova).count(), 1);
        // The bodies the Host has: a Napalm glows with heat, the Sapper wears a helmet.
        let world = run.apps[on].world_mut();
        assert_eq!(world.query::<&Napalm>().iter(world).count(), 1, "machine {on}");
        let mut sappers = world.query::<&Sapper>();
        assert!(sappers.single(world).is_ok_and(|sapper| !sapper.helmet_lost()), "machine {on}");
    }

    // The Sapper loses its helmet on the Host, and the Nova Crawler is killed by a bullet and bursts.
    run.apps[0]
        .world_mut()
        .run_system_once(move |mut commands: Commands, mut sappers: Query<&mut Sapper>| {
            sappers.single_mut().unwrap().lose_helmet(&mut commands);
        })
        .unwrap();
    run.apps[0].world_mut().entity_mut(nova).insert((Burst, Dying::default()));
    run.step(IDLE, 80);
    for on in 1..=2 {
        let world = run.apps[on].world_mut();
        let mut sappers = world.query::<&Sapper>();
        assert!(sappers.single(world).is_ok_and(Sapper::helmet_lost), "machine {on}");
        assert_eq!(world.query::<&GasCloud>().iter(world).count(), 1, "machine {on} has no gas");
        let mut novas = world.query::<(&Zombie, Has<Burst>, Has<Dying>)>();
        let burst: Vec<_> = novas.iter(world).filter(|(zombie, ..)| zombie.is_nova_crawler()).collect();
        assert!(burst.len() == 1 && burst[0].1 && burst[0].2, "machine {on}");
    }
}

/// The Warden's helmet re-forms on a client's Sapper puppet when the Host's does, as it came off.
#[test]
fn the_wardens_helmet_re_forms_on_the_client_puppet() {
    let mut run = Run::new(1, 2, 0.0, 0);
    run.special(crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0), SpecialKind::Sapper);
    run.step(IDLE, 120);
    let helmet = |run: &mut Run, on: usize| {
        let world = run.apps[on].world_mut();
        let mut sappers = world.query::<&Sapper>();
        let sapper = sappers.single(world).unwrap();
        sapper.helmet_lost()
    };
    assert!(!helmet(&mut run, 1), "dressed with its helmet");
    run.apps[0]
        .world_mut()
        .run_system_once(|mut commands: Commands, mut sappers: Query<&mut Sapper>| {
            sappers.single_mut().unwrap().lose_helmet(&mut commands);
        })
        .unwrap();
    run.step(IDLE, 80);
    assert!(helmet(&mut run, 1), "off with the Host's");
    run.apps[0]
        .world_mut()
        .run_system_once(|mut commands: Commands, assets: SapperAssetsParam, mut sappers: Query<&mut Sapper>| {
            sappers.single_mut().unwrap().regain_helmet(&mut commands, &assets);
        })
        .unwrap();
    run.step(IDLE, 80);
    assert!(!helmet(&mut run, 0));
    assert!(!helmet(&mut run, 1), "back on");
}

/// Each Boss is told by the Boss bit, and its puppet is the Boss's size on every client; a Hellhound
/// is told of by name only for the Pack Alpha, and one that is no Boss stays the usual size.
#[test]
fn each_boss_is_its_size_on_every_client_and_the_pack_alpha_is_told_by_name() {
    use crate::net::state::{is_boss, unpack_special};
    use crate::net::zombies::Horde;
    use crate::run::gauntlet::Boss;
    use crate::run::gauntlet::arc::BossKind;
    let mut run = Run::new(2, 2, 0.0, 0);
    let start = crate::maps::map::PLAYER_START;
    let bloater = run.special(start + Vec3::new(8.0, 0.0, 4.0), SpecialKind::Napalm);
    let warden = run.special(start + Vec3::new(11.0, 0.0, 4.0), SpecialKind::Sapper);
    let alpha = run.hellhound(start + Vec3::new(-8.0, 0.0, 4.0));
    let pup = run.hellhound(start + Vec3::new(-8.0, 0.0, 7.0));
    for (entity, kind) in [(bloater, BossKind::Bloater), (warden, BossKind::Warden), (alpha, BossKind::PackAlpha)] {
        run.apps[0].world_mut().entity_mut(entity).insert(Boss::new(kind));
    }
    run.step(IDLE, 240);
    let id = |run: &Run, entity: Entity| run.apps[0].world().resource::<ZombieIds>().id(entity).unwrap();
    let ids = [bloater, warden, alpha, pup].map(|entity| id(&run, entity));
    let told: Vec<_> = ids
        .iter()
        .map(|&id| run.apps[0].world().resource::<Horde>().specials.iter().find(|s| s.id == id).map(|s| s.body))
        .collect();
    assert_eq!(told[2].map(|body| (is_boss(body), unpack_special(body).map(|(body, _)| body))), Some((true, Some(crate::net::state::SpecialBody::Hellhound))));
    assert!(told[0].is_some_and(is_boss) && told[1].is_some_and(is_boss));
    assert_eq!(told[3], None, "an ordinary Hellhound is told of by no name");
    for on in 1..=2 {
        let world = run.apps[on].world_mut();
        let mut query = world.query::<(&ZombieId, &Transform, &Zombie)>();
        let mut scale_of = |id: u16| query.iter(world).find(|(held, ..)| held.0 == id).map(|(_, t, z)| (t.scale.x, z.half_height()));
        assert_eq!(scale_of(ids[0]).map(|s| s.0), Some(GAUNTLET_BLOATER_SCALE), "machine {on} Bloater");
        assert_eq!(scale_of(ids[1]).map(|s| s.0), Some(GAUNTLET_WARDEN_SCALE), "machine {on} Warden");
        assert_eq!(scale_of(ids[2]).map(|s| s.0), Some(GAUNTLET_PACK_ALPHA_SCALE), "machine {on} Pack Alpha");
        assert_eq!(scale_of(ids[3]).map(|s| s.0), Some(1.0), "machine {on} ordinary hound");
    }
}

/// A Hellhound still in its lightning strike on the Host is in it on a client, and out when the
/// Host has it out.
#[test]
fn a_hellhound_puppet_shows_its_lightning_strike() {
    let mut run = Run::new(1, 2, 0.0, 0);
    let bolts = |run: &mut Run| {
        let world = run.apps[1].world_mut();
        let mut query = world.query::<&PointLight>();
        query.iter(world).count()
    };
    let lights = bolts(&mut run);
    run.hellhound(crate::maps::map::PLAYER_START + Vec3::new(-8.0, 0.0, 4.0));
    run.step(IDLE, 45);
    let strikes = |run: &mut Run, on: usize| {
        let world = run.apps[on].world_mut();
        let mut query = world.query_filtered::<&Telegraph, With<ZombieId>>();
        query.iter(world).count()
    };
    assert_eq!((strikes(&mut run, 0), strikes(&mut run, 1)), (1, 1));
    assert_eq!(bolts(&mut run), lights + 1, "no bolt on the client");
    run.step(IDLE, 150);
    assert_eq!((strikes(&mut run, 0), strikes(&mut run, 1)), (0, 0));
    assert_eq!(bolts(&mut run), lights, "the bolt stayed");
}

/// A client at `delay` ticks each way strafing along Zombies that walk at it, grinding along them
/// for 4 s: the worst correction, and the farthest its Player was drawn to move in a frame, and
/// would have been without easing the corrections out.
fn grinding(delay: u32) -> (f32, f32, f32) {
    let mut run = Run::new(1, delay, 0.0, 0);
    let feet = run.at(0, 1) - Vec3::Y * Stance::Standing.half_height();
    let zombies: Vec<Entity> = (0..3).map(|i| run.zombie(feet + Vec3::new(-1.0 + i as f32 * 0.7, 0.0, -3.0))).collect();
    run.step(IDLE, 30);
    let (mut worst, mut drawn_stride, mut raw_stride) = (0.0f32, 0.0f32, 0.0f32);
    let mut last: Option<(Vec3, Vec3)> = None;
    for tick in 0..256u32 {
        let player = run.at(0, 1);
        let world = run.apps[0].world_mut();
        for &zombie in &zombies {
            let mut transform = world.get_mut::<Transform>(zombie).unwrap();
            let to = (player - transform.translation).with_y(0.0);
            if to.length() > 0.7 {
                transform.translation += to.normalize() * 1.5 / FIXED_HZ as f32;
            }
        }
        let strafe = if (tick / 64) % 2 == 0 { Vec2::X } else { Vec2::NEG_X };
        run.step(&|seat, _| MoveInput { axes: if seat == 1 { strafe + Vec2::Y * 0.5 } else { Vec2::ZERO }, ..default() }, 1);
        worst = worst.max(run.corrected(1));
        let drawn = run.at(1, 1);
        let raw = drawn - run.apps[1].world().resource::<Prediction>().drawn;
        if let Some((drawn_was, raw_was)) = last
            // The turns at each end of the strafe are no correction.
            && tick % 64 > 4
        {
            drawn_stride = drawn_stride.max(drawn.distance(drawn_was));
            raw_stride = raw_stride.max(raw.distance(raw_was));
        }
        last = Some((drawn, raw));
    }
    (worst, drawn_stride, raw_stride)
}

/// Grinding along Zombies at a 156 ms ping, the client is put right by a good deal, as its
/// Zombies are where the Host's were a moment ago. Each correction is eased out of how it is
/// drawn, so the Player never moves farther in a frame than its own stride.
#[test]
fn corrections_grinding_along_zombies_are_eased_out() {
    let (_, stride, _) = grinding(0);
    let (worst, drawn, raw) = grinding(5);
    assert!(worst > 0.15, "the Host only put the Player right by {worst} m");
    assert!(raw > stride + 0.1, "unsmoothed, a frame moved only {raw} m");
    assert!(drawn < stride + 0.01, "a frame moved the Player {drawn} m against a stride of {stride} m");
}

/// A walk through the Cove's surf slows the client exactly where it slows the Host: wading is
/// worked out inside the step from the feet, so the client's replay lands on the Host's place and
/// is never put right.
#[test]
fn a_walk_through_shallow_water_replays_to_the_same_place() {
    use crate::maps::map::MapId;
    let mut run = Run::on(MapId::Cove, 1, 3, 0.0, 0, &|_| {});
    // The Seats are on the dry sand above the surf, so walk south into it first.
    run.step(&|_, _| MoveInput { axes: Vec2::Y, yaw: 0.0, ..default() }, 90);
    run.step(&|_, _| MoveInput::default(), 30);
    let start = run.at(0, 1);
    assert!(
        crate::maps::water::wading(MapId::Cove.layout(), Default::default(), start - Vec3::Y * Stance::Standing.half_height()),
        "the Player isn't in the surf at {start}"
    );
    let east = -std::f32::consts::FRAC_PI_2;
    let mut worst = 0.0f32;
    for _ in 0..200 {
        run.step(&|_, _| MoveInput { axes: Vec2::Y, yaw: east, ..default() }, 1);
        worst = worst.max(run.corrected(1));
    }
    run.step(&|_, _| MoveInput { yaw: east, ..default() }, 60);
    let (host, own) = (run.at(0, 1), run.at(1, 1));
    let went = (host - start).xz().length();
    let dry = PLAYER_WALK_SPEED * 200.0 / FIXED_HZ as f32;
    assert!(went > 3.0, "the Player only went {went} m");
    assert!(went < dry * 0.8, "the Player went {went} m of a dry {dry} m: not slowed");
    assert!(own.distance(host) < 0.05, "the client has {own} for the Host's {host}");
    assert!(worst < 0.05, "the Host put the client right by {worst} m");
}

/// A client's whole Zipline ride, predicted from its own inputs, matches the Host's replay of the
/// same inputs to the landing: the Host is never put right, the other client sees the rider on the
/// line and then landed, and the ride arrives when its pace says.
#[test]
fn a_zipline_ride_replays_to_the_same_place_on_the_host() {
    let line = crate::maps::map::MapId::Cove.layout().ziplines[1];
    let open = |app: &mut App| {
        app.insert_resource(crate::maps::zipline::OpenZones(crate::maps::zone::ZoneId::ALL.into_iter().collect()));
    };
    let mut run = Run::on(crate::maps::map::MapId::Cove, 2, 3, 0.0, 0, &open);
    // Stand seat 1 on the top, on every machine.
    for on in 0..3 {
        let world = run.apps[on].world_mut();
        let mut query = world.query_filtered::<(&Seat, &mut Transform, &mut Position, &mut Locomotion), With<Player>>();
        for (seat, mut transform, mut position, mut locomotion) in query.iter_mut(world) {
            if seat.0 == 1 {
                transform.translation = line.from + Vec3::Y * Stance::Standing.half_height();
                position.0 = transform.translation;
                locomotion.velocity = Vec3::ZERO;
                locomotion.grounded = true;
            }
        }
    }
    let grab = |_: u8, tick: u32| MoveInput { grab: tick == 31, ..default() };
    let (mut worst, mut seen_riding, mut host_riding, mut landed_at) = (0.0f32, false, false, None);
    let start = run.tick;
    run.step(IDLE, 10);
    while run.tick < start + 450 {
        run.step(&grab, 1);
        worst = worst.max(run.corrected(1));
        host_riding |= run.locomotion(0, 1).riding();
        seen_riding |= run.pose_of(2, 1).0.action == Action::Ride;
        if host_riding && !run.locomotion(0, 1).riding() && landed_at.is_none() {
            landed_at = Some(run.tick);
        }
    }
    assert!(host_riding && seen_riding, "the Host had them riding {host_riding}, the other client {seen_riding}");
    assert!(worst < 0.05, "the Host put the Player right by {worst} m");
    let (own, host) = (run.locomotion(1, 1), run.locomotion(0, 1));
    assert!(!own.riding() && !host.riding());
    let feet = |at: Vec3| at - Vec3::Y * Stance::Standing.half_height();
    assert!(feet(run.at(0, 1)).distance(line.to) < 1.5, "the Host has them at {}", feet(run.at(0, 1)));
    assert!(run.at(1, 1).distance(run.at(0, 1)) < 0.05);
    // The other client draws them where the Host has them, .
    assert!(run.at(2, 1).distance(run.at(0, 1)) < 0.2, "the other client draws them at {}", run.at(2, 1));
    assert_eq!(run.pose_of(2, 1).0.action, Action::None);
}

/// On the Dam, the Cove, the Polder, the Sanatorium and the Archive (whose plan is 72 by 36 m, told from its middle in `i16`s), a client has the Host's Player and a Zombie where the
/// Host has them.
#[test]
fn far_maps_show_the_hosts_player_and_zombie_where_they_are() {
    for map in [crate::maps::map::MapId::Dam, crate::maps::map::MapId::Cove, crate::maps::map::MapId::Polder, crate::maps::map::MapId::Sanatorium, crate::maps::map::MapId::Archive] {
        let mut run = Run::on(map, 2, 2, 0.0, 0, &|_| {});
        let at = run.at(0, 0);
        let zombie = run.zombie(at + Vec3::new(6.0, -Stance::Standing.half_height(), 2.0));
        run.step(&walk, 150);
        run.step(IDLE, 60);
        let host_zombie = run.apps[0].world().get::<Transform>(zombie).unwrap().translation;
        for on in 1..=2 {
            let seen = run.at(on, 0);
            assert!(seen.distance(run.at(0, 0)) < 0.2, "{}: machine {on} has the Host's Player at {seen}", map.name());
            let puppets = run.puppets(on);
            assert_eq!(puppets.len(), 1, "{}: machine {on}", map.name());
            assert!(puppets[0].1.at.distance(host_zombie) < 0.5, "{}: machine {on} has the Zombie at {}", map.name(), puppets[0].1.at);
        }
    }
}

/// A Turned ally on the Host (a Zombie that keeps its id but is no longer a `Zombie`) shows on every
/// client as an ally puppet with no hit areas, so a client's shots pass through it. It stays on
/// while it falls, and goes with its body.
#[test]
fn a_turned_ally_shows_on_every_client_with_no_hit_areas() {
    use crate::enemies::zombie::HitArea;
    use crate::net::zombies::PuppetAlly;
    use crate::weapons::ammo_mod::TurnedAlly;
    let mut run = Run::new(2, 2, 0.0, 0);
    let start = crate::maps::map::PLAYER_START;
    let zombie = run.zombie(start + Vec3::new(8.0, 0.0, 4.0));
    let other = run.zombie(start + Vec3::new(10.0, 0.0, 4.0));
    run.step(IDLE, 100);
    let id = run.apps[0].world().resource::<ZombieIds>().id(zombie).unwrap();
    let armed = |run: &mut Run, on: usize, id: u16| {
        let world = run.apps[on].world_mut();
        let entity = world.resource::<ZombieIds>().entity(id).expect("a puppet");
        let kids: Vec<Entity> = world.get::<Children>(entity).unwrap().iter().collect();
        let shootable = kids.iter().filter(|&&kid| world.get::<HitArea>(kid).is_some() && world.get::<Collider>(kid).is_some()).count();
        (world.get::<PuppetAlly>(entity).is_some(), shootable)
    };
    for on in 1..=2 {
        let (ally, shootable) = armed(&mut run, on, id);
        assert!(!ally && shootable > 0, "machine {on} starts with a shootable Zombie");
    }
    // The Host turns it: it keeps its id and loses `Zombie`.
    run.apps[0].world_mut().entity_mut(zombie).remove::<Zombie>().insert(TurnedAlly::default());
    run.step(IDLE, 100);
    let other_id = run.apps[0].world().resource::<ZombieIds>().id(other).unwrap();
    for on in 1..=2 {
        assert_eq!(armed(&mut run, on, id), (true, 0), "machine {on}: an ally nobody can shoot");
        assert!(!armed(&mut run, on, other_id).0, "machine {on}: the other is no ally");
        assert_eq!(run.puppets(on).len(), 2, "machine {on}");
    }
    // It falls when its time is up, still an ally, and then it is gone.
    run.apps[0].world_mut().entity_mut(zombie).remove::<TurnedAlly>().insert(Dying::default());
    run.step(IDLE, 40);
    for on in 1..=2 {
        let (_, puppet) = run.puppets(on).into_iter().find(|(held, _)| *held == id).expect("the fallen ally");
        assert!(puppet.dying, "machine {on}");
    }
    run.apps[0].world_mut().entity_mut(zombie).despawn();
    run.step(IDLE, 60);
    for on in 1..=2 {
        assert_eq!(run.puppets(on).len(), 1, "machine {on}");
    }
}

/// Noclip rides the buffered Steps: the Host, which allows it, flies the Player up to exactly where
/// their game has them, with no correction, and they come down together once the bit stops. A Host
/// that does not allow it ignores the bit, and the Player never leaves the floor. Returns the Run,
/// the highest the Host had the Player and the worst correction.
fn flight(dev: crate::run::dev::DevMode) -> (Run, f32, f32) {
    let mut run = Run::with(1, 3, 0.0, 0, &|app| {
        app.insert_resource(crate::run::dev::DevAccess(dev));
    });
    let floor = run.at(0, 1).y;
    let up = |seat: u8, tick: u32| match (seat, tick) {
        (1, 30..90) => MoveInput { noclip: true, jump: true, ..default() },
        _ => MoveInput::default(),
    };
    let (mut worst, mut highest) = (0.0f32, 0.0f32);
    for _ in 0..250 {
        run.step(&up, 1);
        worst = worst.max(run.corrected(1));
        highest = highest.max(run.at(0, 1).y - floor);
    }
    (run, highest, worst)
}

#[test]
fn noclip_flies_the_same_on_both_without_correction() {
    let (mut run, highest, worst) = flight(crate::run::dev::DevMode::On);
    assert!(highest > 3.0, "the Host only had them {highest} m up");
    assert!(worst < 0.05, "put right by {worst} m");
    assert!(run.at(1, 1).distance(run.at(0, 1)) < 0.05);
    assert!(!run.locomotion(0, 1).flying(), "the flight ends with the bit");
}

#[test]
fn noclip_forward_climbs_with_the_look_the_same_on_both() {
    let mut run = Run::with(1, 3, 0.0, 0, &|app| {
        app.insert_resource(crate::run::dev::DevAccess(crate::run::dev::DevMode::On));
    });
    let floor = run.at(0, 1).y;
    let climb = |seat: u8, tick: u32| match (seat, tick) {
        (1, 30..90) => MoveInput { noclip: true, axes: Vec2::Y, pitch: 0.6, ..default() },
        _ => MoveInput::default(),
    };
    let (mut worst, mut highest) = (0.0f32, 0.0f32);
    for _ in 0..90 {
        run.step(&climb, 1);
        worst = worst.max(run.corrected(1));
        highest = highest.max(run.at(0, 1).y - floor);
    }
    assert!(highest > 3.0, "the Host only had them {highest} m up");
    assert!(worst < 0.05, "put right by {worst} m");
    assert!(run.at(1, 1).distance(run.at(0, 1)) < 0.05);
}

#[test]
fn a_host_that_does_not_allow_noclip_ignores_the_bit() {
    for dev in [crate::run::dev::DevMode::Off, crate::run::dev::DevMode::HostOnly] {
        let (mut run, highest, _) = flight(dev);
        assert!(highest < 1.5, "{dev:?}: flew {highest} m up");
        assert!(run.at(1, 1).distance(run.at(0, 1)) < 0.05, "{dev:?}");
    }
}
