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

/// A `Teleport` sent from `Update` on a chosen frame, as the Teleporter sends one.
#[derive(Resource, Default)]
struct SendTeleport(Option<(Vec3, Option<Quat>)>);

fn send_teleport(mut request: ResMut<SendTeleport>, mut teleport: ResMut<Teleport>) {
    if let Some((feet, facing)) = request.0.take() {
        teleport.send(feet, facing);
    }
}

pub struct Sim {
    app: App,
    pub frame_secs: f32,
}

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
        .init_resource::<SendTeleport>()
        .init_resource::<Teleport>()
        .init_resource::<crate::modifier::RunModifiers>()
        .add_systems(Update, send_teleport)
        .insert_resource(Settings::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / fps)))
        .add_systems(FixedUpdate, (teleport_player, fill_move_input, move_player, |mut steps: ResMut<Steps>| steps.0 += 1).chain())
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

    /// Puts the Player's feet at `feet`, facing `yaw` radians (0 faces -Z), standing still, by
    /// `Teleport`.
    pub fn place(&mut self, feet: Vec3, yaw: f32) {
        self.hold(&[]);
        self.mouse(Vec2::ZERO);
        self.app.world_mut().resource_mut::<Teleport>().send(feet, Some(Quat::from_rotation_y(yaw)));
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

/// Writing the Transform from `Update` (as the Teleporter first did) was undone whenever the next
/// frame ran a physics step: the Player snapped straight back. A `Teleport` lands on every frame
/// offset and frame rate, shows no frame partway there, and turns and stops the Player.
#[test]
fn a_teleport_lands_cleanly_whenever_it_is_sent() {
    let to = Vec3::new(-10.5, 0.0, 23.0);
    let facing = Quat::from_rotation_y(1.0);
    for fps in [144.0, 60.0, 90.0] {
        for offset in 0..4 {
            let mut sim = Sim::new(fps);
            sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
            sim.hold(&[KeyCode::KeyD]);
            sim.run(30);
            sim.hold(&[]);
            sim.run_secs(0.3);
            sim.run(offset);
            let from = sim.frame().pos;
            sim.app.world_mut().resource_mut::<SendTeleport>().0 = Some((to, Some(facing)));
            let samples = sim.run(40);
            for s in &samples {
                let near_start = s.pos.distance(from) < 0.5;
                let near_end = s.pos.with_y(0.0).distance(to) < 0.05;
                assert!(near_start || near_end, "{fps} fps, offset {offset}: a frame at {}", s.pos);
            }
            let last = samples.last().unwrap();
            assert!(last.pos.with_y(0.0).distance(to) < 1e-3, "{fps} fps, offset {offset}: ended at {}", last.pos);
            assert!(last.grounded);
            let world = sim.app.world_mut();
            let rotation = world.query_filtered::<&Transform, With<Player>>().single(world).unwrap().rotation;
            assert!(rotation.angle_between(facing) < 1e-4);
        }
    }
}

/// The step is a plain function of the Player's state and one tick's input: replaying the same
/// inputs from the same start, as client prediction will, lands on the same position every time.
#[test]
fn replaying_the_same_inputs_from_the_same_start_reaches_the_same_position() {
    let mut sim = Sim::new(60.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.hold(&[]);
    let world = sim.app.world_mut();
    let (start, locomotion) = world
        .query_filtered::<(&Transform, &Locomotion), With<Player>>()
        .single(world)
        .map(|(transform, locomotion)| (transform.translation, locomotion.clone()))
        .unwrap();

    // Walk, sprint while turning, then jump and land.
    let inputs: Vec<MoveInput> = (0..120)
        .map(|tick| MoveInput {
            axes: if tick < 90 { Vec2::Y } else { Vec2::new(1.0, 1.0) },
            sprint: (30..70).contains(&tick),
            jump: tick == 80,
            yaw: tick as f32 * 0.02,
            ..default()
        })
        .collect();

    let run = |world: &mut World| {
        let args = (start, locomotion.clone(), inputs.clone());
        let id = world.register_system(
            move |player: Single<(Entity, &Collider, &Loadout, &Modifiers), With<Player>>,
                  move_and_slide: MoveAndSlide| {
                let (entity, collider, loadout, modifiers) = *player;
                let context = MoveContext { entity, collider, loadout, modifiers, drinking: false, last_stand: false };
                let (mut position, mut locomotion, inputs) = args.clone();
                for input in &inputs {
                    step_move(&mut position, &mut locomotion, input, &context, Duration::from_secs_f64(1.0 / 64.0), &move_and_slide);
                }
                (position, locomotion.velocity, locomotion.grounded)
            },
        );
        let result = world.run_system(id).unwrap();
        world.unregister_system(id).unwrap();
        result
    };
    let first = run(sim.app.world_mut());
    let second = run(sim.app.world_mut());
    assert_eq!(first, second);
    assert!(first.0.distance(start) > 5.0, "the Player didn't go anywhere: {}", first.0);
}
