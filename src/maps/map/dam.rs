//! The Dam Map: a concrete dam on three levels, with the reservoir to the north and the valley to
//! the south. A Run starts on the Crest Road along the top, the only Zone out of doors, which has
//! two ways off it: the Tower Head at its west end and the Spillway Gatehouse at its east end.
//!
//! From the Tower Head the Gate Shaft's stairs go down a storey into the Intake Gallery, and from
//! the Gatehouse the Spillway Gallery's own steps do the same. The two galleries run inside the dam
//! to the Control Room in its middle, where Pack-a-Punch stands on a dais, and on down the dam's
//! sides. The Penstock Tunnel's stairs go down from the Control Room to the Turbine Hall at the
//! foot, round its turbine deck, with the Switchgear Room off it. The Drainage Gallery runs along
//! the dam's toe under them all: from the foot of the Intake Gallery's west side, past the Turbine
//! Hall, to the foot of the Spillway Stair, so it closes a Train loop round the Control Room each
//! side.
//!
//! Every Zone but the Crest Road has a ceiling. There are no Risers: Zombies come in through
//! Barriers in the parapets, drain grates and cable ducts, each a pocket of floor walled in outside
//! its window. The Dam has no Power Switch; its Perks (but Quick Revive), Traps and Pack-a-Punch
//! wait for the Generators.
//!
//! The whole Map stands `DAM_X` along x from the world's middle, clear of the others.

use bevy::prelude::*;

use crate::enemies::nav::Gate;
use crate::maps::generator::{Generator, SectionId};
use crate::maps::map::BarrierId;
use crate::maps::zone::ZoneId;
use crate::tuning::*;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{Layout, Prop, PropKind, RaisedFloor, STOREY, Slope, door, palette, prop};

/// How far along x the Dam stands from the world's middle, east of the Quarry.
pub const DAM_X: f32 = 400.0;
const D: f32 = DAM_X;

/// The three levels: the foot, the gallery a storey up, and the crest.
const FOOT: f32 = 0.0;
const GALLERY: f32 = STOREY;
const CREST: f32 = 2.0 * STOREY;

/// The first of the Dam's windows in `WINDOWS`, after the other Maps'.
const W0: u8 = (super::greybox::WINDOWS.len() + super::quarry::WINDOWS.len() + super::village::WINDOWS.len()) as u8;

/// How high a chalk outline's centre sits over its floor.
const CHALK: f32 = 1.5;

/// How wide a flight of stairs is: the width of the 6 m stairwells and galleries, inside their
/// walls.
const FLIGHT_WIDTH: f32 = 5.6;

/// A point in the Dam, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(D + x, y, z)
}

/// A spot on a wall or a floor's plan in the Dam, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(D + x, z)
}

/// The three Sections. The Crest Road and the Control Room are in none.
const SECTIONS: [(SectionId, &[ZoneId]); 3] = {
    use ZoneId::*;
    [
        (SectionId::IntakeTower, &[TowerHead, GateShaft, IntakeGallery]),
        (SectionId::SpillwayGallery, &[SpillwayGatehouse, SpillwayGallery, SpillwayStair]),
        (SectionId::TurbineHall, &[PenstockTunnel, TurbineHall, SwitchgearRoom, DrainageGallery]),
    ]
};

/// The Generators: in the Intake Gallery and the Spillway Gallery, each against a wall of its
/// corridor down the dam's side, and on the Turbine Hall's deck, its console at the deck's east edge.
const GENERATORS: [Generator; 3] = [
    Generator {
        section: SectionId::IntakeTower,
        zone: ZoneId::IntakeGallery,
        at: at(-39.7, GALLERY, -22.0),
        facing: Vec3::NEG_X,
        price: GENERATOR_PRICE_INTAKE_TOWER,
    },
    Generator {
        section: SectionId::SpillwayGallery,
        zone: ZoneId::SpillwayGallery,
        at: at(39.7, GALLERY, -26.0),
        facing: Vec3::X,
        price: GENERATOR_PRICE_SPILLWAY_GALLERY,
    },
    Generator {
        section: SectionId::TurbineHall,
        zone: ZoneId::TurbineHall,
        at: at(3.3, TURBINE_DECK.height, -40.5),
        facing: Vec3::NEG_X,
        price: GENERATOR_PRICE_TURBINE_HALL,
    },
];

const fn barrier(i: u8) -> Gate {
    Gate::Barrier(BarrierId(W0 + i))
}

pub(super) const DAM: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [
            CrestRoad,
            TowerHead,
            SpillwayGatehouse,
            GateShaft,
            IntakeGallery,
            SpillwayGallery,
            ControlRoom,
            PenstockTunnel,
            TurbineHall,
            SwitchgearRoom,
            SpillwayStair,
            DrainageGallery,
        ]
    },
    // In the middle of the Crest Road, in a row facing Quick Revive.
    seats: [at(-4.5, CREST, 25.0), at(-1.5, CREST, 25.0), at(1.5, CREST, 25.0), at(4.5, CREST, 25.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &[],
    raised_floors: &RAISED_FLOORS,
    slopes: &SLOPES,
    sunken: &[],
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &[],
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &[],
    sections: &SECTIONS,
    generators: &GENERATORS,
    darkness: true,
    sun: Vec3::new(1.0, -3.0, -1.0),
    plan: (Vec2::new(D - 52.0, -68.0), Vec2::new(D + 52.0, 36.0)),
    build,
};

/// The chalk outlines: the cheap guns on the Crest Road either side of Quick Revive, an SMG and a
/// shotgun at either end of the crest, and the AK-74u down in the Turbine Hall.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    (WeaponId::M14, ZoneId::CrestRoad, at(-3.5, CREST + CHALK, 29.8), Vec2::NEG_Y),
    (WeaponId::Olympia, ZoneId::CrestRoad, at(3.5, CREST + CHALK, 29.8), Vec2::NEG_Y),
    (WeaponId::Mp40, ZoneId::TowerHead, at(-34.0, CREST + CHALK, 31.8), Vec2::NEG_Y),
    (WeaponId::TrenchGun, ZoneId::SpillwayGatehouse, at(34.0, CREST + CHALK, 31.8), Vec2::NEG_Y),
    (WeaponId::Ak74u, ZoneId::TurbineHall, at(9.8, FOOT + CHALK, -38.0), Vec2::NEG_X),
];

/// The Box Locations, in order. Every Run starts the Box at the first, at the foot of the Gate
/// Shaft, two Doors from the Crest Road. None is on the crest or in the Control Room.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // Against the Gate Shaft's east wall, at the foot of its stairs.
    (ZoneId::GateShaft, at(-38.65, GALLERY, 4.5), Vec2::NEG_X),
    // Against the Spillway Gallery's north wall.
    (ZoneId::SpillwayGallery, at(24.0, GALLERY, 1.35), Vec2::NEG_Y),
    // Against the Spillway Stair's west wall, at the foot of its stairs.
    (ZoneId::SpillwayStair, at(38.65, FOOT, -51.0), Vec2::X),
    // Against the Turbine Hall's west wall, beside the deck.
    (ZoneId::TurbineHall, at(-9.35, FOOT, -42.0), Vec2::X),
    // Against the Drainage Gallery's south wall.
    (ZoneId::DrainageGallery, at(-8.0, FOOT, -61.35), Vec2::Y),
    // Against the Switchgear Room's north wall.
    (ZoneId::SwitchgearRoom, at(20.0, FOOT, -40.65), Vec2::NEG_Y),
];

/// The 20 windows, each opening onto a pocket of floor walled in outside it: the Crest Road's in
/// its parapets, two onto the reservoir side and two onto the valley side; the rest drain grates,
/// cable ducts and inspection hatches.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 20] = [
    // 0-3: the Crest Road's parapets.
    (ZoneId::CrestRoad, at(-18.0, CREST, 30.0), Vec2::Y),
    (ZoneId::CrestRoad, at(18.0, CREST, 30.0), Vec2::Y),
    (ZoneId::CrestRoad, at(-18.0, CREST, 22.0), Vec2::NEG_Y),
    (ZoneId::CrestRoad, at(18.0, CREST, 22.0), Vec2::NEG_Y),
    // 4-5: the ends of the crest.
    (ZoneId::TowerHead, at(-46.0, CREST, 26.0), Vec2::NEG_X),
    (ZoneId::SpillwayGatehouse, at(46.0, CREST, 26.0), Vec2::X),
    // 6-10: the gallery level.
    (ZoneId::GateShaft, at(-44.0, GALLERY, 4.0), Vec2::NEG_X),
    (ZoneId::IntakeGallery, at(-24.0, GALLERY, 2.0), Vec2::Y),
    (ZoneId::IntakeGallery, at(-44.0, GALLERY, -30.0), Vec2::NEG_X),
    (ZoneId::SpillwayGallery, at(44.0, GALLERY, -18.0), Vec2::X),
    (ZoneId::ControlRoom, at(7.0, GALLERY, 8.0), Vec2::Y),
    // 11-19: the foot.
    (ZoneId::PenstockTunnel, at(-3.0, FOOT, -22.0), Vec2::NEG_X),
    (ZoneId::TurbineHall, at(-7.0, FOOT, -28.0), Vec2::Y),
    (ZoneId::TurbineHall, at(-10.0, FOOT, -34.0), Vec2::NEG_X),
    (ZoneId::TurbineHall, at(-10.0, FOOT, -50.0), Vec2::NEG_X),
    (ZoneId::TurbineHall, at(10.0, FOOT, -32.0), Vec2::X),
    (ZoneId::SwitchgearRoom, at(26.0, FOOT, -46.0), Vec2::X),
    (ZoneId::SpillwayStair, at(44.0, FOOT, -50.0), Vec2::X),
    (ZoneId::DrainageGallery, at(-20.0, FOOT, -62.0), Vec2::NEG_Y),
    (ZoneId::DrainageGallery, at(20.0, FOOT, -62.0), Vec2::NEG_Y),
];

/// The Hellhound Spawn Points: one in every Zone, two on the Crest Road, in the Turbine Hall and
/// in the Drainage Gallery.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 15] = [
    (ZoneId::CrestRoad, at(-12.0, CREST, 27.0)),
    (ZoneId::CrestRoad, at(12.0, CREST, 27.0)),
    (ZoneId::TowerHead, at(-36.0, CREST, 21.5)),
    (ZoneId::SpillwayGatehouse, at(36.0, CREST, 21.5)),
    (ZoneId::GateShaft, at(-41.0, GALLERY, 4.5)),
    (ZoneId::IntakeGallery, at(-30.0, GALLERY, -0.5)),
    (ZoneId::SpillwayGallery, at(20.0, GALLERY, -1.0)),
    (ZoneId::ControlRoom, at(-8.0, GALLERY, 6.0)),
    (ZoneId::PenstockTunnel, at(0.0, FOOT, -24.0)),
    (ZoneId::TurbineHall, at(-7.5, FOOT, -40.0)),
    (ZoneId::TurbineHall, at(7.5, FOOT, -40.0)),
    (ZoneId::SwitchgearRoom, at(13.0, FOOT, -50.0)),
    (ZoneId::SpillwayStair, at(41.0, FOOT, -48.0)),
    (ZoneId::DrainageGallery, at(-14.0, FOOT, -59.0)),
    (ZoneId::DrainageGallery, at(26.0, FOOT, -59.0)),
];

/// The Special Spawn Points: only on the Crest Road, in the Turbine Hall and in the Drainage
/// Gallery.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 6] = [
    (ZoneId::CrestRoad, at(-12.0, CREST, 27.0)),
    (ZoneId::CrestRoad, at(12.0, CREST, 27.0)),
    (ZoneId::TurbineHall, at(-7.5, FOOT, -40.0)),
    (ZoneId::TurbineHall, at(7.5, FOOT, -40.0)),
    (ZoneId::DrainageGallery, at(-14.0, FOOT, -59.0)),
    (ZoneId::DrainageGallery, at(26.0, FOOT, -59.0)),
];

/// The lamps: floodlights over the Crest Road, and one or two under every ceiling.
const LAMPS: [(ZoneId, Vec3); 19] = [
    (ZoneId::CrestRoad, at(-15.0, CREST + STOREY - 0.5, 26.0)),
    (ZoneId::CrestRoad, at(15.0, CREST + STOREY - 0.5, 26.0)),
    (ZoneId::TowerHead, at(-38.0, CREST + STOREY - 0.5, 25.0)),
    (ZoneId::SpillwayGatehouse, at(38.0, CREST + STOREY - 0.5, 25.0)),
    (ZoneId::GateShaft, at(-41.0, CREST + STOREY - 0.5, 10.0)),
    (ZoneId::IntakeGallery, at(-24.0, CREST - 0.5, -1.0)),
    (ZoneId::IntakeGallery, at(-41.0, CREST - 0.5, -30.0)),
    (ZoneId::SpillwayGallery, at(24.0, CREST - 0.5, -1.0)),
    (ZoneId::SpillwayGallery, at(41.0, CREST - 0.5, -16.0)),
    (ZoneId::SpillwayGallery, at(41.0, CREST + STOREY - 0.5, 12.0)),
    (ZoneId::ControlRoom, at(0.0, CREST - 0.5, 1.0)),
    (ZoneId::PenstockTunnel, at(0.0, CREST - 0.5, -20.0)),
    (ZoneId::TurbineHall, at(0.0, CREST - 0.5, -36.0)),
    (ZoneId::TurbineHall, at(0.0, CREST - 0.5, -50.0)),
    (ZoneId::SwitchgearRoom, at(18.0, GALLERY - 0.5, -46.0)),
    (ZoneId::SpillwayStair, at(41.0, CREST - 0.5, -45.0)),
    (ZoneId::DrainageGallery, at(-20.0, CREST - 0.5, -59.0)),
    (ZoneId::DrainageGallery, at(20.0, CREST - 0.5, -59.0)),
    (ZoneId::DrainageGallery, at(-41.0, CREST - 0.5, -59.0)),
];

/// Concrete, for the stairs and landings; steel plate, for the turbine deck; and dark terrazzo,
/// for the Control Room's dais.
const CONCRETE: Color = Color::srgb(0.5, 0.49, 0.46);
const STEEL_PLATE: Color = Color::srgb(0.32, 0.34, 0.36);
const TERRAZZO: Color = Color::srgb(0.28, 0.3, 0.3);

/// The landings at the top of each flight of stairs, solid down to their Zone's floor.
const GATE_SHAFT_TOP: RaisedFloor = RaisedFloor { zone: ZoneId::GateShaft, min: v(-44.0, 15.0), max: v(-38.0, 18.0), height: CREST, colour: CONCRETE };
const SPILLWAY_GALLERY_TOP: RaisedFloor = RaisedFloor { zone: ZoneId::SpillwayGallery, min: v(38.0, 15.0), max: v(44.0, 18.0), height: CREST, colour: CONCRETE };
const PENSTOCK_TOP: RaisedFloor = RaisedFloor { zone: ZoneId::PenstockTunnel, min: v(-3.0, -9.0), max: v(3.0, -6.0), height: GALLERY, colour: CONCRETE };
const SPILLWAY_STAIR_TOP: RaisedFloor = RaisedFloor { zone: ZoneId::SpillwayStair, min: v(38.0, -35.0), max: v(44.0, -32.0), height: GALLERY, colour: CONCRETE };
const DRAINAGE_TOP: RaisedFloor = RaisedFloor { zone: ZoneId::DrainageGallery, min: v(-44.0, -62.0), max: v(-38.0, -56.0), height: GALLERY, colour: CONCRETE };
/// The turbine deck in the middle of the Turbine Hall: steps at its north end, a Ledge along its
/// other sides that a Player can scramble up and Zombies only drop from.
const TURBINE_DECK: RaisedFloor = RaisedFloor { zone: ZoneId::TurbineHall, min: v(-5.0, -48.0), max: v(5.0, -36.0), height: 1.0, colour: STEEL_PLATE };
/// The low dais in the Control Room that Pack-a-Punch stands on, steps up at its north end.
const DAIS: RaisedFloor = RaisedFloor { zone: ZoneId::ControlRoom, min: v(-3.0, -2.0), max: v(3.0, 4.0), height: GALLERY + 0.45, colour: TERRAZZO };

const RAISED_FLOORS: [RaisedFloor; 7] = [GATE_SHAFT_TOP, SPILLWAY_GALLERY_TOP, PENSTOCK_TOP, SPILLWAY_STAIR_TOP, DRAINAGE_TOP, TURBINE_DECK, DAIS];

/// The stairs between the levels, and the steps up onto the deck and the dais. The storeys are
/// joined only by stairs: down the Gate Shaft, the Spillway Gallery's end, the Penstock Tunnel,
/// the Spillway Stair and the west end of the Drainage Gallery.
const SLOPES: [Slope; 7] = [
    Slope { zone: ZoneId::GateShaft, foot: at(-41.0, GALLERY, 7.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: FLIGHT_WIDTH, steps: true, colour: CONCRETE },
    Slope { zone: ZoneId::SpillwayGallery, foot: at(41.0, GALLERY, 7.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: FLIGHT_WIDTH, steps: true, colour: CONCRETE },
    Slope { zone: ZoneId::PenstockTunnel, foot: at(0.0, FOOT, -17.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: FLIGHT_WIDTH, steps: true, colour: CONCRETE },
    Slope { zone: ZoneId::SpillwayStair, foot: at(41.0, FOOT, -43.0), up: Vec2::Y, run: 8.0, rise: STOREY, width: FLIGHT_WIDTH, steps: true, colour: CONCRETE },
    Slope { zone: ZoneId::DrainageGallery, foot: at(-30.0, FOOT, -59.0), up: Vec2::NEG_X, run: 8.0, rise: STOREY, width: FLIGHT_WIDTH, steps: true, colour: CONCRETE },
    Slope { zone: ZoneId::TurbineHall, foot: at(0.0, FOOT, -34.0), up: Vec2::NEG_Y, run: 2.0, rise: TURBINE_DECK.height, width: 3.0, steps: true, colour: STEEL_PLATE },
    Slope { zone: ZoneId::ControlRoom, foot: at(0.0, GALLERY, 5.2), up: Vec2::NEG_Y, run: 1.2, rise: 0.45, width: 3.0, steps: true, colour: TERRAZZO },
];

/// The Props. Most stand flush against a wall; the islands to run round are the hoist winches in
/// the Tower Head and the Gatehouse, the turbine deck, the ring of dial desks round the dais, the
/// spare runner, and the Switchgear Room's breaker cabinets.
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Crest Road: a maintenance truck by the valley parapet, the gate hoist housings by
        // the reservoir parapet, and bollards.
        prop(Machine, CrestRoad, (D - 10.0, CREST, 23.1), (4.0, 2.2, 1.8)),
        prop(Machine, CrestRoad, (D - 24.0, CREST, 29.0), (2.0, 2.2, 1.6)),
        prop(Machine, CrestRoad, (D + 24.0, CREST, 29.0), (2.0, 2.2, 1.6)),
        prop(Pillar, CrestRoad, (D - 6.0, CREST, 29.4), (0.4, 0.9, 0.4)),
        prop(Pillar, CrestRoad, (D + 6.0, CREST, 29.4), (0.4, 0.9, 0.4)),
        prop(Pillar, CrestRoad, (D - 4.0, CREST, 22.6), (0.4, 0.9, 0.4)),
        prop(Pillar, CrestRoad, (D + 4.0, CREST, 22.6), (0.4, 0.9, 0.4)),
        // The Tower Head: a hoist winch, and stop-log beams stacked by the wall.
        prop(Machine, TowerHead, (D - 37.0, CREST, 25.0), (2.4, 2.2, 1.6)),
        prop(Crate, TowerHead, (D - 34.0, CREST, 18.6), (3.0, 0.6, 0.8)),
        // The Gate Shaft: cable trays up the wall of its top landing.
        prop(Shelf, GateShaft, (D - 43.65, CREST, 16.5), (0.3, 2.4, 2.0)),
        // The Intake Gallery: trash-rack sections against its walls, and a rake cart.
        prop(Shelf, IntakeGallery, (D - 30.0, GALLERY, -3.55), (3.0, 2.4, 0.5)),
        prop(Shelf, IntakeGallery, (D - 38.45, GALLERY, -45.0), (0.5, 2.4, 3.0)),
        prop(Machine, IntakeGallery, (D - 43.0, GALLERY, -15.0), (1.4, 1.2, 2.0)),
        // The Spillway Gatehouse: the radial-gate winch, and a tool rack.
        prop(Machine, SpillwayGatehouse, (D + 37.0, CREST, 25.0), (2.4, 2.2, 1.6)),
        prop(Shelf, SpillwayGatehouse, (D + 34.0, CREST, 18.5), (2.4, 2.2, 0.6)),
        // The Spillway Gallery: a valve wheel on its stand, and a waist-high pipe run.
        prop(Machine, SpillwayGallery, (D + 16.0, GALLERY, -3.5), (0.6, 1.4, 0.6)),
        prop(Machine, SpillwayGallery, (D + 38.5, GALLERY, -16.0), (0.6, 1.0, 8.0)),
        // The Control Room: a ring of dial desks round the dais, and the mimic panel.
        prop(Counter, ControlRoom, (D - 6.0, GALLERY, -2.5), (0.9, 1.0, 2.4)),
        prop(Counter, ControlRoom, (D + 6.0, GALLERY, -2.5), (0.9, 1.0, 2.4)),
        prop(Counter, ControlRoom, (D - 6.0, GALLERY, 3.5), (0.9, 1.0, 2.4)),
        prop(Counter, ControlRoom, (D + 6.0, GALLERY, 3.5), (0.9, 1.0, 2.4)),
        prop(Shelf, ControlRoom, (D - 3.0, GALLERY, 7.5), (5.0, 2.4, 0.6)),
        // The Penstock Tunnel: the penstock alongside.
        prop(Machine, PenstockTunnel, (D + 2.1, FOOT, -22.5), (1.2, 1.2, 7.0)),
        // The Turbine Hall: a spare runner on its stand, and the crane's hook block let down
        // onto the deck.
        prop(Machine, TurbineHall, (D + 6.5, FOOT, -50.5), (2.4, 2.2, 2.4)),
        prop(Machine, TurbineHall, (D + 3.0, TURBINE_DECK.height, -46.0), (1.0, 1.2, 1.0)),
        // The Switchgear Room: two rows of breaker cabinets.
        prop(Shelf, SwitchgearRoom, (D + 15.5, FOOT, -46.0), (0.6, 2.2, 3.0)),
        prop(Shelf, SwitchgearRoom, (D + 21.0, FOOT, -46.0), (0.6, 2.2, 3.0)),
        // The Drainage Gallery: the sump pump.
        prop(Machine, DrainageGallery, (D + 8.0, FOOT, -61.1), (2.0, 1.4, 1.4)),
    ]
};

/// The walls, the Doors, the ceilings, the pockets outside the windows and the dam's body.
fn build(b: &mut Builder) {
    use ZoneId::*;
    const H: f32 = STOREY;
    let body = palette(Some(CrestRoad)).wall;

    // The crest: the Crest Road's parapets, and the Tower Head and the Gatehouse either end.
    b.outer_wall(v(-30.0, 30.0), v(30.0, 30.0), CREST, H);
    b.outer_wall(v(-30.0, 22.0), v(30.0, 22.0), CREST, H);
    b.wall(v(-30.0, 18.0), v(-30.0, 32.0), CREST, H, &[(8.0, door(CrestRoad, TowerHead, DOOR_PRICE_CREST_TOWER_HEAD))]);
    b.wall(v(30.0, 18.0), v(30.0, 32.0), CREST, H, &[(8.0, door(CrestRoad, SpillwayGatehouse, DOOR_PRICE_CREST_GATEHOUSE))]);
    b.outer_wall(v(-46.0, 32.0), v(-30.0, 32.0), CREST, H);
    b.outer_wall(v(-46.0, 18.0), v(-46.0, 32.0), CREST, H);
    b.wall(v(-46.0, 18.0), v(-30.0, 18.0), CREST, H, &[(5.0, door(TowerHead, GateShaft, DOOR_PRICE_TOWER_HEAD_GATE_SHAFT))]);
    b.outer_wall(v(30.0, 32.0), v(46.0, 32.0), CREST, H);
    b.outer_wall(v(46.0, 18.0), v(46.0, 32.0), CREST, H);
    b.wall(v(30.0, 18.0), v(46.0, 18.0), CREST, H, &[(11.0, door(SpillwayGatehouse, SpillwayGallery, DOOR_PRICE_GATEHOUSE_SPILLWAY_GALLERY))]);

    // The Gate Shaft, two storeys tall.
    b.outer_wall(v(-44.0, 2.0), v(-44.0, 18.0), GALLERY, 2.0 * H);
    b.wall(v(-38.0, 2.0), v(-38.0, 18.0), GALLERY, 2.0 * H, &[]);
    b.wall(v(-44.0, 2.0), v(-38.0, 2.0), GALLERY, 2.0 * H, &[(3.0, door(GateShaft, IntakeGallery, DOOR_PRICE_GATE_SHAFT_INTAKE))]);

    // The Intake Gallery: along to the Control Room, and down the dam's west side, with the dam's
    // body solid inside the bend.
    b.outer_wall(v(-38.0, 2.0), v(-10.0, 2.0), GALLERY, H);
    b.outer_wall(v(-44.0, -56.0), v(-44.0, 2.0), GALLERY, H);
    b.wall(v(-38.0, -4.0), v(-10.0, -4.0), GALLERY, H, &[]);
    b.wall(v(-38.0, -56.0), v(-38.0, -4.0), GALLERY, H, &[]);
    solid(b, v(-38.0, -56.0), v(-10.0, -4.0), GALLERY, CREST, body);
    b.wall(v(-44.0, -56.0), v(-38.0, -56.0), GALLERY, H, &[(3.0, door(DrainageGallery, IntakeGallery, DOOR_PRICE_DRAINAGE_INTAKE))]);

    // The Control Room.
    b.wall(v(-10.0, -6.0), v(-10.0, 8.0), GALLERY, H, &[(5.0, door(IntakeGallery, ControlRoom, DOOR_PRICE_INTAKE_CONTROL))]);
    b.wall(v(10.0, -6.0), v(10.0, 8.0), GALLERY, H, &[(5.0, door(SpillwayGallery, ControlRoom, DOOR_PRICE_SPILLWAY_GALLERY_CONTROL))]);
    b.outer_wall(v(-10.0, 8.0), v(10.0, 8.0), GALLERY, H);
    b.wall(v(-10.0, -6.0), v(10.0, -6.0), GALLERY, H, &[(10.0, door(ControlRoom, PenstockTunnel, DOOR_PRICE_CONTROL_PENSTOCK))]);

    // The Spillway Gallery: along from the Control Room, up its steps to the Gatehouse, and down
    // the dam's east side, the body solid either side.
    b.wall(v(10.0, 2.0), v(38.0, 2.0), GALLERY, 2.0 * H, &[]);
    b.wall(v(10.0, -4.0), v(38.0, -4.0), GALLERY, H, &[]);
    b.wall(v(38.0, -32.0), v(38.0, -4.0), GALLERY, H, &[]);
    b.wall(v(38.0, 2.0), v(38.0, 18.0), GALLERY, 2.0 * H, &[]);
    b.outer_wall(v(44.0, -32.0), v(44.0, 2.0), GALLERY, H);
    b.outer_wall(v(44.0, 2.0), v(44.0, 18.0), GALLERY, 2.0 * H);
    b.wall(v(38.0, -32.0), v(44.0, -32.0), GALLERY, H, &[(3.0, door(SpillwayGallery, SpillwayStair, DOOR_PRICE_SPILLWAY_GALLERY_STAIR))]);
    solid(b, v(10.0, -32.0), v(38.0, -4.0), GALLERY, CREST, body);
    solid(b, v(10.0, 2.0), v(38.0, 18.0), GALLERY, CREST + STOREY, body);
    solid(b, v(10.0, -32.0), v(44.0, 2.0), CREST, CREST + STOREY, body);

    // The Penstock Tunnel.
    b.outer_wall(v(-3.0, -28.0), v(-3.0, -6.0), FOOT, 2.0 * H);
    b.wall(v(3.0, -28.0), v(3.0, -6.0), FOOT, 2.0 * H, &[]);

    // The Turbine Hall, two storeys tall, and the Switchgear Room off it.
    b.wall(
        v(-10.0, -28.0),
        v(10.0, -28.0),
        FOOT,
        2.0 * H,
        &[(3.0, Gap::Window), (10.0, door(PenstockTunnel, TurbineHall, DOOR_PRICE_PENSTOCK_TURBINE_HALL))],
    );
    b.outer_wall(v(-10.0, -56.0), v(-10.0, -28.0), FOOT, 2.0 * H);
    b.wall(
        v(10.0, -56.0),
        v(10.0, -28.0),
        FOOT,
        2.0 * H,
        &[(10.0, door(TurbineHall, SwitchgearRoom, DOOR_PRICE_TURBINE_HALL_SWITCHGEAR)), (24.0, Gap::Window)],
    );
    b.wall(v(10.0, -40.0), v(26.0, -40.0), FOOT, H, &[]);
    b.wall(v(10.0, -52.0), v(26.0, -52.0), FOOT, H, &[]);
    b.outer_wall(v(26.0, -52.0), v(26.0, -40.0), FOOT, H);

    // The Spillway Stair.
    b.wall(v(38.0, -56.0), v(38.0, -32.0), FOOT, 2.0 * H, &[]);
    b.outer_wall(v(44.0, -56.0), v(44.0, -32.0), FOOT, 2.0 * H);

    // The Drainage Gallery, along the toe: the Doors up from it into the Turbine Hall and the
    // Spillway Stair on the ground, and the one into the Intake Gallery up at the top of its
    // stairs (built with the Intake Gallery).
    b.outer_wall(v(-44.0, -62.0), v(44.0, -62.0), FOOT, 2.0 * H);
    b.outer_wall(v(-44.0, -62.0), v(-44.0, -56.0), FOOT, 2.0 * H);
    b.outer_wall(v(44.0, -62.0), v(44.0, -56.0), FOOT, 2.0 * H);
    b.wall(
        v(-38.0, -56.0),
        v(44.0, -56.0),
        FOOT,
        H,
        &[
            (38.0, door(TurbineHall, DrainageGallery, DOOR_PRICE_TURBINE_HALL_DRAINAGE)),
            (79.0, door(SpillwayStair, DrainageGallery, DOOR_PRICE_SPILLWAY_STAIR_DRAINAGE)),
        ],
    );
    b.wall(v(-38.0, -56.0), v(44.0, -56.0), GALLERY, H, &[]);
    // Sump grates in its floor.
    let grate = palette(Some(DrainageGallery)).trim;
    for x in [-4.0, 14.0, 34.0] {
        b.block(Vec3::new(1.2, 0.02, 1.2), at(x, 0.01, -59.0), grate);
    }

    // The space under the upper half of each flight between storeys is boxed in solid, so nothing
    // is ever lost under the stairs.
    for slope in SLOPES.iter().filter(|slope| slope.rise >= STOREY) {
        let along = Vec3::new(slope.up.x, 0.0, slope.up.y);
        let across = Vec3::new(slope.up.y.abs(), 0.0, slope.up.x.abs());
        let tall = slope.rise / 2.0 - 0.4;
        let middle = slope.foot + along * slope.run * 0.75 + Vec3::Y * tall / 2.0;
        let size = across * slope.width + along.abs() * slope.run / 2.0 + Vec3::Y * tall;
        b.block(size, middle, slope.colour);
    }

    // The ceilings over every Zone but the Crest Road.
    for (zone, min, max, y) in [
        (TowerHead, v(-46.0, 18.0), v(-30.0, 32.0), CREST + STOREY),
        (SpillwayGatehouse, v(30.0, 18.0), v(46.0, 32.0), CREST + STOREY),
        (GateShaft, v(-44.0, 2.0), v(-38.0, 18.0), CREST + STOREY),
        (IntakeGallery, v(-44.0, -56.0), v(-10.0, 2.0), CREST),
        (SpillwayGallery, v(38.0, 2.0), v(44.0, 18.0), CREST + STOREY),
        (ControlRoom, v(-10.0, -6.0), v(10.0, 8.0), CREST),
        (PenstockTunnel, v(-3.0, -28.0), v(3.0, -6.0), CREST),
        (TurbineHall, v(-10.0, -56.0), v(10.0, -28.0), CREST),
        (SwitchgearRoom, v(10.0, -52.0), v(26.0, -40.0), GALLERY),
        (SpillwayStair, v(38.0, -56.0), v(44.0, -32.0), CREST),
        (DrainageGallery, v(-44.0, -62.0), v(44.0, -56.0), CREST),
    ] {
        let (size, mid) = (max - min, (min + max) / 2.0);
        b.block(Vec3::new(size.x, 0.3, size.y), Vec3::new(mid.x, y + 0.15, mid.y), palette(Some(zone)).trim);
    }

    // A pocket of floor outside every window, walled in, where Zombies gather.
    for &(_, centre, out) in &WINDOWS {
        pocket(b, centre, out);
    }
}

/// The dam's body: solid from `y0` to `y1` over `min`..`max`, so nothing is ever lost in it.
fn solid(b: &mut Builder, min: Vec2, max: Vec2, y0: f32, y1: f32, colour: Color) {
    let (size, mid) = (max - min, (min + max) / 2.0);
    b.block(Vec3::new(size.x, y1 - y0, size.y), Vec3::new(mid.x, (y0 + y1) / 2.0, mid.y), colour);
}

/// How far a window's pocket reaches out from its wall, and how wide it is along it.
const POCKET: f32 = 4.0;
/// How tall the walls round a pocket are: too tall to jump, so nothing is knocked out of it.
const POCKET_WALL: f32 = 2.6;

/// The pocket of floor outside the window at `centre` looking `out`: solid down to the ground, with
/// a wall round its three open sides.
fn pocket(b: &mut Builder, centre: Vec3, out: Vec2) {
    let outside = palette(None);
    let out3 = Vec3::new(out.x, 0.0, out.y);
    let across = Vec3::new(out.y.abs(), 0.0, out.x.abs());
    let depth = out3.abs();
    let floor = centre.y;
    let middle = centre + out3 * POCKET / 2.0;
    let thick = 0.2_f32.max(floor);
    let size = across * POCKET + depth * POCKET + Vec3::Y * thick;
    b.block(size, Vec3::new(middle.x, floor - thick / 2.0, middle.z), outside.floor);
    let wall = 0.3;
    let rise = Vec3::Y * (floor + POCKET_WALL / 2.0);
    let far = centre + out3 * (POCKET - wall / 2.0);
    b.block(across * POCKET + depth * wall + Vec3::Y * POCKET_WALL, Vec3::new(far.x, 0.0, far.z) + rise, outside.wall);
    for side in [-1.0, 1.0] {
        let end = middle + across * side * (POCKET / 2.0 - wall / 2.0);
        b.block(across * wall + depth * POCKET + Vec3::Y * POCKET_WALL, Vec3::new(end.x, 0.0, end.z) + rise, outside.wall);
    }
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // The Crest Road
    ("cr_w", D - 27.0, CREST, 26.0),
    ("cr_a", D - 18.0, CREST, 26.0),
    ("cr_b", D - 9.0, CREST, 26.5),
    ("cr_c", D, CREST, 26.0),
    ("cr_d", D + 9.0, CREST, 26.0),
    ("cr_e", D + 18.0, CREST, 26.0),
    ("cr_east", D + 27.0, CREST, 26.0),
    ("w0_in", D - 18.0, CREST, 28.5),
    ("w0_out", D - 18.0, CREST, 32.0),
    ("w1_in", D + 18.0, CREST, 28.5),
    ("w1_out", D + 18.0, CREST, 32.0),
    ("w2_in", D - 18.0, CREST, 23.5),
    ("w2_out", D - 18.0, CREST, 20.0),
    ("w3_in", D + 18.0, CREST, 23.5),
    ("w3_out", D + 18.0, CREST, 20.0),
    // The Tower Head, round its winch
    ("th_e", D - 32.5, CREST, 26.0),
    ("th_ne", D - 33.0, CREST, 29.5),
    ("th_se", D - 33.0, CREST, 22.0),
    ("th_s", D - 41.0, CREST, 20.5),
    ("th_sw", D - 41.0, CREST, 23.0),
    ("th_nw", D - 41.5, CREST, 29.0),
    ("w4_in", D - 44.5, CREST, 26.0),
    ("w4_out", D - 48.0, CREST, 26.0),
    // The Spillway Gatehouse, round its winch
    ("sg_w", D + 32.5, CREST, 26.0),
    ("sg_nw", D + 33.0, CREST, 29.5),
    ("sg_sw", D + 33.0, CREST, 22.0),
    ("sg_s", D + 41.0, CREST, 20.5),
    ("sg_se", D + 41.0, CREST, 23.0),
    ("sg_ne", D + 41.5, CREST, 29.0),
    ("w5_in", D + 44.5, CREST, 26.0),
    ("w5_out", D + 48.0, CREST, 26.0),
    // Down the Gate Shaft
    ("gs_top", D - 41.0, CREST, 16.0),
    ("gs_mid", D - 41.0, GALLERY + 2.0, 11.0),
    ("gs_bot", D - 41.0, GALLERY, 4.5),
    ("w6_in", D - 42.5, GALLERY, 4.0),
    ("w6_out", D - 46.0, GALLERY, 4.0),
    // The Intake Gallery, along and then down the dam's west side
    ("ig_j", D - 41.0, GALLERY, -1.0),
    ("ig_a", D - 32.0, GALLERY, -1.0),
    ("ig_b", D - 24.0, GALLERY, -1.0),
    ("ig_c", D - 16.0, GALLERY, -1.0),
    ("ig_e", D - 12.5, GALLERY, -1.0),
    ("w7_in", D - 24.0, GALLERY, 0.5),
    ("w7_out", D - 24.0, GALLERY, 4.0),
    ("ig_s1", D - 41.0, GALLERY, -10.0),
    ("ig_s2", D - 41.0, GALLERY, -20.0),
    ("ig_s3", D - 41.0, GALLERY, -30.0),
    ("w8_in", D - 42.5, GALLERY, -30.0),
    ("w8_out", D - 46.0, GALLERY, -30.0),
    ("ig_s4", D - 41.0, GALLERY, -40.0),
    ("ig_s5", D - 41.0, GALLERY, -48.0),
    ("ig_bot", D - 41.0, GALLERY, -53.5),
    // The Spillway Gallery: along, up its steps to the Gatehouse, and down the dam's east side
    ("sp_w", D + 12.5, GALLERY, -1.0),
    ("sp_a", D + 20.0, GALLERY, -1.0),
    ("sp_b", D + 28.0, GALLERY, -1.0),
    ("sp_c", D + 35.0, GALLERY, -1.0),
    ("sp_j", D + 41.0, GALLERY, -1.0),
    ("sp_up", D + 41.0, GALLERY, 4.5),
    ("sp_mid", D + 41.0, GALLERY + 2.0, 11.0),
    ("sp_top", D + 41.0, CREST, 16.0),
    ("sp_s1", D + 41.0, GALLERY, -10.0),
    ("sp_s2", D + 41.0, GALLERY, -18.0),
    ("w9_in", D + 42.5, GALLERY, -18.0),
    ("w9_out", D + 46.0, GALLERY, -18.0),
    ("sp_s3", D + 41.0, GALLERY, -26.0),
    ("sp_s4", D + 41.0, GALLERY, -30.5),
    // The Control Room: round the walls, between the desks and the dais, and up onto it
    ("ct_w", D - 8.5, GALLERY, -1.0),
    ("ct_sw", D - 8.5, GALLERY, -4.5),
    ("ct_sw_in", D - 4.3, GALLERY, -4.5),
    ("ct_s", D, GALLERY, -4.5),
    ("ct_se_in", D + 4.3, GALLERY, -4.5),
    ("ct_se", D + 8.5, GALLERY, -4.5),
    ("ct_e", D + 8.5, GALLERY, -1.0),
    ("ct_ne", D + 8.5, GALLERY, 6.5),
    ("ct_ne_in", D + 4.3, GALLERY, 6.3),
    ("ct_n", D, GALLERY, 6.3),
    ("ct_nw_in", D - 4.3, GALLERY, 6.3),
    ("ct_nw", D - 8.5, GALLERY, 6.5),
    ("ct_dais", D, DAIS.height, 1.5),
    ("w10_in", D + 7.0, GALLERY, 6.5),
    ("w10_out", D + 7.0, GALLERY, 10.0),
    // Down the Penstock Tunnel
    ("pt_top", D, GALLERY, -8.5),
    ("pt_mid", D, FOOT + 2.0, -13.0),
    ("pt_foot", D - 1.0, FOOT, -18.5),
    ("w11_in", D - 1.5, FOOT, -22.0),
    ("w11_out", D - 5.0, FOOT, -22.0),
    ("pt_bot", D, FOOT, -26.5),
    // The Turbine Hall: round the deck, up its steps and across it
    ("tu_n", D, FOOT, -30.5),
    ("tu_nw", D - 7.5, FOOT, -31.0),
    ("tu_ne", D + 7.5, FOOT, -31.0),
    ("w12_in", D - 7.0, FOOT, -29.5),
    ("w12_out", D - 7.0, FOOT, -26.0),
    ("w13_in", D - 8.5, FOOT, -34.0),
    ("w13_out", D - 12.0, FOOT, -34.0),
    ("w15_in", D + 8.5, FOOT, -32.0),
    ("w15_out", D + 12.0, FOOT, -32.0),
    ("tu_w1", D - 7.5, FOOT, -37.0),
    ("tu_w2", D - 7.5, FOOT, -44.0),
    ("tu_w3", D - 7.5, FOOT, -49.0),
    ("w14_in", D - 8.5, FOOT, -50.0),
    ("w14_out", D - 12.0, FOOT, -50.0),
    ("tu_sw", D - 7.5, FOOT, -53.5),
    ("tu_s", D, FOOT, -53.5),
    ("tu_se", D + 8.75, FOOT, -54.0),
    ("tu_e3", D + 8.75, FOOT, -50.5),
    ("tu_e2", D + 7.5, FOOT, -46.0),
    ("tu_e1", D + 7.5, FOOT, -37.0),
    ("tu_deck_n", D, TURBINE_DECK.height, -37.5),
    ("tu_deck_c", D, TURBINE_DECK.height, -42.0),
    ("tu_deck_w", D - 3.5, TURBINE_DECK.height, -42.0),
    ("tu_deck_s", D, TURBINE_DECK.height, -46.5),
    // The Switchgear Room, round and between the rows of cabinets
    ("sw_w", D + 12.0, FOOT, -46.0),
    ("sw_nw", D + 13.0, FOOT, -42.5),
    ("sw_sw", D + 13.0, FOOT, -49.5),
    ("sw_n", D + 18.25, FOOT, -42.5),
    ("sw_m", D + 18.25, FOOT, -46.0),
    ("sw_s", D + 18.25, FOOT, -49.5),
    ("sw_ne", D + 23.5, FOOT, -42.5),
    ("sw_se", D + 23.5, FOOT, -49.5),
    ("w16_in", D + 24.5, FOOT, -46.0),
    ("w16_out", D + 28.0, FOOT, -46.0),
    // Down the Spillway Stair
    ("ss_top", D + 41.0, GALLERY, -34.0),
    ("ss_mid", D + 41.0, FOOT + 2.0, -39.0),
    ("ss_foot", D + 41.0, FOOT, -45.0),
    ("ss_bot", D + 41.0, FOOT, -53.5),
    ("w17_in", D + 42.5, FOOT, -50.0),
    ("w17_out", D + 46.0, FOOT, -50.0),
    // The Drainage Gallery, from the top of its stairs at the west end along the toe
    ("dg_top", D - 41.0, GALLERY, -59.0),
    ("dg_edge", D - 38.7, GALLERY, -59.0),
    ("dg_mid", D - 34.0, FOOT + 2.0, -59.0),
    ("dg_foot", D - 28.0, FOOT, -59.0),
    ("dg_a", D - 20.0, FOOT, -59.0),
    ("w18_in", D - 20.0, FOOT, -60.5),
    ("w18_out", D - 20.0, FOOT, -64.0),
    ("dg_b", D - 10.0, FOOT, -59.0),
    ("dg_c", D, FOOT, -58.5),
    ("dg_d", D + 10.0, FOOT, -58.5),
    ("dg_e", D + 20.0, FOOT, -59.0),
    ("w19_in", D + 20.0, FOOT, -60.5),
    ("w19_out", D + 20.0, FOOT, -64.0),
    ("dg_f", D + 30.0, FOOT, -59.0),
    ("dg_g", D + 41.0, FOOT, -58.5),
];

const OPEN: Gate = Gate::Open;

/// The edges, by node name. Barrier edges run from the first node to the second.
const EDGES: &[(&str, &str, Gate)] = {
    use ZoneId::*;
    &[
        // The Crest Road
        ("cr_w", "cr_a", OPEN),
        ("cr_a", "cr_b", OPEN),
        ("cr_b", "cr_c", OPEN),
        ("cr_c", "cr_d", OPEN),
        ("cr_d", "cr_e", OPEN),
        ("cr_e", "cr_east", OPEN),
        ("cr_a", "w0_in", OPEN),
        ("cr_a", "w2_in", OPEN),
        ("cr_e", "w1_in", OPEN),
        ("cr_e", "w3_in", OPEN),
        ("w0_out", "w0_in", barrier(0)),
        ("w1_out", "w1_in", barrier(1)),
        ("w2_out", "w2_in", barrier(2)),
        ("w3_out", "w3_in", barrier(3)),
        ("cr_w", "th_e", Gate::Door(CrestRoad, TowerHead)),
        ("cr_east", "sg_w", Gate::Door(CrestRoad, SpillwayGatehouse)),
        // The Tower Head
        ("th_e", "th_ne", OPEN),
        ("th_e", "th_se", OPEN),
        ("th_se", "th_s", OPEN),
        ("th_se", "th_sw", OPEN),
        ("th_s", "th_sw", OPEN),
        ("th_sw", "th_nw", OPEN),
        ("th_sw", "w4_in", OPEN),
        ("th_nw", "w4_in", OPEN),
        ("th_nw", "th_ne", OPEN),
        ("w4_out", "w4_in", barrier(4)),
        ("th_s", "gs_top", Gate::Door(TowerHead, GateShaft)),
        // The Spillway Gatehouse
        ("sg_w", "sg_nw", OPEN),
        ("sg_w", "sg_sw", OPEN),
        ("sg_sw", "sg_s", OPEN),
        ("sg_sw", "sg_se", OPEN),
        ("sg_s", "sg_se", OPEN),
        ("sg_se", "sg_ne", OPEN),
        ("sg_se", "w5_in", OPEN),
        ("sg_ne", "w5_in", OPEN),
        ("sg_ne", "sg_nw", OPEN),
        ("w5_out", "w5_in", barrier(5)),
        ("sg_s", "sp_top", Gate::Door(SpillwayGatehouse, SpillwayGallery)),
        // The Gate Shaft
        ("gs_top", "gs_mid", OPEN),
        ("gs_mid", "gs_bot", OPEN),
        ("gs_bot", "w6_in", OPEN),
        ("w6_out", "w6_in", barrier(6)),
        ("gs_bot", "ig_j", Gate::Door(GateShaft, IntakeGallery)),
        // The Intake Gallery
        ("ig_j", "ig_a", OPEN),
        ("ig_a", "ig_b", OPEN),
        ("ig_b", "ig_c", OPEN),
        ("ig_c", "ig_e", OPEN),
        ("ig_b", "w7_in", OPEN),
        ("w7_out", "w7_in", barrier(7)),
        ("ig_j", "ig_s1", OPEN),
        ("ig_s1", "ig_s2", OPEN),
        ("ig_s2", "ig_s3", OPEN),
        ("ig_s3", "w8_in", OPEN),
        ("w8_out", "w8_in", barrier(8)),
        ("ig_s3", "ig_s4", OPEN),
        ("ig_s4", "ig_s5", OPEN),
        ("ig_s5", "ig_bot", OPEN),
        ("ig_e", "ct_w", Gate::Door(IntakeGallery, ControlRoom)),
        ("ig_bot", "dg_top", Gate::Door(DrainageGallery, IntakeGallery)),
        // The Spillway Gallery
        ("sp_w", "sp_a", OPEN),
        ("sp_a", "sp_b", OPEN),
        ("sp_b", "sp_c", OPEN),
        ("sp_c", "sp_j", OPEN),
        ("sp_j", "sp_up", OPEN),
        ("sp_up", "sp_mid", OPEN),
        ("sp_mid", "sp_top", OPEN),
        ("sp_j", "sp_s1", OPEN),
        ("sp_s1", "sp_s2", OPEN),
        ("sp_s2", "w9_in", OPEN),
        ("w9_out", "w9_in", barrier(9)),
        ("sp_s2", "sp_s3", OPEN),
        ("sp_s3", "sp_s4", OPEN),
        ("sp_w", "ct_e", Gate::Door(SpillwayGallery, ControlRoom)),
        ("sp_s4", "ss_top", Gate::Door(SpillwayGallery, SpillwayStair)),
        // The Control Room
        ("ct_w", "ct_sw", OPEN),
        ("ct_w", "ct_nw", OPEN),
        ("ct_sw", "ct_sw_in", OPEN),
        ("ct_sw_in", "ct_s", OPEN),
        ("ct_s", "ct_se_in", OPEN),
        ("ct_se_in", "ct_se", OPEN),
        ("ct_se", "ct_e", OPEN),
        ("ct_e", "ct_ne", OPEN),
        ("ct_ne", "w10_in", OPEN),
        ("w10_in", "ct_ne_in", OPEN),
        ("ct_ne_in", "ct_n", OPEN),
        ("ct_n", "ct_nw_in", OPEN),
        ("ct_nw_in", "ct_nw", OPEN),
        ("ct_sw_in", "ct_nw_in", OPEN),
        ("ct_se_in", "ct_ne_in", OPEN),
        ("ct_n", "ct_dais", OPEN),
        ("w10_out", "w10_in", barrier(10)),
        ("ct_s", "pt_top", Gate::Door(ControlRoom, PenstockTunnel)),
        // The Penstock Tunnel
        ("pt_top", "pt_mid", OPEN),
        ("pt_mid", "pt_foot", OPEN),
        ("pt_foot", "w11_in", OPEN),
        ("pt_foot", "pt_bot", OPEN),
        ("w11_in", "pt_bot", OPEN),
        ("w11_out", "w11_in", barrier(11)),
        ("pt_bot", "tu_n", Gate::Door(PenstockTunnel, TurbineHall)),
        // The Turbine Hall
        ("tu_n", "tu_nw", OPEN),
        ("tu_n", "tu_ne", OPEN),
        ("tu_nw", "w12_in", OPEN),
        ("tu_nw", "w13_in", OPEN),
        ("tu_nw", "tu_w1", OPEN),
        ("w13_in", "tu_w1", OPEN),
        ("tu_w1", "tu_w2", OPEN),
        ("tu_w2", "tu_w3", OPEN),
        ("tu_w3", "w14_in", OPEN),
        ("tu_w3", "tu_sw", OPEN),
        ("w14_in", "tu_sw", OPEN),
        ("tu_sw", "tu_s", OPEN),
        ("tu_s", "tu_se", OPEN),
        ("tu_se", "tu_e3", OPEN),
        ("tu_e3", "tu_e2", OPEN),
        ("tu_e2", "tu_e1", OPEN),
        ("tu_e1", "tu_ne", OPEN),
        ("tu_ne", "w15_in", OPEN),
        ("tu_e1", "w15_in", OPEN),
        ("tu_n", "tu_deck_n", OPEN),
        ("tu_deck_n", "tu_deck_c", OPEN),
        ("tu_deck_c", "tu_deck_w", OPEN),
        ("tu_deck_c", "tu_deck_s", OPEN),
        ("w12_out", "w12_in", barrier(12)),
        ("w13_out", "w13_in", barrier(13)),
        ("w14_out", "w14_in", barrier(14)),
        ("w15_out", "w15_in", barrier(15)),
        ("tu_e2", "sw_w", Gate::Door(TurbineHall, SwitchgearRoom)),
        ("tu_s", "dg_c", Gate::Door(TurbineHall, DrainageGallery)),
        // The Switchgear Room
        ("sw_w", "sw_nw", OPEN),
        ("sw_w", "sw_sw", OPEN),
        ("sw_nw", "sw_n", OPEN),
        ("sw_sw", "sw_s", OPEN),
        ("sw_n", "sw_m", OPEN),
        ("sw_m", "sw_s", OPEN),
        ("sw_n", "sw_ne", OPEN),
        ("sw_s", "sw_se", OPEN),
        ("sw_ne", "w16_in", OPEN),
        ("sw_se", "w16_in", OPEN),
        ("w16_out", "w16_in", barrier(16)),
        // The Spillway Stair
        ("ss_top", "ss_mid", OPEN),
        ("ss_mid", "ss_foot", OPEN),
        ("ss_foot", "w17_in", OPEN),
        ("ss_foot", "ss_bot", OPEN),
        ("w17_in", "ss_bot", OPEN),
        ("w17_out", "w17_in", barrier(17)),
        ("ss_bot", "dg_g", Gate::Door(SpillwayStair, DrainageGallery)),
        // The Drainage Gallery
        ("dg_top", "dg_edge", OPEN),
        ("dg_edge", "dg_mid", OPEN),
        ("dg_mid", "dg_foot", OPEN),
        ("dg_foot", "dg_a", OPEN),
        ("dg_a", "w18_in", OPEN),
        ("dg_a", "dg_b", OPEN),
        ("dg_b", "dg_c", OPEN),
        ("dg_c", "dg_d", OPEN),
        ("dg_d", "dg_e", OPEN),
        ("dg_e", "w19_in", OPEN),
        ("dg_e", "dg_f", OPEN),
        ("dg_f", "dg_g", OPEN),
        ("w18_out", "w18_in", barrier(18)),
        ("w19_out", "w19_in", barrier(19)),
    ]
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::maps::map::{MapId, WINDOWS as ALL_WINDOWS};
    use ZoneId::*;

    fn dam() -> &'static Layout {
        MapId::Dam.layout()
    }

    /// The Dam's 12 Zones stand on three levels, and a Run starts on the Crest Road, up on the
    /// crest.
    #[test]
    fn the_dam_is_twelve_zones_on_three_levels_starting_on_the_crest_road() {
        let zones = dam().zones;
        assert_eq!(zones.len(), 12);
        assert_eq!(dam().start_zone(), CrestRoad);
        let level = |zone: ZoneId| zone.floor();
        assert_eq!(level(CrestRoad), 2.0 * STOREY);
        for zone in [TowerHead, SpillwayGatehouse] {
            assert_eq!(level(zone), 2.0 * STOREY, "{} is on the crest", zone.name());
        }
        for zone in [IntakeGallery, SpillwayGallery, ControlRoom, GateShaft] {
            assert_eq!(level(zone), STOREY, "{} is in the gallery", zone.name());
        }
        for zone in [TurbineHall, SwitchgearRoom, DrainageGallery, PenstockTunnel, SpillwayStair] {
            assert_eq!(level(zone), 0.0, "{} is at the foot", zone.name());
        }
    }

    #[test]
    fn the_dam_stands_at_dam_x() {
        assert_eq!(DAM_X, 400.0);
        let (min, max) = dam().plan;
        assert!(min.x > crate::maps::map::QUARRY_X + 38.0, "clear of the Quarry");
        assert!((min.x + max.x) / 2.0 == DAM_X);
    }

    /// Every Zone but the Crest Road has a ceiling: straight up from any of its path nodes there's
    /// solid within two storeys. The Crest Road is open to the sky.
    #[test]
    fn every_zone_but_the_crest_road_has_a_ceiling() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::nav::zone_at;
        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;

        let mut sim = Sim::on(MapId::Dam, 60.0, |_| {});
        let open: Vec<(String, Option<ZoneId>)> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                NODES
                    .iter()
                    .filter(|(name, ..)| !name.ends_with("_out"))
                    .map(|&(name, x, y, z)| (name, Vec3::new(x, y, z)))
                    .filter(|&(_, feet)| spatial.cast_ray(feet + Vec3::Y * 2.0, Dir3::Y, 2.0 * STOREY, true, &filter).is_none())
                    .map(|(name, feet)| (name.to_string(), zone_at(feet + Vec3::Y * 0.1)))
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert!(open.iter().all(|(_, zone)| *zone == Some(CrestRoad)), "open to the sky: {open:?}");
        assert!(open.len() >= 7, "the Crest Road is open to the sky");
    }

    /// The 14 Doors are all bought, at the layout's prices, and none waits for the Power.
    #[test]
    fn fourteen_bought_doors_at_their_prices_and_no_power_doors() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::maps::door::Door;

        let prices: HashMap<(ZoneId, ZoneId), u64> = HashMap::from([
            ((CrestRoad, TowerHead), 750),
            ((CrestRoad, SpillwayGatehouse), 750),
            ((TowerHead, GateShaft), 1000),
            ((GateShaft, IntakeGallery), 1000),
            ((SpillwayGatehouse, SpillwayGallery), 1000),
            ((SpillwayGallery, SpillwayStair), 1000),
            ((IntakeGallery, ControlRoom), 1250),
            ((SpillwayGallery, ControlRoom), 1250),
            ((ControlRoom, PenstockTunnel), 1000),
            ((PenstockTunnel, TurbineHall), 1250),
            ((TurbineHall, SwitchgearRoom), 1000),
            ((TurbineHall, DrainageGallery), 1000),
            ((DrainageGallery, IntakeGallery), 1250),
            ((SpillwayStair, DrainageGallery), 1000),
        ]);
        let mut sim = Sim::on(MapId::Dam, 60.0, |_| {});
        let doors: Vec<(Option<(ZoneId, ZoneId)>, Option<u64>)> = sim
            .world_mut()
            .run_system_once(|doors: Query<&Door>| doors.iter().map(|door| (door.joins, door.price)).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(doors.len(), 14);
        for (joins, price) in doors {
            let (a, b) = joins.expect("every Door joins two Zones");
            let want = prices.get(&(a, b)).or_else(|| prices.get(&(b, a)));
            assert_eq!(price, want.copied(), "the Door between {} and {}", a.name(), b.name());
        }
    }

    /// The Doors between Zones that the path nodes cross.
    fn joins() -> Vec<(ZoneId, ZoneId)> {
        EDGES
            .iter()
            .filter_map(|&(.., gate)| match gate {
                Gate::Door(a, b) => Some((a, b)),
                _ => None,
            })
            .collect()
    }

    fn joined(a: ZoneId, b: ZoneId) -> bool {
        joins().iter().any(|&join| join == (a, b) || join == (b, a))
    }

    /// The crest has only two ways off it, and the foot ring along the toe closes a Train loop
    /// round the Control Room each side: down the Intake Gallery, or the Spillway Gallery and its
    /// Stair, along the Drainage Gallery, up through the Turbine Hall and the Penstock Tunnel.
    #[test]
    fn the_foot_ring_closes_both_loops_round_the_control_room() {
        let off_the_crest: Vec<_> = joins().into_iter().filter(|&(a, b)| a == CrestRoad || b == CrestRoad).collect();
        assert_eq!(off_the_crest.len(), 2);
        let west = [ControlRoom, IntakeGallery, DrainageGallery, TurbineHall, PenstockTunnel];
        let east = [ControlRoom, SpillwayGallery, SpillwayStair, DrainageGallery, TurbineHall, PenstockTunnel];
        for ring in [&west[..], &east[..]] {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                assert!(joined(a, b), "no Door between {} and {}", a.name(), b.name());
            }
        }
    }

    /// Zombies come in only through Barriers: 4 on the Crest Road, 3 or 4 in the Turbine Hall, and
    /// 1 or 2 in every other Zone. There are no Risers and no Vents.
    #[test]
    fn barriers_only_four_on_the_crest_road() {
        assert!(dam().risers.is_empty());
        assert!(dam().vents.is_empty());
        for &zone in dam().zones {
            let count = MapId::Dam.barriers().filter(|barrier| ALL_WINDOWS[barrier.0 as usize].0 == zone).count();
            let want = match zone {
                CrestRoad => 4..=4,
                TurbineHall => 3..=4,
                _ => 1..=2,
            };
            assert!(want.contains(&count), "{} has {count} Barriers", zone.name());
        }
    }

    /// Specials rise only on the Crest Road, in the Turbine Hall and in the Drainage Gallery, and
    /// Hellhounds land there most.
    #[test]
    fn specials_rise_on_the_crest_road_in_the_turbine_hall_and_in_the_drainage_gallery() {
        let mut zones: Vec<ZoneId> = dam().special_spawns.iter().map(|&(zone, _)| zone).collect();
        zones.dedup();
        assert_eq!(zones, [CrestRoad, TurbineHall, DrainageGallery]);
        for zone in [CrestRoad, TurbineHall, DrainageGallery] {
            assert_eq!(dam().hellhound_spawns.iter().filter(|(z, _)| *z == zone).count(), 2);
        }
    }

    /// Six Box Locations, starting in the Gate Shaft, none on the crest or in the Control Room.
    #[test]
    fn six_box_locations_starting_in_the_gate_shaft() {
        let zones: Vec<ZoneId> = MapId::Dam.box_locations().map(|i| crate::maps::map::BOX_LOCATIONS[i].0).collect();
        assert_eq!(zones, [GateShaft, SpillwayGallery, SpillwayStair, TurbineHall, DrainageGallery, SwitchgearRoom]);
    }

    /// The Dam's ways round: along the crest, down the Gate Shaft and along the Intake Gallery,
    /// down the Penstock Tunnel, onto the turbine deck, along the toe and up the Spillway Stair,
    /// round the Switchgear Room's cabinets, and up the Control Room's dais to Pack-a-Punch. A
    /// Zombie walks every one to the Player.
    #[test]
    fn a_zombie_follows_the_player_round_every_way_of_the_dam() {
        use crate::enemies::zombie::headless::follows;
        let ways = [
            ("along the crest into the Tower Head", at(10.0, CREST, 26.0), at(-38.0, CREST, 29.0)),
            ("down the Gate Shaft", at(-36.0, CREST, 21.0), at(-41.0, GALLERY, -1.0)),
            ("along the Intake Gallery into the Control Room", at(-41.0, GALLERY, -20.0), at(-8.0, GALLERY, 6.0)),
            ("up onto the dais", at(8.5, GALLERY, -4.5), at(0.0, DAIS.height, 1.0)),
            ("down the Penstock Tunnel", at(-8.0, GALLERY, 6.0), at(0.0, FOOT, -26.0)),
            ("onto the turbine deck", at(-7.5, FOOT, -52.0), at(2.0, TURBINE_DECK.height, -40.0)),
            ("round the breaker cabinets", at(7.5, FOOT, -46.0), at(23.5, FOOT, -46.0)),
            ("down the Drainage Gallery's stairs", at(-41.0, GALLERY, -48.0), at(-10.0, FOOT, -59.0)),
            ("along the toe and up the Spillway Stair", at(20.0, FOOT, -59.0), at(41.0, GALLERY, -26.0)),
            ("up the Spillway Gallery to the Gatehouse", at(28.0, GALLERY, -1.0), at(36.0, CREST, 29.0)),
        ];
        for (way, start, player) in ways {
            let (arrived, last) = follows(MapId::Dam, start, player);
            assert!(arrived, "{way}: last seen at {last}");
        }
    }
}
