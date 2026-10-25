//! The greybox Map: a ring of five indoor Zones around an open Yard, plus a Balcony over the Yard
//! reached by a ramp in West Hall, with a one-way drop back down.

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::buy::Buyable;
use crate::door::Door;
use crate::layers::GameLayer;
use crate::state::{GameState, InRun};
use crate::tuning::*;
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

/// A fixed place in the Map where Zombies enter play, in the Zone it serves.
#[derive(Component)]
pub struct SpawnPoint(pub ZoneId);

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
            };
            let (s, e) = (at - width / 2.0, at + width / 2.0);
            piece(self, cursor, s, 0.0, height, WALL_THICKNESS, &wall_mat);
            piece(self, s, e, top, height, WALL_THICKNESS, &wall_mat);
            cursor = e;
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
}

fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

fn door(a: ZoneId, b: ZoneId, price: u64) -> Gap {
    Gap::Door(Some((a, b)), Some(price))
}

fn spawn_map(
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

    // Outer walls. West Hall's are two storeys tall, since its ramp climbs to the first storey.
    let tall = 2.0 * STOREY;
    b.wall(v(-26.0, 26.0), v(-14.0, 26.0), 0.0, tall, &[]);
    b.wall(v(-14.0, 26.0), v(26.0, 26.0), 0.0, STOREY, &[]);
    b.wall(v(-26.0, -14.0), v(-26.0, 26.0), 0.0, tall, &[]);
    b.wall(v(-26.0, -26.0), v(-26.0, -14.0), 0.0, STOREY, &[]);
    b.wall(v(-26.0, -26.0), v(26.0, -26.0), 0.0, STOREY, &[]);
    b.wall(v(26.0, -26.0), v(26.0, 26.0), 0.0, STOREY, &[]);

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

    // Spawn Points. Windows come with Barriers; until then indoor Zombies appear by the outer
    // walls, and Yard Zombies rise from the ground.
    for (zone, x, z) in [
        (StartRoom, -8.0, 24.5),
        (StartRoom, 8.0, 24.5),
        (WestHall, -24.5, 18.0),
        (WestHall, -24.5, 2.0),
        (WestHall, -24.5, -10.0),
        (EastHall, 24.5, 18.0),
        (EastHall, 24.5, 2.0),
        (EastHall, 24.5, -10.0),
        (DiningRoom, -20.0, -24.5),
        (DiningRoom, -6.0, -24.5),
        (PowerRoom, 6.0, -24.5),
        (PowerRoom, 20.0, -24.5),
        (Yard, -7.0, 10.0),
        (Yard, 10.0, 10.0),
        (Yard, -4.0, -11.0),
        (Yard, 11.0, -10.0),
    ] {
        commands.spawn((InRun, SpawnPoint(zone), Transform::from_xyz(x, 0.0, z)));
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
