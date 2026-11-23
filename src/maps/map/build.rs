//! How the Map's data becomes the world: solid walls each face painted for the Zone it looks
//! into, floors, and the boxes everything else is drawn with.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::economy::buy::{Buyable, Refused};
use crate::maps::door::{Door, LINKED_DOOR_PROMPT, OpensOnLinks};
use crate::layers::GameLayer;
use crate::state::InRun;
use crate::tuning::*;
use crate::maps::zone::ZoneId;

use super::decals::{Decal, DecalFamily, DecalPiece, Instead, leak_transform, leaks, stain_transform, stains};
use super::door_look::{self, Part, Piece};
use super::fitting::FittedModel;
use super::textures::{Material, Shape, Textured, Tint, Veneer, box_uv_mesh};
use crate::maps::machine_look::PowerGlow;
use super::{MapId, DOOR_HEIGHT, DOOR_WIDTH, Infill, Palette, Prop, PropKind, RaisedFloor, STOREY, Slope, Sunken, WALL_THICKNESS, WINDOW_WIDTH, materials, palette, prop_variant};
use crate::maps::generator::{Generator, PlateDisc};
use crate::maps::jump_pad::JumpPad;
use crate::maps::zipline::Zipline;

/// How tall the skirting along the foot of a wall is, and how far it stands out from the face.
const SKIRTING: Vec2 = Vec2::new(0.16, 0.03);
/// How tall the band along the top of each storey of wall is, and how far it stands out.
const CORNICE: Vec2 = Vec2::new(0.18, 0.04);
/// How high a step is drawn, about.
const STEP_RISE: f32 = 0.15;
/// A sunken floor shallower than this gets no facings on its sides: it is a recess, not a well.
const FACING_MIN_DEPTH: f32 = 0.3;
/// How thick the slab under a ramp or a flight of steps is.
const SLAB: f32 = 0.3;

/// What the Props are made of.
const CRATE_WOOD: [Color; 2] = [Color::srgb(0.46, 0.33, 0.18), Color::srgb(0.38, 0.27, 0.15)];
const TABLE_WOOD: Color = Color::srgb(0.3, 0.19, 0.11);
const BENCH_WOOD: Color = Color::srgb(0.34, 0.22, 0.13);
const COUNTER_WOOD: Color = Color::srgb(0.32, 0.2, 0.12);
const COUNTER_TOP: Color = Color::srgb(0.2, 0.13, 0.08);
/// How thick a Counter's top slab is. A unit model stands under it.
const COUNTER_SLAB: f32 = 0.05;
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
const GENERATOR_CONSOLE_PAINT: Color = Color::srgb(0.32, 0.36, 0.3);
const GENERATOR_PLATE_PAINT: Color = Color::srgb(0.75, 0.42, 0.08);
const JUMP_PAD_PAINT: Color = Color::srgb(0.86, 0.68, 0.1);

/// A gap in a wall, at an offset along it.
pub(super) enum Gap {
    /// A Door, joining two Zones; `None` price makes it a Power Door.
    Door(Option<(ZoneId, ZoneId)>, Option<u64>),
    /// The gate Door of a Swing Bridge, bought for the price: it joins nothing, but names the two
    /// Zones whose landings it guards.
    Gate((ZoneId, ZoneId), u64),
    /// The gate Door of a Lock Gate: a Power Door that joins nothing and names the two Zones whose
    /// landings it guards.
    PowerGate((ZoneId, ZoneId)),
    /// A Door that can't be bought and opens once every Teleporter Pad is Linked, like the
    /// Gazebo's.
    LinkedDoor(Option<(ZoneId, ZoneId)>),
    /// An open archway.
    Open { width: f32, height: f32 },
    /// A window in an outer wall, open to Zombies and solid to the Player.
    Window,
}

/// Whether `zone`'s volume reaches down to the bottom of `hole`, a well in its floor. Every Zone's
/// does, but the Sanatorium's two stacked over a ground Zone, whose wells stand in bays outside
/// every Zone's box.
fn reaches_down(zone: ZoneId, hole: &Sunken) -> bool {
    zone.volume().0.y < zone.floor() - hole.depth + 0.5
}

/// A stair tread's nosing: how deep (along the climb) and how thick.
const NOSING: Vec2 = Vec2::new(0.05, 0.03);

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
    /// The material walls wear while `repaint` is set, if any.
    pub repaint_wall: Option<Material>,
    /// The material the outer faces of walls (the ones looking out, into no Zone) wear, if any.
    pub outside_wall: Option<Material>,
    /// The material the ground outside the walls wears, if any.
    pub outside_floor: Option<Material>,
    /// The Map's windows, which its outer walls leave gaps for.
    windows: &'static [(ZoneId, Vec3, Vec2)],
    /// The Map being built: its Zones are the ones a wall's faces look into.
    map: MapId,
    /// Every piece drawn since a Prop began: what its unit model replaces.
    drawn: Vec<Entity>,
}

impl<'a, 'w, 's> Builder<'a, 'w, 's> {
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        meshes: &'a mut Assets<Mesh>,
        materials: &'a mut Assets<StandardMaterial>,
        windows: &'static [(ZoneId, Vec3, Vec2)],
        map: MapId,
    ) -> Self {
        let cube = meshes.add(Cuboid::from_length(1.0));
        let cylinder = meshes.add(Cylinder::new(0.5, 1.0));
        let door = materials.add(Color::srgb(0.85, 0.45, 0.1));
        let power_door = materials.add(Color::srgb(0.5, 0.3, 0.8));
        Self { commands, meshes, materials, cube, cylinder, paints: HashMap::new(), door, power_door, repaint: None, repaint_wall: None, outside_wall: None, outside_floor: None, windows, map, drawn: Vec::new() }
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
    pub fn shape(&mut self, size: Vec3, at: Vec3, rotation: Quat, colour: Color) -> Entity {
        self.shape_in(size, at, rotation, colour, None)
    }

    /// A box drawn in `colour` until `material` (if any) has loaded, then in that.
    pub fn shape_in(&mut self, size: Vec3, at: Vec3, rotation: Quat, colour: Color, material: Option<Material>) -> Entity {
        let paint = self.paint(colour);
        let mut shape = self.commands.spawn((
            InRun,
            Mesh3d(self.cube.clone()),
            MeshMaterial3d(paint),
            Transform { translation: at, rotation, scale: size },
        ));
        if let Some(material) = material {
            shape.insert(Textured { material, shape: Shape::Cuboid });
        }
        let shape = shape.id();
        self.drawn.push(shape);
        shape
    }

    /// A box that glows: `colour` lit from within at `strength`, with no shadow and not solid.
    pub fn glow_box(&mut self, size: Vec3, at: Vec3, colour: Color, strength: f32) -> Entity {
        let material = self.materials.add(StandardMaterial { base_color: colour, emissive: LinearRgba::from(colour) * strength, ..default() });
        let shape = self.commands.spawn((InRun, Mesh3d(self.cube.clone()), MeshMaterial3d(material), Transform { translation: at, rotation: Quat::IDENTITY, scale: size }, NotShadowCaster)).id();
        self.drawn.push(shape);
        shape
    }

    /// An upright drum (a cylinder `size.x` across and `size.y` tall) drawn in `colour`. Not solid.
    pub fn drum(&mut self, size: Vec3, at: Vec3, colour: Color) -> Entity {
        let paint = self.paint(colour);
        let shape = self.commands.spawn((InRun, Mesh3d(self.cylinder.clone()), MeshMaterial3d(paint), Transform { translation: at, rotation: Quat::IDENTITY, scale: size })).id();
        self.drawn.push(shape);
        shape
    }

    /// Adds `bundle` to an entity the Builder spawned.
    pub fn tag(&mut self, entity: Entity, bundle: impl Bundle) {
        self.commands.entity(entity).insert(bundle);
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

    /// An invisible box that stops the Player only: Zombies and bullets pass through it, as at a
    /// window's opening.
    pub fn player_wall(&mut self, size: Vec3, at: Vec3) {
        self.commands.spawn((
            InRun,
            Transform::from_translation(at),
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            CollisionLayers::new(GameLayer::Window, LayerMask::ALL),
        ));
    }

    /// A solid box drawn in `colour`.
    pub fn block(&mut self, size: Vec3, at: Vec3, colour: Color) -> Entity {
        self.block_in(size, at, colour, None)
    }

    /// A solid box drawn in `colour` until `material` (if any) has loaded, then in that.
    pub fn block_in(&mut self, size: Vec3, at: Vec3, colour: Color, material: Option<Material>) -> Entity {
        self.shape_in(size, at, Quat::IDENTITY, colour, material);
        self.bare(Collider::cuboid(size.x, size.y, size.z), at, Quat::IDENTITY)
    }

    /// A solid slab of the ground outside the walls, in the Map's outside floor.
    pub fn ground(&mut self, size: Vec3, at: Vec3) -> Entity {
        self.block_in(size, at, palette(None).floor, self.outside_floor)
    }

    /// A solid mass of the outside's walls (a pocket's parapet, a ledge's rock), in the Map's outside wall.
    pub fn outer(&mut self, size: Vec3, at: Vec3) -> Entity {
        self.block_in(size, at, palette(None).wall, self.outside_wall)
    }

    /// A Zone's floor at its height, round any sunken floors in it, and deep enough that their
    /// edges are solid all the way down.
    pub fn floor(&mut self, zone: ZoneId, holes: &[Sunken]) {
        let (lo, hi) = zone.volume();
        let height = zone.floor();
        // A well the Zone's volume reaches down into has a floor solid all the way down its sides;
        // one cut in the floor of a Zone stacked over another (the Sanatorium's stairs) doesn't.
        let deepest = holes.iter().filter(|hole| reaches_down(zone, hole)).map(|hole| hole.depth).fold(0.0, f32::max);
        let thick = 0.2 + deepest;
        let colour = palette(Some(zone)).floor;
        let set = materials(Some(zone));
        let material = set.map(|set| set.floor);
        let cut: Vec<_> = holes.iter().map(|hole| (hole.min, hole.max)).collect();
        for (min, max) in rects_around((Vec2::new(lo.x, lo.z), Vec2::new(hi.x, hi.z)), &cut) {
            let size = Vec3::new(max.x - min.x, thick, max.y - min.y);
            let centre = Vec3::new((min.x + max.x) / 2.0, height - thick / 2.0, (min.y + max.y) / 2.0);
            self.block_in(size, centre, colour, material);
            if set.is_some_and(|set| set.decals == Some(DecalFamily::OilStains)) {
                for stain in stains(min, max) {
                    self.commands.spawn((InRun, DecalPiece(stain.decal), stain_transform(&stain, height)));
                }
            }
        }
    }

    /// An indoor Zone's ceiling: a slab at the top of its walls, in its wall material. Only drawn,
    /// so nothing in the Zone (a Jump Pad's arc, a Vent's drop) can bump it. It spans the Zone's
    /// volume, so neighbours' slabs meet over the middle of the wall between them, never overlap.
    pub fn ceiling(&mut self, zone: ZoneId, holes: &[Sunken]) {
        const THICKNESS: f32 = 0.2;
        let (lo, hi) = zone.volume();
        let top = (hi.y / STOREY).ceil() * STOREY;
        let material = materials(Some(zone)).map(|set| set.wall);
        // Open over water and stairwells a storey deep or more, which the roof leaves to the sky.
        let cut: Vec<_> = holes.iter().filter(|hole| hole.zone == zone && hole.depth >= 1.5 && reaches_down(zone, hole)).map(|hole| (hole.min, hole.max)).collect();
        for (min, max) in rects_around((Vec2::new(lo.x, lo.z), Vec2::new(hi.x, hi.z)), &cut) {
            let size = Vec3::new(max.x - min.x, THICKNESS, max.y - min.y);
            let centre = Vec3::new((min.x + max.x) / 2.0, top + THICKNESS / 2.0, (min.y + max.y) / 2.0);
            self.shape_in(size, centre, Quat::IDENTITY, palette(Some(zone)).wall, material);
        }
    }

    /// A Raised Floor, with a lip of its Zone's trim round its top.
    pub fn raised_floor(&mut self, raised: &RaisedFloor) {
        let (min, max, height, floor) = (raised.min, raised.max, raised.height, raised.zone.floor());
        let size = Vec3::new(max.x - min.x, height - floor, max.y - min.y);
        let middle = (min + max) / 2.0;
        let material = materials(Some(raised.zone)).map(|set| set.floor);
        self.block_in(size, Vec3::new(middle.x, (floor + height) / 2.0, middle.y), raised.colour, material);
        self.edging(min, max, height, palette(Some(raised.zone)).trim);
    }

    /// An Infill: solid from its Zone's floor to the top of its walls, in the Zone's wall colour
    /// (or wall material), with the Zone's skirting round its foot and its cornice round its top,
    /// as the walls have. The trim runs on into the walls it stands against, out of sight.
    pub fn infill(&mut self, infill: &Infill) {
        let (bottom, top) = infill.span();
        let (min, max) = (infill.min, infill.max);
        let middle = (min + max) / 2.0;
        let size = Vec3::new(max.x - min.x, top - bottom, max.y - min.y);
        let colours = palette(Some(infill.zone));
        let material = materials(Some(infill.zone)).map(|set| set.wall);
        self.block_in(size, Vec3::new(middle.x, (bottom + top) / 2.0, middle.y), colours.wall, material);
        for (tall, out, up) in [(SKIRTING.x, SKIRTING.y, bottom), (CORNICE.x, CORNICE.y, top - CORNICE.x)] {
            let band = Vec3::new(size.x + 2.0 * out, tall, size.z + 2.0 * out);
            self.shape(band, Vec3::new(middle.x, up + tall / 2.0, middle.y), Quat::IDENTITY, colours.trim);
        }
    }

    /// A sunken floor: its floor a shade darker than its Zone's (or in the Zone's floor material),
    /// its sides faced in the Zone's trim, and a coping round its rim if it has one.
    pub fn sunken(&mut self, sunken: &Sunken) {
        let (min, max, depth) = (sunken.min, sunken.max, sunken.depth);
        // Its Zone's floor is the top: the ground, or a storey up where a Zone owns its stairs' foot.
        let top = sunken.zone.floor();
        let colours = palette(Some(sunken.zone));
        let middle = (min + max) / 2.0;
        let size = Vec3::new(max.x - min.x, 0.2, max.y - min.y);
        let material = materials(Some(sunken.zone)).map(|set| set.floor);
        self.block_in(size, Vec3::new(middle.x, top - depth - 0.1, middle.y), darker(colours.floor), material);
        // Thin facings on its sides, just inside the hole. A side on its Zone's own edge has a
        // wall (and a Door, for a flight's foot) standing there, so it gets none: a facing in a
        // doorway would stay drawn, with nothing solid, once the Door had opened.
        let facing = 0.02;
        let (zone_min, zone_max) = sunken.zone.volume();
        let on_edge = |side: f32, edge: f32| (side - edge).abs() < 0.05;
        for (centre, size, skip) in [
            (Vec2::new(middle.x, min.y + facing / 2.0), Vec2::new(max.x - min.x, facing), on_edge(min.y, zone_min.z)),
            (Vec2::new(middle.x, max.y - facing / 2.0), Vec2::new(max.x - min.x, facing), on_edge(max.y, zone_max.z)),
            (Vec2::new(min.x + facing / 2.0, middle.y), Vec2::new(facing, max.y - min.y), on_edge(min.x, zone_min.x)),
            (Vec2::new(max.x - facing / 2.0, middle.y), Vec2::new(facing, max.y - min.y), on_edge(max.x, zone_max.x)),
        ] {
            // A recess no deeper than a step (a pool's bed) is faced by the floor's own side.
            if skip || depth < FACING_MIN_DEPTH {
                continue;
            }
            self.shape(Vec3::new(size.x, depth, size.y), Vec3::new(centre.x, top - depth / 2.0, centre.y), Quat::IDENTITY, colours.trim);
        }
        if sunken.rim {
            self.edging(min, max, top, colours.trim);
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
        // Dark riveted steel for the rim, hazard-painted rusty steel for the disc.
        self.commands.spawn((
            InRun,
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(rim),
            Textured { material: Material::MetalPlate02, shape: Shape::Cylinder },
            Transform::from_translation(pad.at + Vec3::Y * (JUMP_PAD_PLATE / 2.0 - 0.005)).with_scale(plate + Vec3::new(0.24, -0.01, 0.24)),
        ));
        self.commands.spawn((
            InRun,
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(paint),
            Textured { material: Material::RustyMetal02, shape: Shape::Cylinder },
            Tint(JUMP_PAD_PAINT),
            Transform::from_translation(pad.at + Vec3::Y * JUMP_PAD_PLATE / 2.0).with_scale(plate),
        ));
        self.bare(Collider::cylinder(JUMP_PAD_RADIUS, JUMP_PAD_PLATE), pad.at + Vec3::Y * JUMP_PAD_PLATE / 2.0, Quat::IDENTITY);
        let flat = Vec3::new(pad.to.x - pad.at.x, 0.0, pad.to.z - pad.at.z).normalize_or(Vec3::Z);
        let rotation = Quat::from_rotation_arc(Vec3::Z, flat);
        let top = pad.at + Vec3::Y * (JUMP_PAD_PLATE + 0.01);
        // The cue: the arrow glows once the Power is on, which is when the pad throws.
        let glow = self.materials.add(StandardMaterial {
            base_color: DRUM_RING,
            emissive: LinearRgba::BLACK,
            perceptual_roughness: 0.9,
            ..default()
        });
        let lit = Color::srgb(1.0, 0.8, 0.15);
        let mut arrows = vec![(Vec3::new(0.18, 0.02, 0.9), top - flat * 0.15, rotation)];
        for side in [-1.0, 1.0] {
            let wing = rotation * Quat::from_rotation_y(side * 0.7);
            arrows.push((Vec3::new(0.16, 0.02, 0.55), top + flat * 0.2 + rotation * Vec3::X * side * 0.14, wing));
        }
        for (size, at, rotation) in arrows {
            self.commands.spawn((
                InRun,
                Mesh3d(self.cube.clone()),
                MeshMaterial3d(glow.clone()),
                PowerGlow { material: glow.clone(), off: LinearRgba::BLACK, on: LinearRgba::from(lit) * 3.0, light: None },
                Transform { translation: at, rotation, scale: size },
            ));
        }
    }

    /// A Zipline: a pulley post behind its top and a buffer post past its foot, both timber and
    /// solid, a dark cable between the two heads, and a T-bar trolley hanging at the top. The
    /// posts stand clear of the line, a metre either side of where a ride starts and ends.
    pub fn zipline(&mut self, line: &Zipline) {
        let timber = Color::srgb(0.30, 0.20, 0.12);
        let iron = Color::srgb(0.07, 0.07, 0.08);
        let heading = line.heading();
        // The posts' heads are the cable's ends, and each post's foot is on its floor.
        let (top, bottom) = line.cable();
        let pulley = top - heading * 0.6;
        let buffer = bottom + heading * 0.6;
        for (head, floor) in [(pulley, line.from.y), (buffer, line.to.y)] {
            let height = head.y + 0.25 - floor;
            self.block(Vec3::new(0.24, height, 0.24), Vec3::new(head.x, floor + height / 2.0, head.z), timber);
        }
        // The pulley wheel on top of its post, and the buffer's pad on the side facing the cable.
        let across = Quat::from_rotation_arc(Vec3::Z, if heading == Vec3::ZERO { Vec3::Z } else { heading });
        self.shape(Vec3::new(0.1, 0.34, 0.34), pulley + Vec3::Y * 0.1, across, iron);
        self.shape(Vec3::new(0.34, 0.3, 0.14), buffer - heading * 0.17 - Vec3::Y * 0.2, across, iron);
        // The cable runs from the pulley's head to the buffer's, a thin dark cylinder.
        let (from, to) = (pulley + Vec3::Y * 0.1 + heading * 0.0, buffer - heading * 0.2);
        let span = to - from;
        let paint = self.paint(iron);
        self.commands.spawn((
            InRun,
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(paint),
            Transform::from_translation((from + to) / 2.0)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, span.normalize_or(Vec3::Y)))
                .with_scale(Vec3::new(ZIPLINE_CABLE_THICKNESS, span.length(), ZIPLINE_CABLE_THICKNESS)),
        ));
        // The trolley: two wheels on the cable, a stem and the bar a rider holds, hung where a ride
        // starts.
        let trolley = line.from + Vec3::Y * ZIPLINE_HANG;
        self.shape(Vec3::new(0.12, 0.1, 0.3), trolley + Vec3::Y * 0.02, across, iron);
        self.shape(Vec3::new(0.04, 0.3, 0.04), trolley - Vec3::Y * 0.15, across, iron);
        self.shape(Vec3::new(0.5, 0.05, 0.05), trolley - Vec3::Y * 0.3, across, iron);
    }

    /// A Generator: its console, solid, at the back of its plate, and the plate drawn on the floor
    /// in front of it, flush enough to walk over.
    pub fn generator(&mut self, index: usize, generator: &Generator) {
        let (foot, size) = generator.console();
        self.block_in(size, foot + Vec3::Y * size.y / 2.0, GENERATOR_CONSOLE_PAINT, Some(Material::GreenMetalRust));
        let face = foot + generator.facing * (GENERATOR_CONSOLE.z / 2.0 + 0.01);
        let facing = Quat::from_rotation_arc(Vec3::Z, generator.facing);
        self.shape(Vec3::new(0.9, 0.5, 0.02), face + Vec3::Y * GENERATOR_DIAL_HEIGHT, facing, DRUM_RING);
        // The lit panel: an amber readout set in that bezel, always glowing.
        let screen = self.materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.7, 0.2),
            emissive: LinearRgba::rgb(2.4, 1.3, 0.3),
            ..default()
        });
        self.commands.spawn((
            InRun,
            Mesh3d(self.cube.clone()),
            MeshMaterial3d(screen),
            Transform { translation: face + generator.facing * 0.012 + Vec3::Y * GENERATOR_DIAL_HEIGHT, rotation: facing, scale: Vec3::new(0.76, 0.36, 0.02) },
        ));
        let plate = Vec3::new(GENERATOR_PLATE_RADIUS * 2.0, 0.02, GENERATOR_PLATE_RADIUS * 2.0);
        let rim = self.paint(DRUM_RING);
        self.commands.spawn((
            InRun,
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(rim),
            Transform::from_translation(generator.at + Vec3::Y * 0.005).with_scale(plate + Vec3::new(0.2, 0.0, 0.2)),
        ));
        // Its own material, so it can glow without lighting the other plates.
        let paint = self.materials.add(StandardMaterial {
            base_color: GENERATOR_PLATE_PAINT,
            perceptual_roughness: 0.9,
            ..default()
        });
        self.commands.spawn((
            InRun,
            PlateDisc { index, material: paint.clone(), lit: false },
            Mesh3d(self.cylinder.clone()),
            MeshMaterial3d(paint),
            Transform::from_translation(generator.at + Vec3::Y * 0.01).with_scale(plate),
        ));
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
        // Treads and ramps wear their Zone's floor, with a nosing of its trim on each tread.
        let material = materials(Some(slope.zone)).map(|set| set.floor);
        if !slope.steps {
            self.shape_in(size, centre, rotation, slope.colour, material);
            return;
        }
        let trim = palette(Some(slope.zone)).trim;
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
            self.shape_in(size, centre, Quat::IDENTITY, slope.colour, material);
            // The nosing: a thin strip of trim along the tread's front edge.
            let front = slope.foot + along * (from + NOSING.x / 2.0) + Vec3::Y * (top + NOSING.y / 2.0 - 0.01);
            let strip = if slope.up.x != 0.0 { Vec3::new(NOSING.x, NOSING.y, slope.width) } else { Vec3::new(slope.width, NOSING.y, NOSING.x) };
            self.shape(strip, front, Quat::IDENTITY, trim);
        }
    }

    /// A Prop: solid through its whole box (round, for a drum) and drawn as its kind looks, in the
    /// material its Zone's set names for it (`nth` picks between the variants, and varies the look
    /// between Props of a kind). A variant with a unit model lays it over the box once it has loaded.
    pub fn prop(&mut self, prop: &Prop, nth: usize) {
        let Prop { kind, zone, at, size } = *prop;
        let middle = at + Vec3::Y * size.y / 2.0;
        let collider = match kind {
            PropKind::Barrel => Collider::cylinder(size.x / 2.0, size.y),
            _ => Collider::cuboid(size.x, size.y, size.z),
        };
        let solid = self.bare(collider, middle, Quat::IDENTITY);
        self.drawn.clear();
        // What a unit model covers: all of the Prop, but a Counter's top slab, which stays code-built.
        let covered = if kind == PropKind::Counter { Vec3::new(size.x, size.y - COUNTER_SLAB, size.z) } else { size };
        let variant = prop_variant(zone, kind, nth, covered);
        let mat = variant.map(|variant| variant.material);
        let mut kept = None;
        let flat = Quat::IDENTITY;
        // A box `size` across whose bottom is `up` above the Prop's foot, `off` along the floor.
        let piece = |this: &mut Self, size: Vec3, up: f32, off: Vec2, colour: Color, material: Option<Material>| {
            this.shape_in(size, at + Vec3::new(off.x, up + size.y / 2.0, off.y), flat, colour, material)
        };
        // A band round the Prop, `tall`, its middle `up` above its foot.
        let band = |this: &mut Self, up: f32, tall: f32, colour: Color| {
            piece(this, Vec3::new(size.x + 0.03, tall, size.z + 0.03), up - tall / 2.0, Vec2::ZERO, colour, None);
        };
        match kind {
            PropKind::Crate => {
                let wood = CRATE_WOOD[nth % 2];
                piece(self, size, 0.0, Vec2::ZERO, wood, mat);
                let batten = darker(wood);
                band(self, size.y - 0.04, 0.07, batten);
                band(self, 0.05, 0.07, batten);
                if size.y > 1.0 {
                    band(self, size.y / 2.0, 0.07, batten);
                }
            }
            PropKind::Table | PropKind::Bench => {
                let (wood, top) = if kind == PropKind::Table { (TABLE_WOOD, 0.06) } else { (BENCH_WOOD, 0.08) };
                piece(self, Vec3::new(size.x, top, size.z), size.y - top, Vec2::ZERO, wood, mat);
                let leg = Vec3::new(0.09, size.y - top, 0.09);
                let (dx, dz) = (size.x / 2.0 - 0.1, size.z / 2.0 - 0.1);
                for (x, z) in [(-dx, -dz), (dx, -dz), (-dx, dz), (dx, dz)] {
                    piece(self, leg, 0.0, Vec2::new(x, z), darker(wood), mat);
                }
                // A rail between the legs, so it reads as solid as it is.
                piece(self, Vec3::new(size.x - 0.2, 0.12, size.z - 0.2), size.y - top - 0.14, Vec2::ZERO, darker(wood), mat);
            }
            PropKind::Counter => {
                piece(self, Vec3::new(size.x - 0.08, size.y - COUNTER_SLAB, size.z - 0.08), 0.0, Vec2::ZERO, COUNTER_WOOD, mat);
                kept = Some(piece(self, Vec3::new(size.x, COUNTER_SLAB, size.z), size.y - COUNTER_SLAB, Vec2::ZERO, COUNTER_TOP, None));
            }
            PropKind::Altar => {
                piece(self, size, 0.0, Vec2::ZERO, ALTAR_STONE, mat);
                // A cloth over its top, hanging down the long sides.
                let cloth = palette(Some(zone)).trim;
                let fabric = Some(Material::FabricPattern07);
                piece(self, Vec3::new(size.x + 0.04, 0.03, size.z * 0.7), size.y, Vec2::ZERO, cloth, fabric);
                piece(self, Vec3::new(size.x + 0.04, size.y * 0.5, size.z * 0.3), size.y * 0.5, Vec2::ZERO, cloth, fabric);
            }
            PropKind::Pillar => {
                piece(self, size, 0.0, Vec2::ZERO, PILLAR_STONE, mat);
                let trim = palette(Some(zone)).trim;
                for up in [0.0, size.y - 0.3] {
                    piece(self, Vec3::new(size.x + 0.16, 0.3, size.z + 0.16), up, Vec2::ZERO, trim, None);
                }
            }
            PropKind::Shelf => {
                piece(self, size, 0.0, Vec2::ZERO, SHELF_WOOD, mat);
                for k in 1..4 {
                    band(self, size.y * k as f32 / 4.0, 0.04, SHELF_BOARD);
                }
            }
            PropKind::Rubble => {
                // A heap, then broken blocks leaning on it, turned this way and that.
                let turn = nth as f32 * 0.7;
                piece(self, Vec3::new(size.x * 0.92, size.y * 0.55, size.z * 0.92), 0.0, Vec2::ZERO, RUBBLE_STONE[0], mat);
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
                    self.shape_in(chunk, centre, Quat::from_rotation_y(yaw + turn), RUBBLE_STONE[(k + nth) % 3], mat);
                }
            }
            PropKind::Sandbags => {
                // Courses of bags, each a little offset from the one below.
                let courses = (size.y / 0.21).round().max(1.0) as usize;
                let course = size.y / courses as f32;
                for k in 0..courses {
                    let shift = if k % 2 == 0 { 0.03 } else { -0.03 };
                    let off = if size.x > size.z { Vec2::new(shift, 0.0) } else { Vec2::new(0.0, shift) };
                    piece(self, Vec3::new(size.x - 0.06, course - 0.02, size.z - 0.04), k as f32 * course, off, SANDBAG[k % 2], mat);
                }
            }
            PropKind::Barrel => {
                let paint = self.paint(DRUM_PAINT[nth % 2]);
                let mut drum = self.commands.spawn((
                    InRun,
                    Mesh3d(self.cylinder.clone()),
                    MeshMaterial3d(paint),
                    Transform::from_translation(middle).with_scale(size),
                ));
                if let Some(material) = mat {
                    drum.insert(Textured { material, shape: Shape::Cylinder });
                }
                let drum = drum.id();
                self.drawn.push(drum);
                let ring = self.paint(DRUM_RING);
                for up in [size.y / 3.0, size.y * 2.0 / 3.0] {
                    let ring = self
                        .commands
                        .spawn((
                            InRun,
                            Mesh3d(self.cylinder.clone()),
                            MeshMaterial3d(ring.clone()),
                            Transform::from_translation(at + Vec3::Y * up).with_scale(Vec3::new(size.x + 0.03, 0.04, size.z + 0.03)),
                        ))
                        .id();
                    self.drawn.push(ring);
                }
            }
            PropKind::Machine => {
                piece(self, size, 0.0, Vec2::ZERO, MACHINE_STEEL, mat);
                band(self, size.y * 0.7, 0.14, palette(Some(zone)).trim);
                piece(self, Vec3::new(size.x * 0.6, 0.12, size.z * 0.6), size.y, Vec2::ZERO, MACHINE_PANEL, mat);
            }
            PropKind::Tree => {
                piece(self, size, 0.0, Vec2::ZERO, BARK, mat);
                // The crown, drawn but not solid, so nothing catches on it: two boxes of leaves
                // until its alpha cards (crossed, three to a clump) have loaded to replace them.
                let turn = if nth.is_multiple_of(2) { 1.0 } else { -1.0 };
                let clumps = [
                    (Vec3::new(2.8, 1.4, 2.6), Vec2::ZERO, size.y - 0.5, LEAVES[nth % 2]),
                    (Vec3::new(1.9, 1.0, 2.1), Vec2::new(0.25 * turn, -0.2), size.y + 0.8, LEAVES[(nth + 1) % 2]),
                ];
                let crown: Vec<Entity> = clumps.iter().map(|&(clump, off, up, leaves)| piece(self, clump, up, off, leaves, None)).collect();
                for (clump, off, up, _) in clumps {
                    let middle = at + Vec3::new(off.x, up + clump.y / 2.0, off.y);
                    for card in 0..3 {
                        let yaw = (card as f32 / 3.0 + nth as f32 * 0.11) * std::f32::consts::PI;
                        self.commands.spawn((
                            InRun,
                            DecalPiece(Decal::Foliage),
                            Instead(crown.clone()),
                            Transform { translation: middle, rotation: Quat::from_rotation_y(yaw), scale: Vec3::new(clump.x * 1.25, clump.y * 1.5, 1.0) },
                        ));
                    }
                }
            }
            PropKind::Headstone => {
                // A slab with a narrower cap, leaning a little this way or that.
                let stone = HEADSTONE[nth % 3];
                let lean = Quat::from_rotation_x(((nth % 5) as f32 - 2.0) * 0.03);
                let slab = size.y - 0.12;
                self.shape_in(Vec3::new(size.x, slab, size.z), at + Vec3::Y * slab / 2.0, lean, stone, mat);
                self.shape_in(Vec3::new(size.x * 0.7, 0.12, size.z), at + lean * (Vec3::Y * (slab + 0.06)), lean, stone, mat);
            }
        }
        if let Some((path, unit)) = variant.and_then(|variant| variant.model) {
            let mut pieces = std::mem::take(&mut self.drawn);
            pieces.retain(|piece| Some(*piece) != kept);
            self.commands.entity(solid).insert(FittedModel { path, unit, at, size: covered, pieces });
        }
    }

    /// A Door's slab: solid and drawn as one entity, which opening the Door moves and then hides.
    /// Once its material has loaded it wears it (weathered wood, or a steel shutter), and its
    /// boards, frame and ribs (children, so they move and hide with it) appear.
    fn door_slab(&mut self, size: Vec3, at: Vec3, power: bool, zone: Option<ZoneId>) -> Entity {
        let shape = Cuboid::from_size(size);
        let along_x = size.x > size.z;
        let (material, cover) = if power {
            (self.power_door.clone(), Material::RustyCorrugatedIron)
        } else {
            (self.door.clone(), Material::WeatheredPlanks)
        };
        let door = self
            .commands
            .spawn((
                InRun,
                Mesh3d(self.meshes.add(box_uv_mesh(size, at, Quat::IDENTITY, cover.tile()))),
                MeshMaterial3d(material),
                Textured { material: cover, shape: Shape::Baked },
                Transform::from_translation(at),
                RigidBody::Static,
                Collider::from(shape),
                CollisionLayers::new(GameLayer::World, LayerMask::ALL),
            ))
            .id();
        // The Door's own frame: x along the wall, z through it.
        let local = if along_x { size } else { Vec3::new(size.z, size.y, size.x) };
        let trim = palette(zone).trim;
        let (pieces, frame) = if power {
            (door_look::shutter(local), Some(Color::srgb(0.2, 0.21, 0.23)))
        } else {
            (door_look::boarded(local), Some(trim))
        };
        for piece in pieces {
            let child = self.veneer(&piece, at, along_x, cover, if piece.part == Part::Frame { frame } else { None }, false);
            self.commands.entity(door).add_child(child);
        }
        door
    }

    /// One piece of a Door's or window's look, centred `piece.at` from `centre` in the frame whose
    /// x runs along the wall (the world's x if `along_x`, else its z). It is hidden until `cover`
    /// has loaded. In `paint` if it has one (trim), else in `cover` itself; at `centre + piece.at` in
    /// the world if `world`, else relative to its parent.
    fn veneer(&mut self, piece: &Piece, centre: Vec3, along_x: bool, cover: Material, paint: Option<Color>, world: bool) -> Entity {
        // On a wall along z the frame is turned: the tilt is about the world's x, through the wall.
        let (size, at, rotation) = if along_x {
            (piece.size, piece.at, Quat::from_rotation_z(piece.tilt))
        } else {
            (Vec3::new(piece.size.z, piece.size.y, piece.size.x), Vec3::new(piece.at.z, piece.at.y, piece.at.x), Quat::from_rotation_x(piece.tilt))
        };
        // Boards are laid with the grain along their length; the wood's own runs up the face.
        let grain = match (paint, cover) {
            (None, Material::WeatheredPlanks) if along_x => Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            (None, Material::WeatheredPlanks) => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            _ => Quat::IDENTITY,
        };
        let mesh = self.meshes.add(box_uv_mesh(size, centre + at, rotation * grain, cover.tile()));
        let colour = paint.unwrap_or(Color::srgb(0.4, 0.3, 0.2));
        let material = self.paint(colour);
        let mut child = self.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform { translation: if world { centre + at } else { at }, rotation, scale: Vec3::ONE },
            Visibility::Hidden,
            Veneer(cover),
        ));
        if paint.is_none() {
            child.insert(Textured { material: cover, shape: Shape::Baked });
        }
        child.id()
    }

    /// A window's frame (its jambs, head and sill) in its Zone's trim paint, shown with the
    /// weathered wood. Drawn only: the opening stays as it is.
    fn window_frame(&mut self, mid: Vec3, along_x: bool, zone: Option<ZoneId>) {
        let opening = WINDOW_HEIGHT - WINDOW_SILL_HEIGHT;
        let centre = mid + Vec3::Y * (WINDOW_SILL_HEIGHT + opening / 2.0);
        let trim = palette(zone).trim;
        for piece in door_look::window_frame(WINDOW_WIDTH, opening, WALL_THICKNESS) {
            let id = self.veneer(&piece, centre, along_x, Material::WeatheredPlanks, Some(trim), true);
            self.commands.entity(id).insert(InRun);
        }
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
                Gap::Door(..) | Gap::Gate(..) | Gap::PowerGate(_) | Gap::LinkedDoor(_) => (DOOR_WIDTH, DOOR_HEIGHT),
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
                let zone = self
                    .windows
                    .iter()
                    .find(|(_, c, _)| (c.x - mid.x).abs() < 0.01 && (c.z - mid.y).abs() < 0.01 && (c.y - y0).abs() < 0.01)
                    .map(|(zone, ..)| *zone);
                self.window_frame(Vec3::new(mid.x, y0, mid.y), along_x, zone);
            }
            if let Gap::Door(joins, price) = gap {
                let door = self.door_slab(across(0.25, DOOR_HEIGHT), Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y), price.is_none(), joins.map(|j| j.0));
                self.commands.entity(door).insert((
                    Door {
                        joins: *joins,
                        price: *price,
                        open: false,
                        gate: None,
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
            if let Gap::Gate(key, price) = gap {
                let at = Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y);
                let door = self.door_slab(across(0.25, DOOR_HEIGHT), at, false, Some(key.0));
                self.commands.entity(door).insert((
                    Door { joins: None, price: Some(*price), open: false, gate: Some(*key) },
                    Visibility::default(),
                    Buyable { use_point: at, prompt: String::new(), price: Some(*price), range: DOOR_USE_RANGE },
                ));
            }
            if let Gap::PowerGate(key) = gap {
                let at = Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y);
                let door = self.door_slab(across(0.25, DOOR_HEIGHT), at, true, Some(key.0));
                self.commands.entity(door).insert((Door { joins: None, price: None, open: false, gate: Some(*key) }, Visibility::default()));
            }
            if let Gap::LinkedDoor(joins) = gap {
                let at = Vec3::new(mid.x, y0 + DOOR_HEIGHT / 2.0, mid.y);
                let door = self.door_slab(across(0.25, DOOR_HEIGHT), at, true, joins.map(|j| j.0));
                // It can't be bought either, but its Prompt says what opens it, and the Use key
                // buzzes.
                self.commands.entity(door).insert((
                    Door {
                        joins: *joins,
                        price: None,
                        open: false,
                        gate: None,
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
        for face in wall_faces(self.map, a, b, s, e, y0 + lo, y0 + hi) {
            self.skin(&face, along_x, line, foot);
        }
    }

    /// Draws one face of a wall piece whose middle is on `line`: its half of the wall in the
    /// face's wall colour (or its Zone's wall material), with trim in front of it, always in
    /// paint: skirting where it stands on a floor at `foot`, and a band along the top of each
    /// storey.
    fn skin(&mut self, face: &Face, along_x: bool, line: f32, foot: Option<f32>) {
        let colours = self.repaint.unwrap_or_else(|| palette(face.zone));
        let wall = if self.repaint.is_some() {
            self.repaint_wall
        } else if face.zone.is_none() {
            self.outside_wall
        } else {
            materials(face.zone).map(|set| set.wall)
        };
        let out = if along_x { face.normal.y } else { face.normal.x };
        let (from, to) = face.along;
        let slab = |this: &mut Self, off: f32, depth: f32, bottom: f32, top: f32, colour: Color, material: Option<Material>| {
            let across = line + out * (off + depth / 2.0);
            let (mid, y) = ((from + to) / 2.0, (bottom + top) / 2.0);
            let (centre, size) = if along_x {
                (Vec3::new(mid, y, across), Vec3::new(to - from, top - bottom, depth))
            } else {
                (Vec3::new(across, y, mid), Vec3::new(depth, top - bottom, to - from))
            };
            this.shape_in(size, centre, Quat::IDENTITY, colour, material);
        };
        let half = WALL_THICKNESS / 2.0;
        let (bottom, top) = face.y;
        slab(self, 0.0, half, bottom, top, colours.wall, wall);
        if foot == Some(bottom) && top - bottom > SKIRTING.x {
            slab(self, half, SKIRTING.y, bottom, bottom + SKIRTING.x, colours.trim, None);
        }
        let storey_top = top > 0.0 && ((top / STOREY).round() * STOREY - top).abs() < 1e-3;
        if storey_top && top - bottom >= 0.6 {
            slab(self, half, CORNICE.y, top - CORNICE.x, top, colours.trim, None);
            // Damp streaking down from under the cornice.
            let family = if self.repaint.is_some() { None } else { materials(face.zone).and_then(|set| set.decals) };
            if family == Some(DecalFamily::Leaks) {
                let normal = if along_x { Vec3::Z * out } else { Vec3::X * out };
                let surface = line + out * half;
                for leak in leaks(from, to, bottom, top - CORNICE.x, line) {
                    let face = if along_x { Vec3::new(leak.along, leak.top, surface) } else { Vec3::new(surface, leak.top, leak.along) };
                    self.commands.spawn((InRun, DecalPiece(leak.decal), leak_transform(&leak, face, normal)));
                }
            }
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
pub(super) fn wall_faces(map: MapId, a: Vec2, b: Vec2, s: f32, e: f32, y0: f32, y1: f32) -> Vec<Face> {
    let along_x = a.y == b.y;
    let dir = (b - a).normalize();
    let (p, q) = (a + dir * s, a + dir * e);
    let axis = |v: Vec2| if along_x { v.x } else { v.y };
    let (lo, hi) = (axis(p).min(axis(q)), axis(p).max(axis(q)));
    let line = if along_x { a.y } else { a.x };

    // Cut along the wall wherever a Zone starts or ends.
    let mut cuts = vec![lo, hi];
    for zone in ZoneId::ALL.into_iter().filter(|zone| zone.map() == map) {
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
                let zone = crate::enemies::nav::zone_at(map, Vec3::new(at.x, sample_y, at.y));
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
        let mut faces: Vec<_> = wall_faces(MapId::Greybox, a, b, s, e, y0, y1)
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
