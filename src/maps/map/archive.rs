//! The Archive Map: a flooded library under a university, in 14 Zones on three levels going down
//! from the start. Nothing rolls yet: its five Ranges stand as plain walls with their Bookcases as
//! fixed boxes at their start Rests (the Moving Stacks come in a later ticket), each Range joining
//! two Zones through its one open 3 m Aisle behind a bought gate Door.
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
//! the Look, the Material Sets, the Moving Stacks and the Props each come in a later ticket. Its path
//! nodes, lamps, windows, stairwells and Props (in `data`) are laid out by `tools/archive_layout.py`.

use bevy::prelude::*;

use crate::enemies::sapper::SapperTiming;
use crate::maps::zone::ZoneId;
use crate::weapons::arsenal::WeaponId;

use super::build::{Builder, Gap};
use super::{DOOR_HEIGHT, Layout, Level, Pool, Prop, RaisedFloor, STOREY, Slope, concat, door, palette};

#[path = "archive/data.rs"]
mod data;

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
    infills: &[],
    slopes: &SLOPES,
    sunken: &data::SUNKEN,
    water: &POOLS,
    canals: &[],
    props: &PROPS,
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
    outdoor: &[],
    roofed: true,
    // Under a university, so there is no outside: the plan's edge is its walls.
    outside: None,
    sun: Vec3::new(0.3, -1.0, 0.5),
    // The Greybox's look until the Archive has its own.
    look: crate::maps::look::DUSK,
    lamp_style: super::LampStyle::Bulb,
    bulb_zones: &[],
    plan: (Vec2::new(-36.0, 0.0), Vec2::new(36.0, 36.0)),
    build,
};

/// The Well Stair's archway into the Flooded Cellar never closes.
const FIXED_JOINS: [(ZoneId, ZoneId); 1] = [(ZoneId::WellStair, ZoneId::FloodedCellar)];

// ---------------------------------------------------------------- The Ranges

/// A Range: 2 to 4 Bookcases on one floor rail between fixed end walls, with exactly one 3 m Aisle open. A Range of
/// `bookcases` Bookcases has `bookcases + 1` Rests (Aisle positions). Here each is a wall with its Bookcases as fixed
/// boxes at the start Rest; the gate Door stands across its one open Aisle on the `gate` side.
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
pub(super) const BOOKCASE: Vec3 = Vec3::new(1.2, 3.0, 5.0);
pub(super) const AISLE: f32 = 3.0;
#[cfg_attr(not(test), allow(dead_code))]
pub(super) const MOUTH: f32 = 0.6;
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
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn length(&self) -> f32 {
        f32::from(self.bookcases) * BOOKCASE.x + AISLE
    }

    /// Where its Aisle is at `rest`, as the middle of the Aisle along the rail.
    pub fn aisle_mid(&self, rest: u8) -> f32 {
        self.rail + BOOKCASE.x * f32::from(rest) + AISLE / 2.0
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
    pub fn point(&self, along: f32, across: f32) -> Vec2 {
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

    /// Where Bookcase `i` stands at Rest `rest`, as its extent along the rail.
    pub fn bookcase(&self, i: u8, rest: u8) -> (f32, f32) {
        let lo = self.rail + BOOKCASE.x * f32::from(i) + if i >= rest { AISLE } else { 0.0 };
        (lo, lo + BOOKCASE.x)
    }

    /// The open Aisle at `rest`, as a rectangle on the floor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn aisle(&self, rest: u8) -> (Vec2, Vec2) {
        let lo = self.rail + BOOKCASE.x * f32::from(rest);
        self.rect((lo, lo + AISLE), (-BOOKCASE.z / 2.0, BOOKCASE.z / 2.0))
    }
}

/// A Range's wall (with its opening) and fixed Bookcases at the start Rest, and its gate: two stubs closing in the Aisle
/// either side and a plane across it with the Door (2.4 m of 3.4) in the middle, `GATE_OFF` out from the footprint's face.
fn range(b: &mut Builder, r: &Range) {
    let mid = r.aisle_mid(r.start);
    let wood = Color::srgb(0.32, 0.2, 0.11);
    for i in 0..r.bookcases {
        let (lo, hi) = r.bookcase(i, r.start);
        let centre = r.point((lo + hi) / 2.0, 0.0);
        let size = if r.along_x { Vec3::new(BOOKCASE.x, BOOKCASE.y, BOOKCASE.z) } else { Vec3::new(BOOKCASE.z, BOOKCASE.y, BOOKCASE.x) };
        b.block(size, Vec3::new(centre.x, r.floor + BOOKCASE.y / 2.0, centre.y), wood);
    }
    let face = r.gate_side * BOOKCASE.z / 2.0;
    let plane = r.gate_side * (BOOKCASE.z / 2.0 + GATE_OFF);
    let (a, b2) = (r.point(mid - 1.7, plane), r.point(mid + 1.7, plane));
    b.wall(a, b2, r.floor, DOOR_HEIGHT, &[(1.7, door(r.zones.0, r.zones.1, r.price))]);
    for along in [mid - 1.7, mid + 1.7] {
        let (p, q) = (r.point(along, face), r.point(along, plane));
        let (p, q) = (p.min(q), p.max(q));
        b.wall(p, q, r.floor, DOOR_HEIGHT, &[]);
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

/// The Props built so far: the fixed stacks of the ring halls (as plain `Shelf` Props) and the Training loops' centres, which are plain
/// boxes now (the Props ticket dresses them).
const PROPS: [Prop; 29] = concat::<_, 8, 21, 29>(data::FIXED_STACKS, data::LOOP_CENTRES);

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
        assert!(archive().shifting.is_empty() && archive().wings.is_empty());
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
    fn archive_nothing_stands_in_a_range_sweep_or_mouth_strip() {
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

    /// The Archive's five Ranges are 38 Wheels' worth of Aisles (4 + 3 + 2 + 3 + 2 Bookcases, plus one Aisle each), each with its chain of nodes
    /// (mouth, Aisle, mouth) ungated, with its mouth nodes 0.6 m outside the footprint, and one gate Door each across its Aisle.
    #[test]
    fn archive_five_ranges_each_with_an_ungated_aisle_chain_and_a_gate_door() {
        let bookcases: Vec<u8> = RANGES.iter().map(|r| r.bookcases).collect();
        assert_eq!(bookcases, [4, 3, 2, 3, 2]);
        assert_eq!(RANGES.iter().map(|r| usize::from(r.bookcases) + 1).sum::<usize>() * 2, 38);
        let graph = NavGraph::new(MapId::Archive);
        for r in &RANGES {
            let mid = r.aisle_mid(r.start);
            let node = |suffix: &str| {
                let n = graph.node(&format!("{}_{suffix}", r.name)).unwrap_or_else(|| panic!("{} has no {suffix} node", r.name));
                graph.nodes[n].at
            };
            for (suffix, across) in [("m0", -(BOOKCASE.z / 2.0 + MOUTH)), ("ai", 0.0), ("m1", BOOKCASE.z / 2.0 + MOUTH)] {
                let want = r.point(mid, across);
                let got = node(suffix);
                assert!(Vec2::new(got.x, got.z).distance(want) < 0.01, "{}_{suffix} at {got}, wanted {want}", r.name);
                assert_eq!(got.y, r.floor);
            }
            // Ungated: the chain's edges are plain, and the one Door edge is the gate's.
            let ids: Vec<usize> = ["m0", "ai", "m1"].iter().map(|s| graph.node(&format!("{}_{s}", r.name)).unwrap()).collect();
            for pair in ids.windows(2) {
                assert!(graph.edges.iter().any(|e| e.from == pair[0] && e.to == pair[1] && e.gate == Gate::Open), "{}'s chain is gated", r.name);
            }
            let gate = graph.node(&format!("{}_g", r.name)).unwrap();
            assert!(graph.edges.iter().any(|e| e.from == gate && e.to == ids[2] && e.gate == Gate::Door(r.zones.0, r.zones.1)), "{}'s gate", r.name);
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
}
