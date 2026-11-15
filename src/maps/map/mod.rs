//! The Maps: what each is made of, as data, and how the one a Run is played on is built.
//!
//! Every Map has a `Layout`: its Zones, windows, Spawn Points, Box Locations, Wall Weapons,
//! floors, Props and path nodes. Each Map is built in its own part of the world, so no two Maps'
//! Zones overlap and a point is in at most one Map's Zone (ADR 0005).

use bevy::light::{EnvironmentMapLight, LightProbe, NotShadowCaster};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::weapons::arsenal::WeaponId;
use crate::economy::buy::{Buyable, Offers};
use crate::enemies::nav::Gate;
use crate::state::{GameState, InRun};
use crate::tuning::*;
use crate::weapons::wall_weapon::{WallWeapon, priced_for, wall_price};
use crate::maps::zone::ZoneId;

mod build;
mod cove;
mod door_look;
pub mod decals;
mod dam;
mod fitting;
mod greybox;
mod quarry;
pub mod textures;
mod village;

use build::{Builder, Gap};

pub use greybox::CONTROL_PLATFORM;
#[cfg(test)]
pub use greybox::PLAYER_START;
pub use quarry::QUARRY_X;
pub use village::VILLAGE_X;
pub use dam::DAM_X;
pub use cove::COVE_X;

use crate::maps::darkness::{Chalk, Lamp};
use crate::maps::generator::{Generator, SectionId};
use crate::maps::jump_pad::JumpPad;
pub use crate::maps::water::{Canal, Level, Pool, TideLevel, Wading, water_at};
use crate::enemies::sapper::SapperTiming;

/// One storey.
pub(crate) const STOREY: f32 = 4.0;

/// How far a street's gas lamp lights, where a room's lights 25 m: the Village has 86 lamps.
const OUTDOOR_LAMP_RANGE: f32 = 14.0;
pub(crate) const WALL_THICKNESS: f32 = 0.4;
const DOOR_WIDTH: f32 = 2.4;
const DOOR_HEIGHT: f32 = 3.0;
const RAIL_HEIGHT: f32 = 1.1;
/// How thick a roof's parapet is. It stands as high as a rail.
const PARAPET_THICKNESS: f32 = 0.3;

/// Which Map a Run is played on. As a resource, it's the Map of the current (or just-ended) Run.
#[derive(Resource, Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapId {
    /// Five rooms ringing the Yard, the Balcony, and the south wing.
    #[default]
    Greybox,
    /// An open-cast quarry climbed terrace by terrace, from the pit floor to the Headframe.
    Quarry,
    /// A village strung along its High Street and the lanes behind it, the Power at one end and
    /// Pack-a-Punch at the other.
    Village,
    /// A dam in the dark: a road along its crest, galleries inside it and a turbine hall at its
    /// foot, Pack-a-Punch in the Control Room in its middle.
    Dam,
    /// A cliffside town on four levels, climbed by stairs from the beach to a clifftop
    /// lighthouse, the Power at the lighthouse's foot and Pack-a-Punch in a cave on the shore.
    Cove,
}

impl MapId {
    pub const ALL: [MapId; 5] = [MapId::Greybox, MapId::Quarry, MapId::Village, MapId::Dam, MapId::Cove];

    pub fn name(self) -> &'static str {
        match self {
            MapId::Greybox => "Greybox",
            MapId::Quarry => "Quarry",
            MapId::Village => "Village",
            MapId::Dam => "Dam",
            MapId::Cove => "Cove",
        }
    }

    /// A line on what the Map is like, for choosing it.
    pub fn blurb(self) -> &'static str {
        match self {
            MapId::Greybox => "A house round a courtyard, and a wing across the field.",
            MapId::Quarry => "A pit and its works, climbed terrace by terrace.",
            MapId::Village => "A long village of streets and lanes, walked end to end.",
            MapId::Dam => "A dam's crest, its galleries and the turbine hall at its foot.",
            MapId::Cove => "A cliffside town, climbed by stairs from the beach to its lighthouse.",
        }
    }

    /// What the Map is made of.
    pub const fn layout(self) -> &'static Layout {
        match self {
            MapId::Greybox => &greybox::GREYBOX,
            MapId::Quarry => &quarry::QUARRY,
            MapId::Village => &village::VILLAGE,
            MapId::Dam => &dam::DAM,
            MapId::Cove => &cove::COVE,
        }
    }
}

/// What one Map is made of: everything placed in it that the rest of the game reads, as data.
/// Things a system owns, like the Perk machines, the Power Switch, Pack-a-Punch and the Traps,
/// keep their spots with that system, by `MapId`.
pub struct Layout {
    /// The Map's Zones. The first is where every Run starts.
    pub zones: &'static [ZoneId],
    /// Where each Seat's Player starts, in Seat order, all in the first Zone.
    pub seats: [Vec3; 4],
    /// The chalk outlines: each Wall Weapon, the Zone it's in, where on a wall's face its centre
    /// sits, and which way that face looks (x, z).
    pub wall_weapons: &'static [(WeaponId, ZoneId, Vec3, Vec2)],
    /// The windows, in `WINDOWS`' order.
    pub windows: &'static [(ZoneId, Vec3, Vec2)],
    /// The Risers, where Zombies climb out of the ground, and the Zone each serves.
    pub risers: &'static [(ZoneId, Vec3)],
    /// The Wading Points, where Zombies stand up out of Shallow Water, and the Zone each serves:
    /// a spot on the bed, as `risers` is a spot on the ground. Empty on a Map without water.
    pub wading: &'static [(ZoneId, Vec3)],
    /// The Hellhound Spawn Points: the Zone each serves and where it stands on the floor.
    pub hellhound_spawns: &'static [(ZoneId, Vec3)],
    /// The Special Spawn Points, where a Napalm or a Shrieker rises.
    pub special_spawns: &'static [(ZoneId, Vec3)],
    /// The Vents Nova Crawlers drop from: the Zone each is in, and the spot on the floor below
    /// it. Each opens `NOVA_VENT_HEIGHT` up, at the top of a one-way path-node edge to that spot.
    pub vents: &'static [(ZoneId, Vec3)],
    pub raised_floors: &'static [RaisedFloor],
    /// The steps and ramps.
    pub slopes: &'static [Slope],
    pub sunken: &'static [Sunken],
    /// The pools of still water, empty on a Map without any (see `water`).
    pub water: &'static [Pool],
    /// The Canals that pools can follow the Tide with, empty on every Map until the Tide exists.
    pub canals: &'static [Canal],
    /// The Props. Low ones (up to about 0.8 m) can be jumped onto and anything up to about 1.2 m
    /// clambered onto with a running jump; nothing a Player gets on top of lifts them out of a
    /// Zombie's reach. Tall ones are walls to weave round.
    pub props: &'static [Prop],
    /// The lamps: the Zone whose tint each takes, and where it hangs.
    pub lamps: &'static [(ZoneId, Vec3)],
    /// Joins between Zones that never close: archways, ramps and drops. A drop is one-way to walk
    /// but counts both ways for activation.
    pub fixed_joins: &'static [(ZoneId, ZoneId)],
    /// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
    pub nav_nodes: &'static [(&'static str, f32, f32, f32)],
    /// The path-node edges, by node name. Barrier and one-way edges run from the first node to
    /// the second. Every edge is a straight walk clear of walls and Props, and a two-way edge
    /// never climbs a Ledge.
    pub nav_edges: &'static [(&'static str, &'static str, Gate)],
    /// The Jump Pads.
    pub jump_pads: &'static [JumpPad],
    /// When the Sappers come.
    pub sapper: SapperTiming,
    /// The Sections, each with the Zones its Generator Powers. A Zone is in at most one, and many
    /// are in none. Empty on a Map without Generators.
    pub sections: &'static [(SectionId, &'static [ZoneId])],
    /// The Generators, one per Section. A Map with any never turns its `Power` on and has no
    /// Power Switch.
    pub generators: &'static [Generator],
    /// Whether the Map has Darkness: no ambient light, close black fog, and a Zone's lamps on
    /// only while its Section is Powered (see `darkness`).
    pub darkness: bool,
    /// The Zones out of doors: open to the sky, with no ceiling, light probe or dim fog. Every
    /// other Zone is indoors.
    pub outdoor: &'static [ZoneId],
    /// Whether the Map has ceilings over its indoor Zones, and lights them with spot lamps, light
    /// probes and fog that follows the eye's Zone. The Greybox so far.
    pub roofed: bool,
    /// The way the moonlight falls.
    pub sun: Vec3,
    /// How the Map is lit and graded.
    pub look: crate::maps::look::Look,
    /// The corners (x, z) of the patch of the world the Map takes up: everything playable is
    /// inside it, and past its edge is Out of Bounds.
    pub plan: (Vec2, Vec2),
    /// Builds the walls, the Doors and everything else the data above doesn't cover.
    build: fn(&mut Builder),
}

impl Layout {
    /// Whether `zone` is indoors: under a roof (once its Map builds ceilings), not open to the sky.
    pub fn indoor(&self, zone: ZoneId) -> bool {
        !self.outdoor.contains(&zone)
    }

    /// The Zone every Run starts in.
    pub fn start_zone(&self) -> ZoneId {
        self.zones[0]
    }

    /// Where the Player in Seat `seat` starts.
    pub fn seat_start(&self, seat: u8) -> Vec3 {
        self.seats[usize::from(seat) % self.seats.len()]
    }

    /// The height of the Map's lowest floor: its lowest Zone's, or the bottom of its deepest
    /// sunken floor.
    pub fn lowest_floor(&self) -> f32 {
        let zones = self.zones.iter().map(|zone| zone.floor());
        let sunken = self.sunken.iter().map(|sunken| sunken.zone.floor() - sunken.depth);
        zones.chain(sunken).fold(f32::INFINITY, f32::min)
    }

    /// Whether `at` is Out of Bounds: outside the Map's plan, in Deep Water, or more than
    /// `OUT_OF_BOUNDS_DROP` below its lowest floor.
    pub fn out_of_bounds(&self, at: Vec3) -> bool {
        let (min, max) = self.plan;
        at.x < min.x
            || at.x > max.x
            || at.z < min.y
            || at.z > max.y
            || at.y < self.lowest_floor() - OUT_OF_BOUNDS_DROP
            || water_at(self, TideLevel::default(), at) == Some(Wading::Deep)
    }
}

/// Two arrays end to end.
const fn concat<T: Copy, const A: usize, const B: usize, const N: usize>(a: [T; A], b: [T; B]) -> [T; N] {
    assert!(A + B == N && A > 0);
    let mut out = [a[0]; N];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    while i < N {
        out[i] = b[i - A];
        i += 1;
    }
    out
}

/// Every Map's windows, each Map's in a run of its own in `MapId::ALL` order, so a `BarrierId`
/// names one window in one Map: the Zone it lets into, its centre on the wall line at the height
/// of the floor it lets onto, and the way out. Each is planked up by its Barrier.
pub const WINDOWS: [(ZoneId, Vec3, Vec2); BEFORE_THE_COVE_WINDOWS.len() + cove::WINDOWS.len()] = concat(BEFORE_THE_COVE_WINDOWS, cove::WINDOWS);
const BEFORE_THE_COVE_WINDOWS: [(ZoneId, Vec3, Vec2); BEFORE_THE_DAM_WINDOWS.len() + dam::WINDOWS.len()] = concat(BEFORE_THE_DAM_WINDOWS, dam::WINDOWS);
const BEFORE_THE_DAM_WINDOWS: [(ZoneId, Vec3, Vec2); GREYBOX_AND_QUARRY_WINDOWS.len() + village::WINDOWS.len()] = concat(GREYBOX_AND_QUARRY_WINDOWS, village::WINDOWS);
const GREYBOX_AND_QUARRY_WINDOWS: [(ZoneId, Vec3, Vec2); greybox::WINDOWS.len() + quarry::WINDOWS.len()] = concat(greybox::WINDOWS, quarry::WINDOWS);

/// Every Map's Box Locations, each Map's in a run of its own, in order: the Zone, where the Box's
/// centre sits on the floor, and which way its front faces (where the Player stands). Every Run
/// starts the Box at its Map's first.
pub const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); BEFORE_THE_COVE_BOXES.len() + cove::BOX_LOCATIONS.len()] = concat(BEFORE_THE_COVE_BOXES, cove::BOX_LOCATIONS);
const BEFORE_THE_COVE_BOXES: [(ZoneId, Vec3, Vec2); BEFORE_THE_DAM_BOXES.len() + dam::BOX_LOCATIONS.len()] = concat(BEFORE_THE_DAM_BOXES, dam::BOX_LOCATIONS);
const BEFORE_THE_DAM_BOXES: [(ZoneId, Vec3, Vec2); GREYBOX_AND_QUARRY_BOXES.len() + village::BOX_LOCATIONS.len()] =
    concat(GREYBOX_AND_QUARRY_BOXES, village::BOX_LOCATIONS);
const GREYBOX_AND_QUARRY_BOXES: [(ZoneId, Vec3, Vec2); greybox::BOX_LOCATIONS.len() + quarry::BOX_LOCATIONS.len()] =
    concat(greybox::BOX_LOCATIONS, quarry::BOX_LOCATIONS);

impl MapId {
    /// The Map's windows, as Barriers.
    pub fn barriers(self) -> impl Iterator<Item = BarrierId> {
        (0..WINDOWS.len()).filter(move |&i| WINDOWS[i].0.map() == self).map(|i| BarrierId(i as u8))
    }

    /// The Map's Box Locations, as indices into `BOX_LOCATIONS`; the first is where every Run
    /// starts the Box.
    pub fn box_locations(self) -> std::ops::Range<usize> {
        let first = BOX_LOCATIONS.iter().position(|(zone, ..)| zone.map() == self).expect("every Map has Box Locations");
        let count = BOX_LOCATIONS.iter().filter(|(zone, ..)| zone.map() == self).count();
        first..first + count
    }
}

/// The middle of a window on the wall line, at the height of the floor it lets onto.
pub fn window_centre(barrier: BarrierId) -> Vec3 {
    WINDOWS[barrier.0 as usize].1
}

/// Where a window's Spawn Point stands: on the floor just outside it.
pub fn window_spawn(barrier: BarrierId) -> Vec3 {
    let (_, _, out) = WINDOWS[barrier.0 as usize];
    window_centre(barrier) + Vec3::new(out.x, 0.0, out.y) * WINDOW_SPAWN_OUT
}

const WINDOW_WIDTH: f32 = 1.6;
/// How far outside its window a window Spawn Point sits.
const WINDOW_SPAWN_OUT: f32 = 2.0;

/// One of the windows, each the home of one Barrier (an index into `WINDOWS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarrierId(pub u8);

/// A Zone's look: flat colours for its floor, its walls and the trim along their foot and top, and
/// the tint of its lamps. The outside has one too, for the outer walls' faces and the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub floor: Color,
    pub wall: Color,
    pub trim: Color,
    pub light: Color,
}

const fn paints(floor: (f32, f32, f32), wall: (f32, f32, f32), trim: (f32, f32, f32), light: (f32, f32, f32)) -> Palette {
    Palette {
        floor: Color::srgb(floor.0, floor.1, floor.2),
        wall: Color::srgb(wall.0, wall.1, wall.2),
        trim: Color::srgb(trim.0, trim.1, trim.2),
        light: Color::srgb(light.0, light.1, light.2),
    }
}

/// What a Zone's floor and walls wear in place of their palette colours, once the files have
/// loaded. Trim stays in paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSet {
    pub floor: textures::Material,
    pub wall: textures::Material,
    /// The decals its walls or floor wear, if any.
    pub decals: Option<decals::DecalFamily>,
    /// The Props it dresses differently from `default_variants`, each kind with its variants.
    pub props: &'static [(PropKind, &'static [PropVariant])],
}

/// One look of a Prop kind in a Zone: the unit model laid over the Prop's box if it has one and the
/// file is on disk and fits (see `fitting`), and the material the textured box wears otherwise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropVariant {
    /// A unit glTF under `assets/` and its size as modelled, if there's a model yet.
    pub model: Option<(&'static str, Vec3)>,
    pub material: textures::Material,
}

const fn boxed(material: textures::Material) -> PropVariant {
    PropVariant { model: None, material }
}

/// How a kind looks in a Zone whose set doesn't name it: a textured box (a Pillar's is its Zone's
/// wall, set by `prop_variant`).
fn default_variants(kind: PropKind) -> &'static [PropVariant] {
    use textures::Material::*;
    const PLANKS: &[PropVariant] = &[boxed(WeatheredPlanks)];
    const STONE: &[PropVariant] = &[boxed(RockWall08)];
    const PLASTER: &[PropVariant] = &[boxed(PlasteredStoneWall)];
    const BROKEN: &[PropVariant] = &[boxed(BrokenWall)];
    const STEEL: &[PropVariant] = &[boxed(RustyMetal02)];
    const BARK: &[PropVariant] = &[boxed(BarkBrown02)];
    match kind {
        PropKind::Crate | PropKind::Table | PropKind::Bench | PropKind::Counter | PropKind::Shelf => PLANKS,
        PropKind::Altar => STONE,
        PropKind::Pillar => PLASTER,
        PropKind::Rubble => BROKEN,
        PropKind::Barrel | PropKind::Machine => STEEL,
        PropKind::Tree => BARK,
        PropKind::Sandbags | PropKind::Headstone => &[],
    }
}

/// The look of Prop number `nth` of `kind` in `zone`: the Zone's set names its variants and `nth`
/// picks between them, falling back to the kind's default. A Pillar wears its Zone's wall.
/// `None` for a kind that stays in paint.
pub fn prop_variant(zone: ZoneId, kind: PropKind, nth: usize) -> Option<PropVariant> {
    let set = materials(Some(zone));
    let listed = set.and_then(|set| set.props.iter().find(|(k, _)| *k == kind).map(|(_, variants)| *variants));
    let variants = listed.unwrap_or_else(|| default_variants(kind));
    let mut variant = *variants.get(nth % variants.len().max(1))?;
    if kind == PropKind::Pillar && listed.is_none() {
        if let Some(set) = set {
            variant.material = set.wall;
        }
    }
    Some(variant)
}

/// Workbenches, racks and counters are steel in the Workshop.
const WORKSHOP_PROPS: &[(PropKind, &[PropVariant])] = &[
    (PropKind::Table, &[boxed(textures::Material::RustyMetal02)]),
    (PropKind::Shelf, &[boxed(textures::Material::RustyMetal02)]),
    (PropKind::Counter, &[boxed(textures::Material::RustyMetal02)]),
];

/// The material set of a Zone, or of the outside for `None`. Neighbouring Zones differ by family
/// (plaster, tile, brick, tin, stone) more than by colour. Zones without one stay in their palette.
pub fn materials(zone: Option<ZoneId>) -> Option<MaterialSet> {
    use decals::DecalFamily::*;
    use textures::Material::*;
    use ZoneId::*;
    let (floor, wall, decals, props): (_, _, _, &'static [(PropKind, &'static [PropVariant])]) = match zone? {
        // Ruined: bare dark boards and plaster coming off the lath, streaked with damp.
        StartRoom => (DarkWoodenPlanks, DamagedPlaster, Some(Leaks), &[]),
        // Tiled: chequered tiles under damp, mossy plaster.
        WestHall => (FloorTiles06, WornMossyPlasterwall, Some(Leaks), &[]),
        // Peeling: diagonal parquet, cracked red paint.
        DiningRoom => (DiagonalParquet, PeelingPaintedWall, Some(Leaks), &[]),
        // Brick underfoot and either side.
        Passage => (WornBrickFloor, BrickWall001, None, &[]),
        // Rusty tin: concrete with oil on it, and corrugated iron.
        Workshop => (ConcreteFloor02, RustyCorrugatedIron, Some(OilStains), WORKSHOP_PROPS),
        // Cobbled: mossy cobbles (the sunken garden too), and the house's plastered stone outer faces.
        Yard => (MossyCobblestone, PlasteredStoneWall, None, &[]),
        // The Dam, in families that change from Zone to Zone: asphalt and precast slab on the crest,
        // poured concrete in the galleries (ribbed, layered, mossy), painted steel in the Control
        // Room and Switchgear Room, oily slab and tile in the Turbine Hall, rough-cast in the rest.
        CrestRoad => (AsphaltFloor, ConcreteSlabWall02, None, &[]),
        TowerHead => (ConcreteFloor02, ConcreteWall006, None, &[]),
        SpillwayGatehouse => (PaintedConcrete, ConcreteWall007, None, &[]),
        GateShaft => (DamagedConcreteFloor03, RibbedConcreteWall, Some(Leaks), &[]),
        IntakeGallery => (SmoothConcreteFloor, ConcreteLayers02, Some(Leaks), &[]),
        SpillwayGallery => (BrushedConcrete04, ConcreteWall008, Some(Leaks), &[]),
        ControlRoom => (OldMosaicFloor, BlueMetalPlate, None, &[]),
        SwitchgearRoom => (MetalPlate02, GreenMetalRust, None, &[]),
        PenstockTunnel => (GranularConcrete, RockEmbeddedConcreteWall, None, &[]),
        TurbineHall => (HangarConcreteFloor, ConcreteTileFacade, Some(OilStains), &[]),
        SpillwayStair => (WornConcreteFloor, ConcreteBlockWall02, None, &[]),
        DrainageGallery => (CrackedConcrete, ConcreteMoss, Some(Leaks), &[]),
        // The Greybox's far side. East Hall is a chapel: mosaic under pale sandstone blocks, damp
        // coming through. The Power Room is steel plate over oily hangar concrete, to set it off
        // from the chapel and the Dining Room beside it. The Tunnel is wet, mossy poured concrete.
        EastHall => (OldMosaicFloor, WhiteSandstoneBricks03, Some(Leaks), &[]),
        PowerRoom => (HangarConcreteFloor, BlueMetalPlate, Some(OilStains), &[]),
        Tunnel => (WornConcreteFloor, ConcreteMoss, Some(Leaks), &[]),
        // The south wing, Workshop (tin) to Storeroom (whitewash on boards) to Generator Room (steel
        // plate under rusted green panels).
        Storeroom => (WeatheredPlanks, WhitePlasterRough01, None, &[]),
        GeneratorRoom => (MetalPlate02, GreenMetalRust, None, &[]),
        // The Balcony: boards under broken render, open to the night.
        Balcony => (WeatheredPlanks, BrokenWall, None, &[]),
        // The Village, in families that change from street to street: cobbles in the Market,
        // rutted mud in the Ends (between rendered fronts, the beams being trim), grass and flint
        // in the Back Lane, brick along the Mill Lane, hedges along the Church Lane; dark boards in
        // the Inn, flags in the Church, grass over the Orchard and the Graveyard.
        Inn => (DarkWoodenPlanks, WeatheredPlanks, None, &[]),
        Market => (MossyCobblestone, PlasteredStoneWall, None, &[]),
        WestEnd => (MuddyTracks, DamagedPlaster, None, &[]),
        EastEnd => (MuddyTracks, PeelingPaintedWall, None, &[]),
        BackLane => (LeafyGrass, RockWall05, None, &[]),
        MillLane => (MuddyTracks, BrickWall001, None, &[]),
        ChurchLane => (MossyCobblestone, LeafyGrass, None, &[]),
        Mill => (ConcreteFloor02, WornMossyPlasterwall, Some(Leaks), &[]),
        Smithy => (DamagedConcreteFloor03, RockWall08, None, &[]),
        Bakery => (WornBrickFloor, PlasteredStoneWall, None, &[]),
        Schoolhouse => (DiagonalParquet, DamagedPlaster, None, &[]),
        Church => (SlateFloor02, RockWall08, Some(Leaks), &[]),
        Orchard => (LeafyGrass, RockWall05, None, &[]),
        Graveyard => (GrassGround, RockWall08, None, &[]),
        // The Cove, a Mediterranean fishing town, in families that change from Zone to Zone: sand,
        // shingle and cave rock on the Shore; weathered timber in the Boathouse; whitewash and ochre
        // render over cobbles and terracotta in the town; whitewashed towers and grey headland stone.
        Strand => (CoastSand04, PlasteredStoneWall, None, &[]),
        SeaCave => (RockWall08, RockWall08, Some(Leaks), &[]),
        Shingle => (SandyGravel02, WornMossyPlasterwall, None, &[]),
        Boathouse => (WeatheredPlanks, WeatheredPlanks, None, &[]),
        Washhouse => (MossyCobblestone, WhiteStucco, Some(Leaks), &[]),
        LowerLane => (CobblestoneFloor02, WhiteStucco, None, &[]),
        FishMarket => (WornBrickFloor, PlasteredStoneWall, None, &[]),
        Taverna => (TerracottaFloorTiles, YellowStoneWall, None, &[]),
        Chapel => (FloorTiles06, WhiteStucco, None, &[]),
        Piazza => (MossyCobblestone, YellowStoneWall, None, &[]),
        UpperLane => (CobblestoneFloor02, PlasteredStoneWall, None, &[]),
        Pensione => (DiagonalParquet, YellowStoneWall, None, &[]),
        Lighthouse => (OldMosaicFloor, WhiteStucco, None, &[]),
        LighthouseYard => (SandyGravel02, RockWall08, None, &[]),
        KeepersCottage => (DarkWoodenPlanks, WhiteStucco, None, &[]),
        Battery => (CobblestoneFloor02, RockWall08, None, &[]),
        // The Quarry, pit to headframe: raw rock over pale grit, ballast under soot-black timber,
        // corrugated iron and brass tile in the sheds, the offices' parquet, clay-grey rock cutting.
        PumpHouse => (CrackedConcrete, BrickWall001, Some(Leaks), &[]),
        QuarryFloor => (GravelGround01, DarkRock, None, &[]),
        RailYard => (GrayRocks, BlackPaintedPlanks, None, &[]),
        CrusherShed => (HangarConcreteFloor, RustyCorrugatedIron, Some(OilStains), &[]),
        WindingHouse => (CheckeredPavementTiles, ConcreteTileFacade, None, &[]),
        Offices => (DiagonalParquet, PeelingPaintedWall, Some(Leaks), &[]),
        Headframe => (MetalPlate02, BlueMetalPlate, None, &[]),
        Incline => (WeatheredPlanks, RockWall08, None, &[]),
        LoadingDock => (WornConcreteFloor, GreenMetalRust, Some(OilStains), &[]),
        _ => return None,
    };
    Some(MaterialSet { floor, wall, decals, props })
}

/// The palette of a Zone, or of the outside for `None`.
pub fn palette(zone: Option<ZoneId>) -> Palette {
    use ZoneId::*;
    match zone {
        // An entrance hall: dark oak, ochre plaster, oxblood skirting, warm lamps.
        Some(StartRoom) => paints((0.3, 0.19, 0.12), (0.62, 0.5, 0.3), (0.42, 0.11, 0.09), (1.0, 0.78, 0.5)),
        // A library: slate floor, sage-green walls, cream trim.
        Some(WestHall) => paints((0.2, 0.22, 0.26), (0.34, 0.45, 0.34), (0.76, 0.71, 0.56), (1.0, 0.86, 0.62)),
        // A timber walkway with iron rails, under the moon.
        Some(Balcony) => paints((0.42, 0.32, 0.21), (0.27, 0.3, 0.27), (0.12, 0.13, 0.14), (0.75, 0.82, 1.0)),
        // A chapel: blue-grey flags, pale stone, deep blue trim, candlelight.
        Some(EastHall) => paints((0.29, 0.31, 0.36), (0.63, 0.61, 0.55), (0.15, 0.21, 0.42), (1.0, 0.72, 0.42)),
        // Parquet, burgundy walls and gilt.
        Some(DiningRoom) => paints((0.37, 0.24, 0.14), (0.47, 0.16, 0.18), (0.74, 0.57, 0.22), (1.0, 0.8, 0.52)),
        // Oil-stained concrete, battleship grey, hazard yellow, cold light.
        Some(PowerRoom) => paints((0.16, 0.16, 0.17), (0.38, 0.42, 0.48), (0.86, 0.68, 0.1), (0.78, 0.9, 1.0)),
        // Grass and dirt between brick walls with stone copings, under the moon.
        Some(Yard) => paints((0.19, 0.24, 0.13), (0.45, 0.27, 0.19), (0.56, 0.54, 0.48), (0.7, 0.8, 1.0)),
        // Flagstones and sooty brick.
        Some(Passage) => paints((0.31, 0.29, 0.26), (0.33, 0.2, 0.15), (0.14, 0.12, 0.11), (1.0, 0.64, 0.34)),
        // Bare concrete, tan board walls, safety orange.
        Some(Workshop) => paints((0.33, 0.32, 0.3), (0.55, 0.45, 0.31), (0.82, 0.4, 0.1), (1.0, 0.84, 0.58)),
        // Dusty boards, whitewash, olive drab.
        Some(Storeroom) => paints((0.29, 0.25, 0.19), (0.66, 0.66, 0.6), (0.31, 0.35, 0.19), (0.95, 0.9, 0.76)),
        // Steel plate, teal walls, red trim, sodium light.
        Some(GeneratorRoom) => paints((0.24, 0.26, 0.28), (0.17, 0.35, 0.33), (0.62, 0.13, 0.11), (1.0, 0.6, 0.26)),
        // Wet concrete, blue-grey walls, green pipework, a sickly light.
        Some(Tunnel) => paints((0.17, 0.18, 0.19), (0.29, 0.33, 0.38), (0.25, 0.55, 0.3), (0.72, 1.0, 0.62)),
        // Red brick, slate flags, pale stone dressings.
        Some(PumpHouse) => paints((0.25, 0.27, 0.3), (0.55, 0.3, 0.24), (0.82, 0.78, 0.66), (1.0, 0.82, 0.55)),
        // Pale grit under raw grey rock faces.
        Some(QuarryFloor) => paints((0.48, 0.44, 0.36), (0.42, 0.41, 0.4), (0.3, 0.26, 0.22), (0.72, 0.8, 1.0)),
        // Rusty ballast and soot-black timber.
        Some(RailYard) => paints((0.3, 0.22, 0.16), (0.16, 0.14, 0.13), (0.55, 0.3, 0.14), (0.75, 0.82, 1.0)),
        // Corrugated iron, rust-red, with stone dust on everything.
        Some(CrusherShed) => paints((0.55, 0.53, 0.5), (0.5, 0.24, 0.16), (0.86, 0.84, 0.78), (1.0, 0.86, 0.6)),
        // Polished brass and black-and-cream tiles in an engine house.
        Some(WindingHouse) => paints((0.16, 0.15, 0.14), (0.76, 0.7, 0.55), (0.62, 0.47, 0.16), (1.0, 0.75, 0.45)),
        // Brown linoleum, duck-egg blue walls, dark varnished wood.
        Some(Offices) => paints((0.33, 0.24, 0.17), (0.48, 0.64, 0.66), (0.22, 0.13, 0.08), (0.95, 0.92, 0.78)),
        // Black steel deck and red-oxide girders against the sky.
        Some(Headframe) => paints((0.09, 0.09, 0.1), (0.5, 0.16, 0.12), (0.84, 0.82, 0.76), (0.7, 0.78, 1.0)),
        // Trodden clay between ochre rock walls shored with pale timber, by lantern light.
        Some(Incline) => paints((0.28, 0.2, 0.14), (0.58, 0.42, 0.22), (0.86, 0.74, 0.52), (1.0, 0.68, 0.38)),
        // Oily concrete, green-painted corrugated sheeting, hazard yellow, cold floodlight.
        Some(LoadingDock) => paints((0.4, 0.4, 0.38), (0.2, 0.36, 0.26), (0.9, 0.76, 0.12), (0.82, 0.92, 1.0)),
        // Wide dark boards, plaster gone the colour of pipe smoke, oak beams, a fire's glow.
        Some(Inn) => paints((0.24, 0.16, 0.1), (0.7, 0.56, 0.4), (0.2, 0.12, 0.07), (1.0, 0.7, 0.4)),
        // Cobbles between cream-rendered, black-beamed house fronts, under gas lamps.
        Some(Market) => paints((0.36, 0.34, 0.32), (0.82, 0.74, 0.56), (0.14, 0.11, 0.09), (1.0, 0.78, 0.5)),
        // A rutted road between ochre house fronts with brick dressings.
        Some(WestEnd) => paints((0.27, 0.22, 0.16), (0.72, 0.58, 0.26), (0.5, 0.2, 0.14), (1.0, 0.8, 0.52)),
        // A rutted road between dusty pink house fronts with white dressings.
        Some(EastEnd) => paints((0.3, 0.27, 0.22), (0.72, 0.5, 0.5), (0.92, 0.9, 0.84), (1.0, 0.8, 0.55)),
        // A grassy track between flint walls with pale copings, under the moon.
        Some(BackLane) => paints((0.24, 0.3, 0.14), (0.3, 0.31, 0.36), (0.62, 0.6, 0.52), (1.0, 0.78, 0.5)),
        // A cart track between old brick walls.
        Some(MillLane) => paints((0.32, 0.26, 0.18), (0.42, 0.3, 0.22), (0.7, 0.68, 0.6), (1.0, 0.78, 0.5)),
        // Gravel between clipped yew hedges.
        Some(ChurchLane) => paints((0.42, 0.4, 0.36), (0.14, 0.24, 0.15), (0.6, 0.58, 0.5), (1.0, 0.78, 0.5)),
        // Worn flags, limewashed boards, tarred timber, by lantern.
        Some(Mill) => paints((0.4, 0.38, 0.33), (0.6, 0.55, 0.44), (0.36, 0.24, 0.13), (1.0, 0.8, 0.55)),
        // Beaten earth and soot-black stone, lit orange by the forge.
        Some(Smithy) => paints((0.1, 0.09, 0.09), (0.24, 0.24, 0.27), (0.85, 0.38, 0.08), (1.0, 0.55, 0.25)),
        // Terracotta tiles, flour-white walls, blue-grey woodwork.
        Some(Bakery) => paints((0.5, 0.28, 0.18), (0.85, 0.82, 0.74), (0.3, 0.36, 0.46), (1.0, 0.85, 0.6)),
        // Pine boards, pale green walls, a blackboard-dark dado.
        Some(Schoolhouse) => paints((0.52, 0.4, 0.26), (0.6, 0.7, 0.58), (0.12, 0.18, 0.14), (0.95, 0.95, 0.85)),
        // Stone flags, pale limestone, altar-cloth red, candlelight.
        Some(Church) => paints((0.45, 0.45, 0.48), (0.78, 0.76, 0.7), (0.5, 0.08, 0.1), (1.0, 0.72, 0.45)),
        // Long grass inside dry-stone walls, under the moon.
        Some(Orchard) => paints((0.16, 0.22, 0.1), (0.5, 0.47, 0.4), (0.3, 0.22, 0.14), (1.0, 0.78, 0.5)),
        // Dark churchyard grass inside lichened grey walls, under a cold moon.
        Some(Graveyard) => paints((0.12, 0.17, 0.11), (0.52, 0.5, 0.56), (0.2, 0.2, 0.24), (1.0, 0.78, 0.5)),
        // Asphalt between pale concrete parapets with hazard-yellow kerbs, under the moon.
        Some(CrestRoad) => paints((0.08, 0.08, 0.1), (0.66, 0.64, 0.58), (0.85, 0.75, 0.1), (0.75, 0.82, 1.0)),
        // Chequer plate and steel-blue panelling, white trim.
        Some(TowerHead) => paints((0.35, 0.33, 0.3), (0.45, 0.52, 0.6), (0.9, 0.9, 0.88), (1.0, 0.85, 0.6)),
        // Grimy concrete steps between green-grey walls, black handrail paint.
        Some(GateShaft) => paints((0.25, 0.25, 0.22), (0.55, 0.62, 0.45), (0.2, 0.2, 0.2), (0.9, 1.0, 0.8)),
        // Wet concrete and teal-blue walls, a pale stripe at waist height.
        Some(IntakeGallery) => paints((0.2, 0.24, 0.26), (0.3, 0.42, 0.48), (0.75, 0.7, 0.6), (0.7, 0.9, 1.0)),
        // Worn boards and terracotta render, dark timber.
        Some(SpillwayGatehouse) => paints((0.38, 0.3, 0.24), (0.68, 0.45, 0.37), (0.2, 0.15, 0.1), (1.0, 0.8, 0.55)),
        // Stained concrete and faded mauve walls, cream trim.
        Some(SpillwayGallery) => paints((0.26, 0.22, 0.2), (0.5, 0.38, 0.5), (0.85, 0.85, 0.8), (1.0, 0.85, 0.75)),
        // Ochre safety paint over bare concrete, black edging.
        Some(SpillwayStair) => paints((0.4, 0.38, 0.34), (0.7, 0.68, 0.45), (0.15, 0.15, 0.18), (1.0, 0.95, 0.8)),
        // Green linoleum and off-white walls, a dark green dado.
        Some(ControlRoom) => paints((0.15, 0.25, 0.2), (0.8, 0.82, 0.78), (0.1, 0.3, 0.25), (0.9, 0.95, 1.0)),
        // Black mud on the floor and walls stained a rusty maroon.
        Some(PenstockTunnel) => paints((0.18, 0.15, 0.13), (0.38, 0.21, 0.3), (0.7, 0.55, 0.3), (1.0, 0.6, 0.3)),
        // Grey floor paint, machine-blue walls, safety orange.
        Some(TurbineHall) => paints((0.3, 0.32, 0.36), (0.3, 0.36, 0.5), (0.9, 0.45, 0.1), (0.85, 0.9, 1.0)),
        // Rubber matting and switchgear green, yellow warning trim.
        Some(SwitchgearRoom) => paints((0.12, 0.12, 0.14), (0.25, 0.42, 0.28), (0.9, 0.8, 0.2), (0.8, 1.0, 0.85)),
        // Silt on the floor and damp, pale blue-grey walls, a teal tide line.
        Some(DrainageGallery) => paints((0.22, 0.2, 0.16), (0.6, 0.72, 0.72), (0.15, 0.4, 0.45), (0.75, 1.0, 0.9)),
        // Sun-bleached sand and shell under the cave's wet grey rock, a green tide line.
        Some(SeaCave) => paints((0.52, 0.47, 0.38), (0.24, 0.27, 0.3), (0.15, 0.42, 0.4), (0.55, 0.85, 0.95)),
        // Grey pebbles against the tarred planks of the fishermen's huts.
        Some(Shingle) => paints((0.4, 0.4, 0.4), (0.3, 0.22, 0.16), (0.8, 0.78, 0.7), (0.7, 0.82, 1.0)),
        // Pale sand under a whitewashed sea wall, with a blue band.
        Some(Strand) => paints((0.75, 0.67, 0.5), (0.9, 0.88, 0.82), (0.16, 0.38, 0.62), (0.8, 0.88, 1.0)),
        // Weathered boards, red-ochre planking, rope-pale trim.
        Some(Boathouse) => paints((0.28, 0.33, 0.4), (0.74, 0.8, 0.82), (0.78, 0.45, 0.2), (1.0, 0.82, 0.55)),
        // Wet flags, sea-green plaster and white limewash.
        Some(Washhouse) => paints((0.34, 0.4, 0.4), (0.5, 0.66, 0.55), (0.88, 0.9, 0.86), (0.9, 1.0, 0.9)),
        // Worn cobbles between rose-pink house fronts.
        Some(LowerLane) => paints((0.43, 0.38, 0.34), (0.82, 0.55, 0.5), (0.35, 0.2, 0.18), (1.0, 0.78, 0.55)),
        // Marble slabs and ice-wet stone under a slate-blue loggia.
        Some(FishMarket) => paints((0.6, 0.62, 0.64), (0.4, 0.5, 0.64), (0.85, 0.85, 0.8), (0.85, 0.92, 1.0)),
        // Terracotta tiles, mustard walls, dark vine-green trim.
        Some(Taverna) => paints((0.6, 0.35, 0.22), (0.86, 0.68, 0.28), (0.2, 0.3, 0.16), (1.0, 0.72, 0.4)),
        // Cold stone flags, honey limestone, oxblood trim, candles.
        Some(Chapel) => paints((0.4, 0.4, 0.42), (0.62, 0.6, 0.7), (0.4, 0.1, 0.12), (1.0, 0.75, 0.45)),
        // Pale paving round the fountain, sun-baked ochre frontages.
        Some(Piazza) => paints((0.7, 0.66, 0.58), (0.84, 0.64, 0.38), (0.5, 0.25, 0.15), (1.0, 0.82, 0.55)),
        // Packed earth between lilac-grey house fronts.
        Some(UpperLane) => paints((0.5, 0.42, 0.34), (0.62, 0.58, 0.7), (0.9, 0.88, 0.8), (1.0, 0.8, 0.55)),
        // Polished boards, apricot plaster, teal shutters.
        Some(Pensione) => paints((0.46, 0.32, 0.2), (0.92, 0.72, 0.55), (0.14, 0.46, 0.46), (1.0, 0.85, 0.6)),
        // White-painted concrete tower with a red band, a cold lamp.
        Some(Lighthouse) => paints((0.38, 0.38, 0.4), (0.94, 0.94, 0.92), (0.8, 0.12, 0.1), (0.9, 0.95, 1.0)),
        // Short wind-flattened grass and grey flags between pale stone walls.
        Some(LighthouseYard) => paints((0.26, 0.32, 0.2), (0.7, 0.72, 0.7), (0.35, 0.35, 0.38), (0.75, 0.85, 1.0)),
        // Scrubbed boards and slate-green walls, brass trim.
        Some(KeepersCottage) => paints((0.4, 0.3, 0.2), (0.34, 0.5, 0.48), (0.7, 0.55, 0.2), (1.0, 0.78, 0.5)),
        // Dark flags, pale sandstone ramparts and black iron.
        Some(Battery) => paints((0.3, 0.3, 0.33), (0.28, 0.26, 0.42), (0.86, 0.82, 0.7), (0.8, 0.88, 1.0)),
        // Bare ground round weathered brick.
        None => paints((0.15, 0.17, 0.12), (0.3, 0.21, 0.17), (0.42, 0.4, 0.36), (1.0, 0.75, 0.45)),
    }
}

/// A Raised Floor standing on its Zone's floor: a stage, a dais, a dock. It covers `min`..`max`
/// (x, z), its top at `height`, solid down to its Zone's floor. Its edges are Ledges the Player can hop down from; Slopes lead up
/// onto it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaisedFloor {
    pub zone: ZoneId,
    pub min: Vec2,
    pub max: Vec2,
    pub height: f32,
    pub colour: Color,
}

/// A flight of steps or a ramp between two floors. The middle of its foot is `foot`, on the lower
/// floor, and it climbs `rise` over `run` towards `up` (along x or z), `width` across. Either way
/// it's solid as a smooth ramp, so nothing trips on a step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slope {
    pub zone: ZoneId,
    pub foot: Vec3,
    pub up: Vec2,
    pub run: f32,
    pub rise: f32,
    pub width: f32,
    /// Drawn as steps rather than a plain ramp.
    pub steps: bool,
    pub colour: Color,
}

/// A sunken floor, `depth` below its Zone's floor across `min`..`max` (x, z). Its edges are
/// Ledges the Player can hop down, and jump back up; Slopes lead down into it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sunken {
    pub zone: ZoneId,
    pub min: Vec2,
    pub max: Vec2,
    pub depth: f32,
    /// A stone coping round its rim.
    pub rim: bool,
}

/// What a Prop is, which sets how it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    /// A wooden crate, or a stack of them.
    Crate,
    /// A table or a workbench: a top on legs, though solid all the way down.
    Table,
    /// A bench or a pew.
    Bench,
    /// A counter.
    Counter,
    /// East Hall's altar, under a cloth.
    Altar,
    /// A square column, floor to ceiling.
    Pillar,
    /// A tall shelf or cabinet.
    Shelf,
    /// A heap of broken masonry.
    Rubble,
    /// A wall of sandbags.
    Sandbags,
    /// An oil drum, round.
    Barrel,
    /// A big machine: a transformer, the generator.
    Machine,
    /// A tree: its trunk is solid, its crown is drawn overhead, out of reach.
    Tree,
    /// A headstone.
    Headstone,
}

/// Something standing on a floor, for the Player to weave round or jump onto and for Zombies to
/// path round. `at` is the middle of its foot, on the floor it stands on; `size` is how wide (x),
/// tall and deep (z) it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prop {
    pub kind: PropKind,
    pub zone: ZoneId,
    pub at: Vec3,
    pub size: Vec3,
}

const fn prop(kind: PropKind, zone: ZoneId, at: (f32, f32, f32), size: (f32, f32, f32)) -> Prop {
    Prop {
        kind,
        zone,
        at: Vec3::new(at.0, at.1, at.2),
        size: Vec3::new(size.0, size.1, size.2),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnKind {
    /// Outside a window. The Zombie has to get in through that window's Barrier.
    Window(BarrierId),
    /// In open ground: the Zombie climbs up out of the dirt.
    Riser,
    /// On the bed under Shallow Water: the Zombie stands up out of the surf and wades ashore.
    Wading,
    /// Inside a Zone, where a Hellhound arrives in a lightning strike. Only hounds use these.
    Hellhound,
    /// Inside a Zone, where a Napalm or a Shrieker rises. Only they use these.
    Special,
    /// On the floor under a Vent, which a Nova Crawler drops from. Only Nova Crawlers use these.
    Vent,
}

/// A fixed place in the Map where Zombies enter play, in the Zone it serves.
#[derive(Component)]
pub struct SpawnPoint {
    pub zone: ZoneId,
    pub kind: SpawnKind,
}

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.6, 0.65, 0.8),
            brightness: AMBIENT_BRIGHTNESS,
            ..default()
        })
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
        .init_resource::<MapId>()
        .add_systems(OnEnter(GameState::Playing), spawn_map);
    }
}

const fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

fn door(a: ZoneId, b: ZoneId, price: u64) -> Gap {
    Gap::Door(Some((a, b)), Some(price))
}

/// Builds the Map the Run is played on.
pub fn spawn_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Res<MapId>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let layout = map.layout();
    let mut b = Builder::new(&mut commands, &mut meshes, &mut materials, layout.windows);

    // Floors: one per Zone, round any sunken floor in it.
    for &zone in layout.zones {
        let holes: Vec<Sunken> = layout.sunken.iter().copied().filter(|sunken| sunken.zone == zone).collect();
        b.floor(zone, &holes);
    }
    (layout.build)(&mut b);

    // Raised and sunken floors, and the steps and ramps between floors.
    for raised in layout.raised_floors {
        b.raised_floor(raised);
    }
    for sunken in layout.sunken {
        b.sunken(sunken);
    }
    for slope in layout.slopes {
        b.slope(slope);
    }
    for jump_pad in layout.jump_pads {
        b.jump_pad(jump_pad);
    }
    for (index, generator) in layout.generators.iter().enumerate() {
        b.generator(index, generator);
    }

    // The Props.
    for (nth, prop) in layout.props.iter().enumerate() {
        b.prop(prop, nth);
    }

    // Ceilings over the indoor Zones.
    // (A Map with Darkness builds its own, solid ones.)
    if layout.roofed && !layout.darkness {
        for &zone in layout.zones.iter().filter(|&&zone| layout.indoor(zone)) {
            b.ceiling(zone);
        }
    }

    // Water: the surfaces, and the walls round Deep Water.
    crate::maps::water::spawn_water(&mut commands, &mut meshes, &mut materials, layout);

    // Chalk outlines: a pale gun shape drawn on the wall, its Buyable just in front.
    let chalk_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.85),
        perceptual_roughness: 1.0,
        ..default()
    });
    let barrel = meshes.add(Cuboid::new(1.2, 0.12, 0.02));
    let grip = meshes.add(Cuboid::new(0.12, 0.3, 0.02));
    for &(weapon, zone, centre, facing) in layout.wall_weapons {
        let price = wall_price(weapon).expect("only Wall Weapons go on walls");
        let normal = Vec3::new(facing.x, 0.0, facing.y);
        // Lay the outline flat against the wall, its length along the wall.
        let rotation = Quat::from_rotation_arc(Vec3::Z, normal);
        commands
            .spawn((
                InRun,
                WallWeapon(weapon),
                priced_for(weapon),
                Offers::default(),
                Buyable {
                    use_point: centre + normal * 0.05,
                    prompt: String::new(),
                    price: Some(price),
                    range: USE_RANGE,
                },
                Chalk(zone),
                Mesh3d(barrel.clone()),
                MeshMaterial3d(chalk_mat.clone()),
                Transform::from_translation(centre + normal * 0.01).with_rotation(rotation),
            ))
            .with_child((
                Chalk(zone),
                Mesh3d(grip.clone()),
                MeshMaterial3d(chalk_mat.clone()),
                Transform::from_xyz(0.3, -0.2, 0.0),
            ));
    }

    // Spawn Points: one outside each window, the Yard's Risers, the Hellhound and Special Spawn
    // Points, and under each Vent, with the Vent itself: a dark duct opening up near the top of the room.
    for barrier in map.barriers() {
        let zone = &WINDOWS[barrier.0 as usize].0;
        commands.spawn((
            InRun,
            SpawnPoint {
                zone: *zone,
                kind: SpawnKind::Window(barrier),
            },
            Transform::from_translation(window_spawn(barrier)),
        ));
    }
    for &(zone, at) in layout.risers {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone,
                kind: SpawnKind::Riser,
            },
            Transform::from_translation(at),
        ));
    }
    for &(zone, at) in layout.wading {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone,
                kind: SpawnKind::Wading,
            },
            Transform::from_translation(at),
        ));
    }
    for &(zone, at) in layout.hellhound_spawns {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone,
                kind: SpawnKind::Hellhound,
            },
            Transform::from_translation(at),
        ));
    }
    for &(zone, at) in layout.special_spawns {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone,
                kind: SpawnKind::Special,
            },
            Transform::from_translation(at),
        ));
    }
    let vent_mesh = meshes.add(Cuboid::new(1.2, 0.3, 1.2));
    let vent_mat = materials.add(Color::srgb(0.08, 0.08, 0.09));
    for &(zone, at) in layout.vents {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone,
                kind: SpawnKind::Vent,
            },
            Transform::from_translation(at),
        ));
        commands.spawn((
            InRun,
            Mesh3d(vent_mesh.clone()),
            MeshMaterial3d(vent_mat.clone()),
            Transform::from_translation(at + Vec3::Y * (NOVA_VENT_HEIGHT + 0.15)),
        ));
    }

    // Lighting: the Look's sun, and a lamp in each room.
    let look = layout.look;
    commands.spawn((
        InRun,
        DirectionalLight {
            illuminance: look.sun_lux,
            shadow_maps_enabled: true,
            color: look.sun_colour,
            ..default()
        },
        look.cascade_config(),
        Transform::default().looking_at(look.sun_direction(layout.sun), Vec3::Y),
    ));
    if layout.roofed {
        // Warm downward spots, each with a glowing bulb, that light every indoor floor.
        let bulb = meshes.add(Sphere::new(0.12));
        // A street's gas lamp stands on a thin iron post, and lights less far than a room's.
        let post_mesh = meshes.add(Cuboid::new(0.12, 1.0, 0.12));
        let post_mat = materials.add(Color::srgb(0.05, 0.05, 0.055));
        for &(zone, at) in layout.lamps {
            let out_of_doors = !layout.indoor(zone);
            let colour = palette(Some(zone)).light;
            let glow = materials.add(StandardMaterial {
                base_color: colour,
                emissive: LinearRgba::from(colour) * 12.0,
                ..default()
            });
            commands
                .spawn((
                    InRun,
                    Lamp::in_zone(zone),
                    SpotLight {
                        color: colour,
                        intensity: SPOT_LAMP_INTENSITY * look.lamps,
                        range: if out_of_doors { OUTDOOR_LAMP_RANGE } else { 25.0 },
                        outer_angle: SPOT_LAMP_CONE,
                        inner_angle: SPOT_LAMP_CONE * 0.6,
                        ..default()
                    },
                    Transform::from_translation(at).looking_to(Vec3::NEG_Y, Vec3::X),
                ))
                .with_child((Mesh3d(bulb.clone()), MeshMaterial3d(glow), NotShadowCaster));
            if out_of_doors && !layout.darkness {
                let height = at.y - zone.floor() - 0.15;
                commands.spawn((
                    InRun,
                    Mesh3d(post_mesh.clone()),
                    MeshMaterial3d(post_mat.clone()),
                    Transform::from_translation(Vec3::new(at.x, zone.floor() + height / 2.0, at.z)).with_scale(Vec3::new(1.0, height, 1.0)),
                    NotShadowCaster,
                ));
            }
        }
        // A light probe in each indoor Zone, so the room is lit by its own dim constant colour and
        // not the sky's, however bright the Yard through its Door.
        // (None under Darkness, whose ambient light is zero.)
        if let (Some(mut images), false) = (images, layout.darkness) {
            let probe = EnvironmentMapLight::solid_color(&mut images, Color::srgb(0.75, 0.7, 0.65)).with_intensity(INDOOR_PROBE_BRIGHTNESS);
            for &zone in layout.zones.iter().filter(|&&zone| layout.indoor(zone)) {
                let (lo, hi) = zone.volume();
                let top = (hi.y / STOREY).ceil() * STOREY;
                let (lo, hi) = (Vec3::new(lo.x, zone.floor() - 0.25, lo.z), Vec3::new(hi.x, top + 0.3, hi.z));
                commands.spawn((
                    InRun,
                    LightProbe::default(),
                    probe.clone(),
                    Transform::from_translation((lo + hi) / 2.0).with_scale(hi - lo),
                ));
            }
        }
    } else {
        for &(zone, at) in layout.lamps {
            commands.spawn((
                InRun,
                Lamp::in_zone(zone),
                PointLight {
                    color: palette(Some(zone)).light,
                    intensity: LAMP_INTENSITY * look.lamps,
                    range: 25.0,
                    ..default()
                },
                Transform::from_translation(at),
            ));
        }
    }
}

#[cfg(test)]
mod plan;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::enemies::nav::{Gate, zone_at};
    use crate::enemies::zombie::headless::Sim;

    /// How far apart two colours are, as straight-line distance between their sRGB values.
    fn apart(a: Color, b: Color) -> f32 {
        let (a, b) = (a.to_srgba(), b.to_srgba());
        Vec3::new(a.red - b.red, a.green - b.green, a.blue - b.blue).length()
    }

    /// Whether `at` is inside `zone`'s volume.
    fn inside(zone: ZoneId, at: Vec3) -> bool {
        let (lo, hi) = zone.volume();
        at.cmpge(lo).all() && at.cmple(hi).all()
    }

    #[test]
    fn a_zones_set_names_its_prop_variants_and_nth_picks_between_them() {
        use textures::Material::*;
        // The Workshop's tables are steel; elsewhere in the house they are planks.
        assert_eq!(prop_variant(ZoneId::Workshop, PropKind::Table, 0).unwrap().material, RustyMetal02);
        assert_eq!(prop_variant(ZoneId::StartRoom, PropKind::Table, 0).unwrap().material, WeatheredPlanks);
        // A Pillar wears its Zone's wall.
        assert_eq!(prop_variant(ZoneId::Passage, PropKind::Pillar, 3).unwrap().material, BrickWall001);
        // Every kind but the two still in paint has a look in every Zone, and nth wraps round.
        for kind in [PropKind::Crate, PropKind::Table, PropKind::Bench, PropKind::Counter, PropKind::Altar, PropKind::Pillar, PropKind::Shelf, PropKind::Rubble, PropKind::Barrel, PropKind::Machine, PropKind::Tree] {
            assert!(prop_variant(ZoneId::StartRoom, kind, 7).is_some(), "{kind:?}");
        }
        assert!(prop_variant(ZoneId::StartRoom, PropKind::Sandbags, 0).is_none());
        // With two variants named, nth alternates.
        const TWO: &[PropVariant] = &[boxed(WeatheredPlanks), boxed(RustyMetal02)];
        let pick = |nth: usize| TWO[nth % TWO.len()].material;
        assert_eq!((pick(0), pick(1), pick(2)), (WeatheredPlanks, RustyMetal02, WeatheredPlanks));
    }

    #[test]
    fn every_map_marks_its_outdoor_zones_from_its_own_and_the_greybox_has_two() {
        for map in MapId::ALL {
            let layout = map.layout();
            for zone in layout.outdoor {
                assert!(layout.zones.contains(zone), "{map:?}'s outdoor {zone:?} isn't one of its Zones");
            }
            assert!(layout.outdoor.len() < layout.zones.len(), "{map:?} has an indoor Zone");
        }
        assert_eq!(MapId::Greybox.layout().outdoor, &[ZoneId::Yard, ZoneId::Balcony]);
        assert!(MapId::Greybox.layout().roofed);
    }

    #[test]
    fn every_point_of_every_village_zone_is_within_reach_of_a_lamp_in_its_zone() {
        // The streets, lanes and fields too: the Village is lit by gas lamps all over.
        let layout = MapId::Village.layout();
        for &zone in layout.zones {
            let (lo, hi) = zone.volume();
            let (lo, hi) = (Vec2::new(lo.x + 1.0, lo.z + 1.0), Vec2::new(hi.x - 1.0, hi.z - 1.0));
            let lamps: Vec<Vec2> = layout.lamps.iter().filter(|(lamp, _)| *lamp == zone).map(|(_, at)| Vec2::new(at.x, at.z)).collect();
            assert!(!lamps.is_empty(), "{zone:?} has no lamps");
            let (nx, nz) = (((hi.x - lo.x) / 0.25).ceil().max(1.0) as u32, ((hi.y - lo.y) / 0.25).ceil().max(1.0) as u32);
            for i in 0..=nx {
                for j in 0..=nz {
                    let at = Vec2::new(lo.x + (hi.x - lo.x) * i as f32 / nx as f32, lo.y + (hi.y - lo.y) * j as f32 / nz as f32);
                    let nearest = lamps.iter().map(|lamp| lamp.distance(at)).fold(f32::MAX, f32::min);
                    assert!(nearest <= LAMP_REACH, "{at} in {zone:?} is {nearest} m from its nearest lamp");
                }
            }
        }
    }

    #[test]
    fn every_point_of_an_indoor_floor_of_a_roofed_map_is_within_reach_of_a_lamp_in_its_zone() {
        for map in [MapId::Greybox, MapId::Quarry] {
        let layout = map.layout();
        assert!(layout.roofed, "{map:?}");
        for &zone in layout.zones.iter().filter(|&&zone| layout.indoor(zone)) {
            let (lo, hi) = zone.volume();
            // Clear of the walls, where a Player can't stand.
            let (lo, hi) = (Vec2::new(lo.x + 1.0, lo.z + 1.0), Vec2::new(hi.x - 1.0, hi.z - 1.0));
            let lamps: Vec<Vec2> = layout.lamps.iter().filter(|(lamp, _)| *lamp == zone).map(|(_, at)| Vec2::new(at.x, at.z)).collect();
            assert!(!lamps.is_empty(), "{zone:?} has no lamps");
            let (nx, nz) = (((hi.x - lo.x) / 0.25).ceil().max(1.0) as u32, ((hi.y - lo.y) / 0.25).ceil().max(1.0) as u32);
            for i in 0..=nx {
                for j in 0..=nz {
                    let at = Vec2::new(lo.x + (hi.x - lo.x) * i as f32 / nx as f32, lo.y + (hi.y - lo.y) * j as f32 / nz as f32);
                    let nearest = lamps.iter().map(|lamp| lamp.distance(at)).fold(f32::MAX, f32::min);
                    assert!(nearest <= LAMP_REACH, "{at} in {zone:?} is {nearest} m from its nearest lamp");
                }
            }
        }
        }
    }

    #[test]
    fn every_zone_has_a_palette_of_its_own() {
        let zones: Vec<Option<ZoneId>> = ZoneId::ALL.into_iter().map(Some).chain([None]).collect();
        for (i, &a) in zones.iter().enumerate() {
            for &b in &zones[i + 1..] {
                let (pa, pb) = (palette(a), palette(b));
                let unlike = apart(pa.wall, pb.wall).max(apart(pa.floor, pb.floor));
                assert!(unlike >= 0.12, "{a:?} and {b:?} look alike ({unlike:.2} apart)");
            }
            // The trim stands out against its own walls.
            let own = palette(a);
            assert!(apart(own.trim, own.wall) >= 0.2, "{a:?}'s trim is lost against its walls");
        }
    }

    #[test]
    fn every_map_lists_its_own_zones_and_no_one_elses() {
        for map in MapId::ALL {
            let layout = map.layout();
            for zone in ZoneId::ALL {
                assert_eq!(layout.zones.contains(&zone), zone.map() == map, "{} on the {}", zone.name(), map.name());
            }
        }
    }

    /// Out of Bounds is past the edge of the plan, or `OUT_OF_BOUNDS_DROP` below the lowest
    /// floor: the Quarry's sump, 0.6 m down, is its lowest.
    #[test]
    fn out_of_bounds_is_past_the_plan_or_well_below_the_lowest_floor() {
        assert_eq!(MapId::Quarry.layout().lowest_floor(), -0.6);
        assert_eq!(MapId::Village.layout().lowest_floor(), 0.0);
        for map in MapId::ALL {
            let layout = map.layout();
            let (min, max) = layout.plan;
            let middle = (min + max) / 2.0;
            let floor = layout.lowest_floor();
            assert!(!layout.out_of_bounds(Vec3::new(middle.x, floor, middle.y)));
            assert!(!layout.out_of_bounds(Vec3::new(middle.x, floor - OUT_OF_BOUNDS_DROP + 0.1, middle.y)));
            assert!(layout.out_of_bounds(Vec3::new(middle.x, floor - OUT_OF_BOUNDS_DROP - 0.1, middle.y)), "{}", map.name());
            for past in [Vec2::new(min.x - 0.1, middle.y), Vec2::new(max.x + 0.1, middle.y), Vec2::new(middle.x, min.y - 0.1), Vec2::new(middle.x, max.y + 0.1)] {
                assert!(layout.out_of_bounds(Vec3::new(past.x, 0.0, past.y)), "{} at {past}", map.name());
                assert!(layout.out_of_bounds(Vec3::new(past.x, 50.0, past.y)), "{} high over {past}", map.name());
            }
        }
    }

    /// Nothing a Map places is Out of Bounds: its path nodes, Seats, Spawn Points, Vents, Box
    /// Locations, windows and Jump Pads.
    #[test]
    fn nothing_a_map_places_is_out_of_bounds() {
        for map in MapId::ALL {
            let layout = map.layout();
            let mut spots: Vec<(String, Vec3)> = Vec::new();
            spots.extend(layout.nav_nodes.iter().map(|&(name, x, y, z)| (format!("path node {name}"), Vec3::new(x, y, z))));
            spots.extend(layout.seats.iter().map(|&at| ("a Seat".to_string(), at)));
            spots.extend(layout.risers.iter().map(|&(zone, at)| (format!("a Riser in {}", zone.name()), at)));
            spots.extend(layout.wading.iter().map(|&(zone, at)| (format!("a Wading Point in {}", zone.name()), at)));
            spots.extend(layout.hellhound_spawns.iter().map(|&(zone, at)| (format!("a Hellhound Spawn Point in {}", zone.name()), at)));
            spots.extend(layout.special_spawns.iter().map(|&(zone, at)| (format!("a Special Spawn Point in {}", zone.name()), at)));
            spots.extend(layout.vents.iter().flat_map(|&(zone, at)| [(format!("a Vent in {}", zone.name()), at), (format!("a Vent's opening in {}", zone.name()), at + Vec3::Y * NOVA_VENT_HEIGHT)]));
            spots.extend(map.barriers().flat_map(|barrier| [("a window".to_string(), window_centre(barrier)), ("a window's Spawn Point".to_string(), window_spawn(barrier))]));
            spots.extend(map.box_locations().map(|i| ("a Box Location".to_string(), BOX_LOCATIONS[i].1)));
            spots.extend(layout.jump_pads.iter().flat_map(|pad| [("a Jump Pad".to_string(), pad.at), ("a Jump Pad's landing".to_string(), pad.to)]));
            for (what, at) in spots {
                assert!(!layout.out_of_bounds(at), "{what} on the {} at {at} is Out of Bounds", map.name());
            }
        }
    }

    /// The height of the floor built under (x, z) in `zone`: its sunken floor's, a Raised Floor's,
    /// or the Zone's own.
    fn built_floor(layout: &Layout, zone: ZoneId, at: Vec2) -> f32 {
        if let Some(sunken) = layout.sunken.iter().find(|sunken| sunken.zone == zone && at.cmpgt(sunken.min).all() && at.cmplt(sunken.max).all()) {
            return zone.floor() - sunken.depth;
        }
        if let Some(raised) = layout.raised_floors.iter().find(|raised| raised.zone == zone && at.cmpgt(raised.min).all() && at.cmplt(raised.max).all()) {
            return raised.height;
        }
        zone.floor()
    }

    /// Each pool lies on the floor that's built there, over its whole extent, and no stairs or
    /// ramps cross it.
    #[test]
    fn every_pools_bed_matches_the_floor_built_under_it() {
        for map in MapId::ALL {
            let layout = map.layout();
            for pool in layout.water {
                assert_eq!(pool.zone.map(), map, "a pool of {} on the {}", pool.zone.name(), map.name());
                for fx in [0.02, 0.25, 0.5, 0.75, 0.98] {
                    for fz in [0.02, 0.25, 0.5, 0.75, 0.98] {
                        let at = pool.min + (pool.max - pool.min) * Vec2::new(fx, fz);
                        let floor = built_floor(layout, pool.zone, at);
                        assert!((floor - pool.bed).abs() < 1e-4, "{}'s pool at {at} has its bed at {} over a floor at {floor}", pool.zone.name(), pool.bed);
                    }
                }
                for slope in layout.slopes.iter().filter(|slope| slope.zone == pool.zone) {
                    let side = Vec2::new(-slope.up.y, slope.up.x) * slope.width / 2.0;
                    let foot = Vec2::new(slope.foot.x, slope.foot.z);
                    let (a, b) = (foot - side, foot + slope.up * slope.run + side);
                    let (lo, hi) = (a.min(b), a.max(b));
                    let apart = lo.x >= pool.max.x || hi.x <= pool.min.x || lo.y >= pool.max.y || hi.y <= pool.min.y;
                    assert!(apart, "a flight in {} runs through a pool", pool.zone.name());
                }
            }
        }
    }

    /// Pools that touch share a level (so the surface never steps), and the Cove's are all
    /// 0.4-0.6 m deep but for the sea, 3 m.
    #[test]
    fn pools_that_touch_share_a_level_and_the_coves_depths_are_as_authored() {
        let canals = &[];
        for map in MapId::ALL {
            let layout = map.layout();
            for (i, a) in layout.water.iter().enumerate() {
                for b in &layout.water[i + 1..] {
                    let touch = a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y;
                    if touch && a.bed == b.bed {
                        assert_eq!(a.surface(canals, TideLevel::Low), b.surface(canals, TideLevel::Low), "{} and {} touch", a.zone.name(), b.zone.name());
                    }
                }
            }
        }
        let cove = MapId::Cove.layout();
        let (mut sea, mut shallow) = (0, 0);
        for pool in cove.water {
            let depth = pool.depth(cove.canals, TideLevel::Low);
            match pool.kind(cove.canals, TideLevel::Low) {
                Some(Wading::Shallow) => {
                    shallow += 1;
                    assert!((0.4 - 1e-4..=0.6 + 1e-4).contains(&depth), "a pool in {} is {depth} m deep", pool.zone.name());
                }
                Some(Wading::Deep) => {
                    sea += 1;
                    assert!((depth - 3.0).abs() < 1e-4, "the sea in {} is {depth} m deep", pool.zone.name());
                }
                None => panic!("a pool in {} has no depth", pool.zone.name()),
            }
        }
        assert!(sea > 0 && shallow > 0);
    }

    /// Deep Water has no path nodes and no path-node edge crosses it, and nothing the Map places
    /// (a Seat, a Spawn Point, a Buyable, a Box Location) is in it.
    #[test]
    fn nothing_a_zombie_walks_or_a_map_places_is_in_deep_water() {
        for map in MapId::ALL {
            let layout = map.layout();
            let deep = |at: Vec3| water_at(layout, TideLevel::Low, at) == Some(Wading::Deep);
            let nodes: std::collections::HashMap<_, _> = layout.nav_nodes.iter().map(|&(name, x, y, z)| (name, Vec3::new(x, y, z))).collect();
            for (&name, &at) in &nodes {
                assert!(!deep(at), "path node {name} on the {} is in Deep Water", map.name());
            }
            for &(a, b, _) in layout.nav_edges {
                let (from, to) = (nodes[a], nodes[b]);
                for k in 0..=40 {
                    let at = from.lerp(to, k as f32 / 40.0);
                    assert!(!deep(at), "the edge {a} - {b} on the {} crosses Deep Water at {at}", map.name());
                }
            }
            for (name, at, _) in fixtures(map) {
                assert!(!deep(at), "{name} on the {} is in Deep Water", map.name());
            }
            for seat in layout.seats {
                assert!(!deep(seat), "a Seat on the {} is in Deep Water", map.name());
            }
            for &(zone, at) in layout.wading.iter().chain(layout.risers).chain(layout.hellhound_spawns).chain(layout.special_spawns) {
                assert!(!deep(at), "a Spawn Point in {} on the {} is in Deep Water", zone.name(), map.name());
            }
        }
    }

    /// Every Wading Point is on the bed of Shallow Water of its own Zone's pool, at least
    /// `WADING_DEEP_CLEARANCE` from Deep Water and from dry ground, and has a path node within
    /// reach on its own storey, joined by edges to every other node of its Zone. Only the Cove has
    /// any.
    #[test]
    fn every_wading_point_is_well_inside_shallow_water_and_on_the_path_graph() {
        for map in MapId::ALL {
            let layout = map.layout();
            let tide = TideLevel::Low;
            if map != MapId::Cove {
                assert!(layout.wading.is_empty(), "the {} has Wading Points", map.name());
            }
            let nodes: Vec<_> = layout.nav_nodes.iter().map(|&(name, x, y, z)| (name, Vec3::new(x, y, z))).collect();
            for &(zone, at) in layout.wading {
                let what = format!("the Wading Point at {at} in {}", zone.name());
                assert_eq!(water_at(layout, tide, at), Some(Wading::Shallow), "{what} is not in Shallow Water");
                assert_eq!(crate::maps::water::shallow_zone_at(layout, tide, at), Some(zone), "{what} is not over its own Zone's pool");
                assert!(crate::maps::water::wading_point_active(layout, tide, at), "{what} is not active");
                // A hair under the full clearance, as Deep Water and the shallows share an edge.
                assert!(
                    crate::maps::water::shallow_all_round(layout, tide, at, WADING_DEEP_CLEARANCE - 0.01),
                    "{what} has Deep Water or dry ground within {WADING_DEEP_CLEARANCE} m"
                );
                let near = nodes
                    .iter()
                    .filter(|(_, node)| (node.y - at.y).abs() < 1.0)
                    .map(|&(name, node)| (name, Vec2::new(node.x - at.x, node.z - at.z).length()))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap_or_else(|| panic!("{what} has no path node"));
                assert!(near.1 <= WADING_NAV_REACH, "{what} is {:.1} m from its nearest path node {}", near.1, near.0);
                // The Zone's own nodes, and those the nearest one reaches along edges between them.
                let own: Vec<&str> = nodes.iter().filter(|(_, node)| zone_at(*node + Vec3::Y * 0.5) == Some(zone)).map(|(name, _)| *name).collect();
                let mut reached = vec![near.0];
                let mut frontier = vec![near.0];
                while let Some(here) = frontier.pop() {
                    for &(a, b, _) in layout.nav_edges {
                        let there = if a == here { b } else if b == here { a } else { continue };
                        if own.contains(&there) && !reached.contains(&there) {
                            reached.push(there);
                            frontier.push(there);
                        }
                    }
                }
                for node in &own {
                    assert!(reached.contains(node), "{what}: path node {node} of {} is not joined to {}", zone.name(), near.0);
                }
            }
        }
    }

    /// Feet in Deep Water are Out of Bounds, and feet in Shallow Water or a pool's air aren't.
    #[test]
    fn feet_in_deep_water_are_out_of_bounds() {
        let cove = MapId::Cove.layout();
        let sea = Vec3::new(COVE_X - 30.0, 0.0, -20.0);
        assert!(cove.out_of_bounds(sea));
        assert!(cove.out_of_bounds(sea - Vec3::Y * 1.5));
        assert!(!cove.out_of_bounds(Vec3::new(COVE_X, 0.0, -10.0)), "the surf");
        assert!(!cove.out_of_bounds(sea + Vec3::Y * 1.0), "over the sea, on a parapet say");
    }

    /// Each Map stands in its own part of the world, so nothing of one is inside another's Zones.
    #[test]
    fn every_map_keeps_to_its_own_part_of_the_world() {
        for map in MapId::ALL {
            let (min, max) = map.layout().plan;
            for zone in map.layout().zones {
                let (lo, hi) = zone.volume();
                assert!(lo.x >= min.x && lo.z >= min.y && hi.x <= max.x && hi.z <= max.y, "{} sticks out of the {}'s plan", zone.name(), map.name());
            }
            for other in MapId::ALL.into_iter().filter(|&other| other != map) {
                let (omin, omax) = other.layout().plan;
                let overlap = min.cmplt(omax).all() && omin.cmplt(max).all();
                assert!(!overlap, "the {} and the {} overlap", map.name(), other.name());
            }
        }
    }

    #[test]
    fn every_chalk_outline_sits_in_its_zone() {
        for map in MapId::ALL {
            for &(weapon, zone, at, _) in map.layout().wall_weapons {
                assert!(inside(zone, at), "{weapon:?} is outside {}", zone.name());
                assert_eq!(zone.map(), map);
                assert!(wall_price(weapon).is_some(), "{weapon:?} is not a Wall Weapon");
            }
        }
    }

    /// What spoils each of `map`'s loops to lead a Train round: each loop is a name and its
    /// corners (x, z) on the floor at `floor`, and every leg must be a level lane at least 1.6 m
    /// wide with nothing in it. Empty when every loop is clear.
    pub(crate) fn spoilt_loops(map: MapId, floor: f32, loops: &'static [(&'static str, &'static [Vec2])]) -> Vec<String> {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;

        let mut sim = Sim::on(map, 60.0, |_| {});
        sim.world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                let solid = SpatialQueryFilter::from_mask(GameLayer::World);
                let lane = Collider::sphere(0.8);
                let mut spoilt = Vec::new();
                for &(name, corners) in loops {
                    for i in 0..corners.len() {
                        let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
                        let (from, to) = (Vec3::new(a.x, floor + 1.0, a.y), Vec3::new(b.x, floor + 1.0, b.y));
                        let config = ShapeCastConfig::from_max_distance(from.distance(to));
                        if spatial.cast_shape(&lane, from, Quat::IDENTITY, Dir3::new(to - from).unwrap(), &config, &solid).is_some() {
                            spoilt.push(format!("round {name}: {a} to {b} is cluttered"));
                        }
                        let level = (0..=20).all(|k| {
                            let at = from.lerp(to, k as f32 / 20.0);
                            spatial.cast_ray(at, Dir3::NEG_Y, 1.5, true, &solid).is_some_and(|hit| (hit.distance - 1.0).abs() < 0.05)
                        });
                        if !level {
                            spoilt.push(format!("round {name}: {a} to {b} isn't level"));
                        }
                    }
                }
                spoilt
            })
            .unwrap()
    }

    /// The headings (degrees) a Zombie could leave the ledge or roof outside a raised window by:
    /// from `barrier`'s Spawn Point, every way out at knee height that meets no wall or parapet
    /// within 4 m. Empty when it's walled in all round, so nothing knocked about out there falls.
    pub(crate) fn ways_off_outside(map: MapId, barrier: BarrierId) -> Vec<f32> {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;

        let spawn = window_spawn(barrier);
        let (_, _, out) = WINDOWS[barrier.0 as usize];
        let inward = -Vec3::new(out.x, 0.0, out.y);
        let mut sim = Sim::on(map, 60.0, |_| {});
        sim.world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                // The window's opening lets Zombies through; only the World layer walls them in.
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                (0..32)
                    .map(|k| k as f32 / 32.0 * std::f32::consts::TAU)
                    .filter(|&angle| {
                        let dir = Dir3::new(Vec3::new(angle.cos(), 0.0, angle.sin())).unwrap();
                        // Under the window's opening, its Sill.
                        let toward_window = dir.dot(inward) > 0.5;
                        let from = spawn + Vec3::Y * if toward_window { 0.45 } else { 0.5 };
                        spatial.cast_ray(from, dir, 4.0, true, &filter).is_none()
                    })
                    .map(f32::to_degrees)
                    .collect::<Vec<_>>()
            })
            .unwrap()
    }

    /// Everything fixed to `map`'s walls and floors, besides the walls and Doors themselves:
    /// a name, where it stands (x, y, z), and how far round it must stay clear.
    pub(crate) fn fixtures(map: MapId) -> Vec<(String, Vec3, f32)> {
        use crate::weapons::equipment::equipment_walls;
        use crate::economy::perk::Perk;
        use crate::maps::teleporter::{MAINFRAME_AT, PadId};

        let layout = map.layout();
        let mut fixtures = Vec::new();
        for &(weapon, _, at, _) in layout.wall_weapons {
            fixtures.push((format!("{weapon:?} chalk"), at, 0.8));
        }
        for i in map.box_locations() {
            let (zone, at, _) = BOX_LOCATIONS[i];
            fixtures.push((format!("Box Location {i} ({})", zone.name()), at, 1.0));
        }
        for barrier in map.barriers() {
            let at = window_centre(barrier) + Vec3::Y * 1.1;
            fixtures.push((format!("window {}", barrier.0), at, WINDOW_WIDTH / 2.0));
        }
        for perk in Perk::ALL {
            fixtures.push((format!("{perk:?}"), perk.machine_spot(map).0, 0.7));
        }
        for &(wall, _, at, _) in equipment_walls(map) {
            fixtures.push((format!("{wall:?} chalk"), at, 0.6));
        }
        if crate::maps::teleporter::stands_on(map) {
            for pad in PadId::ALL {
                fixtures.push((format!("{pad:?} Pad"), pad.at(), TELEPORTER_PAD_RADIUS));
            }
            fixtures.push(("Mainframe".into(), MAINFRAME_AT, 0.8));
        }
        for (i, pad) in layout.jump_pads.iter().enumerate() {
            fixtures.push((format!("Jump Pad {i}"), pad.at, JUMP_PAD_RADIUS));
        }
        for generator in layout.generators {
            let (console, size) = generator.console();
            fixtures.push((format!("{} Generator's console", generator.section.name()), console, size.x.max(size.z) / 2.0));
            fixtures.push((format!("{} Generator's plate", generator.section.name()), generator.at, GENERATOR_PLATE_RADIUS));
        }
        if let Some((at, _)) = crate::maps::power::switch_spot(map) {
            fixtures.push(("Power Switch".into(), at, 0.5));
        }
        fixtures.push(("Pack-a-Punch".into(), crate::weapons::pack_a_punch::machine_at(map), 0.8));
        fixtures
    }

    #[test]
    fn every_fixture_stands_in_its_map() {
        for map in MapId::ALL {
            for (name, at, _) in fixtures(map) {
                let zone = zone_at(at.with_y(at.y.min(at.y.floor() + 0.5)) + Vec3::Y * 0.1);
                assert_eq!(zone.map(|zone| zone.map()), Some(map), "{name} on the {} stands at {at}", map.name());
            }
        }
    }

    #[test]
    fn every_jump_pad_stands_in_its_zone_and_throws_up_into_another_floor() {
        for map in MapId::ALL {
            for pad in map.layout().jump_pads {
                assert_eq!(zone_at(pad.at + Vec3::Y * 0.1), Some(pad.zone), "the Jump Pad at {}", pad.at);
                assert_eq!(pad.zone.map(), map);
                assert!(pad.to.y >= pad.at.y + STOREY - 0.01, "the Jump Pad at {} doesn't throw a storey up", pad.at);
            }
        }
    }

    /// What stands on the floor (the Box Locations, the Perk machines, Pack-a-Punch, the Jump
    /// Pads) has floor just under its middle on the built Map, not a drop.
    #[test]
    fn everything_standing_on_the_floor_has_floor_under_it() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;
        for map in MapId::ALL {
            let mut standing: Vec<(String, Vec3)> = map.box_locations().map(|i| (format!("Box Location {i}"), BOX_LOCATIONS[i].1)).collect();
            standing.extend(crate::economy::perk::Perk::ALL.map(|perk| (format!("{perk:?}"), perk.machine_spot(map).0)));
            standing.push(("Pack-a-Punch".into(), crate::weapons::pack_a_punch::machine_at(map)));
            standing.extend(map.layout().jump_pads.iter().map(|pad| ("a Jump Pad".to_string(), pad.at)));
            standing.extend(map.layout().generators.iter().map(|generator| (format!("the {} Generator's plate", generator.section.name()), generator.at)));
            let mut sim = Sim::on(map, 60.0, |_| {});
            let floating: Vec<String> = sim
                .world_mut()
                .run_system_once(move |spatial: SpatialQuery| {
                    let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                    standing
                        .iter()
                        .filter(|(_, at)| {
                            // From just above the floor; a Jump Pad's own plate counts as floor.
                            let hit = spatial.cast_ray(*at + Vec3::Y * 0.3, Dir3::NEG_Y, 0.5, true, &filter);
                            hit.is_none_or(|hit| hit.distance < 0.2)
                        })
                        .map(|(name, at)| format!("{name} at {at}"))
                        .collect::<Vec<String>>()
                })
                .unwrap();
            assert!(floating.is_empty(), "with no floor under them on the {}: {floating:?}", map.name());
        }
    }

    /// Every Generator's whole plate is floor, with no wall or Prop inside it.
    #[test]
    fn every_generator_plate_fits_its_floor_clear_of_walls_and_props() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;
        for map in MapId::ALL {
            let plates: Vec<(String, Vec3)> =
                map.layout().generators.iter().map(|generator| (generator.section.name().to_string(), generator.at)).collect();
            let mut sim = Sim::on(map, 60.0, |_| {});
            let bad: Vec<String> = sim
                .world_mut()
                .run_system_once(move |spatial: SpatialQuery| {
                    let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                    let mut bad = Vec::new();
                    for (name, at) in &plates {
                        for k in 0..32 {
                            let angle = k as f32 / 32.0 * std::f32::consts::TAU;
                            let out = Vec3::new(angle.cos(), 0.0, angle.sin());
                            let dir = Dir3::new(out).unwrap();
                            // Nothing between the middle and the rim, at knee and at waist.
                            for height in [0.3, 1.0] {
                                let from = *at + Vec3::Y * height;
                                if let Some(hit) = spatial.cast_ray(from, dir, GENERATOR_PLATE_RADIUS - 0.02, true, &filter) {
                                    bad.push(format!("{name}: blocked {:.2} m out at {angle:.2} rad, {height} m up", hit.distance));
                                }
                            }
                            // Floor under the rim, level with the middle.
                            let rim = *at + out * (GENERATOR_PLATE_RADIUS - 0.05) + Vec3::Y * 0.3;
                            let level = spatial
                                .cast_ray(rim, Dir3::NEG_Y, 0.6, true, &filter)
                                .is_some_and(|hit| (hit.distance - 0.3).abs() < 0.1);
                            if !level {
                                bad.push(format!("{name}: no level floor under the rim at {angle:.2} rad"));
                            }
                        }
                    }
                    bad
                })
                .unwrap();
            assert!(bad.is_empty(), "on the {}: {bad:?}", map.name());
        }
    }

    #[test]
    fn nothing_on_the_map_stands_in_anything_else() {
        for map in MapId::ALL {
            let fixtures = fixtures(map);
            for (i, (a, at_a, clear_a)) in fixtures.iter().enumerate() {
                for (b, at_b, clear_b) in &fixtures[i + 1..] {
                    // A storey apart is apart.
                    if (at_a.y - at_b.y).abs() > 2.5 {
                        continue;
                    }
                    // A Generator's console stands at the back of its own plate.
                    if let (Some(console), Some(plate)) = (a.strip_suffix("'s console"), b.strip_suffix("'s plate"))
                        && console == plate
                    {
                        continue;
                    }
                    let gap = Vec2::new(at_a.x - at_b.x, at_a.z - at_b.z).length();
                    assert!(gap >= clear_a + clear_b + 0.3, "{a} and {b} are only {gap:.2} m apart");
                }
            }
        }
    }

    /// How far `at` is from the footprint `min`..`max`, across the floor; 0 inside it.
    fn off(min: Vec2, max: Vec2, at: Vec3) -> f32 {
        let at = Vec2::new(at.x, at.z);
        (min - at).max(at - max).max(Vec2::ZERO).length()
    }

    /// A Prop's footprint.
    fn footprint(prop: &Prop) -> (Vec2, Vec2) {
        let (middle, half) = (Vec2::new(prop.at.x, prop.at.z), Vec2::new(prop.size.x, prop.size.z) / 2.0);
        (middle - half, middle + half)
    }

    /// How far `at` is from a Prop's footprint, across the floor; 0 inside it.
    fn off_prop(prop: &Prop, at: Vec3) -> f32 {
        let (min, max) = footprint(prop);
        off(min, max, at)
    }

    /// Whether `at` is on the same storey as a Prop, near enough to stand in its way.
    fn beside(prop: &Prop, at: Vec3) -> bool {
        at.y > prop.at.y - 2.5 && at.y < prop.at.y + prop.size.y + 2.5
    }

    type Footprint = (String, ZoneId, (Vec2, Vec2), (f32, f32), Box<dyn Fn(Vec3) -> bool>);

    /// Everything that takes up floor on `layout`: a name, its Zone, its footprint, its bottom and
    /// top, and what may stand within it all the same: things up on or over a Raised Floor, or
    /// down on a sunken one's floor.
    fn footprints(layout: &'static Layout) -> Vec<Footprint> {
        let mut all: Vec<Footprint> = Vec::new();
        for prop in layout.props {
            let name = format!("the {:?} at {}", prop.kind, prop.at);
            all.push((name, prop.zone, footprint(prop), (prop.at.y, prop.at.y + prop.size.y), Box::new(|_| false)));
        }
        for raised in layout.raised_floors {
            let top = raised.height;
            let name = format!("the Raised Floor in {}", raised.zone.name());
            all.push((name, raised.zone, (raised.min, raised.max), (raised.zone.floor(), top), Box::new(move |at: Vec3| at.y >= top - 0.05)));
        }
        for slope in layout.slopes {
            let end = Vec2::new(slope.foot.x, slope.foot.z) + slope.up * slope.run;
            let across = Vec2::new(slope.up.y.abs(), slope.up.x.abs()) * slope.width / 2.0;
            let (a, b) = (Vec2::new(slope.foot.x, slope.foot.z), end);
            let name = format!("the steps or ramp at {}", slope.foot);
            all.push((name, slope.zone, (a.min(b) - across, a.max(b) + across), (slope.foot.y, slope.foot.y + slope.rise), Box::new(|_| false)));
        }
        for sunken in layout.sunken {
            // A sea bed lies beyond its Zone, under the sea, not in the way of anything in it.
            if sunken.max.y <= sunken.zone.volume().0.z {
                continue;
            }
            let floor = -sunken.depth;
            let name = format!("the sunken floor in {}", sunken.zone.name());
            all.push((name, sunken.zone, (sunken.min, sunken.max), (floor, 0.0), Box::new(move |at: Vec3| (at.y - floor).abs() < 0.05)));
        }
        all
    }

    #[test]
    fn nothing_on_the_floor_crowds_anything_fixed_to_the_map() {
        for map in MapId::ALL {
            nothing_on_the_floor_crowds_anything_fixed_to(map);
        }
    }

    fn nothing_on_the_floor_crowds_anything_fixed_to(map: MapId) {
        use crate::maps::teleporter::{ARRIVAL_AT, HOUNDS_AROUND};
        let layout = map.layout();
        // What must stay clear, the Zone it's in, and how much room round it: what's on the walls
        // and floors with space to stand at it, where Zombies land climbing in at the windows,
        // where Nova Crawlers drop, where the Players start, Teleport to and land off a Jump Pad,
        // where the Teleporter's hounds land, and every Spawn Point. A wall between is room
        // enough.
        let mut keep: Vec<(String, Option<ZoneId>, Vec3, f32)> = fixtures(map)
            .into_iter()
            .map(|(name, at, clear)| (name, zone_at(at.with_y(at.y.min(at.y.floor() + 0.5))), at, clear + 0.6))
            .collect();
        for barrier in map.barriers() {
            let (zone, ..) = WINDOWS[barrier.0 as usize];
            keep.push((format!("window {}'s landing", barrier.0), Some(zone), window_centre(barrier), 2.6));
        }
        for &(zone, at) in layout.vents {
            keep.push((format!("{}'s Vent drop", zone.name()), Some(zone), at, 1.0));
        }
        for at in crate::maps::trap::switch_spots(map) {
            keep.push(("a Trap switch".into(), zone_at(at), at, 1.0));
        }
        for (i, at) in layout.seats.iter().enumerate() {
            keep.push((format!("Seat {i}"), Some(layout.start_zone()), *at, PLAYER_RADIUS + 0.8));
        }
        for (i, pad) in layout.jump_pads.iter().enumerate() {
            keep.push((format!("Jump Pad {i}'s landing"), zone_at(pad.to + Vec3::Y * 0.1), pad.to, PLAYER_RADIUS + 0.8));
        }
        if crate::maps::teleporter::stands_on(map) {
            keep.push(("the Teleporter's landing".into(), Some(layout.start_zone()), ARRIVAL_AT, PLAYER_RADIUS + 0.8));
            let ring = HELLHOUND_AROUND_MAX_DIST + HELLHOUND_RADIUS + 0.1;
            keep.push(("the Teleporter's hounds".into(), Some(layout.start_zone()), HOUNDS_AROUND, ring));
        }
        for &(zone, at) in layout.risers {
            keep.push((format!("the Riser at {at}"), Some(zone), at, 1.0));
        }
        for &(zone, at) in layout.wading {
            keep.push((format!("the Wading Point at {at}"), Some(zone), at, 1.0));
        }
        for (zone, at) in layout.hellhound_spawns.iter().chain(layout.special_spawns) {
            keep.push((format!("the {} Spawn Point at {at}", zone.name()), Some(*zone), *at, 0.9));
        }
        let mut crowded = Vec::new();
        for (what, its_zone, (min, max), (bottom, top), may_stand) in footprints(layout) {
            for (name, zone, at, clear) in &keep {
                let gap = off(min, max, *at);
                let same_storey = at.y > bottom - 2.5 && at.y < top + 2.5;
                if *zone == Some(its_zone) && same_storey && gap < *clear && !may_stand(*at) {
                    crowded.push(format!("{what} is {gap:.2} m from {name}"));
                }
            }
        }
        assert!(crowded.is_empty(), "on the {}: {crowded:#?}", map.name());
    }

    /// The floor `prop` stands on in `layout`: its Zone's own, or a Raised Floor's top.
    fn floor_under(layout: &Layout, prop: &Prop) -> Option<f32> {
        let (min, max) = footprint(prop);
        let raised = layout
            .raised_floors
            .iter()
            .find(|raised| raised.zone == prop.zone && min.cmpge(raised.min).all() && max.cmple(raised.max).all())
            .map(|raised| raised.height);
        [raised, Some(prop.zone.floor())].into_iter().flatten().find(|&floor| (floor - prop.at.y).abs() < 1e-3)
    }

    #[test]
    fn every_prop_stands_on_a_floor_in_its_zone_clear_of_the_doors() {
        use bevy::ecs::system::RunSystemOnce;
        for map in MapId::ALL {
            let layout = map.layout();
            let mut sim = Sim::on(map, 60.0, |_| {});
            let doors: Vec<Vec3> = sim
                .world_mut()
                .run_system_once(|doors: Query<&Transform, With<crate::maps::door::Door>>| doors.iter().map(|t| t.translation).collect::<Vec<_>>())
                .unwrap();
            assert!(!doors.is_empty());
            for prop in layout.props {
                let (lo, hi) = prop.zone.volume();
                let half = Vec3::new(prop.size.x, 0.0, prop.size.z) / 2.0;
                let (min, max) = (prop.at - half, prop.at + half);
                assert!(min.x >= lo.x && max.x <= hi.x && min.z >= lo.z && max.z <= hi.z, "the {:?} at {} sticks out of {}", prop.kind, prop.at, prop.zone.name());
                assert!(floor_under(layout, prop).is_some(), "the {:?} at {} stands on no floor", prop.kind, prop.at);
                for door in &doors {
                    let gap = off_prop(prop, *door);
                    assert!(!beside(prop, *door) || gap >= 1.8, "the {:?} at {} is {gap:.2} m from the Door at {door}", prop.kind, prop.at);
                }
            }
        }
    }

    /// Nothing a Player can get on top of lifts them out of a Zombie's reach. A Prop is climbed
    /// onto from the floor it stands on if it's no taller than a running jump clambers, or from
    /// another such Prop close beside it; every one that can be must be low enough over its floor,
    /// and narrow enough, for a Zombie at its side to swipe a Player anywhere on it.
    #[test]
    fn no_prop_lifts_a_player_out_of_reach() {
        for map in MapId::ALL {
            let layout = map.layout();
            let props = layout.props;
            let clamber = PLAYER_JUMP_SPEED * PLAYER_JUMP_SPEED / (2.0 * GRAVITY) + PLAYER_RADIUS;
            let top = |prop: &Prop| prop.at.y + prop.size.y;
            let gap = |a: &Prop, b: &Prop| {
                let ((amin, amax), (bmin, bmax)) = (footprint(a), footprint(b));
                (amin - bmax).max(bmin - amax).max(Vec2::ZERO).length()
            };
            let mut climbable: Vec<bool> = props.iter().map(|prop| prop.size.y <= clamber).collect();
            loop {
                let more: Vec<bool> = props
                    .iter()
                    .enumerate()
                    .map(|(i, prop)| {
                        climbable[i]
                            || props.iter().enumerate().any(|(j, from)| {
                                climbable[j] && j != i && gap(prop, from) <= 1.0 && top(prop) - top(from) <= clamber
                            })
                    })
                    .collect();
                if more == climbable {
                    break;
                }
                climbable = more;
            }
            // A Zombie's middle stands its body's half-height over the floor; a Player's on top, theirs.
            let zombie_half = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
            let player_half = PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0;
            for (prop, _) in props.iter().zip(&climbable).filter(|(_, up)| **up) {
                let floor = floor_under(layout, prop).unwrap();
                let above = top(prop) + player_half - (floor + zombie_half);
                assert!(above < 1.5, "a Player on the {:?} at {} stands {above:.2} m over a Zombie's middle", prop.kind, prop.at);
                let narrow = prop.size.x.min(prop.size.z);
                assert!(narrow / 2.0 + ZOMBIE_RADIUS <= ZOMBIE_REACH, "the {:?} at {} is too wide to swipe across", prop.kind, prop.at);
            }
        }
    }

    /// Hellhounds, specials, Risers and Nova Crawlers arrive standing on the floor at their Spawn
    /// Points: there's floor just under each, and room for a Zombie's body over it, clear of the
    /// walls and everything on the floor.
    #[test]
    fn every_spawn_point_inside_stands_on_open_floor() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;
        for map in MapId::ALL {
            let mut sim = Sim::on(map, 60.0, |_| {});
            let blocked: Vec<String> = sim
                .world_mut()
                .run_system_once(|spatial: SpatialQuery, spawns: Query<(&SpawnPoint, &Transform)>| {
                    let map = SpatialQueryFilter::from_mask(GameLayer::World);
                    let body = Collider::capsule(ZOMBIE_RADIUS, ZOMBIE_CAPSULE_LENGTH);
                    let half = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
                    spawns
                        .iter()
                        .filter(|(spawn, _)| !matches!(spawn.kind, SpawnKind::Window(_)))
                        .filter_map(|(spawn, transform)| {
                            let feet = transform.translation;
                            let floor = spatial.cast_ray(feet + Vec3::Y * 0.3, Dir3::NEG_Y, 0.6, true, &map);
                            let on_floor = floor.is_some_and(|hit| (hit.distance - 0.3).abs() < 0.05);
                            let room = spatial.shape_intersections(&body, feet + Vec3::Y * (half + 0.05), Quat::IDENTITY, &map).is_empty();
                            (!on_floor || !room).then(|| format!("{:?} in {} at {feet}", spawn.kind, spawn.zone.name()))
                        })
                        .collect::<Vec<String>>()
                })
                .unwrap();
            assert!(blocked.is_empty(), "not on open floor on the {}: {blocked:#?}", map.name());
        }
    }

    #[test]
    fn every_seat_starts_in_the_first_zone_on_open_floor_and_can_walk_out() {
        use crate::enemies::nav::NavGraph;
        use crate::enemies::target::path_distance;
        assert_eq!(MapId::Greybox.layout().seats[0], PLAYER_START, "solo starts where it always did");
        for map in MapId::ALL {
            let layout = map.layout();
            let graph = NavGraph::new(map);
            let all_open: Vec<_> = layout.zones.iter().flat_map(|&a| layout.zones.iter().map(move |&b| (a, b))).collect();
            // Somewhere past the start's Doors.
            let out = graph.nodes.iter().find(|node| node.zone.is_some_and(|zone| zone != layout.start_zone())).unwrap().at;
            let start = layout.start_zone();
            for (i, &seat) in layout.seats.iter().enumerate() {
                let (lo, hi) = start.volume();
                assert!((lo.x + 2.0..=hi.x - 2.0).contains(&seat.x) && (lo.z + 2.0..=hi.z - 2.0).contains(&seat.z), "Seat {i} is by a wall");
                assert!((seat.y - start.floor()).abs() < 1e-3, "Seat {i} isn't on the floor");
                for (name, at, clear) in fixtures(map) {
                    let gap = Vec2::new(at.x - seat.x, at.z - seat.z).length();
                    assert!((at.y - seat.y).abs() > 2.5 || gap >= clear + PLAYER_RADIUS + 0.5, "Seat {i} is only {gap:.2} m from {name}");
                }
                for &other in &layout.seats[i + 1..] {
                    assert!(seat.distance(other) >= 2.5, "Seats are only {:.2} m apart", seat.distance(other));
                }
                assert!(path_distance(&graph, seat, out, &all_open, |_| true).is_some(), "Seat {i} can't walk out on the {}", map.name());
            }
        }
    }

    #[test]
    fn every_box_location_sits_in_its_zone() {
        for map in MapId::ALL {
            for i in map.box_locations() {
                let (zone, at, _) = BOX_LOCATIONS[i];
                assert!(inside(zone, at + Vec3::Y * 0.1), "a Box Location is outside {}", zone.name());
                assert_eq!(zone.map(), map);
            }
            // Never in the first Zone: every Run starts the Box two or three Doors out of it, so
            // the first Points go on opening the way to it.
            let start = map.layout().start_zone();
            for i in map.box_locations() {
                assert_ne!(BOX_LOCATIONS[i].0, start, "Box Location {i} is in {}", start.name());
            }
            let (first, ..) = BOX_LOCATIONS[map.box_locations().start];
            let doors = doors_between(map, start, first);
            assert!((2..=3).contains(&doors), "the {}'s Box starts {doors} Doors out of {}", map.name(), start.name());
        }
    }

    /// The fewest Doors bought on the way from Zone `from` to Zone `to`: a fixed join, like a drop,
    /// costs none.
    fn doors_between(map: MapId, from: ZoneId, to: ZoneId) -> usize {
        let layout = map.layout();
        let mut joins: Vec<(ZoneId, ZoneId, usize)> = layout.fixed_joins.iter().map(|&(a, b)| (a, b, 0)).collect();
        joins.extend(layout.nav_edges.iter().filter_map(|&(.., gate)| match gate {
            Gate::Door(a, b) if a != b => Some((a, b, 1)),
            _ => None,
        }));
        let mut best = vec![(from, 0)];
        // Few enough Zones that relaxing every join once per Zone settles it.
        for _ in layout.zones {
            for &(a, b, cost) in &joins {
                for (here, there) in [(a, b), (b, a)] {
                    let Some(&(_, d)) = best.iter().find(|(zone, _)| *zone == here) else { continue };
                    match best.iter_mut().find(|(zone, _)| *zone == there) {
                        Some(known) if known.1 <= d + cost => {}
                        Some(known) => known.1 = d + cost,
                        None => best.push((there, d + cost)),
                    }
                }
            }
        }
        best.iter().find(|(zone, _)| *zone == to).map_or(usize::MAX, |&(_, d)| d)
    }

    #[test]
    fn every_zone_has_a_hellhound_spawn_point_standing_in_it() {
        for map in MapId::ALL {
            let layout = map.layout();
            for &(zone, at) in layout.hellhound_spawns.iter().chain(layout.special_spawns).chain(layout.risers).chain(layout.wading) {
                // Just above the floor, which sits a little inside the Zone.
                assert_eq!(zone_at(at + Vec3::Y * 0.5), Some(zone), "{at} is outside {}", zone.name());
            }
            for &zone in layout.zones {
                assert!(layout.hellhound_spawns.iter().any(|(z, _)| *z == zone), "{} has no Hellhound Spawn Point", zone.name());
            }
        }
    }

    #[test]
    fn vents_open_over_the_floor_of_their_zones() {
        for map in MapId::ALL {
            for &(zone, at) in map.layout().vents {
                assert_eq!(zone_at(at + Vec3::Y * 0.1), Some(zone), "under the Vent");
                assert_eq!(zone_at(at + Vec3::Y * NOVA_VENT_HEIGHT), Some(zone), "the Vent");
            }
        }
    }

    /// Rays straight out through every window of `map` on the World layer, from 1 m inside to 1 m
    /// outside: what solid wall stands in the way at `height` above the window's floor.
    fn window_walls_at(map: MapId, height: f32) -> Vec<(u8, bool)> {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;

        let mut sim = Sim::on(map, 60.0, |_| {});
        sim.world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                map.barriers()
                    .map(|barrier| {
                        let (_, _, out) = WINDOWS[barrier.0 as usize];
                        let from = window_centre(barrier) + Vec3::Y * height - Vec3::new(out.x, 0.0, out.y);
                        let dir = Dir3::new(Vec3::new(out.x, 0.0, out.y)).unwrap();
                        (barrier.0, spatial.cast_ray(from, dir, 2.0, true, &filter).is_some())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap()
    }

    /// Every window's Spawn Point stands on a floor outside it.
    #[test]
    fn every_window_spawn_point_stands_on_a_floor() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;

        for map in MapId::ALL {
            let mut sim = Sim::on(map, 60.0, |_| {});
            let floating: Vec<u8> = sim
                .world_mut()
                .run_system_once(move |spatial: SpatialQuery| {
                    let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                    map.barriers()
                        .filter(|&barrier| {
                            let spawn = window_spawn(barrier);
                            let hit = spatial.cast_ray(spawn + Vec3::Y * 0.5, Dir3::NEG_Y, 0.7, true, &filter);
                            hit.is_none()
                        })
                        .map(|barrier| barrier.0)
                        .collect::<Vec<_>>()
                })
                .unwrap();
            assert!(floating.is_empty(), "window Spawn Points with no floor under them: {floating:?}");
            for barrier in map.barriers() {
                assert_eq!(zone_at(window_spawn(barrier) + Vec3::Y * 0.1), None, "window {}'s Spawn Point is indoors", barrier.0);
            }
        }
    }

    #[test]
    fn every_window_has_a_solid_sill_below_its_opening() {
        for map in MapId::ALL {
            let open: Vec<u8> = window_walls_at(map, 0.45).into_iter().filter(|(_, hit)| !hit).map(|(i, _)| i).collect();
            assert!(open.is_empty(), "windows with no Sill at knee height: {open:?}");
        }
    }

    #[test]
    fn a_window_opening_is_open_to_everything_but_the_player() {
        for map in MapId::ALL {
            let shut: Vec<u8> = window_walls_at(map, 1.5).into_iter().filter(|(_, hit)| *hit).map(|(i, _)| i).collect();
            assert!(shut.is_empty(), "windows with solid wall across the opening: {shut:?}");
        }
    }
}
