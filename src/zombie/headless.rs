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
        .add_sub_state::<Pause>()
        .add_message::<PlayerHit>()
        .add_message::<HellhoundBite>()
        .add_message::<NovaSwipe>()
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
            Modifiers::default(),
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
