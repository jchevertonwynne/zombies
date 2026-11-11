//! The greybox Map: a ring of five indoor Zones around an open Yard, plus a Balcony over the Yard
//! reached by a ramp in West Hall, with a one-way drop back down. South of the ring, across open
//! ground, lies the south wing: the Workshop, the Storeroom and the Generator Room, reached down
//! the Passage from the Dining Room and the Tunnel from the Power Room. The Teleporter's Pads sit
//! at the wing's far corners.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::arsenal::WeaponId;
use crate::buy::{Buyable, Offers};
use crate::state::{GameState, InRun};
use crate::tuning::*;
use crate::wall_weapon::{WallWeapon, priced_for, wall_price};
use crate::zone::ZoneId;

mod build;

use build::{Builder, Gap};

/// Where the Player's feet start: the middle of the Start Room, facing the Yard.
pub const PLAYER_START: Vec3 = Vec3::new(0.0, 0.0, 20.0);

/// Where each Seat's Player starts, in Seat order: the first is `PLAYER_START`, the rest stand in
/// the Start Room beside it, clear of the Box, the Wall Weapons and the Yard Door.
pub const SEAT_STARTS: [Vec3; 4] = [
    PLAYER_START,
    Vec3::new(-3.0, 0.0, 20.0),
    Vec3::new(3.0, 0.0, 20.0),
    Vec3::new(0.0, 0.0, 17.0),
];

/// Where the Player in Seat `seat` starts.
pub fn seat_start(seat: u8) -> Vec3 {
    SEAT_STARTS[usize::from(seat) % SEAT_STARTS.len()]
}

/// One storey.
const STOREY: f32 = 4.0;
pub(crate) const WALL_THICKNESS: f32 = 0.4;
const DOOR_WIDTH: f32 = 2.4;
const DOOR_HEIGHT: f32 = 3.0;
const RAIL_HEIGHT: f32 = 1.1;

/// Which Map a Run is played on. As a resource, it's the Map of the current (or just-ended) Run.
#[derive(Resource, Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapId {
    /// The greybox Map: five rooms ringing the Yard, the Balcony, and the south wing. The only Map
    /// so far.
    #[default]
    Greybox,
}

impl MapId {
    pub const ALL: [MapId; 1] = [MapId::Greybox];

    pub fn name(self) -> &'static str {
        match self {
            MapId::Greybox => "Greybox",
        }
    }
}

/// How high a chalk outline's centre sits on its wall.
const CHALK_HEIGHT: f32 = 1.5;

/// The chalk outlines: each Wall Weapon, the Zone it's in, where on a wall's face its centre
/// sits (x, z), and which way that face looks.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec2, Vec2); 5] = [
    // Either side of the Start Room's Yard Door, facing the Player at the start.
    (WeaponId::M14, ZoneId::StartRoom, Vec2::new(-5.0, 14.2), Vec2::Y),
    (WeaponId::Olympia, ZoneId::StartRoom, Vec2::new(5.0, 14.2), Vec2::Y),
    // On the outer wall, between two windows and clear of Juggernog.
    (WeaponId::Mp40, ZoneId::WestHall, Vec2::new(-25.8, -2.0), Vec2::X),
    // On the wall to the Yard.
    (WeaponId::TrenchGun, ZoneId::EastHall, Vec2::new(14.2, 4.0), Vec2::X),
    // On the solid Yard–Power Room wall, past the Gazebo.
    (WeaponId::Ak74u, ZoneId::Yard, Vec2::new(11.0, -13.8), Vec2::Y),
];

/// The Box Locations, in order: the Zone, where the Box's centre sits on the floor (x, z), and
/// which way its front faces (where the Player stands). Every Run starts the Box at the first.
pub const BOX_LOCATIONS: [(ZoneId, Vec2, Vec2); 5] = [
    // Against the back wall, behind the Player at the start.
    (ZoneId::StartRoom, Vec2::new(0.0, 25.35), Vec2::NEG_Y),
    // On the wall to the Yard, south of the Trench Gun.
    (ZoneId::EastHall, Vec2::new(14.65, -6.0), Vec2::X),
    // On the north wall, between the West Hall and Yard Doors.
    (ZoneId::DiningRoom, Vec2::new(-13.5, -14.65), Vec2::NEG_Y),
    // Against the East Hall wall.
    (ZoneId::Yard, Vec2::new(13.35, -2.0), Vec2::NEG_X),
    // Against the Storeroom's north wall, between the wing's two halves.
    (ZoneId::Storeroom, Vec2::new(0.0, -38.65), Vec2::NEG_Y),
];

const WINDOW_WIDTH: f32 = 1.6;
/// How far outside its window a window Spawn Point sits.
const WINDOW_SPAWN_OUT: f32 = 2.0;

/// One of the 20 windows in the outer walls, each the home of one Barrier (an index into
/// `WINDOWS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarrierId(pub u8);

/// The windows: the Zone each lets into, its centre on the wall (x, z), and the way out. Each is
/// planked up by its Barrier.
pub const WINDOWS: [(ZoneId, Vec2, Vec2); 20] = [
    (ZoneId::StartRoom, Vec2::new(-7.0, 26.0), Vec2::Y),
    (ZoneId::StartRoom, Vec2::new(7.0, 26.0), Vec2::Y),
    (ZoneId::WestHall, Vec2::new(-26.0, 18.0), Vec2::NEG_X),
    (ZoneId::WestHall, Vec2::new(-26.0, 4.0), Vec2::NEG_X),
    (ZoneId::WestHall, Vec2::new(-26.0, -8.0), Vec2::NEG_X),
    (ZoneId::EastHall, Vec2::new(26.0, 18.0), Vec2::X),
    (ZoneId::EastHall, Vec2::new(26.0, 4.0), Vec2::X),
    (ZoneId::EastHall, Vec2::new(26.0, -8.0), Vec2::X),
    (ZoneId::DiningRoom, Vec2::new(-18.0, -26.0), Vec2::NEG_Y),
    (ZoneId::DiningRoom, Vec2::new(-6.0, -26.0), Vec2::NEG_Y),
    (ZoneId::DiningRoom, Vec2::new(-26.0, -20.0), Vec2::NEG_X),
    (ZoneId::PowerRoom, Vec2::new(8.0, -26.0), Vec2::NEG_Y),
    (ZoneId::PowerRoom, Vec2::new(20.0, -26.0), Vec2::NEG_Y),
    // The south wing.
    (ZoneId::Workshop, Vec2::new(-28.0, -50.0), Vec2::NEG_Y),
    (ZoneId::Workshop, Vec2::new(-16.0, -50.0), Vec2::NEG_Y),
    (ZoneId::Workshop, Vec2::new(-34.0, -44.0), Vec2::NEG_X),
    (ZoneId::Storeroom, Vec2::new(0.0, -50.0), Vec2::NEG_Y),
    (ZoneId::GeneratorRoom, Vec2::new(16.0, -50.0), Vec2::NEG_Y),
    (ZoneId::GeneratorRoom, Vec2::new(28.0, -50.0), Vec2::NEG_Y),
    (ZoneId::GeneratorRoom, Vec2::new(34.0, -44.0), Vec2::X),
];

/// Where a window's Spawn Point stands: on the ground just outside it.
pub fn window_spawn(barrier: BarrierId) -> Vec3 {
    let (_, centre, out) = WINDOWS[barrier.0 as usize];
    let at = centre + out * WINDOW_SPAWN_OUT;
    Vec3::new(at.x, 0.0, at.y)
}

/// The Yard's Risers, where Zombies climb out of the ground (x, z), on open ground clear of the
/// sunken garden.
const RISERS: [Vec2; 4] = [
    Vec2::new(-10.5, 12.0),
    Vec2::new(10.0, 10.0),
    Vec2::new(-4.0, -11.0),
    Vec2::new(11.0, -10.0),
];

/// The Hellhound Spawn Points: the Zone each serves and where it stands on the floor. Spread
/// through every Zone, clear of walls, machines and cover, so a spot 10–25 m from the Player is
/// almost always open.
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 28] = [
    (ZoneId::StartRoom, Vec3::new(-11.0, 0.0, 20.0)),
    (ZoneId::StartRoom, Vec3::new(11.0, 0.0, 20.0)),
    (ZoneId::StartRoom, Vec3::new(-6.0, 0.0, 16.5)),
    (ZoneId::StartRoom, Vec3::new(6.0, 0.0, 23.0)),
    (ZoneId::WestHall, Vec3::new(-20.0, 0.0, 20.0)),
    (ZoneId::WestHall, Vec3::new(-21.0, 0.0, 4.0)),
    (ZoneId::WestHall, Vec3::new(-21.0, 0.0, -9.0)),
    (ZoneId::Balcony, Vec3::new(-12.0, STOREY, -8.0)),
    (ZoneId::Balcony, Vec3::new(-12.0, STOREY, 4.0)),
    (ZoneId::EastHall, Vec3::new(20.0, 0.0, 20.0)),
    (ZoneId::EastHall, Vec3::new(20.0, 0.0, 20.0)),
    (ZoneId::EastHall, Vec3::new(20.0, 0.0, -8.0)),
    (ZoneId::DiningRoom, Vec3::new(-20.0, 0.0, -20.0)),
    (ZoneId::DiningRoom, Vec3::new(-8.0, 0.0, -20.0)),
    (ZoneId::PowerRoom, Vec3::new(6.0, 0.0, -20.0)),
    (ZoneId::PowerRoom, Vec3::new(18.0, 0.0, -20.0)),
    (ZoneId::Yard, Vec3::new(-9.0, 0.0, 0.0)),
    (ZoneId::Yard, Vec3::new(0.0, 0.0, 0.0)),
    (ZoneId::Yard, Vec3::new(9.0, 0.0, 9.0)),
    (ZoneId::Yard, Vec3::new(-9.0, 0.0, -10.0)),
    (ZoneId::Yard, Vec3::new(10.0, 0.0, -12.0)),
    (ZoneId::Passage, Vec3::new(-12.4, 0.0, -36.8)),
    (ZoneId::Workshop, Vec3::new(-19.0, 0.0, -42.0)),
    (ZoneId::Workshop, Vec3::new(-8.0, 0.0, -46.0)),
    (ZoneId::Storeroom, Vec3::new(0.0, 0.0, -45.0)),
    (ZoneId::GeneratorRoom, Vec3::new(18.5, 0.0, -41.2)),
    (ZoneId::GeneratorRoom, Vec3::new(8.0, 0.0, -46.0)),
    (ZoneId::Tunnel, Vec3::new(14.0, -0.5, -32.0)),
];

/// The Special Spawn Points, where a Napalm or a Shrieker rises: the Zone each serves and where it
/// stands on the floor. One or two in each of the Start Room, West Hall, East Hall, Dining Room,
/// Power Room, the Yard, the Workshop and the Generator Room, on open spots the hounds use, clear
/// of the Vents.
const SPECIAL_SPAWNS: [(ZoneId, Vec3); 12] = [
    (ZoneId::StartRoom, Vec3::new(-11.0, 0.0, 20.0)),
    (ZoneId::StartRoom, Vec3::new(11.0, 0.0, 20.0)),
    (ZoneId::WestHall, Vec3::new(-20.0, 0.0, 20.0)),
    (ZoneId::WestHall, Vec3::new(-21.0, 0.0, -9.0)),
    (ZoneId::EastHall, Vec3::new(20.0, 0.0, 20.0)),
    (ZoneId::EastHall, Vec3::new(20.0, 0.0, -8.0)),
    (ZoneId::DiningRoom, Vec3::new(-20.0, 0.0, -20.0)),
    (ZoneId::PowerRoom, Vec3::new(18.0, 0.0, -20.0)),
    (ZoneId::Yard, Vec3::new(-9.0, 0.0, 0.0)),
    (ZoneId::Yard, Vec3::new(9.0, 0.0, 9.0)),
    (ZoneId::Workshop, Vec3::new(-19.0, 0.0, -42.0)),
    (ZoneId::GeneratorRoom, Vec3::new(18.5, 0.0, -41.2)),
];

/// The Vents Nova Crawlers drop from: the Zone each is in, and the spot on the floor below it.
/// Each opens `NOVA_VENT_HEIGHT` up, at the top of a one-way path-node edge down to that spot.
pub const VENTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::WestHall, Vec3::new(-21.0, 0.0, 4.0)),
    (ZoneId::EastHall, Vec3::new(21.0, 0.0, 4.0)),
    (ZoneId::DiningRoom, Vec3::new(-13.0, 0.0, -20.0)),
    (ZoneId::PowerRoom, Vec3::new(13.0, 0.0, -20.0)),
];

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
        // Bare ground round weathered brick.
        None => paints((0.15, 0.17, 0.12), (0.3, 0.21, 0.17), (0.42, 0.4, 0.36), (1.0, 0.75, 0.45)),
    }
}

/// The Pack-a-Punch Gazebo's own look, inside and out.
const GAZEBO: Palette = paints((0.2, 0.18, 0.22), (0.3, 0.22, 0.36), (0.66, 0.54, 0.24), (1.0, 0.8, 0.5));

/// The lamps: the Zone whose tint each takes, and where it hangs.
const LAMPS: [(ZoneId, Vec3); 14] = {
    use ZoneId::*;
    [
        (StartRoom, Vec3::new(0.0, STOREY - 0.5, 20.0)),
        (WestHall, Vec3::new(-20.0, STOREY - 0.5, 16.0)),
        (WestHall, Vec3::new(-20.0, 2.0 * STOREY - 0.5, 0.0)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, 16.0)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, -2.0)),
        (DiningRoom, Vec3::new(-13.0, STOREY - 0.5, -20.0)),
        (PowerRoom, Vec3::new(13.0, STOREY - 0.5, -20.0)),
        (Balcony, Vec3::new(-12.0, 2.0 * STOREY - 0.5, -2.0)),
        (Yard, Vec3::new(0.0, STOREY + 2.0, 0.0)),
        (Passage, Vec3::new(-12.0, STOREY - 0.5, -32.0)),
        (Tunnel, Vec3::new(14.0, STOREY - 0.5, -32.0)),
        (Workshop, Vec3::new(-19.0, STOREY - 0.5, -44.0)),
        (Storeroom, Vec3::new(0.0, STOREY - 0.5, -44.0)),
        (GeneratorRoom, Vec3::new(19.0, STOREY - 0.5, -44.0)),
    ]
};

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

/// Dark stone, for the dais.
const DAIS_STONE: Color = Color::srgb(0.36, 0.33, 0.3);
/// Riveted steel plate, for the control platform.
const STEEL_PLATE: Color = Color::srgb(0.3, 0.32, 0.34);
/// Bare concrete, for the loading dock.
const CONCRETE: Color = Color::srgb(0.42, 0.41, 0.38);
/// Weathered stone, for the sunken garden's steps.
const GARDEN_STONE: Color = Color::srgb(0.5, 0.48, 0.42);

/// The Yard's sunken garden, off to one side of its open middle.
const GARDEN: Sunken = Sunken { zone: ZoneId::Yard, min: Vec2::new(-7.5, 4.5), max: Vec2::new(-0.5, 11.5), depth: 0.6, rim: true };
/// The Tunnel's dip, wall to wall, between a ramp down and a ramp up.
const DIP: Sunken = Sunken { zone: ZoneId::Tunnel, min: Vec2::new(12.5, -35.5), max: Vec2::new(15.5, -28.5), depth: 0.5, rim: false };
/// Wet concrete, for the Tunnel's ramps.
const WET_CONCRETE: Color = Color::srgb(0.22, 0.23, 0.24);

/// East Hall's altar dais, against the outer wall between two windows.
const DAIS: RaisedFloor = RaisedFloor { zone: ZoneId::EastHall, min: Vec2::new(22.4, 8.0), max: Vec2::new(25.8, 14.0), height: 0.6, colour: DAIS_STONE };

/// The Workshop's loading dock along its north wall: too high to jump onto, so it's the ramp up,
/// or a clamber, and a hop down anywhere along its edge.
const DOCK: RaisedFloor = RaisedFloor { zone: ZoneId::Workshop, min: Vec2::new(-28.0, -40.6), max: Vec2::new(-16.0, -38.2), height: 1.1, colour: CONCRETE };

/// The Power Room's control platform, a few steps up at its east end, under the Power Switch.
pub const CONTROL_PLATFORM: RaisedFloor =
    RaisedFloor { zone: ZoneId::PowerRoom, min: Vec2::new(22.0, -22.6), max: Vec2::new(25.8, -17.4), height: 0.45, colour: STEEL_PLATE };

/// The Raised Floors.
pub const RAISED_FLOORS: [RaisedFloor; 3] = [
    DAIS,
    CONTROL_PLATFORM,
    DOCK,
];

/// The steps and ramps.
pub const SLOPES: [Slope; 8] = [
    // West Hall's ramp up its inner wall, from the floor at z = 8 to the landing at z = 0.
    Slope {
        zone: ZoneId::WestHall,
        foot: Vec3::new(-15.7, 0.0, 8.0),
        up: Vec2::NEG_Y,
        run: 8.0,
        rise: STOREY,
        width: 3.0,
        steps: false,
        colour: Color::srgb(0.42, 0.32, 0.21),
    },
    // Up the front of the altar dais.
    Slope { zone: ZoneId::EastHall, foot: Vec3::new(DAIS.min.x - 1.2, 0.0, 11.0), up: Vec2::X, run: 1.2, rise: DAIS.height, width: 3.0, steps: true, colour: DAIS_STONE },
    // Up onto the control platform, facing the Power Switch.
    Slope {
        zone: ZoneId::PowerRoom,
        foot: Vec3::new(CONTROL_PLATFORM.min.x - 0.9, 0.0, -20.0),
        up: Vec2::X,
        run: 0.9,
        rise: CONTROL_PLATFORM.height,
        width: 3.0,
        steps: true,
        colour: STEEL_PLATE,
    },
    // Up the west end of the loading dock.
    Slope { zone: ZoneId::Workshop, foot: Vec3::new(DOCK.min.x - 3.6, 0.0, -39.4), up: Vec2::X, run: 3.6, rise: DOCK.height, width: 2.4, steps: false, colour: CONCRETE },
    // Down into the sunken garden from the Yard's middle, on its east and south sides.
    Slope {
        zone: ZoneId::Yard,
        foot: Vec3::new(GARDEN.max.x - 1.2, -GARDEN.depth, 8.0),
        up: Vec2::X,
        run: 1.2,
        rise: GARDEN.depth,
        width: 3.0,
        steps: true,
        colour: GARDEN_STONE,
    },
    Slope {
        zone: ZoneId::Yard,
        foot: Vec3::new(-4.0, -GARDEN.depth, GARDEN.min.y + 1.2),
        up: Vec2::NEG_Y,
        run: 1.2,
        rise: GARDEN.depth,
        width: 3.0,
        steps: true,
        colour: GARDEN_STONE,
    },
    // Down into the Tunnel's dip from each end.
    Slope {
        zone: ZoneId::Tunnel,
        foot: Vec3::new(14.0, -DIP.depth, DIP.max.y - 2.0),
        up: Vec2::Y,
        run: 2.0,
        rise: DIP.depth,
        width: 3.0,
        steps: false,
        colour: WET_CONCRETE,
    },
    Slope {
        zone: ZoneId::Tunnel,
        foot: Vec3::new(14.0, -DIP.depth, DIP.min.y + 2.0),
        up: Vec2::NEG_Y,
        run: 2.0,
        rise: DIP.depth,
        width: 3.0,
        steps: false,
        colour: WET_CONCRETE,
    },
];

/// The sunken floors.
pub const SUNKEN: [Sunken; 2] = [GARDEN, DIP];

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

/// The Props. Low ones (up to about 0.8 m) can be jumped onto and anything up to about 1.2 m
/// clambered onto with a running jump; nothing a Player gets on top of lifts them out of a
/// Zombie's reach. Tall ones are walls to weave round. Training loops stay clear: clutter keeps to
/// the walls and corners, and a room's middle holds at most one island to run laps round.
pub const PROPS: &[Prop] = {
    use PropKind::*;
    use ZoneId::*;
    &[
        // The Start Room: a reception desk, and a bench by the West Hall end of the south wall.
        prop(Counter, StartRoom, (10.2, 0.0, 16.2), (3.6, 1.05, 0.8)),
        prop(Bench, StartRoom, (-11.7, 0.0, 14.55), (2.6, 0.5, 0.5)),
        // The Yard, round its edges: crates and drums under the Balcony, a sandbag wall by the
        // Dining Room Door, rubble in the north-east corner and a crate in the south-east.
        prop(Crate, Yard, (-13.15, 0.0, -5.4), (1.1, 1.6, 1.1)),
        prop(Crate, Yard, (-13.25, 0.0, -3.2), (0.9, 0.7, 0.9)),
        prop(Barrel, Yard, (-13.25, 0.0, 2.3), (0.65, 0.95, 0.65)),
        prop(Barrel, Yard, (-12.5, 0.0, 2.9), (0.65, 0.95, 0.65)),
        prop(Sandbags, Yard, (-2.0, 0.0, -12.5), (3.2, 1.05, 0.8)),
        prop(Rubble, Yard, (12.85, 0.0, 12.8), (1.5, 0.85, 1.6)),
        prop(Crate, Yard, (13.1, 0.0, -13.0), (1.0, 0.7, 1.0)),
        // West Hall, a library: bookcases on the walls, a reading table, and crates stored under
        // the ramp's landing.
        prop(Shelf, WestHall, (-25.5, 0.0, 13.6), (0.6, 2.4, 3.2)),
        prop(Shelf, WestHall, (-25.5, 0.0, -12.1), (0.6, 2.4, 3.0)),
        prop(Shelf, WestHall, (-23.1, 0.0, 25.5), (4.2, 2.4, 0.6)),
        prop(Table, WestHall, (-22.7, 0.0, 14.5), (1.2, 0.74, 2.8)),
        prop(Crate, WestHall, (-16.5, 0.0, -3.0), (1.2, 1.5, 1.2)),
        prop(Crate, WestHall, (-15.0, 0.0, 0.5), (0.9, 0.7, 0.9)),
        // East Hall, a chapel: the altar up on its dais, two pews before it, and columns.
        prop(Altar, EastHall, (25.1, DAIS.height, 11.0), (1.0, 1.0, 2.2)),
        prop(Bench, EastHall, (18.7, 0.0, 8.3), (0.6, 0.5, 3.0)),
        prop(Bench, EastHall, (18.7, 0.0, 13.7), (0.6, 0.5, 3.0)),
        prop(Pillar, EastHall, (17.5, 0.0, -1.0), (0.8, STOREY, 0.8)),
        prop(Pillar, EastHall, (22.5, 0.0, -1.0), (0.8, STOREY, 0.8)),
        prop(Pillar, EastHall, (17.5, 0.0, 17.0), (0.8, STOREY, 0.8)),
        prop(Pillar, EastHall, (22.5, 0.0, 22.0), (0.8, STOREY, 0.8)),
        // The Dining Room: two long tables each side of an aisle down the middle.
        prop(Table, DiningRoom, (-17.0, 0.0, -17.9), (5.0, 0.74, 1.0)),
        prop(Table, DiningRoom, (-9.0, 0.0, -17.9), (5.0, 0.74, 1.0)),
        prop(Table, DiningRoom, (-17.0, 0.0, -22.1), (5.0, 0.74, 1.0)),
        prop(Table, DiningRoom, (-9.0, 0.0, -22.1), (5.0, 0.74, 1.0)),
        // The Power Room: transformers along the north wall, and drums in the corner.
        prop(Machine, PowerRoom, (10.3, 0.0, -14.8), (2.6, 2.2, 1.2)),
        prop(Machine, PowerRoom, (14.7, 0.0, -14.8), (2.6, 2.2, 1.2)),
        prop(Barrel, PowerRoom, (1.8, 0.0, -24.5), (0.65, 0.95, 0.65)),
        prop(Barrel, PowerRoom, (2.6, 0.0, -25.1), (0.65, 0.95, 0.65)),
        // The Passage, half fallen in: rubble against each wall in turn.
        prop(Rubble, Passage, (-12.8, 0.0, -30.6), (1.0, 0.8, 2.0)),
        prop(Rubble, Passage, (-11.2, 0.0, -34.3), (1.0, 0.7, 1.8)),
        // The Workshop: a workbench island to run round, a tool cabinet up on the dock, a tool rack
        // and crates.
        prop(Table, Workshop, (-21.0, 0.0, -44.5), (7.0, 0.9, 1.4)),
        prop(Shelf, Workshop, (-21.0, DOCK.height, -38.55), (3.0, 1.8, 0.6)),
        prop(Shelf, Workshop, (-7.7, 0.0, -38.5), (3.4, 2.2, 0.6)),
        prop(Crate, Workshop, (-5.2, 0.0, -48.9), (1.2, 1.5, 1.2)),
        prop(Crate, Workshop, (-8.0, 0.0, -49.0), (0.9, 0.7, 0.9)),
        // The Storeroom: shelving down both side walls, either side of the Doors.
        prop(Shelf, Storeroom, (-3.5, 0.0, -48.0), (0.6, 2.2, 3.0)),
        prop(Shelf, Storeroom, (3.5, 0.0, -48.0), (0.6, 2.2, 3.0)),
        prop(Shelf, Storeroom, (-3.5, 0.0, -40.4), (0.6, 2.2, 2.0)),
        prop(Shelf, Storeroom, (3.5, 0.0, -40.4), (0.6, 2.2, 2.0)),
        // The Generator Room: the generator itself, an island to run round, with a workbench,
        // a crate and fuel drums by the walls.
        prop(Machine, GeneratorRoom, (19.5, 0.0, -44.0), (7.0, 2.4, 2.4)),
        prop(Table, GeneratorRoom, (22.0, 0.0, -38.7), (4.0, 0.9, 1.0)),
        prop(Crate, GeneratorRoom, (30.5, 0.0, -38.9), (1.2, 1.1, 1.2)),
        prop(Barrel, GeneratorRoom, (5.6, 0.0, -48.9), (0.7, 0.95, 0.7)),
        prop(Barrel, GeneratorRoom, (6.5, 0.0, -49.3), (0.7, 0.95, 0.7)),
        // The Balcony: sandbags at its north end, against the West Hall wall.
        prop(Sandbags, Balcony, (-13.4, STOREY, 6.2), (0.8, 1.05, 2.8)),
    ]
};

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

fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

fn door(a: ZoneId, b: ZoneId, price: u64) -> Gap {
    Gap::Door(Some((a, b)), Some(price))
}

pub fn spawn_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use ZoneId::*;

    let mut b = Builder::new(&mut commands, &mut meshes, &mut materials);
    let floor = |zone: ZoneId| palette(Some(zone)).floor;

    // Floors: one per Zone, round any sunken floor in it, then the Balcony and the ramp's landing
    // up at the first storey.
    for zone in [StartRoom, WestHall, EastHall, DiningRoom, PowerRoom, Yard, Passage, Workshop, Storeroom, GeneratorRoom, Tunnel] {
        let holes: Vec<Sunken> = SUNKEN.into_iter().filter(|sunken| sunken.zone == zone).collect();
        b.floor(zone, &holes);
    }
    b.block(Vec3::new(4.0, 0.2, 24.0), Vec3::new(-12.0, STOREY - 0.1, -2.0), floor(Balcony));
    b.block(Vec3::new(3.0, 0.2, 4.0), Vec3::new(-15.7, STOREY - 0.1, -2.0), floor(WestHall));

    // Ground outside the outer walls, where Zombies gather at the windows: round the ring, across
    // the gap between the ring and the south wing (just under the Passage floor, and either side of
    // the Tunnel), and round the wing.
    for (size, centre) in [
        (Vec3::new(60.0, 0.2, 4.0), Vec3::new(0.0, -0.1, 28.0)),
        (Vec3::new(4.0, 0.2, 52.0), Vec3::new(-28.0, -0.1, 0.0)),
        (Vec3::new(4.0, 0.2, 52.0), Vec3::new(28.0, -0.1, 0.0)),
        (Vec3::new(50.5, 0.2, 12.0), Vec3::new(-12.75, -0.11, -32.0)),
        (Vec3::new(22.5, 0.2, 12.0), Vec3::new(26.75, -0.11, -32.0)),
        (Vec3::new(76.0, 0.2, 4.0), Vec3::new(0.0, -0.1, -52.0)),
        (Vec3::new(4.0, 0.2, 12.0), Vec3::new(-36.0, -0.1, -44.0)),
        (Vec3::new(4.0, 0.2, 12.0), Vec3::new(36.0, -0.1, -44.0)),
    ] {
        b.block(size, centre, palette(None).floor);
    }

    // Outer walls, with the windows. West Hall's are two storeys tall, since its ramp climbs to
    // the first storey.
    let tall = 2.0 * STOREY;
    b.outer_wall(v(-26.0, 26.0), v(-14.0, 26.0), tall);
    b.outer_wall(v(-14.0, 26.0), v(26.0, 26.0), STOREY);
    b.outer_wall(v(-26.0, -14.0), v(-26.0, 26.0), tall);
    b.outer_wall(v(-26.0, -26.0), v(-26.0, -14.0), STOREY);
    // The south wall, with the Doors down to the Passage and the Tunnel.
    b.outer_wall(v(-26.0, -26.0), v(-13.5, -26.0), STOREY);
    b.wall(v(-13.5, -26.0), v(-10.5, -26.0), 0.0, STOREY, &[(1.5, door(DiningRoom, Passage, DOOR_PRICE_DINING_PASSAGE))]);
    b.outer_wall(v(-10.5, -26.0), v(12.5, -26.0), STOREY);
    b.wall(v(12.5, -26.0), v(15.5, -26.0), 0.0, STOREY, &[(1.5, door(PowerRoom, Tunnel, DOOR_PRICE_POWER_TUNNEL))]);
    b.outer_wall(v(15.5, -26.0), v(26.0, -26.0), STOREY);
    b.outer_wall(v(26.0, -26.0), v(26.0, 26.0), STOREY);

    // The south wing: the Passage and the Tunnel across the open ground, then the Workshop, the
    // Storeroom and the Generator Room side by side, open to the corridors at their north ends.
    for x in [-13.5, -10.5] {
        b.wall(v(x, -38.0), v(x, -26.0), 0.0, STOREY, &[]);
    }
    // The Tunnel's walls reach down past its dip.
    let below = -DIP.depth - 0.2;
    for x in [12.5, 15.5] {
        b.wall(v(x, -38.0), v(x, -26.0), below, STOREY - below, &[]);
    }
    let corridor = || Gap::Open { width: 3.0, height: DOOR_HEIGHT };
    b.wall(v(-34.0, -38.0), v(34.0, -38.0), 0.0, STOREY, &[(22.0, corridor()), (48.0, corridor())]);
    b.outer_wall(v(-34.0, -50.0), v(34.0, -50.0), STOREY);
    b.outer_wall(v(-34.0, -50.0), v(-34.0, -38.0), STOREY);
    b.outer_wall(v(34.0, -50.0), v(34.0, -38.0), STOREY);
    b.wall(v(-4.0, -50.0), v(-4.0, -38.0), 0.0, STOREY, &[(6.0, door(Workshop, Storeroom, DOOR_PRICE_WORKSHOP_STOREROOM))]);
    b.wall(v(4.0, -50.0), v(4.0, -38.0), 0.0, STOREY, &[(6.0, door(Storeroom, GeneratorRoom, DOOR_PRICE_STOREROOM_GENERATOR))]);

    // Start Room
    b.wall(v(-14.0, 14.0), v(-14.0, 26.0), 0.0, tall, &[(6.0, door(StartRoom, WestHall, DOOR_PRICE_START_WEST))]);
    b.wall(v(14.0, 14.0), v(14.0, 26.0), 0.0, STOREY, &[(6.0, door(StartRoom, EastHall, DOOR_PRICE_START_EAST))]);
    b.wall(v(-14.0, 14.0), v(14.0, 14.0), 0.0, STOREY, &[(14.0, door(StartRoom, Yard, DOOR_PRICE_START_YARD))]);

    // West Hall: solid to the Yard below, open to the Balcony above at the top of the ramp.
    b.wall(v(-14.0, -14.0), v(-14.0, 14.0), 0.0, STOREY, &[]);
    b.wall(v(-14.0, -14.0), v(-14.0, 14.0), STOREY, STOREY, &[(12.0, Gap::Open { width: 4.0, height: 3.0 })]);
    b.wall(v(-26.0, -14.0), v(-14.0, -14.0), 0.0, tall, &[(6.0, door(WestHall, DiningRoom, DOOR_PRICE_WEST_DINING))]);

    // East Hall: solid to the Yard.
    b.wall(v(14.0, -14.0), v(14.0, 14.0), 0.0, STOREY, &[]);
    b.wall(v(14.0, -14.0), v(26.0, -14.0), 0.0, STOREY, &[(6.0, door(EastHall, PowerRoom, DOOR_PRICE_EAST_POWER))]);

    // Dining Room and Power Room. The Yard–Power Room wall is solid.
    b.wall(v(0.0, -26.0), v(0.0, -14.0), 0.0, STOREY, &[(6.0, door(DiningRoom, PowerRoom, DOOR_PRICE_DINING_POWER))]);
    b.wall(v(-14.0, -14.0), v(0.0, -14.0), 0.0, STOREY, &[(7.0, door(Yard, DiningRoom, DOOR_PRICE_YARD_DINING))]);
    b.wall(v(0.0, -14.0), v(14.0, -14.0), 0.0, STOREY, &[]);

    // Balcony: a full wall at the south end (so no one walks onto the Dining wall), and rails
    // along the Yard side and the north end, with a gap in the Yard rail for the drop.
    b.wall(v(-14.0, -14.0), v(-10.0, -14.0), STOREY, STOREY, &[]);
    b.wall(v(-10.0, -14.0), v(-10.0, 10.0), STOREY, RAIL_HEIGHT, &[(18.0, Gap::Open { width: DOOR_WIDTH, height: RAIL_HEIGHT })]);
    b.wall(v(-14.0, 10.0), v(-10.0, 10.0), STOREY, RAIL_HEIGHT, &[]);
    for z in [-11.0, -2.0, 7.0] {
        b.block(Vec3::new(0.5, STOREY, 0.5), Vec3::new(-10.3, STOREY / 2.0, z), palette(Some(Yard)).trim);
    }

    // Raised and sunken floors, and the steps and ramps between floors.
    for raised in RAISED_FLOORS {
        b.raised_floor(&raised);
    }
    for sunken in SUNKEN {
        b.sunken(&sunken);
    }
    for slope in SLOPES {
        b.slope(&slope);
    }

    // The Pack-a-Punch Gazebo, behind a Power Door that stays shut until the Power comes on.
    let gazebo_height = DOOR_HEIGHT;
    b.repaint = Some(GAZEBO);
    b.wall(v(2.0, -10.0), v(8.0, -10.0), 0.0, gazebo_height, &[]);
    b.wall(v(2.0, -10.0), v(2.0, -4.0), 0.0, gazebo_height, &[]);
    b.wall(v(8.0, -10.0), v(8.0, -4.0), 0.0, gazebo_height, &[]);
    b.wall(v(2.0, -4.0), v(8.0, -4.0), 0.0, gazebo_height, &[(3.0, Gap::Door(Some((Yard, Yard)), None))]);
    b.repaint = None;
    b.block(Vec3::new(6.8, 0.2, 6.8), Vec3::new(5.0, gazebo_height + 0.1, -7.0), GAZEBO.trim);

    // The Props.
    for (nth, prop) in PROPS.iter().enumerate() {
        b.prop(prop, nth);
    }

    // Chalk outlines: a pale gun shape drawn on the wall, its Buyable just in front.
    let chalk_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.85),
        perceptual_roughness: 1.0,
        ..default()
    });
    let barrel = meshes.add(Cuboid::new(1.2, 0.12, 0.02));
    let grip = meshes.add(Cuboid::new(0.12, 0.3, 0.02));
    for (weapon, _, at, facing) in WALL_WEAPONS {
        let price = wall_price(weapon).expect("only Wall Weapons go on walls");
        let centre = Vec3::new(at.x, CHALK_HEIGHT, at.y);
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
    for (i, (zone, ..)) in WINDOWS.iter().enumerate() {
        let barrier = BarrierId(i as u8);
        commands.spawn((
            InRun,
            SpawnPoint {
                zone: *zone,
                kind: SpawnKind::Window(barrier),
            },
            Transform::from_translation(window_spawn(barrier)),
        ));
    }
    for at in RISERS {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone: Yard,
                kind: SpawnKind::Riser,
            },
            Transform::from_xyz(at.x, 0.0, at.y),
        ));
    }
    for (zone, at) in HELLHOUND_SPAWNS {
        commands.spawn((
            InRun,
            SpawnPoint {
                zone,
                kind: SpawnKind::Hellhound,
            },
            Transform::from_translation(at),
        ));
    }
    for (zone, at) in SPECIAL_SPAWNS {
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
    for (zone, at) in VENTS {
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

    // Lighting: a dim moon over the Yard, and a warm lamp in each room.
    commands.spawn((
        InRun,
        DirectionalLight {
            illuminance: 800.0,
            shadow_maps_enabled: true,
            color: Color::srgb(0.7, 0.75, 1.0),
            ..default()
        },
        Transform::default().looking_at(Vec3::new(-1.0, -3.0, -2.0), Vec3::Y),
    ));
    for (zone, at) in LAMPS {
        commands.spawn((
            InRun,
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
mod tests {
    use super::*;

    /// How far apart two colours are, as straight-line distance between their sRGB values.
    fn apart(a: Color, b: Color) -> f32 {
        let (a, b) = (a.to_srgba(), b.to_srgba());
        Vec3::new(a.red - b.red, a.green - b.green, a.blue - b.blue).length()
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
    fn every_chalk_outline_sits_in_its_zone() {
        for (weapon, zone, at, _) in WALL_WEAPONS {
            let (lo, hi) = zone.volume();
            let inside = (lo.x..=hi.x).contains(&at.x) && (lo.z..=hi.z).contains(&at.y);
            assert!(inside, "{weapon:?} is outside {}", zone.name());
            assert!(wall_price(weapon).is_some(), "{weapon:?} is not a Wall Weapon");
        }
    }

    /// Everything fixed to the Map's walls and floors, besides the walls and Doors themselves:
    /// a name, where it stands (x, y, z), and how far round it must stay clear.
    pub(super) fn fixtures() -> Vec<(String, Vec3, f32)> {
        use crate::equipment::EQUIPMENT_WALLS;
        use crate::perk::Perk;
        use crate::teleporter::{MAINFRAME_AT, PadId};

        let flat = |at: Vec2, y: f32| Vec3::new(at.x, y, at.y);
        let mut fixtures = Vec::new();
        for (weapon, _, at, _) in WALL_WEAPONS {
            fixtures.push((format!("{weapon:?} chalk"), flat(at, CHALK_HEIGHT), 0.8));
        }
        for (i, (zone, at, _)) in BOX_LOCATIONS.iter().enumerate() {
            fixtures.push((format!("Box Location {i} ({})", zone.name()), flat(*at, 0.0), 1.0));
        }
        for (i, (_, at, _)) in WINDOWS.iter().enumerate() {
            fixtures.push((format!("window {i}"), flat(*at, 1.1), WINDOW_WIDTH / 2.0));
        }
        for perk in Perk::ALL {
            fixtures.push((format!("{perk:?}"), perk.machine_spot().0, 0.7));
        }
        for (wall, _, at, _) in EQUIPMENT_WALLS {
            fixtures.push((format!("{wall:?} chalk"), at, 0.6));
        }
        for pad in PadId::ALL {
            fixtures.push((format!("{pad:?} Pad"), pad.at(), TELEPORTER_PAD_RADIUS));
        }
        fixtures.push(("Mainframe".into(), MAINFRAME_AT, 0.8));
        fixtures.push(("Power Switch".into(), crate::power::SWITCH_AT, 0.5));
        fixtures.push(("Pack-a-Punch".into(), crate::pack_a_punch::MACHINE_AT, 0.8));
        fixtures
    }

    #[test]
    fn nothing_on_the_map_stands_in_anything_else() {
        let fixtures = fixtures();
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

    /// Everything that takes up floor: a name, its Zone, its footprint, its bottom and top, and
    /// what may stand within it all the same: things up on or over a Raised Floor, or down on a
    /// sunken one's floor.
    fn footprints() -> Vec<(String, ZoneId, (Vec2, Vec2), (f32, f32), Box<dyn Fn(Vec3) -> bool>)> {
        let mut all: Vec<(String, ZoneId, (Vec2, Vec2), (f32, f32), Box<dyn Fn(Vec3) -> bool>)> = Vec::new();
        for prop in PROPS {
            let name = format!("the {:?} at {}", prop.kind, prop.at);
            all.push((name, prop.zone, footprint(prop), (prop.at.y, prop.at.y + prop.size.y), Box::new(|_| false)));
        }
        for raised in RAISED_FLOORS {
            let top = raised.height;
            let name = format!("the Raised Floor in {}", raised.zone.name());
            all.push((name, raised.zone, (raised.min, raised.max), (0.0, top), Box::new(move |at: Vec3| at.y >= top - 0.05)));
        }
        for slope in SLOPES {
            let end = Vec2::new(slope.foot.x, slope.foot.z) + slope.up * slope.run;
            let across = Vec2::new(slope.up.y.abs(), slope.up.x.abs()) * slope.width / 2.0;
            let (a, b) = (Vec2::new(slope.foot.x, slope.foot.z), end);
            let name = format!("the steps or ramp at {}", slope.foot);
            all.push((name, slope.zone, (a.min(b) - across, a.max(b) + across), (slope.foot.y, slope.foot.y + slope.rise), Box::new(|_| false)));
        }
        for sunken in SUNKEN {
            let floor = -sunken.depth;
            let name = format!("the sunken floor in {}", sunken.zone.name());
            all.push((name, sunken.zone, (sunken.min, sunken.max), (floor, 0.0), Box::new(move |at: Vec3| (at.y - floor).abs() < 0.05)));
        }
        all
    }

    #[test]
    fn nothing_on_the_floor_crowds_anything_fixed_to_the_map() {
        use crate::nav::zone_at;
        use crate::teleporter::{ARRIVAL_AT, HOUNDS_AROUND};
        use ZoneId::*;
        // What must stay clear, the Zone it's in, and how much room round it: what's on the walls
        // and floors with space to stand at it, where Zombies land climbing in at the windows,
        // where Nova Crawlers drop, where the Players start and Teleport to, where the
        // Teleporter's hounds land, and every Spawn Point. A wall between is room enough.
        let mut keep: Vec<(String, Option<ZoneId>, Vec3, f32)> = fixtures()
            .into_iter()
            .map(|(name, at, clear)| (name, zone_at(at.with_y(at.y.min(STOREY + 0.5))), at, clear + 0.6))
            .collect();
        for (i, (zone, centre, _)) in WINDOWS.iter().enumerate() {
            keep.push((format!("window {i}'s landing"), Some(*zone), Vec3::new(centre.x, 0.0, centre.y), 2.6));
        }
        for (zone, at) in VENTS {
            keep.push((format!("{}'s Vent drop", zone.name()), Some(zone), at, 1.0));
        }
        for at in crate::trap::switch_spots() {
            keep.push(("a Trap switch".into(), zone_at(at), at, 1.0));
        }
        for (i, at) in SEAT_STARTS.iter().enumerate() {
            keep.push((format!("Seat {i}"), Some(StartRoom), *at, PLAYER_RADIUS + 0.8));
        }
        keep.push(("the Teleporter's landing".into(), Some(StartRoom), ARRIVAL_AT, PLAYER_RADIUS + 0.8));
        let ring = HELLHOUND_AROUND_MAX_DIST + HELLHOUND_RADIUS + 0.1;
        keep.push(("the Teleporter's hounds".into(), Some(StartRoom), HOUNDS_AROUND, ring));
        for at in RISERS {
            keep.push((format!("the Riser at {at}"), Some(Yard), Vec3::new(at.x, 0.0, at.y), 1.0));
        }
        for (zone, at) in HELLHOUND_SPAWNS.iter().chain(&SPECIAL_SPAWNS) {
            keep.push((format!("the {} Spawn Point at {at}", zone.name()), Some(*zone), *at, 0.9));
        }
        let mut crowded = Vec::new();
        for (what, its_zone, (min, max), (bottom, top), may_stand) in footprints() {
            for (name, zone, at, clear) in &keep {
                let gap = off(min, max, *at);
                let same_storey = at.y > bottom - 2.5 && at.y < top + 2.5;
                if *zone == Some(its_zone) && same_storey && gap < *clear && !may_stand(*at) {
                    crowded.push(format!("{what} is {gap:.2} m from {name}"));
                }
            }
        }
        assert!(crowded.is_empty(), "{crowded:#?}");
    }

    /// The floor `prop` stands on: the Zone's own, a Raised Floor's top, or the Balcony.
    fn floor_under(prop: &Prop) -> Option<f32> {
        let (min, max) = (Vec2::new(prop.at.x, prop.at.z) - Vec2::new(prop.size.x, prop.size.z) / 2.0, Vec2::new(prop.at.x, prop.at.z) + Vec2::new(prop.size.x, prop.size.z) / 2.0);
        let raised = RAISED_FLOORS
            .iter()
            .find(|raised| raised.zone == prop.zone && min.cmpge(raised.min).all() && max.cmple(raised.max).all())
            .map(|raised| raised.height);
        let ground = if prop.zone == ZoneId::Balcony { STOREY } else { 0.0 };
        [raised, Some(ground)].into_iter().flatten().find(|&floor| (floor - prop.at.y).abs() < 1e-3)
    }

    #[test]
    fn every_prop_stands_on_a_floor_in_its_zone_clear_of_the_doors() {
        use bevy::ecs::system::RunSystemOnce;
        let mut sim = crate::zombie::headless::Sim::new(60.0, |_| {});
        let doors: Vec<Vec3> = sim
            .world_mut()
            .run_system_once(|doors: Query<&Transform, With<crate::door::Door>>| doors.iter().map(|t| t.translation).collect::<Vec<_>>())
            .unwrap();
        assert!(!doors.is_empty());
        for prop in PROPS {
            let (lo, hi) = prop.zone.volume();
            let half = Vec3::new(prop.size.x, 0.0, prop.size.z) / 2.0;
            let (min, max) = (prop.at - half, prop.at + half);
            assert!(min.x >= lo.x && max.x <= hi.x && min.z >= lo.z && max.z <= hi.z, "the {:?} at {} sticks out of {}", prop.kind, prop.at, prop.zone.name());
            assert!(floor_under(prop).is_some(), "the {:?} at {} stands on no floor", prop.kind, prop.at);
            for door in &doors {
                let gap = off_prop(prop, *door);
                assert!(!beside(prop, *door) || gap >= 1.8, "the {:?} at {} is {gap:.2} m from the Door at {door}", prop.kind, prop.at);
            }
        }
    }

    /// Nothing a Player can get on top of lifts them out of a Zombie's reach. A Prop is climbed
    /// onto from the floor it stands on if it's no taller than a running jump clambers, or from
    /// another such Prop close beside it; every one that can be must be low enough over its floor,
    /// and narrow enough, for a Zombie at its side to swipe a Player anywhere on it.
    #[test]
    fn no_prop_lifts_a_player_out_of_reach() {
        let clamber = PLAYER_JUMP_SPEED * PLAYER_JUMP_SPEED / (2.0 * GRAVITY) + PLAYER_RADIUS;
        let top = |prop: &Prop| prop.at.y + prop.size.y;
        let footprint = |prop: &Prop| (Vec2::new(prop.at.x - prop.size.x / 2.0, prop.at.z - prop.size.z / 2.0), Vec2::new(prop.at.x + prop.size.x / 2.0, prop.at.z + prop.size.z / 2.0));
        let gap = |a: &Prop, b: &Prop| {
            let ((amin, amax), (bmin, bmax)) = (footprint(a), footprint(b));
            (amin - bmax).max(bmin - amax).max(Vec2::ZERO).length()
        };
        let mut climbable: Vec<bool> = PROPS.iter().map(|prop| prop.size.y <= clamber).collect();
        loop {
            let more: Vec<bool> = PROPS
                .iter()
                .enumerate()
                .map(|(i, prop)| {
                    climbable[i]
                        || PROPS.iter().enumerate().any(|(j, from)| {
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
        for (prop, _) in PROPS.iter().zip(&climbable).filter(|(_, up)| **up) {
            let floor = floor_under(prop).unwrap();
            let above = top(prop) + player_half - (floor + zombie_half);
            assert!(above < 1.5, "a Player on the {:?} at {} stands {above:.2} m over a Zombie's middle", prop.kind, prop.at);
            let narrow = prop.size.x.min(prop.size.z);
            assert!(narrow / 2.0 + ZOMBIE_RADIUS <= ZOMBIE_REACH, "the {:?} at {} is too wide to swipe across", prop.kind, prop.at);
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
        let mut sim = crate::zombie::headless::Sim::new(60.0, |_| {});
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
        assert!(blocked.is_empty(), "not on open floor: {blocked:#?}");
    }

    /// The Yard stays trainable: loops to lead a Train round the Gazebo, round the sunken garden
    /// and round the whole Yard, each a level lane at least 1.6 m wide with nothing in it.
    #[test]
    fn the_yard_keeps_clear_loops_to_train_round() {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;
        const ROUND_THE_GAZEBO: &[Vec2] = & [Vec2::new(0.0, -2.0), Vec2::new(10.5, -2.0), Vec2::new(10.5, -11.8), Vec2::new(0.4, -11.0)];
        const ROUND_THE_GARDEN: &[Vec2] = & [Vec2::new(-9.0, 2.8), Vec2::new(0.9, 2.8), Vec2::new(0.9, 12.6), Vec2::new(-9.0, 12.6)];
        const ROUND_THE_YARD: &[Vec2] =
            &[Vec2::new(-9.0, 12.6), Vec2::new(11.0, 12.6), Vec2::new(11.0, -11.8), Vec2::new(1.0, -11.0), Vec2::new(-9.0, -11.0)];
        let mut sim = crate::zombie::headless::Sim::new(60.0, |_| {});
        let blocked: Vec<String> = sim
            .world_mut()
            .run_system_once(|spatial: SpatialQuery| {
                let map = SpatialQueryFilter::from_mask(GameLayer::World);
                let lane = Collider::sphere(0.8);
                let mut blocked = Vec::new();
                for (name, corners) in [("the Gazebo", ROUND_THE_GAZEBO), ("the garden", ROUND_THE_GARDEN), ("the Yard", ROUND_THE_YARD)] {
                    for i in 0..corners.len() {
                        let (a, b) = (corners[i], corners[(i + 1) % corners.len()]);
                        let (from, to) = (Vec3::new(a.x, 1.0, a.y), Vec3::new(b.x, 1.0, b.y));
                        let config = ShapeCastConfig::from_max_distance(from.distance(to));
                        if spatial.cast_shape(&lane, from, Quat::IDENTITY, Dir3::new(to - from).unwrap(), &config, &map).is_some() {
                            blocked.push(format!("round {name}: {a} to {b} is cluttered"));
                        }
                        let level = (0..=20).all(|k| {
                            let at = from.lerp(to, k as f32 / 20.0);
                            spatial.cast_ray(at, Dir3::NEG_Y, 1.5, true, &map).is_some_and(|hit| (hit.distance - 1.0).abs() < 0.05)
                        });
                        if !level {
                            blocked.push(format!("round {name}: {a} to {b} isn't level"));
                        }
                    }
                }
                blocked
            })
            .unwrap();
        assert!(blocked.is_empty(), "{blocked:#?}");
    }

    #[test]
    fn every_seat_starts_in_the_start_room_on_open_floor_and_can_walk_out() {
        use crate::nav::NavGraph;
        use crate::target::path_distance;
        assert_eq!(SEAT_STARTS[0], PLAYER_START, "solo starts where it always did");
        let graph = NavGraph::new();
        let all_open: Vec<_> = ZoneId::ALL.iter().flat_map(|&a| ZoneId::ALL.iter().map(move |&b| (a, b))).collect();
        let yard = Vec3::new(0.0, 0.0, 0.0);
        for (i, &seat) in SEAT_STARTS.iter().enumerate() {
            let (lo, hi) = ZoneId::StartRoom.volume();
            assert!((lo.x + 2.0..=hi.x - 2.0).contains(&seat.x) && (lo.z + 2.0..=hi.z - 2.0).contains(&seat.z), "Seat {i} is by a wall");
            for (name, at, clear) in fixtures() {
                let gap = Vec2::new(at.x - seat.x, at.z - seat.z).length();
                assert!(gap >= clear + PLAYER_RADIUS + 0.5, "Seat {i} is only {gap:.2} m from {name}");
            }
            for &other in &SEAT_STARTS[i + 1..] {
                assert!(seat.distance(other) >= 2.5, "Seats are only {:.2} m apart", seat.distance(other));
            }
            assert!(path_distance(&graph, seat, yard, &all_open, |_| true).is_some(), "Seat {i} can't reach the Yard");
        }
    }

    #[test]
    fn the_teleporter_pads_are_far_from_each_other_and_the_mainframe() {
        use crate::teleporter::{MAINFRAME_AT, PadId};
        let [a, b] = PadId::ALL.map(PadId::at);
        assert!(a.distance(b) > 50.0, "the Pads are {:.1} m apart", a.distance(b));
        for pad in [a, b] {
            assert!(pad.distance(MAINFRAME_AT) > 60.0);
        }
    }

    #[test]
    fn every_box_location_sits_in_its_zone() {
        for (zone, at, _) in BOX_LOCATIONS {
            let (lo, hi) = zone.volume();
            let inside = (lo.x..=hi.x).contains(&at.x) && (lo.z..=hi.z).contains(&at.y);
            assert!(inside, "a Box Location is outside {}", zone.name());
        }
        assert_eq!(BOX_LOCATIONS[0].0, ZoneId::StartRoom, "every Run starts the Box in the Start Room");
    }

    #[test]
    fn every_hellhound_spawn_point_stands_in_its_zone() {
        for (zone, at) in HELLHOUND_SPAWNS {
            // Just above the floor, which sits a little inside the Zone.
            assert_eq!(crate::nav::zone_at(at + Vec3::Y * 0.5), Some(zone), "{at} is outside {}", zone.name());
        }
        for zone in ZoneId::ALL {
            assert!(HELLHOUND_SPAWNS.iter().any(|(z, _)| *z == zone), "{} has no Hellhound Spawn Point", zone.name());
        }
    }

    #[test]
    fn special_spawn_points_stand_in_their_zones_one_or_two_each() {
        use ZoneId::*;
        for (zone, at) in SPECIAL_SPAWNS {
            assert_eq!(crate::nav::zone_at(at + Vec3::Y * 0.5), Some(zone), "{at} is outside {}", zone.name());
        }
        for zone in [StartRoom, WestHall, EastHall, DiningRoom, PowerRoom, Yard, Workshop, GeneratorRoom] {
            let count = SPECIAL_SPAWNS.iter().filter(|(z, _)| *z == zone).count();
            assert!((1..=2).contains(&count), "{} has {count} Special Spawn Points", zone.name());
        }
        assert!(!SPECIAL_SPAWNS.iter().any(|(z, _)| *z == Balcony));
    }

    #[test]
    fn vents_open_over_the_floor_of_four_indoor_zones() {
        use ZoneId::*;
        let zones: Vec<_> = VENTS.iter().map(|(zone, _)| *zone).collect();
        assert_eq!(zones, [WestHall, EastHall, DiningRoom, PowerRoom]);
        for (zone, at) in VENTS {
            assert_eq!(crate::nav::zone_at(at + Vec3::Y * 0.1), Some(zone), "under the Vent");
            assert_eq!(crate::nav::zone_at(at + Vec3::Y * NOVA_VENT_HEIGHT), Some(zone), "the Vent");
        }
    }

    /// Rays straight out through every window on the World layer, from 1 m inside to 1 m outside:
    /// what solid wall stands in the way at `height` above the floor.
    fn window_walls_at(height: f32) -> Vec<(usize, bool)> {
        use avian3d::prelude::*;
        use bevy::ecs::system::RunSystemOnce;

        use crate::layers::GameLayer;

        let mut sim = crate::zombie::headless::Sim::new(60.0, |_| {});
        sim.world_mut()
            .run_system_once(move |spatial: SpatialQuery| {
                let filter = SpatialQueryFilter::from_mask(GameLayer::World);
                WINDOWS
                    .iter()
                    .enumerate()
                    .map(|(i, (_, centre, out))| {
                        let from = Vec3::new(centre.x, height, centre.y) - Vec3::new(out.x, 0.0, out.y);
                        let dir = Dir3::new(Vec3::new(out.x, 0.0, out.y)).unwrap();
                        (i, spatial.cast_ray(from, dir, 2.0, true, &filter).is_some())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap()
    }

    #[test]
    fn every_window_has_a_solid_sill_below_its_opening() {
        let open: Vec<usize> = window_walls_at(0.45).into_iter().filter(|(_, hit)| !hit).map(|(i, _)| i).collect();
        assert!(open.is_empty(), "windows with no Sill at knee height: {open:?}");
    }

    #[test]
    fn a_window_opening_is_open_to_everything_but_the_player() {
        let shut: Vec<usize> = window_walls_at(1.5).into_iter().filter(|(_, hit)| *hit).map(|(i, _)| i).collect();
        assert!(shut.is_empty(), "windows with solid wall across the opening: {shut:?}");
    }
}
