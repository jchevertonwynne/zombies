//! The Refinery Map: an oil refinery at night, in 15 Zones on three levels, with no Fire yet (a plain, playable slice: the Fuel Lines and
//! their Blazes come in a later ticket, which replaces `BLAZE_RECTS` below with `Layout.fuel_lines`).
//!
//! Built at the world origin (ADR 0009), plan (-60, 0) to (60, 90), north +z. Two networks laid over each other:
//!
//! - The ground (y 0): a 3 x 3 grid of yards and sheds, hemmed by a 3 m blast wall and cut by 3.6 m fire walls with a Door in each. South
//!   row: the Tanker Dock, the Gatehouse (start) and the Fitting Shop; middle row: the Tank Farm, the Rack Road and the Compressor House;
//!   north row: the Flare Yard, the Column Yard and the Substation. Nine Zones, three of them (the sheds) indoors.
//! - The catwalks (y 4) over the yards: the Rack Walk down the middle (the top deck of the pipe rack), the Tank Walk (the Girder Ring round
//!   Tank A and the North Gantry) to the west, the Pipe Bridge to the east, the Control House on stilts and the Column Platform.
//! - The Tower Deck (y 8) round the cracking column, with Pack-a-Punch.
//!
//! Six flights of steel stairs (S1 to S6) climb in bays that stand in no ground Zone's box (`ZoneId::notches`), each owned by the Zone at its
//! top, its Door at the foot. The one drop is the gap in the Rack Walk's east railing at z 30, on a 1 m cantilever, a 4 m fall into the Rack
//! Road's east lane, gated on S1.
//!
//! The lamps, the plant structures (the tanks, the column, the heater, the sleepers: Props, plain solids for now), the Blazes' rects and the
//! per-Zone tables are in separate items on purpose: the Fire, the Look, the Infills and the Props each come in a later ticket. Its path
//! nodes, lamps, windows and plant (in `data`) are laid out by `tools/refinery_layout.py`.

use bevy::prelude::*;

use crate::enemies::sapper::SapperTiming;
use crate::maps::zone::ZoneId;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, Layout, RaisedFloor, STOREY, Slope, palette};

#[path = "refinery/data.rs"]
mod data;

pub(in crate::maps::map) use data::WINDOWS;

/// The levels: the ground, the catwalks a storey up and the Tower Deck two.
const GROUND: f32 = 0.0;
const CATWALK: f32 = STOREY;
const DECK: f32 = 2.0 * STOREY;
/// How high a chalk outline's centre sits over its floor.
const CHALK: f32 = 1.5;
/// The blast wall round the plan, the fire walls between the yards (and the sheds' walls and roofs), and a railing.
const BLAST: f32 = 3.0;
pub(in crate::maps::map) const FIRE_WALL: f32 = 3.6;
const RAIL: f32 = 1.1;
/// A taller wall: a ladder cage, or the wall of a catwalk Door (a Door is 3 m high).
const CAGE: f32 = 3.4;
/// A stair bay's fence: a storey of climb and a railing over it.
const FENCE: f32 = STOREY + RAIL;
/// The blast wall where a catwalk stands within a jump of it: too tall to be cleared from the catwalk's deck.
const BLAST_TALL: f32 = 11.0;
const BLAST_TALLEST: f32 = 15.0;
/// How far a window's hatch wall (or a cage's pen) is, and how wide a pen is either side of its window.
const PEN: f32 = 2.8;
const PEN_HALF: f32 = 2.0;
const PEN_WALL: f32 = 3.0;

/// The first of the Refinery's windows in `WINDOWS`, after the other Maps'.
const W0: u8 = (super::greybox::WINDOWS.len()
    + super::quarry::WINDOWS.len()
    + super::village::WINDOWS.len()
    + super::dam::WINDOWS.len()
    + super::cove::WINDOWS.len()
    + super::polder::WINDOWS.len()
    + super::sanatorium::WINDOWS.len()
    + super::archive::WINDOWS.len()) as u8;

/// The Doors' prices: D1 to D12, S1 to S5 and G1 to G3 (S6's is the Power Door PD).
const D1: u64 = 750;
const D2: u64 = 750;
const D3: u64 = 1000;
const D4: u64 = 1000;
const D5: u64 = 1000;
const D6: u64 = 1000;
const D7: u64 = 1000;
const D8: u64 = 1250;
const D9: u64 = 1000;
const D10: u64 = 1250;
const D11: u64 = 1000;
const D12: u64 = 1000;
const S1: u64 = 1000;
const S2: u64 = 1000;
const S3: u64 = 1250;
const S4: u64 = 1000;
const S5: u64 = 1000;
const G1: u64 = 1000;
const G2: u64 = 1000;
const G3: u64 = 750;

/// A point in the Refinery.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A spot on a wall or a floor's plan in the Refinery.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

pub(super) const REFINERY: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [Gatehouse, TankerDock, FittingShop, TankFarm, RackRoad, CompressorHouse, FlareYard, ColumnYard, Substation, RackWalk, TankWalk, PipeBridge, ControlHouse, ColumnPlatform, TowerDeck]
    },
    // In a row across the Gatehouse, between its south wall's chalk and the weighbridge island.
    seats: [at(-4.5, GROUND, 3.5), at(-1.5, GROUND, 3.5), at(1.5, GROUND, 3.5), at(4.5, GROUND, 3.5)],
    wall_weapons: &WALL_WEAPONS,
    windows: &data::WINDOWS,
    risers: &RISERS,
    wading: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &[],
    raised_floors: &RAISED_FLOORS,
    infills: &[],
    slopes: &SLOPES,
    sunken: &[],
    water: &[],
    canals: &[],
    props: &data::PLANT,
    lamps: &data::LAMPS,
    fixed_joins: &FIXED_JOINS,
    shifting: &[],
    wings: &[],
    nav_nodes: data::NODES,
    nav_edges: data::EDGES,
    jump_pads: &[],
    ziplines: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    // Twelve of the 15 Zones are out of doors: only the three sheds (the Fitting Shop, the Compressor House and the Control House) have roofs.
    outdoor: &[
        ZoneId::Gatehouse,
        ZoneId::TankerDock,
        ZoneId::TankFarm,
        ZoneId::RackRoad,
        ZoneId::FlareYard,
        ZoneId::ColumnYard,
        ZoneId::Substation,
        ZoneId::RackWalk,
        ZoneId::TankWalk,
        ZoneId::PipeBridge,
        ZoneId::ColumnPlatform,
        ZoneId::TowerDeck,
    ],
    roofed: true,
    // The plan's edge is the blast wall: the ground round it is oily gravel and its outer faces are the blast wall's concrete (the Look ticket
    // draws the silhouettes beyond it).
    outside: Some(super::Outside { floor: super::textures::Material::TarredGravel, wall: super::textures::Material::RebarReinforcedConcrete }),
    sun: Vec3::new(0.3, -1.0, 0.5),
    // A copy of the Village's night until the Refinery's own Look (#510).
    look: crate::maps::look::VILLAGE_NIGHT,
    lamp_style: super::LampStyle::Bulb,
    bulb_zones: &[],
    plan: (Vec2::new(-60.0, 0.0), Vec2::new(60.0, 90.0)),
    build,
};

/// The one fixed join: the archway from the Pipe Bridge into the Control House (F1).
const FIXED_JOINS: [(ZoneId, ZoneId); 1] = [(ZoneId::PipeBridge, ZoneId::ControlHouse)];

/// The seven Blazes of #508's six Fuel Lines (the Rack Line's two, the Bund, Gantry, Flare, Furnace and Dock Lines'): (name, zone, min, max,
/// the floor's y). No path node stands in one and nothing placed stands within 1.5 m of one. The Fire ticket replaces this table with
/// `Layout.fuel_lines` and its Blazes; until then it is what the tests check against.
#[cfg(test)]
const BLAZE_RECTS: [(&str, ZoneId, Vec2, Vec2, f32); 7] = [
    ("Rack Line south", ZoneId::RackRoad, v(-3.5, 28.0), v(3.5, 32.0), GROUND),
    ("Rack Line north", ZoneId::RackRoad, v(-3.5, 44.0), v(3.5, 48.0), GROUND),
    ("Bund Line", ZoneId::TankFarm, v(-42.0, 38.0), v(-38.0, 42.0), GROUND),
    ("Gantry Line", ZoneId::TankWalk, v(-30.0, 50.0), v(-26.0, 54.0), CATWALK),
    ("Flare Line", ZoneId::FlareYard, v(-34.0, 85.0), v(-30.0, 90.0), GROUND),
    ("Furnace Line", ZoneId::ColumnYard, v(3.0, 74.0), v(8.0, 78.0), GROUND),
    ("Dock Line", ZoneId::TankerDock, v(-40.0, 7.0), v(-34.0, 11.0), GROUND),
];

/// The twelve Valves' spots (two a Fuel Line, one each side of its Blazes), also kept clear: (x, y, z).
#[cfg(test)]
const VALVE_SPOTS: [Vec3; 12] = [
    at(-4.5, GROUND, 24.0),
    at(4.5, GROUND, 56.0),
    at(-40.0, GROUND, 35.0),
    at(-40.0, GROUND, 45.0),
    at(-33.5, CATWALK, 52.0),
    at(-22.5, CATWALK, 52.0),
    at(-37.5, GROUND, 87.5),
    at(-26.5, GROUND, 87.5),
    at(5.5, GROUND, 71.5),
    at(5.5, GROUND, 80.5),
    at(-37.0, GROUND, 4.0),
    at(-37.0, GROUND, 14.0),
];

/// The chalk outlines: the cheap guns either side of the Gatehouse's Quick Revive, the MP40 on the Tanker Dock's south wall, the Trench Gun in
/// the Compressor House (indoors, close) and the AK-74u on the Column Yard's north wall.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    (WeaponId::M14, ZoneId::Gatehouse, at(-6.0, GROUND + CHALK, 0.2), Vec2::Y),
    (WeaponId::Olympia, ZoneId::Gatehouse, at(6.0, GROUND + CHALK, 0.2), Vec2::Y),
    (WeaponId::Mp40, ZoneId::TankerDock, at(-42.0, GROUND + CHALK, 0.2), Vec2::Y),
    (WeaponId::TrenchGun, ZoneId::CompressorHouse, at(59.8, GROUND + CHALK, 40.0), Vec2::NEG_X),
    (WeaponId::Ak74u, ZoneId::ColumnYard, at(-4.0, GROUND + CHALK, 89.8), Vec2::NEG_Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first, in the Tank Farm's south band, two Doors from the Gatehouse (D1 and
/// D4). None is in the Gatehouse, the Control House, the Tower Deck or on a catwalk.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // Against the Tank Farm's south fire wall, east of D4.
    (ZoneId::TankFarm, at(-26.0, GROUND, 20.65), Vec2::Y),
    // Against the Rack Road's east wall, north of D7: in the east lane.
    (ZoneId::RackRoad, at(19.35, GROUND, 40.0), Vec2::NEG_X),
    // Against the Fitting Shop's north wall, west of D5.
    (ZoneId::FittingShop, at(30.0, GROUND, 19.35), Vec2::NEG_Y),
    // Against the Compressor House's east wall.
    (ZoneId::CompressorHouse, at(59.35, GROUND, 54.0), Vec2::NEG_X),
    // Against the Column Yard's west wall.
    (ZoneId::ColumnYard, at(-19.35, GROUND, 66.0), Vec2::X),
    // Against the Substation's south wall, west of D10.
    (ZoneId::Substation, at(28.0, GROUND, 60.65), Vec2::Y),
];

/// The Hellhound Spawn Points: one in every Zone, and two each in the Rack Road, the Tank Farm, the Flare Yard and the Column Yard.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 19] = [
    (ZoneId::Gatehouse, at(12.0, GROUND, 6.0)),
    (ZoneId::TankerDock, at(-38.0, GROUND, 16.0)),
    (ZoneId::FittingShop, at(40.0, GROUND, 10.0)),
    (ZoneId::TankFarm, at(-56.0, GROUND, 38.0)),
    (ZoneId::TankFarm, at(-30.0, GROUND, 54.0)),
    (ZoneId::RackRoad, at(-12.0, GROUND, 36.0)),
    (ZoneId::RackRoad, at(12.0, GROUND, 46.0)),
    (ZoneId::CompressorHouse, at(40.0, GROUND, 40.0)),
    (ZoneId::FlareYard, at(-50.0, GROUND, 72.0)),
    (ZoneId::FlareYard, at(-28.0, GROUND, 70.0)),
    (ZoneId::ColumnYard, at(-8.0, GROUND, 66.0)),
    (ZoneId::ColumnYard, at(14.0, GROUND, 86.0)),
    (ZoneId::Substation, at(30.0, GROUND, 70.0)),
    (ZoneId::RackWalk, at(0.0, CATWALK, 40.0)),
    (ZoneId::TankWalk, at(-46.0, CATWALK, 52.0)),
    (ZoneId::PipeBridge, at(20.0, CATWALK, 57.0)),
    (ZoneId::ControlHouse, at(40.0, CATWALK, 70.0)),
    (ZoneId::ColumnPlatform, at(-8.0, CATWALK, 68.0)),
    (ZoneId::TowerDeck, at(-5.0, DECK, 72.0)),
];

/// The Special Spawn Points, where a Napalm or a Shrieker rises: the Rack Road, the Tank Farm, the Column Yard and the Substation.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 4] = [
    (ZoneId::RackRoad, at(12.0, GROUND, 32.0)),
    (ZoneId::TankFarm, at(-30.0, GROUND, 28.0)),
    (ZoneId::ColumnYard, at(-8.0, GROUND, 84.0)),
    (ZoneId::Substation, at(50.0, GROUND, 70.0)),
];

/// The Risers: two in the Flare Yard, two in the Tank Farm (the bund's sludge) and two in the Rack Road's side lanes (gravel).
const RISERS: [(ZoneId, Vec3); 6] = [
    (ZoneId::FlareYard, at(-48.0, GROUND, 66.0)),
    (ZoneId::FlareYard, at(-28.0, GROUND, 76.0)),
    (ZoneId::TankFarm, at(-40.0, GROUND, 26.0)),
    (ZoneId::TankFarm, at(-40.0, GROUND, 54.0)),
    (ZoneId::RackRoad, at(-14.0, GROUND, 46.0)),
    (ZoneId::RackRoad, at(14.0, GROUND, 34.0)),
];

/// Steel, for the stairs and the platform's steps.
const STEEL: Color = Color::srgb(0.2, 0.21, 0.23);
/// The loading platform in the Tanker Dock: 8 x 12 m and 1.2 m up, with steps on its north and south sides.
const PLATFORM: RaisedFloor = RaisedFloor { zone: ZoneId::TankerDock, min: v(-32.0, 4.0), max: v(-24.0, 16.0), height: 1.2, colour: STEEL };
const RAISED_FLOORS: [RaisedFloor; 1] = [PLATFORM];

/// The stairs: S1 to S6, each a straight 8 m flight for a storey in its bay, and the steps up onto the loading platform.
const SLOPES: [Slope; 8] = [
    Slope { zone: ZoneId::RackWalk, foot: at(-6.0, GROUND, 32.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.2, steps: true, colour: STEEL },
    Slope { zone: ZoneId::TankWalk, foot: at(-48.0, GROUND, 22.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.2, steps: true, colour: STEEL },
    Slope { zone: ZoneId::TankWalk, foot: at(-56.0, GROUND, 64.0), up: Vec2::NEG_Y, run: 8.0, rise: STOREY, width: 3.2, steps: true, colour: STEEL },
    Slope { zone: ZoneId::ControlHouse, foot: at(58.0, GROUND, 66.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.2, steps: true, colour: STEEL },
    Slope { zone: ZoneId::ColumnPlatform, foot: at(-14.0, GROUND, 68.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.2, steps: true, colour: STEEL },
    // S6 climbs from the heater's roof, a Zone over the Column Platform's catwalk, to the Tower Deck.
    Slope { zone: ZoneId::TowerDeck, foot: at(14.0, CATWALK, 72.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: 3.2, steps: true, colour: STEEL },
    Slope { zone: ZoneId::TankerDock, foot: at(-28.0, GROUND, 18.4), up: Vec2::NEG_Y, run: 2.4, rise: 1.2, width: 3.0, steps: true, colour: STEEL },
    Slope { zone: ZoneId::TankerDock, foot: at(-28.0, GROUND, 1.6), up: Vec2::Y, run: 2.4, rise: 1.2, width: 3.0, steps: true, colour: STEEL },
];

/// A stretch of wall along x at `z`, from `x0` to `x1`, with gaps at absolute `x`.
fn xwall(b: &mut Builder, (x0, x1): (f32, f32), z: f32, y0: f32, height: f32, gaps: Vec<(f32, Gap)>) {
    b.wall(v(x0, z), v(x1, z), y0, height, &gaps.into_iter().map(|(x, gap)| (x - x0, gap)).collect::<Vec<_>>());
}

/// A stretch of wall along z at `x`, from `z0` to `z1`, with gaps at absolute `z`.
fn zwall(b: &mut Builder, x: f32, (z0, z1): (f32, f32), y0: f32, height: f32, gaps: Vec<(f32, Gap)>) {
    b.wall(v(x, z0), v(x, z1), y0, height, &gaps.into_iter().map(|(z, gap)| (z - z0, gap)).collect::<Vec<_>>());
}

fn door(a: ZoneId, b: ZoneId, price: u64) -> Gap {
    Gap::Door(Some((a, b)), Some(price))
}

/// The Power Door.
fn pdoor(a: ZoneId, b: ZoneId) -> Gap {
    Gap::Door(Some((a, b)), None)
}

/// An archway: open to the floor.
fn arch(width: f32) -> Gap {
    Gap::Open { width, height: DOOR_HEIGHT }
}

/// A railing along x or z (a block 1.1 m high on `y0`, as every catwalk's edge has).
fn rail(b: &mut Builder, a: Vec2, c: Vec2, y0: f32) {
    b.wall(a, c, y0, RAIL, &[]);
}

/// The back-of-house behind the window at `centre` (looking `out` toward the wall): its hatch wall, `PEN_HALF` either side of the window, and
/// two walls back to the blast wall (or a shed's), `PEN` deep.
fn pen(b: &mut Builder, centre: Vec3, out: Vec2) {
    let along = Vec2::new(out.y.abs(), out.x.abs());
    let at = Vec2::new(centre.x, centre.z);
    let (lo, hi) = (at - along * PEN_HALF, at + along * PEN_HALF);
    b.wall(lo, hi, centre.y, PEN_WALL, &[(PEN_HALF, Gap::Window)]);
    for end in [lo, hi] {
        let back = end + out * PEN;
        b.wall(end.min(back), end.max(back), centre.y, PEN_WALL, &[]);
    }
}

/// A ladder cage: a catwalk's window `centre` in a cage wall on the edge, and behind it a pen on a ladder landing outside the railing, its floor a
/// slab at the catwalk's height (nothing under it) and walled round the other three sides. The Control House's windows are in its wall, whose pen
/// is the same.
fn cage_pen(b: &mut Builder, centre: Vec3, out: Vec2, in_wall: bool) {
    let along = Vec2::new(out.y.abs(), out.x.abs());
    let at = Vec2::new(centre.x, centre.z);
    let (lo, hi) = (at - along * PEN_HALF, at + along * PEN_HALF);
    if !in_wall {
        b.wall(lo, hi, centre.y, CAGE, &[(PEN_HALF, Gap::Window)]);
    }
    for end in [lo, hi] {
        let back = end + out * PEN;
        b.wall(end.min(back), end.max(back), centre.y, CAGE, &[]);
    }
    let (lo2, hi2) = (lo + out * PEN, hi + out * PEN);
    b.wall(lo2.min(hi2), lo2.max(hi2), centre.y, CAGE, &[]);
    let middle = at + out * PEN / 2.0;
    let size = Vec3::new(along.x * 2.0 * PEN_HALF + out.x.abs() * PEN, 0.3, along.y * 2.0 * PEN_HALF + out.y.abs() * PEN);
    b.block(size, Vec3::new(middle.x, centre.y - 0.15, middle.y), palette(Some(ZoneId::RackWalk)).floor);
}

/// A stair bay's solid landing: a block from the ground to `top`.
fn landing(b: &mut Builder, min: Vec2, max: Vec2, top: f32, zone: ZoneId) {
    let size = Vec3::new(max.x - min.x, top, max.y - min.y);
    b.block(size, Vec3::new((min.x + max.x) / 2.0, top / 2.0, (min.y + max.y) / 2.0), palette(Some(zone)).floor);
}

/// The walls, Doors, railings, cages, pens and bays of the Refinery.
fn build(b: &mut Builder) {
    use ZoneId::*;
    // The blast wall round the plan, 3 m high; the sheds' sides are as tall as their roofs (3.6 m), and it stands 11 m (15 m behind the Column
    // Platform) where a catwalk is close enough to be jumped from.
    xwall(b, (-60.0, 20.0), 0.0, GROUND, BLAST, vec![]);
    xwall(b, (20.0, 60.0), 0.0, GROUND, FIRE_WALL, vec![]);
    xwall(b, (-60.0, -16.0), 90.0, GROUND, BLAST, vec![]);
    xwall(b, (-16.0, 16.0), 90.0, GROUND, BLAST_TALLEST, vec![]);
    xwall(b, (16.0, 60.0), 90.0, GROUND, BLAST, vec![]);
    zwall(b, -60.0, (0.0, 26.0), GROUND, BLAST, vec![]);
    zwall(b, -60.0, (26.0, 68.0), GROUND, BLAST_TALL, vec![]);
    zwall(b, -60.0, (68.0, 90.0), GROUND, BLAST, vec![]);
    zwall(b, 60.0, (0.0, 60.0), GROUND, FIRE_WALL, vec![]);
    zwall(b, 60.0, (60.0, 62.0), GROUND, BLAST, vec![]);
    zwall(b, 60.0, (62.0, 80.0), GROUND, BLAST_TALL, vec![]);
    zwall(b, 60.0, (80.0, 90.0), GROUND, BLAST, vec![]);

    // The fire walls between the yards: x = -20 and x = 20 (D1, D2, D6, D7, D11 and D12), z = 20 (D4, D3, D5) and z = 60 (D8, D9, D10).
    zwall(b, -20.0, (0.0, 90.0), GROUND, FIRE_WALL, vec![(10.0, door(Gatehouse, TankerDock, D1)), (28.0, door(RackRoad, TankFarm, D6)), (75.0, door(FlareYard, ColumnYard, D11))]);
    zwall(b, 20.0, (0.0, 90.0), GROUND, FIRE_WALL, vec![(10.0, door(Gatehouse, FittingShop, D2)), (28.0, door(RackRoad, CompressorHouse, D7)), (75.0, door(ColumnYard, Substation, D12))]);
    xwall(b, (-60.0, 60.0), 20.0, GROUND, FIRE_WALL, vec![(-40.0, door(TankerDock, TankFarm, D4)), (-10.5, door(Gatehouse, RackRoad, D3)), (40.0, door(FittingShop, CompressorHouse, D5))]);
    // z = 60 runs under S3's flight, which crosses it at x -58..-54: the fire wall stops at its two sides, which are its bay's walls.
    xwall(b, (-60.0, -58.0), 60.0, GROUND, FIRE_WALL, vec![]);
    xwall(b, (-54.0, 60.0), 60.0, GROUND, FIRE_WALL, vec![(-40.0, door(TankFarm, FlareYard, D8)), (11.5, door(RackRoad, ColumnYard, D9)), (40.0, door(CompressorHouse, Substation, D10))]);

    // A pen behind every window on the ground, as `WINDOWS` has them; the catwalks' are cages.
    for &(zone, centre, out) in &data::WINDOWS {
        if centre.y == GROUND {
            pen(b, centre, out);
        } else if zone != ControlHouse {
            cage_pen(b, centre, out, false);
        } else {
            cage_pen(b, centre, out, true);
        }
    }

    stair_bays(b);
    catwalks(b);
    control_house(b);
}

/// The six stair bays: each fenced all round to a storey and a railing, its Door in the wall at its foot, its landing a solid block.
fn stair_bays(b: &mut Builder) {
    use ZoneId::*;
    let steel = palette(Some(RackWalk)).trim;
    // S1: x -8..-4, z 32..44, the flight z 32..40 and a landing z 40..44 open to the Rack Walk on its east side.
    xwall(b, (-8.0, -4.0), 32.0, GROUND, FENCE, vec![(-6.0, door(RackRoad, RackWalk, S1))]);
    zwall(b, -8.0, (32.0, 44.0), GROUND, FENCE, vec![]);
    xwall(b, (-8.0, -4.0), 44.0, GROUND, FENCE, vec![]);
    zwall(b, -4.0, (32.0, 40.0), GROUND, FENCE, vec![]);
    zwall(b, -4.0, (40.0, 44.0), GROUND, CATWALK, vec![]);
    landing(b, v(-8.0, 40.0), v(-4.0, 44.0), CATWALK, RackWalk);
    // S2: x -50..-46, z 22..30, the flight z 22..30 into the Girder Ring's south side.
    xwall(b, (-50.0, -46.0), 22.0, GROUND, FENCE, vec![(-48.0, door(TankFarm, TankWalk, S2))]);
    zwall(b, -50.0, (22.0, 30.0), GROUND, FENCE, vec![]);
    zwall(b, -46.0, (22.0, 30.0), GROUND, FENCE, vec![]);
    // S3: x -58..-54, z 54..64, the flight z 64..56 south onto a landing z 54..56 at the North Gantry's north edge. Its west side is filled
    // solid to the blast wall, so there is no pocket behind it, and the fire wall at z 60 stops at its sides.
    xwall(b, (-58.0, -54.0), 64.0, GROUND, FENCE, vec![(-56.0, door(FlareYard, TankWalk, S3))]);
    b.block(Vec3::new(2.0, FENCE, 10.0), Vec3::new(-59.0, FENCE / 2.0, 59.0), palette(Some(TankFarm)).wall);
    zwall(b, -58.0, (54.0, 64.0), GROUND, FENCE, vec![]);
    zwall(b, -54.0, (54.0, 64.0), GROUND, FENCE, vec![]);
    landing(b, v(-58.0, 54.0), v(-54.0, 56.0), CATWALK, TankWalk);
    // S4: x 56..60, z 66..76, the flight z 66..74 and a landing z 74..76 open west to the Control House's east doorway at z 75.
    xwall(b, (56.0, 60.0), 66.0, GROUND, FENCE, vec![(58.0, door(Substation, ControlHouse, S4))]);
    zwall(b, 60.0, (66.0, 76.0), GROUND, FENCE, vec![]);
    xwall(b, (56.0, 60.0), 76.0, GROUND, FENCE, vec![]);
    zwall(b, 56.0, (66.0, 73.4), GROUND, FENCE, vec![]);
    zwall(b, 56.0, (73.4, 76.0), GROUND, CATWALK, vec![]);
    landing(b, v(56.0, 74.0), v(60.0, 76.0), CATWALK, ControlHouse);
    // S5: x -16..-12, z 68..78, the flight z 68..76 and a landing z 76..78 open east onto the Column Platform.
    xwall(b, (-16.0, -12.0), 68.0, GROUND, FENCE, vec![(-14.0, door(ColumnYard, ColumnPlatform, S5))]);
    zwall(b, -16.0, (68.0, 78.0), GROUND, FENCE, vec![]);
    xwall(b, (-16.0, -12.0), 78.0, GROUND, FENCE, vec![]);
    zwall(b, -12.0, (68.0, 76.0), GROUND, FENCE, vec![]);
    zwall(b, -12.0, (76.0, 78.0), GROUND, CATWALK, vec![]);
    landing(b, v(-16.0, 76.0), v(-12.0, 78.0), CATWALK, ColumnPlatform);
    // S6: x 12..16, z 70..82, over the heater's roof (y 4): the Power Door PD in its west wall at the foot, the flight z 72..80 up to the Tower
    // Deck's level, and a landing z 80..82 that runs west over the platform to the Tower Deck's east edge.
    // The heater's roof, which S6 climbs from: the Prop is its body to 2.9 m, and this is the rest of its 4 m, so its Door (PD) isn't beside the Prop.
    b.block(Vec3::new(8.0, CATWALK - 2.9, 12.0), Vec3::new(12.0, (CATWALK + 2.9) / 2.0, 76.0), steel);
    zwall(b, 12.0, (70.0, 80.0), CATWALK, FENCE, vec![(71.0, pdoor(ColumnPlatform, TowerDeck))]);
    zwall(b, 12.0, (80.0, 82.0), CATWALK, STOREY, vec![]);
    zwall(b, 16.0, (70.0, 82.0), CATWALK, FENCE, vec![]);
    xwall(b, (12.0, 16.0), 70.0, CATWALK, FENCE, vec![]);
    xwall(b, (12.0, 16.0), 82.0, CATWALK, FENCE, vec![]);
    b.block(Vec3::new(8.0, 0.3, 2.0), Vec3::new(12.0, DECK - 0.15, 81.0), steel);
    rail(b, v(8.0, 80.0), v(12.0, 80.0), DECK);
    rail(b, v(8.0, 82.0), v(12.0, 82.0), DECK);
}

/// The catwalks' railings, cages, Door walls and the Ledge, edge by edge.
fn catwalks(b: &mut Builder) {
    use ZoneId::*;
    // The Rack Walk (x -4..4, z 20..64): a wall at its south end (Juggernog stands against it), railings along its sides but for S1's landing on
    // the west, G3's wall, G1's wall and the one gap in the east railing at z 28.5..31.5 (the Ledge), and G2's wall at its north end.
    xwall(b, (-4.0, 4.0), 20.0, CATWALK, CAGE, vec![]);
    zwall(b, -4.0, (20.0, 32.0), CATWALK, RAIL, vec![]);
    zwall(b, -4.0, (44.0, 50.0), CATWALK, RAIL, vec![]);
    zwall(b, -4.0, (50.0, 54.0), CATWALK, CAGE, vec![(52.0, door(TankWalk, RackWalk, G3))]);
    zwall(b, -4.0, (54.0, 64.0), CATWALK, RAIL, vec![]);
    zwall(b, 4.0, (20.0, 28.5), CATWALK, RAIL, vec![]);
    zwall(b, 4.0, (31.5, 34.0), CATWALK, RAIL, vec![]);
    zwall(b, 4.0, (38.0, 52.0), CATWALK, RAIL, vec![]);
    zwall(b, 4.0, (52.0, 62.0), CATWALK, CAGE, vec![(57.0, door(RackWalk, PipeBridge, G1))]);
    zwall(b, 4.0, (62.0, 64.0), CATWALK, RAIL, vec![]);
    xwall(b, (-4.0, 4.0), 64.0, CATWALK, CAGE, vec![(0.0, door(RackWalk, ColumnPlatform, G2))]);
    // The Ledge: a 1 m cantilever through the gap, railed down its two sides, its end open.
    b.block(Vec3::new(1.0, 0.3, 3.0), Vec3::new(4.5, CATWALK - 0.15, 30.0), palette(Some(RackWalk)).floor);
    rail(b, v(4.0, 28.5), v(5.0, 28.5), CATWALK);
    rail(b, v(4.0, 31.5), v(5.0, 31.5), CATWALK);

    // The Pipe Bridge (x 4..36, z 52..62): railings all round but G1's wall, the cage on its south side and the Control House's wall (F1's arch).
    xwall(b, (4.0, 24.0), 62.0, CATWALK, RAIL, vec![]);
    zwall(b, 36.0, (52.0, 62.0), CATWALK, RAIL, vec![]);
    xwall(b, (4.0, 10.0), 52.0, CATWALK, RAIL, vec![]);
    xwall(b, (14.0, 36.0), 52.0, CATWALK, RAIL, vec![]);

    // The Tank Walk: the Girder Ring (x -58..-38, z 30..50) round Tank A, with S2's flight arriving in the middle of its south side, and the
    // North Gantry (x -58..-4, z 50..54) running east from it to G3, with S3's landing at its west end on the north.
    xwall(b, (-58.0, -50.0), 30.0, CATWALK, RAIL, vec![]);
    xwall(b, (-46.0, -44.0), 30.0, CATWALK, RAIL, vec![]);
    xwall(b, (-40.0, -38.0), 30.0, CATWALK, RAIL, vec![]);
    zwall(b, -58.0, (30.0, 34.0), CATWALK, RAIL, vec![]);
    // The Claymore's wall, on the ring's west side.
    zwall(b, -58.0, (34.0, 38.0), CATWALK, CAGE, vec![]);
    zwall(b, -58.0, (38.0, 54.0), CATWALK, RAIL, vec![]);
    zwall(b, -38.0, (30.0, 50.0), CATWALK, RAIL, vec![]);
    xwall(b, (-38.0, -4.0), 50.0, CATWALK, RAIL, vec![]);
    xwall(b, (-54.0, -14.0), 54.0, CATWALK, RAIL, vec![]);
    xwall(b, (-10.0, -4.0), 54.0, CATWALK, RAIL, vec![]);

    // The Column Platform (x -12..12, z 64..88): railings but for G2's wall, S5's and S6's walls and landings, and the two cages.
    xwall(b, (-12.0, -4.0), 64.0, CATWALK, RAIL, vec![]);
    xwall(b, (4.0, 12.0), 64.0, CATWALK, RAIL, vec![]);
    zwall(b, -12.0, (64.0, 68.0), CATWALK, RAIL, vec![]);
    zwall(b, -12.0, (78.0, 82.0), CATWALK, RAIL, vec![]);
    zwall(b, -12.0, (86.0, 88.0), CATWALK, RAIL, vec![]);
    xwall(b, (-12.0, 12.0), 88.0, CATWALK, RAIL, vec![]);
    zwall(b, 12.0, (64.0, 70.0), CATWALK, RAIL, vec![]);
    zwall(b, 12.0, (82.0, 84.0), CATWALK, RAIL, vec![]);

    // The Tower Deck (x -8..8, z 68..84): railings all round but the cage on the north side and S6's landing on the east.
    xwall(b, (-8.0, 8.0), 68.0, DECK, RAIL, vec![]);
    zwall(b, -8.0, (68.0, 84.0), DECK, RAIL, vec![]);
    xwall(b, (-4.0, 8.0), 84.0, DECK, RAIL, vec![]);
    zwall(b, 8.0, (68.0, 80.0), DECK, RAIL, vec![]);
    zwall(b, 8.0, (82.0, 84.0), DECK, RAIL, vec![]);
}

/// The Control House's walls (y 4..8) on its stilts: F1's archway in the south wall, its two north windows, its east doorway from S4's
/// landing and a blank west wall.
fn control_house(b: &mut Builder) {
    xwall(b, (24.0, 56.0), 62.0, CATWALK, STOREY, vec![(30.0, arch(4.0))]);
    xwall(b, (24.0, 56.0), 86.0, CATWALK, STOREY, vec![(36.0, Gap::Window), (46.0, Gap::Window)]);
    zwall(b, 24.0, (62.0, 86.0), CATWALK, STOREY, vec![]);
    zwall(b, 56.0, (62.0, 86.0), CATWALK, STOREY, vec![(75.0, arch(2.4))]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::nav::NavGraph;
    use crate::maps::map::{BOX_LOCATIONS, MapId, WINDOWS, window_spawn};
    use ZoneId::*;

    fn refinery() -> &'static Layout {
        MapId::Refinery.layout()
    }

    const GROUND_ZONES: [ZoneId; 9] = [Gatehouse, TankerDock, FittingShop, TankFarm, RackRoad, CompressorHouse, FlareYard, ColumnYard, Substation];
    const CATWALK_ZONES: [ZoneId; 5] = [RackWalk, TankWalk, PipeBridge, ControlHouse, ColumnPlatform];

    #[test]
    fn fifteen_zones_starting_in_the_gatehouse_centred_on_the_origin_twelve_of_them_out_of_doors() {
        assert_eq!(refinery().zones.len(), 15);
        assert_eq!(refinery().start_zone(), Gatehouse);
        assert_eq!(refinery().plan, (Vec2::new(-60.0, 0.0), Vec2::new(60.0, 90.0)));
        for zone in GROUND_ZONES {
            assert_eq!(zone.floor(), GROUND, "{}", zone.name());
        }
        for zone in CATWALK_ZONES {
            assert_eq!(zone.floor(), CATWALK, "{}", zone.name());
        }
        assert_eq!(TowerDeck.floor(), DECK);
        assert_eq!(refinery().outdoor.len(), 12);
        for zone in [FittingShop, CompressorHouse, ControlHouse] {
            assert!(refinery().indoor(zone), "{}", zone.name());
        }
        for zone in refinery().zones.iter().filter(|zone| ![FittingShop, CompressorHouse, ControlHouse].contains(zone)) {
            assert!(!refinery().indoor(*zone), "{}", zone.name());
        }
        assert!(refinery().roofed && refinery().outside.is_some());
        assert!(refinery().sections.is_empty() && refinery().generators.is_empty() && !refinery().darkness && refinery().shifting.is_empty());
        // The Refinery's 15 Zones take the ZoneSet to 125 of its 128 bits.
        assert_eq!(ZoneId::ALL.len(), 125);
        assert_eq!(MapId::Refinery.barriers().count(), 29);
        assert_eq!(crate::maps::map::WINDOWS.len(), 227);
    }

    /// The Zones' boxes at #508's volumes: the Tank Walk's is its three rectangles' bounding box, and every ground Zone is a yard or shed of the grid.
    #[test]
    fn the_zones_stand_where_the_plan_puts_them() {
        let rect = |zone: ZoneId| {
            let (lo, hi) = zone.volume();
            (lo.x, hi.x, lo.z, hi.z)
        };
        assert_eq!(rect(Gatehouse), (-20.0, 20.0, 0.0, 20.0));
        assert_eq!(rect(TankerDock), (-60.0, -20.0, 0.0, 20.0));
        assert_eq!(rect(FittingShop), (20.0, 60.0, 0.0, 20.0));
        assert_eq!(rect(TankFarm), (-60.0, -20.0, 20.0, 60.0));
        assert_eq!(rect(RackRoad), (-20.0, 20.0, 20.0, 60.0));
        assert_eq!(rect(CompressorHouse), (20.0, 60.0, 20.0, 60.0));
        assert_eq!(rect(FlareYard), (-60.0, -20.0, 60.0, 90.0));
        assert_eq!(rect(ColumnYard), (-20.0, 20.0, 60.0, 90.0));
        assert_eq!(rect(Substation), (20.0, 60.0, 60.0, 90.0));
        assert_eq!(rect(RackWalk), (-4.0, 4.0, 20.0, 64.0));
        assert_eq!(rect(TankWalk), (-58.0, -4.0, 30.0, 64.0));
        assert_eq!(rect(PipeBridge), (4.0, 36.0, 52.0, 62.0));
        assert_eq!(rect(ControlHouse), (24.0, 56.0, 62.0, 86.0));
        assert_eq!(rect(ColumnPlatform), (-12.0, 12.0, 64.0, 88.0));
        assert_eq!(rect(TowerDeck), (-8.0, 8.0, 68.0, 84.0));
        // The Tank Walk is three rectangles: the Girder Ring, the North Gantry and the landing of S3.
        let walk = TankWalk;
        let on_walk = |x: f32, z: f32| walk.has(Vec3::new(x, CATWALK + 0.1, z));
        assert!(on_walk(-48.0, 32.0) && on_walk(-30.0, 52.0) && on_walk(-56.0, 55.0));
        assert!(!on_walk(-20.0, 40.0) && !on_walk(-30.0, 60.0) && !on_walk(-56.0, 60.0));
    }

    /// Boxes may be stacked (a catwalk's over a ground Zone's) but never overlap in all three axes.
    #[test]
    fn refinery_zone_boxes_do_not_overlap_across_levels() {
        let mut stacked = 0;
        for &a in refinery().zones {
            for &b in refinery().zones.iter().filter(|&&b| b != a) {
                let ((alo, ahi), (blo, bhi)) = (a.volume(), b.volume());
                let overlap_plan = alo.x < bhi.x && blo.x < ahi.x && alo.z < bhi.z && blo.z < ahi.z;
                let overlap_height = alo.y < bhi.y && blo.y < ahi.y;
                assert!(!(overlap_plan && overlap_height), "{} overlaps {}", a.name(), b.name());
                if overlap_plan && !overlap_height {
                    stacked += 1;
                }
            }
        }
        assert!(stacked > 10, "only {stacked} stacked pairs");
    }

    /// Every stair bay stands in no ground Zone's box (S3's crosses the Tank Farm and the Flare Yard's wall), and each Door at its foot joins the
    /// Zone it climbs from to the Zone at its top.
    #[test]
    fn refinery_stair_bays_sit_in_no_ground_zone() {
        use crate::maps::zone::{REFINERY_S1, REFINERY_S2, REFINERY_S3, REFINERY_S4, REFINERY_S5, REFINERY_S6};
        let bays = [("S1", REFINERY_S1), ("S2", REFINERY_S2), ("S3", REFINERY_S3), ("S4", REFINERY_S4), ("S5", REFINERY_S5), ("S6", REFINERY_S6)];
        for (name, (min, max)) in bays {
            let mut x = min.x + 0.2;
            while x < max.x {
                let mut z = min.y + 0.2;
                while z < max.y {
                    for y in [0.1, 1.0, 3.0] {
                        let zone = crate::enemies::nav::zone_at(MapId::Refinery, Vec3::new(x, y, z));
                        assert!(zone.is_none_or(|zone| zone.floor() != GROUND), "the {name}'s bay is in the {zone:?}'s box at ({x}, {y}, {z})");
                    }
                    z += 0.5;
                }
                x += 0.5;
            }
        }
        // The stairs are owned by the Zone at their top: each Slope's Zone is a catwalk or the Tower Deck.
        for slope in &SLOPES[..6] {
            assert!(slope.zone.floor() >= CATWALK, "{}", slope.zone.name());
            assert_eq!((slope.rise, slope.run), (STOREY, 8.0));
        }
        // A ground Zone's notches are exactly the bays it holds.
        assert!(TankFarm.notches().len() == 2 && FlareYard.notches().len() == 1 && RackRoad.notches().len() == 1);
        assert!(ColumnYard.notches().len() == 2 && Substation.notches().len() == 1);
    }

    /// The seven Blazes stay clear of every path node (their floor's own), whatever Zone it is in.
    #[test]
    fn refinery_no_path_node_in_a_blaze_rect() {
        for &(name, nodes) in &[("nodes", refinery().nav_nodes)] {
            for &(node, x, y, z) in nodes {
                for (blaze, _, lo, hi, floor) in BLAZE_RECTS {
                    let inside = x > lo.x && x < hi.x && z > lo.y && z < hi.y && (y - floor).abs() < 1.0;
                    assert!(!inside, "the {name}' node {node} at ({x}, {y}, {z}) is in the {blaze}");
                }
            }
        }
    }

    /// Nothing placed stands within 1.5 m of a Blaze: no Spawn Point, Barrier, Buyable, Box Location, Trap or Valve spot (a Valve's spot is also
    /// clear of the plant and of the Blaze itself).
    #[test]
    fn refinery_nothing_stands_within_1_5_m_of_a_blaze_rect() {
        use crate::economy::perk::Perk;
        let layout = refinery();
        let mut things: Vec<(String, Vec3)> = Vec::new();
        for i in 0..layout.windows.len() {
            let barrier = crate::maps::map::BarrierId(W0 + i as u8);
            things.push((format!("window {i}'s Barrier"), crate::maps::map::window_centre(barrier)));
            things.push((format!("window {i}'s Spawn Point"), window_spawn(barrier)));
        }
        for &(_, at) in layout.risers.iter().chain(layout.hellhound_spawns).chain(layout.special_spawns) {
            things.push(("a Spawn Point".into(), at));
        }
        for &(weapon, _, at, _) in layout.wall_weapons {
            things.push((format!("the {weapon:?} wall"), at));
        }
        for &(wall, _, at, _) in crate::weapons::equipment::equipment_walls(MapId::Refinery) {
            things.push((format!("the {wall:?} wall"), at));
        }
        for perk in Perk::ALL {
            things.push((format!("{perk:?}"), perk.machine_spot(MapId::Refinery).0));
        }
        things.push(("the Power Switch".into(), crate::maps::power::switch_spot(MapId::Refinery).unwrap().0));
        things.push(("Pack-a-Punch".into(), crate::weapons::pack_a_punch::machine_at(MapId::Refinery)));
        for i in MapId::Refinery.box_locations() {
            things.push((format!("Box Location {i}"), BOX_LOCATIONS[i].1));
        }
        for &((a, b), at, switch_x) in crate::maps::trap::traps(MapId::Refinery) {
            things.push((format!("the Trap at {} / {}", a.name(), b.name()), at));
            things.push(("a Trap switch".into(), Vec3::new(switch_x, at.y, at.z)));
        }
        for (i, spot) in VALVE_SPOTS.iter().enumerate() {
            things.push((format!("Valve spot {i}"), *spot));
        }
        // The Doors: every wall gap in the Layout's nav graph (its `d<n>_a` and `_b` nodes stand 1.8 m either side of a Door).
        for &(node, x, y, z) in layout.nav_nodes.iter().filter(|(n, ..)| n.ends_with("_a") || n.ends_with("_b")) {
            things.push((format!("the Door node {node}"), Vec3::new(x, y, z)));
        }
        let mut bad = Vec::new();
        for (what, at) in &things {
            for (blaze, _, lo, hi, floor) in BLAZE_RECTS {
                // The one on a catwalk is judged against catwalk fixtures only, and the ground ones against the ground's.
                if (at.y - floor).abs() > 1.0 {
                    continue;
                }
                let d = Vec2::new((lo.x - at.x).max(0.0).max(at.x - hi.x), (lo.y - at.z).max(0.0).max(at.z - hi.y));
                if d.length() < 1.5 {
                    bad.push(format!("{what} at {at} is {:.2} m from the {blaze}", d.length()));
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// The Rack Walk's one drop, the sleepers, the heater and the tanks are solid where a Zombie walks: the sleepers and the heater stop a
    /// Zombie's body (the Players hop the sleepers, but a Zombie can't: they are 1 m high).
    #[test]
    fn refinery_sleepers_and_heater_are_solid_to_zombies() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        use crate::tuning::ZOMBIE_RADIUS;
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let blocked: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                let body = Collider::sphere(ZOMBIE_RADIUS * 0.8);
                let blocked = |from: Vec3, to: Vec3| {
                    let dir = Dir3::new(to - from).unwrap();
                    spatial.cast_shape(&body, from, Quat::IDENTITY, dir, &ShapeCastConfig::from_max_distance(from.distance(to)), &filter).is_some()
                };
                let mut out = Vec::new();
                // From each side lane into the channel, across the sleepers, at knee height of a walking Zombie, along several z.
                for z in [26.0, 30.0, 36.0, 44.0, 50.0, 54.0] {
                    for (from, to) in [(Vec3::new(-6.0, 0.5, z), Vec3::new(-2.0, 0.5, z)), (Vec3::new(6.0, 0.5, z), Vec3::new(2.0, 0.5, z))] {
                        if !blocked(from, to) {
                            out.push(format!("a Zombie walks over the sleepers at z {z}"));
                        }
                    }
                }
                // Through the heater (x 8..16, z 70..82) in each direction.
                for (from, to) in [
                    (Vec3::new(6.0, 1.0, 76.0), Vec3::new(18.0, 1.0, 76.0)),
                    (Vec3::new(12.0, 1.0, 68.0), Vec3::new(12.0, 1.0, 84.0)),
                ] {
                    if !blocked(from, to) {
                        out.push("a Zombie walks through the heater".into());
                    }
                }
                // And the sleepers are low enough for the Player to hop: their top is 1 m.
                let hit = spatial.cast_ray(Vec3::new(-4.0, 3.0, 48.0), Dir3::NEG_Y, 4.0, true, &filter).map(|hit| 3.0 - hit.distance);
                if hit.is_none_or(|top| (top - 1.0).abs() > 0.01) {
                    out.push(format!("the sleepers' top is {hit:?}, not 1 m"));
                }
                out
            })
            .unwrap();
        assert!(blocked.is_empty(), "{blocked:#?}");
    }

    /// Each catwalk's outer edge has a railing at least 1.1 m high, but at the Rack Walk's one Ledge (the gap in its east railing at z 28.5..31.5,
    /// where the cantilever's deck is flush with the floor) and where a bay, a Door or a window opens it (stairs, doors, archways and cages' windows).
    #[test]
    fn refinery_catwalk_railings_are_at_least_1_1_m_except_at_the_ledge() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let low: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                // (name, level, edge: from, to along the edge, inset direction into the deck)
                let edges: Vec<(&str, f32, Vec2, Vec2, Vec2)> = vec![
                    ("Rack Walk west", CATWALK, v(-4.0, 20.0), v(-4.0, 64.0), Vec2::X),
                    ("Rack Walk east", CATWALK, v(4.0, 20.0), v(4.0, 64.0), Vec2::NEG_X),
                    ("Rack Walk south", CATWALK, v(-4.0, 20.0), v(4.0, 20.0), Vec2::Y),
                    ("Pipe Bridge north", CATWALK, v(4.0, 62.0), v(36.0, 62.0), Vec2::NEG_Y),
                    ("Pipe Bridge south", CATWALK, v(4.0, 52.0), v(36.0, 52.0), Vec2::Y),
                    ("Pipe Bridge east", CATWALK, v(36.0, 52.0), v(36.0, 62.0), Vec2::NEG_X),
                    ("Girder Ring south", CATWALK, v(-58.0, 30.0), v(-38.0, 30.0), Vec2::Y),
                    ("Girder Ring west", CATWALK, v(-58.0, 30.0), v(-58.0, 54.0), Vec2::X),
                    ("Girder Ring east", CATWALK, v(-38.0, 30.0), v(-38.0, 50.0), Vec2::NEG_X),
                    ("North Gantry south", CATWALK, v(-38.0, 50.0), v(-4.0, 50.0), Vec2::Y),
                    ("North Gantry north", CATWALK, v(-54.0, 54.0), v(-4.0, 54.0), Vec2::NEG_Y),
                    ("Column Platform south", CATWALK, v(-12.0, 64.0), v(12.0, 64.0), Vec2::Y),
                    ("Column Platform west", CATWALK, v(-12.0, 64.0), v(-12.0, 88.0), Vec2::X),
                    ("Column Platform east", CATWALK, v(12.0, 64.0), v(12.0, 88.0), Vec2::NEG_X),
                    ("Column Platform north", CATWALK, v(-12.0, 88.0), v(12.0, 88.0), Vec2::NEG_Y),
                    ("Tower Deck south", DECK, v(-8.0, 68.0), v(8.0, 68.0), Vec2::Y),
                    ("Tower Deck west", DECK, v(-8.0, 68.0), v(-8.0, 84.0), Vec2::X),
                    ("Tower Deck north", DECK, v(-8.0, 84.0), v(8.0, 84.0), Vec2::NEG_Y),
                    ("Tower Deck east", DECK, v(8.0, 68.0), v(8.0, 84.0), Vec2::NEG_X),
                ];
                // Where each edge is open on purpose: a bay's landing, a stair's top, a Door's gap, a window's, an archway, the Ledge.
                let open = |name: &str, along: f32, level: f32| -> bool {
                    match name {
                        "Rack Walk west" => (40.0..=44.0).contains(&along) || (51.3..=52.7).contains(&along),
                        "Rack Walk east" => (28.5..=31.5).contains(&along) || (35.2..=36.8).contains(&along) || (55.8..=58.2).contains(&along),
                        "Rack Walk south" => false,
                        "Pipe Bridge north" => (28.0..=32.0).contains(&along),
                        "Pipe Bridge south" => (11.2..=12.8).contains(&along),
                        "Girder Ring south" => (-50.0..=-46.0).contains(&along) || (-42.8..=-41.2).contains(&along),
                        "Girder Ring west" => (54.0..=54.1).contains(&along),
                        "North Gantry north" => (-12.8..=-11.2).contains(&along),
                        "Column Platform south" => (-1.2..=1.2).contains(&along),
                        "Column Platform west" => (76.0..=78.0).contains(&along) || (83.2..=84.8).contains(&along),
                        "Column Platform east" => (70.0..=82.0).contains(&along) || (85.2..=86.8).contains(&along),
                        "Tower Deck north" => (-6.8..=-5.2).contains(&along),
                        "Tower Deck east" => (80.0..=82.0).contains(&along),
                        _ => {
                            let _ = level;
                            false
                        }
                    }
                };
                let mut out = Vec::new();
                for (name, level, from, to, inward) in edges {
                    let length = from.distance(to);
                    let dir = (to - from) / length;
                    let steps = (length / 0.25) as usize;
                    for i in 0..=steps {
                        let along = i as f32 * 0.25;
                        let p = from + dir * along;
                        // The coordinate along the edge, as the open ranges give it (z for a west or east edge, x for the others).
                        let coord = if dir.x.abs() > 0.5 { p.x } else { p.y };
                        if open(name, coord, level) {
                            continue;
                        }
                        // Looking down a hand's width outside the deck: the railing's top, not the deck.
                        let at = p - inward * 0.1;
                        let Some(hit) = spatial.cast_ray(Vec3::new(at.x, level + 3.0, at.y), Dir3::NEG_Y, 4.0, true, &filter) else {
                            out.push(format!("{name} at ({}, {}) has no railing", at.x, at.y));
                            continue;
                        };
                        let top = level + 3.0 - hit.distance;
                        if top < level + RAIL - 0.05 {
                            out.push(format!("{name} at ({}, {}) is only {} m", at.x, at.y, top - level));
                        }
                    }
                }
                out
            })
            .unwrap();
        assert!(low.is_empty(), "{low:#?}");
        // The Ledge is a gap: the east railing is gone but for the cantilever's own deck, flush with the floor.
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let tops: Vec<f32> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| {
                let filter = SpatialQueryFilter::from_mask(crate::layers::GameLayer::World);
                [28.7, 29.5, 30.0, 30.5, 31.3]
                    .into_iter()
                    .map(|z| spatial.cast_ray(Vec3::new(3.9, CATWALK + 3.0, z), Dir3::NEG_Y, 4.0, true, &filter).map_or(f32::NEG_INFINITY, |hit| CATWALK + 3.0 - hit.distance))
                    .collect::<Vec<f32>>()
            })
            .unwrap();
        assert!(tops.iter().all(|&top| top <= CATWALK + 0.05), "the Ledge is not open: {tops:?}");
    }

    /// Every ladder cage's Spawn Point (a catwalk window's) stands on a floor: the ladder landing's slab, a hand's breadth under the point.
    #[test]
    fn refinery_ladder_cage_spawn_points_stand_on_a_floor() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let bad: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                MapId::Refinery
                    .barriers()
                    .filter_map(|barrier| {
                        let spawn = window_spawn(barrier);
                        let hit = spatial.cast_ray(spawn + Vec3::Y * 0.5, Dir3::NEG_Y, 1.0, true, &filter);
                        let floor = hit.map(|hit| spawn.y + 0.5 - hit.distance);
                        (floor.is_none_or(|floor| (floor - spawn.y).abs() > 0.05)).then(|| format!("window {} at {spawn}: floor {floor:?}", barrier.0 - W0))
                    })
                    .collect()
            })
            .unwrap();
        assert!(bad.is_empty(), "{bad:#?}");
        // 9 of the 29 are on a catwalk or the Tower Deck (the Control House's two are in its walls and count here too).
        let cages = MapId::Refinery.barriers().filter(|b| window_spawn(*b).y > GROUND).count();
        assert_eq!(cages, 9);
    }

    /// The Doors a Run can buy, with their joins: 20 bought and the one Power Door.
    fn doors_of(sim: &mut crate::enemies::zombie::headless::Sim) -> Vec<(Option<(ZoneId, ZoneId)>, Option<u64>)> {
        use bevy::ecs::system::RunSystemOnce;

        use crate::maps::door::Door;
        sim.world_mut()
            .run_system_once(|doors: Query<&Door>| doors.iter().map(|door| (door.route(), door.price)).collect::<Vec<_>>())
            .unwrap()
    }

    /// The cheapest price of Doors from the Gatehouse to each Zone, doors bought one by one (a fixed join costs nothing).
    fn door_costs(doors: &[(Option<(ZoneId, ZoneId)>, Option<u64>)]) -> std::collections::HashMap<ZoneId, u64> {
        let mut cost = std::collections::HashMap::from([(Gatehouse, 0u64)]);
        for _ in 0..30 {
            for &(joins, price) in doors {
                let (Some((a, b)), Some(price)) = (joins, price) else { continue };
                for (from, to) in [(a, b), (b, a)] {
                    let Some(&here) = cost.get(&from) else { continue };
                    let best = cost.entry(to).or_insert(u64::MAX);
                    *best = (*best).min(here + price);
                }
            }
            for &(a, b) in &FIXED_JOINS {
                for (from, to) in [(a, b), (b, a)] {
                    if let Some(&here) = cost.get(&from) {
                        let best = cost.entry(to).or_insert(u64::MAX);
                        *best = (*best).min(here);
                    }
                }
            }
        }
        cost
    }

    /// 21 Doors (20 bought and PD) and the fixed join F1: the Power costs 3000 from the start (D3, S1 and G1, across the Pipe Bridge).
    #[test]
    fn refinery_power_costs_3000_from_the_start() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let doors = doors_of(&mut sim);
        assert_eq!(doors.len(), 21);
        assert_eq!(doors.iter().filter(|(_, price)| price.is_none()).count(), 1, "one Power Door");
        let cost = door_costs(&doors);
        assert_eq!(cost[&ControlHouse], 3000);
        // The dearer way, D2, D5, D10 and S4, is 4000.
        let price = |a: ZoneId, b: ZoneId| doors.iter().find(|(joins, _)| matches!(joins, Some((x, y)) if (*x, *y) == (a, b) || (*x, *y) == (b, a))).and_then(|d| d.1).unwrap();
        assert_eq!(price(Gatehouse, FittingShop) + price(FittingShop, CompressorHouse) + price(CompressorHouse, Substation) + price(Substation, ControlHouse), 4000);
        assert_eq!(price(Gatehouse, RackRoad) + price(RackRoad, RackWalk) + price(RackWalk, PipeBridge), 3000);
    }

    /// PD opens on the Power, so Pack-a-Punch is the Doors that reach the Control House and the Column Platform: D3, S1, G1 and G2 come to 4000.
    #[test]
    fn refinery_pack_a_punch_costs_4000_and_pd_opens_on_the_power() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let doors = doors_of(&mut sim);
        let power_door = doors.iter().find(|(_, price)| price.is_none()).unwrap().0.unwrap();
        assert!(power_door == (ColumnPlatform, TowerDeck) || power_door == (TowerDeck, ColumnPlatform));
        let bought: Vec<((ZoneId, ZoneId), u64)> = doors.iter().filter_map(|&(joins, price)| Some((joins?, price?))).collect();
        assert_eq!(bought.len(), 20);
        let mut cheapest = u64::MAX;
        for mask in 0u32..(1 << bought.len()) {
            let cost: u64 = (0..bought.len()).filter(|i| mask >> i & 1 == 1).map(|i| bought[i].1).sum();
            if cost >= cheapest {
                continue;
            }
            let mut reached = std::collections::HashSet::from([Gatehouse]);
            loop {
                let before = reached.len();
                for (i, &((a, b), _)) in bought.iter().enumerate() {
                    if mask >> i & 1 == 1 && (reached.contains(&a) || reached.contains(&b)) {
                        reached.insert(a);
                        reached.insert(b);
                    }
                }
                for &(a, b) in &FIXED_JOINS {
                    if reached.contains(&a) || reached.contains(&b) {
                        reached.insert(a);
                        reached.insert(b);
                    }
                }
                if reached.len() == before {
                    break;
                }
            }
            // The Power is on once the Control House is reached; PD then joins the Column Platform to the Tower Deck.
            if reached.contains(&ControlHouse) && reached.contains(&ColumnPlatform) {
                cheapest = cost;
            }
        }
        assert_eq!(cheapest, 4000, "D3, S1 and G1 for the Power, then G2 to the Column Platform, where PD opens");
        // The ground route (D3, D9 and S5) reaches the Column Platform for 3000 too.
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let cost = door_costs(&doors_of(&mut sim));
        assert_eq!(cost[&ColumnPlatform], 3000);
    }

    #[test]
    fn refinery_box_starts_two_doors_out() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
        let doors = doors_of(&mut sim);
        let first = MapId::Refinery.box_locations().next().unwrap();
        let zone = BOX_LOCATIONS[first].0;
        let mut hops = std::collections::HashMap::from([(Gatehouse, 0u32)]);
        for step in 0..8 {
            for &(joins, _) in &doors {
                let Some((a, b)) = joins else { continue };
                for (from, to) in [(a, b), (b, a)] {
                    if hops.get(&from) == Some(&step) && !hops.contains_key(&to) {
                        hops.insert(to, step + 1);
                    }
                }
            }
        }
        assert_eq!(zone, TankFarm);
        assert_eq!(hops[&zone], 2, "the Box starts in the Tank Farm, two Doors out (D1 and D4)");
        assert_eq!(door_costs(&doors)[&zone], 1750);
    }

    #[test]
    fn refinery_counts_29_barriers_6_risers_19_hellhound_spawns_and_four_special_zones() {
        let layout = refinery();
        assert_eq!(MapId::Refinery.barriers().count(), 29);
        assert_eq!(layout.risers.len(), 6);
        assert_eq!(layout.hellhound_spawns.len(), 19);
        assert_eq!(layout.lamps.len(), 87);
        for &zone in layout.zones {
            let want = if matches!(zone, RackRoad | TankFarm | FlareYard | ColumnYard) { 2 } else { 1 };
            assert_eq!(layout.hellhound_spawns.iter().filter(|(z, _)| *z == zone).count(), want, "{}", zone.name());
        }
        let specials: Vec<ZoneId> = layout.special_spawns.iter().map(|&(zone, _)| zone).collect();
        assert_eq!(specials, [RackRoad, TankFarm, ColumnYard, Substation]);
        let boxes: Vec<ZoneId> = MapId::Refinery.box_locations().map(|i| BOX_LOCATIONS[i].0).collect();
        assert_eq!(boxes, [TankFarm, RackRoad, FittingShop, CompressorHouse, ColumnYard, Substation]);
        let zones_of = |zone: ZoneId| MapId::Refinery.barriers().filter(|b| WINDOWS[b.0 as usize].0 == zone).count();
        for (zone, count) in [(Gatehouse, 2), (TankerDock, 3), (FittingShop, 3), (FlareYard, 3), (Substation, 3), (TankFarm, 2), (CompressorHouse, 2), (ColumnYard, 2)] {
            assert_eq!(zones_of(zone), count, "{}", zone.name());
        }
        for zone in [TankWalk, ColumnPlatform, ControlHouse] {
            assert_eq!(zones_of(zone), 2, "{}", zone.name());
        }
        for zone in [RackWalk, PipeBridge, TowerDeck] {
            assert_eq!(zones_of(zone), 1, "{}", zone.name());
        }
        assert_eq!(zones_of(RackRoad), 0);
        // Every lamp: 28 sodium posts, 17 catwalk lamps and 42 indoor ones.
        let lamps_in = |zones: &[ZoneId]| layout.lamps.iter().filter(|(zone, _)| zones.contains(zone)).count();
        assert_eq!(lamps_in(&[Gatehouse, TankerDock, TankFarm, RackRoad, FlareYard, ColumnYard, Substation]), 28);
        assert_eq!(lamps_in(&[RackWalk, TankWalk, PipeBridge, ColumnPlatform, TowerDeck]), 17);
        assert_eq!(lamps_in(&[FittingShop, CompressorHouse, ControlHouse]), 42);
        // Every Hellhound, Special and Riser Spawn Point stands in the Zone it serves.
        for &(zone, at) in layout.hellhound_spawns.iter().chain(layout.special_spawns).chain(layout.risers) {
            assert_eq!(crate::enemies::nav::zone_at(MapId::Refinery, at + Vec3::Y * 0.1), Some(zone), "a Spawn Point at {at}");
        }
    }

    /// The nav graph and the windows: every window has its one Barrier edge, and every node is on the graph.
    #[test]
    fn refinery_graph_has_a_window_edge_for_every_barrier() {
        let graph = NavGraph::new(MapId::Refinery);
        for barrier in MapId::Refinery.barriers() {
            let edges: Vec<_> = graph.edges.iter().filter(|e| e.gate == crate::enemies::nav::Gate::Barrier(barrier)).collect();
            assert_eq!(edges.len(), 1, "window {}", barrier.0 - W0);
        }
    }
}
