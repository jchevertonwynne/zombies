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

fn send_teleport(
    mut request: ResMut<SendTeleport>,
    player: Single<Entity, With<Player>>,
    mut teleport: MessageWriter<Teleport>,
) {
    if let Some((feet, facing)) = request.0.take() {
        teleport.write(Teleport { player: *player, feet, facing });
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
        .add_message::<Teleport>()
        .init_resource::<PendingTeleports>()
        .add_systems(PostUpdate, queue_teleports)
        .add_systems(Update, place_eye)
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
        let world = self.app.world_mut();
        let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.write_message(Teleport {
            player,
            feet,
            facing: Some(Quat::from_rotation_y(yaw)),
        });
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

    /// The Player's Stance.
    pub fn stance(&mut self) -> Stance {
        self.app
            .world_mut()
            .query_filtered::<&Locomotion, With<Player>>()
            .single(self.app.world())
            .unwrap()
            .stance
    }

    /// The Player's `Locomotion` as it stands.
    pub fn locomotion(&mut self) -> Locomotion {
        self.app.world_mut().query_filtered::<&Locomotion, With<Player>>().single(self.app.world()).unwrap().clone()
    }

    /// The Player goes Down: a solo Last Stand, as the Host would start it.
    pub fn go_down(&mut self) {
        let world = self.app.world_mut();
        let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.entity_mut(player).insert(LastStand::shown(LAST_STAND_SECS, false));
    }

    /// The Down ends, as a Revive or Self-Revive ends it.
    pub fn get_up(&mut self) {
        let world = self.app.world_mut();
        let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.entity_mut(player).remove::<LastStand>();
    }

    /// How far the Player's body reaches above their feet: the collider's height, end to end.
    pub fn body_height(&mut self) -> f32 {
        let world = self.app.world_mut();
        let collider = world.query_filtered::<&Collider, With<Player>>().single(world).unwrap();
        let aabb = collider.aabb(Vec3::ZERO, Quat::IDENTITY, 0.0);
        aabb.max.y - aabb.min.y
    }

    /// The camera's `Eye` height above the capsule centre, as the stance glide has it.
    pub fn eye(&mut self) -> f32 {
        self.app
            .world_mut()
            .query_filtered::<&Eye, With<PlayerCamera>>()
            .single(self.app.world())
            .unwrap()
            .height
    }

    /// Presses the Crouch key long enough for one tick to see it, then lets go.
    pub fn tap_crouch(&mut self) {
        self.hold(&[KeyCode::KeyC]);
        self.run_secs(0.05);
        self.hold(&[]);
    }

    /// Presses the Prone key long enough for one tick to see it, then lets go.
    pub fn tap_prone(&mut self) {
        self.hold(&[KeyCode::KeyZ]);
        self.run_secs(0.05);
        self.hold(&[]);
    }

    /// Whether the weapon side's firing gate has the Player blocked: the prone transitions, and
    /// a Dive from its launch through its landing recovery.
    pub fn firing_blocked(&mut self) -> bool {
        !self.app.world_mut().run_system_once(not_changing_prone).unwrap()
    }

    /// The camera's pitch, in radians: positive looking up.
    pub fn pitch(&mut self) -> f32 {
        let (_, pitch, _) = self
            .app
            .world_mut()
            .query_filtered::<&Transform, With<PlayerCamera>>()
            .single(self.app.world())
            .unwrap()
            .rotation
            .to_euler(EulerRot::YXZ);
        pitch
    }

    /// Which way the Player faces, in radians about +Y.
    pub fn yaw(&mut self) -> f32 {
        let (yaw, _, _) = self
            .app
            .world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(self.app.world())
            .unwrap()
            .rotation
            .to_euler(EulerRot::YXZ);
        yaw
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
/// inputs from the same start, as client prediction will, lands on the same position and the
/// same Stance every time.
#[test]
fn replaying_the_same_inputs_from_the_same_start_reaches_the_same_position_and_stance() {
    let mut sim = Sim::new(60.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.hold(&[]);
    let world = sim.app.world_mut();
    let (start, locomotion) = world
        .query_filtered::<(&Transform, &Locomotion), With<Player>>()
        .single(world)
        .map(|(transform, locomotion)| (transform.translation, locomotion.clone()))
        .unwrap();

    // Walk, sprint while turning, jump and land, crouch and crawl, go prone and crawl, stand
    // back up, sprint again and Dive — through the arc, the landing and its recovery — then
    // settle Prone.
    let inputs: Vec<MoveInput> = (0..340)
        .map(|tick| MoveInput {
            axes: if tick < 90 { Vec2::Y } else { Vec2::new(1.0, 1.0) },
            sprint: (30..70).contains(&tick) || (240..300).contains(&tick),
            jump: tick == 80,
            crouch: (100..140).contains(&tick),
            prone: (140..200).contains(&tick) || tick >= 300,
            yaw: tick as f32 * 0.02,
            ..default()
        })
        .collect();

    let run = |world: &mut World| {
        let args = (start, locomotion.clone(), inputs.clone());
        let id = world.register_system(
            move |player: Single<(Entity, &Loadout, &Modifiers), With<Player>>,
                  move_and_slide: MoveAndSlide| {
                let (entity, loadout, modifiers) = *player;
                let mut context = MoveContext { entity, loadout, modifiers, drinking: false, last_stand: false };
                let (mut position, mut locomotion, inputs) = args.clone();
                for input in &inputs {
                    step_move(&mut position, &mut locomotion, input, &mut context, Duration::from_secs_f64(1.0 / 64.0), &move_and_slide);
                }
                (
                    position,
                    locomotion.velocity,
                    locomotion.grounded,
                    locomotion.stance,
                    locomotion.eye,
                    // The Dive's own state rides the replay too: the same scripted arc, the
                    // same landing, the same recovery clock.
                    (locomotion.diving, locomotion.since_dive_landing),
                )
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
    // The scripted Dive really ran: it landed, and its recovery clock is part of the replay.
    assert!(
        !first.5.0 && first.5.1 < DIVE_RECOVERY_SECS,
        "the scripted Dive never landed: {:?}",
        first.5
    );
}

/// The Crouch key lowers the Player at once — the collider shrinks to the crouched capsule with
/// the feet staying put — while the camera glides between the eyes over the transition, and the
/// key toggles back up.
#[test]
fn crouching_takes_the_body_at_once_and_glides_the_eye() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    let feet = sim.frame().pos.y - Stance::Standing.half_height();
    assert!((sim.eye() - PLAYER_EYE_HEIGHT).abs() < 1e-6);

    sim.tap_crouch();
    assert_eq!(sim.stance(), Stance::Crouched, "the Crouch key didn't lower the Player");
    // The body went down at the start of the transition, feet staying put.
    let dropped = sim.frame().pos;
    assert!(
        (dropped.y - (feet + Stance::Crouched.half_height())).abs() < 1e-3,
        "the feet moved: {} down to {}",
        feet,
        dropped.y
    );
    // The camera stays where it was in the world (the height above the centre rises as the
    // centre drops) and then glides down over the transition.
    let crouched_eye = CROUCH_EYE_HEIGHT - Stance::Crouched.half_height();
    let starts_high = PLAYER_EYE_HEIGHT + Stance::Standing.half_height() - Stance::Crouched.half_height();
    assert!((sim.eye() - starts_high).abs() < 0.2, "the camera world height didn't hold: {}", sim.eye());
    sim.run_secs(0.125);
    let mid = sim.eye();
    assert!(mid > crouched_eye + 0.05 && mid < starts_high - 0.05, "the eye jumped to {mid}");
    sim.run_secs(0.25);
    assert!((sim.eye() - crouched_eye).abs() < 1e-3, "the eye settled at {}", sim.eye());

    // The key toggles back up, and the eye glides home.
    sim.tap_crouch();
    assert_eq!(sim.stance(), Stance::Standing);
    sim.run_secs(0.25);
    assert!((sim.eye() - PLAYER_EYE_HEIGHT).abs() < 1e-3, "the eye came back to {}", sim.eye());
}

/// A Crouched Player moves at 0.65× walk speed, mid-transition no faster.
#[test]
fn a_crouched_player_moves_at_crouch_speed() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.tap_crouch();
    sim.hold(&[KeyCode::KeyW]);
    // Mid-transition, and then settled: the slower Stance's speed the whole way.
    let mid = sim.run_secs(0.1);
    for speed in speeds(&mid, sim.frame_secs) {
        assert!(speed < PLAYER_WALK_SPEED * 0.9, "mid-transition it moved {speed} m/s");
    }
    sim.run_secs(0.2);
    let steady = sim.run_secs(0.8);
    for speed in speeds(&steady, sim.frame_secs) {
        assert!(
            (speed - PLAYER_WALK_SPEED * CROUCH_SPEED_MULT).abs() < 0.05,
            "crouched, it moved {speed} m/s"
        );
    }
    assert!(steady.iter().all(|s| s.grounded), "crouching lost the floor");
}

/// The Prone key lies the Player down at once — the collider shrinks to the 0.7 m prone shape
/// with the feet staying put — while the camera glides to the prone eye: 0.6 s down from
/// Standing and 0.4 s from Crouched, and the same back up.
#[test]
fn pressing_prone_takes_the_body_at_once_and_glides_the_eye_to_0_35() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    let feet = sim.frame().pos.y - Stance::Standing.half_height();

    // Down from Standing, over `STANCE_PRONE_FROM_STAND_SECS`.
    sim.tap_prone();
    assert_eq!(sim.stance(), Stance::Prone, "the Prone key didn't lie the Player down");
    // The body went down at the start of the transition, feet staying put.
    let dropped = sim.frame().pos;
    assert!((dropped.y - (feet + Stance::Prone.half_height())).abs() < 1e-3, "the feet moved");
    // The camera stays where it was in the world and then glides down over the transition.
    let prone_eye = PRONE_EYE_HEIGHT - Stance::Prone.half_height();
    let starts_high = PLAYER_EYE_HEIGHT + Stance::Standing.half_height() - Stance::Prone.half_height();
    assert!((sim.eye() - starts_high).abs() < 0.2, "the camera world height didn't hold: {}", sim.eye());
    sim.run_secs(0.3);
    let mid = sim.eye();
    assert!(mid > prone_eye + 0.05 && mid < starts_high - 0.05, "the eye jumped to {mid}");
    sim.run_secs(0.4);
    assert!((sim.eye() - prone_eye).abs() < 1e-3, "the eye settled at {}", sim.eye());

    // The key toggles back up, over the same 0.6 s, and the eye glides home.
    sim.tap_prone();
    assert_eq!(sim.stance(), Stance::Standing);
    sim.run_secs(0.3);
    assert!(sim.eye() > prone_eye + 0.05 && sim.eye() < PLAYER_EYE_HEIGHT - 0.05, "the eye jumped up to {}", sim.eye());
    sim.run_secs(0.4);
    assert!((sim.eye() - PLAYER_EYE_HEIGHT).abs() < 1e-3, "the eye came back to {}", sim.eye());

    // From Crouched the transitions take `STANCE_PRONE_SECS`, both ways.
    sim.tap_crouch();
    sim.run_secs(0.3);
    sim.tap_prone();
    assert_eq!(sim.stance(), Stance::Prone);
    // 0.35 s in, the 0.4 s glide is nearly down — the 0.6 s from-Standing one wouldn't be.
    sim.run_secs(0.3);
    assert!(sim.eye() < 0.15, "the 0.4 s glide was still high: {}", sim.eye());
    sim.run_secs(0.2);
    assert!((sim.eye() - prone_eye).abs() < 1e-3, "the eye settled at {}", sim.eye());
    sim.tap_crouch();
    sim.run_secs(0.55);
    assert!((sim.eye() - (CROUCH_EYE_HEIGHT - Stance::Crouched.half_height())).abs() < 1e-3, "the rise to Crouched settled at {}", sim.eye());
}

/// A Prone Player Crawls at 0.15× walk speed, mid-transition no faster.
#[test]
fn a_prone_player_crawls_at_prone_speed() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.tap_prone();
    sim.hold(&[KeyCode::KeyW]);
    // Mid-transition, and then settled: the slower Stance's speed the whole way.
    let mid = sim.run_secs(0.1);
    for speed in speeds(&mid, sim.frame_secs) {
        assert!(speed < PLAYER_WALK_SPEED * 0.9, "mid-transition it moved {speed} m/s");
    }
    sim.run_secs(0.6);
    let steady = sim.run_secs(0.8);
    for speed in speeds(&steady, sim.frame_secs) {
        assert!((speed - PLAYER_WALK_SPEED * PRONE_SPEED_MULT).abs() < 0.05, "prone, it moved {speed} m/s");
    }
    assert!(steady.iter().all(|s| s.grounded), "the crawl lost the floor");
}

/// Going down to Prone and getting up block firing until the transition is done; crouch
/// transitions block nothing.
#[test]
fn prone_transitions_block_firing_and_crouch_transitions_block_nothing() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    // A crouch change blocks nothing, either way.
    sim.tap_crouch();
    assert!(!sim.firing_blocked(), "going down to Crouched blocked firing");
    sim.run_secs(0.2);
    sim.tap_crouch();
    assert!(!sim.firing_blocked(), "standing up from Crouched blocked firing");
    sim.run_secs(0.3);

    // Going down to Prone from Standing blocks firing for the whole 0.6 s.
    sim.tap_prone();
    assert!(sim.firing_blocked(), "going down to Prone didn't block firing");
    sim.run_secs(0.4);
    assert!(sim.firing_blocked(), "the block ended early");
    sim.run_secs(0.2);
    assert!(!sim.firing_blocked(), "the block never ended");
    // Lying still in Prone blocks nothing.
    sim.run_secs(0.3);
    assert!(!sim.firing_blocked(), "lying prone blocked firing");

    // Getting up blocks too, until the rise is done.
    sim.tap_prone();
    assert!(sim.firing_blocked(), "standing up from Prone didn't block firing");
    sim.run_secs(0.4);
    assert!(sim.firing_blocked(), "the rise's block ended early");
    sim.run_secs(0.3);
    assert!(!sim.firing_blocked(), "the rise's block never ended");
}

/// Stance presses are ignored in the air — there are no crouch-jumps — but a held key still
/// lands the Player in its Stance when they do.
#[test]
fn stance_presses_are_ignored_in_the_air() {
    // Toggle: a press in the air does nothing, and isn't remembered for the landing.
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[KeyCode::KeyZ]);
    let airborne = sim.run_secs(0.1);
    assert!(airborne.iter().any(|s| !s.grounded), "the jump never left the floor");
    sim.hold(&[]);
    sim.run_secs(1.0);
    assert_eq!(sim.stance(), Stance::Standing, "the airborne press went prone");

    // Hold: the key held through the air lands the Player prone when they do.
    let mut sim = Sim::new(144.0);
    sim.app
        .world_mut()
        .insert_resource(crate::settings::Settings { stance_keys: crate::settings::StanceMode::Hold, ..default() });
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[KeyCode::KeyZ]);
    sim.run_secs(1.2);
    assert_eq!(sim.stance(), Stance::Prone, "the held key didn't land them prone");
}

/// While Prone, looking up is capped at `PRONE_LOOK_UP_MAX` above level; looking down and
/// turning stay free.
#[test]
fn looking_up_is_capped_while_prone() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    // Standing, a hard look up reaches the pitch limit.
    sim.mouse(Vec2::new(0.0, -30.0));
    sim.run_secs(0.2);
    assert!(sim.pitch() > 1.2, "the look up didn't reach: {}", sim.pitch());
    // Prone, the same look is capped.
    sim.mouse(Vec2::ZERO);
    sim.tap_prone();
    sim.run_secs(0.1);
    assert!((sim.pitch() - PRONE_LOOK_UP_MAX).abs() < 0.01, "the prone look wasn't capped: {}", sim.pitch());
    // A hard look up past the cap stays at it.
    sim.mouse(Vec2::new(0.0, -50.0));
    sim.run_secs(0.2);
    assert!((sim.pitch() - PRONE_LOOK_UP_MAX).abs() < 0.01, "the cap slipped: {}", sim.pitch());
    // Looking down is free to the pitch limit, and turning with it.
    let facing = sim.yaw();
    sim.mouse(Vec2::new(20.0, 50.0));
    sim.run_secs(0.2);
    assert!(sim.pitch() < -1.4, "looking down was capped: {}", sim.pitch());
    assert!((sim.yaw() - facing).abs() > 1.0, "the turn was limited");
}

/// Jump from Prone goes straight to Standing without jumping; Sprint from Prone stands the
/// Player up, and the sprint starts once upright if Sprint is still held.
#[test]
fn jump_and_sprint_from_prone_stand_you_up() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.tap_prone();
    sim.run_secs(0.7);
    let feet = sim.frame().pos.y - Stance::Prone.half_height();

    // Jump goes straight to Standing, and the press doesn't go on to jump.
    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[]);
    assert_eq!(sim.stance(), Stance::Standing, "Jump didn't stand the Player up");
    sim.run_secs(0.8);
    assert!((sim.frame().pos.y - Stance::Standing.half_height() - feet).abs() < 0.05, "the stand-up jumped");
    // A second press jumps like normal.
    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[]);
    let rising = sim.run_secs(0.4);
    let peak = rising.iter().map(|s| s.pos.y).fold(f32::MIN, f32::max);
    assert!(peak > feet + 0.5, "the jump reached {peak}");

    // Sprint stands the Player up, and the sprint starts once upright, Sprint still held.
    sim.run_secs(0.5);
    sim.tap_prone();
    sim.run_secs(0.7);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    let rising = sim.run_secs(0.3);
    assert_eq!(sim.stance(), Stance::Standing, "Sprint didn't stand the Player up");
    assert!(rising.iter().all(|s| !s.sprinting), "sprinted while getting up");
    let sprint = sim.run_secs(0.4);
    assert!(sprint.last().unwrap().sprinting, "the sprint never started once upright");
}

/// A ceiling the prone body fits under but the crouched one doesn't keeps the Prone Player
/// down: every rise passes the ceiling check for the shape it's growing into, and once
/// there's room the Player rises.
#[test]
fn a_ceiling_too_low_to_crouch_under_keeps_the_prone_player_down() {
    for mode in [crate::settings::StanceMode::Toggle, crate::settings::StanceMode::Hold] {
        let mut sim = Sim::new(144.0);
        sim.app
            .world_mut()
            .insert_resource(crate::settings::Settings { stance_keys: mode, ..default() });
        sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
        let feet = sim.frame().pos.y - Stance::Standing.half_height();
        // Its bottom 1.0 m up: the prone body (0.7 m) fits under, the crouched one (1.2 m) doesn't.
        let ceiling = sim
            .app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, feet + 1.25, 20.0),
                RigidBody::Static,
                Collider::cuboid(3.0, 0.5, 3.0),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            .id();
        sim.run_secs(0.1);

        // Going prone under it is fine — going down never needs room. The rise into Crouched is
        // refused: on Toggle a Crouch press, on Hold letting go of Prone with Crouch held.
        if mode == crate::settings::StanceMode::Toggle {
            sim.tap_prone();
            sim.run_secs(0.1);
            assert_eq!(sim.stance(), Stance::Prone);
            sim.tap_crouch();
        } else {
            sim.hold(&[KeyCode::KeyZ]);
            sim.run_secs(0.05);
            assert_eq!(sim.stance(), Stance::Prone);
            sim.hold(&[KeyCode::KeyC]);
        }
        sim.run_secs(0.5);
        assert_eq!(sim.stance(), Stance::Prone, "{mode:?}: rose into the ceiling");

        // Once there's room, the Player rises.
        sim.app.world_mut().entity_mut(ceiling).despawn();
        sim.run_secs(0.1);
        assert_eq!(sim.stance(), Stance::Crouched, "{mode:?}: didn't rise once there was room");
        assert!(sim.frame().grounded);
    }
}

/// Jump while Crouched stands the Player up instead of jumping; a second press jumps. Sprint
/// stands and sprints at once.
#[test]
fn jump_and_sprint_stand_you_up() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.tap_crouch();
    sim.run_secs(0.3);
    let feet = sim.frame().pos.y - Stance::Crouched.half_height();

    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[]);
    assert_eq!(sim.stance(), Stance::Standing, "Jump didn't stand the Player up");
    sim.run_secs(0.5);
    let after = sim.frame().pos.y - Stance::Standing.half_height();
    assert!((after - feet).abs() < 0.05, "the stand-up jumped: {after} against {feet}");

    // A second press jumps like normal.
    sim.hold(&[KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[]);
    let rising = sim.run_secs(0.4);
    let peak = rising.iter().map(|s| s.pos.y).fold(f32::MIN, f32::max);
    assert!(peak > feet + 0.5, "the jump reached {peak}");

    // Sprint stands and sprints at once.
    sim.tap_crouch();
    sim.run_secs(0.3);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    sim.run_secs(0.3);
    let up = sim.frame();
    assert!(up.sprinting, "Sprint didn't stand the Player up");

    // A jump swallowed on the way down, too: press Crouch and Jump together.
    sim.tap_crouch();
    sim.hold(&[KeyCode::KeyC, KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[]);
    assert_eq!(sim.stance(), Stance::Standing, "the pair didn't land Standing");
    sim.run_secs(0.4);
    assert!(
        (sim.frame().pos.y - Stance::Standing.half_height() - feet).abs() < 0.1,
        "the swallowed jump jumped anyway"
    );
}

/// Hold mode: the key keeps the Player down, letting go brings them up, and a Jump stand-up gives
/// way until the key is released and pressed again.
#[test]
fn hold_keeps_the_player_down_while_held() {
    let mut sim = Sim::new(144.0);
    sim.app
        .world_mut()
        .insert_resource(crate::settings::Settings { stance_keys: crate::settings::StanceMode::Hold, ..default() });
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);

    sim.hold(&[KeyCode::KeyC]);
    sim.run_secs(0.05);
    assert_eq!(sim.stance(), Stance::Crouched);
    sim.run_secs(0.3);
    assert_eq!(sim.stance(), Stance::Crouched, "the held key let go on its own");

    sim.hold(&[]);
    sim.run_secs(0.05);
    assert_eq!(sim.stance(), Stance::Standing, "letting go didn't bring them up");

    // Jump stands the Player up, and the still-held key gives way until it is released.
    sim.hold(&[KeyCode::KeyC]);
    sim.run_secs(0.05);
    sim.hold(&[KeyCode::KeyC, KeyCode::Space]);
    sim.run_secs(0.05);
    sim.hold(&[KeyCode::KeyC]);
    sim.run_secs(0.3);
    assert_eq!(sim.stance(), Stance::Standing, "the held key went back down after the Jump");

    // Released and pressed again goes back down.
    sim.hold(&[]);
    sim.run_secs(0.05);
    sim.hold(&[KeyCode::KeyC]);
    sim.run_secs(0.05);
    assert_eq!(sim.stance(), Stance::Crouched, "released and pressed again didn't go down");
}

/// A low ceiling refuses the stand-up silently, and the Player stands once there's room again.
#[test]
fn a_low_ceiling_refuses_the_stand_up_until_there_is_room() {
    for mode in [crate::settings::StanceMode::Toggle, crate::settings::StanceMode::Hold] {
        let mut sim = Sim::new(144.0);
        sim.app
            .world_mut()
            .insert_resource(crate::settings::Settings { stance_keys: mode, ..default() });
        sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
        let feet = sim.frame().pos.y - Stance::Standing.half_height();
        // A ceiling too low to stand under, high enough to crouch under: its bottom 1.4 m up.
        let ceiling = sim
            .app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, feet + 1.65, 20.0),
                RigidBody::Static,
                Collider::cuboid(3.0, 0.5, 3.0),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            .id();
        sim.run_secs(0.1);

        // Crouching under it is fine; standing back up is refused, silently. The two presses
        // get a tick of release between them, as a hand would.
        if mode == crate::settings::StanceMode::Toggle {
            sim.tap_crouch();
            sim.run_secs(0.05);
            sim.tap_crouch();
        } else {
            sim.hold(&[KeyCode::KeyC]);
            sim.run_secs(0.05);
            sim.hold(&[]);
        }
        sim.run_secs(0.5);
        assert_eq!(sim.stance(), Stance::Crouched, "{mode:?}: stood into the ceiling");

        // Once there's room, the Player stands.
        sim.app.world_mut().entity_mut(ceiling).despawn();
        sim.run_secs(0.1);
        assert_eq!(sim.stance(), Stance::Standing, "{mode:?}: didn't stand once there was room");
        assert!(sim.frame().grounded);
    }
}

/// A Teleporter ride places the Player Standing, whatever they rode as.
#[test]
fn a_teleporter_ride_places_the_player_standing() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    sim.tap_crouch();
    sim.run_secs(0.3);
    assert_eq!(sim.stance(), Stance::Crouched);

    sim.place(Vec3::new(-10.5, 0.0, 23.0), 0.0);
    assert_eq!(sim.stance(), Stance::Standing, "the ride didn't place them Standing");
    let arrived = sim.frame().pos;
    assert!((arrived.y - Stance::Standing.half_height()).abs() < 0.05, "arrived at {arrived}");
}

/// Pressing Prone out of a forward sprint Dives: the target going Prone while sprinting is
/// detected in the shared step, from the same `MoveInput` bits the Host replays. A lunge of
/// `DIVE_DISTANCE` in a low airborne arc of about `DIVE_FLIGHT_SECS`, under gravity, ending
/// Prone where it lands; releasing the keys mid-flight doesn't slow the lunge.
#[test]
fn a_dive_out_of_a_forward_sprint_lunges_prone_in_a_low_arc() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    sim.run_secs(0.25);
    let feet = sim.frame().pos.y - Stance::Standing.half_height();
    assert!(sim.frame().sprinting, "never got up to sprint");

    sim.hold(&[KeyCode::KeyZ]);
    let launch = sim.frame();
    assert_eq!(sim.stance(), Stance::Prone, "the prone press out of the sprint didn't Dive");
    assert!(!launch.grounded, "the Dive didn't leave the floor");
    // The keys are let go through the flight — the lunge carries on regardless.
    sim.hold(&[]);
    assert!(sim.firing_blocked(), "firing wasn't blocked in flight");

    let mut flight = Vec::new();
    while flight.len() < 200 && !flight.last().is_some_and(|s: &Sample| s.grounded) {
        flight.push(sim.frame());
    }
    let landed = flight.last().expect("the Dive never landed");
    assert_eq!(sim.stance(), Stance::Prone, "the Dive didn't end Prone");
    // About `DIVE_DISTANCE` along the sprint's way — EAST faces +X — in about
    // `DIVE_FLIGHT_SECS`, in a low arc, back on its feet where it started them. The floor probe
    // catches the landing a touch early and the rendered frames trail the stepped ones, so a
    // little of either end goes missing.
    let along = landed.pos.x - launch.pos.x;
    assert!((along - DIVE_DISTANCE).abs() < 0.5, "lunged {along} m");
    let secs = flight.len() as f32 * sim.frame_secs;
    assert!((secs - DIVE_FLIGHT_SECS).abs() < 0.15, "the flight took {secs} s");
    let peak = flight.iter().map(|s| s.pos.y).fold(f32::MIN, f32::max);
    assert!(
        peak - launch.pos.y > 0.3 && peak - launch.pos.y < 0.85,
        "the arc peaked {} m up",
        peak - launch.pos.y
    );
    // The landing frame is rendered before the interpolation has caught the step, so let the
    // body settle: Prone, back on its feet where it started them.
    sim.run_secs(0.1);
    assert!(
        (sim.frame().pos.y - feet - Stance::Prone.half_height()).abs() < 0.05,
        "didn't land Prone on the floor it left: {}",
        sim.frame().pos.y
    );
}

/// The trigger is Prone out of a sprint and nothing else. Crouch while sprinting just
/// crouches — there is no slide — and Prone while merely walking goes down on the floor.
#[test]
fn only_a_prone_press_out_of_a_sprint_dives() {
    // Crouch while sprinting: a crouch on the floor, no arc.
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    sim.run_secs(0.25);
    assert!(sim.frame().sprinting);
    sim.hold(&[KeyCode::KeyC]);
    let mut frames = sim.run_secs(0.05);
    sim.hold(&[]);
    frames.extend(sim.run_secs(0.5));
    assert_eq!(sim.stance(), Stance::Crouched, "Crouch while sprinting didn't crouch");
    assert!(frames.iter().all(|s| s.grounded), "the Crouch press left the floor");

    // Prone while walking, no Sprint: the prone transition, on the floor the whole way.
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.hold(&[KeyCode::KeyW]);
    sim.run_secs(0.2);
    assert!(!sim.frame().sprinting);
    sim.hold(&[KeyCode::KeyZ]);
    let mut frames = sim.run_secs(0.05);
    sim.hold(&[]);
    frames.extend(sim.run_secs(0.5));
    assert_eq!(sim.stance(), Stance::Prone, "the walking prone press didn't go Prone");
    assert!(frames.iter().all(|s| s.grounded), "the prone press without a sprint left the floor");
}

/// In flight the Dive steers only slightly: the held direction turns the lunge a little, and
/// nothing slows it or stops it.
#[test]
fn the_dive_is_stearable_only_slightly() {
    let mut straight = None;
    let mut steered = None;
    for keys in [&[] as &[KeyCode], &[KeyCode::KeyD]] {
        let mut sim = Sim::new(144.0);
        sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
        sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
        sim.run_secs(0.25);
        sim.hold(&[KeyCode::KeyZ]);
        let launch = sim.frame().pos;
        // Held through the flight: nothing, or a hard steer to the right.
        sim.hold(keys);
        let mut flight = Vec::new();
        while flight.len() < 200 && !flight.last().is_some_and(|s: &Sample| s.grounded) {
            flight.push(sim.frame());
        }
        let landed = flight.last().expect("the Dive never landed").pos;
        let offset = landed - launch;
        (if keys.is_empty() { &mut straight } else { &mut steered }).replace(offset);
    }
    let (straight, steered) = (straight.unwrap(), steered.unwrap());
    // The steer turns the lunge aside a little, but it's still a lunge: about as far along.
    let aside = (steered - straight).with_y(0.0).length();
    assert!(aside > 0.03 && aside < 0.6, "the steer moved the landing {aside} m aside");
    assert!((steered.x - straight.x).abs() < 0.3, "the steer ate the lunge: {}", steered.x - straight.x);
}

/// A Dive's landing recovery blocks firing and stance changes for `DIVE_RECOVERY_SECS` —
/// an ignored press changes nothing at all — and once it lifts the Player is free again.
#[test]
fn a_dive_lands_prone_locked_for_the_recovery() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    sim.run_secs(0.25);
    sim.hold(&[KeyCode::KeyZ]);
    assert!(!sim.frame().grounded, "the Dive didn't launch");
    sim.hold(&[]);
    let mut flight = Vec::new();
    while flight.len() < 200 && !flight.last().is_some_and(|s: &Sample| s.grounded) {
        flight.push(sim.frame());
    }
    assert!(flight.last().is_some_and(|s| s.grounded), "the Dive never landed");
    assert_eq!(sim.stance(), Stance::Prone);

    // 0.15 s after landing the prone transition's own block is over (it ended 0.1 s in) — any
    // block now is the recovery's.
    sim.run_secs(0.15);
    assert!(sim.firing_blocked(), "the recovery didn't hold the firing block");
    // Stance input under the lock is ignored outright: no stance change, nothing.
    sim.tap_crouch();
    assert_eq!(sim.stance(), Stance::Prone, "a stance change went through the recovery lock");
    sim.run_secs(0.05);
    assert_eq!(sim.stance(), Stance::Prone, "the lock let go early");

    // 0.5 s after landing the recovery has lifted: the target applies and the Player rises.
    sim.run_secs(0.3);
    assert_eq!(sim.stance(), Stance::Crouched, "the crouched target never applied once free");
    // And once the rise out of Prone is done too, firing is clear again.
    sim.run_secs(0.5);
    assert!(!sim.firing_blocked(), "the firing block never ended");
}

/// Off a ledge the Dive falls under gravity: the same lunge keeps falling past where the
/// flat-ground arc would have landed, and it still ends Prone.
#[test]
fn a_dive_off_a_ledge_falls_under_gravity() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(-12.0, 4.0, 4.0), EAST);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    let mut sprint = Vec::new();
    while sprint.last().is_none_or(|s: &Sample| s.pos.x < -10.7) {
        sprint.push(sim.frame());
    }
    assert!(sprint.last().unwrap().sprinting);

    sim.hold(&[KeyCode::KeyZ]);
    let launch = sim.frame();
    assert_eq!(sim.stance(), Stance::Prone, "the prone press out of the sprint didn't Dive");
    assert!(!launch.grounded);
    sim.hold(&[]);
    let mut flight = Vec::new();
    while flight.len() < 300 && !flight.last().is_some_and(|s: &Sample| s.grounded) {
        flight.push(sim.frame());
    }
    let landed = flight.last().expect("the ledge Dive never landed");
    let secs = flight.len() as f32 * sim.frame_secs;
    assert!(
        secs > DIVE_FLIGHT_SECS * 1.2,
        "the ledge flight took no longer than a flat one: {secs} s"
    );
    assert!(landed.pos.y < 3.0, "never fell below the Balcony: {}", landed.pos.y);
    assert!(landed.pos.x > -9.0, "didn't clear the edge: {}", landed.pos.x);
    assert_eq!(sim.stance(), Stance::Prone, "the ledge Dive didn't end Prone");
}

/// One `step_move` tick straight against the sim's world, from a crafted start — for the states
/// the Sim's own route can't reach, like the middle of a Dive's cooldown.
fn step_once(sim: &mut Sim, position: Vec3, locomotion: Locomotion, input: MoveInput) -> Locomotion {
    let args = (position, locomotion, input);
    let world = sim.app.world_mut();
    let id = world.register_system(
        move |player: Single<(Entity, &Loadout, &Modifiers), With<Player>>, move_and_slide: MoveAndSlide| {
            let (entity, loadout, modifiers) = *player;
            let mut context = MoveContext { entity, loadout, modifiers, drinking: false, last_stand: false };
            let (mut position, mut locomotion, input) = args.clone();
            step_move(&mut position, &mut locomotion, &input, &mut context, Duration::from_secs_f64(1.0 / 64.0), &move_and_slide);
            locomotion
        },
    );
    let result = world.run_system(id).unwrap();
    world.unregister_system(id).unwrap();
    result
}

/// A prone press out of a sprint inside `DIVE_COOLDOWN_SECS` of the last landing is ignored
/// outright — no Dive and no stance change either, the sprint carries on — and under the
/// recovery lock every stance input is; past the cooldown the same press Dives.
#[test]
fn the_cooldown_ignores_a_dive_press_and_the_recovery_locks_every_stance_change() {
    let mut sim = Sim::new(60.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), 0.0);
    let world = sim.app.world_mut();
    let (start, settled) = world
        .query_filtered::<(&Transform, &Locomotion), With<Player>>()
        .single(world)
        .map(|(transform, locomotion)| (transform.translation, locomotion.clone()))
        .unwrap();
    // A stand-in for a Player mid-sprint, somewhere in a past Dive's cooldown.
    let mut sprinting = settled.clone();
    sprinting.sprinting = true;
    sprinting.grounded = true;
    sprinting.stance = Stance::Standing;
    sprinting.left_stance = Stance::Standing;
    let dive_press = MoveInput { axes: Vec2::Y, sprint: true, prone: true, ..default() };
    let prone_press = MoveInput { axes: Vec2::Y, prone: true, ..default() };

    // 0.2 s after the last landing, the recovery holds: a prone press — even a plain one out
    // of a walk — changes nothing at all. The stance stays Standing.
    let mut locked = sprinting.clone();
    locked.since_dive_landing = 0.2;
    let stepped = step_once(&mut sim, start, locked, prone_press);
    assert_eq!(stepped.stance, Stance::Standing, "a stance change went through the recovery lock");

    // 0.9 s after: past the recovery, inside the cooldown. The prone press out of the sprint
    // is ignored outright — no Dive, no prone transition — and the sprint carries on.
    let mut cooling = sprinting.clone();
    cooling.since_dive_landing = 0.9;
    let stepped = step_once(&mut sim, start, cooling, dive_press);
    assert_eq!(stepped.stance, Stance::Standing, "the cooldown ate a stance change with the Dive");
    assert!(stepped.grounded, "the cooldown Dive left the floor");
    assert!(stepped.sprinting, "the sprint didn't carry on through the refused press");

    // 1.1 s after: the cooldown has passed, and the same press Dives.
    let mut free = sprinting;
    free.since_dive_landing = 1.1;
    let stepped = step_once(&mut sim, start, free, dive_press);
    assert_eq!(stepped.stance, Stance::Prone, "the press past the cooldown didn't Dive");
    assert!(!stepped.grounded, "the Dive didn't leave the floor");
    assert!(stepped.vertical_speed() > 3.0, "the lunge didn't lift: {}", stepped.vertical_speed());
}

/// The eye above the capsule centre in the Down pose: the prone eye in the prone body.
fn down_eye() -> f32 {
    PRONE_EYE_HEIGHT - Stance::Prone.half_height()
}

/// A Down from any Stance lies in the one Down pose: the prone body (0.7 m), the prone eye, the
/// stance reading Standing, the feet where they were. The collapse glides the eye down at one
/// speed, so from Crouched or Prone, where it starts lower, it's quicker.
#[test]
fn a_down_from_any_stance_collapses_into_the_one_down_pose() {
    let mut collapse_secs = Vec::new();
    for from in [Stance::Standing, Stance::Crouched, Stance::Prone] {
        let mut sim = Sim::new(144.0);
        sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
        match from {
            Stance::Standing => {}
            Stance::Crouched => sim.tap_crouch(),
            Stance::Prone => sim.tap_prone(),
        }
        sim.run_secs(1.0);
        assert_eq!(sim.stance(), from);
        let feet = sim.frame().pos.y - from.half_height();

        sim.go_down();
        let mut frames = 0;
        while (sim.eye() - down_eye()).abs() > 1e-3 && frames < 500 {
            sim.frame();
            frames += 1;
        }
        collapse_secs.push(frames as f32 * sim.frame_secs);
        sim.run_secs(0.2);
        let locomotion = sim.locomotion();
        assert!(locomotion.down, "{from:?}: not in the Down pose");
        assert_eq!(sim.stance(), Stance::Standing, "{from:?}: the Down didn't reset the stance");
        assert!((sim.body_height() - PRONE_COLLIDER_HEIGHT).abs() < 1e-3, "{from:?}: a {} m body", sim.body_height());
        assert!((sim.eye() - down_eye()).abs() < 1e-3, "{from:?}: the eye settled at {}", sim.eye());
        assert!(
            (sim.frame().pos.y - Stance::Prone.half_height() - feet).abs() < 0.05,
            "{from:?}: the feet moved"
        );
    }
    let [standing, crouched, prone] = collapse_secs[..] else { unreachable!() };
    assert!((standing - LAST_STAND_COLLAPSE_SECS).abs() < 0.05, "the collapse from Standing took {standing} s");
    assert!(crouched < standing, "from Crouched ({crouched} s) wasn't quicker than from Standing");
    assert!(prone < crouched, "from Prone ({prone} s) wasn't quicker than from Crouched");
}

/// While Down the camera shares the prone look limit, and stance input is dead: the keys change
/// nothing, and no lock of the prone transitions applies — Last Stand's own rules don't change.
#[test]
fn down_shares_the_prone_look_limit_and_stance_input_is_dead() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.go_down();
    sim.run_secs(0.5);
    sim.mouse(Vec2::new(0.0, -400.0));
    sim.run_secs(0.5);
    sim.mouse(Vec2::ZERO);
    assert!(sim.pitch() <= PRONE_LOOK_UP_MAX + 1e-3, "looked {} rad up while Down", sim.pitch());

    for tap in [Sim::tap_crouch, Sim::tap_prone] {
        tap(&mut sim);
        sim.run_secs(0.5);
        assert_eq!(sim.stance(), Stance::Standing, "a stance key moved a Downed Player");
        assert!(sim.locomotion().down);
        assert!(!sim.firing_blocked(), "a stance lock held while Down");
    }
}

/// A Revive or Self-Revive ends Standing, through a timed rise mirroring the collapse: a change out
/// of Prone, so the firing lock holds until upright, then lifts, with the feet where they were.
#[test]
fn the_end_of_a_down_rises_to_standing_under_the_stance_rules() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.tap_prone();
    sim.run_secs(1.0);
    sim.go_down();
    sim.run_secs(1.0);
    let feet = sim.frame().pos.y - Stance::Prone.half_height();

    sim.get_up();
    sim.run_secs(0.05);
    let rising = sim.locomotion();
    assert!(!rising.down, "still lying Down");
    assert_eq!(rising.stance, Stance::Standing, "the get-up wasn't to Standing");
    assert!(rising.prone_transition(), "the rise isn't run as a change out of Prone");
    assert!(sim.firing_blocked(), "firing wasn't blocked through the rise");
    assert!((sim.body_height() - 2.0 * Stance::Standing.half_height()).abs() < 1e-3, "the body didn't grow at once");

    sim.run_secs(LAST_STAND_COLLAPSE_SECS);
    assert!(!sim.firing_blocked(), "the lock outlasted the rise");
    let standing_eye = PLAYER_EYE_HEIGHT;
    assert!((sim.eye() - standing_eye).abs() < 1e-3, "the rise settled the eye at {}", sim.eye());
    assert!((sim.frame().pos.y - Stance::Standing.half_height() - feet).abs() < 0.05, "the feet moved");
    assert_eq!(sim.stance(), Stance::Standing, "there's a route back to Prone through the Down");
}

/// A Down mid-Dive takes over: the recovery lock and the cooldown are wiped, physics finishes the
/// flight, and the Player lands straight into the Down pose — no Prone, no lock.
#[test]
fn a_down_mid_dive_takes_over_the_flight() {
    let mut sim = Sim::new(144.0);
    sim.place(Vec3::new(0.0, 0.0, 20.0), EAST);
    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
    sim.run_secs(0.25);
    let feet = sim.frame().pos.y - Stance::Standing.half_height();
    sim.hold(&[KeyCode::KeyZ]);
    sim.run(3);
    sim.hold(&[]);
    assert!(sim.locomotion().diving, "never Dove");

    sim.go_down();
    let first = sim.frame();
    let downed = sim.locomotion();
    assert!(downed.down, "the Down didn't take over");
    assert!(!downed.diving, "the Dive kept flying as a Dive");
    assert!(!first.grounded, "the Down stopped the flight in the air");
    assert_eq!(downed.stance, Stance::Standing);

    let mut frames = 0;
    while !sim.frame().grounded && frames < 300 {
        frames += 1;
    }
    sim.run_secs(0.1);
    let landed = sim.locomotion();
    assert!(landed.down && landed.grounded, "didn't land in the Down pose");
    assert!(!landed.recovering_from_dive() && !landed.dive_on_cooldown(), "the Dive's locks survived the Down");
    assert!(!sim.firing_blocked(), "a Dive lock held while Down");
    assert!((sim.frame().pos.y - Stance::Prone.half_height() - feet).abs() < 0.05, "didn't land on the floor it left");
}
