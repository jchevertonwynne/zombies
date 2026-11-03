//! Camera sway: a head bob tied to the stride, a dip on landing, and the view model trailing mouse
//! look and bobbing with the stride. Purely cosmetic: it slides the camera and the view model a
//! few centimetres without ever turning the camera, so the crosshair stays exactly on the
//! bullets' path, and it never touches the Player.

use std::f32::consts::{PI, TAU};
use std::ops::{Add, Mul, Sub};

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::last_stand::{LastStand, lower_eye};
use crate::player::{Eye, LocalPlayer, Locomotion, PlayerCamera};
use crate::settings::Settings;
use crate::state::Pause;
use crate::tuning::*;
use crate::weapon::{AnimateViewModel, Loadout, ViewModel};

/// How the Player is getting about, for the bob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    OnFoot,
    /// The Last Stand crawl, which keeps a slow sway.
    Crawl,
}

/// Strides per second (one stride is two footsteps) at a ground speed.
pub fn stride_rate(speed: f32, gait: Gait) -> f32 {
    let stride = match gait {
        Gait::OnFoot => BOB_STRIDE_METRES,
        Gait::Crawl => CRAWL_SWAY_STRIDE_METRES,
    };
    speed.max(0.0) / stride
}

/// The (side-to-side, down) bob amplitude at a ground speed: nothing standing still, growing to the
/// walk values at walk speed and on to the sprint values at sprint speed. The crawl has its own.
pub fn bob_amplitude(speed: f32, gait: Gait) -> Vec2 {
    let speed = speed.max(0.0);
    let walk = Vec2::new(BOB_WALK_LATERAL, BOB_WALK_VERTICAL);
    let sprint = Vec2::new(BOB_SPRINT_LATERAL, BOB_SPRINT_VERTICAL);
    match gait {
        Gait::Crawl => {
            let crawl_speed = PLAYER_WALK_SPEED * LAST_STAND_CRAWL_MULT;
            Vec2::new(CRAWL_SWAY_LATERAL, CRAWL_SWAY_VERTICAL) * (speed / crawl_speed).min(1.0)
        }
        Gait::OnFoot if speed <= PLAYER_WALK_SPEED => walk * speed / PLAYER_WALK_SPEED,
        Gait::OnFoot => {
            let t = (speed - PLAYER_WALK_SPEED) / (PLAYER_SPRINT_SPEED - PLAYER_WALK_SPEED);
            walk.lerp(sprint, t.min(1.0))
        }
    }
}

/// The bob at a point in the stride (`phase` counts strides): side to side once a stride, and
/// down at each footfall, just as the side-to-side is at its widest. Never above the eye.
pub fn bob_offset(phase: f32, amplitude: Vec2) -> Vec3 {
    let angle = phase * TAU;
    Vec3::new(
        amplitude.x * angle.sin(),
        amplitude.y * ((2.0 * angle).cos() - 1.0) / 2.0,
        0.0,
    )
}

/// How deep the landing dip goes for a landing at `fall_speed` m/s.
pub fn landing_dip_depth(fall_speed: f32) -> f32 {
    ((fall_speed - LANDING_DIP_MIN_SPEED) * LANDING_DIP_PER_SPEED).clamp(0.0, LANDING_DIP_MAX)
}

/// The landing dip's offset `elapsed` seconds after landing: sharply down, then eased back up.
pub fn landing_dip(elapsed: f32, depth: f32) -> f32 {
    let t = elapsed / LANDING_DIP_SECS;
    if !(0.0..1.0).contains(&t) {
        return 0.0;
    }
    -depth * (PI * t.sqrt()).sin()
}

/// Where the view model trails to while looking around at `look_rate` radians per second
/// (x turning right, y looking down): left behind a turn to the right, up behind a look down.
pub fn weapon_lag(look_rate: Vec2) -> Vec2 {
    (Vec2::new(-look_rate.x, look_rate.y) * WEAPON_SWAY_LAG).clamp_length_max(WEAPON_SWAY_LAG_MAX)
}

/// Moves `current` toward `target` by the share `rate` sets for a step of `dt` seconds, so it
/// eases the same at any frame rate.
pub fn ease<T>(current: T, target: T, rate: f32, dt: f32) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Mul<f32, Output = T>,
{
    current + (target - current) * (1.0 - (-rate * dt).exp())
}

/// The camera's sway, kept frame to frame.
#[derive(Component, Default)]
pub struct CameraSway {
    /// Strides taken, mod 1.
    phase: f32,
    /// Strides per second, eased.
    rate: f32,
    /// Bob amplitude, eased.
    amplitude: Vec2,
    was_grounded: bool,
    /// The last fall speed in the air, for the landing that ends it.
    fall_speed: f32,
    since_landing: f32,
    dip_depth: f32,
    /// The view model's trail behind mouse look, eased.
    lag: Vec2,
}

pub struct SwayPlugin;

impl Plugin for SwayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, add_sway).add_systems(
            Update,
            sway.after(lower_eye)
                .after(AnimateViewModel)
                .run_if(in_state(Pause::Running)),
        );
    }
}

fn add_sway(mut commands: Commands, cameras: Query<Entity, Added<PlayerCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert(CameraSway::default());
    }
}

/// Places the camera at the eye plus the sway, and adds the view model's sway on top of its own
/// animation.
fn sway(
    player: Single<(&Locomotion, &Loadout, Has<LastStand>), With<LocalPlayer>>,
    camera: Single<(&Eye, &mut Transform, &mut CameraSway), With<PlayerCamera>>,
    mut view_models: Query<&mut Transform, (With<ViewModel>, Without<PlayerCamera>)>,
    settings: Res<Settings>,
    mouse: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let (locomotion, loadout, last_stand) = player.into_inner();
    let (eye, mut transform, mut sway) = camera.into_inner();

    // None down the sights, and none at all with the Setting off.
    let weight = if settings.camera_sway { 1.0 - loadout.aim } else { 0.0 };

    // The stride: none in the air.
    let gait = if last_stand { Gait::Crawl } else { Gait::OnFoot };
    let grounded = locomotion.grounded();
    let speed = if grounded { locomotion.ground_speed() } else { 0.0 };
    sway.amplitude = ease(sway.amplitude, bob_amplitude(speed, gait) * weight, SWAY_EASE_RATE, dt);
    sway.rate = ease(sway.rate, stride_rate(speed, gait), SWAY_EASE_RATE, dt);
    sway.phase = (sway.phase + sway.rate * dt).fract();
    let bob = bob_offset(sway.phase, sway.amplitude);

    // The landing dip.
    if !grounded {
        sway.fall_speed = (-locomotion.vertical_speed()).max(0.0);
    } else if !sway.was_grounded {
        sway.dip_depth = landing_dip_depth(sway.fall_speed);
        sway.since_landing = 0.0;
    }
    sway.was_grounded = grounded;
    sway.since_landing += dt;
    let dip = landing_dip(sway.since_landing, sway.dip_depth) * weight;

    transform.translation = Vec3::new(bob.x, eye.height + bob.y + dip, 0.0);

    // The view model trails mouse look and bobs a little against the camera.
    let look_rate = if dt > 0.0 { settings.look_delta(mouse.delta) / dt } else { Vec2::ZERO };
    sway.lag = ease(sway.lag, weapon_lag(look_rate) * weight, WEAPON_SWAY_EASE_RATE, dt);
    let tilt = sway.lag / WEAPON_SWAY_LAG_MAX * WEAPON_SWAY_TILT_MAX;
    for mut model in &mut view_models {
        model.translation += bob_offset(sway.phase, sway.amplitude * WEAPON_BOB_SCALE) + sway.lag.extend(0.0);
        model.rotation = Quat::from_euler(EulerRot::YXZ, -tilt.x, tilt.y, 0.0) * model.rotation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bob `secs` into moving at a steady `speed`.
    fn bob_at(speed: f32, secs: f32, gait: Gait) -> Vec3 {
        bob_offset(stride_rate(speed, gait) * secs, bob_amplitude(speed, gait))
    }

    #[test]
    fn standing_still_there_is_no_bob() {
        for secs in [0.0, 0.13, 0.5, 2.7] {
            assert_eq!(bob_at(0.0, secs, Gait::OnFoot), Vec3::ZERO);
        }
        assert_eq!(stride_rate(0.0, Gait::OnFoot), 0.0);
    }

    #[test]
    fn the_bob_repeats_every_stride_and_never_lifts_the_eye() {
        let speed = PLAYER_WALK_SPEED;
        let stride_secs = 1.0 / stride_rate(speed, Gait::OnFoot);
        for i in 0..50 {
            let secs = i as f32 * 0.037;
            let now = bob_at(speed, secs, Gait::OnFoot);
            let next = bob_at(speed, secs + stride_secs, Gait::OnFoot);
            assert!(now.distance(next) < 1e-4);
            assert!(now.y <= 0.0);
            assert_eq!(now.z, 0.0);
        }
    }

    #[test]
    fn each_footfall_dips_at_the_widest_sway() {
        let amplitude = bob_amplitude(PLAYER_WALK_SPEED, Gait::OnFoot);
        let right = bob_offset(0.25, amplitude);
        let left = bob_offset(0.75, amplitude);
        assert!((right - Vec3::new(BOB_WALK_LATERAL, -BOB_WALK_VERTICAL, 0.0)).length() < 1e-5);
        assert!((left - Vec3::new(-BOB_WALK_LATERAL, -BOB_WALK_VERTICAL, 0.0)).length() < 1e-5);
        // Between footfalls the eye is back up, centred.
        assert!(bob_offset(0.5, amplitude).length() < 1e-5);
    }

    #[test]
    fn sprinting_bobs_harder_and_faster_than_walking() {
        let walk = bob_amplitude(PLAYER_WALK_SPEED, Gait::OnFoot);
        let sprint = bob_amplitude(PLAYER_SPRINT_SPEED, Gait::OnFoot);
        assert_eq!(walk, Vec2::new(BOB_WALK_LATERAL, BOB_WALK_VERTICAL));
        assert_eq!(sprint, Vec2::new(BOB_SPRINT_LATERAL, BOB_SPRINT_VERTICAL));
        assert!(sprint.x > walk.x && sprint.y > walk.y);
        assert!(stride_rate(PLAYER_SPRINT_SPEED, Gait::OnFoot) > stride_rate(PLAYER_WALK_SPEED, Gait::OnFoot));
    }

    #[test]
    fn the_bob_grows_smoothly_with_speed() {
        let mut last = Vec2::ZERO;
        for i in 1..=100 {
            let amplitude = bob_amplitude(PLAYER_SPRINT_SPEED * i as f32 / 100.0, Gait::OnFoot);
            assert!(amplitude.x >= last.x && amplitude.y >= last.y);
            assert!((amplitude - last).length() < 0.002);
            last = amplitude;
        }
    }

    #[test]
    fn the_crawl_keeps_a_slow_sway() {
        let crawl_speed = PLAYER_WALK_SPEED * LAST_STAND_CRAWL_MULT;
        let amplitude = bob_amplitude(crawl_speed, Gait::Crawl);
        assert_eq!(amplitude, Vec2::new(CRAWL_SWAY_LATERAL, CRAWL_SWAY_VERTICAL));
        assert!(stride_rate(crawl_speed, Gait::Crawl) < stride_rate(PLAYER_WALK_SPEED, Gait::OnFoot));
    }

    #[test]
    fn a_hard_landing_dips_deeper_up_to_a_limit() {
        assert_eq!(landing_dip_depth(LANDING_DIP_MIN_SPEED - 1.0), 0.0);
        // A jump lands at jump speed; the Balcony drop at about 12 m/s.
        let jump = landing_dip_depth(PLAYER_JUMP_SPEED);
        let drop = landing_dip_depth((2.0 * GRAVITY * 4.0).sqrt());
        assert!(0.0 < jump && jump < drop && drop <= LANDING_DIP_MAX);
        assert_eq!(landing_dip_depth(100.0), LANDING_DIP_MAX);
    }

    #[test]
    fn the_landing_dip_goes_down_and_comes_back() {
        let depth = 0.1;
        assert_eq!(landing_dip(0.0, depth), 0.0);
        assert!((landing_dip(LANDING_DIP_SECS / 4.0, depth) + depth).abs() < 1e-6);
        assert_eq!(landing_dip(LANDING_DIP_SECS, depth), 0.0);
        assert_eq!(landing_dip(LANDING_DIP_SECS * 2.0, depth), 0.0);
        for i in 1..20 {
            let dip = landing_dip(LANDING_DIP_SECS * i as f32 / 20.0, depth);
            assert!(-depth <= dip && dip < 0.0);
        }
    }

    #[test]
    fn the_weapon_trails_behind_the_look_up_to_a_limit() {
        assert_eq!(weapon_lag(Vec2::ZERO), Vec2::ZERO);
        let right = weapon_lag(Vec2::new(1.0, 0.0));
        assert!(right.x < 0.0 && right.y == 0.0);
        let down = weapon_lag(Vec2::new(0.0, 1.0));
        assert!(down.y > 0.0 && down.x == 0.0);
        assert!((weapon_lag(Vec2::new(500.0, -300.0)).length() - WEAPON_SWAY_LAG_MAX).abs() < 1e-6);
    }

    #[test]
    fn easing_is_the_same_at_any_frame_rate() {
        let once = ease(0.0, 1.0, SWAY_EASE_RATE, 0.02);
        let twice = ease(ease(0.0, 1.0, SWAY_EASE_RATE, 0.01), 1.0, SWAY_EASE_RATE, 0.01);
        assert!((once - twice).abs() < 1e-6);
        assert!(once > 0.0 && once < 1.0);
        assert!((ease(0.0, 1.0, SWAY_EASE_RATE, 10.0) - 1.0).abs() < 1e-6);
    }
}
