//! The Refinery's plant structures, dressed (#573, from #510 sections 2, 5 and 8): the tanks and their cone roofs, the column, the knockout
//! drum on its saddles, the flare stack's base, the fired heater with its stack and eight glowing peepholes, the pipe rack's bents, pipe runs and
//! sleepers, the stilts of the Control House, the booth and the weighbridge.
//!
//! The walkable slice (#556) made these plain solid Props, and they keep every collider: `Builder::solid_prop` makes each one's collider
//! and nothing else, and what is seen of it is drawn here, over the same box or cylinder, from `Layout::props` alone. So every machine builds
//! the same plant from the same Layout, and nothing is on the wire. The one new solid is the heater's stack (a Prop of its own in
//! `data::PLANT`). The heater's peepholes and the red aviation lamps (on the column's top and the heater's stack) are emissive only: no light,
//! so they cost nothing in the light budget. Their flicker and blink are pure functions of where they stand and the clock.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;

use crate::maps::darkness::lamp_flicker::position_hash;
use crate::maps::map::build::Builder;
use crate::maps::map::textures::{Material, Tint};
use crate::maps::map::{Prop, PropKind, palette};
use crate::maps::zone::ZoneId;
use crate::state::InRun;

use super::data;
use super::sky::{Aviation, aviation_material};

/// How far a skin stands proud of the collider it covers, so the two never fight.
const SKIN: f32 = 0.02;
/// The tanks' tint: white paint, weathered.
const TANK_TINT: Color = Color::srgb(0.75, 0.72, 0.66);
/// The column's cladding tint.
const COLUMN_TINT: Color = Color::srgb(0.75, 0.75, 0.72);
/// The colours the pieces wear until their materials load (and in the headless tests, for good).
const RUST: Color = Color::srgb(0.27, 0.19, 0.15);
const CLADDING: Color = Color::srgb(0.36, 0.37, 0.36);
const CONCRETE: Color = Color::srgb(0.3, 0.3, 0.29);
const TREAD: Color = Color::srgb(0.18, 0.19, 0.2);
const CONTAINER: Color = Color::srgb(0.2, 0.25, 0.27);
const DARK_STEEL: Color = Color::srgb(0.07, 0.07, 0.08);
const HAZARD: Color = Color::srgb(0.7, 0.55, 0.08);
/// The booth's lit window.
const WINDOW: Color = Color::srgb(1.0, 0.7, 0.35);

/// How tall the column stands (to its top, over the Tower Deck), and (in the tests) how tall the heater's stack does.
pub const COLUMN_TOP: f32 = 30.0;
#[cfg(test)]
pub const STACK_TOP: f32 = 20.0;

/// The heater's peepholes: each glows (1.0, 0.4, 0.1) at 6 times over, flickering on a hash of where it is. The count, and each one's centre
/// on the heater's faces (x 8..16, z 70..82), 1.4 m up.
pub const PEEPHOLE_COLOUR: Color = Color::srgb(1.0, 0.4, 0.1);
pub const PEEPHOLE_GLOW: f32 = 6.0;
pub const PEEPHOLES: [Vec3; 8] = [
    Vec3::new(7.98, 1.4, 72.0),
    Vec3::new(7.98, 1.4, 76.0),
    Vec3::new(7.98, 1.4, 80.0),
    Vec3::new(10.0, 1.4, 69.98),
    Vec3::new(14.0, 1.4, 69.98),
    Vec3::new(10.0, 1.4, 82.02),
    Vec3::new(14.0, 1.4, 82.02),
    Vec3::new(16.02, 1.4, 76.0),
];

/// A peephole of the fired heater.
#[derive(Component)]
pub struct Peephole;

/// How bright the peephole at `at` glows `now` seconds in, from `PEEPHOLE_GLOW` times 0.6 to times 1: a flutter of two sines, its phase from a
/// hash of where it stands. A pure function of its two arguments, so every machine's heater breathes the same.
pub fn peephole_strength(at: Vec3, now: f32) -> f32 {
    let phase = (position_hash(at) >> 8) as f32 / (1u32 << 24) as f32;
    let flutter = ((now * 5.3 + phase * TAU).sin() + (now * 11.1 + phase * 17.0).sin()) / 2.0;
    PEEPHOLE_GLOW * (0.8 + 0.2 * flutter)
}

/// What a peephole's emissive colour is at `strength`.
pub fn peephole_emissive(strength: f32) -> LinearRgba {
    LinearRgba::from(PEEPHOLE_COLOUR) * strength
}

/// Whether `prop` is dressed here (its look is built by `dress`, and it is a collider alone): every plant structure, which is every Prop of
/// `data::PLANT`. The rest of the Refinery's Props (#574: the tankers, the transformers, the pump sets, the drums and the rest) are ordinary Props
/// the builder dresses by their Zone's set, unit model and all.
pub fn dressed(prop: &Prop) -> bool {
    data::PLANT.contains(prop)
}

/// A box of `size` standing on `foot` (the middle of its base), drawn in `colour` or the material, tinted if it is.
fn slab(b: &mut Builder, size: Vec3, foot: Vec3, colour: Color, material: Option<(Material, Color)>) {
    let shape = b.shape_in(size, foot + Vec3::Y * size.y / 2.0, Quat::IDENTITY, colour, material.map(|(material, _)| material));
    if let Some((_, tint)) = material.filter(|(_, tint)| *tint != Color::WHITE) {
        b.tag(shape, Tint(tint));
    }
}

/// An upright cylinder `across` wide standing on `foot`, `tall`.
fn pipe(b: &mut Builder, across: f32, tall: f32, foot: Vec3, colour: Color, material: Option<(Material, Color)>) {
    let shape = b.cylinder_in(Vec3::new(across, tall, across), foot + Vec3::Y * tall / 2.0, Quat::IDENTITY, colour, material.map(|(material, _)| material));
    if let Some((_, tint)) = material.filter(|(_, tint)| *tint != Color::WHITE) {
        b.tag(shape, Tint(tint));
    }
}

/// A cylinder lying along x or z (`along_z`), `across` wide and `long`, its middle at `at`.
fn lying(b: &mut Builder, across: f32, long: f32, at: Vec3, along_z: bool, colour: Color, material: Option<Material>) {
    let turn = if along_z { Quat::from_rotation_x(FRAC_PI_2) } else { Quat::from_rotation_z(FRAC_PI_2) };
    b.cylinder_in(Vec3::new(across, long, across), at, turn, colour, material);
}

/// The ring of trim paint round a drum at `up`.
fn band(b: &mut Builder, across: f32, foot: Vec3, up: f32, colour: Color) {
    pipe(b, across, 0.12, foot + Vec3::Y * up, colour, None);
}

/// Dresses every plant structure of `data::PLANT`, and adds the rack's bents' beams and pipe runs and the aviation lamps. Nothing here is solid.
pub(super) fn dress(b: &mut Builder) {
    for prop in data::PLANT.iter().filter(|prop| dressed(prop)) {
        match (prop.kind, prop.zone) {
            (PropKind::Barrel, ZoneId::TankFarm) => tank(b, prop),
            (PropKind::Barrel, _) => column(b, prop),
            (PropKind::Machine, ZoneId::FlareYard) if prop.size.x < prop.size.z + 1.0 => flare_base(b, prop),
            (PropKind::Machine, ZoneId::FlareYard) => drum(b, prop),
            (PropKind::Machine, ZoneId::ColumnYard) => heater(b, prop),
            (PropKind::Machine, ZoneId::Gatehouse) if prop.size.x > 3.5 => weighbridge(b, prop),
            (PropKind::Machine, _) => booth(b, prop),
            (PropKind::Sandbags, _) => sleeper(b, prop),
            (PropKind::Pillar, ZoneId::RackRoad) => bent_post(b, prop),
            (PropKind::Pillar, ZoneId::Substation) => stilt(b, prop),
            (PropKind::Pillar, _) => heater_stack(b, prop),
            _ => {}
        }
    }
    rack_beams_and_pipes(b);
}

/// A storage tank: a weathered steel skin with trim paint rings, under a `metal_plate` cone roof.
fn tank(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    let (across, tall) = (prop.size.x + 2.0 * SKIN, prop.size.y);
    pipe(b, across, tall, prop.at, RUST, Some((Material::RustyMetal04, TANK_TINT)));
    for up in [tall * 0.33, tall * 0.66, tall - 0.12] {
        band(b, across + 0.1, prop.at, up, trim);
    }
    // The roof's eave, and the cone over it.
    pipe(b, across + 0.3, 0.2, prop.at + Vec3::Y * tall, DARK_STEEL, Some((Material::MetalPlate, Color::WHITE)));
    b.cone_in(across / 2.0 + 0.15, 1.6, prop.at + Vec3::Y * (tall + 0.2 + 0.8), TREAD, Some(Material::MetalPlate));
    // A vent on the apex.
    pipe(b, 0.5, 0.35, prop.at + Vec3::Y * (tall + 1.9), DARK_STEEL, None);
}

/// The cracking column: 6 m of corrugated cladding to y 30, rings at each deck, a dome and a red aviation lamp on top.
fn column(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    let across = prop.size.x + 2.0 * SKIN;
    pipe(b, across, COLUMN_TOP, prop.at, CLADDING, Some((Material::CorrugatedIron03, COLUMN_TINT)));
    for up in [1.0, 4.4, 8.4, 14.0, 22.0, COLUMN_TOP - 0.4] {
        band(b, across + 0.14, prop.at, up, trim);
    }
    b.cone_in(across / 2.0, 1.4, prop.at + Vec3::Y * (COLUMN_TOP + 0.7), DARK_STEEL, Some(Material::MetalPlate));
    aviation(b, prop.at + Vec3::Y * (COLUMN_TOP + 1.65));
}

/// The knockout drum: a horizontal cylinder with dished ends on two concrete saddles.
fn drum(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    let (long, across) = (prop.size.x, prop.size.z.min(prop.size.y) - 0.7);
    let saddle_top = 0.7;
    let middle = prop.at + Vec3::Y * (saddle_top + across / 2.0);
    for dx in [-long * 0.3, long * 0.3] {
        slab(b, Vec3::new(1.0, saddle_top, prop.size.z - 0.6), prop.at + Vec3::X * dx, CONCRETE, Some((Material::RebarReinforcedConcrete, Color::WHITE)));
    }
    lying(b, across, long - 1.2, middle, false, RUST, Some(Material::RustyMetal04));
    for side in [-1.0, 1.0] {
        // The dished head: a cone laid on its side, its apex out.
        let head = b.cone_in(across / 2.0, 0.6, middle + Vec3::X * side * (long / 2.0 - 0.9), RUST, Some(Material::RustyMetal04));
        b.tag(head, Transform::from_translation(middle + Vec3::X * side * (long / 2.0 - 0.9)).with_rotation(Quat::from_rotation_z(-side * FRAC_PI_2)));
        lying(b, across + 0.1, 0.12, middle + Vec3::X * side * (long / 2.0 - 1.2), false, trim, None);
    }
    for dx in [-2.5, 0.5, 3.0] {
        pipe(b, 0.35, 0.4, prop.at + Vec3::new(dx, saddle_top + across - 0.05, 0.0), DARK_STEEL, None);
    }
}

/// The flare stack's base: a concrete plinth and a rusty knock-out pot the 1.6 m stack rises from.
fn flare_base(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    slab(b, Vec3::new(prop.size.x, 0.8, prop.size.z), prop.at, CONCRETE, Some((Material::RebarReinforcedConcrete, Color::WHITE)));
    pipe(b, 3.0, prop.size.y - 0.8, prop.at + Vec3::Y * 0.8, RUST, Some((Material::RustyMetal04, Color::WHITE)));
    for up in [1.4, 2.6, prop.size.y - 0.16] {
        band(b, 3.12, prop.at, up, trim);
    }
}

/// The fired heater: a rusty box with its stack's breeching, a pair of trim bands and its eight glowing peepholes.
fn heater(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    let size = prop.size + Vec3::new(2.0 * SKIN, 0.0, 2.0 * SKIN);
    slab(b, size, prop.at, RUST, Some((Material::RustyMetal04, Color::WHITE)));
    for up in [0.5, 2.5] {
        slab(b, Vec3::new(size.x + 0.08, 0.14, size.z + 0.08), prop.at + Vec3::Y * up, trim, None);
    }
    for at in PEEPHOLES {
        // A square of glow flush on a face: thin across x on the faces looking along it, across z on the others.
        let on_x = (at.x - prop.at.x).abs() > (at.z - prop.at.z).abs();
        let size = if on_x { Vec3::new(0.04, 0.3, 0.3) } else { Vec3::new(0.3, 0.3, 0.04) };
        let glow = b.glow_box(size, at, PEEPHOLE_COLOUR, PEEPHOLE_GLOW);
        b.tag(glow, Peephole);
    }
}

/// The heater's stack: 1.2 m of rusty steel to y 20, bands, and a red aviation lamp on top.
fn heater_stack(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    pipe(b, prop.size.x, prop.size.y, prop.at, RUST, Some((Material::RustyMetal04, Color::WHITE)));
    for up in [3.0, 9.0, 15.0, prop.size.y - 0.2] {
        band(b, prop.size.x + 0.1, prop.at, up, trim);
    }
    aviation(b, prop.at + Vec3::Y * (prop.size.y + 0.3));
}

/// The weighbridge island: a concrete plinth under a steel weighing house, hazard-striped along its kerb.
fn weighbridge(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    slab(b, prop.size.with_y(1.0) + Vec3::new(2.0 * SKIN, 0.0, 2.0 * SKIN), prop.at, CONCRETE, Some((Material::RebarReinforcedConcrete, Color::WHITE)));
    slab(b, Vec3::new(prop.size.x + 0.1, 0.12, prop.size.z + 0.1), prop.at + Vec3::Y * 0.9, HAZARD, None);
    slab(b, Vec3::new(prop.size.x - 0.4, prop.size.y - 1.0, prop.size.z - 0.4), prop.at + Vec3::Y * 1.0, CONTAINER, Some((Material::ContainerSide, Color::WHITE)));
    slab(b, Vec3::new(prop.size.x, 0.12, prop.size.z), prop.at + Vec3::Y * (prop.size.y - 0.01), trim, None);
}

/// The gatehouse booth: a steel cabin with a lit window, under a flat overhanging roof.
fn booth(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    slab(b, prop.size + Vec3::new(2.0 * SKIN, 0.0, 2.0 * SKIN), prop.at, CONTAINER, Some((Material::ContainerSide, Color::WHITE)));
    slab(b, Vec3::new(prop.size.x + 0.5, 0.15, prop.size.z + 0.5), prop.at + Vec3::Y * (prop.size.y - 0.02), DARK_STEEL, Some((Material::MetalPlate, Color::WHITE)));
    slab(b, Vec3::new(prop.size.x + 0.06, 0.12, prop.size.z + 0.06), prop.at + Vec3::Y * 0.9, trim, None);
    // The window looks east, over the lane the trucks leave by.
    b.glow_box(Vec3::new(0.04, 0.7, prop.size.z - 1.2), prop.at + Vec3::new(prop.size.x / 2.0 + SKIN + 0.02, 1.9, 0.0), WINDOW, 0.9);
}

/// A rack sleeper: a concrete kerb with a trim cap and an expansion joint every 4 m.
fn sleeper(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    let (long_z, low) = (prop.size.z > prop.size.x, prop.at);
    slab(b, prop.size + Vec3::new(2.0 * SKIN, 0.0, 2.0 * SKIN), low, CONCRETE, Some((Material::RebarReinforcedConcrete, Color::WHITE)));
    slab(b, Vec3::new(prop.size.x + 0.08, 0.06, prop.size.z + 0.08), low + Vec3::Y * (prop.size.y - 0.02), trim, None);
    let length = if long_z { prop.size.z } else { prop.size.x };
    let mut along = -length / 2.0 + 2.0;
    while along < length / 2.0 - 0.5 {
        let (size, off) = if long_z { (Vec3::new(prop.size.x + 0.06, 0.9, 0.05), Vec3::new(0.0, 0.0, along)) } else { (Vec3::new(0.05, 0.9, prop.size.z + 0.06), Vec3::new(along, 0.0, 0.0)) };
        slab(b, size, low + off, DARK_STEEL, None);
        along += 4.0;
    }
}

/// A pipe-rack bent's post: an I-section, two flanges and a web, to the Rack Walk's underside.
fn bent_post(b: &mut Builder, prop: &Prop) {
    let steel = Some((Material::RustyMetal04, Color::srgb(0.55, 0.55, 0.55)));
    let (w, d) = (prop.size.x, prop.size.z);
    for side in [-1.0, 1.0] {
        slab(b, Vec3::new(0.07, prop.size.y, d + 0.02), prop.at + Vec3::X * side * (w / 2.0 - 0.035 + SKIN), RUST, steel);
    }
    slab(b, Vec3::new(w - 0.1, prop.size.y, 0.07), prop.at, RUST, steel);
    slab(b, Vec3::new(w + 0.2, 0.1, d + 0.2), prop.at, CONCRETE, None);
}

/// A stilt under the Control House: a steel column in a base plate and a cap plate.
fn stilt(b: &mut Builder, prop: &Prop) {
    let trim = palette(Some(prop.zone)).trim;
    slab(b, prop.size + Vec3::new(2.0 * SKIN, 0.0, 2.0 * SKIN), prop.at, RUST, Some((Material::RustyMetal04, Color::srgb(0.6, 0.6, 0.6))));
    slab(b, Vec3::new(prop.size.x + 0.36, 0.14, prop.size.z + 0.36), prop.at, CONCRETE, None);
    slab(b, Vec3::new(prop.size.x + 0.2, 0.1, prop.size.z + 0.2), prop.at + Vec3::Y * (prop.size.y - 0.3), trim, None);
}

/// Each bent's cross beam under the Rack Walk (its deck's underside is y 3.8), and the six pipe runs that lie along the rack at y 2.6 to 3.1.
fn rack_beams_and_pipes(b: &mut Builder) {
    let rack = ZoneId::RackRoad;
    let trim = palette(Some(rack)).trim;
    for z in [26.0, 34.0, 42.0, 50.0] {
        slab(b, Vec3::new(6.8, 0.3, 0.3), Vec3::new(0.0, 3.35, z), RUST, Some((Material::RustyMetal04, Color::srgb(0.55, 0.55, 0.55))));
        slab(b, Vec3::new(6.8, 0.06, 0.4), Vec3::new(0.0, 3.62, z), trim, None);
    }
    // (x, y of the middle, diameter)
    for (x, y, across) in [(-2.7, 2.8, 0.6), (-1.7, 3.1, 0.3), (-0.8, 2.85, 0.45), (0.3, 3.1, 0.35), (1.3, 2.8, 0.5), (2.4, 2.95, 0.6)] {
        lying(b, across, 43.0, Vec3::new(x, y, 42.0), true, RUST, Some(Material::RustyMetal02));
    }
}

/// A red aviation lamp at `at`: emissive only, so it costs no light.
fn aviation(b: &mut Builder, at: Vec3) {
    let (commands, meshes, materials) = b.assets();
    commands.spawn((
        InRun,
        Aviation,
        Mesh3d(meshes.add(Sphere::new(0.25).mesh().ico(1).expect("a small sphere"))),
        MeshMaterial3d(materials.add(aviation_material())),
        Transform::from_translation(at),
        bevy::light::NotShadowCaster,
    ));
}

#[cfg(test)]
#[path = "plant_tests.rs"]
mod tests;
