//! The Archive's daylight (#535). Every Zone is underground, so the "sky" is four openings, all
//! faked so no light leaks through a ceiling: the Reading Room's lantern over the issue desk and
//! the three light-well gratings in its north wall. Each is an emissive cool panel, one unshadowed
//! cool spot, an unlit additive shaft under it (alpha 0.05) and 40 dust motes drifting in the shaft
//! on a deterministic hash. Everything is built from the Layout on every machine, so a client sees
//! the same shafts; the motes' phase comes from the local clock and is no gameplay fact. None of
//! it has a collider, and none of it needs an asset file.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::maps::darkness::lamp_flicker::position_hash;
use crate::state::InRun;

/// The daylight's colour: a cool, pale blue.
pub const DAYLIGHT: Color = Color::srgb(0.72, 0.8, 0.95);
/// How strong the lantern's glazing and a grating's panel glow.
const LANTERN_GLOW: f32 = 6.0;
const WELL_GLOW: f32 = 4.0;
/// The shaft's alpha.
const SHAFT_ALPHA: f32 = 0.05;
/// How many dust motes drift in each shaft, and how fast they sink along it, in m/s.
pub const MOTES: u32 = 40;
const DRIFT: f32 = 0.08;
/// The lantern's glazing is 6 x 6 m at this height, over the issue desk.
const LANTERN_AT: Vec3 = Vec3::new(0.0, 14.0, 24.0);
const LANTERN_SIZE: f32 = 6.0;
/// The Reading Room's floor, and the light wells' windows' centre over it.
const READING: f32 = 8.0;
const WELL_HEIGHT: f32 = 2.0;
/// How far in front of its window a well's spot hangs and how far behind it its panel stands.
const WELL_SPOT_OFF: f32 = 0.2;
const WELL_PANEL_OFF: f32 = 0.3;

/// A shaft of daylight: from `origin` along `dir` for `length`, a square section `r0` either side
/// of its axis at the start, `r1` at the end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shaft {
    pub origin: Vec3,
    pub dir: Vec3,
    pub length: f32,
    pub r0: f32,
    pub r1: f32,
}

impl Shaft {
    /// The axes across the shaft.
    fn axes(&self) -> (Vec3, Vec3) {
        let right = if self.dir.dot(Vec3::Y).abs() > 0.99 { Vec3::X } else { self.dir.cross(Vec3::Y).normalize() };
        (right, self.dir.cross(right).normalize())
    }

    /// Where the mote seeded `seed` is at `now` seconds: sinking along the shaft, wrapping at its
    /// end, and wobbling a little. The same on every machine for the same clock.
    pub fn mote_at(&self, seed: u32, now: f32) -> Vec3 {
        let unit = |k: u32| (position_hash(Vec3::new(seed as f32, k as f32, 7.5)) >> 8) as f32 / (1u32 << 24) as f32;
        let (u0, a, b, phase) = (unit(0), unit(1) * 2.0 - 1.0, unit(2) * 2.0 - 1.0, unit(3) * std::f32::consts::TAU);
        let along = (u0 + now * DRIFT / self.length).fract();
        let reach = (self.r0 + (self.r1 - self.r0) * along) * 0.85;
        let (right, up) = self.axes();
        let wobble = |speed: f32| 0.1 * (now * speed + phase).sin();
        self.origin + self.dir * (along * self.length) + right * (a * reach + wobble(0.31)) + up * (b * reach + wobble(0.23))
    }
}

/// A dust mote in a shaft.
#[derive(Component, Debug, Clone, Copy)]
pub struct Mote {
    shaft: Shaft,
    seed: u32,
}

/// The four openings' shafts: the lantern's, then the three gratings'.
pub fn shafts() -> [Shaft; 4] {
    let lantern = Shaft { origin: LANTERN_AT - Vec3::Y * 0.05, dir: Vec3::NEG_Y, length: 4.3, r0: LANTERN_SIZE / 2.0, r1: LANTERN_SIZE / 2.0 - 0.4 };
    let well = |at: Vec3| Shaft { origin: at, dir: well_dir(), length: 3.9, r0: 0.7, r1: 1.8 };
    let [a, b, c] = wells().map(well);
    [lantern, a, b, c]
}

/// The way a well's light falls: into the room (towards -z), 30 degrees below level.
fn well_dir() -> Vec3 {
    Vec3::new(0.0, -0.5, -(0.75f32).sqrt())
}

/// Where each grating's spot hangs: in front of its window, at the window's middle.
fn wells() -> [Vec3; 3] {
    let windows = crate::maps::map::WINDOWS;
    let mut out = [Vec3::ZERO; 3];
    let mut found = 0;
    for (zone, at, _) in windows {
        if zone == crate::maps::zone::ZoneId::ReadingRoom && found < 3 {
            out[found] = Vec3::new(at.x, READING + WELL_HEIGHT, at.z - WELL_SPOT_OFF);
            found += 1;
        }
    }
    out
}

/// The small cool probe under the lantern, from the desk to the glazing: its low and high corners,
/// and its colour.
pub fn lantern_probe() -> (Vec3, Vec3, Color) {
    let half = Vec3::new(LANTERN_SIZE / 2.0, 0.0, LANTERN_SIZE / 2.0);
    (Vec3::new(LANTERN_AT.x, READING + 1.6, LANTERN_AT.z) - half, LANTERN_AT + half, DAYLIGHT)
}

/// How bright the lantern probe is next to a Zone's.
pub const LANTERN_PROBE_FACTOR: f32 = 0.6;

/// An open-ended square frustum along local -Z.
fn shaft_mesh(shaft: &Shaft) -> Mesh {
    let corner = |r: f32, z: f32, i: usize| {
        let (sx, sy) = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)][i];
        [sx * r, sy * r, -z]
    };
    let mut positions: Vec<[f32; 3]> = (0..4).map(|i| corner(shaft.r0, 0.0, i)).collect();
    positions.extend((0..4).map(|i| corner(shaft.r1, shaft.length, i)));
    let mut indices = Vec::new();
    for i in 0..4u32 {
        let j = (i + 1) % 4;
        indices.extend([i, j, 4 + j, i, 4 + j, 4 + i]);
    }
    let count = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
        .with_inserted_indices(Indices::U32(indices))
}

/// Builds the four openings: panel, spot, shaft and motes.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    let glow = |strength: f32, materials: &mut Assets<StandardMaterial>| {
        materials.add(StandardMaterial { base_color: DAYLIGHT, emissive: LinearRgba::from(DAYLIGHT) * strength, ..default() })
    };
    let shaft_material = materials.add(StandardMaterial {
        base_color: DAYLIGHT.with_alpha(SHAFT_ALPHA),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        cull_mode: None,
        double_sided: true,
        ..default()
    });
    let mote_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.92, 0.75, 0.7),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        ..default()
    });
    let mote_mesh = meshes.add(Sphere::new(0.015).mesh().ico(1).expect("a small sphere"));
    let [lantern, wells @ ..] = shafts();

    // The lantern: glazing in the ceiling and a spot on the desk.
    commands.spawn((
        InRun,
        Mesh3d(meshes.add(Cuboid::new(LANTERN_SIZE, 0.04, LANTERN_SIZE))),
        MeshMaterial3d(glow(LANTERN_GLOW, materials)),
        Transform::from_translation(LANTERN_AT - Vec3::Y * 0.03),
        NotShadowCaster,
    ));
    commands.spawn((
        InRun,
        SpotLight { color: DAYLIGHT, intensity: 600_000.0, range: 9.0, outer_angle: 0.35, inner_angle: 0.2, shadow_maps_enabled: false, ..default() },
        Transform::from_translation(LANTERN_AT - Vec3::Y * 0.5).looking_to(Vec3::NEG_Y, Vec3::X),
    ));
    // The gratings: a panel behind the boards and a spot into the room.
    let panel = meshes.add(Cuboid::new(super::super::WINDOW_WIDTH - 0.2, crate::tuning::WINDOW_HEIGHT - 0.2, 0.04));
    let panel_material = glow(WELL_GLOW, materials);
    for at in wells.iter().map(|shaft| shaft.origin) {
        commands.spawn((
            InRun,
            Mesh3d(panel.clone()),
            MeshMaterial3d(panel_material.clone()),
            Transform::from_translation(at + Vec3::Z * (WELL_SPOT_OFF + WELL_PANEL_OFF)),
            NotShadowCaster,
        ));
        commands.spawn((
            InRun,
            SpotLight { color: DAYLIGHT, intensity: 250_000.0, range: 8.0, outer_angle: 0.45, inner_angle: 0.25, shadow_maps_enabled: false, ..default() },
            Transform::from_translation(at).looking_to(well_dir(), Vec3::Y),
        ));
    }
    // A shaft under each, with its motes.
    for (index, shaft) in [lantern].into_iter().chain(wells).enumerate() {
        let hint = if shaft.dir.dot(Vec3::Y).abs() > 0.99 { Vec3::X } else { Vec3::Y };
        commands.spawn((
            InRun,
            Mesh3d(meshes.add(shaft_mesh(&shaft))),
            MeshMaterial3d(shaft_material.clone()),
            Transform::from_translation(shaft.origin).looking_to(shaft.dir, hint),
            NotShadowCaster,
        ));
        for i in 0..MOTES {
            let seed = index as u32 * MOTES + i;
            commands.spawn((
                InRun,
                Mote { shaft, seed },
                Mesh3d(mote_mesh.clone()),
                MeshMaterial3d(mote_material.clone()),
                Transform::from_translation(shaft.mote_at(seed, 0.0)),
                NotShadowCaster,
            ));
        }
    }
}

/// The motes drift.
pub fn drift_motes(time: Res<Time>, mut motes: Query<(&Mote, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (mote, mut at) in &mut motes {
        at.translation = mote.shaft.mote_at(mote.seed, now);
    }
}
