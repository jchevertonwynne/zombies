//! The Sanatorium Map: a hospital on a fogbound hill, in 17 Zones. Its wards and corridors ring a
//! garden court, with three Wings (the West, East and Isolation Wings) off the ring, each of two
//! Zones that meet at a fixed archway and are entered through two doorways (Q1 to Q6). This is the
//! Quarantine: the six doorways are bought gate Doors, and behind each stands a Quarantine Door, a
//! Shifting Piece the Quarantine turns between Open (folded against the doorway's outside wall) and
//! Sealed (shut across it).
//!
//! Three levels, built at the world origin (ADR 0009), plan (-72, -30) to (72, 66):
//!
//! - Ground (y 0): the Entrance Hall (start), the Forecourt, the West and East Corridors, the Garden
//!   Court, the Refectory, the Boiler House (the Power) and the Mortuary, and the West and East
//!   Wings' Long Ward, Day Room, Bath Hall and Treatment Room. In front of the corridors stands the
//!   front range of shut rooms: not Zones, but with floors, because the corridors' and the Hall's
//!   hatches open onto them.
//! - First floor (y 4, the north range only): the Upper Landing, the Operating Theatre
//!   (Pack-a-Punch) and the Isolation Wing's Isolation Ward and Children's Ward.
//! - The Sun Roof (y 4): the Long Ward's flat roof, out of doors.
//!
//! Two flights climb in bays that stand in no ground Zone's box: the Main Stair in a tower behind the
//! Refectory's north wall (x -4..4, z 56..64) and the Fire Escape in a bay between the Long Ward and
//! the Garden Court (x -29..-26). Each is cut in the floor of the Zone at its top, which is stacked
//! over a ground Zone, so that Zone's volume stops at its floor (`ZoneId::volume`) and the path
//! nodes in the bay end in `_out`, as every node outside a Zone does. Each Wing's two Zones meet at a
//! fixed archway. The one drop is the Sun Roof's court-side parapet gap down into the Garden Court,
//! gated on D13, never a fixed join.
//!
//! The lamps, the outside scenery and the per-Zone tables are in separate items on purpose: the
//! Look, the Material Sets, the Infills and the Props each come in a later ticket. Its path nodes,
//! lamps, windows and stairwells (in `data`) are laid out from this file's walls by
//! `tools/sanatorium_layout.py`.

use bevy::prelude::*;

use crate::enemies::sapper::SapperTiming;
use crate::maps::zone::ZoneId;
use crate::tuning::QUARANTINE_LEAF_SIZE;
use crate::weapons::arsenal::WeaponId;

use crate::maps::quarantine::WingId;
use crate::maps::shifting::{Driver, Leaf, Piece, Rest};

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, Infill, Layout, Prop, PropKind, RaisedFloor, STOREY, Slope, Wing, palette, prop};

#[path = "sanatorium/data.rs"]
mod data;
pub(crate) mod scenery;

pub(in crate::maps::map) use data::WINDOWS;

/// The levels: the ground, and the first floor and the Sun Roof a storey up.
const GROUND: f32 = 0.0;
const FIRST: f32 = STOREY;
/// How tall a storey of wall is, and a two-storey room's (the Hall and the Boiler House).
const H: f32 = STOREY;
const TALL: f32 = 2.0 * STOREY;
const RAIL: f32 = 1.1;
/// How tall the Sun Roof's parapets are, but at its one drop.
const PARAPET: f32 = 1.6;
/// The forecourt's railings: too tall to clamber.
const RAILING: f32 = 2.4;
/// How high a chalk outline's centre sits over its floor.
const CHALK: f32 = 1.5;
/// The observation gallery's height over the Operating Theatre's floor, and its windows' floor.
const GALLERY: f32 = FIRST + 1.2;

/// The first of the Sanatorium's windows in `WINDOWS`, after the other Maps'.
const W0: u8 = (super::greybox::WINDOWS.len()
    + super::quarry::WINDOWS.len()
    + super::village::WINDOWS.len()
    + super::dam::WINDOWS.len()
    + super::cove::WINDOWS.len()
    + super::polder::WINDOWS.len()) as u8;

/// The Doors' prices, D1 to D13 and Q1 to Q6 (D12 is the Power Door).
const D1: u64 = 750;
const D2: u64 = 750;
const D3: u64 = 750;
const D4: u64 = 1250;
const D5: u64 = 1250;
const D6: u64 = 1000;
const D7: u64 = 1000;
const D8: u64 = 750;
const D9: u64 = 750;
const D10: u64 = 1000;
const D11: u64 = 1250;
const D13: u64 = 1250;
const Q1: u64 = 1000;
const Q2: u64 = 1000;
const Q3: u64 = 1000;
const Q4: u64 = 1000;
const Q5: u64 = 1250;
const Q6: u64 = 1250;

/// A point in the Sanatorium.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A spot on a wall or a floor's plan in the Sanatorium.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const fn infill(zone: ZoneId, min: (f32, f32), max: (f32, f32)) -> Infill {
    Infill { zone, min: Vec2::new(min.0, min.1), max: Vec2::new(max.0, max.1) }
}

const fn p(kind: PropKind, zone: ZoneId, x: f32, y: f32, z: f32, size: (f32, f32, f32)) -> Prop {
    prop(kind, zone, (x, y, z), size)
}

pub(super) const SANATORIUM: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [
            EntranceHall,
            Forecourt,
            WestCorridor,
            EastCorridor,
            GardenCourt,
            Refectory,
            BoilerHouse,
            Mortuary,
            LongWard,
            DayRoom,
            BathHall,
            TreatmentRoom,
            UpperLanding,
            OperatingTheatre,
            SunRoof,
            IsolationWard,
            ChildrensWard,
        ]
    },
    // In a row across the Hall, between its front doors and Quick Revive.
    seats: [at(-4.5, GROUND, 8.0), at(-1.5, GROUND, 8.0), at(1.5, GROUND, 8.0), at(4.5, GROUND, 8.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &data::WINDOWS,
    risers: &RISERS,
    wading: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &RAISED_FLOORS,
    infills: &INFILLS,
    slopes: &SLOPES,
    sunken: &data::SUNKEN,
    water: &[],
    canals: &[],
    props: &PROPS,
    lamps: &data::LAMPS,
    fixed_joins: &FIXED_JOINS,
    shifting: &SHIFTING,
    wings: &WINGS,
    nav_nodes: data::NODES,
    nav_edges: data::EDGES,
    jump_pads: &[],
    ziplines: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    outdoor: &[ZoneId::Forecourt, ZoneId::GardenCourt, ZoneId::SunRoof],
    roofed: true,
    sun: Vec3::new(0.3, -1.0, 0.5),
    look: crate::maps::look::FOGBOUND,
    // Fluorescent tubes, but for the Boiler House's caged incandescents (and the outdoor posts).
    lamp_style: super::LampStyle::Tube,
    bulb_zones: &[ZoneId::BoilerHouse],
    outside: Some(super::Outside { floor: super::textures::Material::GrassGround, wall: super::textures::Material::WornMossyPlasterwall }),
    plan: (Vec2::new(-72.0, -30.0), Vec2::new(72.0, 66.0)),
    build,
};

/// Each Wing's two Zones meet at an archway that never closes.
const FIXED_JOINS: [(ZoneId, ZoneId); 3] = [
    (ZoneId::LongWard, ZoneId::DayRoom),
    (ZoneId::BathHall, ZoneId::TreatmentRoom),
    (ZoneId::IsolationWard, ZoneId::ChildrensWard),
];

/// The three Wings, in the order the Quarantine seals them. Each is two Zones that meet at an archway
/// and is entered through two doorways, on two different Zones outside every Wing.
const WINGS: [Wing; 3] = [
    Wing { name: "West Wing", zones: &[ZoneId::LongWard, ZoneId::DayRoom] },
    Wing { name: "East Wing", zones: &[ZoneId::BathHall, ZoneId::TreatmentRoom] },
    Wing { name: "Isolation Wing", zones: &[ZoneId::IsolationWard, ZoneId::ChildrensWard] },
];

const PI: f32 = std::f32::consts::PI;
const FRAC_PI_2: f32 = std::f32::consts::FRAC_PI_2;

/// How far the Leaves' hinges stand out from the doorway's centre line: past the wall's outside face
/// and half a Leaf's thickness, so a shut Leaf lies flush against the face.
const HINGE_OUT: f32 = 0.23;
/// How far a hinge stands from the doorway's centre along the wall: half the doorway.
const HINGE_ALONG: f32 = 1.2;

/// A Quarantine Door's Leaf, hinged at `hinge` and lying along its own x at yaw 0.
const fn q_leaf(hinge: Vec3) -> Leaf {
    let (width, height, thick) = QUARANTINE_LEAF_SIZE;
    Leaf { hinge, size: Vec3::new(width, height, thick), offset: Vec3::new(width / 2.0, height / 2.0, 0.0) }
}

/// Where a Quarantine Door's hinge stands: `side` (-1 or 1) of the doorway's centre `at` along the
/// wall, out past the wall's face by `HINGE_OUT` the way `out` looks.
const fn hinge(at: Vec3, out: Vec2, side: f32) -> Vec3 {
    let (along_x, along_z) = if out.x == 0.0 { (1.0, 0.0) } else { (0.0, 1.0) };
    Vec3::new(at.x + side * along_x * HINGE_ALONG + out.x * HINGE_OUT, at.y, at.z + side * along_z * HINGE_ALONG + out.y * HINGE_OUT)
}

/// A Quarantine Door in the doorway at `at` whose outside looks `out`. Rest 0 is Open (the Leaves
/// folded against the outside wall, joining outside to inside) and Rest 1 is Sealed (shut across the
/// doorway, joining nothing); each gives the yaw of the Leaf at the lower and then the higher hinge
/// along the wall. Both Leaves swing through the outside, a half turn between the Rests.
macro_rules! quarantine_door {
    ($outside:expr, $inside:expr, $at:expr, $out:expr, $wing:expr, open [$o0:expr, $o1:expr], sealed [$s0:expr, $s1:expr]) => {
        Piece {
            zone: $outside,
            leaves: &[q_leaf(hinge($at, $out, -1.0)), q_leaf(hinge($at, $out, 1.0))],
            rests: &[Rest { yaws: &[$o0, $o1], joins: &[($outside, $inside)] }, Rest { yaws: &[$s0, $s1], joins: &[] }],
            start: 0,
            driver: Driver::Quarantine(WingId($wing)),
            door: Some(($outside, $inside)),
        }
    };
}

/// The six Quarantine Doors, in the order Q1 to Q6: two to a Wing, which is the Piece's `PieceId`
/// divided by two.
const SHIFTING: [Piece; 6] = {
    use ZoneId::*;
    [
        quarantine_door!(WestCorridor, LongWard, at(-40.0, GROUND, 14.0), Vec2::NEG_Y, 0, open [PI, 0.0], sealed [0.0, PI]),
        quarantine_door!(Refectory, DayRoom, at(-26.0, GROUND, 49.0), Vec2::X, 0, open [FRAC_PI_2, -FRAC_PI_2], sealed [-FRAC_PI_2, FRAC_PI_2]),
        quarantine_door!(EastCorridor, BathHall, at(40.0, GROUND, 14.0), Vec2::NEG_Y, 1, open [PI, 0.0], sealed [0.0, PI]),
        quarantine_door!(Refectory, TreatmentRoom, at(26.0, GROUND, 49.0), Vec2::NEG_X, 1, open [-3.0 * FRAC_PI_2, 3.0 * FRAC_PI_2], sealed [-FRAC_PI_2, FRAC_PI_2]),
        quarantine_door!(UpperLanding, IsolationWard, at(-10.0, FIRST, 49.0), Vec2::X, 2, open [FRAC_PI_2, -FRAC_PI_2], sealed [-FRAC_PI_2, FRAC_PI_2]),
        quarantine_door!(SunRoof, ChildrensWard, at(-38.0, FIRST, 42.0), Vec2::NEG_Y, 2, open [PI, 0.0], sealed [0.0, PI]),
    ]
};

#[cfg_attr(not(test), allow(dead_code))]
/// The six Quarantine doorways, as plain Doors: where each is, the Zone outside it, the way the
/// outside Zone looks at it (out of the wall) and the Zone inside.
pub(super) const QUARANTINE_DOORWAYS: [(&str, Vec3, Vec2, ZoneId, ZoneId); 6] = [
    ("Q1", at(-40.0, GROUND, 14.0), Vec2::NEG_Y, ZoneId::WestCorridor, ZoneId::LongWard),
    ("Q2", at(-26.0, GROUND, 49.0), Vec2::X, ZoneId::Refectory, ZoneId::DayRoom),
    ("Q3", at(40.0, GROUND, 14.0), Vec2::NEG_Y, ZoneId::EastCorridor, ZoneId::BathHall),
    ("Q4", at(26.0, GROUND, 49.0), Vec2::NEG_X, ZoneId::Refectory, ZoneId::TreatmentRoom),
    ("Q5", at(-10.0, FIRST, 49.0), Vec2::X, ZoneId::UpperLanding, ZoneId::IsolationWard),
    ("Q6", at(-38.0, FIRST, 42.0), Vec2::NEG_Y, ZoneId::SunRoof, ZoneId::ChildrensWard),
];

/// The Infills, one to three in each of the 14 indoor Zones, so no two rooms share a shape: #505 §6's
/// rects, moved along their wall (or trimmed) off every window and its landing, Box Location, chalk,
/// Perk, lamp and Spawn Point they crowded. Each keeps clear of the Doors, the Quarantine doorways with their sweeps and outside landings, the
/// Crash Bars, windows, lamps, Buyables, Spawn Points and every path-node edge. Every machine builds
/// them from this Layout, as World colliders.
const INFILLS: [Infill; 26] = {
    use ZoneId::*;
    [
        // The Entrance Hall: the porter's lodge in the east wall's corner, and a pier between the windows.
        infill(EntranceHall, (6.5, 0.0), (10.0, 1.3)),
        infill(EntranceHall, (-0.6, 13.4), (0.6, 14.0)),
        // The West Corridor: a hatch pier, a duct casing and a hose cupboard along the south wall.
        infill(WestCorridor, (-42.4, 8.0), (-41.6, 8.5)),
        infill(WestCorridor, (-33.3, 8.0), (-32.7, 8.9)),
        infill(WestCorridor, (-24.1, 8.0), (-22.7, 8.6)),
        // The East Corridor: a hatch pier, a linen chute and a radiator nib.
        infill(EastCorridor, (22.7, 8.0), (23.7, 8.5)),
        infill(EastCorridor, (48.8, 13.0), (50.0, 14.0)),
        infill(EastCorridor, (33.7, 8.0), (34.3, 9.2)),
        // The Refectory: a chimney breast and a servery nib.
        infill(Refectory, (-13.3, 55.2), (-10.9, 56.0)),
        infill(Refectory, (14.7, 53.0), (15.3, 56.0)),
        // The Boiler House: the chimney's base and a coal bunker.
        infill(BoilerHouse, (-72.0, 10.0), (-68.0, 14.0)),
        infill(BoilerHouse, (-72.0, -10.0), (-69.5, -4.0)),
        // The Mortuary: the cold-room block and a pier.
        infill(Mortuary, (68.7, 9.3), (72.0, 14.0)),
        infill(Mortuary, (71.4, -0.3), (72.0, 1.7)),
        // The Long Ward: a chimney pier and a slimmer pier.
        infill(LongWard, (-50.0, 26.7), (-49.2, 28.3)),
        infill(LongWard, (-50.0, 20.7), (-49.5, 21.3)),
        // The Day Room: a bay-window nib.
        infill(DayRoom, (-50.0, 48.0), (-48.8, 48.6)),
        // The Bath Hall: the calorifier cupboard and a pier.
        infill(BathHall, (46.0, 14.0), (50.0, 17.0)),
        infill(BathHall, (49.4, 28.4), (50.0, 29.2)),
        // The Treatment Room: a cupboard.
        infill(TreatmentRoom, (47.0, 53.0), (50.0, 56.0)),
        // The Upper Landing: a linen cupboard and the lift shaft.
        infill(UpperLanding, (-10.0, 42.0), (-7.0, 43.5)),
        infill(UpperLanding, (7.5, 42.0), (10.0, 44.6)),
        // The Operating Theatre: the scrub-room block.
        infill(OperatingTheatre, (10.0, 42.0), (14.0, 45.0)),
        // The Isolation Ward: two cell partitions.
        infill(IsolationWard, (-21.9, 42.0), (-21.5, 46.0)),
        infill(IsolationWard, (-15.9, 42.0), (-15.5, 47.0)),
        // The Children's Ward: a chimney breast.
        infill(ChildrensWard, (-42.1, 55.2), (-39.7, 56.0)),
    ]
};

/// The chalk outlines: the cheap guns either side of the Hall's front doors, the MP40 in the West
/// Corridor, the Trench Gun in the East, and the AK-74u in the Refectory.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    (WeaponId::M14, ZoneId::EntranceHall, at(-4.5, GROUND + CHALK, 0.2), Vec2::Y),
    (WeaponId::Olympia, ZoneId::EntranceHall, at(4.5, GROUND + CHALK, 0.2), Vec2::Y),
    (WeaponId::Mp40, ZoneId::WestCorridor, at(-30.0, GROUND + CHALK, 13.8), Vec2::NEG_Y),
    (WeaponId::TrenchGun, ZoneId::EastCorridor, at(30.0, GROUND + CHALK, 13.8), Vec2::NEG_Y),
    (WeaponId::Ak74u, ZoneId::Refectory, at(-6.0, GROUND + CHALK, 55.8), Vec2::NEG_Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first, on the bandstand in the
/// Garden Court, two Doors from the Hall (D2 and D8). None is in the Hall, the Boiler House or the
/// Operating Theatre. Two are in Wings (the Long Ward and the Children's Ward).
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // On the bandstand, at its north edge between the posts.
    (ZoneId::GardenCourt, at(0.0, GROUND + 0.6, 30.6), Vec2::NEG_Y),
    // Against the Refectory's north wall.
    (ZoneId::Refectory, at(-18.0, GROUND, 55.35), Vec2::NEG_Y),
    // Against the Mortuary's east wall.
    (ZoneId::Mortuary, at(71.35, GROUND, -2.0), Vec2::NEG_X),
    // Against the Forecourt's front wall.
    (ZoneId::Forecourt, at(-18.0, GROUND, -0.65), Vec2::NEG_Y),
    // Against the Long Ward's west wall (West Wing).
    (ZoneId::LongWard, at(-49.35, GROUND, 30.0), Vec2::X),
    // Against the Children's Ward's north wall (Isolation Wing).
    (ZoneId::ChildrensWard, at(-38.0, FIRST, 55.35), Vec2::NEG_Y),
];

/// The Hellhound Spawn Points: one in every Zone, and two each in the Forecourt, the Garden Court
/// and the Refectory.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 20] = [
    (ZoneId::EntranceHall, at(0.0, GROUND, 5.0)),
    (ZoneId::Forecourt, at(-20.0, GROUND, -12.0)),
    (ZoneId::Forecourt, at(22.0, GROUND, -20.0)),
    (ZoneId::WestCorridor, at(-28.0, GROUND, 11.0)),
    (ZoneId::EastCorridor, at(28.0, GROUND, 11.0)),
    (ZoneId::GardenCourt, at(-12.0, GROUND, 22.0)),
    (ZoneId::GardenCourt, at(12.0, GROUND, 36.0)),
    (ZoneId::Refectory, at(-12.0, GROUND, 50.0)),
    (ZoneId::Refectory, at(12.0, GROUND, 46.0)),
    (ZoneId::BoilerHouse, at(-60.0, GROUND, 2.0)),
    (ZoneId::Mortuary, at(60.0, GROUND, 2.0)),
    (ZoneId::LongWard, at(-40.0, GROUND, 28.0)),
    (ZoneId::DayRoom, at(-38.0, GROUND, 49.0)),
    (ZoneId::BathHall, at(38.0, GROUND, 28.0)),
    (ZoneId::TreatmentRoom, at(38.0, GROUND, 49.0)),
    (ZoneId::UpperLanding, at(0.0, FIRST, 48.0)),
    (ZoneId::OperatingTheatre, at(30.0, FIRST, 48.0)),
    (ZoneId::SunRoof, at(-40.0, FIRST, 24.0)),
    (ZoneId::IsolationWard, at(-18.0, FIRST, 49.0)),
    (ZoneId::ChildrensWard, at(-38.0, FIRST, 50.0)),
];

/// The Special Spawn Points, where a Napalm or a Shrieker rises, all on the spine: the Forecourt,
/// the Garden Court, the Refectory and the Sun Roof.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 4] = [
    (ZoneId::Forecourt, at(10.0, GROUND, -22.0)),
    (ZoneId::GardenCourt, at(-14.0, GROUND, 34.0)),
    (ZoneId::Refectory, at(14.0, GROUND, 52.0)),
    (ZoneId::SunRoof, at(-38.0, FIRST, 36.0)),
];

/// The Risers: three in the Forecourt, on the lawns by the railings, and two in the Garden Court,
/// in the rose beds.
const RISERS: [(ZoneId, Vec3); 5] = [
    (ZoneId::Forecourt, at(-30.0, GROUND, -26.0)),
    (ZoneId::Forecourt, at(0.0, GROUND, -26.0)),
    (ZoneId::Forecourt, at(30.0, GROUND, -26.0)),
    (ZoneId::GardenCourt, at(-18.0, GROUND, 38.0)),
    (ZoneId::GardenCourt, at(18.0, GROUND, 38.0)),
];

/// The Vents: in the West and East Corridors, the Refectory and the Upper Landing, each over the
/// floor spot named.
const VENTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::WestCorridor, at(-26.0, GROUND, 11.0)),
    (ZoneId::EastCorridor, at(26.0, GROUND, 11.0)),
    (ZoneId::Refectory, at(0.0, GROUND, 49.0)),
    (ZoneId::UpperLanding, at(6.0, FIRST, 50.0)),
];

/// Stone and iron, for the pieces that aren't a Zone's wall.
const STONE: Color = Color::srgb(0.46, 0.46, 0.43);
const IRON: Color = Color::srgb(0.13, 0.14, 0.15);
const TIMBER: Color = Color::srgb(0.28, 0.2, 0.13);

// The Raised Floors.
/// The bandstand: 7 x 7 m and 0.6 m up, with steps on its west and east sides.
const BANDSTAND: RaisedFloor = RaisedFloor { zone: ZoneId::GardenCourt, min: v(-3.5, 24.5), max: v(3.5, 31.5), height: GROUND + 0.6, colour: TIMBER };
/// The Operating Theatre's observation gallery: 1.2 m up along the north wall, up steps at each end.
const OBSERVATION: RaisedFloor = RaisedFloor { zone: ZoneId::OperatingTheatre, min: v(14.0, 52.0), max: v(30.0, 56.0), height: GALLERY, colour: IRON };
const RAISED_FLOORS: [RaisedFloor; 2] = [BANDSTAND, OBSERVATION];

/// The stairs: the Main Stair and the Fire Escape, in their wells (`data::SUNKEN`), and the steps up
/// onto the two Raised Floors.
const SLOPES: [Slope; 6] = [
    // The Main Stair: from the tower's north end, climbing south onto the Upper Landing, beside
    // the corridor in from D11.
    Slope { zone: ZoneId::UpperLanding, foot: at(-2.0, GROUND, 63.0), up: Vec2::NEG_Y, run: 6.8, rise: STOREY, width: 3.4, steps: true, colour: STONE },
    // The Fire Escape: an iron flight up the bay, from just inside D13, north onto the Sun Roof.
    Slope { zone: ZoneId::SunRoof, foot: at(-27.5, GROUND, 18.5), up: Vec2::Y, run: 8.5, rise: STOREY, width: 2.4, steps: true, colour: IRON },
    // Up onto the bandstand, from the west and the east.
    Slope { zone: ZoneId::GardenCourt, foot: at(-5.3, GROUND, 28.0), up: Vec2::X, run: 1.8, rise: 0.6, width: 3.0, steps: true, colour: TIMBER },
    Slope { zone: ZoneId::GardenCourt, foot: at(5.3, GROUND, 28.0), up: Vec2::NEG_X, run: 1.8, rise: 0.6, width: 3.0, steps: true, colour: TIMBER },
    // Up onto the observation gallery, from each end.
    Slope { zone: ZoneId::OperatingTheatre, foot: at(11.6, FIRST, 54.0), up: Vec2::X, run: 2.4, rise: 1.2, width: 3.0, steps: true, colour: IRON },
    Slope { zone: ZoneId::OperatingTheatre, foot: at(32.4, FIRST, 54.0), up: Vec2::NEG_X, run: 2.4, rise: 1.2, width: 3.0, steps: true, colour: IRON },
];

/// The 68 Props (#505 §7), low and against the walls in the corridors and wards, each between the path
/// nodes' rows and clear of every doorway, window landing, sweep, Crash Bar face, Buyable, Spawn Point,
/// Infill and path edge. The Founder's plinth and the bandstand's corner posts (the Training loops'
/// centres) come first. Each Zone's set names what its kinds look like (`zone_props`); a Prop is the
/// size of its unit model, so the model fits it. `tools/sanatorium_layout.py` reads these to keep the
/// path edges off them, so keep the `p(...)` calls one to a line.
const PROPS: [Prop; 68] = {
    use PropKind::*;
    use ZoneId::*;
    const BED: (f32, f32, f32) = (2.0, 1.1, 0.9);
    const BED_ALONG_Z: (f32, f32, f32) = (0.9, 1.1, 2.0);
    const TREE: (f32, f32, f32) = (0.5, 4.0, 0.5);
    const BENCH: (f32, f32, f32) = (1.8, 0.55, 0.55);
    const WHEELCHAIR: (f32, f32, f32) = (1.0, 1.0, 0.75);
    const TROLLEY: (f32, f32, f32) = (0.9, 0.968, 0.53);
    const SIGN: (f32, f32, f32) = (0.36, 0.63, 0.3);
    const TABLE: (f32, f32, f32) = (2.5, 0.8, 1.1);
    const DRUM: (f32, f32, f32) = (0.66, 0.95, 0.66);
    const SLAB: (f32, f32, f32) = (2.1, 0.98, 0.75);
    const TUB: (f32, f32, f32) = (0.82, 0.72, 1.74);
    const COT: (f32, f32, f32) = (1.25, 0.85, 0.65);
    [
        // The Forecourt: the Founder's statue on its 5 x 5 m plinth, a derelict ambulance, three dead trees and a bench.
        p(Pillar, Forecourt, 0.0, GROUND, -15.0, (5.0, 2.2, 5.0)),
        p(Machine, Forecourt, -37.0, GROUND, -22.0, (5.12, 2.26, 2.14)),
        p(Tree, Forecourt, -42.2, GROUND, -7.0, TREE),
        p(Tree, Forecourt, 42.2, GROUND, -15.0, TREE),
        p(Tree, Forecourt, -19.2, GROUND, -23.0, TREE),
        p(Bench, Forecourt, 19.2, GROUND, -7.0, BENCH),
        // The Garden Court: the bandstand's corner posts, two dead trees and a bench.
        p(Pillar, GardenCourt, -3.0, GROUND + 0.6, 25.0, (0.5, 3.0, 0.5)),
        p(Pillar, GardenCourt, 3.0, GROUND + 0.6, 25.0, (0.5, 3.0, 0.5)),
        p(Pillar, GardenCourt, -3.0, GROUND + 0.6, 31.0, (0.5, 3.0, 0.5)),
        p(Pillar, GardenCourt, 3.0, GROUND + 0.6, 31.0, (0.5, 3.0, 0.5)),
        p(Tree, GardenCourt, -20.1, GROUND, 25.1, TREE),
        p(Tree, GardenCourt, 20.1, GROUND, 31.0, TREE),
        p(Bench, GardenCourt, -13.4, GROUND, 19.4, BENCH),
        // The Entrance Hall: the reception counter, two columns and a wheelchair.
        p(Counter, EntranceHall, -5.2, GROUND, 12.9, (2.8, 1.05, 0.85)),
        p(Pillar, EntranceHall, -6.0, GROUND, 5.0, (0.6, 7.9, 0.6)),
        p(Pillar, EntranceHall, 6.0, GROUND, 5.0, (0.6, 7.9, 0.6)),
        p(Trolley, EntranceHall, -9.4, GROUND, 7.5, (0.75, 1.0, 1.0)),
        // The West Corridor: a wheelchair, a dressing trolley and a wet-floor sign.
        p(Trolley, WestCorridor, -46.0, GROUND, 8.4, WHEELCHAIR),
        p(Trolley, WestCorridor, -34.0, GROUND, 13.6, TROLLEY),
        p(Sign, WestCorridor, -15.0, GROUND, 8.4, SIGN),
        // The East Corridor, the same mirrored.
        p(Trolley, EastCorridor, 46.0, GROUND, 8.4, WHEELCHAIR),
        p(Trolley, EastCorridor, 34.0, GROUND, 13.6, TROLLEY),
        p(Sign, EastCorridor, 15.0, GROUND, 8.4, SIGN),
        // The Refectory: four long tables and the servery counter.
        p(Table, Refectory, -20.1, GROUND, 46.75, TABLE),
        p(Table, Refectory, -6.7, GROUND, 46.75, TABLE),
        p(Table, Refectory, 6.7, GROUND, 46.75, TABLE),
        p(Table, Refectory, 20.1, GROUND, 46.75, TABLE),
        p(Counter, Refectory, -21.5, GROUND, 55.55, (3.0, 1.05, 0.7)),
        // The Boiler House: two boilers, the coal heap, two drums and a tool cart.
        p(Machine, BoilerHouse, -61.0, GROUND, -8.0, (3.2, 2.6, 1.8)),
        p(Machine, BoilerHouse, -61.0, GROUND, 8.0, (3.2, 2.6, 1.8)),
        p(Rubble, BoilerHouse, -66.8, GROUND, -9.5, (1.7, 1.1, 1.6)),
        p(Barrel, BoilerHouse, -50.9, GROUND, -2.5, DRUM),
        p(Barrel, BoilerHouse, -50.9, GROUND, -1.6, DRUM),
        p(Trolley, BoilerHouse, -66.5, GROUND, 4.5, (1.2, 0.949, 0.749)),
        // The Mortuary: the drawer bank (three drawer cabinets) on the south wall, first so its index is even and
        // `nth` picks the drawer cabinet over the locker bank, and three slabs in a row.
        p(Locker, Mortuary, 60.0, GROUND, -13.7, (3.228, 1.8, 0.5)),
        p(Bed, Mortuary, 55.3, GROUND, -8.6, SLAB),
        p(Bed, Mortuary, 61.0, GROUND, -8.6, SLAB),
        p(Bed, Mortuary, 66.67, GROUND, -8.6, SLAB),
        // The Long Ward: six beds (four down the east wall, two on the west) and the nurses' station.
        p(Bed, LongWard, -29.6, GROUND, 19.0, BED_ALONG_Z),
        p(Bed, LongWard, -29.6, GROUND, 23.0, BED_ALONG_Z),
        p(Bed, LongWard, -29.6, GROUND, 27.0, BED_ALONG_Z),
        p(Bed, LongWard, -29.6, GROUND, 31.0, BED_ALONG_Z),
        p(Bed, LongWard, -49.5, GROUND, 35.5, BED_ALONG_Z),
        p(Bed, LongWard, -49.5, GROUND, 38.9, BED_ALONG_Z),
        p(Counter, LongWard, -39.5, GROUND, 30.9, (3.0, 1.05, 0.8)),
        // The Day Room: a day bed, an armchair and the piano.
        p(Bed, DayRoom, -49.5, GROUND, 51.5, (0.85, 1.1, 2.0)),
        p(Bench, DayRoom, -45.5, GROUND, 55.4, (0.9, 0.85, 0.9)),
        p(Counter, DayRoom, -31.0, GROUND, 55.6, (1.6, 1.2, 0.65)),
        // The Bath Hall: four tubs down the west wall.
        p(Tub, BathHall, 26.5, GROUND, 18.5, TUB),
        p(Tub, BathHall, 26.5, GROUND, 22.5, TUB),
        p(Tub, BathHall, 26.5, GROUND, 26.5, TUB),
        p(Tub, BathHall, 26.5, GROUND, 30.5, TUB),
        // The Treatment Room: the electrotherapy machine and a gurney.
        p(Machine, TreatmentRoom, 47.2, GROUND, 43.4, (1.0, 1.5, 0.8)),
        p(Bed, TreatmentRoom, 32.0, GROUND, 55.5, (2.016, 1.044, 0.58)),
        // The Upper Landing: steel shelves and a wheelchair.
        p(Shelf, UpperLanding, 9.7, FIRST, 60.5, (0.5, 2.1, 1.1)),
        p(Trolley, UpperLanding, 9.4, FIRST, 54.5, (0.75, 1.0, 1.0)),
        // The Operating Theatre: a dressing trolley.
        p(Trolley, OperatingTheatre, 30.5, FIRST, 43.0, TROLLEY),
        // The Isolation Ward: three beds in the cells and a screen.
        p(Bed, IsolationWard, -24.0, FIRST, 43.5, BED),
        p(Bed, IsolationWard, -18.7, FIRST, 43.5, BED),
        p(Bed, IsolationWard, -13.0, FIRST, 43.5, BED),
        p(Shelf, IsolationWard, -12.5, FIRST, 55.5, (1.2, 1.8, 0.1)),
        // The Children's Ward: three cots and the rocking horse.
        p(Bed, ChildrensWard, -46.0, FIRST, 55.5, COT),
        p(Bed, ChildrensWard, -34.4, FIRST, 55.5, COT),
        p(Bed, ChildrensWard, -30.8, FIRST, 55.5, COT),
        p(Bench, ChildrensWard, -44.3, FIRST, 46.75, (1.192, 1.025, 0.38)),
        // The Sun Roof: two beds out in the weather and a deck chair.
        p(Bed, SunRoof, -49.5, FIRST, 21.0, BED_ALONG_Z),
        p(Bed, SunRoof, -49.5, FIRST, 25.5, BED_ALONG_Z),
        p(Bench, SunRoof, -39.25, FIRST, 19.9, (1.4, 0.8, 0.7)),
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

/// The gate Door of a Quarantine Door, bought for the price: it joins nothing itself, but names the
/// two Zones (outside, then inside) whose doorway it guards.
fn gate(outside: ZoneId, inside: ZoneId, price: u64) -> Gap {
    Gap::Gate((outside, inside), price)
}

/// The Power Door.
fn pdoor(a: ZoneId, b: ZoneId) -> Gap {
    Gap::Door(Some((a, b)), None)
}

/// An archway: open to the floor, 4 m wide.
fn arch() -> Gap {
    Gap::Open { width: 4.0, height: DOOR_HEIGHT }
}

/// A pocket of floor outside the window at `centre` looking `out`, walled in, where Zombies gather.
/// A first-floor window's stands on a lean-to roof, a block down to the ground.
fn pocket(b: &mut Builder, centre: Vec3, out: Vec2) {
    const POCKET: f32 = 4.0;
    const WALL: f32 = 0.3;
    const HEIGHT: f32 = 2.6;
    let outside = palette(None);
    let out3 = Vec3::new(out.x, 0.0, out.y);
    let across = Vec3::new(out.y.abs(), 0.0, out.x.abs());
    let depth = out3.abs();
    let floor = centre.y;
    let middle = centre + out3 * POCKET / 2.0;
    let thick = floor.max(0.2);
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

/// The walls, Doors, balustrades and pockets of the Sanatorium, then its outside scenery.
fn build(b: &mut Builder) {
    use ZoneId::*;
    // The front of the building along z 0: the shut front range's south wall either side of the
    // two-storey Hall, whose front doors are D1.
    xwall(b, (-50.0, -10.0), 0.0, GROUND, H, vec![]);
    xwall(b, (-10.0, 10.0), 0.0, GROUND, TALL, vec![(0.0, door(EntranceHall, Forecourt, D1))]);
    xwall(b, (10.0, 50.0), 0.0, GROUND, H, vec![]);
    // The Hall's side walls, with a hatch each into the front range (the porter's lodge and the
    // almoner's) and Doors D2 and D3 into the corridors, and its north wall, glazed and blind.
    zwall(b, -10.0, (0.0, 14.0), GROUND, TALL, vec![(4.0, Gap::Window), (11.0, door(WestCorridor, EntranceHall, D2))]);
    zwall(b, 10.0, (0.0, 14.0), GROUND, TALL, vec![(4.0, Gap::Window), (11.0, door(EntranceHall, EastCorridor, D3))]);
    xwall(b, (-10.0, 10.0), 14.0, GROUND, TALL, vec![]);
    // The corridors' south walls, with their hatches into the front range, and their north walls
    // with Q1 and D8, D9 and Q3 (the Hall's north wall stands between).
    xwall(b, (-50.0, -10.0), 8.0, GROUND, H, vec![(-30.0, Gap::Window), (-20.0, Gap::Window)]);
    xwall(b, (10.0, 50.0), 8.0, GROUND, H, vec![(20.0, Gap::Window), (30.0, Gap::Window)]);
    xwall(b, (-50.0, -10.0), 14.0, GROUND, H, vec![(-40.0, gate(WestCorridor, LongWard, Q1)), (-18.0, door(WestCorridor, GardenCourt, D8))]);
    xwall(b, (10.0, 50.0), 14.0, GROUND, H, vec![(18.0, door(EastCorridor, GardenCourt, D9)), (40.0, gate(EastCorridor, BathHall, Q3))]);

    // The Boiler House and the Mortuary, tall, at the ends of the range, each with a Door from
    // its corridor and one from the Forecourt, and two windows.
    zwall(b, -50.0, (-14.0, 14.0), GROUND, TALL, vec![(-7.0, door(BoilerHouse, Forecourt, D5)), (11.0, door(BoilerHouse, WestCorridor, D4))]);
    zwall(b, 50.0, (-14.0, 14.0), GROUND, TALL, vec![(-7.0, door(Forecourt, Mortuary, D7)), (11.0, door(EastCorridor, Mortuary, D6))]);
    xwall(b, (-72.0, -50.0), -14.0, GROUND, TALL, vec![(-58.0, Gap::Window)]);
    xwall(b, (-72.0, -50.0), 14.0, GROUND, TALL, vec![(-60.0, Gap::Window)]);
    zwall(b, -72.0, (-14.0, 14.0), GROUND, TALL, vec![]);
    xwall(b, (50.0, 72.0), -14.0, GROUND, TALL, vec![]);
    xwall(b, (50.0, 72.0), 14.0, GROUND, TALL, vec![(56.0, Gap::Window), (66.0, Gap::Window)]);
    zwall(b, 72.0, (-14.0, 14.0), GROUND, TALL, vec![]);

    // The Forecourt's boundary: the railings along the south edge, over the hill falling away into
    // fog, and the walls at its sides.
    xwall(b, (-50.0, 50.0), -30.0, GROUND, RAILING, vec![]);
    zwall(b, -50.0, (-30.0, -14.0), GROUND, H, vec![]);
    zwall(b, 50.0, (-30.0, -14.0), GROUND, H, vec![]);

    // The Garden Court's walls: the Long Ward's and the East side's, the Fire Escape's D13, and the
    // Refectory's south wall with D10.
    zwall(b, -29.0, (14.0, 42.0), GROUND, H, vec![]);
    zwall(b, -26.0, (14.0, 42.0), GROUND, H, vec![(17.0, door(GardenCourt, SunRoof, D13))]);
    zwall(b, 26.0, (14.0, 42.0), GROUND, H, vec![]);
    xwall(b, (-50.0, -26.0), 42.0, GROUND, H, vec![(-40.0, arch())]);
    xwall(b, (-26.0, 26.0), 42.0, GROUND, H, vec![(0.0, door(GardenCourt, Refectory, D10))]);
    xwall(b, (26.0, 50.0), 42.0, GROUND, H, vec![(38.0, arch())]);
    // The Wings' outer walls.
    zwall(b, -50.0, (14.0, 56.0), GROUND, H, vec![(18.0, Gap::Window), (24.0, Gap::Window)]);
    zwall(b, 50.0, (14.0, 56.0), GROUND, H, vec![(22.0, Gap::Window), (32.0, Gap::Window)]);
    // The Refectory's ends, with Q2 and Q4.
    zwall(b, -26.0, (42.0, 56.0), GROUND, H, vec![(49.0, gate(Refectory, DayRoom, Q2))]);
    zwall(b, 26.0, (42.0, 56.0), GROUND, H, vec![(49.0, gate(Refectory, TreatmentRoom, Q4))]);
    // The north wall of the ground floor: the Day Room's window, the Refectory's two, D11 into the
    // Main Stair's tower, and the Treatment Room's.
    xwall(
        b,
        (-50.0, 50.0),
        56.0,
        GROUND,
        H,
        vec![(-40.0, Gap::Window), (-16.0, Gap::Window), (2.0, door(Refectory, UpperLanding, D11)), (12.0, Gap::Window), (40.0, Gap::Window)],
    );
    // The Main Stair's tower: its sides, up to the Landing's floor, and its north wall, which is the
    // building's, two storeys tall.
    zwall(b, -4.0, (56.0, 64.0), GROUND, H, vec![]);
    zwall(b, 4.0, (56.0, 64.0), GROUND, H, vec![]);
    xwall(b, (-10.0, 10.0), 64.0, GROUND, TALL, vec![]);
    // Balustrades round the well, on the Landing's floor: along both sides and across the east half
    // of its south end (the stair's top is the west half's).
    for x in [-4.1, 4.1] {
        b.block(Vec3::new(0.2, RAIL, 7.6), Vec3::new(x, FIRST + RAIL / 2.0, 60.0), palette(Some(UpperLanding)).trim);
    }
    b.block(Vec3::new(4.0, RAIL, 0.2), Vec3::new(2.0, FIRST + RAIL / 2.0, 56.1), palette(Some(UpperLanding)).trim);

    // The first floor: the south wall (the Theatre's, the Landing's, the Isolation Ward's and the
    // Children's Ward's, with Q6), the Landing's side walls with Q5 and D12 and a window, the
    // Theatre's east wall, and the north wall in four runs (the gallery's is higher).
    xwall(b, (-50.0, 34.0), 42.0, FIRST, H, vec![(-38.0, gate(SunRoof, ChildrensWard, Q6))]);
    zwall(b, -10.0, (42.0, 64.0), FIRST, H, vec![(49.0, gate(UpperLanding, IsolationWard, Q5)), (61.0, Gap::Window)]);
    zwall(b, 10.0, (42.0, 64.0), FIRST, H, vec![(49.0, pdoor(UpperLanding, OperatingTheatre))]);
    zwall(b, 34.0, (42.0, 56.0), FIRST, H, vec![]);
    xwall(b, (-50.0, -10.0), 56.0, FIRST, H, vec![(-22.0, Gap::Window)]);
    xwall(b, (10.0, 14.0), 56.0, FIRST, H, vec![]);
    xwall(b, (14.0, 30.0), 56.0, GALLERY, TALL - GALLERY, vec![(18.0, Gap::Window), (26.0, Gap::Window)]);
    xwall(b, (30.0, 34.0), 56.0, FIRST, H, vec![]);
    // The Isolation Wing's two Zones meet at an archway, and the Children's Ward has its windows.
    zwall(b, -26.0, (42.0, 56.0), FIRST, H, vec![(49.0, arch())]);
    zwall(b, -50.0, (42.0, 56.0), FIRST, H, vec![(46.0, Gap::Window), (52.0, Gap::Window)]);

    // The Sun Roof's parapets: 1.6 m all round, higher round the two west windows, but for the one
    // gap on the court side, down into the Garden Court (beside D13's Fire Escape).
    xwall(b, (-50.0, -26.0), 14.0, FIRST, PARAPET, vec![]);
    zwall(b, -50.0, (14.0, 28.0), FIRST, PARAPET, vec![]);
    zwall(b, -50.0, (28.0, 40.0), FIRST, H, vec![(30.0, Gap::Window), (38.0, Gap::Window)]);
    zwall(b, -50.0, (40.0, 42.0), FIRST, PARAPET, vec![]);
    zwall(b, -26.0, (14.0, 31.0), FIRST, PARAPET, vec![]);
    zwall(b, -26.0, (34.0, 42.0), FIRST, PARAPET, vec![]);
    // A rail along the Long Ward's wall over the Fire Escape's well, which the roof's floor runs up to.
    b.block(Vec3::new(0.2, RAIL, 12.8), Vec3::new(-29.0, FIRST + RAIL / 2.0, 20.6), palette(Some(SunRoof)).trim);

    // The threshold under D11 and D13, between a floor and a stairwell's.
    b.block(Vec3::new(2.4, 0.2, 0.4), Vec3::new(2.0, -0.1, 56.0), palette(Some(UpperLanding)).floor);
    b.block(Vec3::new(0.4, 0.2, 2.4), Vec3::new(-26.0, -0.1, 17.0), palette(Some(SunRoof)).floor);

    // The front range: shut rooms between the corridors and the Forecourt, with floors, because the
    // hatches open onto them.
    b.ground(Vec3::new(40.0, 0.2, 8.0), Vec3::new(-30.0, -0.1, 4.0));
    b.ground(Vec3::new(40.0, 0.2, 8.0), Vec3::new(30.0, -0.1, 4.0));

    // A pocket of floor outside every window, walled in, where Zombies gather.
    for &(_, centre, out) in &data::WINDOWS {
        pocket(b, centre, out);
    }

    // The look in code: the bulkheads, the dead theatre lamp and the outside.
    scenery::build(b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::nav::{Gate, NavGraph};
    use crate::maps::map::MapId;
    use ZoneId::*;

    fn sanatorium() -> &'static Layout {
        MapId::Sanatorium.layout()
    }

    #[test]
    fn seventeen_zones_starting_in_the_entrance_hall_centred_on_the_origin() {
        assert_eq!(sanatorium().zones.len(), 17);
        assert_eq!(sanatorium().start_zone(), EntranceHall);
        assert_eq!(sanatorium().plan, (Vec2::new(-72.0, -30.0), Vec2::new(72.0, 66.0)));
        for zone in [EntranceHall, Forecourt, WestCorridor, EastCorridor, GardenCourt, Refectory, BoilerHouse, Mortuary, LongWard, DayRoom, BathHall, TreatmentRoom] {
            assert_eq!(zone.floor(), GROUND, "{}", zone.name());
        }
        for zone in [UpperLanding, OperatingTheatre, SunRoof, IsolationWard, ChildrensWard] {
            assert_eq!(zone.floor(), FIRST, "{}", zone.name());
        }
        assert_eq!(sanatorium().outdoor, [Forecourt, GardenCourt, SunRoof]);
        assert!(sanatorium().roofed);
        // The first Zone of the map's 17 are the 17 new ZoneIds, 96 of ZoneSet's 128 bits in all.
        assert_eq!(ZoneId::ALL.len(), 96);
    }

    /// The Doors a Run can buy, with their joins: 18 bought and the one Power Door.
    fn doors(sim: &mut crate::enemies::zombie::headless::Sim) -> Vec<(Option<(ZoneId, ZoneId)>, Option<u64>)> {
        use bevy::ecs::system::RunSystemOnce;

        use crate::maps::door::Door;
        sim.world_mut()
            .run_system_once(|doors: Query<&Door>| doors.iter().map(|door| (door.route(), door.price)).collect::<Vec<_>>())
            .unwrap()
    }

    /// The cheapest price of Doors from the Hall to each Zone, doors bought one by one.
    fn door_costs(doors: &[(Option<(ZoneId, ZoneId)>, Option<u64>)]) -> std::collections::HashMap<ZoneId, u64> {
        let mut cost = std::collections::HashMap::from([(EntranceHall, 0u64)]);
        for _ in 0..20 {
            for &(joins, price) in doors {
                let (Some((a, b)), Some(price)) = (joins, price) else { continue };
                for (from, to) in [(a, b), (b, a)] {
                    let Some(&here) = cost.get(&from) else { continue };
                    let best = cost.entry(to).or_insert(u64::MAX);
                    *best = (*best).min(here + price);
                }
            }
        }
        for &(a, b) in &FIXED_JOINS {
            let (ca, cb) = (cost.get(&a).copied(), cost.get(&b).copied());
            if let (Some(ca), None) = (ca, cb) {
                cost.insert(b, ca);
            }
            if let (None, Some(cb)) = (ca, cb) {
                cost.insert(a, cb);
            }
        }
        cost
    }

    /// 19 Doors: 18 bought and the Power Door D12. The Power costs 2000 either way (D2 and D4, or
    /// D1 and D5), and Pack-a-Punch 5000 in Doors from the Hall (D2, D8, D10 and D11).
    #[test]
    fn power_costs_2000_either_way_and_pack_a_punch_5000_in_doors() {
        use crate::enemies::zombie::headless::Sim;

        let mut sim = Sim::on(MapId::Sanatorium, 60.0, |_| {});
        let doors = doors(&mut sim);
        assert_eq!(doors.len(), 19);
        assert_eq!(doors.iter().filter(|(_, price)| price.is_none()).count(), 1, "one Power Door");
        let cost = door_costs(&doors);
        assert_eq!(cost[&BoilerHouse], 2000);
        // Both ways to the Boiler House cost the same: through the West Corridor, or the Forecourt.
        let via = |first: ZoneId, second: ZoneId, third: ZoneId| {
            let price = |a: ZoneId, b: ZoneId| {
                doors.iter().find(|(joins, _)| matches!(joins, Some((x, y)) if (*x, *y) == (a, b) || (*x, *y) == (b, a))).and_then(|d| d.1).unwrap()
            };
            price(first, second) + price(second, third)
        };
        assert_eq!(via(EntranceHall, WestCorridor, BoilerHouse), 2000);
        assert_eq!(via(EntranceHall, Forecourt, BoilerHouse), 2000);
        // D12 opens on the Power, so Pack-a-Punch is the Doors to the Landing. The Doors that turn
        // the Power on and reach it, the fewest points' worth in all, come to 5000.
        let power_door = doors.iter().find(|(_, price)| price.is_none()).unwrap().0.unwrap();
        assert_eq!(power_door, (UpperLanding, OperatingTheatre));
        let bought: Vec<((ZoneId, ZoneId), u64)> = doors.iter().filter_map(|&(joins, price)| Some((joins?, price?))).collect();
        assert_eq!(bought.len(), 18);
        let mut cheapest = u64::MAX;
        for mask in 0u32..(1 << bought.len()) {
            let cost: u64 = (0..bought.len()).filter(|i| mask >> i & 1 == 1).map(|i| bought[i].1).sum();
            if cost >= cheapest {
                continue;
            }
            let mut reached = std::collections::HashSet::from([EntranceHall]);
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
            if reached.contains(&BoilerHouse) && reached.contains(&UpperLanding) {
                cheapest = cost;
            }
        }
        assert_eq!(cheapest, 5000, "D2 and D4 for the Power, then D8, D10 and D11 up to the Power Door");
    }

    #[test]
    fn sanatorium_box_starts_two_doors_out() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Sanatorium, 60.0, |_| {});
        let doors = doors(&mut sim);
        let first = MapId::Sanatorium.box_locations().next().unwrap();
        let zone = crate::maps::map::BOX_LOCATIONS[first].0;
        // Breadth first from the Hall, one Door a step (the archways cost none).
        let mut hops = std::collections::HashMap::from([(EntranceHall, 0u32)]);
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
        assert_eq!(zone, GardenCourt);
        assert_eq!(hops[&zone], 2, "the Box starts in the Garden Court, two Doors out");
        // The cheapest of them: D2 and D8.
        assert_eq!(door_costs(&doors)[&zone], 1500);
    }

    #[test]
    fn counts_26_barriers_20_hellhound_spawns_and_the_spine_specials() {
        assert_eq!(MapId::Sanatorium.barriers().count(), 26);
        assert_eq!(sanatorium().risers.len(), 5);
        assert_eq!(sanatorium().vents.len(), 4);
        assert_eq!(sanatorium().hellhound_spawns.len(), 20);
        assert!(sanatorium().sections.is_empty() && sanatorium().generators.is_empty() && !sanatorium().darkness);
        for &zone in sanatorium().zones {
            let want = if matches!(zone, Forecourt | GardenCourt | Refectory) { 2 } else { 1 };
            assert_eq!(sanatorium().hellhound_spawns.iter().filter(|(z, _)| *z == zone).count(), want, "{}", zone.name());
        }
        let specials: Vec<ZoneId> = sanatorium().special_spawns.iter().map(|&(zone, _)| zone).collect();
        assert_eq!(specials, [Forecourt, GardenCourt, Refectory, SunRoof]);
        let boxes: Vec<ZoneId> = MapId::Sanatorium.box_locations().map(|i| crate::maps::map::BOX_LOCATIONS[i].0).collect();
        assert_eq!(boxes, [GardenCourt, Refectory, Mortuary, Forecourt, LongWard, ChildrensWard]);
        assert_eq!(sanatorium().lamps.len(), 96);
    }

    /// Every point of an indoor Zone's floor, clear of its walls, is within `LAMP_REACH` of a lamp in
    /// that Zone (the Upper Landing's stairwell included: its steps are floor too).
    #[test]
    fn sanatorium_every_point_of_an_indoor_zone_is_within_reach_of_a_lamp() {
        use crate::tuning::LAMP_REACH;
        let layout = sanatorium();
        for &zone in layout.zones.iter().filter(|&&zone| layout.indoor(zone)) {
            let (lo, hi) = zone.volume();
            let lamps: Vec<Vec2> = layout.lamps.iter().filter(|(lamp, _)| *lamp == zone).map(|(_, at)| Vec2::new(at.x, at.z)).collect();
            assert!(!lamps.is_empty(), "{zone:?} has no lamps");
            let mut x = lo.x + 1.0;
            while x <= hi.x - 1.0 + 1e-3 {
                let mut z = lo.z + 1.0;
                while z <= hi.z - 1.0 + 1e-3 {
                    let nearest = lamps.iter().map(|lamp| lamp.distance(Vec2::new(x, z))).fold(f32::MAX, f32::min);
                    assert!(nearest <= LAMP_REACH, "({x}, {z}) in {zone:?} is {nearest} m from a lamp");
                    z += 0.25;
                }
                x += 0.25;
            }
        }
    }

    /// Every Wing Zone pair has 3 Spawn Points: window Barriers plus Risers, Vents and Specials.
    #[test]
    fn sanatorium_every_wing_zone_pair_has_three_spawn_points() {
        let layout = sanatorium();
        let points = |zone: ZoneId| {
            MapId::Sanatorium.barriers().filter(|b| crate::maps::map::WINDOWS[b.0 as usize].0 == zone).count()
                + layout.risers.iter().filter(|(z, _)| *z == zone).count()
                + layout.vents.iter().filter(|(z, _)| *z == zone).count()
                + layout.special_spawns.iter().filter(|(z, _)| *z == zone).count()
                + layout.hellhound_spawns.iter().filter(|(z, _)| *z == zone).count()
        };
        for (a, b) in FIXED_JOINS {
            assert!(points(a) + points(b) >= 3, "{} and {} have only {} Spawn Points", a.name(), b.name(), points(a) + points(b));
        }
    }

    /// The Sun Roof's parapets are at least 1.6 m, but at the one drop.
    #[test]
    fn sanatorium_sun_roof_outer_parapets_are_at_least_1_6_m() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let mut sim = Sim::on(MapId::Sanatorium, 60.0, |_| {});
        let low: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                let mut low = Vec::new();
                // Along each outer edge, a hand's width inside it: the south, the west, and the court side.
                let mut edges: Vec<(f32, f32, &str)> = Vec::new();
                for i in 0..=46 {
                    let t = i as f32 * 0.5;
                    edges.push((-49.8 + t, 14.0, "south"));
                }
                for i in 0..=54 {
                    let t = i as f32 * 0.5;
                    edges.push((-50.0, 14.0 + t.min(27.8), "west"));
                    edges.push((-26.0, 14.0 + t.min(27.8), "court"));
                }
                for (x, z, side) in edges {
                    // The Sun Roof's own floor, at y 4; the parapet's top is the first thing under 4+3.
                    let Some(hit) = spatial.cast_ray(Vec3::new(x, FIRST + 3.0, z), Dir3::NEG_Y, 5.0, true, &filter) else { continue };
                    let top = FIRST + 3.0 - hit.distance;
                    // The drop's gap, z 31 to 34 on the court side, is the one place that's flush.
                    let in_gap = side == "court" && (31.2..=33.8).contains(&z);
                    let (floor_top, wall_top) = (FIRST + 0.05, FIRST + PARAPET - 0.05);
                    if in_gap {
                        assert!(top <= floor_top, "the gap at z {z} has something {top} high in it");
                    } else if top < wall_top {
                        low.push(format!("{side} edge at ({x}, {z}) is only {} m", top - FIRST));
                    }
                }
                low
            })
            .unwrap();
        assert!(low.is_empty(), "{low:#?}");
    }

    /// The first-floor windows' Spawn Points stand on lean-to roofs, and the front range's on its floor.
    #[test]
    fn sanatorium_front_range_spawn_points_stand_on_a_floor() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let mut sim = Sim::on(MapId::Sanatorium, 60.0, |_| {});
        let bad: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                MapId::Sanatorium
                    .barriers()
                    .filter_map(|barrier| {
                        let spawn = crate::maps::map::window_spawn(barrier);
                        let hit = spatial.cast_ray(spawn + Vec3::Y * 0.5, Dir3::NEG_Y, 1.0, true, &filter);
                        let floor = hit.map(|hit| spawn.y + 0.5 - hit.distance);
                        (floor.is_none_or(|floor| (floor - spawn.y).abs() > 0.05)).then(|| format!("window {} at {spawn}: floor {floor:?}", barrier.0 - W0))
                    })
                    .collect()
            })
            .unwrap();
        assert!(bad.is_empty(), "{bad:#?}");
        // The ones in the front range are the Hall's two and the corridors' four.
        let front_range = MapId::Sanatorium
            .barriers()
            .filter(|b| {
                let at = crate::maps::map::window_spawn(*b);
                (0.0..8.0).contains(&at.z) && at.x.abs() > 10.0 && at.x.abs() < 50.0
            })
            .count();
        assert_eq!(front_range, 6);
    }

    /// The Main Stair's tower and the Fire Escape's bay stand in no ground Zone's box.
    #[test]
    fn sanatorium_stair_bays_sit_in_no_ground_zone() {
        let bays = [("Main Stair", Vec2::new(-4.0, 56.0), Vec2::new(4.0, 64.0)), ("Fire Escape", Vec2::new(-29.0, 14.0), Vec2::new(-26.0, 27.0))];
        for (name, min, max) in bays {
            for zone in sanatorium().zones.iter().filter(|zone| zone.floor() == GROUND) {
                let (lo, hi) = zone.volume();
                let overlap = Vec2::new(lo.x, lo.z).cmplt(max).all() && min.cmplt(Vec2::new(hi.x, hi.z)).all();
                assert!(!overlap, "the {name}'s bay is in the {}'s box", zone.name());
            }
        }
        // Each is cut in the floor of the Zone at its top, whose volume stops at its floor.
        for zone in [UpperLanding, SunRoof] {
            assert!(zone.volume().0.y > GROUND, "{}", zone.name());
            assert!(sanatorium().sunken.iter().any(|sunken| sunken.zone == zone && sunken.depth == STOREY));
        }
    }

    /// Everything placed in the Map that a Quarantine Door's Leaves must keep clear of: Spawn Points,
    /// Buyables, Box Locations, chalk outlines, Traps, the machines and the Power Switch.
    fn placed_things() -> Vec<(String, Vec3)> {
        use crate::maps::map::{BOX_LOCATIONS, window_spawn};
        use crate::weapons::equipment::equipment_walls;
        let layout = sanatorium();
        let mut things: Vec<(String, Vec3)> = Vec::new();
        for barrier in MapId::Sanatorium.barriers() {
            things.push((format!("window {}'s Spawn Point", barrier.0 - W0), window_spawn(barrier)));
        }
        for &(zone, at) in layout.hellhound_spawns.iter().chain(layout.special_spawns).chain(layout.risers).chain(layout.vents) {
            things.push((format!("a Spawn Point in the {}", zone.name()), at));
        }
        for i in MapId::Sanatorium.box_locations() {
            things.push(("a Box Location".into(), BOX_LOCATIONS[i].1));
        }
        for &(weapon, _, at, _) in layout.wall_weapons {
            things.push((format!("the {weapon:?} wall"), at));
        }
        for &(_, at, _) in crate::maps::trap::traps(MapId::Sanatorium) {
            things.push(("a Trap".into(), at));
        }
        for at in crate::maps::trap::switch_spots(MapId::Sanatorium) {
            things.push(("a Trap switch".into(), at));
        }
        for perk in crate::economy::perk::Perk::ALL {
            things.push((format!("{perk:?}'s machine"), perk.machine_spot(MapId::Sanatorium).0));
        }
        for &(wall, _, at, _) in equipment_walls(MapId::Sanatorium) {
            things.push((format!("{wall:?}"), at));
        }
        things.push(("Pack-a-Punch".into(), crate::weapons::pack_a_punch::machine_at(MapId::Sanatorium)));
        if let Some((at, _)) = crate::maps::power::switch_spot(MapId::Sanatorium) {
            things.push(("the Power Switch".into(), at));
        }
        things
    }

    /// Nothing is within 1.5 m of a Quarantine doorway's outside face: no Spawn Point, Buyable, chalk
    /// outline, Box Location, Trap or Prop.
    #[test]
    fn sanatorium_nothing_stands_within_1_5_m_of_a_quarantine_doorway() {
        let layout = sanatorium();
        let things = placed_things();
        for &(name, doorway, out, ..) in &QUARANTINE_DOORWAYS {
            let face = Vec2::new(doorway.x, doorway.z) + out * 0.2;
            for (what, at) in &things {
                if (at.y - doorway.y).abs() > 3.0 {
                    continue;
                }
                let d = face.distance(Vec2::new(at.x, at.z));
                assert!(d >= 1.5, "{what} is {d:.2} m from {name}'s doorway");
            }
            for prop in layout.props {
                let (centre, half) = (Vec2::new(prop.at.x, prop.at.z), Vec2::new(prop.size.x, prop.size.z) / 2.0);
                let nearest = (face - centre).abs() - half;
                let d = nearest.max(Vec2::ZERO).length();
                assert!(prop.zone.floor() != doorway.y || d >= 1.5, "a Prop is {d:.2} m from {name}'s doorway");
            }
            // Each doorway has 2.4 m of room between its Door's leaves, and an open floor at least 6 m across outside it.
            let (outside_zone, _) = (QUARANTINE_DOORWAYS.iter().find(|q| q.0 == name).unwrap().3, ());
            let (lo, hi) = outside_zone.volume();
            let across = if out.x != 0.0 { hi.z - lo.z } else { hi.x - lo.x };
            let deep = if out.x != 0.0 { hi.x - lo.x } else { hi.z - lo.z };
            assert!(across >= 6.0 && deep >= 6.0, "{name}'s outside is {across} by {deep} m");
        }
    }

    /// The Sun Roof's drop is a one-way edge gated on D13, and no fixed join.
    #[test]
    fn the_sun_roofs_drop_is_one_way_and_gated_on_d13() {
        let drops: Vec<(ZoneId, ZoneId)> = data::EDGES
            .iter()
            .filter_map(|&(.., gate)| match gate {
                Gate::DoorDrop(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        assert_eq!(drops, [(GardenCourt, SunRoof)]);
        assert!(!FIXED_JOINS.iter().any(|&(a, b)| (a, b) == (GardenCourt, SunRoof) || (a, b) == (SunRoof, GardenCourt)));
        let graph = NavGraph::new(MapId::Sanatorium);
        let (top, bottom) = (graph.nodes[graph.node("sr_drop").unwrap()].at, graph.nodes[graph.node("gc_drop").unwrap()].at);
        assert!(top.y - bottom.y >= 3.9);
    }

    // ---- Wings and Quarantine Doors ----

    use crate::maps::quarantine::WingId;
    use crate::maps::shifting::{outside_landing, pieces};

    /// Every join the Map can make with all its Doors bought and every Quarantine Door at `rests`
    /// (a Rest for each Piece): the ordinary Doors, the fixed joins and what the Pieces' Rests join.
    fn joins_at(rests: &[u8]) -> Vec<(ZoneId, ZoneId)> {
        let layout = sanatorium();
        let mut joins: Vec<(ZoneId, ZoneId)> = layout.fixed_joins.to_vec();
        for (id, piece) in pieces(layout) {
            joins.extend(piece.open_joins(Some(rests[usize::from(id.0)]), true).iter().copied());
        }
        joins.extend(ordinary_door_joins());
        joins
    }

    /// The joins of the Map's Doors that are not Quarantine gates (the bought Doors and the Power
    /// Door), from the Doors the Map builds. The gate Doors of the Quarantine Doors join nothing
    /// themselves: their Pieces' Rests do.
    fn ordinary_door_joins() -> Vec<(ZoneId, ZoneId)> {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Sanatorium, 60.0, |_| {});
        let all = doors(&mut sim);
        let gates: Vec<(ZoneId, ZoneId)> = pieces(sanatorium()).filter_map(|(_, piece)| piece.door).collect();
        all.into_iter()
            .filter_map(|(route, _)| route)
            .filter(|&(a, b)| !gates.iter().any(|&gate| gate == (a, b) || gate == (b, a)))
            .collect()
    }

    /// The Zones reachable from the Entrance Hall over `joins`.
    fn reach(joins: &[(ZoneId, ZoneId)]) -> std::collections::HashSet<ZoneId> {
        let mut seen = std::collections::HashSet::from([EntranceHall]);
        loop {
            let before = seen.len();
            for &(a, b) in joins {
                if seen.contains(&a) || seen.contains(&b) {
                    seen.insert(a);
                    seen.insert(b);
                }
            }
            if seen.len() == before {
                return seen;
            }
        }
    }

    fn wing_of(zone: ZoneId) -> Option<usize> {
        sanatorium().wings.iter().position(|wing| wing.zones.contains(&zone))
    }

    #[test]
    fn only_the_sanatorium_has_wings_three_of_two_zones_in_the_order_west_east_isolation() {
        for map in MapId::ALL {
            assert_eq!(map.layout().wings.len(), if map == MapId::Sanatorium { 3 } else { 0 }, "{}", map.name());
        }
        let names: Vec<&str> = sanatorium().wings.iter().map(|wing| wing.name).collect();
        assert_eq!(names, ["West Wing", "East Wing", "Isolation Wing"]);
        for wing in sanatorium().wings {
            assert_eq!(wing.zones.len(), 2, "{}", wing.name);
            assert!(wing.zones.iter().all(|zone| sanatorium().zones.contains(zone)));
        }
        let all: Vec<ZoneId> = sanatorium().wings.iter().flat_map(|wing| wing.zones.iter().copied()).collect();
        assert_eq!(all.len(), all.iter().collect::<std::collections::HashSet<_>>().len(), "no Zone is in two Wings");
    }

    /// Six Quarantine Doors, two to a Wing, each of two 1.2 x 2.4 x 0.06 m Leaves, Open at the start.
    #[test]
    fn six_quarantine_doors_two_leaves_each_open_joining_outside_to_inside_and_sealed_joining_nothing() {
        use crate::maps::shifting::Driver;
        let layout = sanatorium();
        assert_eq!(layout.shifting.len(), 6);
        for (id, piece) in pieces(layout) {
            assert_eq!(piece.driver, Driver::Quarantine(WingId(id.0 / 2)), "{id:?}");
            assert_eq!(piece.leaves.len(), 2);
            for leaf in piece.leaves {
                assert_eq!(leaf.size, Vec3::new(1.2, 2.4, 0.06));
            }
            assert_eq!((piece.rests.len(), piece.start), (2, 0));
            let (outside, inside) = piece.door.expect("a gate Door");
            assert_eq!(piece.rests[0].joins, [(outside, inside)], "Open joins outside and inside");
            assert!(piece.rests[1].joins.is_empty(), "Sealed joins nothing");
            assert_eq!(wing_of(outside), None, "{id:?}'s outside is in no Wing");
            assert_eq!(wing_of(inside), Some(usize::from(id.0 / 2)), "{id:?}'s inside is in its own Wing");
            assert_eq!(piece.zone, outside);
            assert!(piece.levers().is_empty());
        }
    }

    /// Each Wing's Zones join the rest of the Map only through its own Quarantine Doors, no Wing joins
    /// another, and every Wing has at least two doorways and two Spawn Points.
    #[test]
    fn each_wings_zones_join_outside_only_through_its_own_quarantine_doors() {
        let layout = sanatorium();
        let open = joins_at(&[0; 6]);
        for (index, wing) in layout.wings.iter().enumerate() {
            let own: Vec<(ZoneId, ZoneId)> =
                pieces(layout).filter(|(_, piece)| piece.wing() == Some(WingId(index as u8))).map(|(_, piece)| piece.door.unwrap()).collect();
            assert!(own.len() >= 2, "{} has only {} doorways", wing.name, own.len());
            let mut crossings = 0;
            for &(a, b) in &open {
                let (a_in, b_in) = (wing.zones.contains(&a), wing.zones.contains(&b));
                if a_in == b_in {
                    continue;
                }
                crossings += 1;
                let outside = if a_in { b } else { a };
                assert!(
                    own.iter().any(|&(o, i)| (o, i) == (a, b) || (o, i) == (b, a)),
                    "{} joins {} to {} outside its Quarantine Doors",
                    wing.name,
                    a.name(),
                    b.name()
                );
                assert_eq!(wing_of(outside), None, "{} joins another Wing's {}", wing.name, outside.name());
            }
            assert_eq!(crossings, own.len(), "{}", wing.name);
            // The doorways stand on two different Zones outside.
            let mut outsides: Vec<&str> = own.iter().map(|&(o, _)| o.name()).collect();
            outsides.sort_unstable();
            outsides.dedup();
            assert_eq!(outsides.len(), own.len(), "{}'s doorways open on one Zone", wing.name);
            // Spawn Points of its own: window Barriers, Risers and Vents.
            let points: usize = wing
                .zones
                .iter()
                .map(|&zone| {
                    MapId::Sanatorium.barriers().filter(|b| crate::maps::map::WINDOWS[b.0 as usize].0 == zone).count()
                        + layout.risers.iter().filter(|(z, _)| *z == zone).count()
                        + layout.vents.iter().filter(|(z, _)| *z == zone).count()
                })
                .sum();
            assert!(points >= 2, "{} has {points} Spawn Points", wing.name);
        }
    }

    /// The Zones in no Wing stay connected without any Wing, with every ordinary Door bought.
    #[test]
    fn the_zones_in_no_wing_are_connected_without_any_wing() {
        let reached = reach(&joins_at(&[1; 6]));
        for &zone in sanatorium().zones {
            if wing_of(zone).is_none() {
                assert!(reached.contains(&zone), "{} is cut off with every Wing Sealed", zone.name());
            } else {
                assert!(!reached.contains(&zone), "{} is reached through a Sealed Wing", zone.name());
            }
        }
    }

    /// With each Wing Sealed in turn and every Door bought, every other Zone is reachable by joins and
    /// by nav; the Sealed Wing's own nodes are not.
    #[test]
    fn with_each_wing_sealed_and_every_door_bought_all_other_zones_are_reachable_by_joins_and_nav() {
        use crate::enemies::nav::NavState;
        let layout = sanatorium();
        let graph = NavGraph::new(MapId::Sanatorium);
        let mut doors: Vec<(ZoneId, ZoneId)> = ordinary_door_joins();
        doors.extend(pieces(layout).filter_map(|(_, piece)| piece.door));
        let yes = |_: crate::maps::map::BarrierId| true;
        let start = graph.nearest(layout.seat_start(0));
        for (sealed, sealed_wing) in layout.wings.iter().enumerate() {
            let rests: Vec<u8> = pieces(layout).map(|(_, piece)| u8::from(piece.wing() == Some(WingId(sealed as u8)))).collect();
            let reached = reach(&joins_at(&rests));
            let standing: Vec<Option<u8>> = rests.iter().map(|&rest| Some(rest)).collect();
            let state = NavState::new(&doors, &yes).with_rests(&standing);
            for &zone in layout.zones {
                let inside = sealed_wing.zones.contains(&zone);
                assert_eq!(reached.contains(&zone), !inside, "{} with the {} Sealed", zone.name(), sealed_wing.name);
            }
            for (to, node) in graph.nodes.iter().enumerate() {
                let Some(zone) = node.zone else { continue };
                if node.vent || node.name.ends_with("_out") {
                    continue;
                }
                let planned = graph.plan(to, start, state).is_some();
                assert_eq!(planned, !sealed_wing.zones.contains(&zone), "{} ({}) with the {} Sealed", node.name, zone.name(), sealed_wing.name);
            }
        }
    }

    #[test]
    fn no_power_switch_or_pack_a_punch_is_in_a_wing() {
        let power = crate::maps::power::switch_spot(MapId::Sanatorium).expect("the Sanatorium has a Power Switch").0;
        let pap = crate::weapons::pack_a_punch::machine_at(MapId::Sanatorium);
        for (what, at) in [("the Power Switch", power), ("Pack-a-Punch", pap)] {
            let zone = crate::enemies::nav::zone_at(MapId::Sanatorium, at + Vec3::Y * 0.1).unwrap_or_else(|| panic!("{what} is in no Zone"));
            assert_eq!(wing_of(zone), None, "{what} is in the {}", zone.name());
        }
    }

    /// Each Piece's outside landing is clear of every sweep, outside its Wing, and 2.5 m out.
    #[test]
    fn each_quarantine_door_has_an_outside_landing_clear_of_its_sweep() {
        let layout = sanatorium();
        let graph = NavGraph::new(MapId::Sanatorium);
        for (id, piece) in pieces(layout) {
            let landing = outside_landing(&graph, id, piece).unwrap_or_else(|| panic!("{id:?} has no outside landing"));
            let (outside, _) = piece.door.unwrap();
            assert_eq!(crate::enemies::nav::zone_at(MapId::Sanatorium, landing + Vec3::Y * 0.1), Some(outside), "{id:?}'s landing is in {}", outside.name());
            assert!(!piece.in_way(landing, 0, 1) && !piece.in_way(landing, 1, 0), "{id:?}'s landing is in its sweep");
            let (_, doorway, out, ..) = QUARANTINE_DOORWAYS[usize::from(id.0)];
            let away = (Vec2::new(landing.x, landing.z) - Vec2::new(doorway.x, doorway.z)).dot(out);
            assert!((away - 2.5).abs() < 0.05, "{id:?}'s landing is {away} m out");
        }
    }

    /// No Spawn Point, Buyable, Box Location, chalk outline or Trap is in a sweep, and no Prop is.
    #[test]
    fn nothing_placed_is_in_a_quarantine_doors_sweep() {
        let layout = sanatorium();
        for (what, at) in placed_things() {
            for (id, piece) in pieces(layout) {
                assert!(!piece.in_way(at, 0, 1) && !piece.in_way(at, 1, 0), "{what} is in {id:?}'s sweep");
            }
        }
        for prop in layout.props {
            let feet = Vec3::new(prop.at.x, prop.zone.floor(), prop.at.z);
            let half = Vec2::new(prop.size.x, prop.size.z) / 2.0;
            for (id, piece) in pieces(layout) {
                for corner in [Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::ZERO] {
                    let at = feet + Vec3::new(corner.x * half.x, 0.0, corner.y * half.y);
                    assert!(!piece.in_way(at, 0, 1), "a Prop of the {} is in {id:?}'s sweep", prop.zone.name());
                }
            }
        }
    }

    /// Each doorway's nodes chain a Door edge to a Rest edge, and no path node sits on a Leaf.
    #[test]
    fn the_doorway_nodes_chain_a_door_edge_to_a_rest_edge_and_no_node_sits_on_a_leaf() {
        let layout = sanatorium();
        let graph = NavGraph::new(MapId::Sanatorium);
        for (id, piece) in pieces(layout) {
            let (outside, inside) = piece.door.unwrap();
            let rest: Vec<_> = graph.edges.iter().filter(|edge| edge.gate == Gate::Rest(id, 0)).collect();
            assert_eq!(rest.len(), 1, "{id:?}");
            let (doorway, far) = (&graph.nodes[rest[0].from], &graph.nodes[rest[0].to]);
            assert_eq!((doorway.zone, far.zone), (Some(outside), Some(inside)), "{id:?}");
            let door_edges = graph.edges.iter().filter(|edge| matches!(edge.gate, Gate::Door(a, b) if (a, b) == (outside, inside)) && edge.to == rest[0].from).count();
            assert_eq!(door_edges, 1, "{id:?}'s landing reaches its doorway node through the gate Door");
            for node in &graph.nodes {
                for rest in piece.rests {
                    for (leaf, &yaw) in piece.leaves.iter().zip(rest.yaws) {
                        assert!(!leaf.over(node.at, yaw, 0.0), "{} is on a Leaf of {id:?}", node.name);
                    }
                }
            }
        }
        assert_eq!(graph.edges.iter().filter(|edge| matches!(edge.gate, Gate::Rest(..))).count(), 6);
    }

    /// Every Zone of the Map wears a Material Set, and so does its outside (grass and white render gone green).
    #[test]
    fn every_sanatorium_zone_has_a_material_set() {
        use crate::maps::map::{materials, textures::Material};
        for &zone in sanatorium().zones {
            assert!(materials(Some(zone)).is_some(), "{} has no Material Set", zone.name());
        }
        let outside = sanatorium().outside.expect("the Sanatorium has an outside");
        assert_eq!((outside.floor, outside.wall), (Material::GrassGround, Material::WornMossyPlasterwall));
    }

    /// Two Zones that are joined (by a Door or a fixed archway) differ in the family of their floor or their wall.
    #[test]
    fn sanatorium_joined_neighbours_differ_in_floor_or_wall_family() {
        use crate::maps::map::materials;
        let mut joins: Vec<(ZoneId, ZoneId)> = data::EDGES
            .iter()
            .filter_map(|&(.., gate)| match gate {
                Gate::Door(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        joins.extend(FIXED_JOINS);
        assert!(joins.len() >= 18, "only {} joins", joins.len());
        for (a, b) in joins {
            let (ma, mb) = (materials(Some(a)).unwrap(), materials(Some(b)).unwrap());
            assert!(
                ma.floor.family() != mb.floor.family() || ma.wall.family() != mb.wall.family(),
                "{} and {} share floor and wall families",
                a.name(),
                b.name()
            );
        }
    }
}
