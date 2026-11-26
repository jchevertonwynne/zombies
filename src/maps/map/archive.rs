//! The Archive Map: a flooded library under a university, in 14 Zones on three levels going down
//! from the start. Its five Ranges are the Moving Stacks: each a Shifting Piece whose Bookcases slide along a
//! floor rail, joining two Zones through its one open 3 m Aisle behind a bought gate Door, and rolled by the
//! Wheels at both mouths of every Aisle (`shifting`).
//!
//! Built at the world origin (ADR 0009), plan (-36, 0) to (36, 36), with no Zone out of doors:
//!
//! - The Reading level (y 8): the Catalogue Hall (start), the Reading Room and the Map Room, and the
//!   Lecture Theatre's top crossover, which rakes down to its dais at y 4. The Great Range (R0) stands
//!   in the wall between the Catalogue Hall and the Reading Room.
//! - The Stacks level (y 4): four ring halls (the Old, Folio, Periodical and New Stacks) round the
//!   book-lift core, each joined to the next by a Range (RA to RD), the Bindery, and the Well Stair's
//!   top landing. The Map Stair runs down from the Map Room along its north wall to D3, and the
//!   Bindery Stair down from the Bindery to D8.
//! - The Cellar (y 0): the Flooded Cellar (a Still Pool over the whole floor, Shallow everywhere, with the
//!   Causeway across it), the Undercroft, the Strong Room (Pack-a-Punch) and the Pump Room (the Power).
//!
//! There are no windows underground, so each window is a boarded service hatch: a window in a hatch
//! wall that stands `PEN` in from an outer wall of its Zone, with a walled pen of floor behind it where
//! the Zombies gather (the back-of-house; the path nodes there end in `_out`, as the Polder's strips do).
//!
//! The lamps, the Ranges, the fixed stacks and the per-Zone tables are in separate items on purpose:
//! the Material Sets and the Props each come in a later ticket. Its path
//! nodes, lamps, windows, stairwells and Props (in `data`) are laid out by `tools/archive_layout.py`.

use bevy::prelude::*;

use crate::enemies::sapper::SapperTiming;
use crate::maps::shifting::{Driver, Leaf, Motion, Piece, Rest, Wheel};
use crate::maps::stacks_look;
use crate::maps::zone::ZoneId;
use crate::tuning::{AISLE_WIDTH, BOOKCASE_DEPTH, BOOKCASE_HEIGHT, BOOKCASE_LENGTH};
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, Infill, Layout, Level, Pool, Prop, RaisedFloor, STOREY, Slope, concat, door, palette};

#[path = "archive/data.rs"]
mod data;
#[path = "archive/daylight.rs"]
pub(crate) mod daylight;
#[path = "archive/flood.rs"]
pub(crate) mod flood;

pub(in crate::maps::map) use data::WINDOWS;

/// The levels: the Cellar, the Stacks level a storey up and the Reading level two up.
const CELLAR: f32 = 0.0;
const STACKS: f32 = STOREY;
const READING: f32 = 2.0 * STOREY;
/// How high a chalk outline's centre sits over its floor.
const CHALK: f32 = 1.5;
/// How tall a balustrade round a stairwell is, and a pen's walls.
const RAIL: f32 = 1.1;
const PEN_WALL: f32 = 3.0;
/// How far a window's hatch wall stands in from the wall it faces, and how wide a pen is each side of its window.
pub(super) const PEN: f32 = 2.8;
const PEN_HALF: f32 = 2.0;

/// The first of the Archive's windows in `WINDOWS`, after the other Maps'.
const W0: u8 = (super::greybox::WINDOWS.len()
    + super::quarry::WINDOWS.len()
    + super::village::WINDOWS.len()
    + super::dam::WINDOWS.len()
    + super::cove::WINDOWS.len()
    + super::polder::WINDOWS.len()
    + super::sanatorium::WINDOWS.len()) as u8;

/// The Doors' prices: G0 and GA to GD are the Ranges' gates, then D1 to D10 (D7 was cut, and PD is the Power Door).
const G0: u64 = 750;
const GA: u64 = 1000;
const GB: u64 = 1000;
const GC: u64 = 1000;
const GD: u64 = 1000;
const D1: u64 = 750;
const D2: u64 = 1000;
const D3: u64 = 1000;
const D4: u64 = 1000;
const D5: u64 = 1000;
const D6: u64 = 1250;
const D8: u64 = 1250;
const D9: u64 = 1000;
const D10: u64 = 1000;

/// A point in the Archive.
const fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A spot on a wall or a floor's plan in the Archive.
const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

pub(super) const ARCHIVE: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [CatalogueHall, ReadingRoom, MapRoom, LectureTheatre, OldStacks, FolioStacks, PeriodicalStacks, NewStacks, Bindery, WellStair, FloodedCellar, Undercroft, StrongRoom, PumpRoom]
    },
    // In a row across the Catalogue Hall, between the south wall's chalk and the card catalogues.
    seats: [at(-4.5, READING, 3.5), at(-1.5, READING, 3.5), at(1.5, READING, 3.5), at(4.5, READING, 3.5)],
    wall_weapons: &WALL_WEAPONS,
    windows: &data::WINDOWS,
    risers: &[],
    wading: &WADING_POINTS,
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &RAISED_FLOORS,
    infills: &data::INFILLS,
    slopes: &SLOPES,
    sunken: &data::SUNKEN,
    water: &POOLS,
    canals: &[],
    props: &PROPS,
    lamps: &data::LAMPS,
    fixed_joins: &FIXED_JOINS,
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
    outdoor: &[],
    roofed: true,
    // Under a university, so there is no outside: the plan's edge is its walls.
    outside: None,
    sun: Vec3::new(0.3, -1.0, 0.5),
    // Lamplight: no sky and no sun, the daylight faked by `daylight` (#535).
    look: crate::maps::look::LAMPLIGHT,
    // Brass pendants with opal globes; the five cellar Zones keep caged bulbs, which run at half until the Power.
    lamp_style: super::LampStyle::Pendant,
    bulb_zones: &[ZoneId::WellStair, ZoneId::FloodedCellar, ZoneId::Undercroft, ZoneId::StrongRoom, ZoneId::PumpRoom],
    plan: (Vec2::new(-36.0, 0.0), Vec2::new(36.0, 36.0)),
    build,
};

/// The Well Stair's archway into the Flooded Cellar never closes.
const FIXED_JOINS: [(ZoneId, ZoneId); 1] = [(ZoneId::WellStair, ZoneId::FloodedCellar)];

// ---------------------------------------------------------------- The Ranges

/// A Range: 2 to 4 Bookcases on one floor rail between fixed end walls, with exactly one 3 m Aisle open. A Range of
/// `bookcases` Bookcases has `bookcases + 1` Rests (Aisle positions). Each is a wall opening filled by its Bookcases
/// (Leaves of a `Piece`, sliding along the rail); its gate Door stands in a grille along the whole `gate_side` face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Range {
    pub name: &'static str,
    /// Whether the rail runs along x (its wall along x at `line`, z) or along z (its wall at x = `line`).
    pub along_x: bool,
    /// Where the wall stands across the rail.
    pub line: f32,
    /// Where the rail starts along its length.
    pub rail: f32,
    pub bookcases: u8,
    /// The Rest the Aisle is at to begin with.
    pub start: u8,
    /// The floor it stands on.
    pub floor: f32,
    /// Which side of the Range its gate Door is on (-1 or +1 across the rail).
    pub gate_side: f32,
    /// The two Zones it joins, the low side first, and what its gate Door costs.
    pub zones: (ZoneId, ZoneId),
    pub price: u64,
}

/// A Bookcase is 1.2 m deep (along the rail), 5.0 m long (across it) and 3.0 m tall to a soffit; the Aisle is 3.0 m
/// wide; a clear 0.6 m of floor lies beyond each end of the Aisles, for the mouth nodes and the Wheels' posts.
pub(super) const BOOKCASE: Vec3 = Vec3::new(BOOKCASE_DEPTH, BOOKCASE_HEIGHT, BOOKCASE_LENGTH);
pub(super) const AISLE: f32 = AISLE_WIDTH;
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const MOUTH: f32 = 0.6;
/// How far from a Bookcase's face a Wheel's post stands, inside the mouth strip.
const WHEEL_OFF: f32 = 0.3;
/// How far from a footprint's face its gate Door's wall stands (to its middle).
const GATE_OFF: f32 = 1.4;

pub(super) const RANGES: [Range; 5] = {
    use ZoneId::*;
    [
        // R0, the Great Range: the wall between the Catalogue Hall and the Reading Room, its Aisle in the middle.
        Range { name: "r0", along_x: true, line: 12.0, rail: -3.9, bookcases: 4, start: 2, floor: READING, gate_side: 1.0, zones: (CatalogueHall, ReadingRoom), price: G0 },
        // RA, between the Old and the Folio Stacks, its Aisle at the west end.
        Range { name: "ra", along_x: true, line: 20.0, rail: -14.3, bookcases: 3, start: 0, floor: STACKS, gate_side: 1.0, zones: (OldStacks, FolioStacks), price: GA },
        // RB, between the Folio and the Periodical Stacks, its Aisle at the south end.
        Range { name: "rb", along_x: false, line: 0.0, rail: 26.8, bookcases: 2, start: 0, floor: STACKS, gate_side: 1.0, zones: (FolioStacks, PeriodicalStacks), price: GB },
        // RC, between the New and the Periodical Stacks, its Aisle at the east end.
        Range { name: "rc", along_x: true, line: 20.0, rail: 7.7, bookcases: 3, start: 3, floor: STACKS, gate_side: 1.0, zones: (NewStacks, PeriodicalStacks), price: GC },
        // RD, between the Old and the New Stacks, its Aisle at the north end.
        Range { name: "rd", along_x: false, line: 0.0, rail: 7.8, bookcases: 2, start: 2, floor: STACKS, gate_side: 1.0, zones: (OldStacks, NewStacks), price: GD },
    ]
};

impl Range {
    /// How long its rail is.
    pub fn length(&self) -> f32 {
        f32::from(self.bookcases) * BOOKCASE.x + AISLE
    }

    /// Where its Aisle is at `rest`, as the middle of the Aisle along the rail.
    pub const fn aisle_mid(&self, rest: u8) -> f32 {
        self.rail + BOOKCASE.x * rest as f32 + AISLE / 2.0
    }

    /// A rectangle from `along` (a start and end on the rail) and `across` (offsets from the wall).
    fn rect(&self, along: (f32, f32), across: (f32, f32)) -> (Vec2, Vec2) {
        if self.along_x {
            (Vec2::new(along.0, self.line + across.0), Vec2::new(along.1, self.line + across.1))
        } else {
            (Vec2::new(self.line + across.0, along.0), Vec2::new(self.line + across.1, along.1))
        }
    }

    /// A point `along` the rail and `across` it from the wall.
    pub const fn point(&self, along: f32, across: f32) -> Vec2 {
        if self.along_x { Vec2::new(along, self.line + across) } else { Vec2::new(self.line + across, along) }
    }

    /// Its sweep: the rail's length by the Bookcases' length, where they roll.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn sweep(&self) -> (Vec2, Vec2) {
        self.rect((self.rail, self.rail + self.length()), (-BOOKCASE.z / 2.0, BOOKCASE.z / 2.0))
    }

    /// Its two mouth strips, each `MOUTH` deep beyond the sweep's end, across the whole rail.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn mouth_strips(&self) -> [(Vec2, Vec2); 2] {
        let (rail, half) = ((self.rail, self.rail + self.length()), BOOKCASE.z / 2.0);
        [self.rect(rail, (-half - MOUTH, -half)), self.rect(rail, (half, half + MOUTH))]
    }

    /// The open Aisle at `rest`, as a rectangle on the floor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn aisle(&self, rest: u8) -> (Vec2, Vec2) {
        let lo = self.rail + BOOKCASE.x * f32::from(rest);
        self.rect((lo, lo + AISLE), (-BOOKCASE.z / 2.0, BOOKCASE.z / 2.0))
    }
}

/// Where a Range's Bookcases are at Rest `rest`: for each of `N`, how far along its rail from its leftmost place (against
/// the start of the rail) it stands, which is the Aisle's width for those the Aisle has moved past and none for the rest.
const fn rest_table<const N: usize, const R: usize>() -> [[f32; N]; R] {
    let mut out = [[0.0; N]; R];
    let mut rest = 0;
    while rest < R {
        let mut i = 0;
        while i < N {
            out[rest][i] = if i >= rest { AISLE } else { 0.0 };
            i += 1;
        }
        rest += 1;
    }
    out
}

/// A Range's Bookcases, each a Rail Leaf whose datum is where it stands leftmost along the rail.
const fn leaves<const N: usize>(r: &Range) -> [Leaf; N] {
    let blank = Leaf { hinge: Vec3::ZERO, size: Vec3::ZERO, offset: Vec3::ZERO, motion: Motion::Hinge };
    let (size, dir) = if r.along_x { (Vec3::new(BOOKCASE.x, BOOKCASE.y, BOOKCASE.z), Vec2::X) } else { (Vec3::new(BOOKCASE.z, BOOKCASE.y, BOOKCASE.x), Vec2::Y) };
    let mut out = [blank; N];
    let mut i = 0;
    while i < N {
        let centre = r.point(r.rail + BOOKCASE.x * i as f32 + BOOKCASE.x / 2.0, 0.0);
        out[i] = Leaf { hinge: Vec3::new(centre.x, r.floor, centre.y), size, offset: Vec3::new(0.0, BOOKCASE.y / 2.0, 0.0), motion: Motion::Rail(dir) };
        i += 1;
    }
    out
}

/// A Range's Rests: the Aisle at each, every one joining its Range's Zone pair.
const fn rests<const N: usize, const R: usize>(table: &'static [[f32; N]; R], joins: &'static [(ZoneId, ZoneId)]) -> [Rest; R] {
    let mut out = [Rest { at: &[], joins }; R];
    let mut rest = 0;
    while rest < R {
        out[rest] = Rest { at: &table[rest], joins };
        rest += 1;
    }
    out
}

/// A Range's Wheels, two (one at each mouth) for each of its Aisles: Rest `k`'s at `2k` (the low side) and `2k + 1`, on the
/// mouth strip in front of the Aisle's middle.
const fn wheels<const W: usize>(r: &Range) -> [Wheel; W] {
    let blank = Wheel { zone: r.zones.0, at: Vec3::ZERO, facing: Vec2::ZERO, rest: 0 };
    let mut out = [blank; W];
    let mut i = 0;
    while i < W {
        let (rest, high) = ((i / 2) as u8, i % 2 == 1);
        let side = if high { 1.0 } else { -1.0 };
        let spot = r.point(r.aisle_mid(rest), side * (BOOKCASE.z / 2.0 + WHEEL_OFF));
        let facing = if r.along_x { Vec2::new(0.0, side) } else { Vec2::new(side, 0.0) };
        out[i] = Wheel { zone: if high { r.zones.1 } else { r.zones.0 }, at: Vec3::new(spot.x, r.floor, spot.y), facing, rest };
        i += 1;
    }
    out
}

/// A Range as a Piece: its Bookcases, its Rests and its Wheels, behind its gate Door.
const fn range_piece(r: &Range, leaves: &'static [Leaf], rests: &'static [Rest], wheels: &'static [Wheel]) -> Piece {
    Piece { zone: r.zones.0, leaves, rests, start: r.start, driver: Driver::Wheels(wheels), door: Some(r.zones) }
}

const R0_AT: [[f32; 4]; 5] = rest_table();
const R0_LEAVES: [Leaf; 4] = leaves(&RANGES[0]);
const R0_RESTS: [Rest; 5] = rests(&R0_AT, &[(ZoneId::CatalogueHall, ZoneId::ReadingRoom)]);
const R0_WHEELS: [Wheel; 10] = wheels(&RANGES[0]);
const RA_AT: [[f32; 3]; 4] = rest_table();
const RA_LEAVES: [Leaf; 3] = leaves(&RANGES[1]);
const RA_RESTS: [Rest; 4] = rests(&RA_AT, &[(ZoneId::OldStacks, ZoneId::FolioStacks)]);
const RA_WHEELS: [Wheel; 8] = wheels(&RANGES[1]);
const RB_AT: [[f32; 2]; 3] = rest_table();
const RB_LEAVES: [Leaf; 2] = leaves(&RANGES[2]);
const RB_RESTS: [Rest; 3] = rests(&RB_AT, &[(ZoneId::FolioStacks, ZoneId::PeriodicalStacks)]);
const RB_WHEELS: [Wheel; 6] = wheels(&RANGES[2]);
const RC_AT: [[f32; 3]; 4] = rest_table();
const RC_LEAVES: [Leaf; 3] = leaves(&RANGES[3]);
const RC_RESTS: [Rest; 4] = rests(&RC_AT, &[(ZoneId::NewStacks, ZoneId::PeriodicalStacks)]);
const RC_WHEELS: [Wheel; 8] = wheels(&RANGES[3]);
const RD_AT: [[f32; 2]; 3] = rest_table();
const RD_LEAVES: [Leaf; 2] = leaves(&RANGES[4]);
const RD_RESTS: [Rest; 3] = rests(&RD_AT, &[(ZoneId::OldStacks, ZoneId::NewStacks)]);
const RD_WHEELS: [Wheel; 6] = wheels(&RANGES[4]);

/// The five Ranges as Shifting Pieces, in `RANGES`' order: R0, RA, RB, RC, RD (`PieceId` 0 to 4).
const SHIFTING: [Piece; 5] = [
    range_piece(&RANGES[0], &R0_LEAVES, &R0_RESTS, &R0_WHEELS),
    range_piece(&RANGES[1], &RA_LEAVES, &RA_RESTS, &RA_WHEELS),
    range_piece(&RANGES[2], &RB_LEAVES, &RB_RESTS, &RB_WHEELS),
    range_piece(&RANGES[3], &RC_LEAVES, &RC_RESTS, &RC_WHEELS),
    range_piece(&RANGES[4], &RD_LEAVES, &RD_RESTS, &RD_WHEELS),
];

/// A Range's gate: a grille along the whole of its `gate_side` face, `GATE_OFF` out from the Bookcases (closed in at both
/// ends by caps, so the Bookcases' Aisle is never bypassed at any Rest), with the Door in it across the Aisle's start Rest.
/// Its Bookcases are the Piece's Leaves (`SHIFTING`), built as bodies by `spawn_pieces`; its Wheels' posts too.
fn range(b: &mut Builder, r: &Range) {
    let face = r.gate_side * BOOKCASE.z / 2.0;
    let plane = r.gate_side * (BOOKCASE.z / 2.0 + GATE_OFF);
    let (start, end) = (r.rail, r.rail + r.length());
    b.wall(r.point(start, plane), r.point(end, plane), r.floor, DOOR_HEIGHT, &[(r.aisle_mid(r.start) - r.rail, Gap::Gate(r.zones, r.price))]);
    for along in [start, end] {
        let (p, q) = (r.point(along, face), r.point(along, plane));
        b.wall(p.min(q), p.max(q), r.floor, DOOR_HEIGHT, &[]);
    }
    rail_and_soffit(b, r);
}

/// A Range's look (#537): the iron rail flush in the floor along the whole of its length, and its brass letter on the soffit (the wall
/// over its 3 m opening) on both faces of the wall.
fn rail_and_soffit(b: &mut Builder, r: &Range) {
    let piece = RANGES.iter().position(|other| other.name == r.name).unwrap_or(0);
    let length = r.length();
    // The rail's own frame runs along the rail from where it starts, across it from the wall's line.
    let (at, turn) = if r.along_x {
        (Vec3::new(r.rail, r.floor, r.line), Quat::IDENTITY)
    } else {
        (Vec3::new(r.line, r.floor, r.rail), Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
    };
    b.look(&format!("rail {length}"), stacks_look::rail(length), at, turn);
    let mid = r.rail + length / 2.0;
    for side in [-1.0, 1.0] {
        let spot = r.point(mid, side * stacks_look::WALL_FACE);
        let out = if r.along_x { Vec3::Z } else { Vec3::X } * side;
        let letters = stacks_look::soffit(&stacks_look::soffit_label(piece), stacks_look::soffit_height(piece));
        b.look(&format!("soffit {piece}"), letters, Vec3::new(spot.x, r.floor, spot.y), stacks_look::facing(out));
    }
}

// ---------------------------------------------------------------- The per-Zone tables

/// The chalk outlines: the cheap guns either side of the Catalogue Hall's south wall, the MP40 in the Map Room, the Trench Gun
/// in the Folio Stacks for the lanes and the AK-74u in the Periodical Stacks.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    (WeaponId::M14, ZoneId::CatalogueHall, at(-4.5, READING + CHALK, 0.2), Vec2::Y),
    (WeaponId::Olympia, ZoneId::CatalogueHall, at(4.5, READING + CHALK, 0.2), Vec2::Y),
    (WeaponId::Mp40, ZoneId::MapRoom, at(-30.0, READING + CHALK, 12.2), Vec2::Y),
    (WeaponId::TrenchGun, ZoneId::FolioStacks, at(-10.0, STACKS + CHALK, 35.8), Vec2::NEG_Y),
    (WeaponId::Ak74u, ZoneId::PeriodicalStacks, at(10.0, STACKS + CHALK, 35.8), Vec2::NEG_Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first, in the Map Room, two Doors from the Catalogue Hall (G0 and
/// D2). None is in the Catalogue Hall, the Pump Room or the Strong Room, and none is in a Range's sweep.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 6] = [
    // Against the Map Room's south wall.
    (ZoneId::MapRoom, at(-26.0, READING, 12.65), Vec2::Y),
    // Against the Reading Room's west wall.
    (ZoneId::ReadingRoom, at(-19.35, READING, 14.0), Vec2::X),
    // Against the Periodical Stacks' east wall.
    (ZoneId::PeriodicalStacks, at(19.35, STACKS, 33.0), Vec2::NEG_X),
    // Against the Bindery's west wall.
    (ZoneId::Bindery, at(20.65, STACKS, 26.0), Vec2::X),
    // Against the Undercroft's east wall.
    (ZoneId::Undercroft, at(-0.65, CELLAR, 22.5), Vec2::NEG_X),
    // On the Causeway, at its west end.
    (ZoneId::FloodedCellar, at(-14.5, 0.6, 10.0), Vec2::X),
];

/// The Hellhound Spawn Points: one in every Zone, and two each in the Reading Room, the Flooded Cellar and the Undercroft.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 17] = [
    (ZoneId::CatalogueHall, at(10.0, READING, 9.6)),
    (ZoneId::ReadingRoom, at(-11.0, READING, 21.5)),
    (ZoneId::ReadingRoom, at(11.0, READING, 28.8)),
    (ZoneId::MapRoom, at(-26.0, READING, 23.5)),
    (ZoneId::LectureTheatre, at(30.5, STACKS, 19.5)),
    (ZoneId::OldStacks, at(-9.2, STACKS, 6.5)),
    (ZoneId::FolioStacks, at(-9.2, STACKS, 25.6)),
    (ZoneId::PeriodicalStacks, at(10.1, STACKS, 25.6)),
    (ZoneId::NewStacks, at(10.1, STACKS, 15.4)),
    (ZoneId::Bindery, at(27.0, STACKS, 29.5)),
    (ZoneId::WellStair, at(-27.0, STACKS, 6.5)),
    (ZoneId::FloodedCellar, at(-4.5, CELLAR, 6.8)),
    (ZoneId::FloodedCellar, at(9.0, CELLAR, 13.0)),
    (ZoneId::Undercroft, at(-12.5, CELLAR, 26.0)),
    (ZoneId::Undercroft, at(-7.5, CELLAR, 31.0)),
    (ZoneId::StrongRoom, at(8.0, CELLAR, 29.0)),
    (ZoneId::PumpRoom, at(28.0, CELLAR, 13.0)),
];

/// The Special Spawn Points, where a Napalm or a Shrieker rises: the Reading Room, the Bindery, the Flooded Cellar and the Undercroft.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 4] = [
    (ZoneId::ReadingRoom, at(11.0, READING, 21.5)),
    (ZoneId::Bindery, at(30.5, STACKS, 29.5)),
    (ZoneId::FloodedCellar, at(13.5, CELLAR, 6.8)),
    (ZoneId::Undercroft, at(-12.5, CELLAR, 31.0)),
];

/// The Wading Points: four on the bed of the Flooded Cellar, standing in for Risers.
const WADING_POINTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::FloodedCellar, at(-13.5, CELLAR, 6.8)),
    (ZoneId::FloodedCellar, at(-4.5, CELLAR, 13.0)),
    (ZoneId::FloodedCellar, at(4.5, CELLAR, 6.8)),
    (ZoneId::FloodedCellar, at(13.5, CELLAR, 13.0)),
];

/// The Vents: in the Reading Room, the Lecture Theatre's dais, the Folio Stacks and the New Stacks, each over the floor spot named.
const VENTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::ReadingRoom, at(-11.0, READING, 28.8)),
    (ZoneId::LectureTheatre, at(26.5, STACKS, 19.5)),
    (ZoneId::FolioStacks, at(-13.4, STACKS, 25.6)),
    (ZoneId::NewStacks, at(14.3, STACKS, 15.4)),
];

/// Wood and iron, for the pieces that aren't a Zone's wall.
const TIMBER: Color = Color::srgb(0.28, 0.2, 0.13);
const IRON: Color = Color::srgb(0.13, 0.14, 0.15);
const STONE: Color = Color::srgb(0.46, 0.46, 0.43);

/// The Lecture Theatre's top crossover (z 0 to 4, up at the Reading level) and the Causeway across the Flooded Cellar's pool (3 m wide, 0.6 m
/// over the bed, with steps at both ends, built in `build`).
const CROSSOVER: RaisedFloor = RaisedFloor { zone: ZoneId::LectureTheatre, min: v(20.0, 0.0), max: v(36.0, 4.0), height: READING, colour: TIMBER };
const CAUSEWAY: RaisedFloor = RaisedFloor { zone: ZoneId::FloodedCellar, min: v(-16.0, 8.5), max: v(16.0, 11.5), height: 0.6, colour: STONE };
const RAISED_FLOORS: [RaisedFloor; 2] = [CROSSOVER, CAUSEWAY];

/// The stairs: the Theatre's two aisles (7 tiers' worth, raking from the dais up to the crossover), the Map Stair, the Bindery Stair
/// and the Well Stair, in their wells (`data::SUNKEN`). The Causeway's steps are built in `build`.
const SLOPES: [Slope; 5] = [
    Slope { zone: ZoneId::LectureTheatre, foot: at(21.5, STACKS, 18.0), up: Vec2::NEG_Y, run: 14.0, rise: STOREY, width: 3.0, steps: true, colour: STONE },
    Slope { zone: ZoneId::LectureTheatre, foot: at(34.5, STACKS, 18.0), up: Vec2::NEG_Y, run: 14.0, rise: STOREY, width: 3.0, steps: true, colour: STONE },
    // The Map Stair: down along the Map Room's north wall, climbing west to the Map Room's floor.
    Slope { zone: ZoneId::MapRoom, foot: at(-27.2, STACKS, 34.5), up: Vec2::NEG_X, run: 6.8, rise: STOREY, width: 2.8, steps: true, colour: STONE },
    // The Bindery Stair: from D8 up north onto the Bindery's floor.
    Slope { zone: ZoneId::Bindery, foot: at(34.2, CELLAR, 25.4), up: Vec2::Y, run: 6.8, rise: STOREY, width: 3.0, steps: true, colour: IRON },
    // The Well Stair: from the Flooded Cellar's arch up north to the landing by D6.
    Slope { zone: ZoneId::WellStair, foot: at(-22.0, CELLAR, 2.6), up: Vec2::Y, run: 6.4, rise: STOREY, width: 3.4, steps: true, colour: STONE },
];

/// The pool: Still water, its surface at `0.5` over the bed at the floor under it (the Cellar's `Sunken`), over the whole Zone, so it is
/// Shallow everywhere and Deep nowhere.
const POOLS: [Pool; 1] = [Pool { zone: ZoneId::FloodedCellar, min: v(-20.0, 0.0), max: v(20.0, 20.0), level: Level::Still(0.5), bed: 0.0 }];

/// The Props built so far: the fixed stacks of the ring halls (`Stack` Props, drawn by `stacks_look`), the Training loops' centres (the first
/// four, the card-catalogue banks, the issue desk and the theatre's tiers, wear their Zone's variants; the cellar's piers and columns are still
/// boxes) and the reading level's Props (#540). New ones go last, so the others keep their `nth`.
const PROPS: [Prop; 58] = concat::<_, 29, 29, 58>(concat::<_, 8, 21, 29>(data::FIXED_STACKS, data::LOOP_CENTRES), data::READING_PROPS);

// ---------------------------------------------------------------- The book-lift core

/// The book-lift core: a 6 x 6 m shaft at x -3..3, z 17..23 that runs from the Strong Room to the Stacks level as a quarter Infill in each
/// Zone it crosses, and as a half or a cut-back pit in the cellar's three (#515 section 7).
pub(super) const CORE: (Vec2, Vec2) = (Vec2::new(-3.0, 17.0), Vec2::new(3.0, 23.0));

/// Whether an Infill is part of the core.
pub(super) fn is_core(infill: &Infill) -> bool {
    infill.min.cmpge(CORE.0).all() && infill.max.cmple(CORE.1).all()
}

/// The lattice's flats: how wide and how far proud of the face they stand, how far apart the uprights are, and how far over the floor and
/// under the cornice they run.
const FLAT_WIDTH: f32 = 0.04;
const FLAT_PROUD: f32 = 0.02;
const UPRIGHT_PITCH: f32 = 0.5;
const LATTICE_FOOT: f32 = 0.15;
const LATTICE_HEAD: f32 = 0.35;
const FLAT_IRON: Color = Color::srgb(0.02, 0.02, 0.025);

/// An iron lattice of black flats on each face of a core quarter that stands free of its Zone's walls: uprights at a half metre and three
/// cross-bars, drawn only (no collider), so what stands in the Infill's face is what the Infill is.
fn lift_lattice(b: &mut Builder, infill: &Infill) {
    let (floor, top) = infill.span();
    let (lo, hi) = infill.zone.volume();
    let (bottom, ceiling) = (floor + LATTICE_FOOT, top - LATTICE_HEAD);
    let height = ceiling - bottom;
    // Each face: where it lies across its normal, the normal's sign, which way it runs, its run, and the Zone's wall on that side.
    let faces = [
        (infill.min.x, -1.0, false, infill.min.y, infill.max.y, lo.x),
        (infill.max.x, 1.0, false, infill.min.y, infill.max.y, hi.x),
        (infill.min.y, -1.0, true, infill.min.x, infill.max.x, lo.z),
        (infill.max.y, 1.0, true, infill.min.x, infill.max.x, hi.z),
    ];
    for (across, sign, along_x, start, end, wall) in faces {
        // A face on the Zone's wall is inside the wall.
        if (across - wall).abs() < 0.05 {
            continue;
        }
        let out = across + sign * FLAT_PROUD / 2.0;
        let place = |along: f32, y: f32, run: f32, tall: f32| -> (Vec3, Vec3) {
            if along_x {
                (Vec3::new(run, tall, FLAT_PROUD), Vec3::new(along, y, out))
            } else {
                (Vec3::new(FLAT_PROUD, tall, run), Vec3::new(out, y, along))
            }
        };
        let uprights = ((end - start) / UPRIGHT_PITCH).round().max(1.0) as u32;
        for i in 0..=uprights {
            // The two end uprights sit just inside the face's ends.
            let along = (start + (end - start) * i as f32 / uprights as f32).clamp(start + FLAT_WIDTH / 2.0, end - FLAT_WIDTH / 2.0);
            let (size, at) = place(along, (bottom + ceiling) / 2.0, FLAT_WIDTH, height);
            b.shape(size, at, Quat::IDENTITY, FLAT_IRON);
        }
        for k in 1..=3 {
            let (size, at) = place((start + end) / 2.0, bottom + height * k as f32 / 4.0, end - start, FLAT_WIDTH);
            b.shape(size, at, Quat::IDENTITY, FLAT_IRON);
        }
    }
}

// ---------------------------------------------------------------- The walls

/// A stretch of wall along x at `z`, from `x0` to `x1`, with gaps at absolute `x`.
fn xwall(b: &mut Builder, (x0, x1): (f32, f32), z: f32, y0: f32, height: f32, gaps: Vec<(f32, Gap)>) {
    b.wall(v(x0, z), v(x1, z), y0, height, &gaps.into_iter().map(|(x, gap)| (x - x0, gap)).collect::<Vec<_>>());
}

/// A stretch of wall along z at `x`, from `z0` to `z1`, with gaps at absolute `z`.
fn zwall(b: &mut Builder, x: f32, (z0, z1): (f32, f32), y0: f32, height: f32, gaps: Vec<(f32, Gap)>) {
    b.wall(v(x, z0), v(x, z1), y0, height, &gaps.into_iter().map(|(z, gap)| (z - z0, gap)).collect::<Vec<_>>());
}

/// The Power Door.
fn pdoor(a: ZoneId, b: ZoneId) -> Gap {
    Gap::Door(Some((a, b)), None)
}

/// A doorway 2.4 m wide to the floor: an archway.
fn arch() -> Gap {
    Gap::Open { width: 2.4, height: DOOR_HEIGHT }
}

/// A Range's wall opening, `length` wide and 3 m tall to the soffit.
fn opening(length: f32) -> Gap {
    Gap::Open { width: length, height: BOOKCASE.y }
}

/// The back-of-house behind the window at `centre` (looking `out` toward the wall): its hatch wall, `PEN_HALF` either side of the window, and
/// two walls back to the wall itself, `PEN` deep.
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

/// The walls, Doors, balustrades, pens, Ranges and the Causeway of the Archive.
fn build(b: &mut Builder) {
    use ZoneId::*;
    // The outer walls: the plan's four sides, each as tall as the rooms it closes in.
    xwall(b, (-36.0, -20.0), 0.0, CELLAR, 2.0 * STOREY, vec![]);
    xwall(b, (-20.0, 36.0), 0.0, CELLAR, 3.0 * STOREY, vec![]);
    xwall(b, (-36.0, -20.0), 36.0, CELLAR, 3.0 * STOREY, vec![]);
    xwall(b, (-20.0, 20.0), 36.0, CELLAR, 4.0 * STOREY, vec![]);
    xwall(b, (20.0, 36.0), 36.0, CELLAR, 2.0 * STOREY, vec![]);
    zwall(b, -36.0, (0.0, 12.0), CELLAR, 2.0 * STOREY, vec![]);
    zwall(b, -36.0, (12.0, 36.0), CELLAR, 3.0 * STOREY, vec![]);
    zwall(b, 36.0, (0.0, 24.0), CELLAR, 3.0 * STOREY, vec![]);
    zwall(b, 36.0, (24.0, 36.0), CELLAR, 2.0 * STOREY, vec![]);

    // x = -20: the Cellar level (with the Well Stair's archway into the Flooded Cellar), the Stacks level (D6 up the Well Stair's landing and
    // D3 at the Map Stair's foot) and the Reading level (D2 between the Reading Room and the Map Room).
    zwall(b, -20.0, (0.0, 36.0), CELLAR, STOREY, vec![(1.3, arch())]);
    zwall(b, -20.0, (0.0, 36.0), STACKS, STOREY, vec![(10.4, door(WellStair, OldStacks, D6)), (34.5, door(MapRoom, FolioStacks, D3))]);
    zwall(b, -20.0, (0.0, 12.0), READING, STOREY, vec![]);
    zwall(b, -20.0, (12.0, 36.0), READING, 2.0 * STOREY, vec![(24.0, door(MapRoom, ReadingRoom, D2))]);

    // x = 0: the ring's wall between the Old and the New Stacks and the Folio and the Periodical, with RD's and RB's openings, and the
    // Undercroft's wall with the Strong Room.
    zwall(b, 0.0, (4.0, 36.0), STACKS, STOREY, vec![(10.5, opening(5.4)), (29.5, opening(5.4))]);
    zwall(b, 0.0, (20.0, 36.0), CELLAR, STOREY, vec![]);

    // x = 20: the Pump Room's D9 to the Flooded Cellar, D5 between the Periodical Stacks and the Bindery, D1 from the Catalogue Hall to the
    // Theatre's crossover and the Reading Room's east wall.
    zwall(b, 20.0, (0.0, 36.0), CELLAR, STOREY, vec![(10.0, door(FloodedCellar, PumpRoom, D9))]);
    zwall(b, 20.0, (0.0, 36.0), STACKS, STOREY, vec![(30.0, door(PeriodicalStacks, Bindery, D5))]);
    zwall(b, 20.0, (0.0, 12.0), READING, STOREY, vec![(2.0, door(CatalogueHall, LectureTheatre, D1))]);
    zwall(b, 20.0, (12.0, 36.0), READING, 2.0 * STOREY, vec![]);

    // z = 4: the Stacks level's south wall, over the hoist passage above the Cellar and under the Catalogue Hall.
    xwall(b, (-20.0, 20.0), 4.0, STACKS, STOREY, vec![]);
    // z = 12: the Map Room's south wall, and the Reading level's wall with the Great Range's opening.
    xwall(b, (-36.0, -20.0), 12.0, CELLAR, 3.0 * STOREY, vec![]);
    xwall(b, (-20.0, 20.0), 12.0, READING, 2.0 * STOREY, vec![(0.0, opening(7.8))]);
    // z = 20: the ring's wall with RA's and RC's openings, and the Cellar's with D10 and the Power Door.
    xwall(b, (-20.0, 20.0), 20.0, STACKS, STOREY, vec![(-11.0, opening(6.6)), (11.0, opening(6.6))]);
    xwall(b, (-20.0, 20.0), 20.0, CELLAR, STOREY, vec![(-10.0, door(FloodedCellar, Undercroft, D10)), (10.0, pdoor(FloodedCellar, StrongRoom))]);
    // z = 24: between the Pump Room and the Bindery (D8), the Theatre's dais and the Bindery (D4) and the Theatre's high wall.
    xwall(b, (20.0, 36.0), 24.0, CELLAR, STOREY, vec![(34.2, door(PumpRoom, Bindery, D8))]);
    xwall(b, (20.0, 36.0), 24.0, STACKS, STOREY, vec![(26.0, door(LectureTheatre, Bindery, D4))]);
    xwall(b, (20.0, 36.0), 24.0, READING, STOREY, vec![]);

    // Balustrades round the open stairwells: along the Map Stair's south edge, the Bindery Stair's west and the Well Stair's west.
    let trim = palette(Some(MapRoom)).trim;
    b.block(Vec3::new(14.0, RAIL, 0.2), Vec3::new(-27.0, READING + RAIL / 2.0, 32.9), trim);
    b.block(Vec3::new(0.2, RAIL, 8.2), Vec3::new(32.3, STACKS + RAIL / 2.0, 28.1), palette(Some(Bindery)).trim);
    b.block(Vec3::new(0.2, RAIL, 9.0), Vec3::new(-24.1, STACKS + RAIL / 2.0, 4.5), palette(Some(WellStair)).trim);

    // The Ranges: their walls (above), their Bookcases and their gates.
    for r in &RANGES {
        range(b, r);
    }

    // The book-lift core's iron lattice: black flats on the faces of its quarter Infills that stand free of a wall.
    for infill in data::INFILLS.iter().filter(|infill| is_core(infill)) {
        lift_lattice(b, infill);
    }

    // A pen behind every window, as `WINDOWS` has them.
    for &(_, centre, out) in &data::WINDOWS {
        pen(b, centre, out);
    }

    // Under each of the Theatre's aisles, a wedge of tiers, so nobody (and nothing) stands in the void under the rake.
    let wedge = palette(Some(LectureTheatre)).floor;
    for x in [21.5, 34.5] {
        for k in 0..7 {
            let (z0, z1) = (18.0 - 2.0 * (k as f32 + 1.0), 18.0 - 2.0 * k as f32);
            let top = STACKS + (18.0 - z1) * STOREY / 14.0;
            if top > STACKS + 0.05 {
                b.block(Vec3::new(3.0, top - STACKS, 2.0), Vec3::new(x, (STACKS + top) / 2.0, (z0 + z1) / 2.0), wedge);
            }
        }
    }

    // The tide mark round the Flooded Cellar's walls, piers and the Causeway's sides (#539).
    flood::tide_mark(b, super::MapId::Archive.layout());

    // The Causeway: the Raised Floor's block floats over the pool's bed, so its core runs down to the bed, and its steps climb at both ends.
    b.block(Vec3::new(32.0, 0.5, 3.0), Vec3::new(0.0, 0.25, 10.0), STONE);
    for sign in [-1.0, 1.0] {
        for i in 1..=3 {
            let run = 0.45 * (4 - i) as f32;
            let height = 0.15 * i as f32;
            b.block(Vec3::new(run + 0.0, height, 3.0), Vec3::new(sign * (16.0 + run / 2.0), height / 2.0, 10.0), STONE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::nav::{Gate, NavGraph};
    use crate::maps::map::MapId;
    use crate::maps::map::PropKind;
    use crate::maps::map::door_look;
    use crate::maps::map::textures::{Material, Textured};
    use ZoneId::*;

    fn archive() -> &'static Layout {
        MapId::Archive.layout()
    }

    /// The five Ranges' gate Doors' joins.
    fn gate_joins() -> Vec<(ZoneId, ZoneId)> {
        RANGES.iter().map(|r| r.zones).collect()
    }

    #[test]
    fn fourteen_zones_starting_in_the_catalogue_hall_centred_on_the_origin_and_all_indoors() {
        assert_eq!(archive().zones.len(), 14);
        assert_eq!(archive().start_zone(), CatalogueHall);
        assert_eq!(archive().plan, (Vec2::new(-36.0, 0.0), Vec2::new(36.0, 36.0)));
        assert!(archive().outdoor.is_empty() && archive().roofed && archive().outside.is_none());
        assert!(archive().sections.is_empty() && archive().generators.is_empty() && !archive().darkness);
        assert_eq!(archive().shifting.len(), 5);
        assert!(archive().wings.is_empty());
        for zone in [CatalogueHall, ReadingRoom, MapRoom] {
            assert_eq!(zone.floor(), READING, "{}", zone.name());
        }
        for zone in [LectureTheatre, OldStacks, FolioStacks, PeriodicalStacks, NewStacks, Bindery, WellStair] {
            assert_eq!(zone.floor(), STACKS, "{}", zone.name());
        }
        for zone in [Undercroft, StrongRoom, PumpRoom] {
            assert_eq!(zone.floor(), CELLAR, "{}", zone.name());
        }
    }

    /// No two of the Archive's boxes overlap, on one level or across levels (the Map Room, the Bindery and the Well Stair reach down
    /// into their stairwells, the Theatre up its tiers, and the levels meet at the floors).
    #[test]
    fn archive_zone_boxes_do_not_overlap_across_levels() {
        for (i, &a) in archive().zones.iter().enumerate() {
            for &b in &archive().zones[i + 1..] {
                let ((alo, ahi), (blo, bhi)) = (a.volume(), b.volume());
                let overlap = alo.cmplt(bhi).all() && blo.cmplt(ahi).all();
                assert!(!overlap, "{} overlaps {}", a.name(), b.name());
            }
        }
        // The stairwells' Zones reach down to where their stairs end.
        assert_eq!(MapRoom.volume().0.y, STACKS - 0.25);
        assert_eq!(Bindery.volume().0.y, CELLAR - 0.25);
        assert_eq!(WellStair.volume().0.y, CELLAR - 0.25);
        assert_eq!(FloodedCellar.volume().0.y, CELLAR - 0.25);
        // The Lecture Theatre is the Reading level's height over its dais.
        assert_eq!(LectureTheatre.volume().1.y, 3.0 * STOREY);
    }

    /// The Doors a Run can buy, with their joins: 14 bought and the one Power Door.
    fn doors(sim: &mut crate::enemies::zombie::headless::Sim) -> Vec<(Option<(ZoneId, ZoneId)>, Option<u64>)> {
        use bevy::ecs::system::RunSystemOnce;

        use crate::maps::door::Door;
        sim.world_mut()
            .run_system_once(|doors: Query<&Door>| doors.iter().map(|door| (door.route(), door.price)).collect::<Vec<_>>())
            .unwrap()
    }

    /// The cheapest price of Doors from the Catalogue Hall to each Zone, bought one by one, over the Doors that `allowed` lets through.
    fn door_costs(doors: &[(Option<(ZoneId, ZoneId)>, Option<u64>)], allowed: impl Fn((ZoneId, ZoneId)) -> bool) -> std::collections::HashMap<ZoneId, u64> {
        let mut cost = std::collections::HashMap::from([(CatalogueHall, 0u64)]);
        for _ in 0..20 {
            for &(joins, price) in doors {
                let (Some((a, b)), Some(price)) = (joins, price) else { continue };
                if !allowed((a, b)) {
                    continue;
                }
                for (from, to) in [(a, b), (b, a)] {
                    let Some(&here) = cost.get(&from) else { continue };
                    let best = cost.entry(to).or_insert(u64::MAX);
                    *best = (*best).min(here + price);
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
        }
        cost
    }

    fn is_a_gate((a, b): (ZoneId, ZoneId)) -> bool {
        gate_joins().iter().any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a))
    }

    /// 15 Doors: 14 bought (G0, D1 to D6, D8 to D10, GA to GD) and the Power Door PD. The Power costs 3000 by the east route (D1, D4 and D8)
    /// with no Range on the way, and no Range makes it any cheaper.
    #[test]
    fn archive_power_costs_3000_by_the_east_route_with_no_range() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let doors = doors(&mut sim);
        assert_eq!(doors.len(), 15);
        assert_eq!(doors.iter().filter(|(_, price)| price.is_none()).count(), 1, "one Power Door");
        let prices = |wanted: &[(ZoneId, ZoneId)]| -> u64 {
            wanted
                .iter()
                .map(|&(a, b)| doors.iter().find(|(joins, _)| matches!(joins, Some((x, y)) if (*x, *y) == (a, b) || (*x, *y) == (b, a))).and_then(|d| d.1).unwrap())
                .sum()
        };
        assert_eq!(prices(&[(CatalogueHall, LectureTheatre), (LectureTheatre, Bindery), (Bindery, PumpRoom)]), 3000);
        assert_eq!(door_costs(&doors, |joins| !is_a_gate(joins))[&PumpRoom], 3000, "the east route has no Range on it");
        assert_eq!(door_costs(&doors, |_| true)[&PumpRoom], 3000, "no Range is a cheaper way to the Power");
    }

    /// Pack-a-Punch costs 4000: the Power route and D9, with the Power Door PD opening on the Power.
    #[test]
    fn archive_pack_a_punch_costs_4000_and_pd_opens_on_the_power() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let doors = doors(&mut sim);
        let costs = door_costs(&doors, |_| true);
        assert_eq!(costs[&FloodedCellar], 4000, "D1, D4, D8 and D9");
        let power_door = doors.iter().find(|(_, price)| price.is_none()).unwrap().0.unwrap();
        assert_eq!(power_door, (FloodedCellar, StrongRoom), "the Power Door leads from the Flooded Cellar into the Strong Room");
        assert_eq!(crate::weapons::pack_a_punch::machine_at(MapId::Archive), at(4.0, CELLAR, 21.0));
        let zone = crate::enemies::nav::zone_at(MapId::Archive, crate::weapons::pack_a_punch::machine_at(MapId::Archive) + Vec3::Y * 0.1);
        assert_eq!(zone, Some(StrongRoom));
        let switch = crate::maps::power::switch_spot(MapId::Archive).expect("the Archive has a Power Switch").0;
        assert_eq!(crate::enemies::nav::zone_at(MapId::Archive, switch), Some(PumpRoom));
    }

    /// The Power Switch and Pack-a-Punch are reachable with every Range gate unbought: by the Doors that aren't Range gates (and the
    /// Power Door, once the Power is on) and the fixed joins alone.
    #[test]
    fn archive_power_and_pack_a_punch_are_reachable_with_every_range_gate_unbought() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let doors = doors(&mut sim);
        let mut seen = std::collections::HashSet::from([CatalogueHall]);
        loop {
            let before = seen.len();
            for &(joins, _) in &doors {
                let Some((a, b)) = joins else { continue };
                if is_a_gate((a, b)) {
                    continue;
                }
                if seen.contains(&a) || seen.contains(&b) {
                    seen.insert(a);
                    seen.insert(b);
                }
            }
            for &(a, b) in &FIXED_JOINS {
                if seen.contains(&a) || seen.contains(&b) {
                    seen.insert(a);
                    seen.insert(b);
                }
            }
            if seen.len() == before {
                break;
            }
        }
        assert!(seen.contains(&PumpRoom), "the Power Switch's Pump Room");
        assert!(seen.contains(&StrongRoom), "Pack-a-Punch's Strong Room");
        // And by nav, with every Door but the Range gates bought.
        let graph = NavGraph::new(MapId::Archive);
        let yes = |_: crate::maps::map::BarrierId| true;
        let bought: Vec<(ZoneId, ZoneId)> = doors.iter().filter_map(|&(joins, _)| joins).filter(|&joins| !is_a_gate(joins)).collect();
        let state = crate::enemies::nav::NavState::new(&bought, &yes);
        let start = graph.nearest(archive().seat_start(0));
        for (what, at) in [("the Power Switch", crate::maps::power::switch_spot(MapId::Archive).unwrap().0), ("Pack-a-Punch", crate::weapons::pack_a_punch::machine_at(MapId::Archive))] {
            let near = graph.nearest(Vec3::new(at.x, CELLAR, at.z) + Vec3::Y * 0.1);
            assert!(graph.plan(near, start, state).is_some(), "{what} is cut off with every Range gate unbought");
        }
    }

    #[test]
    fn archive_pool_is_shallow_everywhere() {
        use crate::maps::water::{TideLevel, Wading, water_at};
        let layout = archive();
        assert_eq!(layout.water.len(), 1, "one Still Pool");
        let pool = layout.water[0];
        assert_eq!((pool.zone, pool.bed, pool.level), (FloodedCellar, 0.0, Level::Still(0.5)));
        assert!(pool.depth(layout.canals, TideLevel::Low) < crate::tuning::DEEP_WATER_DEPTH);
        for i in 0..=20 {
            for j in 0..=10 {
                let at = Vec3::new(pool.min.x + (pool.max.x - pool.min.x) * i as f32 / 20.0, 0.0, pool.min.y + (pool.max.y - pool.min.y) * j as f32 / 10.0);
                let in_bounds = at.x > pool.min.x && at.x < pool.max.x && at.z > pool.min.y && at.z < pool.max.y;
                if in_bounds && !(super::CAUSEWAY.min.x..=super::CAUSEWAY.max.x).contains(&at.x) | !(super::CAUSEWAY.min.y..=super::CAUSEWAY.max.y).contains(&at.z) {
                    assert_eq!(water_at(layout, TideLevel::Low, at), Some(Wading::Shallow), "{at}");
                }
            }
        }
        for tide in [TideLevel::Low, TideLevel::Mid, TideLevel::High] {
            assert!(layout.wading.iter().all(|&(_, spot)| water_at(layout, tide, spot) == Some(Wading::Shallow)));
        }
        assert_eq!(layout.wading.len(), 4);
    }

    /// The Causeway: a 3 m wide Raised Floor 0.6 m up, with its own path nodes along it and a flight of steps to each end.
    #[test]
    fn archive_causeway_has_path_nodes_and_steps() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let causeway = archive().raised_floors.iter().find(|raised| raised.zone == FloodedCellar).expect("a Causeway");
        assert_eq!((causeway.height, causeway.max.y - causeway.min.y, causeway.max.x - causeway.min.x), (0.6, 3.0, 32.0));
        let on: Vec<&(&str, f32, f32, f32)> =
            archive().nav_nodes.iter().filter(|&&(_, x, y, z)| y == 0.6 && x >= causeway.min.x && x <= causeway.max.x && z > causeway.min.y && z < causeway.max.y).collect();
        assert!(on.len() >= 6, "{} path nodes on the Causeway", on.len());
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let heights = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<f32> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                // Along the east end's steps, from the bed to the Causeway's top.
                [17.6, 17.1, 16.65, 16.2, 15.0]
                    .into_iter()
                    .map(|x| {
                        let hit = spatial.cast_ray(Vec3::new(x, 2.0, 10.0), Dir3::NEG_Y, 3.0, true, &filter).expect("a floor");
                        ((2.0 - hit.distance) * 100.0).round() / 100.0
                    })
                    .collect()
            })
            .unwrap();
        // The bed, three treads of 0.15 and the Causeway's top.
        assert_eq!(heights, [0.0, 0.15, 0.3, 0.45, 0.6]);
    }

    /// A path node, Spawn Point, Buyable, Box Location, Barrier, lamp or chalk outline in a Range's sweep or on either mouth strip. The Range's
    /// own chain of nodes and its gate Door are the exceptions.
    fn in_a_range(at: Vec2, floor: f32) -> Option<&'static str> {
        RANGES.iter().find(|r| {
            let inside = |(lo, hi): (Vec2, Vec2)| at.x > lo.x && at.x < hi.x && at.y > lo.y && at.y < hi.y;
            (floor - r.floor).abs() < 1.0 && (inside(r.sweep()) || r.mouth_strips().into_iter().any(inside))
        }).map(|r| r.name)
    }

    #[test]
    fn nothing_else_stands_in_a_range_sweep() {
        use crate::maps::map::{BOX_LOCATIONS, window_spawn};
        use crate::weapons::equipment::equipment_walls;
        let layout = archive();
        let mut things: Vec<(String, Vec3)> = Vec::new();
        for barrier in MapId::Archive.barriers() {
            things.push((format!("window {}'s Spawn Point", barrier.0 - W0), window_spawn(barrier)));
            things.push((format!("window {}", barrier.0 - W0), crate::maps::map::window_centre(barrier)));
        }
        for &(zone, at) in layout.hellhound_spawns.iter().chain(layout.special_spawns).chain(layout.wading).chain(layout.vents) {
            things.push((format!("a Spawn Point in the {}", zone.name()), at));
        }
        for i in MapId::Archive.box_locations() {
            things.push(("a Box Location".into(), BOX_LOCATIONS[i].1));
        }
        for &(weapon, _, at, _) in layout.wall_weapons {
            things.push((format!("the {weapon:?} wall"), at));
        }
        for &(_, at, _) in crate::maps::trap::traps(MapId::Archive) {
            things.push(("a Trap".into(), at));
        }
        for at in crate::maps::trap::switch_spots(MapId::Archive) {
            things.push(("a Trap switch".into(), at));
        }
        for perk in crate::economy::perk::Perk::ALL {
            things.push((format!("{perk:?}'s machine"), perk.machine_spot(MapId::Archive).0));
        }
        for &(wall, _, at, _) in equipment_walls(MapId::Archive) {
            things.push((format!("{wall:?}"), at));
        }
        things.push(("Pack-a-Punch".into(), crate::weapons::pack_a_punch::machine_at(MapId::Archive)));
        things.push(("the Power Switch".into(), crate::maps::power::switch_spot(MapId::Archive).unwrap().0));
        // Each gate Door, a Buyable in its grille (the Wheels' are checked against the sweeps alone: they stand in the mouth strips).
        for r in &RANGES {
            let door = r.point(r.aisle_mid(r.start), r.gate_side * (BOOKCASE.z / 2.0 + GATE_OFF));
            things.push((format!("{}'s gate Door", r.name), Vec3::new(door.x, r.floor, door.y)));
        }
        for &(zone, at) in layout.lamps {
            things.push((format!("a lamp in the {}", zone.name()), Vec3::new(at.x, zone.floor(), at.z)));
        }
        for (what, at) in &things {
            assert_eq!(in_a_range(Vec2::new(at.x, at.z), at.y), None, "{what} at {at} is in a Range's sweep or mouth strip");
        }
        // Path nodes: only each Range's own chain (named for it) may be there, and the nodes round its gate.
        for &(name, x, y, z) in layout.nav_nodes {
            if let Some(range) = in_a_range(Vec2::new(x, z), y) {
                assert!(name.starts_with(range) && name[range.len()..].starts_with('_'), "path node {name} is in {range}'s sweep or mouth strip");
            }
        }
        // And no Prop's box is.
        for prop in layout.props {
            let (lo, hi) = (Vec2::new(prop.at.x, prop.at.z) - Vec2::new(prop.size.x, prop.size.z) / 2.0, Vec2::new(prop.at.x, prop.at.z) + Vec2::new(prop.size.x, prop.size.z) / 2.0);
            for r in &RANGES {
                let (slo, shi) = r.sweep();
                let regions = [(slo, shi), r.mouth_strips()[0], r.mouth_strips()[1]];
                if (prop.at.y - r.floor).abs() > 1.0 {
                    continue;
                }
                for (rlo, rhi) in regions {
                    let overlap = lo.x < rhi.x && rlo.x < hi.x && lo.y < rhi.y && rlo.y < hi.y;
                    assert!(!overlap, "a {:?} Prop of the {} is in {}'s sweep or mouth strip", prop.kind, prop.zone.name(), r.name);
                }
            }
        }
        // Each Range's open Aisle at its start Rest is clear, 3 m wide, and its Bookcases fill the rest of its sweep.
        for r in &RANGES {
            assert!(r.start <= r.bookcases);
            let (alo, ahi) = r.aisle(r.start);
            let gap = if r.along_x { ahi.x - alo.x } else { ahi.y - alo.y };
            assert!((gap - AISLE).abs() < 1e-4, "{}", r.name);
            assert!((r.length() - (f32::from(r.bookcases) * BOOKCASE.x + AISLE)).abs() < 1e-4);
        }
    }

    #[test]
    fn no_archive_lamp_hangs_over_a_fixed_stack_or_in_a_sweep() {
        for &(zone, at) in archive().lamps {
            assert_eq!(in_a_range(Vec2::new(at.x, at.z), zone.floor()), None, "a lamp in the {} at {at} is in a Range's sweep or mouth strip", zone.name());
            for stack in data::FIXED_STACKS {
                let (lo, hi) = (Vec2::new(stack.at.x, stack.at.z) - Vec2::new(stack.size.x, stack.size.z) / 2.0, Vec2::new(stack.at.x, stack.at.z) + Vec2::new(stack.size.x, stack.size.z) / 2.0);
                let over = at.x > lo.x && at.x < hi.x && at.z > lo.y && at.z < hi.y && (stack.at.y - zone.floor()).abs() < 1.0;
                assert!(!over, "a lamp in the {} at {at} hangs over a fixed stack at {}", zone.name(), stack.at);
            }
            // Hung 0.15 m under the ceiling, or 2.5 m down in the Reading Room's 6 m.
            let (_, hi) = zone.volume();
            if zone != ReadingRoom && hi.y - at.y > 0.5 {
                assert!((at.y - zone.floor()).rem_euclid(STOREY) > STOREY - 0.2, "a lamp in the {} at {at}", zone.name());
            }
        }
        assert_eq!(archive().lamps.iter().filter(|(zone, _)| *zone == ReadingRoom).map(|(_, at)| at.y).fold(0.0, f32::max), 11.5);
    }

    #[test]
    fn every_archive_range_has_a_rail_covering_its_whole_length() {
        use stacks_look::{Part, RAIL_EXTRA, RAIL_GAUGE};
        for r in &RANGES {
            let items = stacks_look::rail(r.length());
            // The strips reach from before the rail's start to after its end, and sit in the rail's gauge across the Bookcases'
            // length, well inside the sweep's width.
            let strips: Vec<_> = items.iter().filter(|item| item.part == Part::Rail).collect();
            assert_eq!(strips.len(), 2, "{}", r.name);
            for strip in strips {
                let (from, to) = (r.rail + strip.at.x - strip.size.x / 2.0, r.rail + strip.at.x + strip.size.x / 2.0);
                assert!(from <= r.rail - RAIL_EXTRA / 2.0 + 1e-4 && to >= r.rail + r.length() + RAIL_EXTRA / 2.0 - 1e-4, "{}: {from} to {to}", r.name);
                assert!(strip.at.z.abs() + strip.size.z / 2.0 < BOOKCASE.z / 2.0 && (strip.at.z.abs() - RAIL_GAUGE / 2.0).abs() < 1e-5, "{}", r.name);
            }
            // A stop block at each end, so a Bookcase never leaves it.
            let stops: Vec<f32> = items.iter().filter(|item| item.part == Part::Stop).map(|item| r.rail + item.at.x).collect();
            assert_eq!(stops, [r.rail, r.rail + r.length()], "{}", r.name);
            // The Bookcases' own rail travel is the rail's length: every Bookcase at every Rest is over the strips.
            for rest in SHIFTING[RANGES.iter().position(|other| other.name == r.name).unwrap()].rests {
                for (leaf, at) in SHIFTING[RANGES.iter().position(|other| other.name == r.name).unwrap()].leaves.iter().zip(rest.at) {
                    let along = if r.along_x { leaf.hinge.x + at } else { leaf.hinge.z + at };
                    assert!(along - BOOKCASE.x / 2.0 >= r.rail - 1e-4 && along + BOOKCASE.x / 2.0 <= r.rail + r.length() + 1e-4, "{} Bookcase at {along}", r.name);
                }
            }
        }
        // And each is built, at its Range's start, in the world.
        use bevy::ecs::system::RunSystemOnce;
        let mut sim = crate::enemies::zombie::headless::Sim::on(MapId::Archive, 60.0, |_| {});
        let rails: Vec<Vec3> = sim
            .world_mut()
            .run_system_once(|looks: Query<(&Name, &Transform, &Children)>, parts: Query<&Textured>| {
                looks
                    .iter()
                    .filter(|(name, _, children)| name.as_str() == "Stack look" && children.iter().any(|child| parts.get(child).is_ok_and(|p| p.material == Material::RustyMetal02)))
                    .map(|(_, transform, _)| transform.translation)
                    .collect::<Vec<_>>()
            })
            .unwrap();
        for r in &RANGES {
            let start = if r.along_x { Vec3::new(r.rail, r.floor, r.line) } else { Vec3::new(r.line, r.floor, r.rail) };
            assert!(rails.iter().any(|at| at.distance(start) < 1e-3), "{} has no rail built at {start}", r.name);
        }
        assert_eq!(rails.len(), 5);
    }

    #[test]
    fn range_gate_doors_use_the_grille() {
        use crate::maps::door::Door;
        use bevy::ecs::system::RunSystemOnce;
        // The rule: a gate of the Archive is a grille, and nothing else of it is.
        for r in &RANGES {
            assert_eq!(door_look::look(Some(r.zones.0.map()), true, false), door_look::DoorLook::Grille, "{}", r.name);
        }
        assert_eq!(door_look::look(Some(MapId::Archive), false, false), door_look::DoorLook::Boarded);
        // And as built: each Range's gate Door has no slab to draw but its brass bars, which are children (so they move and hide
        // with it), and it is still solid.
        let mut sim = crate::enemies::zombie::headless::Sim::on(MapId::Archive, 60.0, |_| {});
        let gates: Vec<(Option<(ZoneId, ZoneId)>, bool, usize, bool)> = sim
            .world_mut()
            .run_system_once(|doors: Query<(&Door, Has<Mesh3d>, Option<&Children>, Has<avian3d::prelude::Collider>)>, parts: Query<&Textured>| {
                doors
                    .iter()
                    .map(|(door, slab, children, solid)| {
                        let bars = children.map_or(0, |children| children.iter().filter(|&child| parts.get(child).is_ok_and(|p| p.material == Material::MetalPlate02)).count());
                        (door.gate, slab, bars, solid)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
        let gated: Vec<_> = gates.iter().filter(|(gate, ..)| gate.is_some()).collect();
        assert_eq!(gated.len(), 5);
        for &&(gate, slab, bars, solid) in &gated {
            assert!(RANGES.iter().any(|r| Some(r.zones) == gate));
            assert!(!slab, "{gate:?} still has its boarded slab");
            assert!(bars >= 15, "{gate:?} has {bars} bars");
            assert!(solid, "{gate:?}");
        }
        // The Archive's other Doors keep their boards.
        assert!(gates.iter().filter(|(gate, ..)| gate.is_none()).all(|&(_, slab, bars, _)| slab && bars == 0));
    }

    #[test]
    fn the_fixed_stacks_are_stack_props_and_the_ranges_wear_soffit_letters() {
        use bevy::ecs::system::RunSystemOnce;
        // Every fixed stack is a Stack (none of the stack halls' Props a Shelf), a Bookcase's size, so the shared book rows fit it.
        assert!(archive().props.iter().filter(|prop| [OldStacks, FolioStacks, PeriodicalStacks, NewStacks].contains(&prop.zone)).all(|prop| prop.kind != PropKind::Shelf));
        let stacks: Vec<_> = archive().props.iter().filter(|prop| prop.kind == PropKind::Stack).collect();
        assert_eq!(stacks.len(), data::FIXED_STACKS.len());
        assert!(stacks.iter().all(|prop| prop.size.x.min(prop.size.z) == BOOKCASE.x && prop.size.x.max(prop.size.z) == BOOKCASE.z && prop.size.y == BOOKCASE.y));
        let mut sim = crate::enemies::zombie::headless::Sim::on(MapId::Archive, 60.0, |_| {});
        let (fixed, soffits, faces) = sim
            .world_mut()
            .run_system_once(|names: Query<(&Name, &Children)>, faces: Query<&stacks_look::BookFace>| {
                let fixed = names.iter().filter(|(name, _)| name.as_str() == "Fixed stack").collect::<Vec<_>>();
                let faces = fixed.iter().map(|(_, children)| children.iter().filter(|&child| faces.get(child).is_ok()).count()).collect::<Vec<_>>();
                // A soffit's lettering is a brass mesh: both faces of the wall over every Range's opening.
                let soffits = names.iter().filter(|(name, _)| name.as_str() == "Stack look").count();
                (fixed.len(), soffits, faces)
            })
            .unwrap();
        assert_eq!(fixed, 8);
        assert!(faces.iter().all(|&n| n == 2), "each fixed stack has the two faces of the shared book rows: {faces:?}");
        // One rail and two faces of soffit lettering for each of the five Ranges.
        assert_eq!(soffits, 15);
    }

    #[test]
    fn archive_has_15_probes_covering_every_zone_volume() {
        use crate::maps::map::{ProbeSpec, probes};
        let probes = probes(MapId::Archive);
        assert_eq!(probes.len(), 15);
        let zone_probes: Vec<&ProbeSpec> = probes.iter().filter(|probe| probe.zone.is_some()).collect();
        assert_eq!(zone_probes.len(), 14);
        for &zone in archive().zones {
            let (lo, hi) = zone.volume();
            let probe = zone_probes.iter().find(|probe| probe.zone == Some(zone)).unwrap_or_else(|| panic!("no probe in the {}", zone.name()));
            assert!(probe.lo.cmple(lo).all() && probe.hi.cmpge(hi).all(), "the {}'s probe {:?}..{:?} doesn't cover its volume {lo}..{hi}", zone.name(), probe.lo, probe.hi);
            // Tinted 60% of the way to its lamps' colour.
            assert_eq!(probe.colour, archive().look.probe_colour(palette(Some(zone)).light));
            assert_ne!(probe.colour, crate::maps::look::INDOOR_PROBE_COLOUR);
        }
        // The Theatre and the Well Stair are covered through their full height.
        let height = |zone: ZoneId| zone_probes.iter().find(|probe| probe.zone == Some(zone)).map(|probe| (probe.lo.y, probe.hi.y)).unwrap();
        assert!(height(LectureTheatre).0 <= 4.0 && height(LectureTheatre).1 >= 12.0);
        assert!(height(WellStair).0 <= 0.0 && height(WellStair).1 >= 8.0);
        // The extra one is the lantern's: 6 x 6 m, from the desk to the glazing, cool, at 0.6x.
        let lantern = probes.iter().find(|probe| probe.zone.is_none()).unwrap();
        assert_eq!((lantern.hi - lantern.lo).x, 6.0);
        assert_eq!((lantern.hi - lantern.lo).z, 6.0);
        assert_eq!(lantern.hi.y, 14.0);
        assert!(lantern.lo.y >= 9.0 && lantern.lo.y <= 10.0);
        assert!((lantern.brightness - crate::tuning::INDOOR_PROBE_BRIGHTNESS * 0.6).abs() < 1e-3);
        assert_eq!(lantern.colour, Color::srgb(0.72, 0.8, 0.95));
        // Nothing else has one but the indoor Zones.
        assert_eq!(crate::maps::map::probes(MapId::Greybox).len(), MapId::Greybox.layout().zones.iter().filter(|&&zone| MapId::Greybox.layout().indoor(zone)).count());
    }

    #[test]
    fn archive_box_starts_two_doors_out() {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let doors = doors(&mut sim);
        let first = MapId::Archive.box_locations().next().unwrap();
        let zone = crate::maps::map::BOX_LOCATIONS[first].0;
        // Breadth first from the Catalogue Hall, one Door a step (the archway costs none).
        let mut hops = std::collections::HashMap::from([(CatalogueHall, 0u32)]);
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
        assert_eq!(zone, MapRoom);
        assert_eq!(hops[&zone], 2, "the Box starts in the Map Room, two Doors out");
        // G0 and D2.
        assert_eq!(door_costs(&doors, |_| true)[&zone], 1750);
    }

    #[test]
    fn archive_counts_28_barriers_17_hellhound_spawns_and_four_special_zones() {
        assert_eq!(MapId::Archive.barriers().count(), 28);
        assert!(archive().risers.is_empty());
        assert_eq!(archive().wading.len(), 4);
        assert_eq!(archive().vents.len(), 4);
        assert_eq!(archive().hellhound_spawns.len(), 17);
        for &zone in archive().zones {
            let want = if matches!(zone, ReadingRoom | FloodedCellar | Undercroft) { 2 } else { 1 };
            assert_eq!(archive().hellhound_spawns.iter().filter(|(z, _)| *z == zone).count(), want, "{}", zone.name());
        }
        let specials: Vec<ZoneId> = archive().special_spawns.iter().map(|&(zone, _)| zone).collect();
        assert_eq!(specials, [ReadingRoom, Bindery, FloodedCellar, Undercroft]);
        let boxes: Vec<ZoneId> = MapId::Archive.box_locations().map(|i| crate::maps::map::BOX_LOCATIONS[i].0).collect();
        assert_eq!(boxes, [MapRoom, ReadingRoom, PeriodicalStacks, Bindery, Undercroft, FloodedCellar]);
        assert_eq!(archive().lamps.len(), 88);
        // Every window is in its own Zone's walls, 2 each but the Reading Room's 3 and the Well Stair's 1.
        for &zone in archive().zones {
            let want = match zone {
                ReadingRoom => 3,
                WellStair => 1,
                _ => 2,
            };
            let got = MapId::Archive.barriers().filter(|b| crate::maps::map::WINDOWS[b.0 as usize].0 == zone).count();
            assert_eq!(got, want, "{}", zone.name());
        }
        // The Archive's windows are a run of their own after the other Maps', each the size of its own table.
        assert_eq!(data::WINDOWS.len(), 28);
        assert_eq!(usize::from(W0) + 28, crate::maps::map::WINDOWS.len());
        assert!(crate::maps::map::WINDOWS.len() <= 256, "a BarrierId is a u8");
    }

    /// The Archive's five Ranges are 38 Wheels' worth of Aisles (4 + 3 + 2 + 3 + 2 Bookcases, plus one Aisle each), each Rest with its own
    /// chain of nodes (mouth, Aisle, mouth), the mouth nodes 0.6 m outside the footprint, and one gate Door each in the grille on the high side.
    #[test]
    fn archive_five_ranges_each_with_a_chain_for_every_rest_and_a_gate_door() {
        let bookcases: Vec<u8> = RANGES.iter().map(|r| r.bookcases).collect();
        assert_eq!(bookcases, [4, 3, 2, 3, 2]);
        assert_eq!(RANGES.iter().map(|r| usize::from(r.bookcases) + 1).sum::<usize>() * 2, 38);
        assert_eq!(archive().shifting.iter().map(|piece| piece.wheels().len()).sum::<usize>(), 38);
        assert_eq!(RANGES.iter().map(|r| r.start).collect::<Vec<_>>(), [2, 0, 0, 3, 2], "the start Rests");
        let graph = NavGraph::new(MapId::Archive);
        for (k, r) in RANGES.iter().enumerate() {
            let piece = &archive().shifting[k];
            assert_eq!((piece.rests.len(), piece.leaves.len(), piece.start, piece.door), (usize::from(r.bookcases) + 1, usize::from(r.bookcases), r.start, Some(r.zones)));
            for rest in 0..=r.bookcases {
                let mid = r.aisle_mid(rest);
                for (suffix, across) in [("m0", -(BOOKCASE.z / 2.0 + MOUTH)), ("ai", 0.0), ("m1", BOOKCASE.z / 2.0 + MOUTH)] {
                    let n = graph.node(&format!("{}_{suffix}_{rest}", r.name)).unwrap_or_else(|| panic!("{} has no {suffix} node at Rest {rest}", r.name));
                    let got = graph.nodes[n].at;
                    let want = r.point(mid, across);
                    assert!(Vec2::new(got.x, got.z).distance(want) < 0.01, "{}_{suffix}_{rest} at {got}, wanted {want}", r.name);
                    assert_eq!(got.y, r.floor);
                }
                // Every Rest's joins are the Range's Zone pair.
                assert_eq!(piece.rests[usize::from(rest)].joins, [r.zones]);
            }
            // The one Door edge is the gate's, from the node beyond the grille to the mouth across the start Rest.
            let gate = graph.node(&format!("{}_g", r.name)).unwrap();
            let start_mouth = graph.node(&format!("{}_m1_{}", r.name, r.start)).unwrap();
            assert!(graph.edges.iter().any(|e| e.from == gate && e.to == start_mouth && e.gate == Gate::Door(r.zones.0, r.zones.1)), "{}'s gate", r.name);
            assert_eq!(graph.edges.iter().filter(|e| matches!(e.gate, Gate::Door(a, b) if (a, b) == r.zones) && graph.nodes[e.from].name.starts_with(r.name)).count(), 1);
        }
    }

    /// Every Aisle's two chain edges (mouth to Aisle, Aisle to mouth) are gated by their own Rest of their own Range, and nothing else is.
    #[test]
    fn every_aisle_edge_is_gated_by_its_own_rest() {
        let graph = NavGraph::new(MapId::Archive);
        let mut gated = 0;
        for (k, r) in RANGES.iter().enumerate() {
            for rest in 0..=r.bookcases {
                let ai = graph.node(&format!("{}_ai_{rest}", r.name)).unwrap();
                let at_aisle: Vec<_> = graph.edges.iter().filter(|e| e.from == ai || e.to == ai).collect();
                assert_eq!(at_aisle.len(), 2, "{} Rest {rest}: the Aisle node has just its two mouths", r.name);
                for edge in at_aisle {
                    assert_eq!(edge.gate, Gate::Rest(crate::maps::shifting::PieceId(k as u8), rest), "{} Rest {rest}", r.name);
                    gated += 1;
                    let other = graph.nodes[if edge.from == ai { edge.to } else { edge.from }].name;
                    assert!(other.starts_with(&format!("{}_m", r.name)), "{other}");
                }
            }
        }
        assert_eq!(gated, 2 * 19, "an Aisle edge at each of the 19 Rests' two mouths");
        // And every Rest gate in the graph is one of them.
        let all: Vec<_> = graph.edges.iter().filter(|e| matches!(e.gate, Gate::Rest(..))).collect();
        assert_eq!(all.len(), gated);
        for edge in all {
            let names = (graph.nodes[edge.from].name, graph.nodes[edge.to].name);
            assert!(names.0.contains("_ai_") || names.1.contains("_ai_"), "{names:?}");
        }
    }

    /// Each Range's mouth nodes stand 0.6 m outside its Bookcases' footprint, so clear of every Range's sweep at every Rest.
    #[test]
    fn mouth_nodes_are_outside_every_sweep() {
        let graph = NavGraph::new(MapId::Archive);
        let mut mouths = 0;
        for node in &graph.nodes {
            let Some((range, rest)) = RANGES.iter().find_map(|r| {
                let rest = node.name.strip_prefix(r.name)?.strip_prefix("_m")?;
                Some((r, rest))
            }) else {
                continue;
            };
            mouths += 1;
            let _ = rest;
            for other in &RANGES {
                let (lo, hi) = other.sweep();
                let inside = node.at.x > lo.x && node.at.x < hi.x && node.at.z > lo.y && node.at.z < hi.y;
                assert!(!(inside && (node.at.y - other.floor).abs() < 1.0), "{} is in {}'s sweep", node.name, other.name);
            }
            let across = (Vec2::new(node.at.x, node.at.z) - range.point(range.rail, 0.0)).dot(if range.along_x { Vec2::Y } else { Vec2::X }).abs();
            assert!((across - (BOOKCASE.z / 2.0 + MOUTH)).abs() < 0.01, "{} stands {across} m from the line", node.name);
        }
        assert_eq!(mouths, 2 * 19);
    }

    /// All of the 720 Rest combinations of the five Ranges, with every Door bought: every Zone is reachable from the start, by the nav graph
    /// (every node bar the Aisle that is shut, and the vents and pens' outsides) and by the Zone joins alone.
    #[test]
    fn every_rest_combination_keeps_every_archive_zone_reachable() {
        let graph = NavGraph::new(MapId::Archive);
        let zones = archive().zones;
        let doors: Vec<(ZoneId, ZoneId)> = zones.iter().flat_map(|&a| zones.iter().map(move |&b| (a, b))).collect();
        let yes = |_: crate::maps::map::BarrierId| true;
        let start = graph.nearest(archive().seat_start(0));
        let mut combos = 0;
        let plain = plain_door_joins();
        for rests in rest_combinations() {
            combos += 1;
            let state = crate::enemies::nav::NavState::new(&doors, &yes).with_rests(&rests);
            for (to, node) in graph.nodes.iter().enumerate() {
                if node.zone.is_none() || node.vent || node.name.ends_with("_out") {
                    continue;
                }
                // An Aisle node is only there while its Range stands at it.
                let shut = RANGES.iter().enumerate().any(|(k, r)| node.name.strip_prefix(r.name).is_some_and(|rest| rest.starts_with("_ai_")) && !node.name.ends_with(&format!("_{}", rests[k].unwrap())));
                let reached = graph.plan(to, start, state).is_some();
                assert_eq!(reached, !shut, "{} with Rests {rests:?}", node.name);
            }
            // By joins: each Range's Rest joins its Zone pair, so the Zones are one connected whole (the Doors' joins plus the fixed ones).
            let mut seen = std::collections::HashSet::from([CatalogueHall]);
            let mut joins: Vec<(ZoneId, ZoneId)> = FIXED_JOINS.to_vec();
            joins.extend(archive().shifting.iter().enumerate().flat_map(|(k, piece)| piece.open_joins(rests[k], true).iter().copied()));
            joins.extend(plain.iter().copied());
            loop {
                let before = seen.len();
                for &(a, b) in &joins {
                    if seen.contains(&a) || seen.contains(&b) {
                        seen.insert(a);
                        seen.insert(b);
                    }
                }
                if seen.len() == before {
                    break;
                }
            }
            assert_eq!(seen.len(), zones.len(), "joins with Rests {rests:?}: {:?}", zones.iter().filter(|z| !seen.contains(z)).collect::<Vec<_>>());
        }
        assert_eq!(combos, 5 * 4 * 3 * 4 * 3);
    }

    /// Every combination of the five Ranges' Rests, 5 x 4 x 3 x 4 x 3 of them.
    fn rest_combinations() -> Vec<Vec<Option<u8>>> {
        let mut all: Vec<Vec<Option<u8>>> = vec![vec![]];
        for r in &RANGES {
            all = all.into_iter().flat_map(|so_far| (0..=r.bookcases).map(move |rest| so_far.iter().copied().chain([Some(rest)]).collect::<Vec<_>>())).collect();
        }
        all
    }

    /// The plain Doors' joins (the gates guard the Ranges' own, which come from the Rests).
    fn plain_door_joins() -> Vec<(ZoneId, ZoneId)> {
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::maps::door::Door;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        sim.world_mut().run_system_once(|doors: Query<&Door>| doors.iter().filter_map(|door| door.joins).collect::<Vec<_>>()).unwrap()
    }

    /// The Power Switch and Pack-a-Punch are reachable at every Rest combination: with every Door bought, and with only the Doors that are
    /// not Range gates, since the east route has no Range on it.
    #[test]
    fn power_and_pack_a_punch_are_reachable_at_every_rest_combination() {
        use crate::enemies::zombie::headless::Sim;
        let graph = NavGraph::new(MapId::Archive);
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let doors = doors(&mut sim);
        let all: Vec<(ZoneId, ZoneId)> = doors.iter().filter_map(|&(joins, _)| joins).collect();
        let bought: Vec<(ZoneId, ZoneId)> = all.iter().copied().filter(|&joins| !is_a_gate(joins)).collect();
        let yes = |_: crate::maps::map::BarrierId| true;
        let start = graph.nearest(archive().seat_start(0));
        let spots = [("the Power Switch", crate::maps::power::switch_spot(MapId::Archive).unwrap().0), ("Pack-a-Punch", crate::weapons::pack_a_punch::machine_at(MapId::Archive))];
        let mut checked = 0;
        for rests in rest_combinations() {
            for open in [&all, &bought] {
                let state = crate::enemies::nav::NavState::new(open, &yes).with_rests(&rests);
                for (what, at) in spots {
                    let near = graph.nearest(Vec3::new(at.x, CELLAR, at.z) + Vec3::Y * 0.1);
                    assert!(graph.plan(near, start, state).is_some(), "{what} is cut off with Rests {rests:?}");
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 720 * 2 * 2);
    }

    /// Every Wheel stands on its Range's mouth strip in front of its Aisle's middle, on the floor of its own Zone, facing away from the Range
    /// and clear of every sweep; two for every Aisle, one at each mouth.
    #[test]
    fn every_wheel_stands_in_a_mouth_strip_in_front_of_its_aisle() {
        for (k, r) in RANGES.iter().enumerate() {
            for wheel in archive().shifting[k].wheels() {
                let high = wheel.facing.dot(if r.along_x { Vec2::Y } else { Vec2::X }) > 0.0;
                assert_eq!(wheel.zone, if high { r.zones.1 } else { r.zones.0 });
                assert_eq!(wheel.at.y, r.floor);
                let want = r.point(r.aisle_mid(wheel.rest), (if high { 1.0 } else { -1.0 }) * (BOOKCASE.z / 2.0 + WHEEL_OFF));
                assert!(Vec2::new(wheel.at.x, wheel.at.z).distance(want) < 1e-4, "{} Rest {}", r.name, wheel.rest);
                assert!((wheel.facing.length() - 1.0).abs() < 1e-5);
                assert_eq!(crate::enemies::nav::zone_at(MapId::Archive, wheel.at + Vec3::Y * 0.1), Some(wheel.zone), "{} Rest {} stands in its Zone", r.name, wheel.rest);
                for other in &RANGES {
                    let (lo, hi) = other.sweep();
                    assert!(!(wheel.at.x > lo.x && wheel.at.x < hi.x && wheel.at.z > lo.y && wheel.at.z < hi.y && (wheel.at.y - other.floor).abs() < 1.0), "{} is in {}'s sweep", r.name, other.name);
                }
            }
        }
    }

    /// Every spawn point stands on a floor, and every Zone's two ends of a Door are on its floor.
    #[test]
    fn archive_spawn_points_stand_on_a_floor() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let bad: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                let layout = MapId::Archive.layout();
                let mut spots: Vec<(String, Vec3)> = MapId::Archive.barriers().map(|b| (format!("window {}", b.0 - W0), crate::maps::map::window_spawn(b))).collect();
                spots.extend(layout.hellhound_spawns.iter().chain(layout.special_spawns).chain(layout.wading).map(|&(zone, at)| (format!("a spawn point in the {}", zone.name()), at)));
                spots.extend(layout.vents.iter().map(|&(zone, at)| (format!("a vent's floor in the {}", zone.name()), at)));
                spots
                    .into_iter()
                    .filter_map(|(what, spawn)| {
                        let hit = spatial.cast_ray(spawn + Vec3::Y * 0.5, Dir3::NEG_Y, 1.0, true, &filter);
                        let floor = hit.map(|hit| spawn.y + 0.5 - hit.distance);
                        floor.is_none_or(|floor| (floor - spawn.y).abs() > 0.05).then(|| format!("{what} at {spawn}: floor {floor:?}"))
                    })
                    .collect()
            })
            .unwrap();
        assert!(bad.is_empty(), "{bad:#?}");
    }

    // ---------------------------------------------------------------- Infills (#536)

    /// How far the rectangle `min..max` is from `at` on the plan.
    fn gap(min: Vec2, max: Vec2, at: Vec2) -> f32 {
        (min - at).max(at - max).max(Vec2::ZERO).length()
    }

    /// How far the rectangle `min..max` is from the segment `a`..`b`.
    fn gap_to_segment(min: Vec2, max: Vec2, a: Vec2, b: Vec2) -> f32 {
        let steps = ((b - a).length() / 0.05).ceil().max(1.0) as u32;
        (0..=steps).map(|k| gap(min, max, a.lerp(b, k as f32 / steps as f32))).fold(f32::MAX, f32::min)
    }

    /// Whether two rectangles on the plan overlap, or touch within `margin`.
    fn near((alo, ahi): (Vec2, Vec2), (blo, bhi): (Vec2, Vec2), margin: f32) -> bool {
        alo.x < bhi.x + margin && blo.x < ahi.x + margin && alo.y < bhi.y + margin && blo.y < ahi.y + margin
    }

    /// A Range's gate corridor: the grille's face along the whole rail, a metre beyond it, and the mouth strip on the same side, where the
    /// Door and the Wheels' corridor stand.
    fn gate_corridor(r: &Range) -> (Vec2, Vec2) {
        let (face, plane) = (r.gate_side * BOOKCASE.z / 2.0, r.gate_side * (BOOKCASE.z / 2.0 + GATE_OFF));
        let beyond = if r.gate_side > 0.0 { plane + 1.0 } else { plane - 1.0 };
        r.rect((r.rail - MOUTH, r.rail + r.length() + MOUTH), (face.min(beyond), face.max(beyond)))
    }

    /// No Archive Infill stands in a Range's sweep, under its soffit (the wall over its opening, along the whole rail), on either mouth strip
    /// or in its gate's corridor: a Bookcase rolls over none of them and no Wheel or Door is crowded.
    #[test]
    fn no_archive_infill_in_a_sweep_soffit_or_mouth_strip() {
        let mut bad = Vec::new();
        for infill in archive().infills {
            let what = format!("the Infill at {}..{} in {}", infill.min, infill.max, infill.zone.name());
            for r in RANGES.iter().filter(|r| (r.floor - infill.zone.floor()).abs() < 1.0) {
                let (sweep, strips, gate) = (r.sweep(), r.mouth_strips(), gate_corridor(r));
                // The soffit is the wall over the sweep's whole run, the Range's line down the middle of it.
                let soffit = r.rect((r.rail, r.rail + r.length()), (-stacks_look::WALL_FACE, stacks_look::WALL_FACE));
                for (name, region) in [("sweep", sweep), ("soffit", soffit), ("mouth strip", strips[0]), ("mouth strip", strips[1]), ("gate corridor", gate)] {
                    if near((infill.min, infill.max), region, 0.0) {
                        bad.push(format!("{what} is in {}'s {name}", r.name));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// The book-lift core is a quarter Infill in each Zone it crosses, 3 m or more from every Range's sweep on its level (RA and RC 4.7 m,
    /// RB and RD 3.8 m), and it crosses the Stacks level's four halls and the cellar's Flooded Cellar, Undercroft and Strong Room.
    #[test]
    fn the_book_lift_core_is_3_m_from_every_sweep() {
        let core: Vec<&Infill> = archive().infills.iter().filter(|infill| is_core(infill)).collect();
        let mut zones: Vec<ZoneId> = core.iter().map(|infill| infill.zone).collect();
        zones.sort_by_key(|zone| zone.name());
        let mut wanted = vec![OldStacks, FolioStacks, PeriodicalStacks, NewStacks, FloodedCellar, Undercroft, StrongRoom];
        wanted.sort_by_key(|zone| zone.name());
        assert_eq!(zones, wanted, "the core's quarters");
        for infill in core {
            for r in RANGES.iter().filter(|r| (r.floor - infill.zone.floor()).abs() < 1.0) {
                let (lo, hi) = r.sweep();
                let apart = (lo - infill.max).max(infill.min - hi).max(Vec2::ZERO).length();
                assert!(apart >= 3.0, "the core in {} is {apart:.2} m from {}'s sweep", infill.zone.name(), r.name);
            }
        }
        // The Stacks level's four quarters meet at the middle of the core: each holds one quadrant of the 6 x 6 m shaft.
        let quarter_area: f32 = archive().infills.iter().filter(|i| is_core(i) && i.zone.floor() == STACKS).map(|i| (i.max - i.min).element_product()).sum();
        assert_eq!(quarter_area, 36.0, "the four quarters fill the shaft");
    }

    /// A Training loop's lane: each leg's centreline, per Zone, that must keep 3 m clear (1.5 m either side). The Catalogue banks, the Issue Desk and the
    /// Undercroft's columns are ringed 1.5 m off (a lane 3 m wide round each), the Tiers run down both aisles, across the dais and along the crossover,
    /// the Causeway out and back through the water, the Stack Ring through each hall's lanes between the Ranges' mouths, and the Grand loop's legs the
    /// others don't cover.
    const LANE: f32 = 1.5;

    fn ring(centre: Vec2, size: Vec2, off: f32) -> Vec<Vec2> {
        let h = size / 2.0 + Vec2::splat(off);
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)].map(|(sx, sz)| centre + h * Vec2::new(sx, sz)).to_vec()
    }

    fn training_loops() -> Vec<(&'static str, ZoneId, Vec<Vec2>)> {
        let pts = |list: &[(f32, f32)]| list.iter().map(|&(x, z)| Vec2::new(x, z)).collect::<Vec<_>>();
        vec![
            ("the west Catalogue bank", CatalogueHall, ring(v(-12.5, 6.5), v(8.0, 1.2), LANE)),
            ("the east Catalogue bank", CatalogueHall, ring(v(12.5, 6.5), v(8.0, 1.2), LANE)),
            ("the Issue Desk", ReadingRoom, ring(v(0.0, 24.0), v(6.0, 6.0), LANE)),
            ("the Tiers", LectureTheatre, pts(&[(21.5, 2.0), (34.5, 2.0), (34.5, 19.5), (21.5, 19.5), (21.5, 2.0)])),
            ("the Columns (west)", Undercroft, ring(v(-15.0, 28.5), v(0.6, 0.6), 2.5)),
            ("the Columns (middle)", Undercroft, ring(v(-10.0, 28.5), v(0.6, 0.6), 2.5)),
            ("the Columns (east)", Undercroft, ring(v(-5.0, 28.5), v(0.6, 0.6), 2.5)),
            ("the Causeway", FloodedCellar, pts(&[(-16.0, 10.0), (16.0, 10.0), (16.0, 6.7), (-16.0, 6.7), (-16.0, 10.0)])),
            ("the Stack Ring (Old Stacks)", OldStacks, pts(&[(-3.1, 10.5), (-4.8, 10.5), (-4.8, 15.4), (-9.2, 15.4), (-9.2, 16.9)])),
            ("the Stack Ring (Folio Stacks)", FolioStacks, pts(&[(-9.2, 23.1), (-9.2, 25.6), (-4.8, 25.6), (-4.8, 28.3), (-3.1, 28.3)])),
            ("the Stack Ring (Periodical Stacks)", PeriodicalStacks, pts(&[(3.1, 28.3), (5.75, 28.3), (5.75, 25.6), (12.8, 25.6), (12.8, 23.1)])),
            ("the Stack Ring (New Stacks)", NewStacks, pts(&[(12.8, 16.9), (12.8, 15.4), (5.75, 15.4), (5.75, 10.5), (3.1, 10.5)])),
            ("the Grand loop (Bindery)", Bindery, pts(&[(26.0, 25.5), (26.0, 29.5), (21.8, 30.0)])),
            ("the Grand loop (Map Room)", MapRoom, pts(&[(-35.0, 31.2), (-31.0, 31.2), (-31.0, 24.0), (-21.8, 24.0)])),
        ]
    }

    /// No Training loop in #513's table is narrowed below 3 m by an Infill: each stays 1.5 m from every lane's centreline in its Zone.
    #[test]
    fn no_archive_infill_narrows_a_training_loop_below_3_m() {
        let mut bad = Vec::new();
        for (name, zone, lane) in training_loops() {
            for infill in archive().infills.iter().filter(|infill| infill.zone == zone) {
                for leg in lane.windows(2) {
                    let apart = gap_to_segment(infill.min, infill.max, leg[0], leg[1]);
                    if apart < LANE - 1e-4 {
                        bad.push(format!("the Infill at {}..{} is {apart:.2} m from {name}'s lane", infill.min, infill.max));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    // ---------------------------------------------------------------- Props (#540)

    /// A Prop's footprint on the plan.
    fn footprint_of(prop: &Prop) -> (Vec2, Vec2) {
        let half = Vec2::new(prop.size.x, prop.size.z) / 2.0;
        let at = Vec2::new(prop.at.x, prop.at.z);
        (at - half, at + half)
    }

    /// No Archive Prop stands in a Range's sweep, under its soffit (the wall over the sweep's whole run), on either mouth strip or in its gate's
    /// corridor: a Bookcase rolls over none of them, and no Wheel or Door is crowded. (A Range only reaches the Props on its own floor.)
    #[test]
    fn no_archive_prop_in_a_sweep_soffit_or_mouth_strip() {
        let mut bad = Vec::new();
        for prop in archive().props {
            let what = format!("the {:?} at {} in {}", prop.kind, prop.at, prop.zone.name());
            for r in RANGES.iter().filter(|r| (r.floor - prop.at.y).abs() < 1.0) {
                let (sweep, strips, gate) = (r.sweep(), r.mouth_strips(), gate_corridor(r));
                let soffit = r.rect((r.rail, r.rail + r.length()), (-stacks_look::WALL_FACE, stacks_look::WALL_FACE));
                for (name, region) in [("sweep", sweep), ("soffit", soffit), ("mouth strip", strips[0]), ("mouth strip", strips[1]), ("gate corridor", gate)] {
                    if near(footprint_of(prop), region, 0.0) {
                        bad.push(format!("{what} is in {}'s {name}", r.name));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// No Prop narrows a Training loop below 3 m: each stays 1.5 m from every lane's centreline in its Zone (the loop's own centre is 1.5 m inside).
    #[test]
    fn no_archive_prop_narrows_a_training_loop_below_3_m() {
        let mut bad = Vec::new();
        for (name, zone, lane) in training_loops() {
            for prop in archive().props.iter().filter(|prop| prop.zone == zone) {
                let (min, max) = footprint_of(prop);
                for leg in lane.windows(2) {
                    let apart = gap_to_segment(min, max, leg[0], leg[1]);
                    if apart < LANE - 1e-4 {
                        bad.push(format!("the {:?} at {} is {apart:.2} m from {name}'s lane", prop.kind, prop.at));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    /// Every reading-level Prop stands on its Zone's floor, wholly inside its Zone and under its ceiling, and turns its model a half turn at most.
    #[test]
    fn every_reading_level_prop_stands_on_its_zones_floor_inside_its_zone() {
        for prop in data::READING_PROPS {
            let (lo, hi) = prop.zone.volume();
            assert!(prop.at.y == prop.zone.floor() || archive().raised_floors.iter().any(|r| r.zone == prop.zone && (r.height - prop.at.y).abs() < 1e-3), "the {:?} at {}", prop.kind, prop.at);
            let (min, max) = footprint_of(&prop);
            assert!(min.x >= lo.x && max.x <= hi.x && min.y >= lo.z && max.y <= hi.z, "the {:?} at {} sticks out of {}", prop.kind, prop.at, prop.zone.name());
            assert!(prop.at.y + prop.size.y < hi.y, "the {:?} at {} reaches the ceiling", prop.kind, prop.at);
            assert!(prop.turn == 0 || prop.turn == 2, "the {:?} at {}: only half turns face a wall", prop.kind, prop.at);
            // Ladders only stand against a wall (a Zone's edge), never in the open.
            if prop.kind == PropKind::Ladder {
                let against = (min.x - lo.x).min(hi.x - max.x).min(min.y - lo.z).min(hi.z - max.y);
                assert!(against < 0.1, "the Ladder at {} is {against:.2} m from every wall", prop.at);
            }
        }
    }

    /// The Strong Room's vault-door reveal stands on the hinge side of the Power Door PD (the west, the trap switch being on the east), close
    /// by it but clear of it, and 1.5 x 1.2 m.
    #[test]
    fn the_vault_door_reveal_stands_beside_pd_on_its_hinge_side() {
        let reveal = archive().infills.iter().find(|infill| infill.zone == StrongRoom && (infill.max - infill.min - v(1.5, 1.2)).abs().max_element() < 1e-4).expect("a 1.5 x 1.2 m reveal");
        let pd = v(10.0, 20.0);
        assert!(reveal.max.x < pd.x && reveal.min.y == 20.0, "the reveal is on the west side, against the wall");
        let apart = gap(reveal.min, reveal.max, pd);
        assert!((1.8..3.5).contains(&apart), "the reveal is {apart:.2} m from PD");
    }

    /// The core's quarters stand in the Flooded Cellar's pool and run down to its bed, so no gap shows under the water, and every Archive
    /// Infill is a World collider in the built Map (every peer builds from the same Layout).
    #[test]
    fn the_flooded_cellars_infills_run_down_to_the_pools_bed_and_every_infill_is_solid() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::enemies::zombie::headless::Sim;
        use crate::layers::GameLayer;
        let mut sim = Sim::on(MapId::Archive, 60.0, |_| {});
        let bad: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                let probe = Collider::sphere(0.1);
                let mut bad = Vec::new();
                for infill in archive().infills {
                    let middle = (infill.min + infill.max) / 2.0;
                    let (floor, top) = infill.span();
                    let mut heights = vec![floor + 0.5, (floor + top) / 2.0, top - 0.5];
                    // In the pool: just over the bed, under the water's face.
                    if infill.zone == FloodedCellar {
                        heights.push(floor - 0.4);
                    }
                    for y in heights {
                        if spatial.shape_intersections(&probe, Vec3::new(middle.x, y, middle.y), Quat::IDENTITY, &filter).is_empty() {
                            bad.push(format!("the Infill at {}..{} in {} is hollow at y {y}", infill.min, infill.max, infill.zone.name()));
                        }
                    }
                }
                bad
            })
            .unwrap();
        assert!(bad.is_empty(), "{bad:#?}");
    }
}
