//! How the Map's data becomes the world: solid walls each face painted for the Zone it looks
//! into, floors, and the boxes everything else is drawn with.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::economy::buy::{Buyable, Refused};
use crate::maps::door::{Door, LINKED_DOOR_PROMPT, OpensOnLinks};
use crate::layers::GameLayer;
use crate::state::InRun;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

use super::{DOOR_HEIGHT, DOOR_WIDTH, Palette, Prop, PropKind, RaisedFloor, STOREY, Slope, Sunken, WALL_THICKNESS, WINDOW_WIDTH, palette};
use crate::maps::jump_pad::JumpPad;

/// How tall the skirting along the foot of a wall is, and how far it stands out from the face.
const SKIRTING: Vec2 = Vec2::new(0.16, 0.03);
/// How tall the band along the top of each storey of wall is, and how far it stands out.
const CORNICE: Vec2 = Vec2::new(0.18, 0.04);
/// How high a step is drawn, about.
const STEP_RISE: f32 = 0.15;
/// How thick the slab under a ramp or a flight of steps is.
const SLAB: f32 = 0.3;

/// What the Props are made of.
const CRATE_WOOD: [Color; 2] = [Color::srgb(0.46, 0.33, 0.18), Color::srgb(0.38, 0.27, 0.15)];
const TABLE_WOOD: Color = Color::srgb(0.3, 0.19, 0.11);
const BENCH_WOOD: Color = Color::srgb(0.34, 0.22, 0.13);
const COUNTER_WOOD: Color = Color::srgb(0.32, 0.2, 0.12);
const COUNTER_TOP: Color = Color::srgb(0.2, 0.13, 0.08);
const ALTAR_STONE: Color = Color::srgb(0.58, 0.56, 0.5);
const PILLAR_STONE: Color = Color::srgb(0.52, 0.5, 0.45);
const SHELF_WOOD: Color = Color::srgb(0.26, 0.19, 0.13);
const SHELF_BOARD: Color = Color::srgb(0.4, 0.3, 0.2);
const RUBBLE_STONE: [Color; 3] = [Color::srgb(0.42, 0.4, 0.37), Color::srgb(0.36, 0.34, 0.31), Color::srgb(0.49, 0.46, 0.42)];
const SANDBAG: [Color; 2] = [Color::srgb(0.52, 0.47, 0.33), Color::srgb(0.45, 0.4, 0.28)];
const DRUM_PAINT: [Color; 2] = [Color::srgb(0.45, 0.16, 0.1), Color::srgb(0.27, 0.31, 0.18)];
const DRUM_RING: Color = Color::srgb(0.15, 0.15, 0.15);
const MACHINE_STEEL: Color = Color::srgb(0.21, 0.26, 0.24);
const MACHINE_PANEL: Color = Color::srgb(0.3, 0.35, 0.33);
const BARK: Color = Color::srgb(0.22, 0.16, 0.11);
const LEAVES: [Color; 2] = [Color::srgb(0.12, 0.2, 0.09), Color::srgb(0.16, 0.24, 0.1)];
const HEADSTONE: [Color; 3] = [Color::srgb(0.55, 0.55, 0.53), Color::srgb(0.46, 0.47, 0.46), Color::srgb(0.6, 0.58, 0.52)];
/// Hazard yellow, for the Jump Pads.
const JUMP_PAD_PAINT: Color = Color::srgb(0.86, 0.68, 0.1);

/// A gap in a wall, at an offset along it.
pub(super) enum Gap {
    /// A Door, joining two Zones; `None` price makes it a Power Door.
    Door(Option<(ZoneId, ZoneId)>, Option<u64>),
    /// A Door that can't be bought and opens once every Teleporter Pad is Linked, like the
    /// Gazebo's.
    LinkedDoor(Option<(ZoneId, ZoneId)>),
    /// An open archway.
    Open { width: f32, height: f32 },
    /// A window in an outer wall, open to Zombies and solid to the Player.
    Window,
}

/// Spawns the Map's pieces. Everything is drawn with one unit cube, scaled, and one material per
/// colour.
pub(super) struct Builder<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    meshes: &'a mut Assets<Mesh>,
    materials: &'a mut Assets<StandardMaterial>,
    cube: Handle<Mesh>,
    /// One unit cylinder, scaled to every drum.
    cylinder: Handle<Mesh>,
    paints: HashMap<[u8; 4], Handle<StandardMaterial>>,
    door: Handle<StandardMaterial>,
    power_door: Handle<StandardMaterial>,
    /// While set, walls are painted in this instead of their Zones' palettes.
    pub repaint: Option<Palette>,
    /// The Map's windows, which its outer walls leave gaps for.
    windows: &'static [(ZoneId, Vec3, Vec2)],
}

impl<'a, 'w, 's> Builder<'a, 'w, 's> {
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        meshes: &'a mut Assets<Mesh>,
        materials: &'a mut Assets<StandardMaterial>,
        windows: &'static [(ZoneId, Vec3, Vec2)],
    ) -> Self {
        let cube = meshes.add(Cuboid::from_length(1.0));
        let cylinder = meshes.add(Cylinder::new(0.5, 1.0));
        let door = materials.add(Color::srgb(0.85, 0.45, 0.1));
        let power_door = materials.add(Color::srgb(0.5, 0.3, 0.8));
        Self { commands, meshes, materials, cube, cylinder, paints: HashMap::new(), door, power_door, repaint: None, windows }
    }

    /// The material for a flat colour.
    fn paint(&mut self, colour: Color) -> Handle<StandardMaterial> {
        let materials = &mut *self.materials;
        self.paints
            .entry(colour.to_srgba().to_u8_array())
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: colour,
                    perceptual_roughness: 0.9,
                    ..default()
                })
            })
            .clone()
    }

    /// A box drawn in `colour`, `size` across before `rotation`, centred on `at`. Not solid.
    fn shape(&mut self, size: Vec3, at: Vec3, rotation: Quat, colour: Color) -> Entity {
        let paint = self.paint(colour);
        self.commands
            .spawn((
                InRun,
                Mesh3d(self.cube.clone()),
                MeshMaterial3d(paint),
                Transform { translation: at, rotation, scale: size },
            ))
            .id()
    }

    /// Something solid with nothing drawn: what's seen of it is drawn on its own.
    fn bare(&mut self, collider: Collider, at: Vec3, rotation: Quat) -> Entity {
        self.commands
            .spawn((
                InRun,
                Transform::from_translation(at).with_rotation(rotation),
                RigidBody::Static,
                collider,
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            .id()
    }

    /// A solid box drawn in `colour`.
    pub fn block(&mut self, size: Vec3, at: Vec3, colour: Color) -> Entity {
        self.shape(size, at, Quat::IDENTITY, colour);
        self.bare(Collider::cuboid(size.x, size.y, size.z), at, Quat::IDENTITY)
    }

    /// A Zone's floor at its height, round any sunken floors in it, and deep enough that their
    /// edges are solid all the way down.
    pub fn floor(&mut self, zone: ZoneId, holes: &[Sunken]) {
        let (lo, hi) = zone.volume();
        let height = zone.floor();
        let deepest = holes.iter().map(|hole| hole.depth).fold(0.0, f32::max);
        let thick = 0.2 + deepest;
        let colour = palette(Some(zone)).floor;
        let cut: Vec<_> = holes.iter().map(|hole| (hole.min, hole.max)).collect();
        for (min, max) in rects_around((Vec2::new(lo.x, lo.z), Vec2::new(hi.x, hi.z)), &cut) {
            let size = Vec3::new(max.x - min.x, thick, max.y - min.y);
            let centre = Vec3::new((min.x + max.x) / 2.0, height - thick / 2.0, (min.y + max.y) / 2.0);
            self.block(size, centre, colour);
        }
    }

    /// A Raised Floor, with a lip of its Zone's trim round its top.
    pub fn raised_floor(&mut self, raised: &RaisedFloor) {
        let (min, max, height, floor) = (raised.min, raised.max, raised.height, raised.zone.floor());
        let size = Vec3::new(max.x - min.x, height - floor, max.y - min.y);
        let middle = (min + max) / 2.0;
        self.block(size, Vec3::new(middle.x, (floor + height) / 2.0, middle.y), raised.colour);
        self.edging(min, max, height, palette(Some(raised.zone)).trim);
    }

    /// A sunken floor: its floor a shade darker than its Zone's, its sides faced in the Zone's
    /// trim, and a coping round its rim if it has one.
    pub fn sunken(&mut self, sunken: &Sunken) {
        let (min, max, depth) = (sunken.min, sunken.max, sunken.depth);
        let colours = palette(Some(sunken.zone));
        let middle = (min + max) / 2.0;
        let size = Vec3::new(max.x - min.x, 0.2, max.y - min.y);
        self.block(size, Vec3::new(middle.x, -depth - 0.1, middle.y), darker(colours.floor));
        // Thin facings on its four sides, just inside the hole.
        let facing = 0.02;
        for (centre, size) in [
            (Vec2::new(middle.x, min.y + facing / 2.0), Vec2::new(max.x - min.x, facing)),
            (Vec2::new(middle.x, max.y - facing / 2.0), Vec2::new(max.x - min.x, facing)),
            (Vec2::new(min.x + facing / 2.0, middle.y), Vec2::new(facing, max.y - min.y)),
            (Vec2::new(max.x - facing / 2.0, middle.y), Vec2::new(facing, max.y - min.y)),
        ] {
            self.shape(Vec3::new(size.x, depth, size.y), Vec3::new(centre.x, -depth / 2.0, centre.y), Quat::IDENTITY, colours.trim);
        }
        if sunken.rim {
            self.edging(min, max, 0.0, colours.trim);
        }
    }

    /// A band of `colour` round the edge of `min`..`max` at height `top`: the lip of a Raised Floor,
    /// the coping of a sunken floor.
    fn edging(&mut self, min: Vec2, max: Vec2, top: f32, colour: Color) {
        let (wide, tall) = (0.12, 0.05);
        let middle = (min + max) / 2.0;
        let y = top - tall / 2.0 + 0.015;
        for (centre, size) in [
            (Vec2::new(middle.x, min.y), Vec2::new(max.x - min.x + wide, wide)),
            (Vec2::new(middle.x, max.y), Vec2::new(max.x - min.x + wide, wide)),
            (Vec2::new(min.x, middle.y), Vec2::new(wide, max.y - min.y + wide)),
            (Vec2::new(max.x, middle.y), Vec2::new(wide, max.y - min.y + wide)),
        ] {
            self.shape(Vec3::new(size.x, tall, size.y), Vec3::new(centre.x, y, centre.y), Quat::IDENTITY, colour);
        }
    }

    /// A Jump Pad's plate: a low steel disc in hazard paint, with a lamp-black ring, and an
    /// arrow on it pointing where it throws. Only the disc is solid, as low as a step.
    pub fn jump_pad(&mut self, pad: &JumpPad) {
        let plate = Vec3::new(JUMP_PAD_RADIUS * 2.0, JUMP_PAD_PLATE, JUMP_PAD_RADIUS * 2.0);
        let paint = self.paint(JUMP_PAD_PAINT);
        let rim = self.paint(DRUM_RING);
        self.commands.spawn((
            InRun,
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(rim),
            Transform::from_translation(pad.at + Vec3::Y * (JUMP_PAD_PLATE / 2.0 - 0.005)).with_scale(plate + Vec3::new(0.24, -0.01, 0.24)),
        ));
        self.commands.spawn((
            InRun,
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(paint),
            Transform::from_translation(pad.at + Vec3::Y * JUMP_PAD_PLATE / 2.0).with_scale(plate),
        ));
        self.bare(Collider::cylinder(JUMP_PAD_RADIUS, JUMP_PAD_PLATE), pad.at + Vec3::Y * JUMP_PAD_PLATE / 2.0, Quat::IDENTITY);
        let flat = Vec3::new(pad.to.x - pad.at.x, 0.0, pad.to.z - pad.at.z).normalize_or(Vec3::Z);
        let rotation = Quat::from_rotation_arc(Vec3::Z, flat);
        let top = pad.at + Vec3::Y * (JUMP_PAD_PLATE + 0.01);
        self.shape(Vec3::new(0.18, 0.02, 0.9), top - flat * 0.15, rotation, DRUM_RING);
        for side in [-1.0, 1.0] {
            let wing = rotation * Quat::from_rotation_y(side * 0.7);
            self.shape(Vec3::new(0.16, 0.02, 0.55), top + flat * 0.2 + rotation * Vec3::X * side * 0.14, wing, DRUM_RING);
        }
    }

    /// A flight of steps or a ramp. It's solid as one smooth ramp from its foot to its top, a slab
    /// whose top face runs through both; steps are drawn over it, each step's front astride that face.
    pub fn slope(&mut self, slope: &Slope) {
        let along = Vec3::new(slope.up.x, 0.0, slope.up.y);
        let length = (slope.rise * slope.rise + slope.run * slope.run).sqrt();
        let climb = (along * slope.run + Vec3::Y * slope.rise) / length;
        let normal = (Vec3::Y * slope.run - along * slope.rise) / length;
        let rotation = Quat::from_mat3(&Mat3::from_cols(normal.cross(climb), normal, climb));
        let size = Vec3::new(slope.width, SLAB, length);
        let centre = slope.foot + along * slope.run / 2.0 + Vec3::Y * slope.rise / 2.0 - normal * SLAB / 2.0;
        self.bare(Collider::cuboid(size.x, size.y, size.z), centre, rotation);
        if !slope.steps {
            self.shape(size, centre, rotation, slope.colour);
            return;
        }
        let count = (slope.rise / STEP_RISE).round().max(1.0) as usize;
        let tread = slope.run / count as f32;
        for i in 1..=count {
            let from = (i as f32 - 0.5) * tread;
            let top = slope.rise * i as f32 / count as f32;
            let reach = slope.run - from;
            let centre = slope.foot + along * (from + reach / 2.0) + Vec3::Y * top / 2.0;
            let size = if slope.up.x != 0.0 {
                Vec3::new(reach, top, slope.width)
            } else {
                Vec3::new(slope.width, top, reach)
            };
            self.shape(size, centre, Quat::IDENTITY, slope.colour);
        }
    }

    /// A Prop: solid through its whole box (round, for a drum) and drawn as its kind looks. `nth`
    /// varies the look between Props of a kind.
    pub fn prop(&mut self, prop: &Prop, nth: usize) {
        let Prop { kind, zone, at, size } = *prop;
        let middle = at + Vec3::Y * size.y / 2.0;
        let collider = match kind {
            PropKind::Barrel => Collider::cylinder(size.x / 2.0, size.y),
            _ => Collider::cuboid(size.x, size.y, size.z),
        };
        self.bare(collider, middle, Quat::IDENTITY);
        let flat = Quat::IDENTITY;
        // A box `size` across whose bottom is `up` above the Prop's foot, `off` along the floor.
        let piece = |this: &mut Self, size: Vec3, up: f32, off: Vec2, colour: Color| {
            this.shape(size, at + Vec3::new(off.x, up + size.y / 2.0, off.y), flat, colour);
        };
        // A band round the Prop, `tall`, its middle `up` above its foot.
        let band = |this: &mut Self, up: f32, tall: f32, colour: Color| {
            piece(this, Vec3::new(size.x + 0.03, tall, size.z + 0.03), up - tall / 2.0, Vec2::ZERO, colour);
        };
        match kind {
            PropKind::Crate => {
                let wood = CRATE_WOOD[nth % 2];
                piece(self, size, 0.0, Vec2::ZERO, wood);
                let batten = darker(wood);
                band(self, size.y - 0.04, 0.07, batten);
                band(self, 0.05, 0.07, batten);
                if size.y > 1.0 {
                    band(self, size.y / 2.0, 0.07, batten);
                }
            }
            PropKind::Table | PropKind::Bench => {
                let (wood, top) = if kind == PropKind::Table { (TABLE_WOOD, 0.06) } else { (BENCH_WOOD, 0.08) };
                piece(self, Vec3::new(size.x, top, size.z), size.y - top, Vec2::ZERO, wood);
                let leg = Vec3::new(0.09, size.y - top, 0.09);
                let (dx, dz) = (size.x / 2.0 - 0.1, size.z / 2.0 - 0.1);
                for (x, z) in [(-dx, -dz), (dx, -dz), (-dx, dz), (dx, dz)] {
                    piece(self, leg, 0.0, Vec2::new(x, z), darker(wood));
                }
                // A rail between the legs, so it reads as solid as it is.
                piece(self, Vec3::new(size.x - 0.2, 0.12, size.z - 0.2), size.y - top - 0.14, Vec2::ZERO, darker(wood));
            }
            PropKind::Counter => {
                piece(self, Vec3::new(size.x - 0.08, size.y - 0.05, size.z - 0.08), 0.0, Vec2::ZERO, COUNTER_WOOD);
                piece(self, Vec3::new(size.x, 0.05, size.z), size.y - 0.05, Vec2::ZERO, COUNTER_TOP);
            }
            PropKind::Altar => {
                piece(self, size, 0.0, Vec2::ZERO, ALTAR_STONE);
                // A cloth over its top, hanging down the long sides.
                let cloth = palette(Some(zone)).trim;
                piece(self, Vec3::new(size.x + 0.04, 0.03, size.z * 0.7), size.y, Vec2::ZERO, cloth);
                piece(self, Vec3::new(size.x + 0.04, size.y * 0.5, size.z * 0.3), size.y * 0.5, Vec2::ZERO, cloth);
            }
            PropKind::Pillar => {
                piece(self, size, 0.0, Vec2::ZERO, PILLAR_STONE);
                let trim = palette(Some(zone)).trim;
                for up in [0.0, size.y - 0.3] {
                    piece(self, Vec3::new(size.x + 0.16, 0.3, size.z + 0.16), up, Vec2::ZERO, trim);
                }
            }
            PropKind::Shelf => {
                piece(self, size, 0.0, Vec2::ZERO, SHELF_WOOD);
                for k in 1..4 {
                    band(self, size.y * k as f32 / 4.0, 0.04, SHELF_BOARD);
                }
            }
            PropKind::Rubble => {
                // A heap, then broken blocks leaning on it, turned this way and that.
                let turn = nth as f32 * 0.7;
                piece(self, Vec3::new(size.x * 0.92, size.y * 0.55, size.z * 0.92), 0.0, Vec2::ZERO, RUBBLE_STONE[0]);
                for (k, (scale, off, yaw)) in [
                    (Vec3::new(0.55, 1.0, 0.5), Vec2::new(-0.18, 0.15), 0.3),
                    (Vec3::new(0.5, 0.8, 0.45), Vec2::new(0.2, -0.2), -0.5),
                    (Vec3::new(0.4, 0.65, 0.6), Vec2::new(0.15, 0.25), 1.1),
                ]
                .into_iter()
                .enumerate()
                {
                    let chunk = size * scale;
                    let centre = at + Vec3::new(off.x * size.x, chunk.y / 2.0, off.y * size.z);
                    self.shape(chunk, centre, Quat::from_rotation_y(yaw + turn), RUBBLE_STONE[(k + nth) % 3]);
                }
            }
            PropKind::Sandbags => {
                // Courses of bags, each a little offset from the one below.
                let courses = (size.y / 0.21).round().max(1.0) as usize;
                let course = size.y / courses as f32;
                for k in 0..courses {
                    let shift = if k % 2 == 0 { 0.03 } else { -0.03 };
                    let off = if size.x > size.z { Vec2::new(shift, 0.0) } else { Vec2::new(0.0, shift) };
                    piece(self, Vec3::new(size.x - 0.06, course - 0.02, size.z - 0.04), k as f32 * course, off, SANDBAG[k % 2]);
                }
            }
            PropKind::Barrel => {
                let paint = self.paint(DRUM_PAINT[nth % 2]);
                self.commands.spawn((
                    InRun,
                    Mesh3d(self.cylinder.clone()),
                    MeshMaterial3d(paint),
                    Transform::from_translation(middle).with_scale(size),
                ));
                let ring = self.paint(DRUM_RING);
                for up in [size.y / 3.0, size.y * 2.0 / 3.0] {
                    self.commands.spawn((
                        InRun,
                        Mesh3d(self.cylinder.clone()),
                        MeshMaterial3d(ring.clone()),
                        Transform::from_translation(at + Vec3::Y * up).with_scale(Vec3::new(size.x + 0.03, 0.04, size.z + 0.03)),
                    ));
                }
            }
            PropKind::Machine => {
                piece(self, size, 0.0, Vec2::ZERO, MACHINE_STEEL);
                band(self, size.y * 0.7, 0.14, palette(Some(zone)).trim);
                piece(self, Vec3::new(size.x * 0.6, 0.12, size.z * 0.6), size.y, Vec2::ZERO, MACHINE_PANEL);
            }
            PropKind::Tree => {
                piece(self, size, 0.0, Vec2::ZERO, BARK);
                // The crown, drawn but not solid, so nothing catches on it.
                let turn = if nth % 2 == 0 { 1.0 } else { -1.0 };
                piece(self, Vec3::new(2.8, 1.4, 2.6), size.y - 0.5, Vec2::ZERO, LEAVES[nth % 2]);
                piece(self, Vec3::new(1.9, 1.0, 2.1), size.y + 0.8, Vec2::new(0.25 * turn, -0.2), LEAVES[(nth + 1) % 2]);
            }
            PropKind::Headstone => {
                // A slab with a narrower cap, leaning a little this way or that.
                let stone = HEADSTONE[nth % 3];
                let lean = Quat::from_rotation_x(((nth % 5) as f32 - 2.0) * 0.03);
                let slab = size.y - 0.12;
                self.shape(Vec3::new(size.x, slab, size.z), at + Vec3::Y * slab / 2.0, lean, stone);
                self.shape(Vec3::new(size.x * 0.7, 0.12, size.z), at + lean * (Vec3::Y * (slab + 0.06)), lean, stone);
            }
        }
    }

    /// A Door's slab: solid and drawn as one entity, which opening the Door moves and then hides.
    fn door_slab(&mut self, size: Vec3, at: Vec3, power: bool) -> Entity {
        let shape = Cuboid::from_size(size);
        let material = if power { self.power_door.clone() } else { self.door.clone() };
        self.commands
            .spawn((
                InRun,
                Mesh3d(self.meshes.add(shape)),
                MeshMaterial3d(material),
                Transform::from_translation(at),
                RigidBody::Static,
                Collider::from(shape),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            .id()
    }

    /// An axis-aligned wall from `a` to `b` (x, z), standing on `y0`, `height` tall, with gaps
    /// centred at offsets from `a`.
    pub fn wall(&mut self, a: Vec2, b: Vec2, y0: f32, height: f32, gaps: &[(f32, Gap)]) {
        let along_x = a.y == b.y;
        let length = (b - a).length();
        let dir = (b - a) / length;
        let mut cursor = 0.0;
        for (at, gap) in gaps {
            let (width, top) = match gap {
                Gap::Door(..) | Gap::LinkedDoor(_) => (DOOR_WIDTH, DOOR_HEIGHT),
                Gap::Open { width, height } => (*width, *height),
                Gap::Window => (WINDOW_WIDTH, WINDOW_HEIGHT),
            };
            let (s, e) = (at - width / 2.0, at + width / 2.0);
            self.piece(a, b, cursor, s, y0, 0.0, height);
            self.piece(a, b, s, e, y0, top, height);
            cursor = e;
            let mid = a + dir * *at;
            let across = |depth: f32, tall: f32| {
                if along_x { Vec3::new(e - s, tall, depth) } else { Vec3::new(depth, tall, e - s) }
            };
            if let Gap::Window = gap {
                // The Sill is wall like any other; the opening above it is solid to the Player only.
                self.piece(a, b, s, e, y0, 0.0, WINDOW_SILL_HEIGHT);
                let opening = WINDOW_HEIGHT - WINDOW_SILL_HEIGHT;
                let size = across(WALL_THICKNESS, opening);
                self.commands.spawn((
                    InRun,
                    Transform::from_xyz(mid.x, y0 + WINDOW_SILL_HEIGHT + opening / 2.0, mid.y),
                    RigidBody::Static,
                    Collider::cuboid(size.x, size.y, size.z),
                    CollisionLayers::new(GameLayer::Window, LayerMask::ALL),
                ));
            }
            if let Gap::Door(joins, price) = gap {
                let door = self.door_slab(across(0.25, DOOR_HEIGHT), Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y), price.is_none());
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
            if let Gap::LinkedDoor(joins) = gap {
                let at = Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y);
                let door = self.door_slab(across(0.25, DOOR_HEIGHT), at, true);
                // It can't be bought either, but its Prompt says what opens it, and the Use key
                // buzzes.
                self.commands.entity(door).insert((
                    Door {
                        joins: *joins,
                        price: None,
                        open: false,
                    },
                    OpensOnLinks,
                    Buyable {
                        use_point: at,
                        prompt: LINKED_DOOR_PROMPT.into(),
                        price: None,
                        range: DOOR_USE_RANGE,
                    },
                    Refused,
                    Visibility::default(),
                ));
            }
        }
        self.piece(a, b, cursor, length, y0, 0.0, height);
    }

    /// An outer wall from `a` to `b`, standing on `y0`, with a gap for each window on it at that
    /// height.
    pub fn outer_wall(&mut self, a: Vec2, b: Vec2, y0: f32, height: f32) {
        let length = (b - a).length();
        let mut gaps: Vec<_> = self
            .windows
            .iter()
            .filter(|(_, centre, _)| (centre.y - y0).abs() < 0.01)
            .map(|(_, centre, _)| Vec2::new(centre.x, centre.z))
            .filter(|&centre| (centre - a).perp_dot(b - a).abs() < 0.01)
            .map(|centre| ((centre - a).dot(b - a) / length, Gap::Window))
            .filter(|(at, _)| *at > 0.0 && *at < length)
            .collect();
        gaps.sort_by(|x, y| x.0.total_cmp(&y.0));
        self.wall(a, b, y0, height, &gaps);
    }

    /// A plain piece of the wall from `a` to `b`: solid from `s` to `e` along it and `lo` to `hi`
    /// above `y0`, with each face painted for the Zone it looks into.
    fn piece(&mut self, a: Vec2, b: Vec2, s: f32, e: f32, y0: f32, lo: f32, hi: f32) {
        if e - s < 0.01 || hi - lo < 0.01 {
            return;
        }
        let along_x = a.y == b.y;
        let mid = a + (b - a).normalize() * (s + e) / 2.0;
        let size = if along_x {
            Vec3::new(e - s, hi - lo, WALL_THICKNESS)
        } else {
            Vec3::new(WALL_THICKNESS, hi - lo, e - s)
        };
        self.bare(Collider::cuboid(size.x, size.y, size.z), Vec3::new(mid.x, y0 + (lo + hi) / 2.0, mid.y), Quat::IDENTITY);
        let line = if along_x { a.y } else { a.x };
        // Only a piece standing on the wall's foot stands on a floor.
        let foot = (lo == 0.0).then_some(y0);
        for face in wall_faces(a, b, s, e, y0 + lo, y0 + hi) {
            self.skin(&face, along_x, line, foot);
        }
    }

    /// Draws one face of a wall piece whose middle is on `line`: its half of the wall in the
    /// face's wall colour, with trim in front of it: skirting where it stands on a floor at
    /// `foot`, and a band along the top of each storey.
    fn skin(&mut self, face: &Face, along_x: bool, line: f32, foot: Option<f32>) {
        let colours = self.repaint.unwrap_or_else(|| palette(face.zone));
        let out = if along_x { face.normal.y } else { face.normal.x };
        let (from, to) = face.along;
        let slab = |this: &mut Self, off: f32, depth: f32, bottom: f32, top: f32, colour: Color| {
            let across = line + out * (off + depth / 2.0);
            let (mid, y) = ((from + to) / 2.0, (bottom + top) / 2.0);
            let (centre, size) = if along_x {
                (Vec3::new(mid, y, across), Vec3::new(to - from, top - bottom, depth))
            } else {
                (Vec3::new(across, y, mid), Vec3::new(depth, top - bottom, to - from))
            };
            this.shape(size, centre, Quat::IDENTITY, colour);
        };
        let half = WALL_THICKNESS / 2.0;
        let (bottom, top) = face.y;
        slab(self, 0.0, half, bottom, top, colours.wall);
        if foot == Some(bottom) && top - bottom > SKIRTING.x {
            slab(self, half, SKIRTING.y, bottom, bottom + SKIRTING.x, colours.trim);
        }
        let storey_top = top > 0.0 && ((top / STOREY).round() * STOREY - top).abs() < 1e-3;
        if storey_top && top - bottom >= 0.6 {
            slab(self, half, CORNICE.y, top - CORNICE.x, top, colours.trim);
        }
    }
}

/// One face of a wall piece, in the colours of the Zone it looks into (`None`: the outside).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Face {
    /// The way the face looks, out of the wall (x, z).
    pub normal: Vec2,
    /// Its span along the wall, as world x for a wall along x and world z for one along z.
    pub along: (f32, f32),
    /// Its bottom and top.
    pub y: (f32, f32),
    pub zone: Option<ZoneId>,
}

/// `colour`, a shade darker.
fn darker(colour: Color) -> Color {
    let c = colour.to_srgba();
    Color::srgb(c.red * 0.72, c.green * 0.72, c.blue * 0.72)
}

/// The rectangle `outer` (min, max) less the rectangles `holes`, as rectangles.
fn rects_around(outer: (Vec2, Vec2), holes: &[(Vec2, Vec2)]) -> Vec<(Vec2, Vec2)> {
    let mut rects = vec![outer];
    for &(hole_min, hole_max) in holes {
        rects = rects
            .into_iter()
            .flat_map(|(min, max)| {
                let (lo, hi) = (min.max(hole_min), max.min(hole_max));
                if lo.x >= hi.x || lo.y >= hi.y {
                    return vec![(min, max)];
                }
                // The strips south and north of the hole, full width, then west and east of it.
                [
                    (min, Vec2::new(max.x, lo.y)),
                    (Vec2::new(min.x, hi.y), max),
                    (Vec2::new(min.x, lo.y), Vec2::new(lo.x, hi.y)),
                    (Vec2::new(hi.x, lo.y), Vec2::new(max.x, hi.y)),
                ]
                .into_iter()
                .filter(|(a, b)| b.x - a.x > 0.01 && b.y - a.y > 0.01)
                .collect()
            })
            .collect();
    }
    rects
}

/// How far off a wall's face the Zone it looks into is sampled.
const FACE_SAMPLE_OUT: f32 = 0.3;

/// The faces of the piece of the wall from `a` to `b` (x, z) that spans `s`..`e` along it and
/// `y0`..`y1` up: one for each side, each storey, and each Zone along the way. A storey is
/// sampled a metre above its floor, so the top of a wall belongs to the room below it, and a
/// storey of wall above a one-storey room looks into the outside.
pub(super) fn wall_faces(a: Vec2, b: Vec2, s: f32, e: f32, y0: f32, y1: f32) -> Vec<Face> {
    let along_x = a.y == b.y;
    let dir = (b - a).normalize();
    let (p, q) = (a + dir * s, a + dir * e);
    let axis = |v: Vec2| if along_x { v.x } else { v.y };
    let (lo, hi) = (axis(p).min(axis(q)), axis(p).max(axis(q)));
    let line = if along_x { a.y } else { a.x };

    // Cut along the wall wherever a Zone starts or ends.
    let mut cuts = vec![lo, hi];
    for zone in ZoneId::ALL {
        let (zlo, zhi) = zone.volume();
        for cut in if along_x { [zlo.x, zhi.x] } else { [zlo.z, zhi.z] } {
            if cut > lo + 0.01 && cut < hi - 0.01 {
                cuts.push(cut);
            }
        }
    }
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() < 0.01);

    // Cut up it at each storey; anything below the ground floor belongs to it.
    let mut storeys = Vec::new();
    let mut bottom = y0;
    while bottom < y1 - 0.01 {
        let floor = (bottom.max(0.0) / super::STOREY).floor() * super::STOREY;
        let top = (floor + super::STOREY).min(y1);
        storeys.push((bottom, top, floor + 1.0));
        bottom = top;
    }

    let mut faces = Vec::new();
    let across = if along_x { Vec2::Y } else { Vec2::X };
    for normal in [across, -across] {
        for &(bottom, top, sample_y) in &storeys {
            let mut run: Option<Face> = None;
            for pair in cuts.windows(2) {
                let mid = (pair[0] + pair[1]) / 2.0;
                let at = if along_x { Vec2::new(mid, line) } else { Vec2::new(line, mid) } + normal * FACE_SAMPLE_OUT;
                let zone = crate::enemies::nav::zone_at(Vec3::new(at.x, sample_y, at.y));
                match &mut run {
                    Some(face) if face.zone == zone => face.along.1 = pair[1],
                    _ => {
                        faces.extend(run.take());
                        run = Some(Face { normal, along: (pair[0], pair[1]), y: (bottom, top), zone });
                    }
                }
            }
            faces.extend(run);
        }
    }
    faces
}

#[cfg(test)]
mod tests {
    use super::*;
    use ZoneId::*;

    /// What a wall piece's faces wear: the way each looks (x, z), its span along the wall and up
    /// it in decimetres, and its Zone, sorted.
    fn worn(a: Vec2, b: Vec2, s: f32, e: f32, y0: f32, y1: f32) -> Vec<((i32, i32), (i32, i32), (i32, i32), Option<ZoneId>)> {
        let dm = |v: f32| (v * 10.0).round() as i32;
        let mut faces: Vec<_> = wall_faces(a, b, s, e, y0, y1)
            .into_iter()
            .map(|f| ((dm(f.normal.x), dm(f.normal.y)), (dm(f.along.0), dm(f.along.1)), (dm(f.y.0), dm(f.y.1)), f.zone))
            .collect();
        faces.sort_by_key(|f| (f.0, f.1, f.2));
        faces
    }

    #[test]
    fn each_side_of_a_wall_wears_the_zone_it_looks_into() {
        // The wall between the Start Room and the Yard.
        let faces = worn(Vec2::new(-14.0, 14.0), Vec2::new(14.0, 14.0), 0.0, 28.0, 0.0, 4.0);
        assert_eq!(
            faces,
            [((0, -10), (-140, 140), (0, 40), Some(Yard)), ((0, 10), (-140, 140), (0, 40), Some(StartRoom))]
        );
    }

    #[test]
    fn a_face_changes_colour_where_the_zone_beyond_it_changes() {
        // The south wing's north wall, between the Passage and the Tunnel: the outside to the
        // north, and the three rooms of the wing side by side to the south.
        let faces = worn(Vec2::new(-34.0, -38.0), Vec2::new(34.0, -38.0), 23.5, 46.5, 0.0, 4.0);
        assert_eq!(
            faces,
            [
                ((0, -10), (-105, -40), (0, 40), Some(Workshop)),
                ((0, -10), (-40, 40), (0, 40), Some(Storeroom)),
                ((0, -10), (40, 125), (0, 40), Some(GeneratorRoom)),
                ((0, 10), (-105, 125), (0, 40), None),
            ]
        );
    }

    #[test]
    fn a_two_storey_wall_wears_each_storey_on_its_own() {
        // West Hall's two-storey wall to the Start Room: West Hall all the way up on its side, and
        // above the Start Room's storey the outside.
        let faces = worn(Vec2::new(-14.0, 14.0), Vec2::new(-14.0, 26.0), 0.0, 12.0, 0.0, 8.0);
        assert_eq!(
            faces,
            [
                ((-10, 0), (140, 260), (0, 40), Some(WestHall)),
                ((-10, 0), (140, 260), (40, 80), Some(WestHall)),
                ((10, 0), (140, 260), (0, 40), Some(StartRoom)),
                ((10, 0), (140, 260), (40, 80), None),
            ]
        );
    }
}
