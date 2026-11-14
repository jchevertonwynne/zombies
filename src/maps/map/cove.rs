//! The Cove Map: a cliffside town on four levels, a storey apart, climbed by stairs from the beach
//! to a lighthouse on the cliff top. The sea lies to the south (-z) and the cliff climbs north.
//! In this first slice the beach is dry sand, and there are no Ziplines.
//!
//! - The Shore (y 0): the Sea Cave, the Shingle, the Strand and the Boathouse, side by side along
//!   the sea wall. A Run starts on the Strand.
//! - The Lower Town (y 4): the Washhouse, the Lower Lane, the Fish Market and the Taverna.
//! - The Upper Town (y 8): the Chapel, the Piazza, the Upper Lane and the Pensione.
//! - The Headland (y 12): the Lighthouse, the Lighthouse Yard, the Keeper's Cottage and the Battery.
//!
//! Seven flights of stairs climb from level to level. Each is owned by the Zone at its top: a
//! sunken well in that Zone's floor, a storey deep, with the flight rising out of it and its Door
//! at the foot, in the wall between the two levels. Five drops run beside five of those Doors, and
//! each is a one-way path-node edge gated on the Door beside it (`Gate::DoorDrop`), never a fixed
//! join, so a drop never wakes a Zone no Player has opened. Three towers (the Lighthouse, the
//! Chapel's bell tower and the Pensione) climb on from their Zone's floor to a lookout.
//!
//! The Power Switch is at the foot of the Lighthouse. Pack-a-Punch is in the Sea Cave, behind the
//! Power Door from the Shingle. Zombies come in only through Barriers: every window opens onto a
//! walled pocket of floor outside it.
//!
//! The whole Map stands `COVE_X` along x from the world's middle, clear of the others.

use bevy::prelude::*;

use crate::enemies::nav::Gate;
use crate::enemies::sapper::SapperTiming;
use crate::maps::map::BarrierId;
use crate::maps::zone::ZoneId;
use crate::tuning::*;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{Layout, Prop, PropKind, RaisedFloor, STOREY, Slope, Sunken, door, palette, prop};

/// How far along x the Cove stands from the world's middle, west of the Village.
pub const COVE_X: f32 = -400.0;
const C: f32 = COVE_X;

/// The four levels.
const SHORE: f32 = 0.0;
const LOWER: f32 = STOREY;
const UPPER: f32 = 2.0 * STOREY;
const HEAD: f32 = 3.0 * STOREY;

/// The first of the Cove's windows in `WINDOWS`, after the other Maps'.
const W0: u8 = (super::greybox::WINDOWS.len() + super::quarry::WINDOWS.len() + super::village::WINDOWS.len() + super::dam::WINDOWS.len()) as u8;

/// How high a chalk outline's centre sits over its floor.
const CHALK: f32 = 1.5;

/// How wide the stairs are: the three with a Trap at their foot are as wide as the Dam's.
const FLIGHT: f32 = 4.0;
const TRAP_FLIGHT: f32 = 5.6;
/// How wide a flight inside a tower is.
const TOWER_FLIGHT: f32 = 3.6;
/// How far from its Door a trapped flight starts to climb, so the switches have a landing.
const LANDING: f32 = 3.0;
/// How long a flight of stairs is, for a storey.
const RUN: f32 = 8.0;

/// A point in the Cove, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(C + x, y, z)
}

/// A spot on a wall or a floor's plan in the Cove, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(C + x, z)
}

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

const fn p(kind: PropKind, zone: ZoneId, x: f32, y: f32, z: f32, size: (f32, f32, f32)) -> Prop {
    prop(kind, zone, (C + x, y, z), size)
}

pub(super) const COVE: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [Strand, SeaCave, Shingle, Boathouse, Washhouse, LowerLane, FishMarket, Taverna, Chapel, Piazza, UpperLane, Pensione, Lighthouse, LighthouseYard, KeepersCottage, Battery]
    },
    // Along the sea wall of the Strand, in a row facing Quick Revive.
    seats: [at(-5.5, SHORE, -8.0), at(-2.5, SHORE, -8.0), at(0.5, SHORE, -8.0), at(3.5, SHORE, -8.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &[],
    raised_floors: &RAISED_FLOORS,
    slopes: &SLOPES,
    sunken: &WELLS,
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &[],
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    sun: Vec3::new(1.0, -3.0, -1.0),
    look: crate::maps::look::NIGHT,
    plan: (Vec2::new(C - 70.0, -24.0), Vec2::new(C + 70.0, 72.0)),
    build,
};

/// The chalk outlines: the cheap guns either side of Quick Revive in the Strand's sea wall, an SMG
/// in the Lower Lane, a shotgun in the Boathouse and the AK-74u in the Upper Lane.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    (WeaponId::M14, ZoneId::Strand, at(-5.0, SHORE + CHALK, -13.8), Vec2::Y),
    (WeaponId::Olympia, ZoneId::Strand, at(3.0, SHORE + CHALK, -13.8), Vec2::Y),
    (WeaponId::Mp40, ZoneId::LowerLane, at(-32.0, LOWER + CHALK, 19.8), Vec2::NEG_Y),
    (WeaponId::TrenchGun, ZoneId::Boathouse, at(33.8, SHORE + CHALK, -6.0), Vec2::NEG_X),
    (WeaponId::Ak74u, ZoneId::UpperLane, at(17.8, UPPER + CHALK, 40.0), Vec2::NEG_X),
];

/// The Box Locations, in order. Every Run starts the Box at the first, in the Taverna, two Doors
/// from the Strand either way round. None is on the Shore, in the Sea Cave or in the Lighthouse.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // Against the Taverna's west wall, south of its Door to the Fish Market.
    (ZoneId::Taverna, at(6.65, LOWER, 9.0), Vec2::X),
    // Against the Washhouse's east wall.
    (ZoneId::Washhouse, at(-44.65, LOWER, 19.5), Vec2::NEG_X),
    // Against the Piazza's north wall, beside Juggernog.
    (ZoneId::Piazza, at(-12.0, UPPER, 39.35), Vec2::NEG_Y),
    // Against the Pensione's west wall, south of its Door.
    (ZoneId::Pensione, at(18.65, UPPER, 37.5), Vec2::X),
    // Against the Keeper's Cottage's south wall.
    (ZoneId::KeepersCottage, at(-3.0, HEAD, 44.65), Vec2::Y),
    // Against the Battery's west wall, north of its Door.
    (ZoneId::Battery, at(6.65, HEAD, 59.5), Vec2::X),
];

/// The 27 windows, each opening onto a pocket of floor walled in outside it: storm-drain grilles
/// and huts in the sea wall, house windows over the lanes and the Piazza, the embrasures and
/// the cottage's windows on the Headland.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 27] = [
    // 0-3: the sea wall.
    (ZoneId::Shingle, at(-34.0, SHORE, -14.0), Vec2::NEG_Y),
    (ZoneId::Strand, at(-14.0, SHORE, -14.0), Vec2::NEG_Y),
    (ZoneId::Strand, at(10.0, SHORE, -14.0), Vec2::NEG_Y),
    (ZoneId::Boathouse, at(27.0, SHORE, -14.0), Vec2::NEG_Y),
    // 4-10: the Lower Town.
    (ZoneId::Washhouse, at(-56.0, LOWER, 14.0), Vec2::NEG_X),
    (ZoneId::LowerLane, at(-38.0, LOWER, 20.0), Vec2::Y),
    (ZoneId::LowerLane, at(-26.0, LOWER, 20.0), Vec2::Y),
    (ZoneId::FishMarket, at(-1.0, LOWER, 24.0), Vec2::Y),
    (ZoneId::FishMarket, at(4.0, LOWER, 24.0), Vec2::Y),
    (ZoneId::Taverna, at(24.0, LOWER, 20.0), Vec2::Y),
    (ZoneId::Taverna, at(30.0, LOWER, 20.0), Vec2::Y),
    // 11-19: the Upper Town.
    (ZoneId::Chapel, at(-56.0, UPPER, 28.0), Vec2::NEG_X),
    (ZoneId::Chapel, at(-56.0, UPPER, 33.0), Vec2::NEG_X),
    (ZoneId::Piazza, at(-27.0, UPPER, 40.0), Vec2::Y),
    (ZoneId::Piazza, at(-22.0, UPPER, 40.0), Vec2::Y),
    (ZoneId::Piazza, at(-8.0, UPPER, 40.0), Vec2::Y),
    (ZoneId::UpperLane, at(9.0, UPPER, 24.0), Vec2::NEG_Y),
    (ZoneId::UpperLane, at(15.0, UPPER, 24.0), Vec2::NEG_Y),
    (ZoneId::Pensione, at(24.0, UPPER, 40.0), Vec2::Y),
    (ZoneId::Pensione, at(30.0, UPPER, 40.0), Vec2::Y),
    // 20-26: the Headland.
    (ZoneId::Lighthouse, at(-50.0, HEAD, 64.0), Vec2::Y),
    (ZoneId::LighthouseYard, at(-30.0, HEAD, 64.0), Vec2::Y),
    (ZoneId::LighthouseYard, at(-22.0, HEAD, 64.0), Vec2::Y),
    (ZoneId::KeepersCottage, at(-7.0, HEAD, 64.0), Vec2::Y),
    (ZoneId::KeepersCottage, at(1.0, HEAD, 64.0), Vec2::Y),
    (ZoneId::Battery, at(20.0, HEAD, 64.0), Vec2::Y),
    (ZoneId::Battery, at(28.0, HEAD, 64.0), Vec2::Y),
];

/// The Hellhound Spawn Points: one in every Zone, and two each on the Strand, the Piazza and the
/// Lighthouse Yard.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 19] = [
    (ZoneId::Strand, at(-8.0, SHORE, 0.0)),
    (ZoneId::Strand, at(10.0, SHORE, 0.0)),
    (ZoneId::SeaCave, at(-47.5, SHORE, 0.0)),
    (ZoneId::Shingle, at(-30.0, SHORE, 0.0)),
    (ZoneId::Boathouse, at(24.0, SHORE, 0.0)),
    (ZoneId::Washhouse, at(-50.0, LOWER, 12.0)),
    (ZoneId::LowerLane, at(-28.0, LOWER, 11.5)),
    (ZoneId::FishMarket, at(-5.0, LOWER, 10.5)),
    (ZoneId::Taverna, at(13.0, LOWER, 12.5)),
    (ZoneId::Chapel, at(-44.0, UPPER, 31.0)),
    (ZoneId::Piazza, at(-26.5, UPPER, 35.0)),
    (ZoneId::Piazza, at(-8.0, UPPER, 27.5)),
    (ZoneId::UpperLane, at(3.0, UPPER, 33.0)),
    (ZoneId::Pensione, at(21.5, UPPER, 36.0)),
    (ZoneId::Lighthouse, at(-47.0, HEAD, 53.0)),
    (ZoneId::LighthouseYard, at(-24.0, HEAD, 52.0)),
    (ZoneId::LighthouseYard, at(-17.0, HEAD, 57.0)),
    (ZoneId::KeepersCottage, at(-4.0, HEAD, 58.0)),
    (ZoneId::Battery, at(17.5, HEAD, 52.0)),
];

/// The Special Spawn Points, where a Napalm or a Shrieker rises: the Strand, the Piazza, the
/// Lighthouse Yard and the Battery.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 4] = [
    (ZoneId::Strand, at(0.0, SHORE, 0.0)),
    (ZoneId::Piazza, at(-26.5, UPPER, 35.0)),
    (ZoneId::LighthouseYard, at(-24.0, HEAD, 52.0)),
    (ZoneId::Battery, at(17.5, HEAD, 52.0)),
];

/// The lamps: one under every roof, and a few along the lanes and the sea wall.
const LAMPS: [(ZoneId, Vec3); 17] = [
    (ZoneId::Strand, at(-10.0, SHORE + 3.4, -12.0)),
    (ZoneId::Strand, at(8.0, SHORE + 3.4, -12.0)),
    (ZoneId::SeaCave, at(-50.0, SHORE + 3.4, -6.0)),
    (ZoneId::Shingle, at(-33.0, SHORE + 3.4, 4.0)),
    (ZoneId::Boathouse, at(27.0, SHORE + 3.4, -4.0)),
    (ZoneId::Washhouse, at(-50.0, LOWER + 3.4, 14.0)),
    (ZoneId::LowerLane, at(-30.0, LOWER + 3.4, 18.0)),
    (ZoneId::FishMarket, at(-6.0, LOWER + 3.4, 14.0)),
    (ZoneId::Taverna, at(20.0, LOWER + 3.4, 14.0)),
    (ZoneId::Chapel, at(-40.0, UPPER + 3.4, 32.0)),
    (ZoneId::Piazza, at(-21.0, UPPER + 3.4, 32.0)),
    (ZoneId::UpperLane, at(7.0, UPPER + 3.4, 38.0)),
    (ZoneId::Pensione, at(23.0, UPPER + 3.4, 33.0)),
    (ZoneId::Lighthouse, at(-48.0, HEAD + 3.4, 52.0)),
    (ZoneId::LighthouseYard, at(-26.0, HEAD + 3.4, 54.0)),
    (ZoneId::KeepersCottage, at(-3.0, HEAD + 3.4, 54.0)),
    (ZoneId::Battery, at(20.0, HEAD + 3.4, 54.0)),
];

/// Rock, for the Sea Cave's shelf and the stairs; pale stone for the platforms; and flagstone for
/// the towers' landings.
const ROCK: Color = Color::srgb(0.3, 0.32, 0.34);
const STONE: Color = Color::srgb(0.66, 0.62, 0.54);
const FLAG: Color = Color::srgb(0.5, 0.48, 0.44);
const WHITE: Color = Color::srgb(0.92, 0.92, 0.9);

/// The Sea Cave's shelf of rock, the Pack-a-Punch's: steps up at its north end.
const SHELF: RaisedFloor = RaisedFloor { zone: ZoneId::SeaCave, min: v(-53.5, -11.0), max: v(-47.0, -3.0), height: SHORE + 0.6, colour: ROCK };
/// The Fish Market's seaward platform, a step up from the loggia floor, with the drop off its front.
const SEAWARD: RaisedFloor = RaisedFloor { zone: ZoneId::FishMarket, min: v(-2.0, 6.0), max: v(5.8, 11.0), height: LOWER + 0.5, colour: STONE };
/// The Battery's gun platform, with its ramp.
const GUNS: RaisedFloor = RaisedFloor { zone: ZoneId::Battery, min: v(22.0, 54.0), max: v(32.0, 61.0), height: HEAD + 1.0, colour: STONE };
/// The Lighthouse's landing at the top of its first flight, and its lantern gallery.
const LIGHT_LANDING: RaisedFloor = RaisedFloor { zone: ZoneId::Lighthouse, min: v(-55.8, 46.0), max: v(-52.0, 59.4), height: HEAD + STOREY, colour: WHITE };
const GALLERY: RaisedFloor = RaisedFloor { zone: ZoneId::Lighthouse, min: v(-44.0, 55.6), max: v(-40.2, 63.8), height: HEAD + 2.0 * STOREY, colour: WHITE };
/// The Chapel's bell tower: the landing at the top of its first flight, and the belfry.
const BELL_LANDING: RaisedFloor = RaisedFloor { zone: ZoneId::Chapel, min: v(-55.8, 36.4), max: v(-53.0, 43.8), height: UPPER + STOREY, colour: FLAG };
const BELFRY: RaisedFloor = RaisedFloor { zone: ZoneId::Chapel, min: v(-45.0, 40.2), max: v(-40.2, 43.8), height: UPPER + 2.0 * STOREY, colour: FLAG };
/// The Pensione's roof garden, up its flight along the south wall.
const GARDEN: RaisedFloor = RaisedFloor { zone: ZoneId::Pensione, min: v(28.0, 24.2), max: v(33.8, 37.0), height: UPPER + STOREY, colour: STONE };

const RAISED_FLOORS: [RaisedFloor; 8] = [SHELF, SEAWARD, GUNS, LIGHT_LANDING, GALLERY, BELL_LANDING, BELFRY, GARDEN];

/// The stairwells: each flight's well in the floor of the Zone at its top, a storey deep.
const WELLS: [Sunken; 7] = [
    // Sea Wall Steps, Strand to Fish Market.
    Sunken { zone: ZoneId::FishMarket, min: v(-12.0, 6.0), max: v(-8.0, 6.2 + RUN), depth: STOREY, rim: false },
    // Boathouse Stair, up to the Taverna's cellar.
    Sunken { zone: ZoneId::Taverna, min: v(25.0, 6.0), max: v(29.0, 6.2 + RUN), depth: STOREY, rim: false },
    // Fishermen's Steps, Shingle to Lower Lane.
    Sunken { zone: ZoneId::LowerLane, min: v(-35.0, 6.0), max: v(-31.0, 6.2 + RUN), depth: STOREY, rim: false },
    // Long Stair, Fish Market to Piazza.
    Sunken { zone: ZoneId::Piazza, min: v(-14.8, 24.0), max: v(-9.2, 24.0 + LANDING + RUN), depth: STOREY, rim: false },
    // Chapel Stair, Washhouse to Chapel.
    Sunken { zone: ZoneId::Chapel, min: v(-52.8, 24.0), max: v(-47.2, 24.0 + LANDING + RUN), depth: STOREY, rim: false },
    // Pilgrims' Stair, Chapel to Lighthouse Yard.
    Sunken { zone: ZoneId::LighthouseYard, min: v(-37.0, 44.0), max: v(-33.0, 44.2 + RUN), depth: STOREY, rim: false },
    // Cliff Stair, Upper Lane to Battery.
    Sunken { zone: ZoneId::Battery, min: v(9.2, 44.0), max: v(14.8, 44.0 + LANDING + RUN), depth: STOREY, rim: false },
];

/// The stairs. The seven between levels climb out of their wells; the others lead up to the Pack-a-
/// Punch's shelf, the Fish Market's platform and the Battery's guns, and up the three towers.
const SLOPES: [Slope; 15] = [
    Slope { zone: ZoneId::FishMarket, foot: at(-10.0, SHORE, 6.2), up: Vec2::Y, run: RUN, rise: STOREY, width: FLIGHT, steps: true, colour: ROCK },
    Slope { zone: ZoneId::Taverna, foot: at(27.0, SHORE, 6.2), up: Vec2::Y, run: RUN, rise: STOREY, width: FLIGHT, steps: true, colour: ROCK },
    Slope { zone: ZoneId::LowerLane, foot: at(-33.0, SHORE, 6.2), up: Vec2::Y, run: RUN, rise: STOREY, width: FLIGHT, steps: true, colour: ROCK },
    Slope { zone: ZoneId::Piazza, foot: at(-12.0, LOWER, 24.0 + LANDING), up: Vec2::Y, run: RUN, rise: STOREY, width: TRAP_FLIGHT, steps: true, colour: ROCK },
    Slope { zone: ZoneId::Chapel, foot: at(-50.0, LOWER, 24.0 + LANDING), up: Vec2::Y, run: RUN, rise: STOREY, width: TRAP_FLIGHT, steps: true, colour: ROCK },
    Slope { zone: ZoneId::LighthouseYard, foot: at(-35.0, UPPER, 44.2), up: Vec2::Y, run: RUN, rise: STOREY, width: FLIGHT, steps: true, colour: ROCK },
    Slope { zone: ZoneId::Battery, foot: at(12.0, UPPER, 44.0 + LANDING), up: Vec2::Y, run: RUN, rise: STOREY, width: TRAP_FLIGHT, steps: true, colour: ROCK },
    // The Lighthouse: up its south lane, and back along its north lane to the lantern gallery.
    Slope { zone: ZoneId::Lighthouse, foot: at(-44.0, HEAD, 47.8), up: Vec2::NEG_X, run: RUN, rise: STOREY, width: TOWER_FLIGHT, steps: true, colour: WHITE },
    Slope { zone: ZoneId::Lighthouse, foot: at(-52.0, HEAD + STOREY, 57.8), up: Vec2::X, run: RUN, rise: STOREY, width: TOWER_FLIGHT, steps: true, colour: WHITE },
    // The bell tower, the same way.
    Slope { zone: ZoneId::Chapel, foot: at(-45.0, UPPER, 38.2), up: Vec2::NEG_X, run: RUN, rise: STOREY, width: TOWER_FLIGHT, steps: true, colour: FLAG },
    Slope { zone: ZoneId::Chapel, foot: at(-53.0, UPPER + STOREY, 42.0), up: Vec2::X, run: RUN, rise: STOREY, width: TOWER_FLIGHT, steps: true, colour: FLAG },
    // The Pensione's, up to its roof garden.
    Slope { zone: ZoneId::Pensione, foot: at(20.0, UPPER, 26.0), up: Vec2::X, run: RUN, rise: STOREY, width: TOWER_FLIGHT, steps: true, colour: STONE },
    // Up onto the Pack-a-Punch's shelf, the Fish Market's platform and the Battery's guns.
    Slope { zone: ZoneId::SeaCave, foot: at(-50.25, SHORE, -1.8), up: Vec2::NEG_Y, run: 1.2, rise: 0.6, width: 3.0, steps: true, colour: ROCK },
    Slope { zone: ZoneId::FishMarket, foot: at(1.9, LOWER, 12.2), up: Vec2::NEG_Y, run: 1.2, rise: 0.5, width: 3.0, steps: true, colour: STONE },
    Slope { zone: ZoneId::Battery, foot: at(27.0, HEAD, 50.4), up: Vec2::Y, run: 3.6, rise: 1.0, width: 3.0, steps: true, colour: STONE },
];

/// The Props. Most stand flush against a wall; the islands to run round are the Piazza's fountain
/// and the Fish Market's slabs.
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Strand: upturned fishing boats and net-drying frames along the sea wall and the ends.
        p(Counter, Strand, -18.0, SHORE, -9.0, (4.0, 1.4, 1.8)),
        p(Counter, Strand, 14.0, SHORE, -8.5, (4.0, 1.4, 1.8)),
        p(Shelf, Strand, -21.5, SHORE, 1.5, (0.4, 2.2, 3.0)),
        p(Shelf, Strand, 19.6, SHORE, -9.0, (0.4, 2.2, 3.0)),
        // The Shingle: rock outcrops and a lobster pot.
        p(Rubble, Shingle, -38.0, SHORE, -8.0, (1.6, 1.5, 1.6)),
        p(Rubble, Shingle, -28.0, SHORE, -8.0, (1.6, 1.5, 1.6)),
        p(Crate, Shingle, -37.0, SHORE, 1.0, (1.0, 1.0, 1.0)),
        // The Boathouse: an oar rack and a crate of buoys.
        p(Shelf, Boathouse, 33.4, SHORE, 2.0, (0.8, 2.2, 3.0)),
        p(Crate, Boathouse, 21.2, SHORE, -11.0, (1.0, 1.0, 1.0)),
        // The Sea Cave: stalagmites flanking the Pack-a-Punch, and the smugglers' crates.
        p(Pillar, SeaCave, -52.5, SHORE + 0.6, -8.0, (0.9, 2.5, 0.9)),
        p(Pillar, SeaCave, -48.0, SHORE + 0.6, -8.0, (0.9, 2.5, 0.9)),
        p(Crate, SeaCave, -54.9, SHORE, 5.0, (1.8, 1.6, 1.8)),
        // The Washhouse: scrubbing stones.
        p(Counter, Washhouse, -55.2, LOWER, 8.5, (1.0, 1.3, 3.0)),
        // The Lower Lane: a parked moped, crates and a house's flower pots.
        p(Crate, LowerLane, -24.5, LOWER, 7.5, (1.2, 1.2, 1.2)),
        p(Shelf, LowerLane, -43.5, LOWER, 17.5, (0.8, 2.2, 2.5)),
        // The Fish Market: marble slabs, and ice crates.
        p(Counter, FishMarket, -6.0, LOWER, 19.5, (2.4, 1.0, 1.2)),
        p(Counter, FishMarket, -6.0, LOWER, 12.5, (2.4, 1.0, 1.2)),
        p(Crate, FishMarket, -18.8, LOWER, 19.0, (1.2, 1.2, 1.2)),
        // The Taverna: a bar, a table and wine barrels.
        p(Counter, Taverna, 9.0, LOWER, 17.5, (1.4, 1.2, 3.0)),
        p(Table, Taverna, 18.0, LOWER, 7.0, (1.6, 0.9, 1.0)),
        p(Barrel, Taverna, 33.2, LOWER, 7.5, (0.9, 1.0, 0.9)),
        // The Chapel: pews, and the altar.
        p(Bench, Chapel, -36.0, UPPER, 37.5, (3.0, 0.9, 0.7)),
        p(Bench, Chapel, -36.5, UPPER, 26.5, (3.0, 0.9, 0.7)),
        p(Altar, Chapel, -32.0, UPPER, 38.5, (1.6, 1.0, 2.4)),
        // The Piazza: the fountain, about 5 m across, and the war memorial.
        p(Barrel, Piazza, -21.0, UPPER, 32.0, (5.0, 1.4, 5.0)),
        p(Pillar, Piazza, -14.0, UPPER, 37.8, (0.9, 2.6, 0.9)),
        // The Upper Lane: stacked crates and a handcart.
        p(Crate, UpperLane, -2.9, UPPER, 39.0, (1.6, 1.4, 1.6)),
        p(Counter, UpperLane, 11.0, UPPER, 36.0, (1.2, 1.0, 2.0)),
        // The Pensione: planters on the roof garden.
        p(Crate, Pensione, 33.0, UPPER + STOREY, 28.0, (1.2, 0.9, 1.2)),
        p(Crate, Pensione, 33.0, UPPER + STOREY, 36.0, (1.2, 0.9, 1.2)),
        // The Lighthouse Yard: oil drums, a flagpole and a fog bell on a frame.
        p(Barrel, LighthouseYard, -39.2, HEAD, 58.2, (0.9, 1.1, 0.9)),
        p(Barrel, LighthouseYard, -39.2, HEAD, 60.2, (0.9, 1.1, 0.9)),
        p(Pillar, LighthouseYard, -14.0, HEAD, 46.0, (0.5, 6.0, 0.5)),
        p(Shelf, LighthouseYard, -26.0, HEAD, 44.5, (2.4, 2.2, 0.5)),
        // The Keeper's Cottage: a kitchen table and a bunk.
        p(Table, KeepersCottage, 2.5, HEAD, 59.0, (1.6, 0.9, 1.0)),
        p(Counter, KeepersCottage, 4.8, HEAD, 48.5, (1.0, 1.3, 2.0)),
        // The Battery: two old cannon on the gun platform, and a stack of shot.
        p(Machine, Battery, 24.5, HEAD + 1.0, 58.5, (1.2, 1.1, 3.0)),
        p(Machine, Battery, 29.5, HEAD + 1.0, 58.5, (1.2, 1.1, 3.0)),
        p(Sandbags, Battery, 13.0, HEAD, 61.0, (2.4, 1.0, 0.8)),
    ]
};

/// A stretch of wall along x at `z`, from `x0` to `x1`, with gaps at absolute `x`.
fn xwall(b: &mut Builder, (x0, x1): (f32, f32), z: f32, y0: f32, height: f32, gaps: Vec<(f32, Gap)>) {
    b.wall(v(x0, z), v(x1, z), y0, height, &gaps.into_iter().map(|(x, gap)| (x - x0, gap)).collect::<Vec<_>>());
}

/// A stretch of wall along z at `x`, from `z0` to `z1`, with gaps at absolute `z`.
fn zwall(b: &mut Builder, x: f32, (z0, z1): (f32, f32), y0: f32, height: f32, gaps: Vec<(f32, Gap)>) {
    b.wall(v(x, z0), v(x, z1), y0, height, &gaps.into_iter().map(|(z, gap)| (z - z0, gap)).collect::<Vec<_>>());
}

/// A roof over `min`..`max`, a plain slab on top of the walls at `y`.
fn roof(b: &mut Builder, zone: ZoneId, min: Vec2, max: Vec2, y: f32) {
    let (size, mid) = (max - min, (min + max) / 2.0);
    b.block(Vec3::new(size.x, 0.3, size.y), Vec3::new(mid.x, y + 0.15, mid.y), palette(Some(zone)).trim);
}

/// A rail along the edge of a lookout, `min`..`max`, standing on `y`.
fn rail(b: &mut Builder, min: Vec2, max: Vec2, y: f32, colour: Color) {
    let (size, mid) = (max - min, (min + max) / 2.0);
    b.block(Vec3::new(size.x.max(0.2), 1.6, size.y.max(0.2)), Vec3::new(mid.x, y + 0.8, mid.y), colour);
}

/// A pocket of floor outside the window at `centre` looking `out`, walled in, where Zombies gather.
fn pocket(b: &mut Builder, centre: Vec3, out: Vec2) {
    super::dam::pocket(b, centre, out);
}

/// The walls and Doors of the four levels, the roofs and the pockets outside the windows.
fn build(b: &mut Builder) {
    use ZoneId::*;
    const H: f32 = STOREY;
    // The Shore: the sea wall with its windows, the cliffs either end, and the walls between.
    xwall(
        b,
        (-56.0, 34.0),
        -14.0,
        SHORE,
        H,
        vec![(-34.0, Gap::Window), (-14.0, Gap::Window), (10.0, Gap::Window), (27.0, Gap::Window)],
    );
    zwall(b, -56.0, (-14.0, 6.0), SHORE, H, vec![]);
    zwall(b, 34.0, (-14.0, 6.0), SHORE, H, vec![]);
    zwall(b, -44.0, (-14.0, 6.0), SHORE, H, vec![(-4.0, Gap::Door(Some((SeaCave, Shingle)), None))]);
    zwall(b, -22.0, (-14.0, 6.0), SHORE, H, vec![(-4.0, door(Shingle, Strand, DOOR_PRICE_STRAND_SHINGLE))]);
    zwall(b, 20.0, (-14.0, 6.0), SHORE, H, vec![(-4.0, door(Strand, Boathouse, DOOR_PRICE_STRAND_BOATHOUSE))]);
    // The wall up to the Lower Town, with the foot of three flights of stairs in it.
    xwall(
        b,
        (-56.0, 34.0),
        6.0,
        SHORE,
        H,
        vec![
            (-33.0, door(Shingle, LowerLane, DOOR_PRICE_SHINGLE_LOWER_LANE)),
            (-10.0, door(Strand, FishMarket, DOOR_PRICE_STRAND_FISH_MARKET)),
            (27.0, door(Boathouse, Taverna, DOOR_PRICE_BOATHOUSE_TAVERNA)),
        ],
    );
    roof(b, SeaCave, v(-56.0, -14.0), v(-44.0, 6.0), SHORE + H);
    roof(b, Boathouse, v(20.0, -14.0), v(34.0, 6.0), SHORE + H);

    // The Lower Town: the front wall over the one below, the ends, and the walls between.
    xwall(b, (-56.0, -44.0), 6.0, LOWER, H, vec![]);
    xwall(b, (-44.0, -42.0), 6.0, LOWER, H, vec![]);
    // The Lower Lane's west end is a lip over the Shingle.
    xwall(b, (-38.0, -20.0), 6.0, LOWER, H, vec![]);
    xwall(b, (-20.0, -2.0), 6.0, LOWER, H, vec![]);
    // The Fish Market's seaward platform is open along its front.
    xwall(b, (6.0, 34.0), 6.0, LOWER, H, vec![]);
    zwall(b, -56.0, (6.0, 24.0), LOWER, H, vec![(14.0, Gap::Window)]);
    zwall(b, -44.0, (6.0, 24.0), LOWER, H, vec![(13.0, door(LowerLane, Washhouse, DOOR_PRICE_LOWER_LANE_WASHHOUSE))]);
    zwall(b, -20.0, (6.0, 24.0), LOWER, H, vec![(13.0, door(FishMarket, LowerLane, DOOR_PRICE_FISH_MARKET_LOWER_LANE))]);
    zwall(b, 6.0, (6.0, 24.0), LOWER, H, vec![(13.0, door(FishMarket, Taverna, DOOR_PRICE_FISH_MARKET_TAVERNA))]);
    zwall(b, 34.0, (6.0, 20.0), LOWER, H, vec![]);
    xwall(b, (-44.0, -20.0), 20.0, LOWER, H, vec![(-38.0, Gap::Window), (-26.0, Gap::Window)]);
    xwall(b, (6.0, 34.0), 20.0, LOWER, H, vec![(24.0, Gap::Window), (30.0, Gap::Window)]);
    // The wall up to the Upper Town, with the foot of two flights of stairs in it and the Fish
    // Market's windows over the Upper Lane's cellars.
    xwall(
        b,
        (-56.0, 34.0),
        24.0,
        LOWER,
        H,
        vec![
            (-50.0, door(Washhouse, Chapel, DOOR_PRICE_WASHHOUSE_CHAPEL)),
            (-12.0, door(FishMarket, Piazza, DOOR_PRICE_FISH_MARKET_PIAZZA)),
            (-1.0, Gap::Window),
            (4.0, Gap::Window),
        ],
    );
    roof(b, Washhouse, v(-56.0, 6.0), v(-44.0, 24.0), LOWER + H);
    roof(b, Taverna, v(6.0, 6.0), v(34.0, 20.0), LOWER + H);

    // The Upper Town.
    xwall(b, (-56.0, -30.0), 24.0, UPPER, H, vec![]);
    // The Piazza's balustrade has a gap over the Fish Market.
    xwall(b, (-30.0, -20.0), 24.0, UPPER, H, vec![]);
    xwall(b, (-16.0, -4.0), 24.0, UPPER, H, vec![]);
    xwall(b, (-4.0, 18.0), 24.0, UPPER, H, vec![(9.0, Gap::Window), (15.0, Gap::Window)]);
    xwall(b, (18.0, 34.0), 24.0, UPPER, H, vec![]);
    zwall(b, -56.0, (24.0, 44.0), UPPER, H, vec![(28.0, Gap::Window), (33.0, Gap::Window)]);
    zwall(b, -30.0, (24.0, 44.0), UPPER, H, vec![(32.0, door(Chapel, Piazza, DOOR_PRICE_CHAPEL_PIAZZA))]);
    zwall(b, -4.0, (24.0, 44.0), UPPER, H, vec![(32.0, door(Piazza, UpperLane, DOOR_PRICE_PIAZZA_UPPER_LANE))]);
    zwall(b, 18.0, (24.0, 44.0), UPPER, H, vec![(32.0, door(UpperLane, Pensione, DOOR_PRICE_UPPER_LANE_PENSIONE))]);
    zwall(b, 34.0, (24.0, 40.0), UPPER, H, vec![]);
    xwall(b, (-30.0, -4.0), 40.0, UPPER, H, vec![(-27.0, Gap::Window), (-22.0, Gap::Window), (-8.0, Gap::Window)]);
    xwall(b, (18.0, 34.0), 40.0, UPPER, H, vec![(24.0, Gap::Window), (30.0, Gap::Window)]);
    // The wall up to the Headland, with the foot of two flights of stairs in it.
    xwall(
        b,
        (-56.0, 34.0),
        44.0,
        UPPER,
        H,
        vec![
            (-35.0, door(Chapel, LighthouseYard, DOOR_PRICE_CHAPEL_LIGHTHOUSE_YARD)),
            (12.0, door(UpperLane, Battery, DOOR_PRICE_UPPER_LANE_BATTERY)),
        ],
    );
    // The bell tower is solid under its second flight, so nothing is lost beneath it. The Chapel
    // is open to the sky above the nave, which keeps the tower open and the drop clear.
    for (x0, x1, top) in [(-53.0, -50.0, 12.0), (-50.0, -48.0, 13.0), (-48.0, -46.0, 14.0), (-46.0, -45.0, 14.5)] {
        b.block(Vec3::new(x1 - x0, top - UPPER, 3.6), Vec3::new(C + (x0 + x1) / 2.0, (UPPER + top) / 2.0, 42.0), FLAG);
    }

    // The Headland: the lighthouse is a tower, tall enough for its lantern gallery.
    let tall = 2.0 * STOREY + 2.0;
    xwall(b, (-56.0, -40.0), 44.0, HEAD, tall, vec![]);
    xwall(b, (-40.0, -39.5), 44.0, HEAD, H, vec![]);
    // The Lighthouse Yard's lip over the Chapel's back court.
    xwall(b, (-37.5, -12.0), 44.0, HEAD, H, vec![]);
    xwall(b, (-12.0, 6.0), 44.0, HEAD, H, vec![]);
    xwall(b, (6.0, 15.5), 44.0, HEAD, H, vec![]);
    // The Battery's rampart has a gap over the Upper Lane.
    xwall(b, (17.5, 34.0), 44.0, HEAD, H, vec![]);
    zwall(b, -56.0, (44.0, 64.0), HEAD, tall, vec![]);
    zwall(b, -40.0, (44.0, 64.0), HEAD, tall, vec![(54.0, door(LighthouseYard, Lighthouse, DOOR_PRICE_YARD_LIGHTHOUSE))]);
    zwall(b, -12.0, (44.0, 64.0), HEAD, H, vec![(54.0, door(LighthouseYard, KeepersCottage, DOOR_PRICE_YARD_KEEPERS_COTTAGE))]);
    zwall(b, 6.0, (44.0, 64.0), HEAD, H, vec![(54.0, door(KeepersCottage, Battery, DOOR_PRICE_KEEPERS_COTTAGE_BATTERY))]);
    zwall(b, 34.0, (44.0, 64.0), HEAD, H, vec![]);
    xwall(b, (-56.0, -40.0), 64.0, HEAD, tall, vec![(-50.0, Gap::Window)]);
    xwall(b, (-40.0, -12.0), 64.0, HEAD, H, vec![(-30.0, Gap::Window), (-22.0, Gap::Window)]);
    xwall(b, (-12.0, 6.0), 64.0, HEAD, H, vec![(-7.0, Gap::Window), (1.0, Gap::Window)]);
    xwall(b, (6.0, 34.0), 64.0, HEAD, H, vec![(20.0, Gap::Window), (28.0, Gap::Window)]);
    roof(b, KeepersCottage, v(-12.0, 44.0), v(6.0, 64.0), HEAD + H);

    // A balustrade along each side of every stairwell, so that nothing walks over a well's rim
    // and back down onto its stairs: a Zombie at the top of a flight heading across the corner of
    // its well would otherwise keep stepping into it and out again.
    for well in &WELLS {
        let (floor, length) = (well.zone.floor(), well.max.y - well.min.y);
        let paint = palette(Some(well.zone)).trim;
        for x in [well.min.x + 0.1, well.max.x - 0.1] {
            b.block(Vec3::new(0.2, 1.1, length), Vec3::new(x, floor + 0.55, (well.min.y + well.max.y) / 2.0), paint);
        }
    }

    // Rails round the lookouts: the lantern gallery, the belfry and the roof garden.
    let rail_paint = palette(Some(Lighthouse)).trim;
    rail(b, v(-44.0, 55.4), v(-40.2, 55.6), GALLERY.height, rail_paint);
    rail(b, v(-44.2, 59.6), v(-44.0, 63.8), GALLERY.height, rail_paint);
    rail(b, v(-45.0, 39.8), v(-40.2, 40.0), BELFRY.height, rail_paint);
    rail(b, v(-40.4, 40.2), v(-40.2, 43.8), BELFRY.height, rail_paint);
    let garden_rail = palette(Some(Pensione)).trim;
    rail(b, v(28.0, 36.8), v(33.8, 37.0), GARDEN.height, garden_rail);
    rail(b, v(33.6, 24.2), v(33.8, 37.0), GARDEN.height, garden_rail);
    rail(b, v(28.0, 27.8), v(28.2, 37.0), GARDEN.height, garden_rail);

    // A pocket of floor outside every window, walled in, where Zombies gather.
    for &(_, centre, out) in &WINDOWS {
        pocket(b, centre, out);
    }
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // The Sea Cave, round the Pack-a-Punch's shelf
    ("sc_nw", C - 53.5, SHORE, 3.5),
    ("sc_ne", C - 47.0, SHORE, 3.5),
    ("sc_step", C - 50.25, SHORE, -0.5),
    ("sc_shelf", C - 50.25, SHORE + 0.6, -5.5),
    ("sc_shelf2", C - 50.25, SHORE + 0.6, -8.5),
    ("sc_w", C - 55.2, SHORE, -6.0),
    ("sc_sw", C - 55.0, SHORE, -12.6),
    ("sc_se", C - 46.0, SHORE, -12.6),
    ("sc_e", C - 45.6, SHORE, -4.0),
    // The Shingle
    ("sh_w", C - 42.5, SHORE, -4.0),
    ("sh_c", C - 33.0, SHORE, -4.0),
    ("sh_e", C - 23.5, SHORE, -4.0),
    ("sh_nw", C - 40.0, SHORE, 3.0),
    ("sh_n", C - 33.0, SHORE, 4.4),
    ("sh_ne", C - 27.0, SHORE, 3.0),
    ("w0_in", C - 34.0, SHORE, -12.4),
    ("w0_out", C - 34.0, SHORE, -16.0),
    ("sh_sw", C - 40.0, SHORE, -11.0),
    ("sh_se", C - 27.0, SHORE, -11.0),
    // The Strand
    ("st_w", C - 20.5, SHORE, -4.0),
    ("st_a", C - 12.0, SHORE, -4.0),
    ("st_b", C - 4.0, SHORE, -4.0),
    ("st_c", C + 4.0, SHORE, -4.0),
    ("st_d", C + 12.0, SHORE, -4.0),
    ("st_e", C + 18.5, SHORE, -4.0),
    ("st_nw", C - 17.0, SHORE, 3.0),
    ("st_n", C - 10.0, SHORE, 4.4),
    ("st_nm", C + 0.0, SHORE, 3.0),
    ("st_nc", C + 7.0, SHORE, 3.0),
    ("st_ne", C + 14.0, SHORE, 3.0),
    ("w1_in", C - 14.0, SHORE, -12.4),
    ("w1_out", C - 14.0, SHORE, -16.0),
    ("st_s1", C - 6.0, SHORE, -11.0),
    ("st_s2", C + 3.0, SHORE, -11.0),
    ("w2_in", C + 10.0, SHORE, -12.4),
    ("w2_out", C + 10.0, SHORE, -16.0),
    // The Boathouse
    ("bo_w", C + 21.8, SHORE, -4.0),
    ("bo_c", C + 27.0, SHORE, -4.0),
    ("bo_e", C + 32.0, SHORE, -4.0),
    ("bo_n", C + 27.0, SHORE, 4.4),
    ("bo_nw", C + 23.0, SHORE, 2.0),
    ("bo_ne", C + 31.0, SHORE, 2.0),
    ("w3_in", C + 27.0, SHORE, -12.4),
    ("w3_out", C + 27.0, SHORE, -16.0),
    ("bo_sw", C + 23.0, SHORE, -10.0),
    ("bo_se", C + 31.5, SHORE, -10.0),
    // The Washhouse
    ("wa_sw", C - 52.5, LOWER, 9.5),
    ("wa_se", C - 47.0, LOWER, 9.5),
    ("w4_in", C - 54.4, LOWER, 14.5),
    ("w4_out", C - 58.0, LOWER, 14.0),
    ("wa_e", C - 45.6, LOWER, 13.0),
    ("wa_nw", C - 53.0, LOWER, 19.5),
    ("wa_n", C - 50.0, LOWER, 21.8),
    ("wa_ne", C - 47.0, LOWER, 19.5),
    // The Lower Lane: its west lip over the Shingle, and up from the Fishermen's Steps
    ("ll_dtop", C - 40.0, LOWER, 7.3),
    ("ll_w", C - 42.2, LOWER, 13.0),
    ("w5_in", C - 38.0, LOWER, 18.4),
    ("w5_out", C - 38.0, LOWER, 22.0),
    ("ll_foot", C - 33.0, SHORE + 0.2, 6.6),
    ("ll_mid", C - 33.0, SHORE + 2.0, 10.2),
    ("ll_top", C - 33.0, LOWER, 15.2),
    ("w6_in", C - 26.0, LOWER, 18.4),
    ("w6_out", C - 26.0, LOWER, 22.0),
    ("ll_e", C - 21.8, LOWER, 13.0),
    ("ll_se", C - 26.0, LOWER, 9.5),
    // The Fish Market: up from the Strand, round the slabs, and the seaward platform
    ("fm_w", C - 18.2, LOWER, 13.0),
    ("fm_sw", C - 16.0, LOWER, 9.5),
    ("fm_foot", C - 10.0, SHORE + 0.2, 6.6),
    ("fm_mid", C - 10.0, SHORE + 2.0, 10.2),
    ("fm_top", C - 10.0, LOWER, 15.2),
    ("fm_a", C - 16.0, LOWER, 17.0),
    ("fm_nw", C - 17.0, LOWER, 21.3),
    ("fm_n", C - 12.0, LOWER, 21.6),
    ("fm_b", C - 3.0, LOWER, 17.0),
    ("fm_se", C - 5.5, LOWER, 8.5),
    ("w7_in", C - 1.0, LOWER, 22.4),
    ("w7_out", C - 1.0, LOWER, 26.0),
    ("w8_in", C + 4.0, LOWER, 22.4),
    ("w8_out", C + 4.0, LOWER, 26.0),
    ("fm_ps", C + 1.9, LOWER, 13.6),
    ("fm_plat", C + 0.5, LOWER + 0.5, 8.4),
    ("fm_e", C + 4.2, LOWER, 13.0),
    // The Taverna: up the Boathouse Stair, round the bar
    ("ta_w", C + 7.8, LOWER, 13.0),
    ("ta_s1", C + 12.0, LOWER, 9.5),
    ("ta_s2", C + 20.0, LOWER, 9.5),
    ("ta_s3", C + 31.5, LOWER, 9.5),
    ("tv_foot", C + 27.0, SHORE + 0.2, 6.6),
    ("tv_mid", C + 27.0, SHORE + 2.0, 10.2),
    ("tv_top", C + 27.0, LOWER, 15.2),
    ("ta_m1", C + 12.0, LOWER, 15.5),
    ("ta_m2", C + 20.0, LOWER, 15.5),
    ("ta_m3", C + 31.5, LOWER, 15.5),
    ("ta_n1", C + 16.0, LOWER, 18.4),
    ("w9_in", C + 24.0, LOWER, 18.4),
    ("w9_out", C + 24.0, LOWER, 22.0),
    ("w10_in", C + 30.0, LOWER, 18.4),
    ("w10_out", C + 30.0, LOWER, 22.0),
    // The Chapel: up the Chapel Stair, round the pews, and up the bell tower
    ("ch_lo", C - 50.0, LOWER, 25.6),
    ("ch_s1", C - 50.0, LOWER + 2.0, 31.0),
    ("ch_top", C - 50.0, UPPER, 35.7),
    ("w11_in", C - 54.4, UPPER, 28.0),
    ("w11_out", C - 58.0, UPPER, 28.0),
    ("w12_in", C - 54.4, UPPER, 33.0),
    ("w12_out", C - 58.0, UPPER, 33.0),
    ("ch_aw", C - 54.3, UPPER, 35.7),
    ("ch_sm", C - 44.0, UPPER, 27.0),
    ("ch_c", C - 38.0, UPPER, 30.0),
    ("ch_e", C - 31.8, UPPER, 32.0),
    ("ch_m", C - 42.0, UPPER, 35.0),
    ("ch_n", C - 35.0, UPPER, 42.2),
    ("ch_db", C - 38.5, UPPER, 41.4),
    ("ch_afoot", C - 43.8, UPPER, 38.2),
    ("ch_amid", C - 49.0, UPPER + 2.0, 38.2),
    ("ch_land", C - 54.4, UPPER + STOREY, 40.0),
    ("ch_bmid", C - 49.0, UPPER + 2.0 * STOREY - 2.0, 42.0),
    ("ch_bell", C - 43.5, UPPER + 2.0 * STOREY, 42.0),
    // The Piazza: round the fountain, up the Long Stair, and the balustrade's gap
    ("pr0", C - 16.8, UPPER, 32.0),
    ("pr1", C - 18.9, UPPER, 35.64),
    ("pr2", C - 23.1, UPPER, 35.64),
    ("pr3", C - 25.2, UPPER, 32.0),
    ("pr4", C - 23.1, UPPER, 28.36),
    ("pr5", C - 18.9, UPPER, 28.36),
    ("pi_w", C - 28.2, UPPER, 32.0),
    ("pi_sw", C - 27.0, UPPER, 26.6),
    ("w13_in", C - 27.0, UPPER, 38.4),
    ("w13_out", C - 27.0, UPPER, 42.0),
    ("w14_in", C - 22.0, UPPER, 38.4),
    ("w14_out", C - 22.0, UPPER, 42.0),
    ("w15_in", C - 8.0, UPPER, 38.4),
    ("w15_out", C - 8.0, UPPER, 42.0),
    ("pi_top", C - 12.0, UPPER, 35.8),
    ("pi_ne", C - 7.0, UPPER, 35.8),
    ("pi_e", C - 6.0, UPPER, 32.0),
    ("pi_se", C - 6.8, UPPER, 27.0),
    ("pi_dtop", C - 18.9, UPPER, 25.6),
    ("pi_lo", C - 12.0, LOWER, 25.6),
    ("pi_s1", C - 12.0, LOWER + 2.0, 31.0),
    // The Upper Lane
    ("ul_w", C - 2.2, UPPER, 32.0),
    ("ul_a", C + 3.0, UPPER, 28.5),
    ("ul_m", C + 6.5, UPPER, 34.0),
    ("w16_in", C + 9.0, UPPER, 25.6),
    ("w16_out", C + 9.0, UPPER, 22.0),
    ("w17_in", C + 15.0, UPPER, 25.6),
    ("w17_out", C + 15.0, UPPER, 22.0),
    ("ul_b", C + 3.0, UPPER, 39.0),
    ("ul_n", C + 12.0, UPPER, 41.6),
    ("ul_d", C + 16.5, UPPER, 40.8),
    ("ul_e", C + 16.2, UPPER, 32.0),
    // The Pensione: up the flight along its south wall to the roof garden
    ("pe_w", C + 19.8, UPPER, 32.0),
    ("pe_foot", C + 20.4, UPPER + 0.2, 26.0),
    ("pe_mid", C + 24.0, UPPER + 2.0, 26.0),
    ("pe_gate", C + 29.2, UPPER + STOREY, 26.0),
    ("pe_g1", C + 31.5, UPPER + STOREY, 29.5),
    ("pe_g2", C + 31.5, UPPER + STOREY, 34.0),
    ("pe_a", C + 23.5, UPPER, 34.0),
    ("pe_c", C + 26.0, UPPER, 31.0),
    ("w18_in", C + 24.0, UPPER, 38.4),
    ("w18_out", C + 24.0, UPPER, 42.0),
    ("w19_in", C + 30.0, UPPER, 38.4),
    ("w19_out", C + 30.0, UPPER, 42.0),
    // The Lighthouse: up its south lane, the landing, and back along the north lane to the gallery
    ("lh_d", C - 41.8, HEAD, 54.0),
    ("lh_foot", C - 42.8, HEAD, 47.8),
    ("lh_a1", C - 46.0, HEAD + 1.0, 47.8),
    ("lh_top", C - 52.5, HEAD + STOREY, 47.8),
    ("lh_land", C - 54.0, HEAD + STOREY, 52.0),
    ("lh_b0", C - 53.4, HEAD + STOREY, 57.8),
    ("lh_b1", C - 48.0, HEAD + STOREY + 2.0, 57.8),
    ("lh_gal", C - 42.2, HEAD + 2.0 * STOREY, 57.8),
    ("lh_gal2", C - 42.2, HEAD + 2.0 * STOREY, 61.8),
    ("lh_m", C - 48.0, HEAD, 52.5),
    ("lh_s1", C - 42.0, HEAD, 45.2),
    ("lh_s2", C - 49.0, HEAD, 45.2),
    ("lh_s3", C - 54.5, HEAD, 45.2),
    ("w20_in", C - 50.0, HEAD, 62.4),
    ("w20_out", C - 50.0, HEAD, 66.0),
    ("lh_nw", C - 54.0, HEAD, 61.8),
    // The Lighthouse Yard: up the Pilgrims' Stair, the lip over the Chapel, and the gate
    ("yd_foot", C - 35.0, UPPER + 0.2, 44.6),
    ("yd_mid", C - 35.0, UPPER + 2.0, 48.2),
    ("yd_top", C - 35.0, HEAD, 52.7),
    ("yd_w", C - 38.2, HEAD, 54.0),
    ("yd_dtop", C - 38.5, HEAD, 46.2),
    ("yd_a", C - 28.0, HEAD, 49.0),
    ("yd_b", C - 20.0, HEAD, 49.0),
    ("yd_e", C - 13.8, HEAD, 54.0),
    ("yd_m", C - 25.0, HEAD, 56.0),
    ("yd_nw", C - 35.5, HEAD, 59.5),
    ("w21_in", C - 30.0, HEAD, 62.4),
    ("w21_out", C - 30.0, HEAD, 66.0),
    ("w22_in", C - 22.0, HEAD, 62.4),
    ("w22_out", C - 22.0, HEAD, 66.0),
    ("yd_ne", C - 16.0, HEAD, 60.5),
    // The Keeper's Cottage
    ("co_w", C - 10.2, HEAD, 54.0),
    ("co_c", C - 3.0, HEAD, 54.0),
    ("co_e", C + 4.2, HEAD, 54.0),
    ("co_sw", C - 7.0, HEAD, 48.5),
    ("co_se", C + 1.0, HEAD, 48.5),
    ("w23_in", C - 7.0, HEAD, 62.4),
    ("w23_out", C - 7.0, HEAD, 66.0),
    ("w24_in", C + 1.0, HEAD, 62.4),
    ("w24_out", C + 1.0, HEAD, 66.0),
    // The Battery: up the Cliff Stair, round to the gun platform and its ramp
    ("ba_w", C + 7.8, HEAD, 54.0),
    ("ba_nw", C + 8.5, HEAD, 57.5),
    ("ba_lo", C + 12.0, UPPER, 45.4),
    ("ba_mid", C + 12.0, UPPER + 2.0, 51.0),
    ("ba_top", C + 12.0, HEAD, 55.8),
    ("ba_c", C + 18.0, HEAD, 57.0),
    ("w25_in", C + 20.0, HEAD, 62.4),
    ("w25_out", C + 20.0, HEAD, 66.0),
    ("w26_in", C + 28.0, HEAD, 62.4),
    ("w26_out", C + 28.0, HEAD, 66.0),
    ("ba_dtop", C + 16.5, HEAD, 46.2),
    ("ba_a", C + 21.0, HEAD, 49.0),
    ("ba_rf", C + 27.0, HEAD, 49.6),
    ("ba_b", C + 32.8, HEAD, 49.0),
    ("ba_e", C + 33.2, HEAD, 56.5),
    ("ba_r", C + 27.0, HEAD + 0.5, 52.2),
    ("ba_p0", C + 27.0, HEAD + 1.0, 55.4),
    ("ba_plat", C + 27.0, HEAD + 1.0, 58.0),
    ("ba_pw", C + 23.0, HEAD + 1.0, 55.4),
    ("ba_pw2", C + 23.0, HEAD + 1.0, 59.0),
    ("ba_pe", C + 31.0, HEAD + 1.0, 55.4),
    ("ba_pe2", C + 31.0, HEAD + 1.0, 59.0),
];

const OPEN: Gate = Gate::Open;

/// The edges, by node name. Barrier edges run from the first node to the second.
const EDGES: &[(&str, &str, Gate)] = {
    use ZoneId::*;
    &[
        // The Sea Cave
        ("sc_nw", "sc_ne", OPEN),
        ("sc_nw", "sc_w", OPEN),
        ("sc_ne", "sc_e", OPEN),
        ("sc_e", "sc_se", OPEN),
        ("sc_se", "sc_sw", OPEN),
        ("sc_sw", "sc_w", OPEN),
        ("sc_nw", "sc_step", OPEN),
        ("sc_ne", "sc_step", OPEN),
        ("sc_step", "sc_shelf", OPEN),
        ("sc_shelf", "sc_shelf2", OPEN),
        ("sc_e", "sh_w", Gate::Door(SeaCave, Shingle)),
        // The Shingle
        ("sh_w", "sh_c", OPEN),
        ("sh_c", "sh_e", OPEN),
        ("sh_w", "sh_nw", OPEN),
        ("sh_nw", "sh_n", OPEN),
        ("sh_n", "sh_ne", OPEN),
        ("sh_ne", "sh_e", OPEN),
        ("sh_c", "sh_n", OPEN),
        ("sh_w", "sh_sw", OPEN),
        ("sh_sw", "w0_in", OPEN),
        ("w0_in", "sh_se", OPEN),
        ("sh_se", "sh_e", OPEN),
        ("sh_c", "w0_in", OPEN),
        ("w0_out", "w0_in", barrier(0)),
        ("sh_e", "st_w", Gate::Door(Shingle, Strand)),
        ("sh_n", "ll_foot", Gate::Door(Shingle, LowerLane)),
        // The Strand
        ("st_w", "st_a", OPEN),
        ("st_a", "st_b", OPEN),
        ("st_b", "st_c", OPEN),
        ("st_c", "st_d", OPEN),
        ("st_d", "st_e", OPEN),
        ("st_w", "st_nw", OPEN),
        ("st_nw", "st_n", OPEN),
        ("st_n", "st_nm", OPEN),
        ("st_nm", "st_nc", OPEN),
        ("st_nc", "st_ne", OPEN),
        ("st_ne", "st_e", OPEN),
        ("st_a", "st_nw", OPEN),
        ("st_a", "st_n", OPEN),
        ("st_b", "st_nm", OPEN),
        ("st_c", "st_nc", OPEN),
        ("st_d", "st_ne", OPEN),
        ("st_a", "w1_in", OPEN),
        ("w1_in", "st_s1", OPEN),
        ("st_s1", "st_b", OPEN),
        ("st_s1", "st_s2", OPEN),
        ("st_s2", "st_c", OPEN),
        ("st_s2", "w2_in", OPEN),
        ("w2_in", "st_d", OPEN),
        ("w1_out", "w1_in", barrier(1)),
        ("w2_out", "w2_in", barrier(2)),
        ("st_e", "bo_w", Gate::Door(Strand, Boathouse)),
        ("st_n", "fm_foot", Gate::Door(Strand, FishMarket)),
        // The Boathouse
        ("bo_w", "bo_c", OPEN),
        ("bo_c", "bo_e", OPEN),
        ("bo_c", "bo_n", OPEN),
        ("bo_w", "bo_nw", OPEN),
        ("bo_nw", "bo_n", OPEN),
        ("bo_n", "bo_ne", OPEN),
        ("bo_ne", "bo_e", OPEN),
        ("bo_c", "w3_in", OPEN),
        ("bo_w", "bo_sw", OPEN),
        ("bo_sw", "w3_in", OPEN),
        ("w3_in", "bo_se", OPEN),
        ("bo_se", "bo_e", OPEN),
        ("w3_out", "w3_in", barrier(3)),
        ("bo_n", "tv_foot", Gate::Door(Boathouse, Taverna)),
        // The Washhouse
        ("wa_sw", "wa_se", OPEN),
        ("wa_se", "wa_e", OPEN),
        ("wa_sw", "w4_in", OPEN),
        ("w4_in", "wa_nw", OPEN),
        ("wa_nw", "wa_n", OPEN),
        ("wa_n", "wa_ne", OPEN),
        ("wa_ne", "wa_e", OPEN),
        ("w4_in", "wa_e", OPEN),
        ("w4_out", "w4_in", barrier(4)),
        ("wa_e", "ll_w", Gate::Door(Washhouse, LowerLane)),
        ("wa_n", "ch_lo", Gate::Door(Washhouse, Chapel)),
        // The Lower Lane
        ("ll_dtop", "ll_w", OPEN),
        ("ll_w", "w5_in", OPEN),
        ("w5_in", "ll_top", OPEN),
        ("ll_foot", "ll_mid", OPEN),
        ("ll_mid", "ll_top", OPEN),
        ("ll_top", "w6_in", OPEN),
        ("ll_top", "ll_e", OPEN),
        ("w6_in", "ll_e", OPEN),
        ("ll_e", "ll_se", OPEN),
        ("w5_out", "w5_in", barrier(5)),
        ("w6_out", "w6_in", barrier(6)),
        ("ll_dtop", "sh_nw", Gate::DoorDrop(Shingle, LowerLane)),
        ("ll_e", "fm_w", Gate::Door(LowerLane, FishMarket)),
        // The Fish Market
        ("fm_w", "fm_sw", OPEN),
        ("fm_w", "fm_a", OPEN),
        ("fm_foot", "fm_mid", OPEN),
        ("fm_mid", "fm_top", OPEN),
        ("fm_a", "fm_top", OPEN),
        ("fm_top", "fm_b", OPEN),
        ("fm_se", "fm_b", OPEN),
        ("fm_a", "fm_nw", OPEN),
        ("fm_nw", "fm_n", OPEN),
        ("fm_top", "fm_n", OPEN),
        ("fm_n", "w7_in", OPEN),
        ("w7_in", "w8_in", OPEN),
        ("fm_b", "w7_in", OPEN),
        ("fm_b", "fm_ps", OPEN),
        ("fm_ps", "fm_e", OPEN),
        ("fm_ps", "fm_plat", OPEN),
        ("w8_in", "fm_e", OPEN),
        ("w7_out", "w7_in", barrier(7)),
        ("w8_out", "w8_in", barrier(8)),
        ("fm_plat", "st_nm", Gate::DoorDrop(Strand, FishMarket)),
        ("fm_e", "ta_w", Gate::Door(FishMarket, Taverna)),
        ("fm_n", "pi_lo", Gate::Door(FishMarket, Piazza)),
        // The Taverna
        ("ta_w", "ta_s1", OPEN),
        ("ta_w", "ta_m1", OPEN),
        ("ta_s1", "ta_s2", OPEN),
        ("ta_s1", "ta_m1", OPEN),
        ("ta_s2", "ta_m2", OPEN),
        ("ta_m1", "ta_m2", OPEN),
        ("ta_m2", "tv_top", OPEN),
        ("tv_foot", "tv_mid", OPEN),
        ("tv_mid", "tv_top", OPEN),
        ("tv_top", "ta_m3", OPEN),
        ("ta_m3", "ta_s3", OPEN),
        ("ta_m2", "w9_in", OPEN),
        ("w9_in", "w10_in", OPEN),
        ("ta_m3", "w10_in", OPEN),
        ("ta_m1", "ta_n1", OPEN),
        ("ta_n1", "w9_in", OPEN),
        ("w9_out", "w9_in", barrier(9)),
        ("w10_out", "w10_in", barrier(10)),
        // The Chapel
        ("ch_lo", "ch_s1", OPEN),
        ("ch_s1", "ch_top", OPEN),
        ("ch_top", "ch_aw", OPEN),
        ("ch_aw", "w12_in", OPEN),
        ("w12_in", "w11_in", OPEN),
        ("ch_top", "ch_m", OPEN),
        ("ch_m", "ch_sm", OPEN),
        ("ch_m", "ch_c", OPEN),
        ("ch_sm", "ch_c", OPEN),
        ("ch_c", "ch_e", OPEN),
        ("ch_e", "ch_n", OPEN),
        ("ch_m", "ch_n", OPEN),
        ("ch_n", "ch_db", OPEN),
        ("ch_m", "ch_afoot", OPEN),
        ("ch_afoot", "ch_amid", OPEN),
        ("ch_amid", "ch_land", OPEN),
        ("ch_land", "ch_bmid", OPEN),
        ("ch_bmid", "ch_bell", OPEN),
        ("w11_out", "w11_in", barrier(11)),
        ("w12_out", "w12_in", barrier(12)),
        ("ch_e", "pi_w", Gate::Door(Chapel, Piazza)),
        ("ch_n", "yd_foot", Gate::Door(Chapel, LighthouseYard)),
        // The Piazza
        ("pr0", "pr1", OPEN),
        ("pr1", "pr2", OPEN),
        ("pr2", "pr3", OPEN),
        ("pr3", "pr4", OPEN),
        ("pr4", "pr5", OPEN),
        ("pr5", "pr0", OPEN),
        ("pi_w", "pr3", OPEN),
        ("pi_w", "pi_sw", OPEN),
        ("pi_sw", "pr4", OPEN),
        ("w13_in", "w14_in", OPEN),
        ("w14_in", "pr2", OPEN),
        ("w13_in", "pi_w", OPEN),
        ("pi_top", "pr1", OPEN),
        ("pi_top", "w15_in", OPEN),
        ("pi_top", "pi_ne", OPEN),
        ("pi_ne", "w15_in", OPEN),
        ("pi_ne", "pi_e", OPEN),
        ("pi_e", "pi_se", OPEN),
        ("pr5", "pi_dtop", OPEN),
        ("pi_lo", "pi_s1", OPEN),
        ("pi_s1", "pi_top", OPEN),
        ("w13_out", "w13_in", barrier(13)),
        ("w14_out", "w14_in", barrier(14)),
        ("w15_out", "w15_in", barrier(15)),
        ("pi_dtop", "fm_nw", Gate::DoorDrop(FishMarket, Piazza)),
        ("pi_e", "ul_w", Gate::Door(Piazza, UpperLane)),
        // The Upper Lane
        ("ul_w", "ul_a", OPEN),
        ("ul_w", "ul_m", OPEN),
        ("ul_a", "ul_m", OPEN),
        ("ul_a", "w16_in", OPEN),
        ("ul_m", "w16_in", OPEN),
        ("w16_in", "w17_in", OPEN),
        ("w17_in", "ul_e", OPEN),
        ("ul_m", "ul_e", OPEN),
        ("ul_m", "ul_b", OPEN),
        ("ul_b", "ul_n", OPEN),
        ("ul_m", "ul_n", OPEN),
        ("ul_n", "ul_d", OPEN),
        ("ul_e", "ul_d", OPEN),
        ("w16_out", "w16_in", barrier(16)),
        ("w17_out", "w17_in", barrier(17)),
        ("ul_e", "pe_w", Gate::Door(UpperLane, Pensione)),
        ("ul_n", "ba_lo", Gate::Door(UpperLane, Battery)),
        // The Pensione
        ("pe_w", "pe_a", OPEN),
        ("pe_w", "pe_foot", OPEN),
        ("pe_foot", "pe_mid", OPEN),
        ("pe_mid", "pe_gate", OPEN),
        ("pe_gate", "pe_g1", OPEN),
        ("pe_g1", "pe_g2", OPEN),
        ("pe_a", "pe_c", OPEN),
        ("pe_w", "pe_c", OPEN),
        ("pe_a", "w18_in", OPEN),
        ("w18_in", "w19_in", OPEN),
        ("w18_out", "w18_in", barrier(18)),
        ("w19_out", "w19_in", barrier(19)),
        // The Lighthouse
        ("lh_d", "lh_foot", OPEN),
        ("lh_foot", "lh_a1", OPEN),
        ("lh_a1", "lh_top", OPEN),
        ("lh_top", "lh_land", OPEN),
        ("lh_land", "lh_b0", OPEN),
        ("lh_b0", "lh_b1", OPEN),
        ("lh_b1", "lh_gal", OPEN),
        ("lh_gal", "lh_gal2", OPEN),
        ("lh_d", "lh_m", OPEN),
        ("lh_foot", "lh_s1", OPEN),
        ("lh_s1", "lh_s2", OPEN),
        ("lh_s2", "lh_s3", OPEN),
        ("lh_m", "w20_in", OPEN),
        ("w20_in", "lh_nw", OPEN),
        ("w20_out", "w20_in", barrier(20)),
        ("lh_d", "yd_w", Gate::Door(Lighthouse, LighthouseYard)),
        // The Lighthouse Yard
        ("yd_foot", "yd_mid", OPEN),
        ("yd_mid", "yd_top", OPEN),
        ("yd_top", "yd_w", OPEN),
        ("yd_top", "yd_m", OPEN),
        ("yd_top", "yd_nw", OPEN),
        ("yd_m", "yd_a", OPEN),
        ("yd_a", "yd_b", OPEN),
        ("yd_b", "yd_e", OPEN),
        ("yd_m", "yd_e", OPEN),
        ("yd_m", "w21_in", OPEN),
        ("yd_m", "w22_in", OPEN),
        ("w21_in", "yd_nw", OPEN),
        ("yd_nw", "yd_w", OPEN),
        ("yd_e", "yd_ne", OPEN),
        ("yd_ne", "w22_in", OPEN),
        ("yd_w", "yd_dtop", OPEN),
        ("w21_out", "w21_in", barrier(21)),
        ("w22_out", "w22_in", barrier(22)),
        ("yd_dtop", "ch_db", Gate::DoorDrop(Chapel, LighthouseYard)),
        ("yd_e", "co_w", Gate::Door(LighthouseYard, KeepersCottage)),
        // The Keeper's Cottage
        ("co_w", "co_c", OPEN),
        ("co_c", "co_e", OPEN),
        ("co_w", "co_sw", OPEN),
        ("co_sw", "co_se", OPEN),
        ("co_se", "co_e", OPEN),
        ("co_sw", "co_c", OPEN),
        ("co_c", "w23_in", OPEN),
        ("co_c", "w24_in", OPEN),
        ("co_w", "w23_in", OPEN),
        ("w23_out", "w23_in", barrier(23)),
        ("w24_out", "w24_in", barrier(24)),
        ("co_e", "ba_w", Gate::Door(KeepersCottage, Battery)),
        // The Battery
        ("ba_lo", "ba_mid", OPEN),
        ("ba_mid", "ba_top", OPEN),
        ("ba_w", "ba_nw", OPEN),
        ("ba_nw", "ba_top", OPEN),
        ("ba_top", "ba_c", OPEN),
        ("ba_c", "w25_in", OPEN),
        ("w25_in", "w26_in", OPEN),
        ("ba_c", "ba_a", OPEN),
        ("ba_a", "ba_dtop", OPEN),
        ("ba_a", "ba_rf", OPEN),
        ("ba_rf", "ba_b", OPEN),
        ("ba_b", "ba_e", OPEN),
        ("ba_rf", "ba_r", OPEN),
        ("ba_r", "ba_p0", OPEN),
        ("ba_p0", "ba_plat", OPEN),
        ("ba_p0", "ba_pw", OPEN),
        ("ba_pw", "ba_pw2", OPEN),
        ("ba_p0", "ba_pe", OPEN),
        ("ba_pe", "ba_pe2", OPEN),
        ("w25_out", "w25_in", barrier(25)),
        ("w26_out", "w26_in", barrier(26)),
        ("ba_dtop", "ul_d", Gate::DoorDrop(UpperLane, Battery)),
    ]
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::maps::map::{MapId, WINDOWS as ALL_WINDOWS};
    use ZoneId::*;

    fn cove() -> &'static Layout {
        MapId::Cove.layout()
    }

    /// The Cove's 16 Zones are on four levels, a storey apart, and a Run starts on the Strand.
    #[test]
    fn sixteen_zones_on_four_levels_starting_on_the_strand() {
        assert_eq!(cove().zones.len(), 16);
        assert_eq!(cove().start_zone(), Strand);
        let levels = [
            (SHORE, [SeaCave, Shingle, Strand, Boathouse]),
            (LOWER, [Washhouse, LowerLane, FishMarket, Taverna]),
            (UPPER, [Chapel, Piazza, UpperLane, Pensione]),
            (HEAD, [Lighthouse, LighthouseYard, KeepersCottage, Battery]),
        ];
        for (level, zones) in levels {
            for zone in zones {
                assert_eq!(zone.floor(), level, "{}", zone.name());
                assert!(cove().zones.contains(&zone));
            }
        }
    }

    #[test]
    fn the_cove_stands_at_cove_x_clear_of_the_other_maps() {
        assert_eq!(COVE_X, -400.0);
        let (min, max) = cove().plan;
        assert_eq!((min.x + max.x) / 2.0, COVE_X);
        for other in [MapId::Greybox, MapId::Quarry, MapId::Village, MapId::Dam] {
            let (omin, omax) = other.layout().plan;
            assert!(omax.x < min.x || omin.x > max.x, "overlaps the {}", other.name());
        }
    }

    /// The Doors, by the two Zones they join, and what each costs: `None` for the Power Door.
    fn prices() -> HashMap<(ZoneId, ZoneId), Option<u64>> {
        HashMap::from([
            ((Strand, Boathouse), Some(750)),
            ((Strand, FishMarket), Some(750)),
            ((Strand, Shingle), Some(1000)),
            ((Shingle, SeaCave), None),
            ((Boathouse, Taverna), Some(1000)),
            ((FishMarket, Taverna), Some(1000)),
            ((FishMarket, LowerLane), Some(750)),
            ((Shingle, LowerLane), Some(1000)),
            ((LowerLane, Washhouse), Some(1000)),
            ((Washhouse, Chapel), Some(1250)),
            ((FishMarket, Piazza), Some(1250)),
            ((Chapel, Piazza), Some(1000)),
            ((Piazza, UpperLane), Some(1000)),
            ((UpperLane, Pensione), Some(1000)),
            ((UpperLane, Battery), Some(1250)),
            ((Chapel, LighthouseYard), Some(1250)),
            ((LighthouseYard, KeepersCottage), Some(1000)),
            ((KeepersCottage, Battery), Some(1000)),
            ((LighthouseYard, Lighthouse), Some(1250)),
        ])
    }

    fn same_join(a: (ZoneId, ZoneId), b: (ZoneId, ZoneId)) -> bool {
        a == b || a == (b.1, b.0)
    }

    /// The 18 bought Doors are at the layout's prices, with the Power Door into the Sea Cave, and
    /// the cheapest way from the start to the Power costs 5500.
    #[test]
    fn eighteen_bought_doors_and_a_power_door_and_five_and_a_half_thousand_to_the_power() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::maps::door::Door;

        let prices = prices();
        let mut sim = Sim::on(MapId::Cove, 60.0, |_| {});
        let doors: Vec<(Option<(ZoneId, ZoneId)>, Option<u64>)> = sim
            .world_mut()
            .run_system_once(|doors: Query<&Door>| doors.iter().map(|door| (door.joins, door.price)).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(doors.len(), 19);
        assert_eq!(doors.iter().filter(|(_, price)| price.is_none()).count(), 1, "one Power Door");
        for (joins, price) in doors {
            let joins = joins.expect("every Door joins two Zones");
            let want = prices.iter().find(|&(&join, _)| same_join(join, joins)).map(|(_, &price)| price);
            assert_eq!(Some(price), want, "the Door between {} and {}", joins.0.name(), joins.1.name());
        }

        // The cheapest bought way from the Strand to the Lighthouse.
        let mut cost: HashMap<ZoneId, u64> = HashMap::from([(Strand, 0)]);
        for _ in 0..16 {
            for (&(a, b), price) in &prices {
                let Some(price) = price else { continue };
                for (from, to) in [(a, b), (b, a)] {
                    let Some(&here) = cost.get(&from) else { continue };
                    let best = cost.entry(to).or_insert(u64::MAX);
                    *best = (*best).min(here + price);
                }
            }
        }
        assert_eq!(cost[&Lighthouse], 5500);
        // Pack-a-Punch needs the Shingle too.
        assert_eq!(cost[&Shingle], 1000);
    }

    /// Seven flights of stairs, each in a well in the floor of the Zone at its top, a storey deep,
    /// with its Door at the foot, in the wall between the levels.
    #[test]
    fn seven_flights_of_stairs_each_owned_by_the_zone_at_its_top() {
        assert_eq!(WELLS.len(), 7);
        let flights: Vec<&Slope> = SLOPES.iter().filter(|slope| slope.rise >= STOREY && slope.foot.y + slope.rise == slope.zone.floor()).collect();
        assert_eq!(flights.len(), 7);
        for well in &WELLS {
            let flight = flights
                .iter()
                .find(|slope| slope.zone == well.zone && (slope.foot.x - (well.min.x + well.max.x) / 2.0).abs() < 0.01)
                .expect("a flight in each well");
            assert_eq!(well.depth, STOREY);
            // It climbs out of the well's bottom at the Door end, and tops out where the well ends.
            assert_eq!(flight.foot.y, well.zone.floor() - well.depth);
            assert!((flight.foot.z + flight.run - well.max.y).abs() < 0.01, "{}", well.zone.name());
        }
    }

    /// Zombies come in only through Barriers: 27, with no Risers and no Vents, and each Zone has
    /// the ones the layout gives it.
    #[test]
    fn twenty_seven_barriers_and_no_risers_or_vents() {
        assert!(cove().risers.is_empty());
        assert!(cove().vents.is_empty());
        assert_eq!(MapId::Cove.barriers().count(), 27);
        let counts = [
            (Strand, 2),
            (Boathouse, 1),
            (Shingle, 1),
            (SeaCave, 0),
            (FishMarket, 2),
            (Taverna, 2),
            (LowerLane, 2),
            (Washhouse, 1),
            (Chapel, 2),
            (Piazza, 3),
            (UpperLane, 2),
            (Pensione, 2),
            (LighthouseYard, 2),
            (Lighthouse, 1),
            (KeepersCottage, 2),
            (Battery, 2),
        ];
        for (zone, want) in counts {
            let count = MapId::Cove.barriers().filter(|barrier| ALL_WINDOWS[barrier.0 as usize].0 == zone).count();
            assert_eq!(count, want, "{} has {count} Barriers", zone.name());
        }
        assert_eq!(cove().sapper.first, SapperTiming::DEFAULT.first);
        assert!(cove().sections.is_empty() && cove().generators.is_empty() && !cove().darkness);
    }

    /// A Hellhound Spawn Point in every Zone, two each on the Strand, the Piazza and the Lighthouse
    /// Yard, and specials on the Strand, the Piazza, the Lighthouse Yard and the Battery.
    #[test]
    fn hellhounds_in_every_zone_and_specials_in_four() {
        for &zone in cove().zones {
            let want = if matches!(zone, Strand | Piazza | LighthouseYard) { 2 } else { 1 };
            assert_eq!(cove().hellhound_spawns.iter().filter(|(z, _)| *z == zone).count(), want, "{}", zone.name());
        }
        let mut specials: Vec<ZoneId> = cove().special_spawns.iter().map(|&(zone, _)| zone).collect();
        specials.sort_by_key(|zone| *zone as u32);
        let mut want = vec![Strand, Piazza, LighthouseYard, Battery];
        want.sort_by_key(|zone| *zone as u32);
        assert_eq!(specials, want);
    }

    /// Six Box Locations, starting in the Taverna, none on the Shore, in the Sea Cave or the Lighthouse.
    #[test]
    fn six_box_locations_starting_in_the_taverna() {
        let zones: Vec<ZoneId> = MapId::Cove.box_locations().map(|i| crate::maps::map::BOX_LOCATIONS[i].0).collect();
        assert_eq!(zones, [Taverna, Washhouse, Piazza, Pensione, KeepersCottage, Battery]);
    }

    /// The five drops are one-way edges gated on the Door beside each: each joins two Zones
    /// that a Door in the graph also joins, and none is a fixed join.
    #[test]
    fn five_drops_each_gated_on_the_door_beside_it() {
        assert!(cove().fixed_joins.is_empty());
        let drops: Vec<(ZoneId, ZoneId)> = EDGES
            .iter()
            .filter_map(|&(.., gate)| match gate {
                Gate::DoorDrop(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        assert_eq!(drops.len(), 5);
        let prices = prices();
        for drop in drops {
            assert!(prices.keys().any(|&join| same_join(join, drop)), "no Door beside the drop between {} and {}", drop.0.name(), drop.1.name());
        }
    }

    /// The Cove's ways round: up the stairs from the beach, round the Piazza's fountain, and up
    /// the towers. A Zombie walks every one to the Player.
    #[test]
    fn a_zombie_follows_the_player_round_every_way_of_the_cove() {
        use crate::enemies::zombie::headless::follows;
        let ways = [
            ("up the Sea Wall Steps", at(0.0, SHORE, -4.0), at(-10.0, LOWER, 17.0)),
            ("up the Boathouse Stair", at(27.0, SHORE, -4.0), at(20.0, LOWER, 15.5)),
            ("up the Fishermen's Steps", at(-33.0, SHORE, -4.0), at(-38.0, LOWER, 18.4)),
            ("up the Long Stair into the Piazza", at(-10.0, LOWER, 17.0), at(-8.0, UPPER, 35.8)),
            ("round the fountain", at(-28.2, UPPER, 32.0), at(-16.8, UPPER, 32.0)),
            ("up the Chapel Stair", at(-50.0, LOWER, 21.8), at(-42.0, UPPER, 35.0)),
            ("up the bell tower", at(-38.0, UPPER, 30.0), at(-43.5, UPPER + 2.0 * STOREY, 42.0)),
            ("up the Pilgrims' Stair", at(-35.0, UPPER, 42.2), at(-28.0, HEAD, 49.0)),
            ("up the Lighthouse", at(-41.8, HEAD, 54.0), at(-42.2, HEAD + 2.0 * STOREY, 61.8)),
            ("up the Cliff Stair", at(12.0, UPPER, 41.6), at(21.0, HEAD, 49.0)),
            ("up onto the guns", at(21.0, HEAD, 49.0), at(27.0, HEAD + 1.0, 58.0)),
            ("up the Pensione's roof garden", at(23.5, UPPER, 34.0), at(31.5, UPPER + STOREY, 29.5)),
            ("up onto the Pack-a-Punch shelf", at(-50.25, SHORE, 3.5), at(-50.25, SHORE + 0.6, -8.5)),
            ("onto the Fish Market's platform", at(-3.0, LOWER, 17.0), at(0.5, LOWER + 0.5, 8.4)),
        ];
        for (way, start, player) in ways {
            let (arrived, last) = follows(MapId::Cove, start, player);
            assert!(arrived, "{way}: last seen at {last}");
        }
    }
}
