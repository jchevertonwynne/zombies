//! The Maps: what each is made of, as data, and how the one a Run is played on is built.
//!
//! Every Map has a `Layout`: its Zones, windows, Spawn Points, Box Locations, Wall Weapons,
//! floors, Props and path nodes. Each Map is built in its own part of the world, so no two Maps'
//! Zones overlap and a point is in at most one Map's Zone (ADR 0005).

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
mod greybox;
mod quarry;
mod village;

use build::{Builder, Gap};

pub use greybox::CONTROL_PLATFORM;
#[cfg(test)]
pub use greybox::PLAYER_START;
pub use quarry::QUARRY_X;
pub use village::VILLAGE_X;

use crate::maps::jump_pad::JumpPad;

/// One storey.
pub(crate) const STOREY: f32 = 4.0;
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
}

impl MapId {
    pub const ALL: [MapId; 3] = [MapId::Greybox, MapId::Quarry, MapId::Village];

    pub fn name(self) -> &'static str {
        match self {
            MapId::Greybox => "Greybox",
            MapId::Quarry => "Quarry",
            MapId::Village => "Village",
        }
    }

    /// A line on what the Map is like, for choosing it.
    pub fn blurb(self) -> &'static str {
        match self {
            MapId::Greybox => "A house round a courtyard, and a wing across the field.",
            MapId::Quarry => "A pit and its works, climbed terrace by terrace.",
            MapId::Village => "A long village of streets and lanes, walked end to end.",
        }
    }

    /// What the Map is made of.
    pub const fn layout(self) -> &'static Layout {
        match self {
            MapId::Greybox => &greybox::GREYBOX,
            MapId::Quarry => &quarry::QUARRY,
            MapId::Village => &village::VILLAGE,
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
    /// The way the moonlight falls.
    pub sun: Vec3,
    /// The corners (x, z) of the patch of the world the Map takes up, for drawing its plan.
    #[cfg_attr(not(test), allow(dead_code))]
    pub plan: (Vec2, Vec2),
    /// Builds the walls, the Doors and everything else the data above doesn't cover.
    build: fn(&mut Builder),
}

impl Layout {
    /// The Zone every Run starts in.
    pub fn start_zone(&self) -> ZoneId {
        self.zones[0]
    }

    /// Where the Player in Seat `seat` starts.
    pub fn seat_start(&self, seat: u8) -> Vec3 {
        self.seats[usize::from(seat) % self.seats.len()]
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
pub const WINDOWS: [(ZoneId, Vec3, Vec2); GREYBOX_AND_QUARRY_WINDOWS.len() + village::WINDOWS.len()] = concat(GREYBOX_AND_QUARRY_WINDOWS, village::WINDOWS);
const GREYBOX_AND_QUARRY_WINDOWS: [(ZoneId, Vec3, Vec2); greybox::WINDOWS.len() + quarry::WINDOWS.len()] = concat(greybox::WINDOWS, quarry::WINDOWS);

/// Every Map's Box Locations, each Map's in a run of its own, in order: the Zone, where the Box's
/// centre sits on the floor, and which way its front faces (where the Player stands). Every Run
/// starts the Box at its Map's first.
pub const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); GREYBOX_AND_QUARRY_BOXES.len() + village::BOX_LOCATIONS.len()] =
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
        Some(BackLane) => paints((0.24, 0.3, 0.14), (0.3, 0.31, 0.36), (0.62, 0.6, 0.52), (0.7, 0.8, 1.0)),
        // A cart track between old brick walls.
        Some(MillLane) => paints((0.32, 0.26, 0.18), (0.42, 0.3, 0.22), (0.7, 0.68, 0.6), (0.75, 0.82, 1.0)),
        // Gravel between clipped yew hedges.
        Some(ChurchLane) => paints((0.42, 0.4, 0.36), (0.14, 0.24, 0.15), (0.6, 0.58, 0.5), (0.75, 0.85, 1.0)),
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
        Some(Orchard) => paints((0.16, 0.22, 0.1), (0.5, 0.47, 0.4), (0.3, 0.22, 0.14), (0.7, 0.8, 1.0)),
        // Dark churchyard grass inside lichened grey walls, under a cold moon.
        Some(Graveyard) => paints((0.12, 0.17, 0.11), (0.52, 0.5, 0.56), (0.2, 0.2, 0.24), (0.6, 0.75, 1.0)),
        // Bare ground round weathered brick.
        None => paints((0.15, 0.17, 0.12), (0.3, 0.21, 0.17), (0.42, 0.4, 0.36), (1.0, 0.75, 0.45)),
    }
}

/// A Raised Floor standing on its Zone's floor: a stage, a dais, a dock. It covers `min`..`max`
/// (x, z), its top `height` up. Its edges are Ledges the Player can hop down from; Slopes lead up
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
            brightness: 120.0,
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

    // The Props.
    for (nth, prop) in layout.props.iter().enumerate() {
        b.prop(prop, nth);
    }

    // Chalk outlines: a pale gun shape drawn on the wall, its Buyable just in front.
    let chalk_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.85),
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.insert_resource(crate::darkness_prototype::ChalkMat(chalk_mat.clone()));
    let barrel = meshes.add(Cuboid::new(1.2, 0.12, 0.02));
    let grip = meshes.add(Cuboid::new(0.12, 0.3, 0.02));
    for &(weapon, _, centre, facing) in layout.wall_weapons {
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
                Mesh3d(barrel.clone()),
                MeshMaterial3d(chalk_mat.clone()),
                Transform::from_translation(centre + normal * 0.01).with_rotation(rotation),
            ))
            .with_child((
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

    // Lighting: a dim moon, and a warm lamp in each room.
    commands.spawn((
        InRun,
        DirectionalLight {
            illuminance: 800.0,
            shadow_maps_enabled: true,
            color: Color::srgb(0.7, 0.75, 1.0),
            ..default()
        },
        Transform::default().looking_at(layout.sun, Vec3::Y),
    ));
    for &(zone, at) in layout.lamps {
        commands.spawn((
            InRun,
            crate::darkness_prototype::Lamp { zone, full: 400_000.0 },
            PointLight {
                color: palette(Some(zone)).light,
                intensity: 400_000.0,
                range: 25.0,
                ..default()
            },
            Transform::from_translation(at),
        ));
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
        fixtures.push(("Power Switch".into(), crate::maps::power::switch_spot(map).0, 0.5));
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
            all.push((name, raised.zone, (raised.min, raised.max), (0.0, top), Box::new(move |at: Vec3| at.y >= top - 0.05)));
        }
        for slope in layout.slopes {
            let end = Vec2::new(slope.foot.x, slope.foot.z) + slope.up * slope.run;
            let across = Vec2::new(slope.up.y.abs(), slope.up.x.abs()) * slope.width / 2.0;
            let (a, b) = (Vec2::new(slope.foot.x, slope.foot.z), end);
            let name = format!("the steps or ramp at {}", slope.foot);
            all.push((name, slope.zone, (a.min(b) - across, a.max(b) + across), (slope.foot.y, slope.foot.y + slope.rise), Box::new(|_| false)));
        }
        for sunken in layout.sunken {
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
            for &(zone, at) in layout.hellhound_spawns.iter().chain(layout.special_spawns).chain(layout.risers) {
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
