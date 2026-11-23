//! The Greybox Map: a ring of five indoor Zones around an open Yard, plus a Balcony over the Yard
//! reached by a ramp in West Hall, with a one-way drop back down. South of the ring, across open
//! ground, lies the south wing: the Workshop, the Storeroom and the Generator Room, reached down
//! the Passage from the Dining Room and the Tunnel from the Power Room. The Teleporter's Pads sit
//! at the wing's far corners, and Linking both opens the Gazebo in the Yard, with Pack-a-Punch
//! inside: the Power alone doesn't, so Pack-a-Punch is earned with a trip round the south wing.

use bevy::prelude::*;

use crate::weapons::arsenal::WeaponId;
use crate::enemies::nav::Gate;
use crate::enemies::sapper::SapperTiming;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

use super::build::{Builder, Gap};
use super::textures::Material;
use super::{
    BarrierId, DOOR_HEIGHT, DOOR_WIDTH, Infill, Layout, PARAPET_THICKNESS, Palette, Prop, PropKind, RAIL_HEIGHT, RaisedFloor, STOREY, Slope, Sunken, door, materials, paints, palette,
    infill, prop, v,
};

/// Where the Player's feet start: the middle of the Start Room, facing the Yard.
pub const PLAYER_START: Vec3 = Vec3::new(0.0, 0.0, 20.0);

/// How high a chalk outline's centre sits on its wall.
const CHALK: f32 = 1.5;

pub(super) const GREYBOX: Layout = Layout {
    zones: &{
        use ZoneId::*;
        [StartRoom, WestHall, Balcony, EastHall, DiningRoom, PowerRoom, Yard, Passage, Workshop, Storeroom, GeneratorRoom, Tunnel]
    },
    // Seats beside the first, in the Start Room clear of the Box, the Wall Weapons and the Yard
    // Door.
    seats: [PLAYER_START, Vec3::new(-3.0, 0.0, 20.0), Vec3::new(3.0, 0.0, 20.0), Vec3::new(0.0, 0.0, 17.0)],
    wall_weapons: &WALL_WEAPONS,
    windows: &WINDOWS,
    risers: &RISERS,
    wading: &[],
    hellhound_spawns: &HELLHOUND_SPAWNS,
    special_spawns: &SPECIAL_SPAWNS,
    vents: &VENTS,
    raised_floors: &RAISED_FLOORS,
    infills: &INFILLS,
    slopes: &SLOPES,
    sunken: &SUNKEN,
    water: &[],
    canals: &[],
    props: PROPS,
    lamps: &LAMPS,
    fixed_joins: &FIXED_JOINS,
    shifting: &[],
    wings: &[],
    nav_nodes: NODES,
    nav_edges: EDGES,
    jump_pads: &[],
    ziplines: &[],
    sapper: SapperTiming::DEFAULT,
    sections: &[],
    generators: &[],
    darkness: false,
    outdoor: &[ZoneId::Yard, ZoneId::Balcony],
    roofed: true,
    outside: Some(super::Outside { floor: super::textures::Material::AsphaltFloor, wall: super::textures::Material::ConcreteWall006 }),
    sun: Vec3::new(-1.0, -3.0, -2.0),
    look: crate::maps::look::DUSK,
    plan: (Vec2::new(-40.0, -56.0), Vec2::new(40.0, 32.0)),
    build,
};

/// The chalk outlines: each Wall Weapon, the Zone it's in, where on a wall's face its centre
/// sits, and which way that face looks.
const WALL_WEAPONS: [(WeaponId, ZoneId, Vec3, Vec2); 5] = [
    // Either side of the Start Room's Yard Door, facing the Player at the start.
    (WeaponId::M14, ZoneId::StartRoom, Vec3::new(-5.0, CHALK, 14.2), Vec2::Y),
    (WeaponId::Olympia, ZoneId::StartRoom, Vec3::new(5.0, CHALK, 14.2), Vec2::Y),
    // On the outer wall, between two windows and clear of Juggernog.
    (WeaponId::Mp40, ZoneId::WestHall, Vec3::new(-25.8, CHALK, -2.0), Vec2::X),
    // On the wall to the Yard.
    (WeaponId::TrenchGun, ZoneId::EastHall, Vec3::new(14.2, CHALK, 4.0), Vec2::X),
    // On the solid Yard–Power Room wall, past the Gazebo.
    (WeaponId::Ak74u, ZoneId::Yard, Vec3::new(11.0, CHALK, -13.8), Vec2::Y),
];

/// The Box Locations, in order. Every Run starts the Box at the first, in the Dining Room, two
/// Doors from the Start Room.
pub(super) const BOX_LOCATIONS: [(ZoneId, Vec3, Vec2); 5] = [
    // On the Dining Room's north wall, between the West Hall and Yard Doors.
    (ZoneId::DiningRoom, Vec3::new(-13.5, 0.0, -14.65), Vec2::NEG_Y),
    // Against West Hall's north wall, just through the Door from the Start Room.
    (ZoneId::WestHall, Vec3::new(-19.0, 0.0, 25.35), Vec2::NEG_Y),
    // On the wall to the Yard, south of the Trench Gun.
    (ZoneId::EastHall, Vec3::new(14.65, 0.0, -6.0), Vec2::X),
    // Against the East Hall wall.
    (ZoneId::Yard, Vec3::new(13.35, 0.0, -2.0), Vec2::NEG_X),
    // Against the Storeroom's north wall, between the wing's two halves.
    (ZoneId::Storeroom, Vec3::new(0.0, 0.0, -38.65), Vec2::NEG_Y),
];

/// The 21 windows in the outer walls.
pub(super) const WINDOWS: [(ZoneId, Vec3, Vec2); 21] = [
    (ZoneId::StartRoom, Vec3::new(-7.0, 0.0, 26.0), Vec2::Y),
    (ZoneId::StartRoom, Vec3::new(7.0, 0.0, 26.0), Vec2::Y),
    (ZoneId::WestHall, Vec3::new(-26.0, 0.0, 18.0), Vec2::NEG_X),
    (ZoneId::WestHall, Vec3::new(-26.0, 0.0, 4.0), Vec2::NEG_X),
    (ZoneId::WestHall, Vec3::new(-26.0, 0.0, -8.0), Vec2::NEG_X),
    (ZoneId::EastHall, Vec3::new(26.0, 0.0, 18.0), Vec2::X),
    (ZoneId::EastHall, Vec3::new(26.0, 0.0, 4.0), Vec2::X),
    (ZoneId::EastHall, Vec3::new(26.0, 0.0, -8.0), Vec2::X),
    (ZoneId::DiningRoom, Vec3::new(-18.0, 0.0, -26.0), Vec2::NEG_Y),
    (ZoneId::DiningRoom, Vec3::new(-6.0, 0.0, -26.0), Vec2::NEG_Y),
    (ZoneId::DiningRoom, Vec3::new(-26.0, 0.0, -20.0), Vec2::NEG_X),
    (ZoneId::PowerRoom, Vec3::new(8.0, 0.0, -26.0), Vec2::NEG_Y),
    (ZoneId::PowerRoom, Vec3::new(20.0, 0.0, -26.0), Vec2::NEG_Y),
    // The south wing.
    (ZoneId::Workshop, Vec3::new(-28.0, 0.0, -50.0), Vec2::NEG_Y),
    (ZoneId::Workshop, Vec3::new(-16.0, 0.0, -50.0), Vec2::NEG_Y),
    (ZoneId::Workshop, Vec3::new(-34.0, 0.0, -44.0), Vec2::NEG_X),
    (ZoneId::Storeroom, Vec3::new(0.0, 0.0, -50.0), Vec2::NEG_Y),
    (ZoneId::GeneratorRoom, Vec3::new(16.0, 0.0, -50.0), Vec2::NEG_Y),
    (ZoneId::GeneratorRoom, Vec3::new(28.0, 0.0, -50.0), Vec2::NEG_Y),
    (ZoneId::GeneratorRoom, Vec3::new(34.0, 0.0, -44.0), Vec2::X),
    // Up at the first storey, in the Balcony's south end, onto the roof over the Dining Room.
    (ZoneId::Balcony, Vec3::new(-12.0, STOREY, -14.0), Vec2::NEG_Y),
];

/// The flat roof outside the Balcony's window, up at the first storey over the Dining Room: its
/// corners (x, z). The Balcony's own wall bounds its north side, and a parapet the other three:
/// below them all is the open Dining Room.
const BALCONY_ROOF: (Vec2, Vec2) = (Vec2::new(-14.5, -18.5), Vec2::new(-9.5, -14.0));

/// The Yard's Risers, where Zombies climb out of the ground, on open ground clear of the sunken
/// garden.
const RISERS: [(ZoneId, Vec3); 4] = [
    (ZoneId::Yard, Vec3::new(-10.5, 0.0, 12.0)),
    (ZoneId::Yard, Vec3::new(10.0, 0.0, 10.0)),
    (ZoneId::Yard, Vec3::new(-4.0, 0.0, -11.0)),
    (ZoneId::Yard, Vec3::new(11.0, 0.0, -10.0)),
];

/// The Hellhound Spawn Points, spread through every Zone, clear of walls, machines and cover, so
/// a spot 10–25 m from the Player is almost always open.
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

/// The Special Spawn Points, where a Napalm or a Shrieker rises. One or two in each of the Start
/// Room, West Hall, East Hall, Dining Room, Power Room, the Yard, the Workshop and the Generator
/// Room, on open spots the hounds use, clear of the Vents.
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

/// The Vents Nova Crawlers drop from, in West Hall, East Hall, the Dining Room and the Power Room.
const VENTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::WestHall, Vec3::new(-21.0, 0.0, 4.0)),
    (ZoneId::EastHall, Vec3::new(21.0, 0.0, 4.0)),
    (ZoneId::DiningRoom, Vec3::new(-13.0, 0.0, -20.0)),
    (ZoneId::PowerRoom, Vec3::new(13.0, 0.0, -20.0)),
];

/// Joins that never close: the ramp from West Hall up to the Balcony, the drop from the Balcony
/// into the Yard, and the open ends of the Passage and the Tunnel into the south wing. The drop is
/// one-way to walk but counts both ways for activation.
const FIXED_JOINS: [(ZoneId, ZoneId); 4] = [
    (ZoneId::WestHall, ZoneId::Balcony),
    (ZoneId::Balcony, ZoneId::Yard),
    (ZoneId::Passage, ZoneId::Workshop),
    (ZoneId::Tunnel, ZoneId::GeneratorRoom),
];

/// The Pack-a-Punch Gazebo's own look, inside and out.
const GAZEBO: Palette = paints((0.2, 0.18, 0.22), (0.3, 0.22, 0.36), (0.66, 0.54, 0.24), (1.0, 0.8, 0.5));

/// The lamps: the Zone whose tint each takes, and where it hangs. Every point of an indoor
/// floor, clear of the walls, is within `LAMP_REACH` of one in its Zone (a test checks).
const LAMPS: [(ZoneId, Vec3); 34] = {
    use ZoneId::*;
    [
        (StartRoom, Vec3::new(-9.8, STOREY - 0.5, 20.0)),
        (StartRoom, Vec3::new(-3.2, STOREY - 0.5, 20.0)),
        (StartRoom, Vec3::new(3.2, STOREY - 0.5, 20.0)),
        (StartRoom, Vec3::new(9.8, STOREY - 0.5, 20.0)),
        (WestHall, Vec3::new(-20.0, STOREY - 0.5, -9.2)),
        (WestHall, Vec3::new(-20.0, STOREY - 0.5, -1.6)),
        (WestHall, Vec3::new(-20.0, STOREY - 0.5, 6.0)),
        (WestHall, Vec3::new(-20.0, STOREY - 0.5, 13.6)),
        (WestHall, Vec3::new(-20.0, STOREY - 0.5, 21.2)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, -9.2)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, -1.6)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, 6.0)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, 13.6)),
        (EastHall, Vec3::new(20.0, STOREY - 0.5, 21.2)),
        (DiningRoom, Vec3::new(-21.0, STOREY - 0.5, -20.0)),
        (DiningRoom, Vec3::new(-13.0, STOREY - 0.5, -20.0)),
        (DiningRoom, Vec3::new(-5.0, STOREY - 0.5, -20.0)),
        (PowerRoom, Vec3::new(5.0, STOREY - 0.5, -20.0)),
        (PowerRoom, Vec3::new(13.0, STOREY - 0.5, -20.0)),
        (PowerRoom, Vec3::new(21.0, STOREY - 0.5, -20.0)),
        (Passage, Vec3::new(-12.0, STOREY - 0.5, -32.0)),
        (Workshop, Vec3::new(-29.5, STOREY - 0.5, -44.0)),
        (Workshop, Vec3::new(-22.5, STOREY - 0.5, -44.0)),
        (Workshop, Vec3::new(-15.5, STOREY - 0.5, -44.0)),
        (Workshop, Vec3::new(-8.5, STOREY - 0.5, -44.0)),
        (Storeroom, Vec3::new(0.0, STOREY - 0.5, -44.0)),
        (GeneratorRoom, Vec3::new(8.5, STOREY - 0.5, -44.0)),
        (GeneratorRoom, Vec3::new(15.5, STOREY - 0.5, -44.0)),
        (GeneratorRoom, Vec3::new(22.5, STOREY - 0.5, -44.0)),
        (GeneratorRoom, Vec3::new(29.5, STOREY - 0.5, -44.0)),
        (Tunnel, Vec3::new(14.0, STOREY - 0.5, -32.0)),
        // West Hall's landing, up the ramp.
        (WestHall, Vec3::new(-20.0, 2.0 * STOREY - 0.5, 0.0)),
        (Balcony, Vec3::new(-12.0, 2.0 * STOREY - 0.5, -2.0)),
        (Yard, Vec3::new(0.0, STOREY + 2.0, 0.0)),
    ]
};

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
const RAISED_FLOORS: [RaisedFloor; 3] = [DAIS, CONTROL_PLATFORM, DOCK];

/// The Infills, one to three in each indoor Zone, so no two rooms share a shape. Each keeps clear
/// of the Doors, windows, Buyables and Spawn Points, and of every path-node edge.
const INFILLS: [Infill; 17] = {
    use ZoneId::*;
    [
        // A chimney breast in the middle of the Start Room's north wall, between its windows.
        infill(StartRoom, (-1.0, 25.2), (1.0, 26.0)),
        // A stair core boxed off in West Hall's north-east corner, both storeys high.
        infill(WestHall, (-16.4, 23.6), (-14.0, 26.0)),
        // East Hall: a pier between the two southern windows, and a vestry cupboard in the north-east
        // corner behind the last column.
        infill(EastHall, (25.2, -3.0), (26.0, -1.0)),
        infill(EastHall, (24.0, 23.5), (26.0, 26.0)),
        // Two nibs out from the Dining Room's north wall, making a servery alcove round the Box.
        infill(DiningRoom, (-16.6, -15.0), (-16.0, -14.0)),
        infill(DiningRoom, (-9.4, -15.0), (-8.8, -14.0)),
        // A switch cupboard boxed off in the Power Room's north-east corner, over the platform.
        infill(PowerRoom, (24.5, -15.8), (26.0, -14.0)),
        // The Passage's dog-leg: a nib out from the west wall at its north end, and one from the
        // east wall at its south end, with the rubble between.
        infill(Passage, (-13.5, -29.3), (-12.7, -27.9)),
        infill(Passage, (-11.3, -38.0), (-10.5, -36.6)),
        // A buttress at the Tunnel's south end, on the west side.
        infill(Tunnel, (12.5, -38.0), (13.2, -36.8)),
        // The Workshop: a boxed-off chimney corner in the south-west, and a pier between the two
        // south windows.
        infill(Workshop, (-34.0, -50.0), (-32.4, -48.4)),
        infill(Workshop, (-22.5, -50.0), (-21.5, -49.3)),
        // Two flue ducts in the Storeroom's north corners, either side of the Box.
        infill(Storeroom, (-4.0, -38.8), (-3.2, -38.0)),
        infill(Storeroom, (3.2, -38.8), (4.0, -38.0)),
        // The Generator Room: a stair core in the north-west corner, a corner cupboard in the
        // north-east, and a pier between the two south windows.
        infill(GeneratorRoom, (4.0, -39.6), (6.2, -38.0)),
        infill(GeneratorRoom, (32.6, -40.0), (34.0, -38.0)),
        infill(GeneratorRoom, (21.5, -50.0), (22.5, -49.3)),
    ]
};

/// The steps and ramps.
const SLOPES: [Slope; 8] = [
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
const SUNKEN: [Sunken; 2] = [GARDEN, DIP];

/// The Props. Training loops stay clear: clutter keeps to the walls and corners, and a room's
/// middle holds at most one island to run laps round.
const PROPS: &[Prop] = {
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

/// The walls, the Doors, the ground outside, the Balcony, its roof and the Gazebo.
fn build(b: &mut Builder) {
    use ZoneId::*;
    let floor = |zone: ZoneId| palette(Some(zone)).floor;

    // The Balcony's floor and the ramp's landing, up at the first storey.
    let balcony = materials(Some(Balcony)).map(|set| set.floor);
    b.block_in(Vec3::new(4.0, 0.2, 24.0), Vec3::new(-12.0, STOREY - 0.1, -2.0), floor(Balcony), balcony);
    b.block(Vec3::new(3.0, 0.2, 4.0), Vec3::new(-15.7, STOREY - 0.1, -2.0), floor(WestHall));

    // The outside: asphalt underfoot, plain concrete on the outer walls' faces, a tar roof with
    // precast parapets.

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
        b.ground(size, centre);
    }

    // Outer walls, with the windows. West Hall's are two storeys tall, since its ramp climbs to
    // the first storey.
    let tall = 2.0 * STOREY;
    b.outer_wall(v(-26.0, 26.0), v(-14.0, 26.0), 0.0, tall);
    b.outer_wall(v(-14.0, 26.0), v(26.0, 26.0), 0.0, STOREY);
    b.outer_wall(v(-26.0, -14.0), v(-26.0, 26.0), 0.0, tall);
    b.outer_wall(v(-26.0, -26.0), v(-26.0, -14.0), 0.0, STOREY);
    // The south wall, with the Doors down to the Passage and the Tunnel.
    b.outer_wall(v(-26.0, -26.0), v(-13.5, -26.0), 0.0, STOREY);
    b.wall(v(-13.5, -26.0), v(-10.5, -26.0), 0.0, STOREY, &[(1.5, door(DiningRoom, Passage, DOOR_PRICE_DINING_PASSAGE))]);
    b.outer_wall(v(-10.5, -26.0), v(12.5, -26.0), 0.0, STOREY);
    b.wall(v(12.5, -26.0), v(15.5, -26.0), 0.0, STOREY, &[(1.5, door(PowerRoom, Tunnel, DOOR_PRICE_POWER_TUNNEL))]);
    b.outer_wall(v(15.5, -26.0), v(26.0, -26.0), 0.0, STOREY);
    b.outer_wall(v(26.0, -26.0), v(26.0, 26.0), 0.0, STOREY);

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
    b.outer_wall(v(-34.0, -50.0), v(34.0, -50.0), 0.0, STOREY);
    b.outer_wall(v(-34.0, -50.0), v(-34.0, -38.0), 0.0, STOREY);
    b.outer_wall(v(34.0, -50.0), v(34.0, -38.0), 0.0, STOREY);
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

    // Balcony: a full wall at the south end (so no one walks onto the Dining wall) with its window,
    // and rails along the Yard side and the north end, with a gap in the Yard rail for the drop.
    b.wall(v(-14.0, -14.0), v(-10.0, -14.0), STOREY, STOREY, &[(2.0, Gap::Window)]);
    b.wall(v(-10.0, -14.0), v(-10.0, 10.0), STOREY, RAIL_HEIGHT, &[(18.0, Gap::Open { width: DOOR_WIDTH, height: RAIL_HEIGHT })]);
    b.wall(v(-14.0, 10.0), v(-10.0, 10.0), STOREY, RAIL_HEIGHT, &[]);
    for z in [-11.0, -2.0, 7.0] {
        b.block(Vec3::new(0.5, STOREY, 0.5), Vec3::new(-10.3, STOREY / 2.0, z), palette(Some(Yard)).trim);
    }
    // Outside the Balcony's window, a flat roof over the Dining Room's corner where Zombies
    // gather, as thick as the Balcony's floor and level with it, with a parapet round its open
    // edges so none of them falls into the room below.
    let (roof_min, roof_max) = BALCONY_ROOF;
    let roof = roof_max - roof_min;
    let roof_mid = (roof_min + roof_max) / 2.0;
    b.block_in(Vec3::new(roof.x, 0.2, roof.y), Vec3::new(roof_mid.x, STOREY - 0.1, roof_mid.y), palette(None).floor, Some(Material::DamagedConcreteFloor03));
    let parapet = Some(Material::ConcreteSlabWall02);
    let thick = PARAPET_THICKNESS;
    let rise = Vec3::Y * (STOREY + RAIL_HEIGHT / 2.0);
    for x in [roof_min.x + thick / 2.0, roof_max.x - thick / 2.0] {
        b.block_in(Vec3::new(thick, RAIL_HEIGHT, roof.y), Vec3::new(x, 0.0, roof_mid.y) + rise, palette(None).trim, parapet);
    }
    b.block_in(Vec3::new(roof.x, RAIL_HEIGHT, thick), Vec3::new(roof_mid.x, 0.0, roof_min.y + thick / 2.0) + rise, palette(None).trim, parapet);

    // The Pack-a-Punch Gazebo, behind a Door that stays shut until both Teleporter Pads are Linked.
    let gazebo_height = DOOR_HEIGHT;
    b.repaint = Some(GAZEBO);
    b.repaint_wall = Some(Material::RockWall08);
    b.wall(v(2.0, -10.0), v(8.0, -10.0), 0.0, gazebo_height, &[]);
    b.wall(v(2.0, -10.0), v(2.0, -4.0), 0.0, gazebo_height, &[]);
    b.wall(v(8.0, -10.0), v(8.0, -4.0), 0.0, gazebo_height, &[]);
    b.wall(v(2.0, -4.0), v(8.0, -4.0), 0.0, gazebo_height, &[(3.0, Gap::LinkedDoor(Some((Yard, Yard))))]);
    b.repaint = None;
    b.repaint_wall = None;
    b.block_in(Vec3::new(6.8, 0.2, 6.8), Vec3::new(5.0, gazebo_height + 0.1, -7.0), GAZEBO.trim, Some(Material::RockWall08));
}

/// The path nodes: a name, then where a Zombie's feet stand there (x, y, z).
const NODES: &[(&str, f32, f32, f32)] = &[
    // Start Room
    ("start", 0.0, 0.0, 20.0),
    ("start_w", -12.5, 0.0, 20.0),
    ("start_e", 12.5, 0.0, 20.0),
    ("start_s", 0.0, 0.0, 15.5),
    ("w0_in", -7.0, 0.0, 24.5),
    ("w0_out", -7.0, 0.0, 28.0),
    ("w1_in", 7.0, 0.0, 24.5),
    ("w1_out", 7.0, 0.0, 28.0),
    // West Hall, its ramp and the Balcony
    ("west_e", -15.5, 0.0, 20.0),
    ("west_ne", -20.0, 0.0, 20.0),
    ("west_mid", -21.0, 0.0, 4.0),
    ("west_s", -20.0, 0.0, -6.0),
    ("west_dd", -20.0, 0.0, -12.5),
    ("ramp_bottom", -15.7, 0.0, 10.0),
    ("ramp_mid", -15.7, 2.0, 4.0),
    ("ramp_top", -15.7, 4.0, -2.0),
    ("w2_in", -24.5, 0.0, 18.0),
    ("w2_out", -28.0, 0.0, 18.0),
    ("w3_in", -24.5, 0.0, 4.0),
    ("w3_out", -28.0, 0.0, 4.0),
    ("w4_in", -24.5, 0.0, -8.0),
    ("w4_out", -28.0, 0.0, -8.0),
    ("bal_w", -12.5, 4.0, -2.0),
    ("w20_in", -12.0, 4.0, -11.5),
    ("w20_out", -12.0, 4.0, -16.0),
    ("drop_top", -11.0, 4.0, 4.0),
    // Yard
    ("drop_bottom", -8.5, 0.0, 4.0),
    ("yard_nw", -9.5, 0.0, 12.5),
    ("yard_n", 0.0, 0.0, 12.5),
    // Down in the sunken garden, and on the Yard floor at the top of each flight of its steps
    ("garden", -4.5, -0.6, 8.5),
    ("garden_e", 0.6, 0.0, 8.0),
    ("garden_s", -4.0, 0.0, 3.4),
    ("yard_c", 0.0, 0.0, 0.0),
    ("yard_w", -8.0, 0.0, 0.0),
    ("yard_e", 11.0, 0.0, -2.0),
    ("yard_sd", -7.0, 0.0, -12.5),
    // Round the Gazebo's back, between it and the sandbags and the Yard's south wall
    ("yard_sw", -1.0, 0.0, -9.5),
    ("yard_s", 5.0, 0.0, -12.0),
    ("yard_se", 11.0, 0.0, -11.0),
    // Outside the Gazebo's Door and inside it, before Pack-a-Punch
    ("gazebo_door", 5.0, 0.0, -2.6),
    ("gazebo", 5.0, 0.0, -6.6),
    // East Hall
    ("east_w", 15.5, 0.0, 20.0),
    ("east_ne", 20.0, 0.0, 20.0),
    ("east_mid", 21.0, 0.0, 4.0),
    ("east_s", 20.0, 0.0, -6.0),
    ("east_pd", 20.0, 0.0, -12.5),
    ("w5_in", 24.5, 0.0, 18.0),
    ("w5_out", 28.0, 0.0, 18.0),
    ("w6_in", 24.5, 0.0, 4.0),
    ("w6_out", 28.0, 0.0, 4.0),
    ("w7_in", 24.5, 0.0, -8.0),
    ("w7_out", 28.0, 0.0, -8.0),
    // At the foot of the altar dais's steps, and up on the dais
    ("east_steps", 20.4, 0.0, 11.0),
    ("east_dais", 23.6, 0.6, 11.0),
    // Dining Room
    ("dining_c", -13.0, 0.0, -20.0),
    ("dining_n", -13.0, 0.0, -15.9),
    ("dining_wn", -20.0, 0.0, -15.5),
    ("dining_yn", -7.0, 0.0, -15.5),
    ("dining_w", -22.0, 0.0, -20.0),
    ("dining_e", -4.0, 0.0, -20.0),
    ("dining_pe", -1.5, 0.0, -20.0),
    ("dining_ps", -12.0, 0.0, -24.5),
    ("w8_in", -18.0, 0.0, -24.5),
    ("w8_out", -18.0, 0.0, -28.0),
    ("w9_in", -6.0, 0.0, -24.5),
    ("w9_out", -6.0, 0.0, -28.0),
    ("w10_in", -24.5, 0.0, -20.0),
    ("w10_out", -28.0, 0.0, -20.0),
    // Power Room
    ("power_c", 13.0, 0.0, -20.0),
    ("power_w", 1.5, 0.0, -20.0),
    ("power_n", 20.0, 0.0, -15.5),
    ("power_s", 14.0, 0.0, -24.5),
    ("w11_in", 8.0, 0.0, -24.5),
    ("w11_out", 8.0, 0.0, -28.0),
    ("w12_in", 20.0, 0.0, -24.5),
    ("w12_out", 20.0, 0.0, -28.0),
    // At the foot of the control platform's steps, and up on it by the Power Switch
    ("power_e", 20.3, 0.0, -20.0),
    ("power_dais", 23.4, 0.45, -20.0),
    // The Passage and the Tunnel, across the open ground to the south wing
    ("pass_n", -12.0, 0.0, -27.5),
    ("pass_m", -11.6, 0.0, -30.6),
    ("pass_m2", -12.4, 0.0, -34.3),
    ("pass_s", -12.0, 0.0, -36.5),
    ("tun_n", 14.0, 0.0, -27.5),
    ("tun_dip", 14.0, -0.5, -32.0),
    ("tun_s", 14.0, 0.0, -36.5),
    // Workshop
    ("work_n", -12.0, 0.0, -39.5),
    ("work_cn", -19.0, 0.0, -42.3),
    ("work_cs", -21.0, 0.0, -46.8),
    ("work_w", -30.0, 0.0, -44.0),
    ("work_e", -5.5, 0.0, -44.0),
    ("w13_in", -28.0, 0.0, -48.5),
    ("w13_out", -28.0, 0.0, -52.0),
    ("w14_in", -16.0, 0.0, -48.5),
    ("w14_out", -16.0, 0.0, -52.0),
    ("w15_in", -32.5, 0.0, -44.0),
    ("w15_out", -36.0, 0.0, -44.0),
    // At the foot of the loading dock's ramp, and along the dock
    ("dock_foot", -32.4, 0.0, -39.4),
    ("dock_w", -27.0, 1.1, -39.4),
    ("dock_e", -17.0, 1.1, -39.4),
    // Storeroom
    ("store_w", -2.5, 0.0, -44.0),
    ("store_c", 0.0, 0.0, -44.0),
    ("store_e", 2.5, 0.0, -44.0),
    ("w16_in", 0.0, 0.0, -48.5),
    ("w16_out", 0.0, 0.0, -52.0),
    // Generator Room
    ("gen_w", 5.5, 0.0, -44.0),
    ("gen_cn", 19.5, 0.0, -41.3),
    ("gen_cs", 19.5, 0.0, -46.8),
    ("gen_n", 14.0, 0.0, -39.5),
    ("gen_e", 30.0, 0.0, -44.0),
    ("w17_in", 16.0, 0.0, -48.5),
    ("w17_out", 16.0, 0.0, -52.0),
    ("w18_in", 28.0, 0.0, -48.5),
    ("w18_out", 28.0, 0.0, -52.0),
    ("w19_in", 32.5, 0.0, -44.0),
    ("w19_out", 36.0, 0.0, -44.0),
    // Vents, up near the top of their rooms, over the spot a Nova Crawler drops to
    ("vent_west", -21.0, NOVA_VENT_HEIGHT, 4.0),
    ("vent_east", 21.0, NOVA_VENT_HEIGHT, 4.0),
    ("vent_dining", -13.0, NOVA_VENT_HEIGHT, -20.0),
    ("vent_power", 13.0, NOVA_VENT_HEIGHT, -20.0),
];

const OPEN: Gate = Gate::Open;

/// The edges, by node name. Barrier and one-way edges run from the first node to the second.
/// Every edge is a straight walk clear of walls and Props, and a two-way edge never climbs a
/// Ledge: Raised Floors are reached by their steps and ramps.
const EDGES: &[(&str, &str, Gate)] = {
    use ZoneId::*;
    &[
        // Start Room
        ("start", "start_w", OPEN),
        ("start", "start_e", OPEN),
        ("start", "start_s", OPEN),
        ("start", "w0_in", OPEN),
        ("start", "w1_in", OPEN),
        ("w0_out", "w0_in", Gate::Barrier(BarrierId(0))),
        ("w1_out", "w1_in", Gate::Barrier(BarrierId(1))),
        ("start_w", "west_e", Gate::Door(StartRoom, WestHall)),
        ("start_e", "east_w", Gate::Door(StartRoom, EastHall)),
        ("start_s", "yard_n", Gate::Door(StartRoom, Yard)),
        // West Hall
        ("west_e", "west_ne", OPEN),
        ("west_e", "ramp_bottom", OPEN),
        ("west_ne", "ramp_bottom", OPEN),
        ("west_ne", "west_mid", OPEN),
        ("west_mid", "west_s", OPEN),
        ("west_s", "west_dd", OPEN),
        ("west_ne", "w2_in", OPEN),
        ("west_mid", "w3_in", OPEN),
        ("west_s", "w4_in", OPEN),
        ("w2_out", "w2_in", Gate::Barrier(BarrierId(2))),
        ("w3_out", "w3_in", Gate::Barrier(BarrierId(3))),
        ("w4_out", "w4_in", Gate::Barrier(BarrierId(4))),
        ("west_dd", "dining_wn", Gate::Door(WestHall, DiningRoom)),
        // Up the ramp, along the Balcony and down the drop
        ("ramp_bottom", "ramp_mid", OPEN),
        ("ramp_mid", "ramp_top", OPEN),
        ("ramp_top", "bal_w", OPEN),
        ("bal_w", "drop_top", OPEN),
        ("bal_w", "w20_in", OPEN),
        ("w20_out", "w20_in", Gate::Barrier(BarrierId(20))),
        ("drop_top", "drop_bottom", Gate::OneWay),
        // Yard
        ("drop_bottom", "yard_w", OPEN),
        ("drop_bottom", "yard_nw", OPEN),
        ("yard_nw", "yard_n", OPEN),
        ("garden", "garden_e", OPEN),
        ("garden", "garden_s", OPEN),
        ("garden_e", "yard_n", OPEN),
        ("garden_e", "yard_c", OPEN),
        ("garden_s", "yard_c", OPEN),
        ("garden_s", "yard_w", OPEN),
        ("yard_w", "yard_c", OPEN),
        ("yard_c", "yard_n", OPEN),
        ("yard_c", "yard_e", OPEN),
        ("yard_w", "yard_sd", OPEN),
        ("yard_sd", "dining_yn", Gate::Door(Yard, DiningRoom)),
        ("yard_c", "yard_sw", OPEN),
        ("yard_sw", "yard_sd", OPEN),
        ("yard_sw", "yard_s", OPEN),
        ("yard_s", "yard_se", OPEN),
        ("yard_se", "yard_e", OPEN),
        // Into the Gazebo, once the Power has opened its Door.
        ("gazebo_door", "gazebo", Gate::Door(Yard, Yard)),
        ("gazebo_door", "yard_c", OPEN),
        ("gazebo_door", "yard_e", OPEN),
        // East Hall
        ("east_w", "east_ne", OPEN),
        ("east_ne", "east_mid", OPEN),
        ("east_mid", "east_s", OPEN),
        ("east_s", "east_pd", OPEN),
        ("east_ne", "w5_in", OPEN),
        ("east_mid", "w6_in", OPEN),
        ("east_s", "w7_in", OPEN),
        ("w5_out", "w5_in", Gate::Barrier(BarrierId(5))),
        ("w6_out", "w6_in", Gate::Barrier(BarrierId(6))),
        ("w7_out", "w7_in", Gate::Barrier(BarrierId(7))),
        ("east_pd", "power_n", Gate::Door(EastHall, PowerRoom)),
        ("east_ne", "east_steps", OPEN),
        ("east_mid", "east_steps", OPEN),
        ("east_steps", "east_dais", OPEN),
        // Dining Room
        // Round the tables: the aisle down the middle, and the lanes round the edges.
        ("dining_c", "dining_n", OPEN),
        ("dining_c", "dining_w", OPEN),
        ("dining_c", "dining_e", OPEN),
        ("dining_n", "dining_wn", OPEN),
        ("dining_n", "dining_yn", OPEN),
        ("dining_w", "dining_wn", OPEN),
        ("dining_w", "w10_in", OPEN),
        ("dining_e", "dining_yn", OPEN),
        ("dining_e", "dining_pe", OPEN),
        ("dining_ps", "w8_in", OPEN),
        ("dining_ps", "w9_in", OPEN),
        ("w8_out", "w8_in", Gate::Barrier(BarrierId(8))),
        ("w9_out", "w9_in", Gate::Barrier(BarrierId(9))),
        ("w10_out", "w10_in", Gate::Barrier(BarrierId(10))),
        ("dining_pe", "power_w", Gate::Door(DiningRoom, PowerRoom)),
        ("dining_c", "dining_ps", OPEN),
        ("dining_ps", "pass_n", Gate::Door(DiningRoom, Passage)),
        // Power Room
        ("power_c", "power_w", OPEN),
        ("power_c", "power_n", OPEN),
        ("power_c", "w11_in", OPEN),
        ("power_c", "w12_in", OPEN),
        ("w11_out", "w11_in", Gate::Barrier(BarrierId(11))),
        ("w12_out", "w12_in", Gate::Barrier(BarrierId(12))),
        ("power_c", "power_s", OPEN),
        ("power_s", "tun_n", Gate::Door(PowerRoom, Tunnel)),
        ("power_c", "power_e", OPEN),
        ("power_n", "power_e", OPEN),
        ("power_e", "power_dais", OPEN),
        // The Passage and the Tunnel, open into the wing at their south ends
        ("pass_n", "pass_m", OPEN),
        ("pass_m", "pass_m2", OPEN),
        ("pass_m2", "pass_s", OPEN),
        ("pass_s", "work_n", OPEN),
        ("tun_n", "tun_dip", OPEN),
        ("tun_dip", "tun_s", OPEN),
        ("tun_s", "gen_n", OPEN),
        // Workshop
        // Round the workbench island.
        ("work_n", "work_cn", OPEN),
        ("work_cn", "work_w", OPEN),
        ("work_cn", "work_e", OPEN),
        ("work_cs", "work_w", OPEN),
        ("work_cs", "work_e", OPEN),
        ("work_cs", "w13_in", OPEN),
        ("work_cs", "w14_in", OPEN),
        ("work_w", "w15_in", OPEN),
        ("w13_out", "w13_in", Gate::Barrier(BarrierId(13))),
        ("w14_out", "w14_in", Gate::Barrier(BarrierId(14))),
        ("w15_out", "w15_in", Gate::Barrier(BarrierId(15))),
        ("work_e", "store_w", Gate::Door(Workshop, Storeroom)),
        ("work_w", "dock_foot", OPEN),
        ("dock_foot", "dock_w", OPEN),
        ("dock_w", "dock_e", OPEN),
        // Off the dock's edge.
        ("dock_e", "work_cn", Gate::OneWay),
        // Storeroom
        ("store_w", "store_c", OPEN),
        ("store_c", "store_e", OPEN),
        ("store_c", "w16_in", OPEN),
        ("w16_out", "w16_in", Gate::Barrier(BarrierId(16))),
        ("store_e", "gen_w", Gate::Door(Storeroom, GeneratorRoom)),
        // Generator Room
        // Round the generator.
        ("gen_n", "gen_cn", OPEN),
        ("gen_cn", "gen_w", OPEN),
        ("gen_cn", "gen_e", OPEN),
        ("gen_cs", "gen_w", OPEN),
        ("gen_cs", "gen_e", OPEN),
        ("gen_cs", "w17_in", OPEN),
        ("gen_cs", "w18_in", OPEN),
        ("gen_e", "w19_in", OPEN),
        ("w17_out", "w17_in", Gate::Barrier(BarrierId(17))),
        ("w18_out", "w18_in", Gate::Barrier(BarrierId(18))),
        ("w19_out", "w19_in", Gate::Barrier(BarrierId(19))),
        // The Vent drops
        ("vent_west", "west_mid", Gate::OneWay),
        ("vent_east", "east_mid", Gate::OneWay),
        ("vent_dining", "dining_c", Gate::OneWay),
        ("vent_power", "power_c", Gate::OneWay),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    /// The Balcony's window, the one onto the roof.
    const BALCONY_WINDOW: BarrierId = BarrierId(20);

    /// The Yard stays trainable: loops to lead a Train round the Gazebo, round the sunken garden
    /// and round the whole Yard, each a level lane at least 1.6 m wide with nothing in it.
    #[test]
    fn the_yard_keeps_clear_loops_to_train_round() {
        const LOOPS: &[(&str, &[Vec2])] = &[
            ("the Gazebo", &[Vec2::new(0.0, -2.0), Vec2::new(10.5, -2.0), Vec2::new(10.5, -11.8), Vec2::new(0.4, -11.0)]),
            ("the garden", &[Vec2::new(-9.0, 2.8), Vec2::new(0.9, 2.8), Vec2::new(0.9, 12.6), Vec2::new(-9.0, 12.6)]),
            ("the Yard", &[Vec2::new(-9.0, 12.6), Vec2::new(11.0, 12.6), Vec2::new(11.0, -11.8), Vec2::new(1.0, -11.0), Vec2::new(-9.0, -11.0)]),
        ];
        let spoilt = crate::maps::map::tests::spoilt_loops(MapId::Greybox, 0.0, LOOPS);
        assert!(spoilt.is_empty(), "{spoilt:#?}");
    }

    /// The roof outside the Balcony's window is walled in all round, so a Zombie knocked about
    /// out there never drops into the Dining Room or the Yard: every way out from its Spawn Point,
    /// at knee height, meets a wall or the parapet before the roof's edge.
    #[test]
    fn the_roof_outside_the_balcony_window_is_walled_in() {
        assert_eq!(WINDOWS[BALCONY_WINDOW.0 as usize].0, ZoneId::Balcony);
        let open = crate::maps::map::tests::ways_off_outside(MapId::Greybox, BALCONY_WINDOW);
        assert!(open.is_empty(), "ways off the roof at these headings (degrees): {open:?}");
    }

    #[test]
    fn special_spawn_points_stand_one_or_two_in_each_busy_zone() {
        use ZoneId::*;
        for zone in [StartRoom, WestHall, EastHall, DiningRoom, PowerRoom, Yard, Workshop, GeneratorRoom] {
            let count = SPECIAL_SPAWNS.iter().filter(|(z, _)| *z == zone).count();
            assert!((1..=2).contains(&count), "{} has {count} Special Spawn Points", zone.name());
        }
        assert!(!SPECIAL_SPAWNS.iter().any(|(z, _)| *z == Balcony));
    }

    #[test]
    fn vents_open_in_four_indoor_zones() {
        use ZoneId::*;
        let zones: Vec<_> = VENTS.iter().map(|(zone, _)| *zone).collect();
        assert_eq!(zones, [WestHall, EastHall, DiningRoom, PowerRoom]);
    }

    #[test]
    fn the_teleporter_pads_are_far_from_each_other_and_the_mainframe() {
        use crate::maps::teleporter::{MAINFRAME_AT, PadId};
        assert!(crate::maps::teleporter::stands_on(MapId::Greybox));
        let [a, b] = PadId::ALL.map(PadId::at);
        assert!(a.distance(b) > 50.0, "the Pads are {:.1} m apart", a.distance(b));
        for pad in [a, b] {
            assert!(pad.distance(MAINFRAME_AT) > 60.0);
        }
    }
}
