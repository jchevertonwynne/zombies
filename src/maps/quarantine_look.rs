//! How the Quarantine looks (#524): a Quarantine Door's Leaves, its frame and hazard header, the
//! beacon over its outside face and the repeater over its inside face, the Crash Bar on its inside
//! face, and the red bulkheads of a Sealed Wing. (The Wing's lamps flicker through `LampFlicker`, and
//! the text for Players inside the Wing is the HUD's.)
//!
//! Everything is drawn from what every machine already has: the `Quarantine` stage, each Piece's
//! phase and the Map's Layout. Nothing here is sent, so a client and a late joiner see the Host's
//! stages. It is all code-built, so headless runs need no asset files; the Leaves' paint, kick plate
//! and biohazard signs wear their files once loaded.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::map::decals::{Decal, DecalPiece};
use super::map::sanatorium::scenery::{Bulkhead, bulkhead_glow};
use super::map::textures::{Material, Shape, Textured, Tint, box_uv_mesh};
use super::quarantine::{CrashBar, Quarantine, Stage, WingId, wing_of_zone};
use super::shifting::{Leaf, PieceId, PiecePhase, PieceState};
#[cfg(test)]
use super::shifting::pieces;
use crate::maps::map::MapId;
use crate::state::{InRun, world_runs};
use crate::tuning::CHALK_GLOW;

// ---- Beacons ----

/// What colour a beacon's lens shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lens {
    Off,
    Red,
    Green,
}

/// The one extra light a beacon casts, if any.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeaconLight {
    None,
    /// An unshadowed spot that turns with the reflector.
    Spot { lumens: f32, range: f32 },
    /// An unshadowed point light.
    Point { lumens: f32, range: f32 },
}

/// How a Wing's beacon looks at one stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeaconLook {
    pub lens: Lens,
    /// How many times the lens's colour its emissive glows at, at its brightest.
    pub emissive: f32,
    /// Turns of the reflector (and its spot) a second.
    pub spin: f32,
    /// Whether the glow and light pulse at `PULSE_HZ`.
    pub pulses: bool,
    pub light: BeaconLight,
}

pub const BEACON_SPIN_REV_PER_SEC: f32 = 1.5;
pub const PULSE_HZ: f32 = 1.0;
const WARNING_SPOT: BeaconLight = BeaconLight::Spot { lumens: 150_000.0, range: 10.0 };
const POINT: BeaconLight = BeaconLight::Point { lumens: 30_000.0, range: 6.0 };

/// A beacon of the Wing that is out, at `stage`. A dark lens at All Clear and Dormant. The beacon of
/// a Wing that is not out is always dark (`beacon_look_by_stage` asks for `out` = true).
pub fn beacon_look(stage: Stage, out: bool) -> BeaconLook {
    let dark = BeaconLook { lens: Lens::Off, emissive: 0.0, spin: 0.0, pulses: false, light: BeaconLight::None };
    if !out {
        return dark;
    }
    match stage {
        Stage::Dormant | Stage::AllClear => dark,
        Stage::Warning | Stage::Sealing => BeaconLook { lens: Lens::Red, emissive: 20.0, spin: BEACON_SPIN_REV_PER_SEC, pulses: false, light: WARNING_SPOT },
        Stage::Sealed => BeaconLook { lens: Lens::Red, emissive: 8.0, spin: 0.0, pulses: false, light: POINT },
        Stage::Release | Stage::Opening => BeaconLook { lens: Lens::Green, emissive: 15.0, spin: 0.0, pulses: true, light: POINT },
    }
}

/// How bright a pulsing beacon is `now` seconds in, from 0.3 to 1.
pub fn pulse(now: f32) -> f32 {
    0.65 + 0.35 * (TAU * PULSE_HZ * now).sin()
}

#[cfg(test)]
/// How many extra lights the beacons of every doorway of a Map cast at `stage` while `wing` is out:
/// only an outside beacon of the Wing that is out has one.
pub fn beacon_lights(map: MapId, stage: Stage, wing: Option<WingId>) -> usize {
    pieces(map.layout())
        .filter(|(_, piece)| piece.wing().is_some() && piece.wing() == wing)
        .filter(|_| beacon_look(stage, true).light != BeaconLight::None)
        .count()
}

/// How a Crash Bar's "PUSH" strip glows at `stage`: at `CHALK_GLOW` only while its Wing is Sealed.
pub fn crash_strip_glow(stage: Stage, wing_out: bool) -> f32 {
    if wing_out && stage == Stage::Sealed { CHALK_GLOW } else { 0.0 }
}

// ---- Parts ----

/// A beacon over a doorway: its Wing, and whether it is the outside beacon (with the light) or the
/// inside repeater (emissive only).
#[derive(Component, Debug, Clone)]
pub struct Beacon {
    pub wing: WingId,
    pub lens: Handle<StandardMaterial>,
}

/// The reflector that turns with a beacon's spot light.
#[derive(Component)]
struct Reflector(Entity);

/// One of an outside beacon's two lights, hidden unless it is the stage's.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BeaconBulb {
    pub beacon: Entity,
    pub spot: bool,
}

/// A Crash Bar's bar and brackets, drawn while its door stands shut; its strip glows while Sealed.
#[derive(Component, Debug, Clone, Copy)]
pub struct CrashBarLook {
    pub piece: PieceId,
}

#[derive(Component, Clone)]
pub struct CrashStrip {
    wing: WingId,
    material: Handle<StandardMaterial>,
}

const HAZARD_YELLOW: Color = Color::srgb(0.9, 0.7, 0.05);
const HAZARD_BLACK: Color = Color::srgb(0.03, 0.03, 0.03);
const FRAME_STEEL: Color = Color::srgb(0.36, 0.42, 0.38);
const LEAF_PAINT: Color = Color::srgb(0.66, 0.72, 0.6);
const BAR_RED: Color = Color::srgb(0.7, 0.04, 0.03);
const LENS_DARK: Color = Color::srgb(0.1, 0.09, 0.09);
const STRIP_GREEN: Color = Color::srgb(0.25, 1.0, 0.4);
const RED: Color = Color::srgb(1.0, 0.08, 0.05);
const GREEN: Color = Color::srgb(0.15, 1.0, 0.3);

/// How far the doorway's wall faces stand from its centre, half of `WALL_THICKNESS`.
const FACE: f32 = 0.2;
/// How far a Leaf's inside face stands out of the doorway's centre when shut.
const LEAF_FACE: f32 = FACE;
/// The doorway's clear height, and the frame and header over it.
const DOORWAY_HEIGHT: f32 = 3.0;
const LINTEL: f32 = 0.12;
const HEADER: f32 = 0.3;
/// A beacon sits this far over the header's top.
const BEACON_OVER: f32 = 0.25;
/// The stripes of a hazard header.
const STRIPES: usize = 10;

/// A Leaf's dressing, in the Leaf's own frame (x along it, y up from the hinge's floor, z through it).
const SIGN_HEIGHT: f32 = 1.6;
const SIGN_SIZE: f32 = 0.35;
const VISION_PANEL: Vec2 = Vec2::new(0.25, 0.6);
const VISION_HEIGHT: f32 = 1.45;
const KICK_PLATE: f32 = 0.25;

/// Dresses a Quarantine Door's Leaf, drawn as the slab `leaf` is: the painted deck, a dark
/// wired-glass vision panel, a kick plate and a biohazard sign on both faces. The deck, kick plate and
/// sign wear their files once loaded, and keep the plain paint without them.
pub fn dress_leaf(parent: &mut ChildSpawnerCommands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, leaf: &Leaf) {
    let (size, centre) = (leaf.size, leaf.offset);
    let tile = Material::PaintedMetalShutter.tile();
    let paint = materials.add(LEAF_PAINT);
    parent.spawn((
        Name::new("Leaf deck"),
        Mesh3d(meshes.add(box_uv_mesh(size, centre, Quat::IDENTITY, tile))),
        MeshMaterial3d(paint),
        Textured { material: Material::PaintedMetalShutter, shape: Shape::Baked },
        Tint(Material::PaintedMetalShutter.tint()),
        Transform::from_translation(centre),
    ));
    // The vision panel: a dark pane through both faces.
    let glass = materials.add(StandardMaterial { base_color: Color::srgb(0.04, 0.07, 0.06), perceptual_roughness: 0.15, metallic: 0.4, ..default() });
    parent.spawn((
        Name::new("Vision panel"),
        Mesh3d(meshes.add(Cuboid::new(VISION_PANEL.x, VISION_PANEL.y, size.z + 0.02))),
        MeshMaterial3d(glass),
        Transform::from_xyz(centre.x, VISION_HEIGHT, 0.0),
    ));
    // The kick plate: rusty metal along the foot.
    let plate = Vec3::new(size.x, KICK_PLATE, size.z + 0.02);
    let plate_at = Vec3::new(centre.x, KICK_PLATE / 2.0, 0.0);
    let rust = materials.add(Color::srgb(0.45, 0.3, 0.2));
    parent.spawn((
        Name::new("Kick plate"),
        Mesh3d(meshes.add(box_uv_mesh(plate, plate_at, Quat::IDENTITY, Material::RustyMetal02.tile()))),
        MeshMaterial3d(rust),
        Textured { material: Material::RustyMetal02, shape: Shape::Baked },
        Transform::from_translation(plate_at),
    ));
    // A biohazard sign on each face, facing out of it.
    for face in [1.0, -1.0] {
        parent.spawn((
            Name::new("Biohazard sign"),
            DecalPiece(Decal::Biohazard),
            Transform::from_xyz(centre.x, SIGN_HEIGHT, face * (size.z / 2.0 + 0.012))
                .with_rotation(Quat::from_rotation_y(if face > 0.0 { 0.0 } else { PI }))
                .with_scale(Vec3::splat(SIGN_SIZE)),
        ));
    }
}

/// The assets a doorway's look is drawn from.
struct Kit<'a> {
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<StandardMaterial>,
}

/// A box part of a doorway's look, in its root's frame.
fn part(commands: &mut Commands, kit: &mut Kit, root: Entity, name: &'static str, size: Vec3, at: Vec3, colour: Color) {
    commands.spawn((
        Name::new(name),
        Mesh3d(kit.meshes.add(Cuboid::from_size(size))),
        MeshMaterial3d(kit.materials.add(StandardMaterial { base_color: colour, perceptual_roughness: 0.8, ..default() })),
        Transform::from_translation(at),
        ChildOf(root),
    ));
}

/// Spawns every Quarantine Door's look from its Crash Bar's doorway, on every machine alike.
fn spawn_look(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, bars: Query<&CrashBar, Added<CrashBar>>, map: Res<MapId>) {
    let layout = map.layout();
    for bar in &bars {
        let Some(wing) = layout.shifting.get(usize::from(bar.piece.0)).and_then(|piece| piece.wing()) else { continue };
        let doorway = bar.doorway;
        // The root: its +z looks out of the Wing, so a face is at z = +-FACE.
        let root = commands
            .spawn((
                InRun,
                Name::new("Quarantine doorway"),
                Transform::from_translation(doorway.centre).looking_to(-doorway.out, Vec3::Y),
                Visibility::default(),
            ))
            .id();
        let mut kit = Kit { meshes: &mut meshes, materials: &mut materials };
        // The steel-channel frame in trim paint: two jambs and a lintel, a little proud of the inside
        // face, flush with the outside one (the folded Leaves lie there).
        let jamb_x = 1.2 + 0.06;
        for side in [-1.0, 1.0] {
            part(&mut commands, &mut kit, root, "Frame jamb", Vec3::new(0.12, DOORWAY_HEIGHT, 0.5), Vec3::new(side * jamb_x, DOORWAY_HEIGHT / 2.0, -0.05), FRAME_STEEL);
        }
        part(&mut commands, &mut kit, root, "Frame lintel", Vec3::new(2.64, LINTEL, 0.5), Vec3::new(0.0, DOORWAY_HEIGHT + LINTEL / 2.0, -0.05), FRAME_STEEL);
        // The black-and-yellow hazard header over it, on both faces.
        let stripe = 2.64 / STRIPES as f32;
        let header_y = DOORWAY_HEIGHT + LINTEL + HEADER / 2.0;
        for i in 0..STRIPES {
            let colour = if i % 2 == 0 { HAZARD_YELLOW } else { HAZARD_BLACK };
            let x = -1.32 + stripe * (i as f32 + 0.5);
            part(&mut commands, &mut kit, root, "Hazard stripe", Vec3::new(stripe, HEADER, 0.5), Vec3::new(x, header_y, 0.0), colour);
        }
        // The beacon over the outside face, and the repeater over the inside one.
        let beacon_y = DOORWAY_HEIGHT + LINTEL + HEADER + BEACON_OVER;
        for outside in [true, false] {
            let face = if outside { 1.0 } else { -1.0 };
            let lens = kit.materials.add(StandardMaterial { base_color: LENS_DARK, perceptual_roughness: 0.3, ..default() });
            commands.spawn((
                    Name::new(if outside { "Beacon" } else { "Repeater" }),
                    Mesh3d(kit.meshes.add(Cylinder::new(0.15, 0.12))),
                    MeshMaterial3d(kit.materials.add(StandardMaterial { base_color: Color::srgb(0.07, 0.08, 0.08), perceptual_roughness: 0.6, metallic: 0.6, ..default() })),
                    Transform::from_xyz(0.0, beacon_y, face * 0.31).with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
                    ChildOf(root),
                ));
            let beacon = commands
                .spawn((
                    Name::new("Beacon lens"),
                    Beacon { wing, lens: lens.clone() },
                    Mesh3d(kit.meshes.add(Sphere::new(0.1))),
                    MeshMaterial3d(lens),
                    Transform::from_xyz(0.0, beacon_y, face * 0.4),
                    NotShadowCaster,
                    ChildOf(root),
                ))
                .id();
            if !outside {
                continue;
            }
            // The reflector turning with the spot: a small bright plate across the lens.
            let spinner = commands
                .spawn((
                    Name::new("Beacon reflector"),
                    Mesh3d(kit.meshes.add(Cuboid::new(0.02, 0.16, 0.06))),
                    MeshMaterial3d(kit.materials.add(StandardMaterial { base_color: Color::srgb(0.8, 0.8, 0.8), metallic: 0.9, perceptual_roughness: 0.2, ..default() })),
                    Transform::from_xyz(0.0, beacon_y, face * 0.4),
                    NotShadowCaster,
                    ChildOf(root),
                ))
                .id();
            commands.entity(beacon).insert(Reflector(spinner));
            // Both lights ride the reflector, so the spot turns with it; only the stage's is shown.
            commands.spawn((
                Name::new("Beacon spot"),
                BeaconBulb { beacon, spot: true },
                SpotLight { color: RED, intensity: 0.0, range: 10.0, outer_angle: 0.5, inner_angle: 0.2, shadow_maps_enabled: false, ..default() },
                Transform::from_xyz(0.0, 0.0, 0.12).looking_to(Vec3::Z, Vec3::Y),
                Visibility::Hidden,
                ChildOf(spinner),
            ));
            commands.spawn((
                Name::new("Beacon point"),
                BeaconBulb { beacon, spot: false },
                PointLight { color: RED, intensity: 0.0, range: 6.0, shadow_maps_enabled: false, ..default() },
                Transform::from_xyz(0.0, 0.0, 0.2),
                Visibility::Hidden,
                ChildOf(spinner),
            ));
        }
        // The Crash Bar on the inside face of the shut Leaves: a 1.0 m red bar at 1.0 m on two
        // brackets, and a green "PUSH" strip over it that glows only while Sealed.
        let on_door = commands
            .spawn((
                Name::new("Crash Bar look"),
                CrashBarLook { piece: bar.piece },
                Transform::default(),
                Visibility::Hidden,
                ChildOf(root),
            ))
            .id();
        part(&mut commands, &mut kit, on_door, "Crash Bar", Vec3::new(1.0, 0.05, 0.05), Vec3::new(0.0, 1.0, LEAF_FACE - 0.08), BAR_RED);
        for side in [-1.0, 1.0] {
            part(&mut commands, &mut kit, on_door, "Crash Bar bracket", Vec3::new(0.04, 0.04, 0.08), Vec3::new(side * 0.4, 1.0, LEAF_FACE - 0.04), BAR_RED);
        }
        let strip = kit.materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.3, 0.12), emissive: LinearRgba::BLACK, ..default() });
        commands.spawn((
            Name::new("Crash Bar PUSH strip"),
            CrashStrip { wing, material: strip.clone() },
            Mesh3d(kit.meshes.add(Cuboid::new(0.5, 0.08, 0.01))),
            MeshMaterial3d(strip),
            Transform::from_xyz(0.0, 1.2, LEAF_FACE - 0.005),
            NotShadowCaster,
            ChildOf(on_door),
        ));
    }
}

fn colour_of(lens: Lens) -> Color {
    match lens {
        Lens::Off => LENS_DARK,
        Lens::Red => RED,
        Lens::Green => GREEN,
    }
}

/// Each beacon follows the stage the machine shows: its lens, its reflector, and its lights.
fn light_beacons(
    quarantine: Res<Quarantine>,
    time: Res<Time>,
    beacons: Query<(Entity, &Beacon, Option<&Reflector>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transforms: Query<&mut Transform>,
    mut bulbs: Query<(&BeaconBulb, &mut Visibility, Option<&mut SpotLight>, Option<&mut PointLight>)>,
) {
    let now = time.elapsed_secs();
    for (entity, beacon, reflector) in &beacons {
        let look = beacon_look(quarantine.stage(), quarantine.wing() == Some(beacon.wing));
        let glow = if look.pulses { pulse(now) } else { 1.0 };
        let colour = colour_of(look.lens);
        let emissive = if look.lens == Lens::Off { LinearRgba::BLACK } else { LinearRgba::from(colour) * look.emissive * glow };
        if materials.get(&beacon.lens).is_some_and(|material| material.emissive != emissive || material.base_color != colour)
            && let Some(mut material) = materials.get_mut(&beacon.lens)
        {
            material.emissive = emissive;
            material.base_color = colour;
        }
        let Some(&Reflector(spinner)) = reflector else { continue };
        if look.spin > 0.0
            && let Ok(mut transform) = transforms.get_mut(spinner)
        {
            transform.rotation = Quat::from_rotation_y(now * look.spin * TAU);
        }
        for (bulb, mut visibility, spot, point) in &mut bulbs {
            if bulb.beacon != entity {
                continue;
            }
            let shown = matches!((look.light, bulb.spot), (BeaconLight::Spot { .. }, true) | (BeaconLight::Point { .. }, false));
            visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
            let (lumens, range) = match look.light {
                BeaconLight::Spot { lumens, range } | BeaconLight::Point { lumens, range } => (lumens * glow, range),
                BeaconLight::None => (0.0, 0.0),
            };
            if shown {
                if let Some(mut light) = spot
                    && (light.intensity != lumens || light.color != colour || light.range != range)
                {
                    light.intensity = lumens;
                    light.color = colour;
                    light.range = range;
                }
                if let Some(mut light) = point
                    && (light.intensity != lumens || light.color != colour || light.range != range)
                {
                    light.intensity = lumens;
                    light.color = colour;
                    light.range = range;
                }
            }
        }
    }
}

/// Each Crash Bar is drawn while its door stands shut, and its strip glows only while its Wing is Sealed.
fn light_crash_bars(
    quarantine: Res<Quarantine>,
    states: Query<&PieceState>,
    mut bars: Query<(&CrashBarLook, &mut Visibility)>,
    strips: Query<&CrashStrip>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (bar, mut visibility) in &mut bars {
        let shut = states.iter().any(|state| state.id() == bar.piece && state.phase() == PiecePhase::Rest(1));
        visibility.set_if_neq(if shut { Visibility::Inherited } else { Visibility::Hidden });
    }
    for strip in &strips {
        let glow = crash_strip_glow(quarantine.stage(), quarantine.wing() == Some(strip.wing));
        let emissive = LinearRgba::from(STRIP_GREEN) * glow;
        if materials.get(&strip.material).is_some_and(|material| material.emissive != emissive)
            && let Some(mut material) = materials.get_mut(&strip.material)
        {
            material.emissive = emissive;
        }
    }
}

/// A Sealed Wing's bulkheads turn red.
fn light_bulkheads(quarantine: Res<Quarantine>, map: Res<MapId>, bulkheads: Query<(&Bulkhead, &MeshMaterial3d<StandardMaterial>)>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (&Bulkhead(zone), handle) in &bulkheads {
        let sealed = quarantine.stage() == Stage::Sealed && wing_of_zone(*map, zone).is_some() && wing_of_zone(*map, zone) == quarantine.wing();
        let (colour, strength) = bulkhead_glow(sealed);
        let emissive = LinearRgba::from(colour) * strength;
        if materials.get(&handle.0).is_some_and(|material| material.emissive != emissive)
            && let Some(mut material) = materials.get_mut(&handle.0)
        {
            material.base_color = colour;
            material.emissive = emissive;
        }
    }
}

pub struct QuarantineLookPlugin;

impl Plugin for QuarantineLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (spawn_look, light_beacons, light_crash_bars, light_bulkheads).chain().run_if(world_runs).run_if(resource_exists::<MapId>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGES: [Stage; 7] = [Stage::Dormant, Stage::AllClear, Stage::Warning, Stage::Sealing, Stage::Sealed, Stage::Release, Stage::Opening];

    #[test]
    fn beacon_look_by_stage() {
        for stage in [Stage::Dormant, Stage::AllClear] {
            let look = beacon_look(stage, true);
            assert_eq!((look.lens, look.emissive, look.light), (Lens::Off, 0.0, BeaconLight::None), "{stage:?}");
        }
        for stage in [Stage::Warning, Stage::Sealing] {
            let look = beacon_look(stage, true);
            assert_eq!(look.lens, Lens::Red, "{stage:?}");
            assert_eq!(look.emissive, 20.0);
            assert_eq!(look.spin, 1.5, "the reflector spins at 1.5 rev/s");
            assert!(!look.pulses);
            assert_eq!(look.light, BeaconLight::Spot { lumens: 150_000.0, range: 10.0 });
        }
        let sealed = beacon_look(Stage::Sealed, true);
        assert_eq!((sealed.lens, sealed.emissive, sealed.spin, sealed.pulses), (Lens::Red, 8.0, 0.0, false));
        assert_eq!(sealed.light, BeaconLight::Point { lumens: 30_000.0, range: 6.0 });
        for stage in [Stage::Release, Stage::Opening] {
            let look = beacon_look(stage, true);
            assert_eq!((look.lens, look.emissive, look.spin, look.pulses), (Lens::Green, 15.0, 0.0, true), "{stage:?}");
            assert_eq!(look.light, BeaconLight::Point { lumens: 30_000.0, range: 6.0 });
        }
        assert_eq!(PULSE_HZ, 1.0);
        // A Wing that is not out stays dark whatever the stage.
        for stage in STAGES {
            assert_eq!(beacon_look(stage, false).lens, Lens::Off, "{stage:?}");
        }
        // The pulse goes up and down, once a second, and never to nothing.
        let samples: Vec<f32> = (0..100).map(|i| pulse(i as f32 / 100.0)).collect();
        assert!(samples.iter().all(|&p| p > 0.2 && p <= 1.0));
        assert!((pulse(0.0) - pulse(1.0)).abs() < 1e-4);
        assert!(samples.iter().cloned().fold(0.0, f32::max) - samples.iter().cloned().fold(1.0, f32::min) > 0.5);
    }

    #[test]
    fn at_most_two_beacon_lights_at_once() {
        for wing in [None, Some(WingId(0)), Some(WingId(1)), Some(WingId(2))] {
            for stage in STAGES {
                let lights = beacon_lights(MapId::Sanatorium, stage, wing);
                assert!(lights <= 2, "{stage:?} {wing:?}: {lights}");
                let lit = wing.is_some() && beacon_look(stage, true).light != BeaconLight::None;
                assert_eq!(lights, if lit { 2 } else { 0 }, "{stage:?} {wing:?}: two doors a Wing, and a light on each only when it is out");
            }
        }
    }

    #[test]
    fn crash_bar_strip_glows_only_while_sealed() {
        for stage in STAGES {
            let glow = crash_strip_glow(stage, true);
            assert_eq!(glow, if stage == Stage::Sealed { CHALK_GLOW } else { 0.0 }, "{stage:?}");
            assert_eq!(crash_strip_glow(stage, false), 0.0, "another Wing's strip stays dark");
        }
    }
}
