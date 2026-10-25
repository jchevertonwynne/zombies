//! A headless Player on the real Map, for measuring how movement looks frame by frame: the
//! physics, the fixed-step movement, the interpolation and mouse look, with no window.

use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::time::TimeUpdateStrategy;

use super::*;

/// The Player as one rendered frame shows it.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub pos: Vec3,
    pub grounded: bool,
    pub sprinting: bool,
    pub fixed_steps: u32,
}

#[derive(Resource, Default)]
struct Steps(u32);

#[derive(Resource, Default)]
struct Placement(Option<Transform>);

fn place_player(mut placement: ResMut<Placement>, player: Single<(&mut Transform, &mut Locomotion), With<Player>>) {
    let (mut transform, mut locomotion) = player.into_inner();
    if let Some(to) = placement.0.take() {
        *transform = to;
        *locomotion = Locomotion::default();
    }
}

pub struct Sim {
    app: App,
    pub frame_secs: f32,
}

const HALF_HEIGHT: f32 = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;

impl Sim {
    /// The Map and the Player, rendering at `fps` while physics runs at its fixed rate.
    pub fn new(fps: f64) -> Self {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            PhysicsPlugins::default(),
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<AccumulatedMouseMotion>()
        .init_resource::<Steps>()
        .init_resource::<Placement>()
        .insert_resource(Settings::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps)))
        .add_systems(FixedUpdate, (place_player, move_player, |mut steps: ResMut<Steps>| steps.0 += 1).chain())
        .add_systems(RunFixedMainLoop, look.in_set(LookSystems));
        configure_look(&mut app);
        app.finish();
        app.cleanup();
        app.world_mut().run_system_once(crate::map::spawn_map).unwrap();
        app.world_mut().run_system_once(spawn_player).unwrap();
        let mut sim = Self {
            app,
            frame_secs: (1.0 / fps) as f32,
        };
        // Let the colliders settle in.
        sim.run(10);
        sim
    }

    /// Puts the Player's feet at `feet`, facing `yaw` radians (0 faces -Z), standing still. It
    /// happens in the fixed step, as a move would, so the interpolation carries it.
    pub fn place(&mut self, feet: Vec3, yaw: f32) {
        self.hold(&[]);
        self.mouse(Vec2::ZERO);
        self.app.world_mut().resource_mut::<Placement>().0 = Some(
            Transform::from_translation(feet + Vec3::Y * (HALF_HEIGHT + 0.02)).with_rotation(Quat::from_rotation_y(yaw)),
        );
        self.run(20);
    }

    pub fn hold(&mut self, keys: &[KeyCode]) {
        let mut input = self.app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        input.reset_all();
        for key in keys {
            input.press(*key);
        }
    }

    /// Mouse motion, in pixels, every frame from now on.
    pub fn mouse(&mut self, delta: Vec2) {
        self.app.world_mut().resource_mut::<AccumulatedMouseMotion>().delta = delta;
    }

    pub fn frame(&mut self) -> Sample {
        self.app.world_mut().resource_mut::<Steps>().0 = 0;
        self.app.update();
        let world = self.app.world_mut();
        let fixed_steps = world.resource::<Steps>().0;
        let (transform, locomotion) = world
            .query_filtered::<(&Transform, &Locomotion), With<Player>>()
            .single(world)
            .unwrap();
        Sample {
            pos: transform.translation,
            grounded: locomotion.grounded,
            sprinting: locomotion.sprinting,
            fixed_steps,
        }
    }

    pub fn run(&mut self, frames: usize) -> Vec<Sample> {
        (0..frames).map(|_| self.frame()).collect()
    }

    pub fn run_secs(&mut self, secs: f32) -> Vec<Sample> {
        self.run((secs / self.frame_secs).round() as usize)
    }
}

/// Facing +X.
const EAST: f32 = -FRAC_PI_2;

/// Rendered horizontal speed between consecutive frames.
fn speeds(samples: &[Sample], frame_secs: f32) -> Vec<f32> {
    samples
        .windows(2)
        .map(|w| (w[1].pos - w[0].pos).with_y(0.0).length() / frame_secs)
        .collect()
}

#[allow(dead_code, reason = "for measuring by hand with --nocapture")]
fn print(label: &str, samples: &[Sample], frame_secs: f32) {
    println!("--- {label}");
    let mut prev = samples[0].pos;
    for (i, s) in samples.iter().enumerate() {
        let d = s.pos - prev;
        println!(
            "{i:4} steps={} pos=({:7.3},{:7.3},{:7.3}) speed={:6.2} vy={:6.2} grounded={} sprint={}",
            s.fixed_steps,
            s.pos.x,
            s.pos.y,
            s.pos.z,
            d.with_y(0.0).length() / frame_secs,
            d.y / frame_secs,
            s.grounded as u8,
            s.sprinting as u8,
        );
        prev = s.pos;
    }
}

/// The whole route, printed. Run with `cargo test headless::measure -- --nocapture --ignored`.
#[test]
#[ignore = "prints a measurement; asserts nothing"]
fn measure() {
    for fps in [144.0, 60.0] {
        let mut sim = Sim::new(fps);
        sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
        sim.hold(&[KeyCode::KeyW]);
        let walk = sim.run_secs(0.8);
        print(&format!("{fps} fps: walk across the Start Room"), &walk, sim.frame_secs);
        sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
        let sprint = sim.run_secs(0.6);
        print(&format!("{fps} fps: sprint"), &sprint, sim.frame_secs);

        sim.place(Vec3::new(-15.7, 0.0, 11.0), 0.0);
        sim.hold(&[KeyCode::KeyW]);
        let ramp = sim.run_secs(2.5);
        print(&format!("{fps} fps: up the West Hall ramp"), &ramp, sim.frame_secs);

        sim.place(Vec3::new(-12.0, 4.0, 4.0), EAST);
        sim.hold(&[KeyCode::KeyW]);
        let drop = sim.run_secs(1.5);
        print(&format!("{fps} fps: off the Balcony drop"), &drop, sim.frame_secs);
    }
}

/// The old jank: turning in `Update` threw the interpolation away, so the Player froze on frames
/// without a physics step and lost ground on the rest (about 1.8 m/s rendered at 144 fps for a
/// 6.5 m/s walk). Now every frame moves the same distance, at any frame rate, even while turning.
#[test]
fn walking_moves_the_player_evenly_every_frame() {
    for fps in [144.0, 60.0, 90.0] {
        let mut sim = Sim::new(fps);
        sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
        sim.hold(&[KeyCode::KeyD]);
        sim.mouse(Vec2::new(0.0, 0.5));
        // Up to speed first.
        sim.run_secs(0.2);
        let samples = sim.run_secs(0.8);
        for speed in speeds(&samples, sim.frame_secs) {
            assert!((speed - PLAYER_WALK_SPEED).abs() < 0.05, "{fps} fps: {speed} m/s");
        }
    }
}

/// Move-and-slide alone lost the floor every other step, which stuttered sprint on and off.
#[test]
fn the_player_stays_grounded_walking_and_sprinting_on_flat_floor() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.hold(&[KeyCode::KeyW]);
    let walk = sim.run_secs(0.8);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    let sprint = sim.run_secs(0.6);
    assert!(walk.iter().chain(&sprint).all(|s| s.grounded));
    assert!(sprint[1..].iter().all(|s| s.sprinting));
    let feet = walk[0].pos.y;
    assert!(walk.iter().chain(&sprint).all(|s| (s.pos.y - feet).abs() < 1e-3));
    let top = speeds(&sprint, sim.frame_secs).into_iter().skip(20);
    assert!(top.into_iter().all(|speed| (speed - PLAYER_SPRINT_SPEED).abs() < 0.05));
}

#[test]
fn the_player_stays_grounded_up_and_down_the_ramp() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(-15.7, 0.0, 11.0), 0.0);
    sim.hold(&[KeyCode::KeyW]);
    let up = sim.run_secs(2.5);
    assert!(up.iter().all(|s| s.grounded), "lost the floor going up");
    let top = up.last().unwrap().pos;
    assert!(top.y > 4.0 && top.z < 0.0, "didn't reach the landing: {top}");

    // And back down, at a sprint.
    sim.place(top.with_y(4.0), std::f32::consts::PI);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    let down = sim.run_secs(1.5);
    assert!(down.iter().all(|s| s.grounded), "lost the floor going down");
    assert!(down.last().unwrap().pos.y < 1.0);
}

#[test]
fn the_balcony_drop_falls_and_lands_once() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(-12.0, 4.0, 4.0), EAST);
    sim.hold(&[KeyCode::KeyW]);
    // Through the gap in the rail and off the edge, then let go before reaching the Yard cover.
    let mut samples = Vec::new();
    while samples.last().is_none_or(|s: &Sample| s.pos.x < -9.0) {
        samples.push(sim.frame());
    }
    sim.hold(&[]);
    samples.extend(sim.run_secs(1.5));
    let grounded: Vec<bool> = samples.iter().map(|s| s.grounded).collect();
    // On the Balcony, then in the air, then on the Yard floor, with no flicker in between.
    let changes = grounded.windows(2).filter(|w| w[0] != w[1]).count();
    assert_eq!(changes, 2, "{grounded:?}");
    assert!(samples.last().unwrap().pos.y < 1.0);
}

/// The floor probe mustn't hold the Player down: a jump still leaves the floor and comes back.
#[test]
fn jumping_still_leaves_the_floor() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    let feet = sim.frame().pos.y;
    // Long enough for a physics step to see it.
    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[]);
    let samples = sim.run_secs(1.0);
    let peak = samples.iter().map(|s| s.pos.y).fold(f32::MIN, f32::max);
    let expected = PLAYER_JUMP_SPEED * PLAYER_JUMP_SPEED / (2.0 * GRAVITY);
    assert!((peak - feet - expected).abs() < 0.05, "peaked {} up", peak - feet);
    let last = samples.last().unwrap();
    assert!(last.grounded && (last.pos.y - feet).abs() < 1e-3);
}
