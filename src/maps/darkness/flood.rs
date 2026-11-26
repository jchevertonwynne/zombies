//! The Refinery's floodlights (#568, #510 §3). Each is a metal-halide spot on a mast, never shadowed, that is off until the Power. When the
//! Power comes on they light one by one, `FLOOD_GAP_SECS` apart, each with a `STUTTER_SECS` warm-up stutter: the plant's "the lights
//! come on" beat. Everything follows the replicated `Power` and a hash of the flood's position, so every machine sees the same beat (a late
//! joiner, already Powered when it first looks, sees the floods steady). Each flood's moment of coming on is a `FloodOn` message, for the
//! sound ticket's `BridgeClunk`.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::maps::darkness::lamp_flicker::position_hash;
use crate::maps::map::{Flood, Layout};
use crate::maps::map::MapId;
use crate::maps::power::Power;
use crate::maps::zone::ZoneId;
use crate::tuning::*;

/// How long after the Power each flood comes on after the one before it.
pub const FLOOD_GAP_SECS: f32 = 0.3;
/// How long a flood's warm-up stutter lasts once it is on.
pub const STUTTER_SECS: f32 = 1.2;
/// Each step of the stutter lasts this long.
const SLOT_SECS: f32 = 0.1;
/// What a dip in the stutter drops a flood to, as a fraction of full.
pub const DIP: f32 = 0.1;
/// A flood's lumens, range and cone (from straight along its aim to its edge, in radians), and its colour.
pub const FLOOD_LUMENS: f32 = 4_000_000.0;
pub const FLOOD_RANGE: f32 = 45.0;
pub const FLOOD_CONE: f32 = 0.45;
pub const FLOOD_COLOUR: Color = Color::srgb(0.92, 0.95, 1.0);
/// The emissive strength of a flood's lens at full.
const LENS_GLOW: f32 = 20.0;

/// A floodlight, the `index`th of its Map's `floods`. Not a `Lamp`: it never follows a Zone's Section, and `shadow_nearest_lamps` never
/// gives it shadows.
#[derive(Component, Debug, Clone)]
pub struct FloodLight {
    pub index: usize,
    pub zone: ZoneId,
    pub at: Vec3,
    glow: Handle<StandardMaterial>,
    /// Whether it was on last frame, so `FloodOn` is sent once.
    on: bool,
    applied: f32,
}

/// A flood has just come on, at `at` in `zone`: for the sound ticket's `BridgeClunk`.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct FloodOn {
    pub zone: ZoneId,
    pub at: Vec3,
}

/// When the Power came on, as this machine saw it.
#[derive(Resource, Default, Debug)]
pub struct FloodBeat {
    /// The game time the Power was seen to come on, backdated past the whole beat for a machine that joined already Powered.
    began: Option<f32>,
    /// Whether this machine has seen the Power off, so a Power it then sees on is a beat to play and not one already over.
    seen_off: bool,
}

/// How lit the `index`th flood at `at` is, as a fraction of full, `since` seconds after the Power came on: off until its turn (`index` times
/// `FLOOD_GAP_SECS`), then a stutter of `STUTTER_SECS` (a rising level with two dips to `DIP`, their steps picked by a hash of `at`),
/// then steady at full.
pub fn level(index: usize, at: Vec3, since: f32) -> f32 {
    let t = since - index as f32 * FLOOD_GAP_SECS;
    if t < 0.0 {
        return 0.0;
    }
    if t >= STUTTER_SECS {
        return 1.0;
    }
    let slot = (t / SLOT_SECS) as u32;
    // Two dips, at a step of 1 to 3 and a step of 6 to 9: the first step is always lit, and the last never dips.
    let hash = position_hash(at);
    if slot == 1 + hash % 3 || slot == 6 + (hash >> 8) % 4 {
        return DIP;
    }
    0.4 + 0.6 * t / STUTTER_SECS
}

/// The game time by which every flood of `count` is steady.
pub fn beat_secs(count: usize) -> f32 {
    count.saturating_sub(1) as f32 * FLOOD_GAP_SECS + STUTTER_SECS
}

/// Builds the Map's floods: a spot, a dark mast (a thin post) and a lens box on it, facing the aim.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, layout: &Layout) {
    if layout.floods.is_empty() {
        return;
    }
    // A plain lens box stands in for the mast until the Props ticket's `flood_mast`.
    let lens = meshes.add(Cuboid::new(0.9, 0.6, 0.12));
    let housing = meshes.add(Cuboid::new(1.0, 0.7, 0.3));
    let pole = meshes.add(Cuboid::new(0.25, 1.0, 0.25));
    let iron = materials.add(Color::srgb(0.05, 0.05, 0.055));
    for (index, &Flood { zone, at, aim }) in layout.floods.iter().enumerate() {
        let glow = materials.add(StandardMaterial {
            base_color: FLOOD_COLOUR,
            emissive: LinearRgba::BLACK,
            ..default()
        });
        let height = at.y - zone.floor();
        commands.spawn((
            crate::state::InRun,
            FloodLight { index, zone, at, glow: glow.clone(), on: false, applied: f32::NAN },
            SpotLight {
                color: FLOOD_COLOUR,
                intensity: 0.0,
                range: FLOOD_RANGE,
                outer_angle: FLOOD_CONE,
                inner_angle: FLOOD_CONE * 0.7,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(at).looking_at(aim, Vec3::Y),
            children![
                (Mesh3d(housing.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, 0.0, 0.2), NotShadowCaster),
                (Mesh3d(lens.clone()), MeshMaterial3d(glow), Transform::from_xyz(0.0, 0.0, -0.02), NotShadowCaster),
            ],
        ));
        commands.spawn((
            crate::state::InRun,
            Mesh3d(pole.clone()),
            MeshMaterial3d(iron.clone()),
            Transform::from_translation(Vec3::new(at.x, zone.floor() + (height - 0.5) / 2.0, at.z)).with_scale(Vec3::new(1.0, height - 0.5, 1.0)),
            NotShadowCaster,
        ));
    }
}

/// The floods follow the Power: off until it is on, then each lighting in its turn with its stutter.
pub fn light_floods(
    map: Res<MapId>,
    power: Option<Res<Power>>,
    time: Res<Time>,
    mut beat: ResMut<FloodBeat>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    mut on: MessageWriter<FloodOn>,
    mut floods: Query<(&mut FloodLight, &mut SpotLight)>,
) {
    let layout = map.layout();
    let powered = power.is_some_and(|power| power.is_on());
    let now = time.elapsed_secs();
    if !powered {
        beat.began = None;
        beat.seen_off = true;
    } else if beat.began.is_none() {
        beat.began = Some(if beat.seen_off { now } else { now - beat_secs(layout.floods.len()) - 1.0 });
    }
    let since = beat.began.map(|began| now - began);
    for (mut flood, mut light) in &mut floods {
        let shown = since.map_or(0.0, |since| level(flood.index, flood.at, since));
        let intensity = FLOOD_LUMENS * layout.look.lamps * shown;
        if light.intensity != intensity {
            light.intensity = intensity;
        }
        if flood.applied != shown {
            flood.applied = shown;
            if let Some(materials) = materials.as_mut()
                && let Some(mut material) = materials.get_mut(&flood.glow)
            {
                material.emissive = LinearRgba::from(FLOOD_COLOUR) * LENS_GLOW * shown;
            }
        }
        let lit = shown > 0.0;
        if lit && !flood.on && since.is_some_and(|since| since < beat_secs(layout.floods.len())) {
            on.write(FloodOn { zone: flood.zone, at: flood.at });
        }
        flood.on = lit;
    }
}
