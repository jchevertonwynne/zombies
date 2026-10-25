//! The greybox Map: a ring of five indoor Zones around an open Yard, plus a Balcony over the Yard
//! reached by a ramp in West Hall, with a one-way drop back down.

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

/// One storey.
const STOREY: f32 = 4.0;
const WALL_THICKNESS: f32 = 0.4;
const DOOR_WIDTH: f32 = 2.4;
const DOOR_HEIGHT: f32 = 3.0;
const RAIL_HEIGHT: f32 = 1.1;

/// Which Map a Run is played on. As a resource, it's the Map of the current (or just-ended) Run.
#[derive(Resource, Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapId {
    /// The greybox Map: five rooms ringing the Yard, plus the Balcony. The only Map so far.
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
pub const BOX_LOCATIONS: [(ZoneId, Vec2, Vec2); 4] = [
    // Against the back wall, behind the Player at the start.
    (ZoneId::StartRoom, Vec2::new(0.0, 25.35), Vec2::NEG_Y),
    // On the wall to the Yard, south of the Trench Gun.
    (ZoneId::EastHall, Vec2::new(14.65, -6.0), Vec2::X),
    // On the north wall, between the West Hall and Yard Doors.
    (ZoneId::DiningRoom, Vec2::new(-13.5, -14.65), Vec2::NEG_Y),
    // Against the East Hall wall.
    (ZoneId::Yard, Vec2::new(13.35, -2.0), Vec2::NEG_X),
];

const WINDOW_WIDTH: f32 = 1.6;
const WINDOW_HEIGHT: f32 = 2.2;
/// How far outside its window a window Spawn Point sits.
const WINDOW_SPAWN_OUT: f32 = 2.0;

/// One of the 13 windows in the ring's outer walls, each the home of one Barrier (an index into
/// `WINDOWS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarrierId(pub u8);

/// The windows: the Zone each lets into, its centre on the wall (x, z), and the way out. Each is
/// planked up by its Barrier.
pub const WINDOWS: [(ZoneId, Vec2, Vec2); 13] = [
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
const HELLHOUND_SPAWNS: [(ZoneId, Vec3); 21] = [
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
    (ZoneId::EastHall, Vec3::new(20.0, 0.0, 4.0)),
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
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnKind {
    /// Outside a window. The Zombie has to get in through that window's Barrier.
    Window(BarrierId),
    /// In open ground: the Zombie climbs up out of the dirt.
    Riser,
    /// Inside a Zone, where a Hellhound arrives in a lightning strike. Only hounds use these.
    Hellhound,
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
    for zone in [StartRoom, WestHall, EastHall, DiningRoom, PowerRoom, Yard] {
        let (lo, hi) = zone.volume();
        let size = Vec3::new(hi.x - lo.x, 0.2, hi.z - lo.z);
        let centre = Vec3::new((lo.x + hi.x) / 2.0, -0.1, (lo.z + hi.z) / 2.0);
        let mat = if zone == Yard { &yard_mat } else { &floor_mat };
        b.solid(size, centre, mat);
    }
    b.solid(Vec3::new(4.0, 0.2, 24.0), Vec3::new(-12.0, STOREY - 0.1, -2.0), &floor_mat);
    b.solid(Vec3::new(3.0, 0.2, 4.0), Vec3::new(-15.7, STOREY - 0.1, -2.0), &floor_mat);

    // Ground outside the outer walls, where Zombies gather at the windows.
    for (size, centre) in [
        (Vec3::new(60.0, 0.2, 4.0), Vec3::new(0.0, -0.1, 28.0)),
        (Vec3::new(60.0, 0.2, 4.0), Vec3::new(0.0, -0.1, -28.0)),
        (Vec3::new(4.0, 0.2, 52.0), Vec3::new(-28.0, -0.1, 0.0)),
        (Vec3::new(4.0, 0.2, 52.0), Vec3::new(28.0, -0.1, 0.0)),
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
    b.outer_wall(v(-26.0, -26.0), v(26.0, -26.0), STOREY);
    b.outer_wall(v(26.0, -26.0), v(26.0, 26.0), STOREY);

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

    // Yard cover
    for (x, z, w, d) in [(-5.0, 4.0, 3.0, 1.0), (9.0, 3.0, 1.0, 3.0), (-3.0, -6.0, 1.2, 1.2), (2.0, 8.0, 1.2, 1.2)] {
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

    // Spawn Points: one outside each window, the Yard's Risers, and the Hellhound Spawn Points.
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
}
