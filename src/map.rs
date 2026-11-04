//! The greybox Map: a ring of five indoor Zones around an open Yard, plus a Balcony over the Yard
//! reached by a ramp in West Hall, with a one-way drop back down. South of the ring, across open
//! ground, lies the south wing: the Workshop, the Storeroom and the Generator Room, reached down
//! the Passage from the Dining Room and the Tunnel from the Power Room. The Teleporter's Pads sit
//! at the wing's far corners.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::arsenal::WeaponId;
use crate::buy::Buyable;
use crate::door::Door;
use crate::layers::GameLayer;
use crate::state::{GameState, InRun};
use crate::tuning::*;
use crate::wall_weapon::{WallWeapon, wall_price};
use crate::zone::ZoneId;

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
const WALL_THICKNESS: f32 = 0.4;
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
const WINDOW_HEIGHT: f32 = 2.2;
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

/// The Yard's Risers, where Zombies climb out of the ground (x, z).
const RISERS: [Vec2; 4] = [
    Vec2::new(-7.0, 10.0),
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
    (ZoneId::Passage, Vec3::new(-12.0, 0.0, -32.0)),
    (ZoneId::Workshop, Vec3::new(-18.0, 0.0, -44.0)),
    (ZoneId::Workshop, Vec3::new(-8.0, 0.0, -46.0)),
    (ZoneId::Storeroom, Vec3::new(0.0, 0.0, -45.0)),
    (ZoneId::GeneratorRoom, Vec3::new(18.0, 0.0, -44.0)),
    (ZoneId::GeneratorRoom, Vec3::new(8.0, 0.0, -46.0)),
    (ZoneId::Tunnel, Vec3::new(14.0, 0.0, -32.0)),
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
    (ZoneId::Workshop, Vec3::new(-18.0, 0.0, -44.0)),
    (ZoneId::GeneratorRoom, Vec3::new(18.0, 0.0, -44.0)),
];

/// The Vents Nova Crawlers drop from: the Zone each is in, and the spot on the floor below it.
/// Each opens `NOVA_VENT_HEIGHT` up, at the top of a one-way path-node edge down to that spot.
pub const VENTS: [(ZoneId, Vec3); 4] = [
    (ZoneId::WestHall, Vec3::new(-21.0, 0.0, 4.0)),
    (ZoneId::EastHall, Vec3::new(21.0, 0.0, 4.0)),
    (ZoneId::DiningRoom, Vec3::new(-13.0, 0.0, -20.0)),
    (ZoneId::PowerRoom, Vec3::new(13.0, 0.0, -20.0)),
];

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

/// A gap in a wall, at an offset along it.
enum Gap {
    /// A Door, joining two Zones; `None` price makes it a Power Door.
    Door(Option<(ZoneId, ZoneId)>, Option<u64>),
    /// An open archway.
    Open { width: f32, height: f32 },
    /// A window in an outer wall, open to Zombies and solid to the Player.
    Window,
}

struct Builder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    wall: Handle<StandardMaterial>,
    door: Handle<StandardMaterial>,
    power_door: Handle<StandardMaterial>,
}

impl Builder<'_, '_, '_> {
    fn solid(&mut self, size: Vec3, at: Vec3, mat: &Handle<StandardMaterial>) -> Entity {
        let shape = Cuboid::from_size(size);
        self.commands
            .spawn((
                InRun,
                Mesh3d(self.meshes.add(shape)),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(at),
                RigidBody::Static,
                Collider::from(shape),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            .id()
    }

    /// An axis-aligned wall from `a` to `b` (x, z), standing on `y0`, `height` tall, with gaps
    /// centred at offsets from `a`.
    fn wall(&mut self, a: Vec2, b: Vec2, y0: f32, height: f32, gaps: &[(f32, Gap)]) {
        let along_x = a.y == b.y;
        let length = (b - a).length();
        let dir = (b - a) / length;
        let wall_mat = self.wall.clone();
        // A box spanning [s, e] along the wall and [lo, hi] up from `y0`.
        let piece = |this: &mut Self, s: f32, e: f32, lo: f32, hi: f32, thick: f32, mat: &Handle<StandardMaterial>| {
            if e - s < 0.01 || hi - lo < 0.01 {
                return None;
            }
            let mid = a + dir * (s + e) / 2.0;
            let size = if along_x {
                Vec3::new(e - s, hi - lo, thick)
            } else {
                Vec3::new(thick, hi - lo, e - s)
            };
            Some(this.solid(size, Vec3::new(mid.x, y0 + (lo + hi) / 2.0, mid.y), mat))
        };

        let mut cursor = 0.0;
        for (at, gap) in gaps {
            let (width, top) = match gap {
                Gap::Door(..) => (DOOR_WIDTH, DOOR_HEIGHT),
                Gap::Open { width, height } => (*width, *height),
                Gap::Window => (WINDOW_WIDTH, WINDOW_HEIGHT),
            };
            let (s, e) = (at - width / 2.0, at + width / 2.0);
            piece(self, cursor, s, 0.0, height, WALL_THICKNESS, &wall_mat);
            piece(self, s, e, top, height, WALL_THICKNESS, &wall_mat);
            cursor = e;
            if let Gap::Window = gap {
                let mid = a + dir * *at;
                let size = if along_x {
                    Vec3::new(e - s, WINDOW_HEIGHT, WALL_THICKNESS)
                } else {
                    Vec3::new(WALL_THICKNESS, WINDOW_HEIGHT, e - s)
                };
                self.commands.spawn((
                    InRun,
                    Transform::from_xyz(mid.x, y0 + WINDOW_HEIGHT / 2.0, mid.y),
                    RigidBody::Static,
                    Collider::cuboid(size.x, size.y, size.z),
                    CollisionLayers::new(GameLayer::Window, LayerMask::ALL),
                ));
            }
            if let Gap::Door(joins, price) = gap {
                let mat = if price.is_some() { self.door.clone() } else { self.power_door.clone() };
                let door = piece(self, s, e, 0.0, DOOR_HEIGHT, 0.25, &mat).unwrap();
                let mid = a + dir * *at;
                self.commands.entity(door).insert((
                    Door {
                        joins: *joins,
                        price: *price,
                        open: false,
                    },
                    Visibility::default(),
                ));
                // A Power Door can't be bought, so it has no Prompt.
                if price.is_some() {
                    self.commands.entity(door).insert(Buyable {
                        use_point: Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y),
                        prompt: String::new(),
                        price: *price,
                        range: DOOR_USE_RANGE,
                    });
                }
            }
        }
        piece(self, cursor, length, 0.0, height, WALL_THICKNESS, &wall_mat);
    }

    /// An outer wall from `a` to `b`, with a gap for each window on it.
    fn outer_wall(&mut self, a: Vec2, b: Vec2, height: f32) {
        let length = (b - a).length();
        let mut gaps: Vec<_> = WINDOWS
            .iter()
            .map(|(_, centre, _)| *centre)
            .filter(|&centre| (centre - a).perp_dot(b - a).abs() < 0.01)
            .map(|centre| ((centre - a).dot(b - a) / length, Gap::Window))
            .filter(|(at, _)| *at > 0.0 && *at < length)
            .collect();
        gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
        self.wall(a, b, 0.0, height, &gaps);
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

    let floor_mat = materials.add(Color::srgb(0.22, 0.2, 0.18));
    let yard_mat = materials.add(Color::srgb(0.18, 0.2, 0.15));
    let crate_mat = materials.add(Color::srgb(0.4, 0.28, 0.15));
    let ramp_mat = materials.add(Color::srgb(0.3, 0.25, 0.2));
    let mut b = Builder {
        commands: &mut commands,
        meshes: &mut meshes,
        wall: materials.add(Color::srgb(0.35, 0.3, 0.27)),
        door: materials.add(Color::srgb(0.85, 0.45, 0.1)),
        power_door: materials.add(Color::srgb(0.5, 0.3, 0.8)),
    };

    // Floors: one per Zone, then the Balcony and the ramp's landing up at the first storey.
    for zone in [StartRoom, WestHall, EastHall, DiningRoom, PowerRoom, Yard, Passage, Workshop, Storeroom, GeneratorRoom, Tunnel] {
        let (lo, hi) = zone.volume();
        let size = Vec3::new(hi.x - lo.x, 0.2, hi.z - lo.z);
        let centre = Vec3::new((lo.x + hi.x) / 2.0, -0.1, (lo.z + hi.z) / 2.0);
        let mat = if zone == Yard { &yard_mat } else { &floor_mat };
        b.solid(size, centre, mat);
    }
    b.solid(Vec3::new(4.0, 0.2, 24.0), Vec3::new(-12.0, STOREY - 0.1, -2.0), &floor_mat);
    b.solid(Vec3::new(3.0, 0.2, 4.0), Vec3::new(-15.7, STOREY - 0.1, -2.0), &floor_mat);

    // Ground outside the outer walls, where Zombies gather at the windows: round the ring, across
    // the gap between the ring and the south wing (just under the Passage and Tunnel floors), and
    // round the wing.
    for (size, centre) in [
        (Vec3::new(60.0, 0.2, 4.0), Vec3::new(0.0, -0.1, 28.0)),
        (Vec3::new(4.0, 0.2, 52.0), Vec3::new(-28.0, -0.1, 0.0)),
        (Vec3::new(4.0, 0.2, 52.0), Vec3::new(28.0, -0.1, 0.0)),
        (Vec3::new(76.0, 0.2, 12.0), Vec3::new(0.0, -0.11, -32.0)),
        (Vec3::new(76.0, 0.2, 4.0), Vec3::new(0.0, -0.1, -52.0)),
        (Vec3::new(4.0, 0.2, 12.0), Vec3::new(-36.0, -0.1, -44.0)),
        (Vec3::new(4.0, 0.2, 12.0), Vec3::new(36.0, -0.1, -44.0)),
    ] {
        b.solid(size, centre, &yard_mat);
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
    for x in [-13.5, -10.5, 12.5, 15.5] {
        b.wall(v(x, -38.0), v(x, -26.0), 0.0, STOREY, &[]);
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
    let wall_mat = b.wall.clone();
    for z in [-11.0, -2.0, 7.0] {
        b.solid(Vec3::new(0.5, STOREY, 0.5), Vec3::new(-10.3, STOREY / 2.0, z), &wall_mat);
    }

    // Ramp up West Hall's inner wall, from the floor at z = 8 to the landing at z = 0.
    let run = 8.0;
    let angle = (STOREY / run).atan();
    let ramp = Cuboid::new(3.0, 0.3, (STOREY * STOREY + run * run).sqrt());
    let normal = Vec3::new(0.0, angle.cos(), angle.sin());
    b.commands.spawn((
        InRun,
        Mesh3d(b.meshes.add(ramp)),
        MeshMaterial3d(ramp_mat),
        Transform::from_translation(Vec3::new(-15.7, STOREY / 2.0, run / 2.0) - normal * 0.15)
            .with_rotation(Quat::from_rotation_x(angle)),
        RigidBody::Static,
        Collider::from(ramp),
        CollisionLayers::new(GameLayer::World, LayerMask::ALL),
    ));

    // The Pack-a-Punch Gazebo, behind a Power Door that stays shut until the Power comes on.
    let gazebo_height = DOOR_HEIGHT;
    b.wall(v(2.0, -10.0), v(8.0, -10.0), 0.0, gazebo_height, &[]);
    b.wall(v(2.0, -10.0), v(2.0, -4.0), 0.0, gazebo_height, &[]);
    b.wall(v(8.0, -10.0), v(8.0, -4.0), 0.0, gazebo_height, &[]);
    b.wall(v(2.0, -4.0), v(8.0, -4.0), 0.0, gazebo_height, &[(3.0, Gap::Door(None, None))]);
    b.solid(Vec3::new(6.8, 0.2, 6.8), Vec3::new(5.0, gazebo_height + 0.1, -7.0), &wall_mat);

    // Yard cover, and workbenches and crates in the south wing, against its north wall where
    // nothing walks.
    for (x, z, w, d) in [
        (-5.0, 4.0, 3.0, 1.0),
        (9.0, 3.0, 1.0, 3.0),
        (-3.0, -6.0, 1.2, 1.2),
        (2.0, 8.0, 1.2, 1.2),
        (-22.0, -38.8, 4.0, 1.2),
        (-30.5, -38.9, 1.2, 1.2),
        (22.0, -38.8, 4.0, 1.2),
        (30.5, -38.9, 1.2, 1.2),
    ] {
        b.solid(Vec3::new(w, 1.1, d), Vec3::new(x, 0.55, z), &crate_mat);
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
    let lamps = [
        (0.0, STOREY - 0.5, 20.0),
        (-20.0, STOREY - 0.5, 16.0),
        (-20.0, 2.0 * STOREY - 0.5, 0.0),
        (20.0, STOREY - 0.5, 16.0),
        (20.0, STOREY - 0.5, -2.0),
        (-13.0, STOREY - 0.5, -20.0),
        (13.0, STOREY - 0.5, -20.0),
        (-12.0, 2.0 * STOREY - 0.5, -2.0),
        (0.0, STOREY + 2.0, 0.0),
        (-12.0, STOREY - 0.5, -32.0),
        (14.0, STOREY - 0.5, -32.0),
        (-19.0, STOREY - 0.5, -44.0),
        (0.0, STOREY - 0.5, -44.0),
        (19.0, STOREY - 0.5, -44.0),
    ];
    for (x, y, z) in lamps {
        commands.spawn((
            InRun,
            PointLight {
                color: Color::srgb(1.0, 0.75, 0.45),
                intensity: 400_000.0,
                range: 25.0,
                ..default()
            },
            Transform::from_xyz(x, y, z),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn fixtures() -> Vec<(String, Vec3, f32)> {
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
}
