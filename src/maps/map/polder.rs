//! The Polder Map: a canal town behind a sea dyke, in 16 Zones. Two Canals cross it east to west,
//! the Town Canal and the Harbour, and a twin lock in the middle joins them, with Pack-a-Punch on
//! its island. The town is at y 0 and the dyke's crest at y 4, with the sea beyond (+z).
//!
//! Its water is the Tide's: three Canals (the Town Canal, the Harbour and the Sea) at -1.7, -1.1
//! and -0.4 for Low, Mid and High. A Canal is Deep everywhere but at its four Fords, where the
//! channel (-2.2) can be waded at Low and the berms (-1.6) at Mid. The four Swing Bridges and the
//! two Lock Gates (the Inland Gate in the West Chamber, the Sea Gate in the East) are Shifting
//! Pieces: the bridges are turned by Levers and the gates by the Tide, so exactly one gate top is
//! a walkway onto the island at each phase.
//!
//! - South bank: the Town Square (start), the Weigh House, the Brewery, the Cheese Market and the
//!   Almshouse Court.
//! - Between the Canals: the Windmill, the Warehouse, the Lock Yard, the Sluice House (the island,
//!   Pack-a-Punch), the Toll Quay, the Shipyard and the Coal Yard.
//! - North bank: the Herring Quay and the Customs House.
//! - The dyke (y 4): the Dyke Road and the Pumping Station (the Power).
//!
//! Three ways climb the dyke, each owned by the Zone at its top with its Door at the foot: the
//! Windmill Ramp (D11), the Dyke Steps (D12) and the Coal Stair (PD3). Three drops run beside those
//! Doors, each a `Gate::DoorDrop`, never a fixed join. The Humpback Bridge is the one fixed way
//! over the Town Canal.
//!
//! The whole Map is centred on the world's origin along x (ADR 0009). Its path
//! nodes and pools (in `data`) are laid out from this file's walls, Props and canals.

use bevy::prelude::*;

use crate::enemies::sapper::SapperTiming;
use crate::maps::shifting::{Driver, Leaf, Motion, Piece, Rest};
use crate::maps::zone::ZoneId;
use crate::tuning::*;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{Canal, Infill, Layout, Prop, PropKind, RaisedFloor, STOREY, Slope, Sunken, infill, palette, prop};

#[path = "polder/data.rs"]
mod data;

/// The levels: the town, and the dyke's crest a storey up.
const GROUND: f32 = 0.0;
const CREST: f32 = STOREY;
/// How tall a wall is, a rail along a quay, and the iron fence along a Ford's far bank.
const H: f32 = STOREY;
const RAIL: f32 = 1.1;
const FENCE: f32 = 1.8;
/// The seaward parapet's height between the windows.
const PARAPET: f32 = 1.6;
/// How high a chalk outline's centre sits over its floor.
const CHALK: f32 = 1.5;
/// The bed of every Deep reach, the Fords' berms and channel, and the Low level.
const BED: f32 = -4.5;
const BERM: f32 = -1.6;
const CHANNEL: f32 = -2.2;

/// The first of the Polder's windows in `WINDOWS`, after the other Maps'.
const W0: u8 = (super::greybox::WINDOWS.len()
    + super::quarry::WINDOWS.len()
    + super::village::WINDOWS.len()
    + super::dam::WINDOWS.len()
    + super::cove::WINDOWS.len()) as u8;

/// How wide the stairs are: the two with a Trap at their foot are as wide as the Dam's.
const FLIGHT: f32 = 4.0;
const TRAP_FLIGHT: f32 = 5.6;
/// How far from its Door a trapped flight starts to climb, so the switches have a landing.
const LANDING: f32 = 3.0;
/// How long a flight up the dyke is.
const RUN: f32 = 6.0;

/// A point in the Polder, `x` along from its middle.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A spot on a wall or a floor's plan in the Polder, `x` along from its middle.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const fn p(kind: PropKind, zone: ZoneId, x: f32, y: f32, z: f32, size: (f32, f32, f32)) -> Prop {
    prop(kind, zone, (x, y, z), size)
}

pub(super) const POLDER: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [TownSquare, WeighHouse, Brewery, CheeseMarket, AlmshouseCourt, Windmill, Warehouse, LockYard, SluiceHouse, TollQuay, Shipyard, CoalYard, HerringQuay, CustomsHouse, DykeRoad, PumpingStation]
    },
    // In a row across the Square's south side, facing the Canal.
    seats: [at(-10.0, GROUND, -32.0), at(-6.5, GROUND, -32.0), at(-3.0, GROUND, -32.0), at(0.5, GROUND, -32.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &[],
    wading: &WADING_POINTS,
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &[],
    raised_floors: &RAISED_FLOORS,
    infills: &INFILLS,
    slopes: &SLOPES,
    sunken: &data::SUNKEN,
    water: &data::POOLS,
    canals: &CANALS,
    props: PROPS,
    lamps: &data::LAMPS,
    fixed_joins: &[],
    shifting: &SHIFTING,
    wings: &[],
    nav_nodes: data::NODES,
    nav_edges: data::EDGES,
    jump_pads: &[],
    ziplines: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    outdoor: &[
        ZoneId::TownSquare,
        ZoneId::CheeseMarket,
        ZoneId::AlmshouseCourt,
        ZoneId::Windmill,
        ZoneId::LockYard,
        ZoneId::TollQuay,
        ZoneId::Shipyard,
        ZoneId::CoalYard,
        ZoneId::HerringQuay,
        ZoneId::DykeRoad,
    ],
    roofed: true,
    sun: Vec3::new(1.0, -3.4, 4.7),
    look: crate::maps::look::NORTHERN_DUSK,
    lamp_style: super::LampStyle::Bulb,
    bulb_zones: &[],
    outside: Some(super::Outside { floor: super::textures::Material::CobblestoneFloor06, wall: super::textures::Material::DarkBrickWall }),
    plan: (Vec2::new(-80.0, -56.0), Vec2::new(80.0, 66.0)),
    build,
};

/// A Swing Bridge's deck: 13 x 3.5 m, 0.4 thick, its top at quay level.
const DECK: Vec3 = Vec3::new(3.5, 0.4, 13.0);

/// The Leaf of a bridge whose deck's centre line is `x` and whose hinge is at the bank's edge `z`,
/// hinged on the deck's east edge (swinging east) or west edge.
const fn deck_leaf(x: f32, z: f32, east: bool) -> Leaf {
    let side = if east { 1.0 } else { -1.0 };
    Leaf { hinge: at(x + side * DECK.x / 2.0, GROUND, z), size: DECK, offset: Vec3::new(-side * DECK.x / 2.0, -DECK.y / 2.0, DECK.z / 2.0), motion: Motion::Hinge }
}

const SPAN_ALONG: f32 = std::f32::consts::FRAC_PI_2;
const FRAC_PI_2: f32 = std::f32::consts::FRAC_PI_2;

/// A Lock Gate's Rests, in order: Shut (the Leaves meet mid-chamber) and Open (folded back).
const SHUT: u8 = 0;
const OPEN_GATE: u8 = 1;

/// One Leaf of a Lock Gate: 1.4 m across (a 1 m walkway between 0.2 m rails) and 3.5 m long, half
/// a chamber, its top at quay level. `across` is where the box sits from the hinge across the
/// Leaf's own width, so that folded Open it lies inside the chamber.
const fn gate_leaf(x: f32, z: f32, across: f32) -> Leaf {
    Leaf { hinge: at(x, GROUND, z), size: Vec3::new(1.4, 0.4, 3.5), offset: Vec3::new(across, -0.2, 1.75), motion: Motion::Hinge }
}

/// The four Swing Bridges, in the order Market, Herring, Cheese, Customs: each a single Leaf
/// hinged at its near bank, with Span (the start) and Along (swung 90 degrees over the water along
/// its bank, joining nothing). Each has a Lever outside its pen at either landing.
const SHIFTING: [Piece; 6] = [
    Piece {
        zone: ZoneId::TownSquare,
        leaves: &[deck_leaf(-22.0, -12.0, true)],
        rests: &[
            Rest { at: &[0.0], joins: &[(ZoneId::TownSquare, ZoneId::LockYard)] },
            Rest { at: &[SPAN_ALONG], joins: &[] },
        ],
        start: 0,
        driver: Driver::Levers(&[
            (ZoneId::TownSquare, at(-25.0, GROUND, -13.2), Vec2::new(0.0, -1.0)),
            (ZoneId::LockYard, at(-25.0, GROUND, 1.6), Vec2::new(0.0, 1.0)),
        ]),
        door: Some((ZoneId::TownSquare, ZoneId::LockYard)),
    },
    Piece {
        zone: ZoneId::LockYard,
        leaves: &[deck_leaf(-24.0, 24.0, false)],
        rests: &[
            Rest { at: &[0.0], joins: &[(ZoneId::LockYard, ZoneId::HerringQuay)] },
            Rest { at: &[-SPAN_ALONG], joins: &[] },
        ],
        start: 0,
        driver: Driver::Levers(&[
            (ZoneId::LockYard, at(-20.5, GROUND, 22.8), Vec2::new(0.0, -1.0)),
            (ZoneId::HerringQuay, at(-20.5, GROUND, 37.4), Vec2::new(0.0, 1.0)),
        ]),
        door: Some((ZoneId::LockYard, ZoneId::HerringQuay)),
    },
    Piece {
        zone: ZoneId::CheeseMarket,
        leaves: &[deck_leaf(44.0, -12.0, true)],
        rests: &[
            Rest { at: &[0.0], joins: &[(ZoneId::CheeseMarket, ZoneId::Shipyard)] },
            Rest { at: &[SPAN_ALONG], joins: &[] },
        ],
        start: 0,
        driver: Driver::Levers(&[
            (ZoneId::CheeseMarket, at(40.0, GROUND, -13.2), Vec2::new(0.0, -1.0)),
            (ZoneId::Shipyard, at(40.0, GROUND, 1.6), Vec2::new(0.0, 1.0)),
        ]),
        door: Some((ZoneId::CheeseMarket, ZoneId::Shipyard)),
    },
    Piece {
        zone: ZoneId::TollQuay,
        leaves: &[deck_leaf(24.0, 24.0, true)],
        rests: &[
            Rest { at: &[0.0], joins: &[(ZoneId::TollQuay, ZoneId::CustomsHouse)] },
            Rest { at: &[SPAN_ALONG], joins: &[] },
        ],
        start: 0,
        driver: Driver::Levers(&[
            (ZoneId::TollQuay, at(19.5, GROUND, 22.8), Vec2::new(0.0, -1.0)),
            (ZoneId::CustomsHouse, at(19.5, GROUND, 37.4), Vec2::new(0.0, 1.0)),
        ]),
        door: Some((ZoneId::TollQuay, ZoneId::CustomsHouse)),
    },
    // The Inland Gate (the West Chamber's south end): Shut at Low and Rising, Open at High and
    // Falling. The Leaves are hinged on the lock wall (x -13) and the island's (x -6) and meet
    // mid-chamber; Open, they fold north against the walls.
    Piece {
        zone: ZoneId::SluiceHouse,
        leaves: &[gate_leaf(-13.0, 3.2, 0.7), gate_leaf(-6.0, 3.2, -0.7)],
        rests: &[
            Rest { at: &[FRAC_PI_2, -FRAC_PI_2], joins: &[(ZoneId::LockYard, ZoneId::SluiceHouse)] },
            Rest { at: &[0.0, 0.0], joins: &[] },
        ],
        start: 0,
        driver: Driver::Tide([SHUT, SHUT, OPEN_GATE, OPEN_GATE]),
        door: Some((ZoneId::LockYard, ZoneId::SluiceHouse)),
    },
    // The Sea Gate (the East Chamber's north end): Open at Low and Rising, Shut at High and
    // Falling. Open, its Leaves fold south against the walls.
    Piece {
        zone: ZoneId::SluiceHouse,
        leaves: &[gate_leaf(6.0, 20.8, -0.7), gate_leaf(13.0, 20.8, 0.7)],
        rests: &[
            Rest { at: &[FRAC_PI_2, -FRAC_PI_2], joins: &[(ZoneId::TollQuay, ZoneId::SluiceHouse)] },
            Rest { at: &[std::f32::consts::PI, -std::f32::consts::PI], joins: &[] },
        ],
        start: 1,
        driver: Driver::Tide([OPEN_GATE, OPEN_GATE, SHUT, SHUT]),
        door: Some((ZoneId::TollQuay, ZoneId::SluiceHouse)),
    },
];

/// The chalk outlines: the cheap guns either side of Quick Revive on the Square's south wall, and
/// one gun in each of four other Zones.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    (WeaponId::M14, ZoneId::TownSquare, at(0.0, GROUND + CHALK, -39.8), Vec2::Y),
    (WeaponId::Olympia, ZoneId::TownSquare, at(10.0, GROUND + CHALK, -39.8), Vec2::Y),
    (WeaponId::Mp40, ZoneId::WeighHouse, at(-26.2, GROUND + CHALK, -18.0), Vec2::NEG_X),
    (WeaponId::TrenchGun, ZoneId::LockYard, at(-33.8, GROUND + CHALK, 6.0), Vec2::X),
    (WeaponId::Ak74u, ZoneId::CustomsHouse, at(30.0, GROUND + CHALK, 47.8), Vec2::NEG_Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first, in the Brewery, two Doors
/// from the Square (D1 and D3). None is in the Square, the Sluice House, the Pumping Station or on
/// the Dyke Road.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // Against the Brewery's west wall.
    (ZoneId::Brewery, at(-79.35, GROUND, -26.0), Vec2::X),
    // Against the Almshouse Court's south wall.
    (ZoneId::AlmshouseCourt, at(0.0, GROUND, -55.35), Vec2::Y),
    // Against the Warehouse's west wall, north of its Door.
    (ZoneId::Warehouse, at(-55.35, GROUND, 20.0), Vec2::X),
    // Against the Shipyard's east wall.
    (ZoneId::Shipyard, at(57.35, GROUND, 20.0), Vec2::NEG_X),
    // Against the Customs House's north wall.
    (ZoneId::CustomsHouse, at(50.0, GROUND, 47.35), Vec2::NEG_Y),
    // Against the Coal Yard's east wall.
    (ZoneId::CoalYard, at(79.35, GROUND, 30.0), Vec2::NEG_X),
];

const fn window(zone: ZoneId, x: f32, y: f32, z: f32, out: Vec2) -> (ZoneId, Vec3, Vec2) {
    (zone, at(x, y, z), out)
}

/// The 30 windows, each opening onto a pocket of floor walled in outside it: some in the free
/// ground round the south of the town, some in a partition that fences a strip off a Zone's own
/// floor, and the Dyke's two over the sea.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 30] = {
    use ZoneId::*;
    [
        // 0-1: the Town Square's shopfronts, in a partition across its south-west corner.
        window(TownSquare, -22.0, GROUND, -36.0, Vec2::NEG_Y),
        window(TownSquare, -18.0, GROUND, -36.0, Vec2::NEG_Y),
        // 2-3: the Weigh House's hall windows, over the ground south of it.
        window(WeighHouse, -50.0, GROUND, -40.0, Vec2::NEG_Y),
        window(WeighHouse, -42.0, GROUND, -40.0, Vec2::NEG_Y),
        // 4-5: the Brewery's brewhouse windows.
        window(Brewery, -56.0, GROUND, -52.0, Vec2::X),
        window(Brewery, -56.0, GROUND, -46.0, Vec2::X),
        // 6-7: the Cheese Market's shutters.
        window(CheeseMarket, 45.0, GROUND, -40.0, Vec2::NEG_Y),
        window(CheeseMarket, 50.0, GROUND, -40.0, Vec2::NEG_Y),
        // 8-9: the almshouses' windows.
        window(AlmshouseCourt, -26.0, GROUND, -52.0, Vec2::NEG_X),
        window(AlmshouseCourt, -26.0, GROUND, -46.0, Vec2::NEG_X),
        // 10-11: the Windmill's: one at the foot, one onto the stage from the cap.
        window(Windmill, -76.0, GROUND, 6.0, Vec2::NEG_X),
        window(Windmill, -68.0, CREST, 36.0, Vec2::Y),
        // 12-13: the Warehouse's loading hatches.
        window(Warehouse, -38.0, GROUND, 4.0, Vec2::X),
        window(Warehouse, -38.0, GROUND, 8.0, Vec2::X),
        // 14-15: the lock keeper's hut window and a culvert grate.
        window(LockYard, -32.0, GROUND, 20.0, Vec2::Y),
        window(LockYard, -28.0, GROUND, 20.0, Vec2::Y),
        // 16: the sluice culvert grate at the island's north tip.
        window(SluiceHouse, 0.0, GROUND, 20.0, Vec2::Y),
        // 17: the tollhouse window.
        window(TollQuay, 30.0, GROUND, 19.0, Vec2::X),
        // 18-19: the shipyard's shed windows.
        window(Shipyard, 54.0, GROUND, 4.0, Vec2::X),
        window(Shipyard, 54.0, GROUND, 8.0, Vec2::X),
        // 20-21: the bunker hatches.
        window(CoalYard, 76.0, GROUND, 6.0, Vec2::X),
        window(CoalYard, 76.0, GROUND, 10.0, Vec2::X),
        // 22-23: the smokehouse windows.
        window(HerringQuay, -42.0, GROUND, 44.0, Vec2::Y),
        window(HerringQuay, -38.0, GROUND, 44.0, Vec2::Y),
        // 24-25: the bonded stores' windows.
        window(CustomsHouse, 30.0, GROUND, 44.0, Vec2::Y),
        window(CustomsHouse, 34.0, GROUND, 44.0, Vec2::Y),
        // 26-27: the dyke's seaward parapet, where Zombies climb the sea face.
        window(DykeRoad, -30.0, CREST, 60.0, Vec2::Y),
        window(DykeRoad, 10.0, CREST, 60.0, Vec2::Y),
        // 28-29: the boiler house's windows.
        window(PumpingStation, 55.0, CREST, 60.0, Vec2::Y),
        window(PumpingStation, 70.0, CREST, 60.0, Vec2::Y),
    ]
};

/// The Hellhound Spawn Points: one in every Zone, and two each in the Town Square, the Herring
/// Quay and the Shipyard.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 19] = [
    (ZoneId::TownSquare, at(-8.0, GROUND, -20.0)),
    (ZoneId::TownSquare, at(8.0, GROUND, -26.0)),
    (ZoneId::WeighHouse, at(-34.0, GROUND, -30.0)),
    (ZoneId::Brewery, at(-68.0, GROUND, -38.0)),
    (ZoneId::CheeseMarket, at(36.0, GROUND, -18.0)),
    (ZoneId::AlmshouseCourt, at(10.0, GROUND, -48.0)),
    (ZoneId::Windmill, at(-68.0, GROUND, 12.0)),
    (ZoneId::Warehouse, at(-46.0, GROUND, 16.0)),
    (ZoneId::LockYard, at(-24.0, GROUND, 9.0)),
    (ZoneId::SluiceHouse, at(0.0, GROUND, 2.5)),
    (ZoneId::TollQuay, at(24.0, GROUND, 9.0)),
    (ZoneId::Shipyard, at(46.0, GROUND, 8.0)),
    (ZoneId::Shipyard, at(46.0, GROUND, 16.0)),
    (ZoneId::CoalYard, at(68.0, GROUND, 20.0)),
    (ZoneId::HerringQuay, at(-30.0, GROUND, 40.0)),
    (ZoneId::HerringQuay, at(0.0, GROUND, 40.0)),
    (ZoneId::CustomsHouse, at(44.0, GROUND, 40.0)),
    (ZoneId::DykeRoad, at(-45.0, CREST, 54.0)),
    (ZoneId::PumpingStation, at(54.0, CREST, 56.0)),
];

/// The Special Spawn Points, where a Napalm or a Shrieker rises: the Town Square, the Herring
/// Quay, the Shipyard and the Dyke Road.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 4] = [
    (ZoneId::TownSquare, at(-14.0, GROUND, -28.0)),
    (ZoneId::HerringQuay, at(-16.0, GROUND, 41.0)),
    (ZoneId::Shipyard, at(52.0, GROUND, 14.0)),
    (ZoneId::DykeRoad, at(-6.0, CREST, 54.0)),
];

/// The Canals, each at the layout's levels: the channel (-2.2) is Shallow at Low, the berms (-1.6)
/// at Mid, and everything is Deep at High. The reaches (-4.5) are Deep at every level.
const CANALS: [Canal; 3] = [
    Canal { name: "Town Canal", low: -1.7, mid: -1.1, high: -0.4 },
    Canal { name: "Harbour", low: -1.7, mid: -1.1, high: -0.4 },
    Canal { name: "Sea", low: -1.7, mid: -1.1, high: -0.4 },
];

/// The Wading Points, three to a Ford: one in its channel, 4 m along from its stairs (active at
/// Low), and two on its own berm, the pool Zone's side (active at Rising and Falling, at Mid). None
/// is on a far berm.
const WADING_POINTS: [(ZoneId, Vec3); 12] = [
    (ZoneId::WeighHouse, at(-47.0, -2.2, -6.0)),
    (ZoneId::CheeseMarket, at(22.0, -2.2, -6.0)),
    (ZoneId::HerringQuay, at(-52.0, -2.2, 30.0)),
    (ZoneId::CustomsHouse, at(43.0, -2.2, 30.0)),
    (ZoneId::WeighHouse, at(-47.0, BERM, -10.25)),
    (ZoneId::WeighHouse, at(-39.0, BERM, -10.25)),
    (ZoneId::CheeseMarket, at(22.5, BERM, -10.25)),
    (ZoneId::CheeseMarket, at(29.5, BERM, -10.25)),
    (ZoneId::HerringQuay, at(-51.5, BERM, 34.25)),
    (ZoneId::HerringQuay, at(-44.5, BERM, 34.25)),
    (ZoneId::CustomsHouse, at(43.0, BERM, 34.25)),
    (ZoneId::CustomsHouse, at(51.0, BERM, 34.25)),
];

/// Quay stone, timber and iron, for the pieces that aren't a Zone's wall.
const STONE: Color = Color::srgb(0.42, 0.42, 0.42);
const SETT: Color = Color::srgb(0.36, 0.36, 0.38);
const TIMBER: Color = Color::srgb(0.16, 0.12, 0.09);
const IRON: Color = Color::srgb(0.13, 0.14, 0.15);
const WHITE: Color = Color::srgb(0.9, 0.9, 0.86);

// The Raised Floors.
/// Pack-a-Punch's winch platform, between the two sluice winches, up steps from the north.
const WINCH: RaisedFloor = RaisedFloor { zone: ZoneId::SluiceHouse, min: v(-2.5, 7.0), max: v(2.5, 13.0), height: GROUND + 0.6, colour: IRON };
/// The Weigh House's scale dais, a step up with a ramp.
const SCALE: RaisedFloor = RaisedFloor { zone: ZoneId::WeighHouse, min: v(-46.0, -32.0), max: v(-38.0, -26.0), height: GROUND + 0.4, colour: TIMBER };
/// The Cheese Market's weighing dais, a low stage with steps at both ends.
const WEIGHING: RaisedFloor = RaisedFloor { zone: ZoneId::CheeseMarket, min: v(28.0, -32.0), max: v(38.0, -26.0), height: GROUND + 0.5, colour: TIMBER };
/// The Windmill's stage at the dyke's height, on the mill's stone base.
const STAGE: RaisedFloor = RaisedFloor { zone: ZoneId::Windmill, min: v(-73.0, 25.0), max: v(-63.0, 41.0), height: CREST, colour: STONE };
/// The Humpback Bridge's deck, 4 m wide over the Town Canal, at the top of its two Slopes.
const HUMPBACK: RaisedFloor = RaisedFloor { zone: ZoneId::Brewery, min: v(-70.0, -12.0), max: v(-66.0, 0.0), height: GROUND + 1.5, colour: STONE };

const RAISED_FLOORS: [RaisedFloor; 5] = [WINCH, SCALE, WEIGHING, STAGE, HUMPBACK];

/// A water stair: a flight 2 m wide down the quay wall from the quay to a berm, with its landing
/// (above the High waterline) on the way.
const fn water_stair(zone: ZoneId, x: f32, bank_z: f32, up: Vec2) -> Slope {
    // `bank_z` is the quay edge the flight climbs to; its foot is on the berm, 3.5 m off it.
    let foot_z = bank_z - up.y * 3.5;
    Slope { zone, foot: at(x, BERM, foot_z), up, run: 3.5, rise: -BERM, width: 2.0, steps: true, colour: STONE }
}

/// A ramp from a Ford's channel (-2.2) up to a berm (-1.6), 2 m wide, in line with a water stair.
const fn ford_ramp(zone: ZoneId, x: f32, foot_z: f32, up: Vec2) -> Slope {
    Slope { zone, foot: at(x, CHANNEL, foot_z), up, run: 1.5, rise: BERM - CHANNEL, width: 2.0, steps: false, colour: STONE }
}

/// The stairs: the dyke's three, the Windmill's, the water stairs, the Humpback Bridge's two
/// approaches, and the little ones up onto the Raised Floors.
const SLOPES: [Slope; 26] = [
    // The Windmill Ramp (D11), the Dyke Steps (D12, Trap 2) and the Coal Stair (PD3, Trap 3).
    Slope { zone: ZoneId::DykeRoad, foot: at(-70.0, GROUND, 48.2), up: Vec2::Y, run: RUN, rise: STOREY, width: FLIGHT, steps: false, colour: STONE },
    Slope { zone: ZoneId::DykeRoad, foot: at(-20.0, GROUND, 48.2 + LANDING), up: Vec2::Y, run: RUN, rise: STOREY, width: TRAP_FLIGHT, steps: true, colour: STONE },
    Slope { zone: ZoneId::PumpingStation, foot: at(68.0, GROUND, 48.2 + LANDING), up: Vec2::Y, run: RUN, rise: STOREY, width: TRAP_FLIGHT, steps: true, colour: STONE },
    // The Windmill's stage.
    Slope { zone: ZoneId::Windmill, foot: at(-68.0, GROUND, 19.0), up: Vec2::Y, run: RUN, rise: STOREY, width: 3.0, steps: true, colour: STONE },
    // The water stairs, an own one and a far one at each Ford.
    water_stair(ZoneId::WeighHouse, -43.0, -12.0, Vec2::NEG_Y),
    water_stair(ZoneId::WeighHouse, -43.0, 0.0, Vec2::Y),
    water_stair(ZoneId::CheeseMarket, 26.0, -12.0, Vec2::NEG_Y),
    water_stair(ZoneId::CheeseMarket, 26.0, 0.0, Vec2::Y),
    water_stair(ZoneId::HerringQuay, -48.0, 36.0, Vec2::Y),
    water_stair(ZoneId::HerringQuay, -48.0, 24.0, Vec2::NEG_Y),
    water_stair(ZoneId::CustomsHouse, 47.0, 36.0, Vec2::Y),
    water_stair(ZoneId::CustomsHouse, 47.0, 24.0, Vec2::NEG_Y),
    // The Humpback Bridge: up from the Brewery's yard to the deck, and down to the Windmill's.
    Slope { zone: ZoneId::Brewery, foot: at(-68.0, GROUND, -20.0), up: Vec2::Y, run: 8.0, rise: 1.5, width: 4.0, steps: false, colour: STONE },
    Slope { zone: ZoneId::Windmill, foot: at(-68.0, GROUND, 8.0), up: Vec2::NEG_Y, run: 8.0, rise: 1.5, width: 4.0, steps: false, colour: STONE },
    // Up onto the winch platform, the scale dais and the weighing dais (from both ends).
    Slope { zone: ZoneId::SluiceHouse, foot: at(0.0, GROUND, 14.2), up: Vec2::NEG_Y, run: 1.2, rise: 0.6, width: 3.0, steps: true, colour: IRON },
    Slope { zone: ZoneId::WeighHouse, foot: at(-42.0, GROUND, -24.8), up: Vec2::NEG_Y, run: 1.2, rise: 0.4, width: 3.0, steps: false, colour: TIMBER },
    Slope { zone: ZoneId::CheeseMarket, foot: at(26.8, GROUND, -29.0), up: Vec2::X, run: 1.2, rise: 0.5, width: 3.0, steps: true, colour: TIMBER },
    Slope { zone: ZoneId::CheeseMarket, foot: at(39.2, GROUND, -29.0), up: Vec2::NEG_X, run: 1.2, rise: 0.5, width: 3.0, steps: true, colour: TIMBER },
    // The Fords' ramps, from each berm down to the channel, in line with its stair.
    ford_ramp(ZoneId::WeighHouse, -43.0, -7.0, Vec2::NEG_Y),
    ford_ramp(ZoneId::WeighHouse, -43.0, -5.0, Vec2::Y),
    ford_ramp(ZoneId::CheeseMarket, 26.0, -7.0, Vec2::NEG_Y),
    ford_ramp(ZoneId::CheeseMarket, 26.0, -5.0, Vec2::Y),
    ford_ramp(ZoneId::HerringQuay, -48.0, 31.0, Vec2::Y),
    ford_ramp(ZoneId::HerringQuay, -48.0, 29.0, Vec2::NEG_Y),
    ford_ramp(ZoneId::CustomsHouse, 47.0, 31.0, Vec2::Y),
    ford_ramp(ZoneId::CustomsHouse, 47.0, 29.0, Vec2::NEG_Y),
];

/// The wells the three flights up the dyke climb out of, each in the floor of the Zone at its top,
/// a storey deep, with a balustrade along each side.
pub(super) const WELLS: [Sunken; 3] = [
    Sunken { zone: ZoneId::DykeRoad, min: v(-72.0, 48.0), max: v(-68.0, 48.2 + RUN), depth: STOREY, rim: false },
    Sunken { zone: ZoneId::DykeRoad, min: v(-22.8, 48.0), max: v(-17.2, 48.2 + LANDING + RUN), depth: STOREY, rim: false },
    Sunken { zone: ZoneId::PumpingStation, min: v(65.2, 48.0), max: v(70.8, 48.2 + LANDING + RUN), depth: STOREY, rim: false },
];

/// The Infills, one to three in each indoor Zone, so no two rooms share a shape. Each keeps clear
/// of the Doors, windows, Buyables, Spawn Points and every path-node edge, none comes more than
/// 1.1 m out of a wall, and none stands in or beside a Canal, a Ford, a stairwell or a Swing
/// Bridge's span or landings: the water's extent is its highest, so they clear it at every Tide.
const INFILLS: [Infill; 17] = {
    use ZoneId::*;
    [
        // The Weigh House: a chimney breast in the west wall, a stair core in the east wall's
        // dry corner, and a pier in the south wall between the two windows.
        infill(WeighHouse, (-56.0, -20.5), (-55.0, -17.5)),
        infill(WeighHouse, (-27.0, -16.0), (-26.0, -13.6)),
        infill(WeighHouse, (-46.8, -40.0), (-45.2, -39.0)),
        // The Brewery: a cupboard in the north-west corner, a flue breast in the west wall past
        // the Box, and a pier in the north wall.
        infill(Brewery, (-80.0, -56.0), (-79.0, -55.0)),
        infill(Brewery, (-80.0, -35.5), (-79.0, -32.5)),
        infill(Brewery, (-68.5, -56.0), (-67.0, -55.0)),
        // The Warehouse: a hoist-motor cupboard in the north-west corner, a nib in the west wall
        // between its Door and the Box, and a buttress in the east wall past Speed Cola.
        infill(Warehouse, (-56.0, 0.0), (-54.9, 1.4)),
        infill(Warehouse, (-56.0, 14.6), (-55.1, 17.2)),
        infill(Warehouse, (-35.0, 20.0), (-34.0, 22.0)),
        // The Sluice House: a gear housing against each long wall inside, offset along it.
        infill(SluiceHouse, (-5.0, 8.5), (-4.1, 11.5)),
        infill(SluiceHouse, (4.1, 12.4), (5.0, 14.0)),
        // The Customs House: a pier in the north wall, a strongroom in the north-west corner and
        // a bonded-store nib in the east wall.
        infill(CustomsHouse, (38.5, 47.0), (40.0, 48.0)),
        infill(CustomsHouse, (14.0, 44.8), (15.0, 48.0)),
        infill(CustomsHouse, (57.0, 44.5), (58.0, 47.0)),
        // The Pumping Station: a stair core in the corner by the dyke's edge, a pier in the
        // seaward wall between the windows, and a cupboard in the far corner.
        infill(PumpingStation, (44.0, 48.0), (45.0, 49.0)),
        infill(PumpingStation, (59.6, 58.9), (60.8, 60.0)),
        infill(PumpingStation, (78.9, 58.9), (80.0, 60.0)),
    ]
};

/// The Props. Most stand flush against a wall; the islands to run round are the Square's pump and
/// its stalls, the Brewery's vats, the Cheese Market's racks, the hull on the Shipyard's stocks and
/// the Coal Yard's heaps. The Levers' plinths (8) and the tide boards (6) are here too.
const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Town Square: the pump on its 5 m base, stalls, bollards, and a Lever for the
        // Market Bridge.
        p(Barrel, TownSquare, -4.0, GROUND, -24.0, (5.0, 1.4, 5.0)),
        p(Counter, TownSquare, 8.0, GROUND, -18.0, (3.0, 1.1, 1.2)),
        p(Counter, TownSquare, -14.0, GROUND, -16.0, (3.0, 1.1, 1.2)),
        p(Crate, TownSquare, 14.0, GROUND, -30.0, (1.2, 1.0, 1.2)),
        p(Barrel, TownSquare, 15.5, GROUND, -14.0, (0.5, 0.9, 0.5)),
        p(Barrel, TownSquare, 1.0, GROUND, -13.6, (0.5, 0.9, 0.5)),
        p(Machine, TownSquare, -16.5, GROUND, -13.0, (0.5, 1.1, 0.5)),
        p(Shelf, TownSquare, -12.0, GROUND, -11.6, (0.3, 2.0, 1.0)),
        // The Weigh House: the great beam scale on its dais, sacks and barrels.
        p(Machine, WeighHouse, -42.0, GROUND + 0.4, -29.0, (2.4, 2.4, 1.0)),
        p(Crate, WeighHouse, -53.5, GROUND, -37.0, (1.4, 1.2, 1.4)),
        p(Barrel, WeighHouse, -30.0, GROUND, -37.5, (0.9, 1.0, 0.9)),
        p(Barrel, WeighHouse, -52.5, GROUND, -20.0, (0.9, 1.0, 0.9)),
        p(Crate, WeighHouse, -32.0, GROUND, -36.0, (1.2, 1.2, 1.2)),
        // The Brewery: copper vats, barrel stacks, a dray cart in the yard.
        p(Barrel, Brewery, -72.0, GROUND, -44.0, (2.4, 3.0, 2.4)),
        p(Barrel, Brewery, -66.0, GROUND, -44.0, (2.4, 3.0, 2.4)),
        p(Barrel, Brewery, -72.0, GROUND, -34.0, (2.4, 3.0, 2.4)),
        p(Crate, Brewery, -78.5, GROUND, -50.0, (1.6, 1.4, 1.6)),
        p(Barrel, Brewery, -77.5, GROUND, -40.0, (0.9, 1.0, 0.9)),
        p(Counter, Brewery, -64.0, GROUND, -28.0, (3.0, 0.9, 1.4)),
        // The Cheese Market: waist-high cheese racks, porters' sledges.
        p(Shelf, CheeseMarket, 24.0, GROUND, -36.0, (0.5, 1.0, 4.0)),
        p(Shelf, CheeseMarket, 34.0, GROUND, -20.0, (4.0, 1.0, 0.5)),
        p(Shelf, CheeseMarket, 44.0, GROUND, -30.0, (0.5, 1.0, 4.0)),
        p(Crate, CheeseMarket, 50.0, GROUND, -20.0, (1.2, 0.9, 1.2)),
        p(Machine, CheeseMarket, 35.0, GROUND + 0.5, -29.0, (1.4, 1.2, 0.8)),
        p(Machine, CheeseMarket, 47.5, GROUND, -15.0, (0.5, 1.1, 0.5)),
        p(Shelf, CheeseMarket, 51.0, GROUND, -13.6, (0.3, 2.0, 1.0)),
        // The Almshouse Court: a small garden, a well, benches.
        p(Barrel, AlmshouseCourt, 8.0, GROUND, -48.0, (1.6, 0.9, 1.6)),
        p(Bench, AlmshouseCourt, -16.0, GROUND, -52.0, (3.0, 0.9, 0.7)),
        p(Bench, AlmshouseCourt, 22.0, GROUND, -52.0, (3.0, 0.9, 0.7)),
        p(Rubble, AlmshouseCourt, 30.0, GROUND, -46.0, (1.6, 0.9, 1.6)),
        p(Rubble, AlmshouseCourt, -10.0, GROUND, -44.0, (1.4, 0.8, 1.4)),
        // The Windmill: millstones, sacks, still sails; the stage's cap on the stone base.
        p(Barrel, Windmill, -64.0, GROUND, 6.0, (1.4, 0.6, 1.4)),
        p(Crate, Windmill, -78.0, GROUND, 18.0, (1.6, 1.2, 1.6)),
        p(Crate, Windmill, -60.0, GROUND, 22.0, (1.4, 1.1, 1.4)),
        p(Counter, Windmill, -60.0, GROUND, 40.0, (2.0, 1.0, 1.2)),
        p(Pillar, Windmill, -72.0, CREST, 40.0, (0.5, 3.0, 0.5)),
        p(Pillar, Windmill, -64.0, CREST, 40.0, (0.5, 3.0, 0.5)),
        // The Warehouse: hoist beams, crates, sacks by both water doors.
        p(Crate, Warehouse, -52.0, GROUND, 4.0, (1.6, 1.4, 1.6)),
        p(Crate, Warehouse, -50.0, GROUND, 20.0, (1.6, 1.4, 1.6)),
        p(Crate, Warehouse, -42.0, GROUND, 12.0, (1.2, 1.2, 1.2)),
        p(Pillar, Warehouse, -45.0, GROUND, 8.0, (0.6, 4.0, 0.6)),
        p(Pillar, Warehouse, -45.0, GROUND, 17.0, (0.6, 4.0, 0.6)),
        p(Shelf, Warehouse, -36.5, GROUND, 17.5, (0.5, 2.2, 3.0)),
        // The Lock Yard: the lock keeper's hut, capstans, bollards, a tide board.
        p(Crate, LockYard, -29.0, GROUND, 8.0, (2.4, 2.2, 2.4)),
        p(Barrel, LockYard, -18.0, GROUND, 14.0, (0.7, 0.9, 0.7)),
        p(Barrel, LockYard, -26.0, GROUND, 16.0, (0.7, 0.9, 0.7)),
        p(Shelf, LockYard, -17.5, GROUND, 1.6, (0.3, 2.0, 1.0)),
        p(Machine, LockYard, -17.5, GROUND, 4.0, (0.5, 1.1, 0.5)),
        p(Machine, LockYard, -18.5, GROUND, 21.5, (0.5, 1.1, 0.5)),
        p(Shelf, LockYard, -20.5, GROUND, 22.0, (1.0, 2.0, 0.3)),
        // The Sluice House: the two winches and their gear wheels.
        p(Machine, SluiceHouse, -2.0, GROUND + 0.6, 11.0, (0.9, 1.6, 0.9)),
        p(Machine, SluiceHouse, 2.0, GROUND + 0.6, 11.0, (0.9, 1.6, 0.9)),
        // The Toll Quay: the tollhouse booth, a barrier pole, a tide board.
        p(Crate, TollQuay, 20.0, GROUND, 8.0, (1.8, 2.2, 1.8)),
        p(Counter, TollQuay, 26.0, GROUND, 15.0, (1.0, 1.0, 2.2)),
        p(Shelf, TollQuay, 17.0, GROUND, 18.0, (0.3, 2.0, 1.0)),
        p(Machine, TollQuay, 31.5, GROUND, 23.3, (0.5, 1.1, 0.5)),
        p(Machine, TollQuay, 20.5, GROUND, 22.5, (0.5, 1.1, 0.5)),
        // The Shipyard: a hull on the stocks (a wall to weave round), timber stacks.
        p(Counter, Shipyard, 44.0, GROUND, 12.0, (10.0, 2.4, 3.0)),
        p(Crate, Shipyard, 38.0, GROUND, 4.0, (3.0, 1.6, 1.4)),
        p(Crate, Shipyard, 38.0, GROUND, 20.0, (3.0, 1.6, 1.4)),
        p(Machine, Shipyard, 48.0, GROUND, 1.2, (0.5, 1.1, 0.5)),
        p(Machine, Shipyard, 39.0, GROUND, 1.3, (0.5, 1.1, 0.5)),
        // The Coal Yard: coal heaps (tall), a hoist frame, wagons on rails.
        p(Rubble, CoalYard, 68.0, GROUND, 8.0, (4.0, 2.4, 4.0)),
        p(Rubble, CoalYard, 72.0, GROUND, 30.0, (4.0, 2.4, 4.0)),
        p(Rubble, CoalYard, 64.0, GROUND, 38.0, (3.0, 2.0, 3.0)),
        p(Machine, CoalYard, 66.0, GROUND, 24.0, (2.4, 1.4, 1.4)),
        p(Pillar, CoalYard, 78.0, GROUND, 24.0, (0.6, 3.5, 0.6)),
        // The Herring Quay: herring barrels, net racks, the smokehouse; barges lie beyond the rail.
        p(Barrel, HerringQuay, -50.0, GROUND, 40.0, (0.9, 1.0, 0.9)),
        p(Barrel, HerringQuay, -34.0, GROUND, 39.0, (0.9, 1.0, 0.9)),
        p(Shelf, HerringQuay, -28.0, GROUND, 41.0, (3.0, 2.2, 0.5)),
        p(Shelf, HerringQuay, -6.0, GROUND, 42.0, (3.0, 2.2, 0.5)),
        p(Crate, HerringQuay, 6.0, GROUND, 41.0, (1.4, 1.2, 1.4)),
        p(Machine, HerringQuay, -26.5, GROUND, 38.0, (0.5, 1.1, 0.5)),
        p(Shelf, HerringQuay, -22.0, GROUND, 38.0, (1.0, 2.0, 0.3)),
        // The Customs House: counters, bonded casks, a wall clock.
        p(Counter, CustomsHouse, 24.0, GROUND, 42.0, (4.0, 1.1, 1.0)),
        p(Barrel, CustomsHouse, 46.0, GROUND, 41.0, (0.9, 1.0, 0.9)),
        p(Barrel, CustomsHouse, 54.0, GROUND, 41.0, (0.9, 1.0, 0.9)),
        p(Machine, CustomsHouse, 28.0, GROUND, 38.5, (0.5, 1.1, 0.5)),
        p(Shelf, CustomsHouse, 20.0, GROUND, 37.5, (0.3, 2.0, 1.0)),
        // The Dyke Road: bollards, low sandbag stacks, a beacon mast.
        p(Sandbags, DykeRoad, -50.0, CREST, 56.0, (2.4, 1.0, 0.8)),
        p(Sandbags, DykeRoad, -2.0, CREST, 56.0, (2.4, 1.0, 0.8)),
        p(Barrel, DykeRoad, 20.0, CREST, 55.0, (0.5, 0.8, 0.5)),
        p(Pillar, DykeRoad, 30.0, CREST, 55.0, (0.5, 6.0, 0.5)),
        p(Crate, DykeRoad, -60.0, CREST, 56.0, (1.2, 1.0, 1.2)),
        // The Pumping Station: the beam engine (the centrepiece), boilers.
        p(Machine, PumpingStation, 62.0, CREST, 54.0, (6.0, 2.6, 2.4)),
        p(Machine, PumpingStation, 49.0, CREST, 52.0, (2.0, 2.2, 2.0)),
        p(Machine, PumpingStation, 76.0, CREST, 52.0, (2.0, 2.2, 2.0)),
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

fn door(a: ZoneId, b: ZoneId, price: u64) -> Gap {
    Gap::Door(Some((a, b)), Some(price))
}

/// The gate Door of a Swing Bridge: bought like a Door, but it joins nothing itself.
fn gate(a: ZoneId, b: ZoneId, price: u64) -> Gap {
    Gap::Gate((a, b), price)
}

/// The gate Door of a Lock Gate: a Power Door that joins nothing, so the gate's Rest joins the
/// Zones once Power is on and it stands Shut.
fn pgate(a: ZoneId, b: ZoneId) -> Gap {
    Gap::PowerGate((a, b))
}

/// A Power Door.
fn pdoor(a: ZoneId, b: ZoneId) -> Gap {
    Gap::Door(Some((a, b)), None)
}

/// A pocket of floor outside the window at `centre` looking `out`, walled in, where Zombies gather,
/// standing on `base` (the ground, or the sea bed under the dyke).
fn pocket(b: &mut Builder, centre: Vec3, out: Vec2) {
    let base = if centre.y >= CREST && centre.z >= 60.0 { BED } else { 0.0 };
    const POCKET: f32 = 4.0;
    const WALL: f32 = 0.3;
    const HEIGHT: f32 = 2.6;
    let outside = palette(None);
    let out3 = Vec3::new(out.x, 0.0, out.y);
    let across = Vec3::new(out.y.abs(), 0.0, out.x.abs());
    let depth = out3.abs();
    let floor = centre.y;
    let middle = centre + out3 * POCKET / 2.0;
    let thick = (floor - base).max(0.2);
    let size = across * POCKET + depth * POCKET + Vec3::Y * thick;
    b.block(size, Vec3::new(middle.x, floor - thick / 2.0, middle.z), outside.floor);
    let rise = Vec3::Y * (floor + HEIGHT / 2.0);
    let far = centre + out3 * (POCKET - WALL / 2.0);
    b.block(across * POCKET + depth * WALL + Vec3::Y * HEIGHT, Vec3::new(far.x, 0.0, far.z) + rise, outside.wall);
    for side in [-1.0, 1.0] {
        let end = middle + across * side * (POCKET / 2.0 - WALL / 2.0);
        b.block(across * WALL + depth * POCKET + Vec3::Y * HEIGHT, Vec3::new(end.x, 0.0, end.z) + rise, outside.wall);
    }
}

/// The walls and Doors of the town, the dyke and the bridges.
fn build(b: &mut Builder) {
    use ZoneId::*;
    // The town wall: the Brewery's, the canal's culverts, the Windmill's, and the dyke's ends.
    zwall(b, -80.0, (-56.0, -12.0), GROUND, H, vec![]);
    zwall(b, -80.0, (-12.0, 0.0), -4.5, 8.5, vec![]);
    zwall(b, -80.0, (0.0, 48.0), GROUND, H, vec![]);
    zwall(b, -80.0, (48.0, 60.0), CREST, H, vec![]);
    zwall(b, 80.0, (-12.0, 0.0), -4.5, 8.5, vec![]);
    zwall(b, 80.0, (0.0, 48.0), GROUND, H, vec![]);
    zwall(b, 80.0, (48.0, 60.0), CREST, H, vec![]);
    xwall(b, (-80.0, -56.0), -56.0, GROUND, H, vec![]);
    xwall(b, (-26.0, 40.0), -56.0, GROUND, H, vec![]);

    // The south bank's south wall: the Weigh House's, the Square's and the Cheese Market's over
    // the Almshouse Court, with the Doors to the Court, and the shopfronts' strip.
    xwall(b, (-56.0, -26.0), -40.0, GROUND, H, vec![(-50.0, Gap::Window), (-42.0, Gap::Window)]);
    xwall(b, (-26.0, 18.0), -40.0, GROUND, H, vec![(-6.0, door(TownSquare, AlmshouseCourt, DOOR_PRICE_TOWN_SQUARE_ALMSHOUSE_COURT))]);
    xwall(b, (18.0, 54.0), -40.0, GROUND, H, vec![(29.0, door(CheeseMarket, AlmshouseCourt, DOOR_PRICE_CHEESE_MARKET_ALMSHOUSE_COURT)), (45.0, Gap::Window), (50.0, Gap::Window)]);
    xwall(b, (-24.0, -16.0), -36.0, GROUND, H, vec![(-22.0, Gap::Window), (-18.0, Gap::Window)]);
    zwall(b, -24.0, (-40.0, -36.0), GROUND, H, vec![]);
    zwall(b, -16.0, (-40.0, -36.0), GROUND, H, vec![]);
    // West and east walls of the south bank, and the Court's ends.
    zwall(b, -56.0, (-56.0, -40.0), GROUND, H, vec![(-52.0, Gap::Window), (-46.0, Gap::Window)]);
    zwall(b, -56.0, (-40.0, -12.0), GROUND, H, vec![(-26.0, door(WeighHouse, Brewery, DOOR_PRICE_WEIGH_HOUSE_BREWERY))]);
    zwall(b, -26.0, (-56.0, -40.0), GROUND, H, vec![(-52.0, Gap::Window), (-46.0, Gap::Window)]);
    zwall(b, -26.0, (-40.0, -12.0), GROUND, H, vec![(-26.0, door(TownSquare, WeighHouse, DOOR_PRICE_TOWN_SQUARE_WEIGH_HOUSE))]);
    zwall(b, 18.0, (-40.0, -12.0), GROUND, H, vec![(-26.0, door(TownSquare, CheeseMarket, DOOR_PRICE_TOWN_SQUARE_CHEESE_MARKET))]);
    zwall(b, 40.0, (-56.0, -40.0), GROUND, H, vec![]);
    zwall(b, 54.0, (-40.0, -12.0), GROUND, H, vec![]);

    // The quay rails along the Town Canal's south bank: the Brewery's (round the Humpback Bridge),
    // the Weigh House's (with Ford F1's stair), the Square's (round the Market Bridge's pen), the
    // Cheese Market's (with F2's stair, round the Cheese Bridge's pen).
    xwall(b, (-80.0, -70.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (-66.0, -56.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (-56.0, -44.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (-42.0, -26.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (-26.0, -24.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (-20.0, 18.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (18.0, 25.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (27.0, 42.0), -12.0, GROUND, RAIL, vec![]);
    xwall(b, (46.0, 54.0), -12.0, GROUND, RAIL, vec![]);
    // The Market Bridge's pen (D4) and the Cheese Bridge's (D6).
    xwall(b, (-24.0, -20.0), -16.0, GROUND, FENCE, vec![(-22.0, gate(TownSquare, LockYard, DOOR_PRICE_TOWN_SQUARE_LOCK_YARD))]);
    zwall(b, -24.0, (-16.0, -12.0), GROUND, FENCE, vec![]);
    zwall(b, -20.0, (-16.0, -12.0), GROUND, FENCE, vec![]);
    xwall(b, (42.0, 46.0), -16.0, GROUND, FENCE, vec![(44.0, gate(CheeseMarket, Shipyard, DOOR_PRICE_CHEESE_MARKET_SHIPYARD))]);
    zwall(b, 42.0, (-16.0, -12.0), GROUND, FENCE, vec![]);
    zwall(b, 46.0, (-16.0, -12.0), GROUND, FENCE, vec![]);

    // The Town Canal's north bank: the Windmill's rail (round the Humpback's landing), the
    // Warehouse's wall with its water door D18, the Lock Yard's rail (the Market Bridge lands
    // here), the island's, the Toll Quay's (with F2's iron fence and its water gate D20), the
    // Shipyard's (the Cheese Bridge lands here) and the Coal Yard's.
    xwall(b, (-80.0, -70.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (-66.0, -56.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (-56.0, -34.0), 0.0, GROUND, H, vec![(-43.0, door(WeighHouse, Warehouse, DOOR_PRICE_WEIGH_HOUSE_WAREHOUSE))]);
    xwall(b, (-34.0, -24.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (-20.0, -13.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (-6.0, 6.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (13.0, 20.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (20.0, 32.0), 0.0, GROUND, FENCE, vec![(26.0, door(CheeseMarket, TollQuay, DOOR_PRICE_CHEESE_MARKET_TOLL_QUAY))]);
    xwall(b, (32.0, 34.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (34.0, 42.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (46.0, 58.0), 0.0, GROUND, RAIL, vec![]);
    xwall(b, (58.0, 80.0), 0.0, GROUND, RAIL, vec![]);
    // The Humpback Bridge's end: the Windmill's gate across the deck (D8), and the deck's rails.
    xwall(b, (-70.0, -66.0), -0.2, GROUND + 1.5, H, vec![(-68.0, door(Brewery, Windmill, DOOR_PRICE_BREWERY_WINDMILL))]);
    zwall(b, -70.1, (-12.0, -0.2), GROUND + 1.5, RAIL, vec![]);
    zwall(b, -65.9, (-12.0, -0.2), GROUND + 1.5, RAIL, vec![]);

    // The walls between the Zones on the town's middle band.
    zwall(b, -56.0, (0.0, 24.0), GROUND, H, vec![(12.0, door(Windmill, Warehouse, DOOR_PRICE_WINDMILL_WAREHOUSE))]);
    zwall(b, -56.0, (24.0, 36.0), -4.5, 8.5, vec![]);
    zwall(b, -56.0, (36.0, 48.0), GROUND, H, vec![]);
    zwall(b, -34.0, (0.0, 24.0), GROUND, H, vec![(12.0, door(Warehouse, LockYard, DOOR_PRICE_WAREHOUSE_LOCK_YARD))]);
    zwall(b, 34.0, (0.0, 24.0), GROUND, H, vec![(12.0, door(TollQuay, Shipyard, DOOR_PRICE_TOLL_QUAY_SHIPYARD))]);
    zwall(b, 58.0, (0.0, 24.0), GROUND, H, vec![(12.0, door(Shipyard, CoalYard, DOOR_PRICE_SHIPYARD_COAL_YARD))]);
    zwall(b, 58.0, (24.0, 36.0), -4.5, 8.5, vec![]);
    zwall(b, 58.0, (36.0, 48.0), GROUND, H, vec![(42.0, door(CustomsHouse, CoalYard, DOOR_PRICE_CUSTOMS_HOUSE_COAL_YARD))]);
    zwall(b, 14.0, (36.0, 48.0), GROUND, H, vec![(42.0, door(HerringQuay, CustomsHouse, DOOR_PRICE_HERRING_QUAY_CUSTOMS_HOUSE))]);

    // The Harbour's south bank: the Warehouse's wall with its water door D19 (Ford F3's far
    // bank), the Lock Yard's rail round the Herring Bridge's pen (D5), the island's rail, the Toll
    // Quay's round the Customs Bridge's pen (D7), and the Shipyard's iron fence with F4's gate (D21).
    xwall(b, (-56.0, -34.0), 24.0, GROUND, H, vec![(-48.0, door(HerringQuay, Warehouse, DOOR_PRICE_HERRING_QUAY_WAREHOUSE))]);
    xwall(b, (-34.0, -26.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (-22.0, -13.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (-6.0, 6.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (13.0, 22.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (26.0, 34.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (34.0, 40.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (40.0, 54.0), 24.0, GROUND, FENCE, vec![(47.0, door(CustomsHouse, Shipyard, DOOR_PRICE_CUSTOMS_HOUSE_SHIPYARD))]);
    xwall(b, (54.0, 58.0), 24.0, GROUND, RAIL, vec![]);
    xwall(b, (-26.0, -22.0), 20.0, GROUND, FENCE, vec![(-24.0, gate(LockYard, HerringQuay, DOOR_PRICE_LOCK_YARD_HERRING_QUAY))]);
    zwall(b, -26.0, (20.0, 24.0), GROUND, FENCE, vec![]);
    zwall(b, -22.0, (20.0, 24.0), GROUND, FENCE, vec![]);
    xwall(b, (22.0, 26.0), 20.0, GROUND, FENCE, vec![(24.0, gate(TollQuay, CustomsHouse, DOOR_PRICE_TOLL_QUAY_CUSTOMS_HOUSE))]);
    zwall(b, 22.0, (20.0, 24.0), GROUND, FENCE, vec![]);
    zwall(b, 26.0, (20.0, 24.0), GROUND, FENCE, vec![]);
    // The north bank: the Herring Quay's rail (Ford F3's stair, the Herring Bridge lands), the
    // Customs House's (the Customs Bridge lands, F4's stair).
    xwall(b, (-56.0, -50.0), 36.0, GROUND, RAIL, vec![]);
    xwall(b, (-46.0, -26.0), 36.0, GROUND, RAIL, vec![]);
    xwall(b, (-22.0, 14.0), 36.0, GROUND, RAIL, vec![]);
    xwall(b, (14.0, 22.0), 36.0, GROUND, RAIL, vec![]);
    xwall(b, (26.0, 46.0), 36.0, GROUND, RAIL, vec![]);
    xwall(b, (48.0, 58.0), 36.0, GROUND, RAIL, vec![]);

    // The lock: the Lock Yard's rail along the West Chamber with PD1's wicket at the Inland Gate,
    // the Toll Quay's along the East Chamber with PD2's at the Sea Gate, and the island's.
    zwall(b, -13.0, (0.0, 24.0), GROUND, RAIL, vec![(2.5, pgate(LockYard, SluiceHouse))]);
    zwall(b, 13.0, (0.0, 24.0), GROUND, RAIL, vec![(21.5, pgate(TollQuay, SluiceHouse))]);
    // The island's rail, open where each gate's top lands (1.4 m: the walkway and its rails).
    zwall(b, -6.0, (0.0, 1.8), GROUND, RAIL, vec![]);
    zwall(b, -6.0, (3.2, 24.0), GROUND, RAIL, vec![]);
    zwall(b, 6.0, (0.0, 20.8), GROUND, RAIL, vec![]);
    zwall(b, 6.0, (22.2, 24.0), GROUND, RAIL, vec![]);
    // The sluice house on the island: open at both ends.
    xwall(b, (-5.0, 5.0), 5.0, GROUND, H, vec![(0.0, Gap::Open { width: 4.0, height: 3.0 })]);
    xwall(b, (-5.0, 5.0), 17.0, GROUND, H, vec![(0.0, Gap::Open { width: 4.0, height: 3.0 })]);
    zwall(b, -5.0, (5.0, 17.0), GROUND, H, vec![]);
    zwall(b, 5.0, (5.0, 17.0), GROUND, H, vec![]);
    // The culvert grate's strip at the island's north tip.
    xwall(b, (-2.0, 2.0), 20.0, GROUND, H, vec![(0.0, Gap::Window)]);
    zwall(b, -2.0, (20.0, 24.0), GROUND, H, vec![]);
    zwall(b, 2.0, (20.0, 24.0), GROUND, H, vec![]);

    // The four Swing Bridges' Leaves and the two Lock Gates' are Pieces (`SHIFTING`), spawned with
    // the Map.

    // The Dyke: the face up from the town, with the foot of the three ways up in it, and the
    // wall between the Dyke Road and the Pumping Station.
    xwall(
        b,
        (-80.0, 80.0),
        48.0,
        GROUND,
        H,
        vec![
            (-70.0, door(Windmill, DykeRoad, DOOR_PRICE_WINDMILL_DYKE_ROAD)),
            (-20.0, door(HerringQuay, DykeRoad, DOOR_PRICE_HERRING_QUAY_DYKE_ROAD)),
            (68.0, pdoor(CoalYard, PumpingStation)),
        ],
    );
    zwall(b, 44.0, (48.0, 60.0), CREST, H, vec![(53.0, door(DykeRoad, PumpingStation, DOOR_PRICE_DYKE_ROAD_PUMPING_STATION))]);
    // The crest's inner parapet, with a gap over each drop (beside D11, D12 and PD3).
    xwall(b, (-80.0, -72.0), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (-68.0, -64.0), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (-60.0, -22.8), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (-17.2, -14.0), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (-10.0, 44.0), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (44.0, 58.0), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (62.0, 65.2), 48.0, CREST, PARAPET, vec![]);
    xwall(b, (70.8, 80.0), 48.0, CREST, PARAPET, vec![]);
    // The seaward parapet is low between the windows and full height round them; the boiler
    // house's wall is full height.
    xwall(b, (-80.0, -34.0), 60.0, CREST, PARAPET, vec![]);
    xwall(b, (-34.0, -26.0), 60.0, CREST, H, vec![(-30.0, Gap::Window)]);
    xwall(b, (-26.0, 6.0), 60.0, CREST, PARAPET, vec![]);
    xwall(b, (6.0, 14.0), 60.0, CREST, H, vec![(10.0, Gap::Window)]);
    xwall(b, (14.0, 44.0), 60.0, CREST, PARAPET, vec![]);
    xwall(b, (44.0, 80.0), 60.0, CREST, H, vec![(55.0, Gap::Window), (70.0, Gap::Window)]);
    // A balustrade along each side of every stairwell, so nothing walks over a well's rim.
    for well in &WELLS {
        let (floor, length) = (well.zone.floor(), well.max.y - well.min.y);
        let paint = palette(Some(well.zone)).trim;
        for x in [well.min.x + 0.1, well.max.x - 0.1] {
            b.block(Vec3::new(0.2, RAIL, length), Vec3::new(x, floor + RAIL / 2.0, (well.min.y + well.max.y) / 2.0), paint);
        }
    }

    // Partitions that fence a strip off a Zone's floor for its windows' pockets: the Windmill's
    // at its foot and on its stage, the Warehouse's, the Lock Yard's, the Toll Quay's, the
    // Shipyard's, the Coal Yard's, the Herring Quay's and the Customs House's.
    zwall(b, -76.0, (4.0, 8.0), GROUND, H, vec![(6.0, Gap::Window)]);
    xwall(b, (-70.0, -66.0), 36.0, CREST, H, vec![(-68.0, Gap::Window)]);
    zwall(b, -38.0, (2.0, 10.0), GROUND, H, vec![(4.0, Gap::Window), (8.0, Gap::Window)]);
    xwall(b, (-34.0, -26.0), 20.0, GROUND, H, vec![(-32.0, Gap::Window), (-28.0, Gap::Window)]);
    zwall(b, 30.0, (17.0, 21.0), GROUND, H, vec![(19.0, Gap::Window)]);
    zwall(b, 54.0, (2.0, 10.0), GROUND, H, vec![(4.0, Gap::Window), (8.0, Gap::Window)]);
    zwall(b, 76.0, (4.0, 12.0), GROUND, H, vec![(6.0, Gap::Window), (10.0, Gap::Window)]);
    xwall(b, (-44.0, -36.0), 44.0, GROUND, H, vec![(-42.0, Gap::Window), (-38.0, Gap::Window)]);
    xwall(b, (28.0, 36.0), 44.0, GROUND, H, vec![(30.0, Gap::Window), (34.0, Gap::Window)]);
    // The Windmill's stage: its stone base is the Raised Floor; rails round its edges.
    zwall(b, -73.0, (25.0, 36.0), CREST, RAIL, vec![]);
    zwall(b, -63.0, (25.0, 36.0), CREST, RAIL, vec![]);
    xwall(b, (-73.0, -69.5), 25.0, CREST, RAIL, vec![]);
    xwall(b, (-66.5, -63.0), 25.0, CREST, RAIL, vec![]);
    // The Pumping Station's cupola, with the tide bell in it.
    b.block(Vec3::new(3.0, 2.0, 3.0), Vec3::new(62.0, 2.0 * STOREY + 1.0 + 0.3, 54.0), STONE);
    b.block(Vec3::new(1.0, 1.0, 1.0), Vec3::new(62.0, 2.0 * STOREY + 2.8, 54.0), IRON);

    // A pocket of floor outside every window, walled in, where Zombies gather.
    for &(_, centre, out) in &WINDOWS {
        pocket(b, centre, out);
    }
    let _ = (SETT, WHITE, BERM);
}

pub(super) const OPEN: crate::enemies::nav::Gate = crate::enemies::nav::Gate::Open;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::enemies::nav::Gate;
    use crate::maps::map::MapId;
    use ZoneId::*;

    fn polder() -> &'static Layout {
        MapId::Polder.layout()
    }

    #[test]
    fn sixteen_zones_starting_in_the_town_square_centred_on_the_origin() {
        assert_eq!(polder().zones.len(), 16);
        assert_eq!(polder().start_zone(), TownSquare);
        assert_eq!(polder().plan, (Vec2::new(-80.0, -56.0), Vec2::new(80.0, 66.0)));
        assert_eq!((DykeRoad.floor(), PumpingStation.floor(), TownSquare.floor()), (CREST, CREST, GROUND));
    }

    /// 23 bought Doors and 3 Power Doors, and the cheapest way to the Power costs 5500.
    #[test]
    fn twenty_three_bought_doors_three_power_doors_and_5500_to_the_power() {
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::maps::door::Door;

        let mut sim = Sim::on(MapId::Polder, 60.0, |_| {});
        let doors: Vec<(Option<(ZoneId, ZoneId)>, Option<u64>)> = sim
            .world_mut()
            .run_system_once(|doors: Query<&Door>| doors.iter().map(|door| (door.route(), door.price)).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(doors.len(), 26);
        assert_eq!(doors.iter().filter(|(_, price)| price.is_none()).count(), 3);
        let mut cost: HashMap<ZoneId, u64> = HashMap::from([(TownSquare, 0)]);
        for _ in 0..16 {
            for &(joins, price) in &doors {
                let (Some((a, b)), Some(price)) = (joins, price) else { continue };
                for (from, to) in [(a, b), (b, a)] {
                    let Some(&here) = cost.get(&from) else { continue };
                    let best = cost.entry(to).or_insert(u64::MAX);
                    *best = (*best).min(here + price);
                }
            }
        }
        assert_eq!(cost[&PumpingStation], 5500);
    }

    #[test]
    fn thirty_barriers_no_risers_or_vents_and_a_hellhound_spawn_in_every_zone() {
        assert_eq!(MapId::Polder.barriers().count(), 30);
        assert!(polder().risers.is_empty() && polder().vents.is_empty());
        assert!(polder().sections.is_empty() && polder().generators.is_empty() && !polder().darkness);
        assert_eq!(polder().hellhound_spawns.len(), 19);
        for &zone in polder().zones {
            let want = if matches!(zone, TownSquare | HerringQuay | Shipyard) { 2 } else { 1 };
            assert_eq!(polder().hellhound_spawns.iter().filter(|(z, _)| *z == zone).count(), want, "{}", zone.name());
        }
        let specials: Vec<ZoneId> = polder().special_spawns.iter().map(|&(zone, _)| zone).collect();
        assert_eq!(specials, [TownSquare, HerringQuay, Shipyard, DykeRoad]);
        let boxes: Vec<ZoneId> = MapId::Polder.box_locations().map(|i| crate::maps::map::BOX_LOCATIONS[i].0).collect();
        assert_eq!(boxes, [Brewery, AlmshouseCourt, Warehouse, Shipyard, CustomsHouse, CoalYard]);
    }

    /// The three drops are one-way edges gated on the Door beside each, and none is a fixed join.
    #[test]
    fn three_drops_each_gated_on_the_door_beside_it() {
        assert!(polder().fixed_joins.is_empty());
        let drops: Vec<(ZoneId, ZoneId)> = data::EDGES
            .iter()
            .filter_map(|&(.., gate)| match gate {
                Gate::DoorDrop(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        assert_eq!(drops, [(Windmill, DykeRoad), (HerringQuay, DykeRoad), (CoalYard, PumpingStation)]);
    }

    /// Each Ford's channel is Shallow at Low and Deep above, its berms dry at Low, Shallow at Mid
    /// and Deep at High, and every Deep reach is Deep at every level.
    #[test]
    fn the_fords_can_be_waded_at_their_levels_and_the_reaches_never() {
        use crate::maps::map::{TideLevel, Wading, water_at};
        let at = |level, x: f32, y: f32, z: f32| water_at(polder(), level, Vec3::new(x, y, z));
        for (x, z) in [(-47.0, -6.0), (22.0, -6.0), (-52.0, 30.0), (43.0, 30.0)] {
            assert_eq!(at(TideLevel::Low, x, -2.2, z), Some(Wading::Shallow), "the channel at {x}, {z}");
            assert_eq!(at(TideLevel::Mid, x, -2.2, z), Some(Wading::Deep));
            assert_eq!(at(TideLevel::High, x, -2.2, z), Some(Wading::Deep));
        }
        for (x, z) in [(-47.0, -10.25), (22.5, -10.25), (-51.5, 34.25), (51.0, 34.25)] {
            assert_eq!(at(TideLevel::Low, x, -1.6, z), None, "the berm at {x}, {z} is dry at Low");
            assert_eq!(at(TideLevel::Mid, x, -1.6, z), Some(Wading::Shallow));
            assert_eq!(at(TideLevel::High, x, -1.6, z), Some(Wading::Deep));
        }
        for level in TideLevel::ALL {
            for (x, z) in [(-60.0, -6.0), (0.0, -6.0), (10.0, 30.0), (-9.0, 12.0), (0.0, 63.0)] {
                assert_eq!(at(level, x, -4.5, z), Some(Wading::Deep), "the reach at {x}, {z} at {level:?}");
            }
        }
        assert!(polder().water.iter().all(|pool| matches!(pool.level, crate::maps::map::Level::Canal(_))));
        assert_eq!(polder().canals.len(), 3);
    }

    /// Every dry point of an indoor Zone's floor, clear of walls, is within reach of a lamp.
    #[test]
    fn every_dry_point_of_an_indoor_zone_is_within_reach_of_a_lamp() {
        for &zone in polder().zones.iter().filter(|&&zone| polder().indoor(zone)) {
            let (lo, hi) = zone.volume();
            let lamps: Vec<Vec2> = polder().lamps.iter().filter(|(lamp, _)| *lamp == zone).map(|(_, at)| Vec2::new(at.x, at.z)).collect();
            assert!(!lamps.is_empty(), "{zone:?} has no lamps");
            let mut x = lo.x + 1.0;
            while x < hi.x - 1.0 {
                let mut z = lo.z + 1.0;
                while z < hi.z - 1.0 {
                    let dry = !polder().sunken.iter().any(|s| s.depth >= 1.5 && x >= s.min.x && x <= s.max.x && z >= s.min.y && z <= s.max.y);
                    if dry {
                        let nearest = lamps.iter().map(|lamp| lamp.distance(Vec2::new(x, z))).fold(f32::MAX, f32::min);
                        assert!(nearest <= LAMP_REACH, "({x}, {z}) in {zone:?} is {nearest} m from a lamp");
                    }
                    z += 0.5;
                }
                x += 0.5;
            }
        }
    }

    /// Whether two footprints overlap across the floor (touching along a wall doesn't).
    fn overlap((a_min, a_max): (Vec2, Vec2), (b_min, b_max): (Vec2, Vec2)) -> bool {
        a_min.cmplt(b_max).all() && b_min.cmplt(a_max).all()
    }

    /// An Infill stays clear of the water at every Tide level (every Pool and Sunken floor here is
    /// at its fullest extent, which the Tide only ever fills to), and clear of every Swing Bridge
    /// and Lock Gate Leaf wherever it rests and of the landings at its ends.
    #[test]
    fn every_infill_keeps_clear_of_the_water_at_every_tide_and_of_the_bridges() {
        let mut bad = Vec::new();
        for infill in polder().infills {
            let at = (infill.min, infill.max);
            let what = format!("the Infill at {}..{} in {}", infill.min, infill.max, infill.zone.name());
            for pool in polder().water {
                if overlap(at, (pool.min, pool.max)) {
                    bad.push(format!("{what} stands in the water at {}..{}", pool.min, pool.max));
                }
            }
            for sunken in polder().sunken {
                if overlap(at, (sunken.min, sunken.max)) {
                    bad.push(format!("{what} stands in the sunken floor at {}..{}", sunken.min, sunken.max));
                }
            }
            for piece in SHIFTING.iter() {
                for (i, leaf) in piece.leaves.iter().enumerate() {
                    for rest in piece.rests {
                        let yaw = rest.at[i];
                        let centre = leaf.centre(yaw);
                        let (c, n) = (yaw.cos().abs(), yaw.sin().abs());
                        let half = Vec2::new(c * leaf.size.x + n * leaf.size.z, n * leaf.size.x + c * leaf.size.z) / 2.0;
                        // A metre of landing all round the Leaf.
                        let reach = half + Vec2::ONE;
                        let middle = Vec2::new(centre.x, centre.z);
                        if overlap(at, (middle - reach, middle + reach)) {
                            bad.push(format!("{what} is in the way of a Leaf at {centre}"));
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }
}
