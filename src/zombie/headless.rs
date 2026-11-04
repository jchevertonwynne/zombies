//! Headless Zombies on the real Map, for measuring how they move frame by frame: the physics, the
//! fixed-step movement and the interpolation, with no window. Zombies are interpolated, so their
//! translation may only be written inside the fixed step (see `crate::player::Teleport`): a write
//! from `Update` is undone whenever the next frame runs a physics step. Other modules' tests use
//! this too, adding their own systems to the `App`.

use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::authority::{Authority, AuthorityPlugin};
use crate::modifier::Modifiers;
use crate::player::{LocalPlayer, Player};
use crate::state::GameState;

/// A Zombie as one rendered frame shows it.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub pos: Vec3,
    pub fixed_steps: u32,
}

#[derive(Resource, Default)]
struct Steps(u32);

pub struct Sim {
    pub app: App,
    pub frame_secs: f32,
}

impl Sim {
    /// The Map, the Zombie systems, and a stand-in Player at `PLAYER_START`, rendering at `fps`
    /// while physics runs at its fixed rate. `configure` adds anything else under test.
    pub fn new(fps: f64, configure: impl FnOnce(&mut App)) -> Self {
        Self::new_as(Authority::Host, fps, configure)
    }

    /// The same, running as `authority`.
    pub fn new_as(authority: Authority, fps: f64, configure: impl FnOnce(&mut App)) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            PhysicsPlugins::default(),
            StatesPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .insert_state(GameState::Playing)
        .insert_state(authority)
        .add_plugins(AuthorityPlugin)
        .add_sub_state::<Pause>()
        .add_message::<PlayerHit>()
        .add_message::<SpecialCue>()
        .add_message::<NovaCrawlerKilled>()
        .init_resource::<ActivePowerUps>()
        .insert_resource(NavGraph::new())
        .insert_resource(Barriers::default())
        .init_resource::<Steps>()
        .add_plugins(ZombiePlugin)
        .add_systems(FixedUpdate, |mut steps: ResMut<Steps>| steps.0 += 1)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps)));
        configure(&mut app);
        app.finish();
        app.cleanup();
        app.world_mut().run_system_once(crate::map::spawn_map).unwrap();
        app.world_mut().run_system_once(load_assets).unwrap();
        let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
        app.world_mut().spawn((
            Player,
            LocalPlayer,
            Modifiers::default(),
            crate::zone::PlayerZone::default(),
            Transform::from_translation(crate::map::PLAYER_START + Vec3::Y * half_height),
        ));
        let mut sim = Self {
            app,
            frame_secs: (1.0 / fps) as f32,
        };
        // Let the colliders settle in.
        sim.run(10);
        sim
    }

    /// A Walker standing at `feet`, or, given `rise` seconds, a Riser buried under it.
    pub fn spawn(&mut self, feet: Vec3, rise: Option<f32>) -> Entity {
        self.app
            .world_mut()
            .run_system_once(move |mut commands: Commands, assets: ZombieAssetsParam| {
                spawn_zombie(&mut commands, &assets, feet, 1000.0, SpeedTier::Walker, rise)
            })
            .unwrap()
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }

    pub fn send<M: Message>(&mut self, message: M) {
        self.app.world_mut().write_message(message);
    }

    /// One rendered frame, and where `entity` shows in it.
    pub fn frame(&mut self, entity: Entity) -> Sample {
        self.app.world_mut().resource_mut::<Steps>().0 = 0;
        self.app.update();
        let world = self.app.world();
        Sample {
            pos: world.get::<Transform>(entity).unwrap().translation,
            fixed_steps: world.resource::<Steps>().0,
        }
    }

    pub fn run(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    /// `frames` rendered frames of `entity`, stopping early if it's gone.
    pub fn track(&mut self, entity: Entity, frames: usize) -> Vec<Sample> {
        let mut samples = Vec::new();
        for _ in 0..frames {
            if self.app.world().get_entity(entity).is_err() {
                break;
            }
            samples.push(self.frame(entity));
        }
        samples
    }

    pub fn frames(&self, secs: f32) -> usize {
        (secs / self.frame_secs).round() as usize
    }
}

#[allow(dead_code, reason = "for measuring by hand with --nocapture")]
pub fn print(label: &str, samples: &[Sample], frame_secs: f32) {
    println!("--- {label}");
    let mut prev = samples[0].pos;
    for (i, s) in samples.iter().enumerate() {
        let d = s.pos - prev;
        println!(
            "{i:4} steps={} pos=({:7.3},{:7.3},{:7.3}) speed={:6.2} vy={:6.2}",
            s.fixed_steps,
            s.pos.x,
            s.pos.y,
            s.pos.z,
            d.with_y(0.0).length() / frame_secs,
            d.y / frame_secs,
        );
        prev = s.pos;
    }
}

/// Where a Zombie standing on the floor at height `floor` has its centre.
pub fn standing(floor: f32) -> f32 {
    floor + body_half_height() + 0.02
}

const FRAME_RATES: [f64; 3] = [144.0, 60.0, 90.0];

/// A Riser by the Yard's Riser point, with the Player out of the way in the Start Room.
const RISER_AT: Vec3 = Vec3::new(-7.0, 0.0, 10.0);

#[test]
#[ignore = "prints a measurement; asserts nothing"]
fn measure() {
    for fps in [144.0, 60.0] {
        let mut sim = Sim::new(fps, |_| {});
        let zombie = sim.spawn(RISER_AT, Some(ZOMBIE_RISE_SECS));
        let frames = sim.frames(ZOMBIE_RISE_SECS + 0.3);
        let samples = sim.track(zombie, frames);
        print(&format!("{fps} fps: a Riser climbing out"), &samples, sim.frame_secs);

        let walking = sim.track(zombie, sim.frames(0.5));
        print(&format!("{fps} fps: walking"), &walking, sim.frame_secs);
        sim.send(DamageZombie {
            player: None,
            zombie,
            damage: 1e6,
            zone: HitZone::Torso,
            attack: Attack::Bullet,
        });
        let dying = sim.track(zombie, sim.frames(ZOMBIE_FALL_SECS + 0.3));
        print(&format!("{fps} fps: falling over"), &dying, sim.frame_secs);

        let crawler = sim.spawn(Vec3::new(4.0, 0.0, 10.0), None);
        sim.run(sim.frames(0.5));
        sim.send(Gibbed { zombie: crawler });
        let lowered = sim.track(crawler, sim.frames(0.4));
        print(&format!("{fps} fps: losing its legs"), &lowered, sim.frame_secs);
    }
}

/// A Riser used to be raised from `Update`, where the step undid each frame's rise until the next
/// write. It climbs steadily out of the floor and stays standing once out, at any frame rate.
#[test]
fn a_riser_climbs_out_steadily_and_stays_up() {
    for fps in FRAME_RATES {
        let mut sim = Sim::new(fps, |_| {});
        let zombie = sim.spawn(RISER_AT, Some(ZOMBIE_RISE_SECS));
        let samples = sim.track(zombie, sim.frames(ZOMBIE_RISE_SECS + 0.5));
        let top = standing(RISER_AT.y);
        let climb = top - samples[0].pos.y;
        // Never sinks back on the way up, and never jumps more than its climb's speed allows in
        // one frame. (Once out, it settles the last 2 cm onto the floor, like any new Zombie.)
        let most = climb / ZOMBIE_RISE_SECS * sim.frame_secs * 1.5;
        let out = (0..samples.len()).max_by(|&a, &b| samples[a].pos.y.total_cmp(&samples[b].pos.y)).unwrap();
        for w in samples[..=out].windows(2) {
            let dy = w[1].pos.y - w[0].pos.y;
            assert!(dy > -1e-3 && dy < most, "{fps} fps: {} to {}", w[0].pos.y, w[1].pos.y);
        }
        // Out, and still standing once it walks off.
        let last = samples.last().unwrap();
        assert!((last.pos.y - top).abs() < 0.02, "{fps} fps: ended at {}", last.pos.y);
        let walking = sim.track(zombie, sim.frames(0.5));
        assert!(walking.iter().all(|s| (s.pos.y - top).abs() < 0.02), "{fps} fps: lost its footing");
    }
}

/// A dying Zombie used to sink from `Update`, so the step undid part of its fall and the body was
/// left floating. It lies on the floor at the end, its fall never reversing.
#[test]
fn a_killed_zombie_falls_all_the_way_to_the_floor() {
    for fps in FRAME_RATES {
        let mut sim = Sim::new(fps, |_| {});
        let zombie = sim.spawn(Vec3::new(0.0, 0.0, 12.0), None);
        sim.run(sim.frames(0.5));
        let killed_at = sim.frame(zombie).pos;
        sim.send(DamageZombie {
            player: None,
            zombie,
            damage: 1e6,
            zone: HitZone::Torso,
            attack: Attack::Bullet,
        });
        let samples = sim.track(zombie, sim.frames(ZOMBIE_FALL_SECS + 0.3));
        // Sinking evenly all the way: turning it over every frame mustn't freeze the sink between
        // physics steps either.
        let drop = body_half_height() - ZOMBIE_RADIUS;
        let per_frame = drop / ZOMBIE_FALL_SECS * sim.frame_secs;
        let lying = standing(0.0) - drop;
        for w in samples[1..].windows(2).filter(|w| w[1].pos.y > lying + 0.05) {
            let dy = w[0].pos.y - w[1].pos.y;
            assert!((dy - per_frame).abs() < per_frame * 0.2, "{fps} fps: {} to {}", w[0].pos.y, w[1].pos.y);
        }
        let last = samples.last().unwrap();
        assert!((last.pos.y - lying).abs() < 0.02, "{fps} fps: ended at {}, not {lying}", last.pos.y);
        // It falls where it was killed, give or take the last stride.
        assert!(last.pos.with_y(0.0).distance(killed_at.with_y(0.0)) < 0.1, "{fps} fps: slid to {}", last.pos);
    }
}

/// Losing its legs used to lower the body from `Update`, which the next step put straight back, so
/// the Crawler hung in the air and dropped. It's down at once, and stays down.
#[test]
fn a_zombie_that_loses_its_legs_drops_at_once() {
    for fps in FRAME_RATES {
        for offset in 0..3 {
            let mut sim = Sim::new(fps, |_| {});
            let zombie = sim.spawn(Vec3::new(4.0, 0.0, 10.0), None);
            sim.run(sim.frames(0.5) + offset);
            sim.send(Gibbed { zombie });
            let samples = sim.track(zombie, sim.frames(0.3));
            let low = standing(0.0) - (body_half_height() - CRAWLER_RADIUS);
            for s in &samples {
                assert!((s.pos.y - low).abs() < 0.03, "{fps} fps, offset {offset}: a frame at {}", s.pos.y);
            }
        }
    }
}

/// Stands the stand-in Player with their feet at `feet`.
fn put_player(sim: &mut Sim, feet: Vec3) {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    sim.world_mut()
        .query_filtered::<&mut Transform, With<Player>>()
        .single_mut(sim.app.world_mut())
        .unwrap()
        .translation = feet + Vec3::Y * half_height;
}

/// West Hall's ramp is the only way up to the Balcony, so a Zombie after a Player up there has to
/// climb it.
#[test]
fn a_zombie_walks_up_the_ramp_to_a_player_on_the_balcony() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(-12.5, 4.0, -2.0));
    let zombie = sim.spawn(Vec3::new(-15.7, 0.0, 14.0), None);
    // Stops following it if the stuck rule despawns it on the way.
    let (mut top, mut last) = (f32::MIN, Vec3::ZERO);
    for _ in 0..sim.frames(40.0) {
        sim.run(1);
        let Some(at) = sim.world_mut().get::<Transform>(zombie).map(|t| t.translation) else { break };
        (top, last) = (top.max(at.y), at);
    }
    assert!(top > standing(0.0) + 3.5, "got up to {top}, ended at {last}");
}

/// A Player on a Yard crate is only a metre above the floor: Zombies come to the crate and swipe
/// from beside it.
#[test]
fn a_zombie_swipes_a_player_standing_on_a_yard_crate() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(-3.0, 1.1, -6.0));
    sim.spawn(Vec3::new(-3.0, 0.0, 0.0), None);
    let mut hits = 0;
    for _ in 0..sim.frames(20.0) {
        sim.run(1);
        hits += sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().count();
    }
    assert!(hits >= 1, "the Zombie never landed a swipe");
}

/// The Pack-a-Punch Gazebo's west wall is thin: a Zombie just outside it, in reach of a Player
/// just inside, must never land a swipe through it.
#[test]
fn a_zombie_cannot_swipe_a_player_through_the_gazebo_wall() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(2.6, 0.0, -7.0));
    sim.spawn(Vec3::new(1.5, 0.0, -7.0), None);
    let mut hits = 0;
    for _ in 0..sim.frames(5.0) {
        sim.run(1);
        hits += sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().count();
    }
    assert_eq!(hits, 0, "the Zombie hit the Player through the wall");
}

/// A second Player standing at `feet`.
fn add_player(sim: &mut Sim, feet: Vec3) -> Entity {
    let half_height = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
    sim.world_mut()
        .spawn((
            Player,
            Modifiers::default(),
            crate::zone::PlayerZone::default(),
            Transform::from_translation(feet + Vec3::Y * half_height),
        ))
        .id()
}

fn hit_players(sim: &mut Sim, secs: f32) -> Vec<Entity> {
    let mut hit = Vec::new();
    for _ in 0..sim.frames(secs) {
        sim.run(1);
        hit.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| hit.player));
    }
    hit
}

/// With two Players up, a Zombie goes after the nearer, and the swipe names them.
#[test]
fn a_zombie_swipes_the_nearer_of_two_players() {
    let mut sim = Sim::new(60.0, |_| {});
    put_player(&mut sim, Vec3::new(11.0, 0.0, -2.0));
    let near = add_player(&mut sim, Vec3::new(-5.0, 0.0, 0.0));
    sim.spawn(Vec3::new(0.0, 0.0, 0.0), None);
    let hit = hit_players(&mut sim, 20.0);
    assert!(!hit.is_empty(), "the Zombie never landed a swipe");
    assert!(hit.iter().all(|&player| player == near), "it swiped the farther Player");
}

/// A Player who goes down is left alone at once, and the Zombie turns on the other.
#[test]
fn a_zombie_leaves_a_player_who_goes_down_for_the_other() {
    let mut sim = Sim::new(60.0, |_| {});
    let far = sim.world_mut().query_filtered::<Entity, With<Player>>().single(sim.app.world()).unwrap();
    put_player(&mut sim, Vec3::new(11.0, 0.0, -2.0));
    let near = add_player(&mut sim, Vec3::new(-5.0, 0.0, 0.0));
    sim.spawn(Vec3::new(-2.0, 0.0, 0.0), None);
    sim.run(sim.frames(0.5));
    let stand = crate::last_stand::LastStand::begin(&mut crate::weapon::Loadout::new(crate::arsenal::WeaponId::M1911));
    sim.world_mut().entity_mut(near).insert(stand);
    let hit = hit_players(&mut sim, 30.0);
    assert!(!hit.is_empty(), "the Zombie never landed a swipe");
    assert!(hit.iter().all(|&player| player == far), "it swiped the Player in Last Stand, or never turned");
}
