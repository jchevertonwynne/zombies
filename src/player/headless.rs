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

    // Walk, sprint while turning, jump and land, crouch and crawl, then stand back up and settle.
    let inputs: Vec<MoveInput> = (0..180)
        .map(|tick| MoveInput {
            axes: if tick < 90 { Vec2::Y } else { Vec2::new(1.0, 1.0) },
            sprint: (30..70).contains(&tick),
            jump: tick == 80,
            crouch: (100..150).contains(&tick),
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
                (position, locomotion.velocity, locomotion.grounded, locomotion.stance, locomotion.eye)
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
