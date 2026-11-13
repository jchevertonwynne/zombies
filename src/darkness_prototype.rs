//! PROTOTYPE — throwaway, never merged. Answers "Decide how Darkness looks and what it costs"
//! (jchevertonwynne/zombies#283) on the Greybox, standing in for the Dam's dark Sections.
//!
//! Four variants, cycled in game with `]`:
//! 0. Off: today's look (ambient 120, moon, every lamp on).
//! 1. Lamps out: no ambient, no moon; a Zone's lamps are its only light, so an unlit Zone is
//!    black. Weapon Lights are spot lights without shadows. Black distance fog.
//! 2. Lamps out + shadows + bloom: as 1, but the local Weapon Light casts shadows and the camera
//!    has HDR bloom, so emissives (eyes, Box, Perks) halo.
//! 3. Grade by where you stand: the world keeps its ambient and moon; only the local camera goes
//!    dark (its own ambient and close black fog) while the local Player stands in an unlit Zone.
//!
//! Other keys: `\` lights or darkens the Zone the local Player stands in (a 1.2 s flickering
//! fade, as a Claim would), `[` toggles faintly glowing chalk.
//! Env: `ZOMBIES_DARK=<0..3>` picks the starting variant, `ZOMBIES_DARK_LIT=none` starts with
//! every Zone unlit, `=all` every Zone lit (default: only the Start Room is lit, as the Dam's crest is).

use std::collections::HashMap;

use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::camera::Hdr;

use crate::enemies::nav::zone_at;
use crate::maps::zone::ZoneId;
use crate::players::player::{LocalPlayer, Player, PlayerCamera};
use crate::state::GameState;

const NAMES: [&str; 4] = ["0 Off", "1 Lamps out", "2 Lamps out + shadows + bloom", "3 Grade by where you stand"];
const FADE_SECS: f32 = 1.2;
const EYE: f32 = 1.6;

/// A lamp as the Map built it: its Zone and its full intensity.
#[derive(Component)]
pub struct Lamp {
    pub zone: ZoneId,
    pub full: f32,
}

/// The chalk outlines' shared material.
#[derive(Resource)]
pub struct ChalkMat(pub Handle<StandardMaterial>);

#[derive(Component)]
struct WeaponLight;

#[derive(Component)]
struct Overlay;

#[derive(Resource)]
struct Dark {
    variant: usize,
    /// How lit each Zone is, 0 to 1, and whether it's heading lit.
    zones: HashMap<ZoneId, (f32, bool)>,
    glow_chalk: bool,
    start_lit: bool,
    all_lit: bool,
}

pub struct DarknessPrototypePlugin;

impl Plugin for DarknessPrototypePlugin {
    fn build(&self, app: &mut App) {
        let variant = std::env::var("ZOMBIES_DARK").ok().and_then(|v| v.parse().ok()).unwrap_or(1usize) % 4;
        let start_lit = std::env::var("ZOMBIES_DARK_LIT").map(|v| v != "none").unwrap_or(true);
        let all_lit = std::env::var("ZOMBIES_DARK_LIT").is_ok_and(|v| v == "all");
        app.insert_resource(Dark {
            all_lit,
            variant,
            zones: HashMap::new(),
            glow_chalk: false,
            start_lit,
        })
        .add_systems(OnEnter(GameState::Playing), (reset, spawn_overlay))
        .add_systems(
            Update,
            (keys, give_weapon_lights, fade, apply, overlay).chain().run_if(in_state(GameState::Playing)),
        );
    }
}

fn reset(mut dark: ResMut<Dark>) {
    let (start_lit, all_lit) = (dark.start_lit, dark.all_lit);
    dark.zones = ZoneId::ALL
        .into_iter()
        .map(|zone| {
            let lit = all_lit || (start_lit && zone == ZoneId::StartRoom);
            (zone, (if lit { 1.0 } else { 0.0 }, lit))
        })
        .collect();
}

fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        crate::state::InRun,
        Overlay,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(1.0, 1.0, 0.4)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
}

fn local_zone(player: &Option<Single<&GlobalTransform, With<LocalPlayer>>>) -> Option<ZoneId> {
    player.as_ref().and_then(|p| zone_at(p.translation() + Vec3::Y * 0.5))
}

fn keys(
    input: Res<ButtonInput<KeyCode>>,
    mut dark: ResMut<Dark>,
    player: Option<Single<&GlobalTransform, With<LocalPlayer>>>,
) {
    if input.just_pressed(KeyCode::BracketRight) {
        dark.variant = (dark.variant + 1) % 4;
    }
    if input.just_pressed(KeyCode::BracketLeft) {
        dark.glow_chalk = !dark.glow_chalk;
    }
    if input.just_pressed(KeyCode::Backslash) {
        if let Some(zone) = local_zone(&player) {
            if let Some(entry) = dark.zones.get_mut(&zone) {
                entry.1 = !entry.1;
            }
        }
    }
}

/// A narrow spot on every Player: on the local camera, and at eye height on every other body.
fn give_weapon_lights(
    mut commands: Commands,
    cameras: Query<Entity, (With<PlayerCamera>, Without<WeaponLight>)>,
    bodies: Query<Entity, (With<Player>, Without<LocalPlayer>, Without<WeaponLight>)>,
    lit: Query<&ChildOf, With<WeaponLight>>,
) {
    let has = |e: Entity| lit.iter().any(|c| c.parent() == e);
    for camera in &cameras {
        if !has(camera) {
            commands.spawn((WeaponLight, spot(), Transform::from_xyz(0.25, -0.2, 0.0), ChildOf(camera)));
        }
    }
    for body in &bodies {
        if !has(body) {
            commands.spawn((
                WeaponLight,
                spot(),
                Transform::from_xyz(0.25, EYE - 0.2, -0.3).looking_to(Vec3::new(0.0, -0.15, -1.0), Vec3::Y),
                ChildOf(body),
            ));
        }
    }
}

fn spot() -> SpotLight {
    SpotLight {
        color: Color::srgb(1.0, 0.95, 0.85),
        intensity: 0.0,
        range: 30.0,
        radius: 0.05,
        inner_angle: 0.12,
        outer_angle: 0.30,
        shadow_maps_enabled: false,
        ..default()
    }
}

fn fade(time: Res<Time>, mut dark: ResMut<Dark>) {
    let step = time.delta_secs() / FADE_SECS;
    for (level, heading_lit) in dark.zones.values_mut() {
        *level = if *heading_lit { (*level + step).min(1.0) } else { (*level - step * 3.0).max(0.0) };
    }
}

fn apply(
    mut commands: Commands,
    time: Res<Time>,
    dark: Res<Dark>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lamps: Query<(&Lamp, &mut PointLight)>,
    mut moon: Query<&mut DirectionalLight>,
    mut spots: Query<(&mut SpotLight, &ChildOf)>,
    local_camera: Query<Entity, With<PlayerCamera>>,
    mut cameras: Query<(Entity, &mut DistanceFog, Has<Bloom>), With<PlayerCamera>>,
    player: Option<Single<&GlobalTransform, With<LocalPlayer>>>,
    chalk: Option<Res<ChalkMat>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let v = dark.variant;
    let lamps_out = v != 0;
    let world_dark = v == 1 || v == 2;
    ambient.brightness = if world_dark { 0.0 } else { 120.0 };
    for mut moon in &mut moon {
        moon.illuminance = if world_dark { 0.0 } else { 800.0 };
    }
    let t = time.elapsed_secs();
    for (lamp, mut light) in &mut lamps {
        let (level, heading_lit) = dark.zones.get(&lamp.zone).copied().unwrap_or((1.0, true));
        let level = if lamps_out { level } else { 1.0 };
        // Flicker while coming on, as a Claimed Generator spins up.
        let flicker = if heading_lit && level < 1.0 && (t * 17.0 + lamp.full).sin() < -0.3 { 0.15 } else { 1.0 };
        light.intensity = lamp.full * level * flicker;
    }
    let local_cam = local_camera.iter().next();
    for (mut light, parent) in &mut spots {
        light.intensity = if v == 0 { 0.0 } else { 900_000.0 };
        light.shadow_maps_enabled = v == 2 && Some(parent.parent()) == local_cam;
    }
    let here_dark = local_zone(&player)
        .and_then(|z| dark.zones.get(&z))
        .is_some_and(|(level, _)| *level < 0.5);
    for (camera, mut fog, has_bloom) in &mut cameras {
        let (start, end) = match v {
            0 => (10.0, 45.0),
            3 if here_dark => (2.0, 14.0),
            _ => (8.0, 40.0),
        };
        fog.color = Color::srgb(0.01, 0.01, 0.02);
        fog.falloff = FogFalloff::Linear { start, end };
        let mut e = commands.entity(camera);
        if v == 3 && here_dark {
            e.insert(AmbientLight {
                color: Color::srgb(0.6, 0.65, 0.8),
                brightness: 3.0,
                ..default()
            });
        } else {
            e.remove::<AmbientLight>();
        }
        match (v == 2, has_bloom) {
            (true, false) => {
                e.insert((Hdr, Bloom::NATURAL));
            }
            (false, true) => {
                e.remove::<(Bloom, Hdr)>();
            }
            _ => {}
        }
    }
    if let Some(chalk) = chalk {
        if let Some(mut mat) = materials.get_mut(&chalk.0) {
            let want = if dark.glow_chalk { LinearRgba::rgb(0.25, 0.25, 0.22) } else { LinearRgba::BLACK };
            if mat.emissive != want {
                mat.emissive = want;
            }
        }
    }
}

fn overlay(
    dark: Res<Dark>,
    player: Option<Single<&GlobalTransform, With<LocalPlayer>>>,
    time: Res<Time<Real>>,
    mut avg: Local<f32>,
    mut text: Query<&mut Text, With<Overlay>>,
) {
    let dt = time.delta_secs() * 1000.0;
    *avg = if *avg == 0.0 { dt } else { *avg * 0.95 + dt * 0.05 };
    let zone = local_zone(&player);
    let level = zone.and_then(|z| dark.zones.get(&z)).map(|l| l.0).unwrap_or(1.0);
    for mut text in &mut text {
        text.0 = format!(
            "PROTOTYPE darkness: {}\nzone {:?} lit {:.0}%  chalk glow {}\n{:.2} ms  ({:.0} fps)\n] variant   \\ light/darken zone   [ chalk",
            NAMES[dark.variant],
            zone,
            level * 100.0,
            if dark.glow_chalk { "on" } else { "off" },
            *avg,
            1000.0 / avg.max(0.001),
        );
    }
}
