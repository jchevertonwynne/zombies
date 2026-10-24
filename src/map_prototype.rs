//! PROTOTYPE — throwaway greybox layouts for "Greybox Map layout and Zone graph".
//! Lives on the `prototype/greybox-layout` branch only; never merge.
//!
//! Three radically different layouts, walkable in first person with no Zombies:
//!   F6  cycle layout (A Loop / B Tree / C Vertical)
//!   F7  cycle overhead map: off / inset / large
//!   F   use the nearest slot: open a Door (free), flip the Power switch
//!
//! Colour key: orange = Door, violet = Power-gated Door, brown planks = Barrier (red pad outside
//! = its Spawn Point), green panel = Wall Weapon slot, cyan crate = Mystery Box location
//! (bright = starting location), tall boxes = Perk machines, yellow = Power switch,
//! purple cube = Pack-a-Punch, red pad with no window = ground Spawn Point.

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ScalingMode, Viewport};
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::player::{Player, PlayerCamera};
use crate::state::{GameState, InRun};

const T: f32 = 0.4; // wall thickness
const H: f32 = 4.0; // storey height
const DOOR_W: f32 = 2.4;
const DOOR_H: f32 = 3.0;
const WINDOW_W: f32 = 1.6;
const USE_RANGE: f32 = 2.6;

pub struct MapPrototypePlugin;

impl Plugin for MapPrototypePlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<MapGizmos>()
            .insert_resource(CurrentLayout(0))
            .insert_resource(Overhead(1))
            .add_systems(Startup, configure_gizmos)
            .add_systems(OnEnter(GameState::Playing), build_layout)
            .add_systems(
                Update,
                (
                    next_layout.run_if(input_just_pressed(KeyCode::F6)),
                    next_overhead.run_if(input_just_pressed(KeyCode::F7)),
                    use_slot.run_if(input_just_pressed(KeyCode::KeyF)),
                    teleport_player,
                    ui_camera,
                    place_overhead,
                    draw_map,
                    status_text,
                )
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
struct MapGizmos;

#[derive(Resource)]
struct CurrentLayout(usize);

/// 0 = off, 1 = inset, 2 = large.
#[derive(Resource)]
struct Overhead(u8);

#[derive(Resource, Default)]
struct Built {
    name: &'static str,
    zones: Vec<Zone>,
    edges: Vec<(&'static str, &'static str, bool)>,
    centre: Vec2,
    span: f32,
    power: bool,
    message: String,
}

#[derive(Resource)]
struct PendingTeleport(Vec3);

#[derive(Component)]
struct Piece;

#[derive(Component)]
struct OverheadCamera;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct Slot(String);

#[derive(Component)]
struct ProtoDoor {
    power_gated: bool,
}

#[derive(Component)]
struct PowerSwitch;

struct Zone {
    name: &'static str,
    rect: Rect, // x/z
    y: f32,
    colour: Color,
}

impl Zone {
    fn contains(&self, p: Vec3) -> bool {
        self.rect.contains(Vec2::new(p.x, p.z)) && p.y > self.y - 0.5 && p.y < self.y + H - 0.5
    }
}

enum Gap {
    /// A buyable Door between two Zones, with a placeholder price.
    Door(&'static str, &'static str, u32),
    /// A Door that opens by itself when the Power comes on.
    PowerDoor(&'static str, &'static str),
    /// A Barrier; the sign says which side of the wall is outside.
    Window(f32),
    /// An open archway or drop of the given width.
    Open(f32),
}

struct B<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    mats: &'a mut Assets<StandardMaterial>,
    built: Built,
    start: Vec3,
}

const WALL: Color = Color::srgb(0.35, 0.3, 0.27);
const PLANK: Color = Color::srgb(0.45, 0.3, 0.15);
const SPAWN: Color = Color::srgb(0.8, 0.1, 0.1);

impl B<'_, '_, '_> {
    fn mat(&mut self, colour: Color, glow: bool) -> Handle<StandardMaterial> {
        let emissive = if glow { colour.to_linear() * 2.0 } else { LinearRgba::BLACK };
        self.mats.add(StandardMaterial {
            base_color: colour,
            emissive,
            ..default()
        })
    }

    fn block(&mut self, size: Vec3, at: Vec3, colour: Color, glow: bool, solid: bool) -> Entity {
        let shape = Cuboid::from_size(size);
        let mat = self.mat(colour, glow);
        let mut e = self.commands.spawn((
            InRun,
            Piece,
            Mesh3d(self.meshes.add(shape)),
            MeshMaterial3d(mat),
            Transform::from_translation(at),
        ));
        if solid {
            e.insert((
                RigidBody::Static,
                Collider::from(shape),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ));
        }
        e.id()
    }

    fn zone(&mut self, name: &'static str, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, colour: Color) {
        let rect = Rect::new(x0, z0, x1, z1);
        let size = rect.size();
        let c = rect.center();
        let floor = colour.mix(&Color::srgb(0.2, 0.19, 0.17), 0.75);
        self.block(Vec3::new(size.x, 0.2, size.y), Vec3::new(c.x, y - 0.1, c.y), floor, false, true);
        self.commands.spawn((
            InRun,
            Piece,
            PointLight {
                color: Color::srgb(1.0, 0.8, 0.55),
                intensity: 150_000.0 + 1_500.0 * size.x * size.y,
                range: size.max_element() + 8.0,
                ..default()
            },
            Transform::from_xyz(c.x, y + H - 0.6, c.y),
        ));
        self.built.zones.push(Zone { name, rect, y, colour });
    }

    /// An axis-aligned wall from `a` to `b` (x/z), standing on `y0`, with gaps at offsets from `a`.
    fn wall(&mut self, a: Vec2, b: Vec2, y0: f32, h: f32, gaps: &[(f32, Gap)]) {
        let along_x = a.y == b.y;
        let len = (b - a).length();
        let dir = (b - a) / len;
        let normal = if along_x { Vec3::Z } else { Vec3::X };
        // A box spanning [s, e] along the wall and [lo, hi] vertically.
        let piece = |this: &mut Self, s: f32, e: f32, lo: f32, hi: f32, colour: Color, thick: f32| {
            if e - s < 0.01 || hi - lo < 0.01 {
                return None;
            }
            let mid = a + dir * (s + e) / 2.0;
            let size = if along_x {
                Vec3::new(e - s, hi - lo, thick)
            } else {
                Vec3::new(thick, hi - lo, e - s)
            };
            Some(this.block(size, Vec3::new(mid.x, y0 + (lo + hi) / 2.0, mid.y), colour, false, true))
        };

        let mut cursor = 0.0;
        for (at, gap) in gaps {
            let w = match gap {
                Gap::Door(..) | Gap::PowerDoor(..) => DOOR_W,
                Gap::Window(_) => WINDOW_W,
                Gap::Open(w) => *w,
            };
            let (s, e) = (at - w / 2.0, at + w / 2.0);
            piece(self, cursor, s, 0.0, h, WALL, T);
            cursor = e;
            let mid = a + dir * *at;
            match gap {
                Gap::Door(za, zb, price) => {
                    piece(self, s, e, DOOR_H, h, WALL, T);
                    let id = piece(self, s, e, 0.0, DOOR_H, Color::srgb(0.85, 0.45, 0.1), 0.25).unwrap();
                    let label = format!("Door  {za} <-> {zb}  ({price})");
                    self.commands.entity(id).insert((Slot(label), ProtoDoor { power_gated: false }));
                    self.built.edges.push((za, zb, false));
                }
                Gap::PowerDoor(za, zb) => {
                    piece(self, s, e, DOOR_H, h, WALL, T);
                    let id = piece(self, s, e, 0.0, DOOR_H, Color::srgb(0.5, 0.3, 0.8), 0.25).unwrap();
                    let label = format!("Power Door  {za} <-> {zb}  (opens with Power)");
                    self.commands.entity(id).insert((Slot(label), ProtoDoor { power_gated: true }));
                    self.built.edges.push((za, zb, true));
                }
                Gap::Window(out) => {
                    piece(self, s, e, 0.0, 1.0, WALL, T);
                    for i in 0..4 {
                        let lo = 1.05 + i as f32 * 0.3;
                        piece(self, s, e, lo, lo + 0.22, PLANK, 0.15);
                    }
                    piece(self, s, e, 2.25, h, WALL, T);
                    let pad = Vec3::new(mid.x, y0 + 0.03, mid.y) + normal * *out * 2.0;
                    self.block(Vec3::new(1.0, 0.06, 1.0), pad, SPAWN, true, false);
                }
                Gap::Open(_) => {
                    piece(self, s, e, DOOR_H, h, WALL, T);
                }
            }
        }
        piece(self, cursor, len, 0.0, h, WALL, T);
    }

    fn slot(&mut self, label: impl Into<String>, size: Vec3, at: Vec3, colour: Color, glow: bool) -> Entity {
        let id = self.block(size, at, colour, glow, true);
        self.commands.entity(id).insert(Slot(label.into()));
        id
    }

    fn perk(&mut self, name: &str, colour: Color, x: f32, y: f32, z: f32) {
        self.slot(format!("Perk machine: {name}"), Vec3::new(1.0, 2.2, 1.0), Vec3::new(x, y + 1.1, z), colour, true);
    }

    fn wall_weapon(&mut self, name: &str, x: f32, y: f32, z: f32, on_x_wall: bool) {
        let size = if on_x_wall { Vec3::new(0.08, 0.7, 1.4) } else { Vec3::new(1.4, 0.7, 0.08) };
        let id = self.block(size, Vec3::new(x, y + 1.6, z), Color::srgb(0.2, 0.8, 0.3), true, false);
        self.commands.entity(id).insert(Slot(format!("Wall Weapon slot: {name}")));
    }

    fn box_location(&mut self, x: f32, y: f32, z: f32, along_x: bool, start: bool) {
        let size = if along_x { Vec3::new(1.8, 0.9, 0.8) } else { Vec3::new(0.8, 0.9, 1.8) };
        let label = if start { "Mystery Box location (starting)" } else { "Mystery Box location" };
        let colour = if start { Color::srgb(0.3, 0.9, 1.0) } else { Color::srgb(0.15, 0.4, 0.5) };
        self.slot(label, size, Vec3::new(x, y + 0.45, z), colour, start);
    }

    fn power(&mut self, x: f32, y: f32, z: f32) {
        let id = self.slot("Power switch", Vec3::new(0.6, 1.0, 0.6), Vec3::new(x, y + 1.5, z), Color::srgb(1.0, 0.85, 0.1), true);
        self.commands.entity(id).insert(PowerSwitch);
    }

    fn pack_a_punch(&mut self, x: f32, y: f32, z: f32) {
        self.slot("Pack-a-Punch", Vec3::splat(1.6), Vec3::new(x, y + 0.8, z), Color::srgb(0.6, 0.1, 0.9), true);
    }

    fn ground_spawn(&mut self, x: f32, y: f32, z: f32) {
        self.block(Vec3::new(1.0, 0.06, 1.0), Vec3::new(x, y + 0.03, z), SPAWN, true, false);
    }

    fn cover(&mut self, x: f32, z: f32, w: f32, d: f32) {
        self.block(Vec3::new(w, 1.1, d), Vec3::new(x, 0.55, z), Color::srgb(0.4, 0.28, 0.15), false, true);
    }
}

fn v(x: f32, z: f32) -> Vec2 {
    Vec2::new(x, z)
}

const JUGG: Color = Color::srgb(0.8, 0.1, 0.1);
const SPEED: Color = Color::srgb(0.1, 0.7, 0.2);
const DTAP: Color = Color::srgb(0.9, 0.7, 0.1);
const QUICK: Color = Color::srgb(0.2, 0.4, 0.9);

fn zc(i: usize) -> Color {
    [
        Color::srgb(0.9, 0.9, 0.9),
        Color::srgb(0.95, 0.6, 0.3),
        Color::srgb(0.4, 0.8, 0.95),
        Color::srgb(0.6, 0.95, 0.4),
        Color::srgb(0.95, 0.5, 0.8),
        Color::srgb(0.95, 0.9, 0.35),
        Color::srgb(0.6, 0.55, 1.0),
    ][i]
}

/// A — Loop (Verrückt-ish ring). Five rooms ring a central Courtyard; two routes of equal length
/// reach Power in the far corner; Pack-a-Punch sits in the Courtyard, behind a Power Door that
/// opens off the Start Room.
fn layout_a(b: &mut B) {
    b.built.name = "A  Loop";
    b.built.centre = v(0.0, 0.0);
    b.built.span = 46.0;
    b.start = Vec3::new(0.0, 1.0, 15.0);

    b.zone("Start Room", -8.0, 8.0, 8.0, 20.0, 0.0, zc(0));
    b.zone("West Hall", -20.0, -8.0, -8.0, 20.0, 0.0, zc(1));
    b.zone("East Hall", 8.0, -8.0, 20.0, 20.0, 0.0, zc(2));
    b.zone("Dining Room", -20.0, -20.0, 0.0, -8.0, 0.0, zc(3));
    b.zone("Power Room", 0.0, -20.0, 20.0, -8.0, 0.0, zc(4));
    b.zone("Courtyard", -8.0, -8.0, 8.0, 8.0, 0.0, zc(5));

    use Gap::*;
    b.wall(v(-20.0, 20.0), v(20.0, 20.0), 0.0, H, &[(6.0, Window(1.0)), (16.0, Window(1.0)), (24.0, Window(1.0)), (34.0, Window(1.0))]);
    b.wall(v(-20.0, -20.0), v(20.0, -20.0), 0.0, H, &[(10.0, Window(-1.0)), (30.0, Window(-1.0))]);
    b.wall(v(-20.0, -20.0), v(-20.0, 20.0), 0.0, H, &[(6.0, Window(-1.0)), (20.0, Window(-1.0)), (32.0, Window(-1.0))]);
    b.wall(v(20.0, -20.0), v(20.0, 20.0), 0.0, H, &[(6.0, Window(1.0)), (20.0, Window(1.0)), (32.0, Window(1.0))]);
    b.wall(v(-8.0, 8.0), v(-8.0, 20.0), 0.0, H, &[(6.0, Door("Start Room", "West Hall", 750))]);
    b.wall(v(8.0, 8.0), v(8.0, 20.0), 0.0, H, &[(6.0, Door("Start Room", "East Hall", 750))]);
    b.wall(v(-20.0, -8.0), v(-8.0, -8.0), 0.0, H, &[(6.0, Door("West Hall", "Dining Room", 1000))]);
    b.wall(v(8.0, -8.0), v(20.0, -8.0), 0.0, H, &[(6.0, Door("East Hall", "Power Room", 1000))]);
    b.wall(v(0.0, -20.0), v(0.0, -8.0), 0.0, H, &[(6.0, Door("Dining Room", "Power Room", 1250))]);
    b.wall(v(-8.0, -8.0), v(-8.0, 8.0), 0.0, H, &[]);
    b.wall(v(8.0, -8.0), v(8.0, 8.0), 0.0, H, &[]);
    b.wall(v(-8.0, -8.0), v(8.0, -8.0), 0.0, H, &[]);
    b.wall(v(-8.0, 8.0), v(8.0, 8.0), 0.0, H, &[(8.0, PowerDoor("Start Room", "Courtyard"))]);

    b.wall_weapon("M14", -4.5, 0.0, 8.25, false);
    b.wall_weapon("Olympia", 4.5, 0.0, 8.25, false);
    b.box_location(-5.0, 0.0, 18.8, true, true);
    b.perk("Quick Revive", QUICK, 6.5, 0.0, 18.5);

    b.perk("Juggernog", JUGG, -18.5, 0.0, 4.0);
    b.wall_weapon("MP40", -8.25, 0.0, 0.0, true);

    b.box_location(-14.0, 0.0, -18.8, true, false);
    b.perk("Speed Cola", SPEED, -3.0, 0.0, -18.5);

    b.power(18.5, 0.0, -18.5);
    b.perk("Double Tap", DTAP, 5.0, 0.0, -18.5);

    b.box_location(18.8, 0.0, 6.0, false, false);
    b.wall_weapon("Trench Gun", 8.25, 0.0, 0.0, true);

    b.pack_a_punch(0.0, 0.0, 0.0);
    b.box_location(-6.0, 0.0, -6.5, true, false);
    b.ground_spawn(5.0, 0.0, -5.0);
    b.ground_spawn(-5.0, 0.0, 4.0);
}

/// B — Tree (Nacht/Kino-ish). A Start Room with a dead-end Storage wing and one long, narrow
/// Corridor to a big Theater holding Power. Power opens a Pack-a-Punch closet back in Start.
fn layout_b(b: &mut B) {
    b.built.name = "B  Tree";
    b.built.centre = v(-3.0, -20.0);
    b.built.span = 60.0;
    b.start = Vec3::new(0.0, 1.0, 2.0);

    b.zone("Start Room", -8.0, -6.0, 8.0, 6.0, 0.0, zc(0));
    b.zone("Storage", -20.0, -10.0, -8.0, 6.0, 0.0, zc(1));
    b.zone("Corridor", -2.0, -26.0, 2.0, -6.0, 0.0, zc(2));
    b.zone("Theater", -14.0, -46.0, 14.0, -26.0, 0.0, zc(3));
    b.zone("PaP Closet", 8.0, -3.0, 12.0, 3.0, 0.0, zc(4));

    use Gap::*;
    // Start Room
    b.wall(v(-8.0, 6.0), v(8.0, 6.0), 0.0, H, &[(4.0, Window(1.0)), (12.0, Window(1.0))]);
    b.wall(v(8.0, -6.0), v(8.0, 6.0), 0.0, H, &[(1.5, Window(1.0)), (6.0, PowerDoor("Start Room", "PaP Closet"))]);
    b.wall(v(-8.0, -10.0), v(-8.0, 6.0), 0.0, H, &[(10.0, Door("Start Room", "Storage", 750))]);
    b.wall(v(-8.0, -6.0), v(8.0, -6.0), 0.0, H, &[(8.0, Door("Start Room", "Corridor", 1000))]);
    // PaP Closet
    b.wall(v(8.0, -3.0), v(12.0, -3.0), 0.0, H, &[]);
    b.wall(v(8.0, 3.0), v(12.0, 3.0), 0.0, H, &[]);
    b.wall(v(12.0, -3.0), v(12.0, 3.0), 0.0, H, &[]);
    // Storage
    b.wall(v(-20.0, -10.0), v(-8.0, -10.0), 0.0, H, &[(6.0, Window(-1.0))]);
    b.wall(v(-20.0, -10.0), v(-20.0, 6.0), 0.0, H, &[(4.0, Window(-1.0)), (12.0, Window(-1.0))]);
    b.wall(v(-20.0, 6.0), v(-8.0, 6.0), 0.0, H, &[(6.0, Window(1.0))]);
    // Corridor
    b.wall(v(-2.0, -26.0), v(-2.0, -6.0), 0.0, H, &[(6.0, Window(-1.0)), (14.0, Window(-1.0))]);
    b.wall(v(2.0, -26.0), v(2.0, -6.0), 0.0, H, &[(10.0, Window(1.0))]);
    // Theater
    b.wall(v(-14.0, -26.0), v(14.0, -26.0), 0.0, H, &[(4.0, Window(1.0)), (14.0, Door("Corridor", "Theater", 1250)), (24.0, Window(1.0))]);
    b.wall(v(-14.0, -46.0), v(14.0, -46.0), 0.0, H, &[(5.0, Window(-1.0)), (23.0, Window(-1.0))]);
    b.wall(v(-14.0, -46.0), v(-14.0, -26.0), 0.0, H, &[(6.0, Window(-1.0)), (14.0, Window(-1.0))]);
    b.wall(v(14.0, -46.0), v(14.0, -26.0), 0.0, H, &[(6.0, Window(1.0)), (14.0, Window(1.0))]);
    // Stage
    b.block(Vec3::new(20.0, 0.5, 6.0), Vec3::new(0.0, 0.25, -43.0), Color::srgb(0.3, 0.2, 0.15), false, true);
    for x in [-8.0, 8.0] {
        b.block(Vec3::new(1.0, H, 1.0), Vec3::new(x, H / 2.0, -33.0), WALL, false, true);
    }

    b.wall_weapon("M14", -4.0, 0.0, 5.75, false);
    b.wall_weapon("Olympia", 7.75, 0.0, -4.5, true);
    b.box_location(-5.0, 0.0, -4.8, true, true);
    b.perk("Quick Revive", QUICK, 6.5, 0.0, 4.5);

    b.perk("Juggernog", JUGG, -18.5, 0.0, -8.5);
    b.wall_weapon("MP40", -14.0, 0.0, -9.75, false);
    b.box_location(-18.8, 0.0, 0.0, false, false);

    b.wall_weapon("Kar98k", 1.75, 0.0, -20.0, true);

    b.power(0.0, 0.5, -45.0);
    b.perk("Speed Cola", SPEED, -12.5, 0.0, -28.0);
    b.perk("Double Tap", DTAP, 12.5, 0.5, -44.5);
    b.box_location(-12.8, 0.0, -36.0, false, false);
    b.wall_weapon("Trench Gun", 13.75, 0.0, -36.0, true);

    b.pack_a_punch(10.5, 0.0, 0.0);
}

/// C — Vertical (balcony over a yard). A Start Room opens three ways: a Stairwell ramping up to a
/// Balcony over a big open Yard, a Kitchen with the Power Room above it, and the Yard itself.
/// The Balcony has a one-way drop into the Yard; Pack-a-Punch sits in a Power-gated gazebo.
fn layout_c(b: &mut B) {
    b.built.name = "C  Vertical";
    b.built.centre = v(0.0, -9.0);
    b.built.span = 48.0;
    b.start = Vec3::new(0.0, 1.0, 8.0);

    b.zone("Start Room", -6.0, 0.0, 6.0, 12.0, 0.0, zc(0));
    b.zone("Stairwell", -18.0, 0.0, -6.0, 12.0, 0.0, zc(1));
    b.built.zones.push(Zone { name: "Stairwell", rect: Rect::new(-18.0, 0.0, -6.0, 12.0), y: H, colour: zc(1) });
    b.zone("Kitchen", 6.0, 0.0, 18.0, 12.0, 0.0, zc(2));
    b.zone("Power Room", 6.0, 0.0, 18.0, 12.0, 4.0, zc(3));
    b.zone("Yard", -18.0, -30.0, 18.0, 0.0, 0.0, zc(4));
    b.zone("Balcony", -18.0, -4.0, 18.0, 0.0, 4.0, zc(5));
    // The Stairwell's upper storey is just a landing at the top of the ramp.
    b.block(Vec3::new(4.0, 0.2, 4.0), Vec3::new(-16.0, H - 0.1, 2.0), zc(1).mix(&WALL, 0.75), false, true);

    use Gap::*;
    // Start Room
    b.wall(v(-6.0, 12.0), v(6.0, 12.0), 0.0, H, &[(3.0, Window(1.0)), (9.0, Window(1.0))]);
    b.wall(v(-6.0, 0.0), v(6.0, 0.0), 0.0, H, &[(6.0, Door("Start Room", "Yard", 1000))]);
    b.wall(v(-6.0, 0.0), v(-6.0, 12.0), 0.0, 2.0 * H, &[(6.0, Door("Start Room", "Stairwell", 750))]);
    b.wall(v(6.0, 0.0), v(6.0, 12.0), 0.0, 2.0 * H, &[(6.0, Door("Start Room", "Kitchen", 750))]);
    // Stairwell (two storeys)
    b.wall(v(-18.0, 0.0), v(-18.0, 12.0), 0.0, 2.0 * H, &[(8.0, Window(-1.0))]);
    b.wall(v(-18.0, 12.0), v(-6.0, 12.0), 0.0, 2.0 * H, &[(6.0, Window(1.0))]);
    b.wall(v(-18.0, 0.0), v(-6.0, 0.0), 0.0, H, &[]);
    b.wall(v(-18.0, 0.0), v(-6.0, 0.0), H, H, &[(2.0, Open(3.6))]);
    // Kitchen and Power Room above it
    b.wall(v(18.0, 0.0), v(18.0, 12.0), 0.0, 2.0 * H, &[(6.0, Window(1.0))]);
    b.wall(v(6.0, 12.0), v(18.0, 12.0), 0.0, 2.0 * H, &[(6.0, Window(1.0))]);
    b.wall(v(6.0, 0.0), v(18.0, 0.0), 0.0, H, &[(6.0, Door("Kitchen", "Yard", 1000))]);
    b.wall(v(6.0, 0.0), v(18.0, 0.0), H, H, &[(6.0, Door("Balcony", "Power Room", 1250))]);
    // Balcony: back wall above the Start Room, end walls, railing with a drop
    b.wall(v(-6.0, 0.0), v(6.0, 0.0), H, H, &[]);
    b.wall(v(-18.0, -4.0), v(-18.0, 0.0), H, H, &[]);
    b.wall(v(18.0, -4.0), v(18.0, 0.0), H, H, &[]);
    b.wall(v(-18.0, -4.0), v(18.0, -4.0), H, 1.0, &[(19.0, Open(2.4))]);
    for x in [-12.0, 0.0, 12.0] {
        b.block(Vec3::new(0.5, H, 0.5), Vec3::new(x, H / 2.0, -3.8), WALL, false, true);
    }
    // Ramp from the Stairwell floor (x = -7) up to the landing (x = -14)
    let rise = H;
    let run = 7.0;
    let len = (rise * rise + run * run).sqrt();
    let ramp = Cuboid::new(len, 0.3, 3.5);
    let mat = b.mat(Color::srgb(0.3, 0.25, 0.2), false);
    b.commands.spawn((
        InRun,
        Piece,
        Mesh3d(b.meshes.add(ramp)),
        MeshMaterial3d(mat),
        Transform::from_xyz(-10.5, rise / 2.0 - 0.17, 2.25).with_rotation(Quat::from_rotation_z(-(rise / run).atan())),
        RigidBody::Static,
        Collider::from(ramp),
        CollisionLayers::new(GameLayer::World, LayerMask::ALL),
    ));
    // Yard
    b.wall(v(-18.0, -30.0), v(-18.0, 0.0), 0.0, H, &[(8.0, Window(-1.0)), (22.0, Window(-1.0))]);
    b.wall(v(18.0, -30.0), v(18.0, 0.0), 0.0, H, &[(8.0, Window(1.0)), (22.0, Window(1.0))]);
    b.wall(v(-18.0, -30.0), v(18.0, -30.0), 0.0, H, &[(10.0, Window(-1.0)), (26.0, Window(-1.0))]);
    // Gazebo
    b.wall(v(-3.0, -18.0), v(3.0, -18.0), 0.0, 3.0, &[]);
    b.wall(v(-3.0, -18.0), v(-3.0, -12.0), 0.0, 3.0, &[]);
    b.wall(v(3.0, -18.0), v(3.0, -12.0), 0.0, 3.0, &[]);
    b.wall(v(-3.0, -12.0), v(3.0, -12.0), 0.0, 3.0, &[(3.0, PowerDoor("Yard", "Gazebo"))]);
    for (x, z, w, d) in [(-10.0, -10.0, 3.0, 1.0), (10.0, -10.0, 3.0, 1.0), (-10.0, -22.0, 1.0, 3.0), (10.0, -22.0, 1.0, 3.0)] {
        b.cover(x, z, w, d);
    }

    b.wall_weapon("M14", -3.0, 0.0, 11.75, false);
    b.wall_weapon("Olympia", 3.0, 0.0, 0.25, false);
    b.box_location(-3.5, 0.0, 1.0, true, true);
    b.perk("Quick Revive", QUICK, 4.5, 0.0, 10.5);

    b.wall_weapon("MP40", -12.0, 0.0, 11.75, false);

    b.perk("Speed Cola", SPEED, -16.0, H, -2.0);

    b.perk("Juggernog", JUGG, 16.5, 0.0, 2.0);
    b.wall_weapon("Trench Gun", 17.75, 0.0, 10.0, true);

    b.power(16.5, H, 10.5);
    b.perk("Double Tap", DTAP, 8.0, H, 10.5);
    b.box_location(12.0, H, 10.8, true, false);

    b.pack_a_punch(0.0, 0.0, -15.0);
    b.box_location(-16.8, 0.0, -15.0, false, false);
    b.wall_weapon("Kar98k", 17.75, 0.0, -15.0, true);
}

const LAYOUTS: [fn(&mut B); 3] = [layout_a, layout_b, layout_c];

fn configure_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<MapGizmos>();
    config.render_layers = RenderLayers::layer(1);
    config.line.width = 3.0;
    config.depth_bias = -1.0;
}

fn build_layout(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    current: Res<CurrentLayout>,
    old: Query<Entity, With<Piece>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let mut b = B {
        commands: &mut commands,
        meshes: &mut meshes,
        mats: &mut mats,
        built: Built::default(),
        start: Vec3::ZERO,
    };
    LAYOUTS[current.0](&mut b);
    let (built, start) = (b.built, b.start);

    // Overhead camera, north up
    commands.spawn((
        InRun,
        Piece,
        OverheadCamera,
        Camera3d::default(),
        Camera {
            order: 1,
            is_active: false,
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: built.span,
            },
            ..OrthographicProjection::default_3d()
        }),
        AmbientLight {
            color: Color::WHITE,
            brightness: 800.0,
            ..default()
        },
        RenderLayers::from_layers(&[0, 1]),
        Transform::from_xyz(built.centre.x, 60.0, built.centre.y).looking_at(
            Vec3::new(built.centre.x, 0.0, built.centre.y),
            Vec3::NEG_Z,
        ),
    ));

    // Status panel plus a Zone legend in each Zone's colour
    commands
        .spawn((
            InRun,
            Piece,
            Node {
                position_type: PositionType::Absolute,
                left: px(16.0),
                top: px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(2.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|col| {
            col.spawn((StatusText, Text::new(""), font(18.0), TextColor(Color::WHITE)));
            let mut seen = Vec::new();
            for zone in &built.zones {
                if !seen.contains(&zone.name) {
                    seen.push(zone.name);
                    col.spawn((Text::new(format!("  # {}", zone.name)), font(16.0), TextColor(zone.colour)));
                }
            }
        });

    commands.insert_resource(built);
    commands.insert_resource(PendingTeleport(start));
}

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn next_layout(mut current: ResMut<CurrentLayout>, mut commands: Commands) {
    current.0 = (current.0 + 1) % LAYOUTS.len();
    commands.run_system_cached(build_layout);
}

fn next_overhead(mut overhead: ResMut<Overhead>) {
    overhead.0 = (overhead.0 + 1) % 3;
}

fn teleport_player(mut commands: Commands, pending: Option<Res<PendingTeleport>>, player: Option<Single<&mut Transform, With<Player>>>) {
    if let (Some(pending), Some(mut player)) = (pending, player) {
        player.translation = pending.0;
        commands.remove_resource::<PendingTeleport>();
    }
}

fn ui_camera(mut commands: Commands, cameras: Query<Entity, Added<PlayerCamera>>) {
    for e in &cameras {
        commands.entity(e).insert(IsDefaultUiCamera);
    }
}

fn place_overhead(overhead: Res<Overhead>, window: Single<&Window>, mut camera: Query<&mut Camera, With<OverheadCamera>>) {
    let Ok(mut camera) = camera.single_mut() else { return };
    let (w, h) = (window.physical_width(), window.physical_height());
    let side = match overhead.0 {
        1 => h * 2 / 5,
        2 => h * 9 / 10,
        _ => 0,
    };
    camera.is_active = side > 0;
    if side > 0 {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(w.saturating_sub(side + 16), 16),
            physical_size: UVec2::new(side, side),
            ..default()
        });
    }
}

fn draw_map(mut gizmos: Gizmos<MapGizmos>, built: Option<Res<Built>>, player: Option<Single<&Transform, With<Player>>>) {
    let Some(built) = built else { return };
    for zone in &built.zones {
        let r = zone.rect;
        let y = zone.y + 0.15;
        let corners = [
            Vec3::new(r.min.x, y, r.min.y),
            Vec3::new(r.max.x, y, r.min.y),
            Vec3::new(r.max.x, y, r.max.y),
            Vec3::new(r.min.x, y, r.max.y),
        ];
        for i in 0..4 {
            gizmos.line(corners[i], corners[(i + 1) % 4], zone.colour);
        }
    }
    // Zone graph: an edge between Zone centres per Door, high above the walls.
    let centre = |name: &str| {
        let z = built.zones.iter().find(|z| z.name == name)?;
        let c = z.rect.center();
        Some(Vec3::new(c.x, 20.0 + z.y, c.y))
    };
    for (a, b, power) in &built.edges {
        if let (Some(pa), Some(pb)) = (centre(a), centre(b)) {
            let colour = if *power { Color::srgb(0.7, 0.4, 1.0) } else { Color::srgb(1.0, 0.55, 0.1) };
            gizmos.line(pa, pb, colour);
        }
    }
    if let Some(player) = player {
        let p = player.translation.with_y(30.0);
        let fwd = player.forward().as_vec3().with_y(0.0) * 1.5;
        gizmos.line(p - fwd * 0.5, p + fwd, Color::WHITE);
        gizmos.line(p + fwd.cross(Vec3::Y) * 0.5, p - fwd.cross(Vec3::Y) * 0.5, Color::WHITE);
    }
}

fn nearest_slot<'a>(at: Vec3, slots: impl Iterator<Item = (Entity, &'a GlobalTransform, &'a Slot)>) -> Option<(Entity, &'a Slot)> {
    slots
        .map(|(e, t, s)| (e, s, t.translation().distance(at)))
        .filter(|(_, _, d)| *d < USE_RANGE)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(e, s, _)| (e, s))
}

fn use_slot(
    mut commands: Commands,
    mut built: ResMut<Built>,
    player: Single<&Transform, With<Player>>,
    slots: Query<(Entity, &GlobalTransform, &Slot)>,
    doors: Query<(Entity, &ProtoDoor)>,
    switches: Query<(), With<PowerSwitch>>,
) {
    let Some((e, slot)) = nearest_slot(player.translation, slots.iter()) else { return };
    if let Ok((_, door)) = doors.get(e) {
        if door.power_gated && !built.power {
            built.message = "Requires Power".into();
        } else {
            commands.entity(e).despawn();
            built.message = format!("Opened: {}", slot.0);
        }
    } else if switches.contains(e) {
        built.power = true;
        built.message = "POWER ON".into();
        for (door, d) in &doors {
            if d.power_gated {
                commands.entity(door).despawn();
            }
        }
    } else {
        built.message = format!("(slot only) {}", slot.0);
    }
}

fn status_text(
    built: Option<Res<Built>>,
    player: Option<Single<&Transform, With<Player>>>,
    slots: Query<(Entity, &GlobalTransform, &Slot)>,
    mut text: Query<&mut Text, With<StatusText>>,
) {
    let (Some(built), Some(player), Ok(mut text)) = (built, player, text.single_mut()) else { return };
    let p = player.translation;
    let zone = built.zones.iter().find(|z| z.contains(p)).map_or("(outside any Zone)", |z| z.name);
    let near = nearest_slot(p, slots.iter()).map_or(String::new(), |(_, s)| format!("[F] {}", s.0));
    text.0 = format!(
        "PROTOTYPE greybox  -  Layout {}   (F6 next layout, F7 overhead map)\nZone: {}\nPower: {}\n{}\n{}\nZones:",
        built.name,
        zone,
        if built.power { "ON" } else { "off" },
        near,
        built.message,
    );
}
