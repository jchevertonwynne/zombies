//! How the Archive's stacks look (#537): a Bookcase and a fixed stack, told apart at a glance, the iron rail
//! the Bookcases roll on, the soffit over each Range's opening and its brass letter.
//!
//! | | Bookcase (rolls) | fixed stack (bolted down) |
//! |---|---|---|
//! | carcass | `wood_table_worn` | the same wood, tinted 0.7 |
//! | foot | a 0.04 m shadow gap and dark castor blocks, over the rail | a 0.15 m `monastery_stone_floor` plinth with four iron bolts |
//! | end panel | `dark_paneled_wood`, a brass frame and a brass plate ("A · 3") | a plain dark end |
//!
//! The look is a pure layout in a stack's own frame (its length along +x, up, its depth along +z; the foot's
//! middle the origin, as a Prop's unit model is, `fitting`): `bookcase`, `fixed_stack`, `rail`, `soffit`. A `Kit`
//! turns each into a few merged meshes (one a finish, so a stack is a handful of entities) and spawns them, and
//! the shelf fill, the shared `book_rows` kitbash (three variants, one mesh a face), is laid over both faces of
//! every Bookcase and every fixed stack, so all of them batch.
//!
//! Everything is built from the Layout on every machine (`dress_bookcases` for a Range's Leaves, `Builder` for the
//! fixed stacks, the rail and the soffit), so a client and a late joiner see what the Host does; nothing is sent.
//! The parts are code-built (they wear their material files once loaded), and the shelf fill is a glTF read only
//! when it is on disk, so headless runs need no asset files.

use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;

use super::map::MapId;
use super::map::textures::{Material, Polish, Shape, Textured, Tint, box_uv_mesh, on_disk};
use super::shifting::{Driver, Leaf, LeafBody, Piece, PieceId, PiecePhase, PieceState, WheelPost, pose as shifting_pose};
use crate::enemies::sapper::Sabotaged;
use crate::enemies::target::half_height_above_feet;
use crate::enemies::zombie::{Dying, Zombie};
use crate::players::player::{Locomotion, Player};
use crate::state::{InRun, world_runs};
use crate::weapons::blast::{Shape as BlastShape, envelope};
use crate::tuning::{BOOKCASE_DEPTH, BOOKCASE_HEIGHT, BOOKCASE_LENGTH};

// ---- Measures ----

/// The shadow gap under a Bookcase's carcass, and the dark castor blocks in it.
pub const CASTOR_GAP: f32 = 0.04;
/// A fixed stack's stone plinth.
pub const PLINTH: f32 = 0.15;
/// How thick an end panel is, and how far its brass frame and plate stand proud of it.
const END_PANEL: f32 = 0.05;
const FRAME_PROUD: f32 = 0.012;
/// The flush iron strips of a rail: wide, apart and how far over the floor.
pub const RAIL_STRIP: f32 = 0.08;
pub const RAIL_GAUGE: f32 = 0.6;
/// How much longer than its Range the rail's strips are, and the stop blocks at its two ends.
pub const RAIL_EXTRA: f32 = 0.3;
const STOP: f32 = 0.2;
/// How far a Range's wall face is from its line, and how high the soffit's lettering is centred over the floor.
pub const WALL_FACE: f32 = 0.2;
const SOFFIT_LETTERS: f32 = BOOKCASE_HEIGHT + 0.5;
/// The tint of a fixed stack's carcass.
pub const STACK_TINT: f32 = 0.7;

/// A Bookcase's (and a fixed stack's) own frame: its length along x, its height and its depth along z.
pub const SIZE: Vec3 = Vec3::new(BOOKCASE_LENGTH, BOOKCASE_HEIGHT, BOOKCASE_DEPTH);

// ---- What a piece is ----

/// What an `Item` is made of and for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// A Bookcase's shelf block.
    Carcass,
    /// A fixed stack's, in the same wood tinted darker.
    StackCarcass,
    /// A Bookcase's end panel, under its brass frame.
    EndPanel,
    /// A fixed stack's plain dark end.
    PlainEnd,
    /// The brass frame round a Bookcase's end panel.
    Frame,
    /// The brass plate on it, and the ink of its lettering.
    Plate,
    Ink,
    /// A dark castor block under a Bookcase.
    Castor,
    /// A fixed stack's stone plinth and the iron bolts that hold it to the floor.
    Plinth,
    Bolt,
    /// One of the rail's two flush iron strips, and an iron stop block at its ends.
    Rail,
    Stop,
    /// A brass letter on a soffit.
    Letter,
}

/// A look shared by several parts, so they merge into one mesh and one material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finish {
    Wood,
    StackWood,
    Panel,
    DarkPanel,
    Brass,
    Ink,
    Iron,
    PolishedIron,
    Stone,
}

/// How a finish is drawn: the material set it wears once loaded, its tint and polish, and the plain
/// look it keeps until then (or for good without the files).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub material: Material,
    pub tint: Color,
    pub polish: Option<Polish>,
    pub colour: Color,
    pub metallic: f32,
    pub roughness: f32,
}

impl Part {
    pub fn finish(self) -> Finish {
        match self {
            Part::Carcass => Finish::Wood,
            Part::StackCarcass => Finish::StackWood,
            Part::EndPanel => Finish::Panel,
            Part::PlainEnd => Finish::DarkPanel,
            Part::Frame | Part::Plate | Part::Letter => Finish::Brass,
            Part::Ink => Finish::Ink,
            Part::Castor | Part::Bolt | Part::Stop => Finish::Iron,
            Part::Rail => Finish::PolishedIron,
            Part::Plinth => Finish::Stone,
        }
    }
}

const BRASS: Color = Color::srgb(0.75, 0.55, 0.25);

impl Finish {
    pub fn look(self) -> Look {
        let plain = |material, tint: Color, colour: Color| Look { material, tint, polish: None, colour, metallic: 0.0, roughness: 0.9 };
        match self {
            Finish::Wood => plain(Material::WoodTableWorn, Color::WHITE, Color::srgb(0.32, 0.21, 0.12)),
            Finish::StackWood => plain(Material::WoodTableWorn, Color::srgb(STACK_TINT, STACK_TINT, STACK_TINT), Color::srgb(0.22, 0.15, 0.09)),
            Finish::Panel => plain(Material::DarkPaneledWood, Color::WHITE, Color::srgb(0.2, 0.12, 0.07)),
            Finish::DarkPanel => plain(Material::DarkPaneledWood, Color::srgb(0.55, 0.55, 0.55), Color::srgb(0.11, 0.07, 0.05)),
            Finish::Brass => Look { material: Material::MetalPlate02, tint: BRASS, polish: Some(Polish { metallic: 0.9, roughness: 0.5 }), colour: BRASS, metallic: 0.8, roughness: 0.4 },
            Finish::Ink => plain(Material::RustyMetal02, Color::srgb(0.05, 0.045, 0.04), Color::srgb(0.05, 0.045, 0.04)),
            Finish::Iron => plain(Material::RustyMetal02, Color::srgb(0.1, 0.1, 0.11), Color::srgb(0.1, 0.1, 0.11)),
            Finish::PolishedIron => Look {
                material: Material::RustyMetal02,
                tint: Color::srgb(0.08, 0.08, 0.09),
                polish: Some(Polish { metallic: 0.9, roughness: 0.35 }),
                colour: Color::srgb(0.08, 0.08, 0.09),
                metallic: 0.9,
                roughness: 0.35,
            },
            Finish::Stone => plain(Material::MonasteryStoneFloor, Color::WHITE, Color::srgb(0.46, 0.46, 0.43)),
        }
    }
}

/// One box of a look, in its own frame: `size` across, centred on `at`, turned `rot`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Item {
    pub part: Part,
    pub size: Vec3,
    pub at: Vec3,
    pub rot: Quat,
}

impl Item {
    fn new(part: Part, size: Vec3, at: Vec3) -> Self {
        Self { part, size, at, rot: Quat::IDENTITY }
    }

    /// The lowest the box reaches.
    #[cfg(test)]
    pub fn bottom(&self) -> f32 {
        self.at.y - self.extent().y
    }

    /// The half-extents of the box round the item once turned.
    pub fn extent(&self) -> Vec3 {
        let m = Mat3::from_quat(self.rot);
        let half = self.size / 2.0;
        Vec3::new(
            m.x_axis.x.abs() * half.x + m.y_axis.x.abs() * half.y + m.z_axis.x.abs() * half.z,
            m.x_axis.y.abs() * half.x + m.y_axis.y.abs() * half.y + m.z_axis.y.abs() * half.z,
            m.x_axis.z.abs() * half.x + m.y_axis.z.abs() * half.y + m.z_axis.z.abs() * half.z,
        )
    }
}

/// Places a group of items: each moves by `at` after turning by `rot`.
fn place(items: Vec<Item>, at: Vec3, rot: Quat) -> Vec<Item> {
    items.into_iter().map(|item| Item { at: at + rot * item.at, rot: rot * item.rot, ..item }).collect()
}

/// The turn that puts a plate's own frame (x right, y up, z out of it) on a face looking along `out`.
pub fn facing(out: Vec3) -> Quat {
    Quat::from_mat3(&Mat3::from_cols(Vec3::Y.cross(out), Vec3::Y, out))
}

// ---- Lettering ----

/// A glyph as polylines on a unit box (x across, y up), drawn in strokes. Only the letters and digits the plates
/// and soffits spell.
fn glyph(c: char) -> &'static [&'static [(f32, f32)]] {
    match c {
        'A' => &[&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)], &[(0.2, 0.4), (0.8, 0.4)]],
        'B' => &[&[(0.0, 0.0), (0.0, 1.0), (0.7, 1.0), (0.9, 0.85), (0.9, 0.65), (0.7, 0.5), (0.0, 0.5)], &[(0.7, 0.5), (0.95, 0.35), (0.95, 0.15), (0.75, 0.0), (0.0, 0.0)]],
        'C' => &[&[(0.95, 0.85), (0.7, 1.0), (0.25, 1.0), (0.0, 0.8), (0.0, 0.2), (0.25, 0.0), (0.7, 0.0), (0.95, 0.15)]],
        'D' => &[&[(0.0, 0.0), (0.0, 1.0), (0.6, 1.0), (0.95, 0.7), (0.95, 0.3), (0.6, 0.0), (0.0, 0.0)]],
        'E' => &[&[(0.9, 1.0), (0.0, 1.0), (0.0, 0.0), (0.9, 0.0)], &[(0.0, 0.5), (0.7, 0.5)]],
        'G' => &[&[(0.95, 0.85), (0.7, 1.0), (0.25, 1.0), (0.0, 0.8), (0.0, 0.2), (0.25, 0.0), (0.7, 0.0), (0.95, 0.15), (0.95, 0.45), (0.55, 0.45)]],
        'N' => &[&[(0.0, 0.0), (0.0, 1.0), (0.9, 0.0), (0.9, 1.0)]],
        'R' => &[&[(0.0, 0.0), (0.0, 1.0), (0.7, 1.0), (0.9, 0.85), (0.9, 0.65), (0.7, 0.5), (0.0, 0.5)], &[(0.5, 0.5), (0.95, 0.0)]],
        'T' => &[&[(0.0, 1.0), (0.9, 1.0)], &[(0.45, 1.0), (0.45, 0.0)]],
        '1' => &[&[(0.15, 0.8), (0.5, 1.0), (0.5, 0.0)], &[(0.15, 0.0), (0.85, 0.0)]],
        '2' => &[&[(0.0, 0.8), (0.2, 1.0), (0.7, 1.0), (0.9, 0.8), (0.9, 0.6), (0.0, 0.0), (0.95, 0.0)]],
        '3' => &[&[(0.0, 0.9), (0.2, 1.0), (0.7, 1.0), (0.9, 0.85), (0.9, 0.65), (0.65, 0.5), (0.3, 0.5)], &[(0.65, 0.5), (0.9, 0.35), (0.9, 0.15), (0.7, 0.0), (0.2, 0.0), (0.0, 0.1)]],
        '4' => &[&[(0.7, 0.0), (0.7, 1.0), (0.0, 0.3), (0.95, 0.3)]],
        // A dot: one stroke of no length.
        '\u{b7}' => &[&[(0.2, 0.5), (0.2, 0.5)]],
        _ => &[],
    }
}

/// `label` as strokes of `part`, `height` tall and `depth` thick, centred on the origin of a plate's frame (x right, y up,
/// z out of it). A glyph is 0.55 of its height wide.
pub fn text(label: &str, part: Part, height: f32, depth: f32) -> Vec<Item> {
    let (glyph_w, gap, space, stroke) = (0.55 * height, 0.2 * height, 0.45 * height, 0.11 * height);
    let advance = |c: char| if c == ' ' { space } else if c == '\u{b7}' { stroke + gap } else { glyph_w + gap };
    let width = label.chars().map(advance).sum::<f32>() - gap;
    let mut at = -width / 2.0;
    let mut items = Vec::new();
    for c in label.chars() {
        if c == '\u{b7}' {
            items.push(Item::new(part, Vec3::new(stroke, stroke, depth), Vec3::new(at + stroke / 2.0, 0.0, 0.0)));
            at += advance(c);
            continue;
        }
        for line in glyph(c) {
            for pair in line.windows(2) {
                let [(x0, y0), (x1, y1)] = [pair[0], pair[1]];
                let (a, b) = (Vec2::new(at + x0 * glyph_w, (y0 - 0.5) * height), Vec2::new(at + x1 * glyph_w, (y1 - 0.5) * height));
                let (mid, d) = ((a + b) / 2.0, b - a);
                // Strokes overshoot by half their width, so the corners close.
                let mut item = Item::new(part, Vec3::new(d.length() + stroke, stroke, depth), Vec3::new(mid.x, mid.y, 0.0));
                item.rot = Quat::from_rotation_z(d.y.atan2(d.x));
                items.push(item);
            }
        }
        at += advance(c);
    }
    items
}

// ---- The Bookcase and the fixed stack ----

/// The end panel of a stack at `side` (+1 the +x end, -1 the -x): its panel, and for a Bookcase the brass frame and
/// the plate (with its lettering) on it.
fn end(size: Vec3, floor: f32, side: f32, label: Option<&str>) -> Vec<Item> {
    let (len, height, depth) = (size.x, size.y, size.z);
    let tall = height - floor;
    let x = side * (len / 2.0 - END_PANEL / 2.0);
    let Some(label) = label else {
        return vec![Item::new(Part::PlainEnd, Vec3::new(END_PANEL, tall, depth), Vec3::new(x, floor + tall / 2.0, 0.0))];
    };
    let mut items = vec![Item::new(Part::EndPanel, Vec3::new(END_PANEL, tall, depth), Vec3::new(x, floor + tall / 2.0, 0.0))];
    // The frame, proud of the panel's face.
    let face = side * (len / 2.0 + FRAME_PROUD / 2.0);
    let (bar, inset) = (0.05, 0.08);
    for up in [floor + inset, height - inset] {
        items.push(Item::new(Part::Frame, Vec3::new(FRAME_PROUD, bar, depth - 2.0 * inset + bar), Vec3::new(face, up, 0.0)));
    }
    for across in [-1.0, 1.0] {
        items.push(Item::new(Part::Frame, Vec3::new(FRAME_PROUD, tall - 2.0 * inset + bar, bar), Vec3::new(face, floor + tall / 2.0, across * (depth / 2.0 - inset))));
    }
    // The plate at eye height, its lettering inked on it: laid in its own frame, then turned onto this end.
    let (plate_h, ink_h) = (0.2, 0.09);
    let letters = text(label, Part::Ink, ink_h, 0.006);
    let width = letters.iter().map(|item| item.at.x.abs() + item.extent().x).fold(0.0, f32::max) * 2.0 + 0.1;
    let mut plate = vec![Item::new(Part::Plate, Vec3::new(width, plate_h, 0.016), Vec3::new(0.0, 0.0, 0.008))];
    plate.extend(place(letters, Vec3::new(0.0, 0.0, 0.019), Quat::IDENTITY));
    place(plate, Vec3::new(side * (len / 2.0), 1.6, 0.0), facing(Vec3::X * side)).into_iter().chain(items).collect()
}

/// A Bookcase (a Range's Leaf): a carcass standing on a 0.04 m shadow gap and four dark castor blocks (over the rail's
/// strips), `dark_paneled_wood` ends with a brass frame and a brass plate reading `label` ("A · 3": Range and Bookcase).
pub fn bookcase(size: Vec3, label: &str) -> Vec<Item> {
    let (len, height, depth) = (size.x, size.y, size.z);
    let tall = height - CASTOR_GAP;
    let mut items = vec![Item::new(Part::Carcass, Vec3::new(len - 2.0 * END_PANEL, tall, depth), Vec3::new(0.0, CASTOR_GAP + tall / 2.0, 0.0))];
    for strip in [-1.0, 1.0] {
        for side in [-1.0, 1.0] {
            items.push(Item::new(Part::Castor, Vec3::new(0.16, CASTOR_GAP, 0.16), Vec3::new(strip * RAIL_GAUGE / 2.0, CASTOR_GAP / 2.0, side * 0.4)));
        }
    }
    for side in [-1.0, 1.0] {
        items.extend(end(size, CASTOR_GAP, side, Some(label)));
    }
    items
}

/// A fixed stack: the carcass in the same wood tinted 0.7 on a 0.15 m stone plinth with four iron floor bolts, plain dark
/// ends with no plate, and nothing under it to roll on.
pub fn fixed_stack(size: Vec3) -> Vec<Item> {
    let (len, height, depth) = (size.x, size.y, size.z);
    let tall = height - PLINTH;
    // The plinth's lip stands 6 cm out of the box all round, and the bolts sit on it at the corners.
    let lip = 0.06;
    let mut items = vec![
        Item::new(Part::Plinth, Vec3::new(len + 2.0 * lip, PLINTH, depth + 2.0 * lip), Vec3::new(0.0, PLINTH / 2.0, 0.0)),
        Item::new(Part::StackCarcass, Vec3::new(len - 2.0 * END_PANEL, tall, depth), Vec3::new(0.0, PLINTH + tall / 2.0, 0.0)),
    ];
    for x in [-1.0, 1.0] {
        for z in [-1.0, 1.0] {
            items.push(Item::new(Part::Bolt, Vec3::new(lip, 0.03, lip), Vec3::new(x * (len / 2.0 + lip / 2.0), PLINTH + 0.015, z * (depth / 2.0 + lip / 2.0))));
        }
    }
    for side in [-1.0, 1.0] {
        items.extend(end(size, PLINTH, side, None));
    }
    items
}

// ---- The rail and the soffit ----

/// A Range's rail, in the Range's frame: x along the rail from where it starts, z across it from the Range's wall line,
/// y up from the floor. Two flush iron strips `RAIL_GAUGE` apart over the rail's `length` plus 0.3 m, polished on top,
/// and a 0.2 m stop block across them at each end.
pub fn rail(length: f32) -> Vec<Item> {
    let mut items = Vec::new();
    for side in [-1.0, 1.0] {
        items.push(Item::new(Part::Rail, Vec3::new(length + RAIL_EXTRA, 0.01, RAIL_STRIP), Vec3::new(length / 2.0, 0.0, side * RAIL_GAUGE / 2.0)));
    }
    for end in [0.0, length] {
        items.push(Item::new(Part::Stop, Vec3::new(STOP, 0.1, RAIL_GAUGE + 2.0 * RAIL_STRIP), Vec3::new(end, 0.05, 0.0)));
    }
    items
}

/// The brass lettering on a Range's soffit, in the frame of the wall face it is on (x right, y up, z out of the wall):
/// `label` centred on the wall over the opening, its middle 0.5 m over the soffit's 3.0 m.
pub fn soffit(label: &str, height: f32) -> Vec<Item> {
    place(text(label, Part::Letter, height, 0.02), Vec3::new(0.0, SOFFIT_LETTERS, 0.01), Quat::IDENTITY)
}

/// The Range letter of Piece `n` of the Archive, in the order of its Ranges (R0, RA to RD): "G" is the Great Range.
pub fn range_letter(piece: usize) -> char {
    ['G', 'A', 'B', 'C', 'D'].get(piece).copied().unwrap_or('A')
}

/// The soffit's wording for Piece `n`: "GREAT RANGE", or the Range's letter.
pub fn soffit_label(piece: usize) -> String {
    if piece == 0 { "GREAT RANGE".into() } else { range_letter(piece).to_string() }
}

/// How tall the soffit's lettering is.
pub fn soffit_height(piece: usize) -> f32 {
    if piece == 0 { 0.4 } else { 0.5 }
}

// ---- The shelf fill ----

/// How many `book_rows` variants there are.
pub const BOOK_VARIANTS: usize = 3;

/// The variant on a stack's front face (`front`, +z: the high side of a Bookcase on its rail) or its back, for the
/// stack `nth` of a row. The faces of one stack differ, and a stack's front never matches the back of the one after it,
/// the face it touches.
pub fn book_variant(nth: usize, front: bool) -> usize {
    (2 * nth + if front { 0 } else { 2 }) % BOOK_VARIANTS
}

/// The glTF of a `book_rows` variant, under `assets/`.
pub fn book_rows_path(variant: usize) -> &'static str {
    ["props/book_rows/book_rows_a.gltf", "props/book_rows/book_rows_b.gltf", "props/book_rows/book_rows_c.gltf"][variant % BOOK_VARIANTS]
}

/// One face of a stack's shelf fill, to be dressed with its `book_rows` glTF when that is on disk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookFace {
    pub variant: usize,
}

/// A stack's two shelf-fill faces, as children of `parent`, in the stack's own frame: the front (+z) and, turned half a
/// revolution, the back. `nth` varies them from stack to stack.
pub fn spawn_book_faces(commands: &mut Commands, parent: Entity, nth: usize) {
    for (front, turn) in [(true, 0.0), (false, PI)] {
        commands.spawn((Name::new("Book rows"), BookFace { variant: book_variant(nth, front) }, Transform::from_rotation(Quat::from_rotation_y(turn)), Visibility::default(), ChildOf(parent)));
    }
}

/// Starts each new `BookFace`'s glTF if it is on disk; with it missing the carcass keeps its wood. Without scene assets at all
/// (a headless run), nothing is asked for.
fn load_book_rows(mut commands: Commands, asset_server: Res<AssetServer>, scenes: Option<Res<Assets<WorldAsset>>>, faces: Query<(Entity, &BookFace), Added<BookFace>>) {
    if scenes.is_none() {
        return;
    }
    for (entity, face) in &faces {
        let path = book_rows_path(face.variant);
        if on_disk(path) {
            commands.entity(entity).insert(WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))));
        } else {
            debug!("{path}: missing, the shelves stay bare");
        }
    }
}

// ---- Building a look ----

/// What a stack's items have been made into: one merged mesh per finish, and one material per finish, shared by
/// every stack, so they batch.
#[derive(Default)]
pub struct Kit {
    meshes: HashMap<String, Vec<(Finish, Handle<Mesh>)>>,
    materials: HashMap<Finish, Handle<StandardMaterial>>,
}

/// `items` merged into one mesh in their own frame, their UVs those of the frame (so a texture moves with the stack).
fn merged(items: &[&Item], tile: f32) -> Mesh {
    let mut out: Option<Mesh> = None;
    for item in items {
        let mesh = box_uv_mesh(item.size, item.at, item.rot, tile).transformed_by(Transform { translation: item.at, rotation: item.rot, scale: Vec3::ONE });
        match &mut out {
            None => out = Some(mesh),
            Some(out) => out.merge(&mesh).expect("boxes have the same attributes"),
        }
    }
    out.expect("a finish with items")
}

impl Kit {
    fn material(&mut self, materials: &mut Assets<StandardMaterial>, finish: Finish) -> Handle<StandardMaterial> {
        self.materials
            .entry(finish)
            .or_insert_with(|| {
                let look = finish.look();
                materials.add(StandardMaterial { base_color: look.colour, perceptual_roughness: look.roughness, metallic: look.metallic, ..default() })
            })
            .clone()
    }

    /// Spawns `items` (made by `make` the first time `key` is asked for) as children of `parent`, one entity a finish.
    pub fn dress(&mut self, commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, parent: Entity, key: &str, make: impl FnOnce() -> Vec<Item>) {
        if !self.meshes.contains_key(key) {
            let items = make();
            let mut finishes: Vec<Finish> = Vec::new();
            for item in &items {
                if !finishes.contains(&item.part.finish()) {
                    finishes.push(item.part.finish());
                }
            }
            let made = finishes
                .into_iter()
                .map(|finish| {
                    let of: Vec<&Item> = items.iter().filter(|item| item.part.finish() == finish).collect();
                    (finish, meshes.add(merged(&of, finish.look().material.tile())))
                })
                .collect();
            self.meshes.insert(key.to_string(), made);
        }
        let parts = self.meshes[key].clone();
        for (finish, mesh) in parts {
            let look = finish.look();
            let mut part = commands.spawn((
                Name::new("Stack part"),
                Mesh3d(mesh),
                MeshMaterial3d(self.material(materials, finish)),
                Textured { material: look.material, shape: Shape::Baked },
                Transform::default(),
                ChildOf(parent),
            ));
            if look.tint != Color::WHITE {
                part.insert(Tint(look.tint));
            }
            if let Some(polish) = look.polish {
                part.insert(polish);
            }
        }
    }
}

/// The turn that lays a stack's own frame (length along +x) on a box `size` across, long along x or z.
pub fn lay(size: Vec3) -> Quat {
    if size.z > size.x { Quat::from_rotation_y(FRAC_PI_2) } else { Quat::IDENTITY }
}

/// Redresses every Range's Bookcases (the Archive's Pieces driven by Wheels) over the plain slabs the Shifting plugin
/// spawned: the carcass, ends, plate and castors, and the shelf fill on both faces. The Leaf body and its collider stay.
fn dress_bookcases(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut kit: Local<Kit>,
    leaves: Query<(Entity, &LeafBody), Added<LeafBody>>,
    map: Res<MapId>,
) {
    let layout = map.layout();
    for (entity, body) in &leaves {
        let Some(piece) = layout.shifting.get(usize::from(body.piece.0)) else { continue };
        let (Driver::Wheels(_), Some(leaf)) = (piece.driver, piece.leaves.get(body.index)) else { continue };
        commands.entity(entity).despawn_related::<Children>();
        // The foot's middle is the Leaf's origin.
        let root = commands
            .spawn((
                Name::new("Bookcase look"),
                Transform::from_translation(leaf.offset - Vec3::Y * leaf.size.y / 2.0).with_rotation(lay(leaf.size)),
                Visibility::default(),
                ChildOf(entity),
            ))
            .id();
        let label = format!("{} \u{b7} {}", range_letter(usize::from(body.piece.0)), body.index + 1);
        kit.dress(&mut commands, &mut meshes, &mut materials, root, &format!("bookcase {label}"), || bookcase(SIZE, &label));
        spawn_book_faces(&mut commands, root, body.index);
    }
}

// ---- The Wheels and their Aisle lamps (#538) ----

/// One turn of a Wheel for every this many metres of rail the Range rolls.
pub const WHEEL_PITCH: f32 = 0.5;
/// How tall a Wheel's post is: its lamp sits on top.
pub const POST_HEIGHT: f32 = 1.1;
/// Where the hand wheel's hub is, in its post's frame (up and out along the way the Wheel faces).
pub const HUB_AT: Vec3 = Vec3::new(0.0, 0.8, 0.2);
/// A hand wheel's radius.
pub const WHEEL_RADIUS: f32 = 0.35;
/// How fast the lamp flashes while the Range warns, in flashes a second.
pub const FLASH_HZ: f32 = 2.0;

/// How far a Range's rail has carried a Bookcase since Rest 0, in metres: at Rest `k` it is `k` hops of one Aisle's width (one
/// Bookcase crosses the Aisle from one Rest to the next), and through a Roll it runs on from there, at the pace of the Bookcases'
/// own smoothstep, one hop whatever the number of Rests the Roll crosses (every Bookcase that moves moves one hop). The same on every
/// machine from the phase alone.
pub fn rail_travel(piece: &Piece, phase: PiecePhase) -> f32 {
    let hop = piece.rests[0].at.iter().sum::<f32>() - piece.rests.get(1).map_or(0.0, |rest| rest.at.iter().sum::<f32>());
    match phase {
        PiecePhase::Swinging { from, to, elapsed } => {
            let toward = if to >= from { 1.0 } else { -1.0 };
            f32::from(from) * hop + toward * hop * shifting_pose(0.0, 1.0, elapsed)
        }
        other => f32::from(other.standing().unwrap_or(0)) * hop,
    }
}

/// How far a Wheel is turned when the Range has rolled `travel` metres: one turn per `WHEEL_PITCH`.
pub fn wheel_angle(travel: f32) -> f32 {
    travel / WHEEL_PITCH * TAU
}

/// What an Aisle lamp says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LampState {
    /// This Aisle is open and the Range idle.
    Open,
    /// This Aisle is shut and the Range idle.
    Shut,
    /// The Range warns or waits: amber, flashing.
    Warning,
    /// The Range rolls: amber, steady.
    Rolling,
    /// The Range has just landed: amber, dim.
    Cooling,
    /// The Range is Sabotaged: dull red, with a chain through the spokes.
    Sabotaged,
}

/// The lamp of the Wheel that opens `rest`, for a Range in `phase`.
pub fn aisle_lamp(phase: PiecePhase, rest: u8, sabotaged: bool) -> LampState {
    if sabotaged {
        return LampState::Sabotaged;
    }
    match phase {
        PiecePhase::Rest(at) if at == rest => LampState::Open,
        PiecePhase::Rest(_) => LampState::Shut,
        PiecePhase::Warning { .. } | PiecePhase::Waiting { .. } => LampState::Warning,
        PiecePhase::Swinging { .. } => LampState::Rolling,
        PiecePhase::Cooling { .. } => LampState::Cooling,
    }
}

/// The bulb materials a lamp swaps between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bulb {
    Green,
    DarkAmber,
    Amber,
    AmberDim,
    Red,
}

impl Bulb {
    pub const ALL: [Bulb; 5] = [Bulb::Green, Bulb::DarkAmber, Bulb::Amber, Bulb::AmberDim, Bulb::Red];

    /// The base colour and the emissive glow.
    pub fn look(self) -> (Color, LinearRgba) {
        let glow = |c: Color, k: f32| {
            let LinearRgba { red, green, blue, .. } = c.to_linear();
            LinearRgba::rgb(red * k, green * k, blue * k)
        };
        match self {
            Bulb::Green => (Color::srgb(0.3, 1.0, 0.45), glow(Color::srgb(0.3, 1.0, 0.45), 8.0)),
            Bulb::DarkAmber => (Color::srgb(0.22, 0.11, 0.03), LinearRgba::BLACK),
            Bulb::Amber => (Color::srgb(1.0, 0.6, 0.15), glow(Color::srgb(1.0, 0.6, 0.15), 10.0)),
            Bulb::AmberDim => (Color::srgb(1.0, 0.6, 0.15), glow(Color::srgb(1.0, 0.6, 0.15), 2.0)),
            Bulb::Red => (Color::srgb(0.55, 0.06, 0.04), glow(Color::srgb(0.55, 0.06, 0.04), 3.0)),
        }
    }
}

/// Whether a flashing lamp is lit `now` seconds in.
pub fn flash_on(now: f32) -> bool {
    (now * FLASH_HZ).fract() < 0.5
}

/// The bulb a lamp in `state` shows `now` seconds in.
pub fn bulb(state: LampState, now: f32) -> Bulb {
    match state {
        LampState::Open => Bulb::Green,
        LampState::Shut => Bulb::DarkAmber,
        LampState::Warning if flash_on(now) => Bulb::Amber,
        LampState::Warning => Bulb::DarkAmber,
        LampState::Rolling => Bulb::Amber,
        LampState::Cooling => Bulb::AmberDim,
        LampState::Sabotaged => Bulb::Red,
    }
}

/// A Wheel's look: its post, hand wheel, lamp and chain stand in a frame at its floor spot, +z the way it faces.
#[derive(Component, Debug, Clone, Copy)]
pub struct WheelLook;

/// The systems that draw the Ranges' Wheels, lamps and dust from the Pieces' phases, after the Host's have run.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct RangeLookSet;

/// The hand wheel, turned from its Range's rail travel.
#[derive(Component, Debug, Clone, Copy)]
pub struct WheelSpin {
    pub piece: PieceId,
}

/// The Aisle lamp on a post's top, and the bulb it shows now.
#[derive(Component, Debug, Clone, Copy)]
pub struct AisleLamp {
    pub piece: PieceId,
    pub rest: u8,
    pub shows: Bulb,
}

/// The chain through a Sabotaged Wheel's spokes, padlocked to its post.
#[derive(Component, Debug, Clone, Copy)]
pub struct WheelChain {
    pub piece: PieceId,
}

/// The glTFs of the Wheel kitbashes (post, hand wheel, chain), under `assets/`.
pub const WHEEL_MODELS: [&str; 3] = ["props/wheel_post/wheel_post.gltf", "props/aisle_wheel/aisle_wheel.gltf", "props/wheel_chain/wheel_chain.gltf"];

/// What a Wheel is made of (the plain look it keeps with no model files): shared by every Wheel.
#[derive(Resource)]
struct WheelKit {
    iron: Handle<StandardMaterial>,
    brass: Handle<StandardMaterial>,
    bulbs: Vec<Handle<StandardMaterial>>,
    post_base: Handle<Mesh>,
    post_shaft: Handle<Mesh>,
    post_cap: Handle<Mesh>,
    axle: Handle<Mesh>,
    rim: Handle<Mesh>,
    spoke: Handle<Mesh>,
    hub: Handle<Mesh>,
    link: Handle<Mesh>,
    lock: Handle<Mesh>,
    dome: Handle<Mesh>,
}

fn init_wheel_kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut mesh = |m: Mesh| meshes.add(m);
    let metal = |colour: Color, metallic: f32, roughness: f32| StandardMaterial { base_color: colour, metallic, perceptual_roughness: roughness, ..default() };
    let bulbs = Bulb::ALL
        .iter()
        .map(|bulb| {
            let (colour, emissive) = bulb.look();
            materials.add(StandardMaterial { base_color: colour, emissive, perceptual_roughness: 0.3, ..default() })
        })
        .collect();
    commands.insert_resource(WheelKit {
        iron: materials.add(metal(Color::srgb(0.1, 0.1, 0.11), 0.8, 0.55)),
        brass: materials.add(metal(BRASS, 0.9, 0.4)),
        bulbs,
        post_base: mesh(Cylinder::new(0.15, 0.1).into()),
        post_shaft: mesh(Cylinder::new(0.07, POST_HEIGHT - 0.14).into()),
        post_cap: mesh(Cylinder::new(0.1, 0.04).into()),
        axle: mesh(Cylinder::new(0.025, 0.25).into()),
        rim: mesh(Torus::new(WHEEL_RADIUS - 0.02, WHEEL_RADIUS + 0.02).into()),
        spoke: mesh(Cuboid::new(0.04, WHEEL_RADIUS - 0.04, 0.03).into()),
        hub: mesh(Cylinder::new(0.06, 0.08).into()),
        link: mesh(Cuboid::new(0.05, 0.022, 0.022).into()),
        lock: mesh(Cuboid::new(0.07, 0.09, 0.035).into()),
        dome: mesh(Sphere::new(0.08).into()),
    });
}

/// Gives every Wheel its look over the plain post the Shifting plugin spawned: that keeps the Buyable and its use point but no
/// drawing of its own (it stands hidden at the open Aisle); the look stands at every mouth, its lamp telling which is open.
fn dress_wheels(
    mut commands: Commands,
    kit: Res<WheelKit>,
    asset_server: Option<Res<AssetServer>>,
    scenes: Option<Res<Assets<WorldAsset>>>,
    posts: Query<(Entity, &WheelPost, &Transform), Added<WheelPost>>,
) {
    for (entity, post, transform) in &posts {
        commands.entity(entity).remove::<Mesh3d>().despawn_related::<Children>();
        let root = commands
            .spawn((
                InRun,
                Name::new("Wheel look"),
                WheelLook,
                Transform::from_translation(transform.translation - Vec3::Y * 0.5).with_rotation(transform.rotation),
                Visibility::default(),
            ))
            .id();
        // A kitbash glTF in place of a part's plain look, if it is on disk (and there is a world to load it): spawned under `parent`.
        let model = |commands: &mut Commands, path: &str, parent: Entity| -> bool {
            let Some(server) = asset_server.as_ref().filter(|_| scenes.is_some() && on_disk(path)) else { return false };
            commands.spawn((Name::new("Model"), WorldAssetRoot(server.load(GltfAssetLabel::Scene(0).from_asset(path.to_string()))), Transform::default(), Visibility::default(), ChildOf(parent)));
            true
        };
        let part = |commands: &mut Commands, parent: Entity, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3, rot: Quat| {
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::from_translation(at).with_rotation(rot), ChildOf(parent)));
        };
        let turn_x = Quat::from_rotation_x(FRAC_PI_2);
        // The post, with its axle.
        if !model(&mut commands, WHEEL_MODELS[0], root) {
            part(&mut commands, root, &kit.post_base, &kit.iron, Vec3::new(0.0, 0.05, 0.0), Quat::IDENTITY);
            part(&mut commands, root, &kit.post_shaft, &kit.iron, Vec3::new(0.0, 0.1 + (POST_HEIGHT - 0.14) / 2.0, 0.0), Quat::IDENTITY);
            part(&mut commands, root, &kit.post_cap, &kit.brass, Vec3::new(0.0, POST_HEIGHT - 0.04, 0.0), Quat::IDENTITY);
            part(&mut commands, root, &kit.axle, &kit.iron, Vec3::new(HUB_AT.x, HUB_AT.y, HUB_AT.z / 2.0 + 0.05), turn_x);
        }
        // The hand wheel: a pivot at its hub, turned about the way the Wheel faces.
        let pivot = commands
            .spawn((Name::new("Hand wheel"), WheelSpin { piece: post.piece }, Transform::from_translation(HUB_AT), Visibility::default(), ChildOf(root)))
            .id();
        if !model(&mut commands, WHEEL_MODELS[1], pivot) {
            part(&mut commands, pivot, &kit.rim, &kit.iron, Vec3::ZERO, turn_x);
            part(&mut commands, pivot, &kit.hub, &kit.brass, Vec3::ZERO, turn_x);
            for k in 0..3 {
                let a = k as f32 * TAU / 3.0;
                part(&mut commands, pivot, &kit.spoke, &kit.iron, Quat::from_rotation_z(a) * Vec3::Y * (WHEEL_RADIUS / 2.0 + 0.02), Quat::from_rotation_z(a));
            }
        }
        // The chain through the spokes, padlocked to the post, shown only while Sabotaged.
        let chain = commands
            .spawn((Name::new("Wheel chain"), WheelChain { piece: post.piece }, Transform::from_translation(HUB_AT), Visibility::Hidden, ChildOf(root)))
            .id();
        if !model(&mut commands, WHEEL_MODELS[2], chain) {
            // A ring of links woven through the spokes (in front of one, behind the next), and a run down to the lock.
            for k in 0..12 {
                let a = k as f32 * TAU / 12.0;
                let weave = if k % 2 == 0 { 0.02 } else { -0.02 };
                part(&mut commands, chain, &kit.link, &kit.iron, Quat::from_rotation_z(a) * Vec3::Y * 0.18 + Vec3::Z * weave, Quat::from_rotation_z(a));
            }
            for k in 0..3 {
                part(&mut commands, chain, &kit.link, &kit.iron, Vec3::new(0.0, -0.22 - 0.05 * k as f32, -0.04 * (k + 1) as f32), Quat::from_rotation_z(FRAC_PI_2));
            }
            part(&mut commands, chain, &kit.lock, &kit.brass, Vec3::new(0.0, -0.4, -0.13), Quat::IDENTITY);
        }
        // The Aisle lamp: an enamel dome on the post's top, emissive only.
        commands.spawn((
            Name::new("Aisle lamp"),
            AisleLamp { piece: post.piece, rest: post.rest, shows: Bulb::DarkAmber },
            Mesh3d(kit.dome.clone()),
            MeshMaterial3d(kit.bulbs[Bulb::DarkAmber as usize].clone()),
            Transform::from_xyz(0.0, POST_HEIGHT + 0.03, 0.0).with_scale(Vec3::new(1.0, 0.8, 1.0)),
            ChildOf(root),
        ));
    }
}

/// A Piece's phase, and whether it is Sabotaged.
fn state_of(pieces: &Query<(&PieceState, Has<Sabotaged>)>, piece: PieceId) -> Option<(PiecePhase, bool)> {
    pieces.iter().find(|(state, _)| state.id() == piece).map(|(state, sabotaged)| (state.phase(), sabotaged))
}

/// Turns each hand wheel to where its Range's rail travel puts it, the same on every machine from the phase. A Sabotaged
/// Wheel is jammed and stays where it is.
fn spin_wheels(pieces: Query<(&PieceState, Has<Sabotaged>)>, mut wheels: Query<(&WheelSpin, &mut Transform)>, map: Res<MapId>) {
    let layout = map.layout();
    for (wheel, mut transform) in &mut wheels {
        let Some((phase, sabotaged)) = state_of(&pieces, wheel.piece) else { continue };
        let Some(piece) = layout.shifting.get(usize::from(wheel.piece.0)) else { continue };
        if sabotaged {
            continue;
        }
        transform.rotation = Quat::from_rotation_z(wheel_angle(rail_travel(piece, phase)));
    }
}

/// Swaps each Aisle lamp's bulb as its Range's phase, Rest and Sabotage say.
fn light_aisle_lamps(pieces: Query<(&PieceState, Has<Sabotaged>)>, kit: Res<WheelKit>, time: Res<Time>, mut lamps: Query<(&mut AisleLamp, &mut MeshMaterial3d<StandardMaterial>)>) {
    for (mut lamp, mut material) in &mut lamps {
        let Some((phase, sabotaged)) = state_of(&pieces, lamp.piece) else { continue };
        let want = bulb(aisle_lamp(phase, lamp.rest, sabotaged), time.elapsed_secs());
        if lamp.shows != want {
            lamp.shows = want;
            material.0 = kit.bulbs[want as usize].clone();
        }
    }
}

/// Shows the chain on a Sabotaged Range's Wheels.
fn show_chains(pieces: Query<(&PieceState, Has<Sabotaged>)>, mut chains: Query<(&WheelChain, &mut Visibility)>) {
    for (chain, mut visibility) in &mut chains {
        let sabotaged = state_of(&pieces, chain.piece).is_some_and(|(_, sabotaged)| sabotaged);
        visibility.set_if_neq(if sabotaged { Visibility::Inherited } else { Visibility::Hidden });
    }
}

// ---- The Roll's dust and pages ----

/// A Range has come to Rest: for the sound ticket's thud and this look's dust. `at` is the middle of the Bookcases that moved,
/// at their tops.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct RangeRested {
    pub piece: PieceId,
    pub rest: u8,
    pub at: Vec3,
}

/// A body was caught in a Range's sweep and Shoved clear, at its feet: for the sound ticket and this look's puff.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct RangeShoved {
    pub at: Vec3,
}

/// The most dust and pages alive at once.
pub const DUST_CAP: usize = 160;
/// Puffs along the top of a Bookcase that moved, as it comes to Rest.
pub const REST_PUFFS: usize = 6;
/// How many puffs a second a moving Bookcase kicks up at its foot.
pub const SKIRT_PER_SEC: f32 = 2.0;
/// How long a fallen page lies on the floor.
const PAGE_LIES_SECS: f32 = 8.0;
const DUST_TINT: Color = Color::srgba(0.55, 0.48, 0.38, 0.55);

/// A deterministic value in [0, 1) from `seed`.
pub fn hash01(seed: u64) -> f32 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn key(piece: u8, rest: u8, bookcase: usize, k: u32, field: u64) -> u64 {
    (u64::from(piece) << 56) ^ (u64::from(rest) << 48) ^ ((bookcase as u64) << 36) ^ (u64::from(k) << 20) ^ (field * 0x1_0000_0001)
}

/// One loose page's fall: where over the Bookcase's top it starts (a fraction of its length and depth from the middle), how it is
/// turned, its sway, its fall speed and how long it waits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSpec {
    pub along: f32,
    pub across: f32,
    pub turn: f32,
    pub sway: f32,
    pub fall: f32,
    pub delay: f32,
}

/// The 3 to 6 loose pages that flutter down from Bookcase `bookcase` of Piece `piece` as it comes to Rest `rest`: picked by a hash of
/// those alone, so every machine sees the same ones.
pub fn rest_pages(piece: u8, rest: u8, bookcase: usize) -> Vec<PageSpec> {
    let count = (3 + (hash01(key(piece, rest, bookcase, 99, 1)) * 4.0) as u32).min(6);
    (0..count)
        .map(|k| {
            let at = |field: u64| hash01(key(piece, rest, bookcase, k, field));
            PageSpec { along: at(2) - 0.5, across: (at(3) - 0.5) * 0.8, turn: at(4) * TAU, sway: at(5) * TAU, fall: 0.5 + at(6) * 0.4, delay: at(7) * 0.5 }
        })
        .collect()
}

/// A Bookcase's top at `pose`: its middle, the way its length runs, and how long it is.
fn top_of(leaf: &Leaf, pose: f32) -> (Vec3, Vec3, f32) {
    let centre = leaf.centre(pose);
    let along = if leaf.size.x >= leaf.size.z { Vec3::X } else { Vec3::Z };
    (centre + Vec3::Y * leaf.size.y / 2.0, along, leaf.size.x.max(leaf.size.z))
}

/// The Bookcases of `piece` that move between Rests `from` and `to`.
pub fn moved(piece: &Piece, from: u8, to: u8) -> Vec<usize> {
    (0..piece.leaves.len()).filter(|&i| (piece.rests[usize::from(from)].at[i] - piece.rests[usize::from(to)].at[i]).abs() > 1e-4).collect()
}

/// What dust is drawn with, shared by every puff and page.
#[derive(Resource)]
struct DustKit {
    quad: Handle<Mesh>,
    puff: Handle<StandardMaterial>,
    page: Handle<StandardMaterial>,
}

fn init_dust_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Option<Res<AssetServer>>,
    images: Option<Res<Assets<Image>>>,
) {
    // The `smoke_03` sprite and `Paper006`, once on disk; plain tints without them (and in headless runs).
    let load = |path: &str| asset_server.as_ref().filter(|_| images.is_some() && on_disk(path)).map(|server| server.load::<Image>(path.to_string()));
    commands.insert_resource(DustKit {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        puff: materials.add(StandardMaterial {
            base_color: DUST_TINT,
            base_color_texture: load("sprites/smoke_03/color.ktx2"),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
        page: materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.76, 0.62),
            base_color_texture: load("materials/paper006/color.ktx2"),
            perceptual_roughness: 0.95,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    });
}

#[derive(Debug, Clone, Copy)]
enum Bit {
    /// A billowing puff of dust, rising.
    Puff { rise: f32 },
    /// A page falling, swaying; once it reaches `floor` it lies there for `landed` seconds.
    Page { floor: f32, fall: f32, sway: f32, turn: f32, landed: f32 },
}

#[derive(Component)]
struct DustBit {
    age: f32,
    delay: f32,
    life: f32,
    size: f32,
    bit: Bit,
}

fn spawn_puff(commands: &mut Commands, kit: &DustKit, at: Vec3, size: f32, life: f32, rise: f32, delay: f32) {
    commands.spawn((
        InRun,
        Name::new("Dust"),
        DustBit { age: 0.0, delay, life, size, bit: Bit::Puff { rise } },
        Mesh3d(kit.quad.clone()),
        MeshMaterial3d(kit.puff.clone()),
        Transform::from_translation(at).with_scale(Vec3::splat(0.01)),
        Visibility::default(),
    ));
}

fn spawn_page(commands: &mut Commands, kit: &DustKit, at: Vec3, floor: f32, spec: PageSpec) {
    commands.spawn((
        InRun,
        Name::new("Page"),
        DustBit { age: 0.0, delay: spec.delay, life: 30.0, size: 1.0, bit: Bit::Page { floor, fall: spec.fall, sway: spec.sway, turn: spec.turn, landed: 0.0 } },
        Mesh3d(kit.quad.clone()),
        MeshMaterial3d(kit.page.clone()),
        Transform::from_translation(at).with_rotation(Quat::from_rotation_y(spec.turn) * Quat::from_rotation_x(-FRAC_PI_2)).with_scale(Vec3::new(0.21, 0.3, 1.0)),
        Visibility::Hidden,
    ));
}

/// Dust and pages as a Range rolls and comes to Rest, drawn on every machine from the phase it shows: a burst of dust along the top of each
/// Bookcase that moved and 3 to 6 pages fluttering down from it; a skirt of dust at the foot of each moving Bookcase, twice a second. A
/// Piece is only seen to land if this machine saw it Swing, so a late joiner is not shown a burst for a Roll it was not at.
fn roll_effects(
    mut commands: Commands,
    kit: Res<DustKit>,
    map: Res<MapId>,
    pieces: Query<&PieceState>,
    live: Query<(), With<DustBit>>,
    mut seen: Local<HashMap<PieceId, PiecePhase>>,
    mut ticks: Local<HashMap<PieceId, i64>>,
    mut rested: MessageWriter<RangeRested>,
) {
    let layout = map.layout();
    let mut alive = live.iter().len();
    for state in &pieces {
        let id = state.id();
        let Some(piece) = layout.shifting.get(usize::from(id.0)) else { continue };
        if !matches!(piece.driver, Driver::Wheels(_)) {
            continue;
        }
        let before = seen.insert(id, state.phase());
        match (before, state.phase()) {
            (Some(PiecePhase::Swinging { from, to, .. }), now) if !now.swinging() => {
                ticks.remove(&id);
                let rest = now.standing().unwrap_or(to);
                let bookcases = moved(piece, from, to);
                let mut middle = Vec3::ZERO;
                for &i in &bookcases {
                    let leaf = &piece.leaves[i];
                    let (top, along, length) = top_of(leaf, piece.rests[usize::from(rest)].at[i]);
                    middle += top / bookcases.len() as f32;
                    for k in 0..REST_PUFFS {
                        if alive >= DUST_CAP {
                            break;
                        }
                        let h = |field: u64| hash01(key(id.0, rest, i, k as u32, field + 20));
                        let spot = top + along * ((k as f32 + 0.5) / REST_PUFFS as f32 - 0.5) * length + Vec3::new(h(1) - 0.5, 0.0, h(2) - 0.5) * 0.3;
                        spawn_puff(&mut commands, &kit, spot, 0.6 + h(3) * 0.4, 1.4 + h(4) * 0.6, 0.35 + h(5) * 0.2, h(6) * 0.15);
                        alive += 1;
                    }
                    let across = along.cross(Vec3::Y);
                    for spec in rest_pages(id.0, rest, i) {
                        if alive >= DUST_CAP {
                            break;
                        }
                        spawn_page(&mut commands, &kit, top + along * spec.along * length + across * spec.across * leaf.size.x.min(leaf.size.z), leaf.hinge.y + 0.012, spec);
                        alive += 1;
                    }
                }
                if !bookcases.is_empty() {
                    rested.write(RangeRested { piece: id, rest, at: middle });
                }
            }
            (_, PiecePhase::Swinging { from, to, elapsed }) => {
                let tick = (elapsed * SKIRT_PER_SEC).floor() as i64;
                let last = ticks.insert(id, tick).unwrap_or(tick - 1);
                let poses = state.phase().poses(piece);
                for t in (last + 1).max(tick - 1)..=tick {
                    for i in moved(piece, from, to) {
                        if alive >= DUST_CAP {
                            break;
                        }
                        let leaf = &piece.leaves[i];
                        let h = |field: u64| hash01(key(id.0, from, i, t as u32, field + 40) ^ u64::from(to));
                        let (top, along, length) = top_of(leaf, poses[i]);
                        let side = if h(1) < 0.5 { -1.0 } else { 1.0 };
                        let across = along.cross(Vec3::Y);
                        let foot = Vec3::new(top.x, leaf.hinge.y + 0.1, top.z) + along * (h(2) - 0.5) * length + across * side * leaf.size.x.min(leaf.size.z) / 2.0;
                        spawn_puff(&mut commands, &kit, foot, 0.4 + h(3) * 0.2, 0.9 + h(4) * 0.4, 0.2, 0.0);
                        alive += 1;
                    }
                }
            }
            _ => {
                ticks.remove(&id);
            }
        }
    }
}

/// A small puff at the feet of every Player and Zombie a Roll Shoves, as the Roll catches it: seen the same way on every machine, from
/// who is in the sweep of a Range that is Swinging.
fn shove_puffs(
    mut commands: Commands,
    kit: Res<DustKit>,
    map: Res<MapId>,
    pieces: Query<&PieceState>,
    zombies: Query<(Entity, &Zombie, &Transform), (Without<Dying>, Without<Player>)>,
    players: Query<(Entity, &Transform, Option<&Locomotion>), With<Player>>,
    live: Query<(), With<DustBit>>,
    mut shoved: Local<HashSet<Entity>>,
    mut shoves: MessageWriter<RangeShoved>,
) {
    let layout = map.layout();
    let mut any = false;
    let mut alive = live.iter().len();
    for state in &pieces {
        let PiecePhase::Swinging { from, to, .. } = state.phase() else { continue };
        let Some(piece) = layout.shifting.get(usize::from(state.id().0)) else { continue };
        if !matches!(piece.driver, Driver::Wheels(_)) {
            continue;
        }
        any = true;
        let bodies = zombies.iter().map(|(entity, zombie, transform)| (entity, transform.translation - Vec3::Y * zombie.half_height())).chain(
            players.iter().map(|(entity, transform, locomotion)| (entity, transform.translation - Vec3::Y * half_height_above_feet(locomotion))),
        );
        for (entity, feet) in bodies {
            if piece.in_way(feet, from, to) && shoved.insert(entity) {
                shoves.write(RangeShoved { at: feet });
                if alive < DUST_CAP {
                    spawn_puff(&mut commands, &kit, feet + Vec3::Y * 0.15, 0.45, 0.8, 0.3, 0.0);
                    alive += 1;
                }
            }
        }
    }
    if !any {
        shoved.clear();
    }
}

/// Dust billows and thins, and turns to the camera; a page falls swaying and then lies for a while.
fn update_dust(mut commands: Commands, mut bits: Query<(Entity, &mut DustBit, &mut Transform, &mut Visibility)>, cameras: Query<&GlobalTransform, With<Camera3d>>, time: Res<Time>) {
    let dt = time.delta_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (entity, mut bit, mut transform, mut visibility) in &mut bits {
        if bit.delay > 0.0 {
            bit.delay -= dt;
            continue;
        }
        bit.age += dt;
        match bit.bit {
            Bit::Puff { rise } => {
                if bit.age >= bit.life {
                    commands.entity(entity).despawn();
                    continue;
                }
                transform.translation.y += rise * dt;
                transform.scale = Vec3::splat(bit.size * envelope(BlastShape::Billow, bit.age / bit.life));
                if let Some(eye) = eye
                    && (eye - transform.translation).length_squared() > 1e-6
                {
                    transform.rotation = Transform::IDENTITY.looking_to(transform.translation - eye, Vec3::Y).rotation;
                }
            }
            Bit::Page { floor, fall, sway, turn, mut landed } => {
                visibility.set_if_neq(Visibility::Inherited);
                if landed > 0.0 {
                    landed += dt;
                    if landed >= PAGE_LIES_SECS {
                        commands.entity(entity).despawn();
                        continue;
                    }
                } else {
                    let t = bit.age;
                    transform.translation.y -= fall * dt;
                    transform.translation.x += (sway + t * 3.0).cos() * 0.25 * dt;
                    transform.translation.z += (sway + t * 2.3).sin() * 0.25 * dt;
                    transform.rotation = Quat::from_rotation_y(turn + t * 0.8) * Quat::from_rotation_x(-FRAC_PI_2 + 0.7 * (sway + t * 4.5).sin());
                    if transform.translation.y <= floor {
                        transform.translation.y = floor;
                        transform.rotation = Quat::from_rotation_y(turn) * Quat::from_rotation_x(-FRAC_PI_2);
                        landed = dt;
                    }
                }
                bit.bit = Bit::Page { floor, fall, sway, turn, landed };
            }
        }
    }
}

pub struct StacksLookPlugin;

impl Plugin for StacksLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RangeRested>()
            .add_message::<RangeShoved>()
            .add_systems(Update, (dress_bookcases, load_book_rows).chain().run_if(world_runs).run_if(resource_exists::<MapId>))
            .add_systems(Update, (init_wheel_kit.run_if(not(resource_exists::<WheelKit>)), init_dust_kit.run_if(not(resource_exists::<DustKit>))).run_if(world_runs).run_if(resource_exists::<MapId>))
            .add_systems(
                Update,
                (dress_wheels, spin_wheels, light_aisle_lamps, show_chains, roll_effects, shove_puffs, update_dust)
                    .chain()
                    .in_set(RangeLookSet)
                    .after(crate::authority::HostSimulation)
                    .after(init_wheel_kit)
                    .after(init_dust_kit)
                    .run_if(world_runs)
                    .run_if(resource_exists::<MapId>)
                    .run_if(resource_exists::<WheelKit>)
                    .run_if(resource_exists::<DustKit>),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    fn parts(items: &[Item], part: Part) -> Vec<&Item> {
        items.iter().filter(|item| item.part == part).collect()
    }

    fn lowest(items: &[Item], part: Part) -> f32 {
        parts(items, part).iter().map(|item| item.bottom()).fold(f32::MAX, f32::min)
    }

    #[test]
    fn bookcase_and_fixed_stack_looks_differ() {
        let bookcase = bookcase(SIZE, "A \u{b7} 3");
        let stack = fixed_stack(SIZE);
        // The carcass: the same wood, the stack's tinted 0.7.
        assert_eq!(Part::Carcass.finish().look().material, Material::WoodTableWorn);
        assert_eq!(Part::StackCarcass.finish().look().material, Material::WoodTableWorn);
        assert_eq!(Part::Carcass.finish().look().tint, Color::WHITE);
        assert_eq!(Part::StackCarcass.finish().look().tint, Color::srgb(0.7, 0.7, 0.7));
        // The foot: a Bookcase stands 0.04 m clear on four dark castor blocks and has no plinth; a stack stands on a 0.15 m
        // stone plinth with four iron bolts and no castors.
        assert!((lowest(&bookcase, Part::Carcass) - CASTOR_GAP).abs() < 1e-5);
        assert_eq!(parts(&bookcase, Part::Castor).len(), 4);
        assert!(parts(&bookcase, Part::Castor).iter().all(|castor| castor.size.y == CASTOR_GAP && castor.bottom().abs() < 1e-5));
        assert!(parts(&bookcase, Part::Plinth).is_empty() && parts(&bookcase, Part::Bolt).is_empty());
        let plinth = parts(&stack, Part::Plinth);
        assert_eq!(plinth.len(), 1);
        assert_eq!(plinth[0].size.y, PLINTH);
        assert_eq!(Part::Plinth.finish().look().material, Material::MonasteryStoneFloor);
        assert_eq!(parts(&stack, Part::Bolt).len(), 4);
        assert!(parts(&stack, Part::Castor).is_empty());
        assert!((lowest(&stack, Part::StackCarcass) - PLINTH).abs() < 1e-5);
        // The end panel: a Bookcase's is `dark_paneled_wood` under a brass frame and a brass plate; a stack's is plain and dark.
        assert_eq!(parts(&bookcase, Part::EndPanel).len(), 2);
        assert_eq!(Part::EndPanel.finish().look().material, Material::DarkPaneledWood);
        assert_eq!(parts(&bookcase, Part::Frame).len(), 8);
        assert_eq!(parts(&bookcase, Part::Plate).len(), 2);
        assert!(!parts(&bookcase, Part::Ink).is_empty());
        assert!(parts(&stack, Part::EndPanel).is_empty() && parts(&stack, Part::Frame).is_empty() && parts(&stack, Part::Plate).is_empty() && parts(&stack, Part::Ink).is_empty());
        assert_eq!(parts(&stack, Part::PlainEnd).len(), 2);
        assert_ne!(Part::PlainEnd.finish().look().tint, Part::EndPanel.finish().look().tint);
        assert_eq!(Part::Plate.finish().look().material, Material::MetalPlate02);
        // Whatever they wear, both stand inside their own box.
        for items in [&bookcase, &stack] {
            for item in items.iter() {
                let (reach, at) = (item.extent(), item.at);
                assert!(at.x.abs() + reach.x <= SIZE.x / 2.0 + 0.07 && at.z.abs() + reach.z <= SIZE.z / 2.0 + 0.07, "{item:?} sticks out");
                assert!(at.y + reach.y <= SIZE.y + 1e-4 && item.bottom() >= -1e-4, "{item:?} is off its foot or over its top");
            }
        }
    }

    /// Everything the stack look built, as a sorted list: what a peer would see of it.
    fn fingerprint(sim: &mut crate::enemies::zombie::headless::Sim) -> Vec<String> {
        use bevy::ecs::system::RunSystemOnce;
        sim.world_mut()
            .run_system_once(|looks: Query<(&Name, &Transform, Option<&Children>, Option<&Mesh3d>)>, faces: Query<&BookFace>| {
                let mut out: Vec<String> = looks
                    .iter()
                    .filter(|(name, ..)| matches!(name.as_str(), "Stack look" | "Fixed stack" | "Bookcase look" | "Gate grille" | "Stack part" | "Grille bar" | "Book rows"))
                    .map(|(name, transform, children, mesh)| {
                        let (kids, variants): (usize, Vec<usize>) =
                            children.map_or((0, vec![]), |children| (children.len(), children.iter().filter_map(|child| faces.get(child).ok()).map(|face| face.variant).collect()));
                        format!("{name} {:.3?} {:.3?} {kids} {variants:?} {}", transform.translation, transform.rotation, mesh.is_some())
                    })
                    .collect();
                out.sort();
                out
            })
            .unwrap()
    }

    #[test]
    fn every_peer_dresses_the_same_bookcases_from_the_layout() {
        use crate::enemies::zombie::headless::Sim;
        use crate::maps::shifting::LeafBody;
        use bevy::ecs::system::RunSystemOnce;
        let build = || {
            let mut sim = Sim::on(MapId::Archive, 60.0, |app| {
                app.add_plugins(StacksLookPlugin);
            });
            sim.run(3);
            sim
        };
        let (mut host, mut client) = (build(), build());
        // Every Leaf of every Range is a Bookcase now: one look root under its body (the plain slab and rails are gone), with
        // its castors, ends and plate, and both faces of the shelf fill, and the body still has its collider.
        let layout = MapId::Archive.layout();
        let bookcases = layout.shifting.iter().map(|piece| piece.leaves.len()).sum::<usize>();
        assert_eq!(bookcases, 14);
        let looks = host
            .world_mut()
            .run_system_once(|bodies: Query<(&LeafBody, &Children, Has<avian3d::prelude::Collider>)>, roots: Query<&Children, With<Transform>>, faces: Query<&BookFace>, textured: Query<&Textured>| {
                bodies
                    .iter()
                    .map(|(body, children, solid)| {
                        let root = roots.get(children[0]).unwrap();
                        let parts = root.iter().filter(|&child| textured.get(child).is_ok()).count();
                        let variants: Vec<usize> = root.iter().filter_map(|child| faces.get(child).ok()).map(|face| face.variant).collect();
                        (body.piece.0, body.index, children.len(), solid, parts, variants)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
        assert_eq!(looks.len(), 14);
        for &(piece, index, children, solid, parts, ref variants) in &looks {
            assert_eq!(children, 1, "Range {piece}'s Bookcase {index} keeps only its look");
            assert!(solid);
            // Carcass, ends, brass and ink for the plate, castors.
            assert_eq!(parts, 5, "Range {piece}'s Bookcase {index}");
            assert_eq!(variants, &[book_variant(index, true), book_variant(index, false)]);
        }
        // The Host's build and a client's are the same, entity for entity: nothing here is sent, and nothing depends on who runs it.
        let (a, b) = (fingerprint(&mut host), fingerprint(&mut client));
        assert!(a.len() > 100, "{}", a.len());
        assert_eq!(a, b);
    }

    #[test]
    fn a_bookcases_plate_reads_on_both_ends() {
        let items = bookcase(SIZE, "B \u{b7} 2");
        for side in [-1.0f32, 1.0] {
            let plate: Vec<_> = parts(&items, Part::Plate).into_iter().filter(|item| item.at.x * side > 0.0).collect();
            assert_eq!(plate.len(), 1, "one plate on the {side} end");
            // At eye height, on the end's face, with the lettering standing out of it.
            assert!((plate[0].at.y - 1.6).abs() < 1e-5);
            assert!(plate[0].at.x.abs() > SIZE.x / 2.0);
            assert!(parts(&items, Part::Ink).iter().filter(|ink| ink.at.x * side > 0.0).all(|ink| ink.at.x.abs() > plate[0].at.x.abs()));
        }
    }

    #[test]
    fn book_rows_variants_alternate_so_neighbours_never_match() {
        assert_eq!(BOOK_VARIANTS, 3);
        for nth in 0..40 {
            // A stack's two faces differ, and its front never matches the back of the stack it touches.
            assert_ne!(book_variant(nth, true), book_variant(nth, false), "stack {nth}'s own faces");
            assert_ne!(book_variant(nth, true), book_variant(nth + 1, false), "stack {nth} and {}", nth + 1);
        }
        // All three variants are in use along a row, and each is a file.
        let used: std::collections::HashSet<_> = (0..4).flat_map(|nth| [book_variant(nth, true), book_variant(nth, false)]).collect();
        assert_eq!(used.len(), 3);
        let paths: std::collections::HashSet<_> = (0..3).map(book_rows_path).collect();
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn the_rail_covers_its_length_plus_a_little_and_stops_at_both_ends() {
        let items = rail(6.6);
        let strips = parts(&items, Part::Rail);
        assert_eq!(strips.len(), 2);
        for strip in strips {
            assert!((strip.size.x - (6.6 + RAIL_EXTRA)).abs() < 1e-5);
            assert!(strip.at.x - strip.size.x / 2.0 <= -0.14 && strip.at.x + strip.size.x / 2.0 >= 6.6 + 0.14);
            assert_eq!(strip.size.z, RAIL_STRIP);
            assert!((strip.at.z.abs() - RAIL_GAUGE / 2.0).abs() < 1e-5);
            // Flush: it stands a few millimetres over the floor, no more.
            assert!(strip.at.y + strip.size.y / 2.0 < 0.01);
        }
        let stops = parts(&items, Part::Stop);
        assert_eq!(stops.len(), 2);
        assert!(stops.iter().all(|stop| stop.size.x == 0.2));
        // The castors stand on the strips.
        for castor in parts(&bookcase(SIZE, "A \u{b7} 1"), Part::Castor) {
            assert!((castor.at.x.abs() - RAIL_GAUGE / 2.0).abs() < 1e-5);
        }
    }

    #[test]
    fn lettering_spells_what_the_plates_and_soffits_say() {
        for c in "GREATRANGEABCD1234 \u{b7}".chars() {
            assert!(c == ' ' || !glyph(c).is_empty(), "{c:?} has no glyph");
        }
        let great = soffit("GREAT RANGE", 0.4);
        assert!(great.iter().all(|item| item.part == Part::Letter));
        // Centred on the wall, over the soffit's 3.0 m.
        let (lo, hi) = great.iter().fold((f32::MAX, f32::MIN), |(lo, hi), item| (lo.min(item.at.x - item.extent().x), hi.max(item.at.x + item.extent().x)));
        assert!((lo + hi).abs() < 0.05, "centred: {lo} to {hi}");
        assert!(hi - lo < 5.0, "narrower than the opening");
        assert!(great.iter().all(|item| item.at.y > BOOKCASE_HEIGHT));
        assert_eq!(soffit_label(0), "GREAT RANGE");
        assert_eq!((1..5).map(soffit_label).collect::<Vec<_>>(), ["A", "B", "C", "D"]);
    }

    #[test]
    fn a_face_looking_along_a_wall_normal_has_its_text_the_right_way_round() {
        // Seen from +z a plate's x runs right; from +x it runs along -z; and so on.
        for (out, right) in [(Vec3::Z, Vec3::X), (Vec3::X, Vec3::NEG_Z), (Vec3::NEG_X, Vec3::Z), (Vec3::NEG_Z, Vec3::NEG_X)] {
            let turn = facing(out);
            assert!((turn * Vec3::X - right).length() < 1e-5 && (turn * Vec3::Z - out).length() < 1e-5 && (turn * Vec3::Y - Vec3::Y).length() < 1e-5, "{out}");
        }
    }

    #[test]
    fn the_stacks_frame_lies_along_its_boxes_long_axis() {
        // A fixed stack is 1.2 x 3 x 5: long along z; a Bookcase on an x rail is the same, on a z rail 5 x 3 x 1.2.
        assert_eq!(lay(Vec3::new(1.2, 3.0, 5.0)) * Vec3::X, Quat::from_rotation_y(FRAC_PI_2) * Vec3::X);
        assert!((lay(Vec3::new(1.2, 3.0, 5.0)) * Vec3::X - Vec3::NEG_Z).length() < 1e-5);
        assert!((lay(Vec3::new(1.2, 3.0, 5.0)) * Vec3::Z - Vec3::X).length() < 1e-5, "+z stays the rail's high side");
        assert_eq!(lay(Vec3::new(5.0, 3.0, 1.2)), Quat::IDENTITY);
    }

    // ---- The Wheels, Aisle lamps and the Roll's dust (#538) ----

    use crate::maps::shifting::pieces;
    use crate::tuning::SWING_SECS;

    #[test]
    fn aisle_lamp_by_phase_and_rest() {
        let phases = |at: u8, to: u8| {
            [
                PiecePhase::Rest(at),
                PiecePhase::Warning { from: at, to, elapsed: 0.7 },
                PiecePhase::Waiting { from: at, to },
                PiecePhase::Swinging { from: at, to, elapsed: 2.0 },
                PiecePhase::Cooling { at: to, elapsed: 1.0 },
            ]
        };
        for rest in 0..5u8 {
            for phase in phases(2, 3) {
                let lamp = aisle_lamp(phase, rest, false);
                // Green only at the open Aisle while idle.
                assert_eq!(lamp == LampState::Open, phase == PiecePhase::Rest(rest), "{phase:?} at the Aisle of Rest {rest}");
                for now in [0.0, 0.1, 0.3, 0.6, 1.2] {
                    assert_eq!(bulb(lamp, now) == Bulb::Green, lamp == LampState::Open);
                }
                // Amber through Warning, Waiting and Swinging, whichever Aisle it is.
                let amber = |lamp| matches!(bulb(lamp, 0.0), Bulb::Amber);
                match phase {
                    PiecePhase::Warning { .. } | PiecePhase::Waiting { .. } | PiecePhase::Swinging { .. } => {
                        assert!(amber(lamp) || lamp == LampState::Warning, "{phase:?}");
                        assert!([Bulb::Amber, Bulb::DarkAmber].contains(&bulb(lamp, 0.3)));
                    }
                    PiecePhase::Cooling { .. } => assert_eq!(bulb(lamp, 0.3), Bulb::AmberDim),
                    PiecePhase::Rest(_) => {}
                }
                // Red when Sabotaged, in any phase.
                let sabotaged = aisle_lamp(phase, rest, true);
                assert_eq!(sabotaged, LampState::Sabotaged);
                assert_eq!(bulb(sabotaged, 0.3), Bulb::Red);
            }
        }
        assert_eq!(bulb(aisle_lamp(PiecePhase::Rest(2), 2, false), 0.0), Bulb::Green);
        // A shut Aisle's lamp is dark amber and unlit.
        assert_eq!(bulb(aisle_lamp(PiecePhase::Rest(2), 0, false), 0.0), Bulb::DarkAmber);
        assert_eq!(Bulb::DarkAmber.look().1, LinearRgba::BLACK);
        // The warning flashes at 2 Hz: lit half of each half-second.
        let warning = aisle_lamp(PiecePhase::Waiting { from: 2, to: 0 }, 0, false);
        assert_eq!([0.0, 0.2, 0.3, 0.45, 0.5, 0.7, 0.8, 1.0].map(|now| bulb(warning, now)), [Bulb::Amber, Bulb::Amber, Bulb::DarkAmber, Bulb::DarkAmber, Bulb::Amber, Bulb::Amber, Bulb::DarkAmber, Bulb::Amber]);
        // Steady amber rolling, dim amber cooling: ten times and twice the glow of the base.
        assert!(Bulb::Amber.look().1.red > Bulb::AmberDim.look().1.red * 4.0 && Bulb::AmberDim.look().1.red > 0.0);
        // Every bulb is a distinct look, so a lamp swaps between shared materials.
        let looks: Vec<_> = Bulb::ALL.iter().map(|bulb| format!("{:?}", bulb.look())).collect();
        assert!(looks.iter().enumerate().all(|(i, look)| !looks[..i].contains(look)));
        // At Rest, on every Range, exactly the two Wheels of the open Aisle are green.
        for (id, piece) in pieces(MapId::Archive.layout()) {
            for rest in 0..piece.rests.len() as u8 {
                let green = piece.wheels().iter().filter(|wheel| aisle_lamp(PiecePhase::Rest(rest), wheel.rest, false) == LampState::Open).count();
                assert_eq!(green, 2, "Range {} at Rest {rest}", id.0);
            }
        }
    }

    #[test]
    fn wheel_spin_follows_rail_travel() {
        // One turn for every half metre of rail.
        assert!((wheel_angle(WHEEL_PITCH) - TAU).abs() < 1e-5 && wheel_angle(0.0) == 0.0 && (wheel_angle(0.25) - PI).abs() < 1e-5);
        for (id, piece) in pieces(MapId::Archive.layout()) {
            let travel = |phase| rail_travel(piece, phase);
            let hop = f32::from(1u8) * (travel(PiecePhase::Rest(1)) - travel(PiecePhase::Rest(0)));
            // One hop is one Bookcase crossing an Aisle's width, a whole number of turns: so a Wheel lands looking as it did.
            assert!((hop - crate::tuning::AISLE_WIDTH).abs() < 1e-4, "Range {}", id.0);
            let turns = wheel_angle(hop) / TAU;
            assert!((turns - turns.round()).abs() < 1e-4, "{turns} turns a hop");
            assert_eq!(travel(PiecePhase::Rest(0)), 0.0);
            // Every Rest sits a hop further; Warning, Waiting and Cooling stand still where they are.
            for rest in 0..piece.rests.len() as u8 {
                assert!((travel(PiecePhase::Rest(rest)) - hop * f32::from(rest)).abs() < 1e-4);
                assert_eq!(travel(PiecePhase::Cooling { at: rest, elapsed: 2.0 }), travel(PiecePhase::Rest(rest)));
            }
            let last = piece.rests.len() as u8 - 1;
            assert_eq!(travel(PiecePhase::Waiting { from: 1, to: last }), travel(PiecePhase::Rest(1)));
            assert_eq!(travel(PiecePhase::Warning { from: 1, to: last, elapsed: 1.0 }), travel(PiecePhase::Rest(1)));
            // Rolling on, it follows the Bookcases' own smoothstep from one Rest's travel to the next hop's, and lands exactly.
            let mut was = travel(PiecePhase::Swinging { from: 0, to: 1, elapsed: 0.0 });
            assert_eq!(was, 0.0);
            for step in 1..=20 {
                let now = travel(PiecePhase::Swinging { from: 0, to: 1, elapsed: SWING_SECS * step as f32 / 20.0 });
                assert!(now >= was - 1e-6, "forward, never back: {was} then {now}");
                was = now;
            }
            assert!((was - hop).abs() < 1e-4, "lands a hop on");
            assert!((travel(PiecePhase::Swinging { from: 0, to: 1, elapsed: SWING_SECS / 2.0 }) - hop / 2.0).abs() < 1e-4, "half way through the roll, half a hop");
            // And back: the Wheel turns the other way.
            assert!((travel(PiecePhase::Swinging { from: 1, to: 0, elapsed: SWING_SECS / 2.0 }) - hop / 2.0).abs() < 1e-4);
            assert!(travel(PiecePhase::Swinging { from: 1, to: 0, elapsed: SWING_SECS * 0.75 }) < travel(PiecePhase::Swinging { from: 1, to: 0, elapsed: SWING_SECS * 0.25 }));
            // A Roll over several Rests is still one hop of each Bookcase that moves.
            assert!(travel(PiecePhase::Swinging { from: 0, to: last, elapsed: SWING_SECS }) <= hop + 1e-4);
        }
    }

    #[test]
    fn rest_pages_are_the_same_on_every_machine() {
        let layout = MapId::Archive.layout();
        let mut seen = std::collections::HashSet::new();
        for (id, piece) in pieces(layout) {
            for rest in 0..piece.rests.len() as u8 {
                for bookcase in 0..piece.leaves.len() {
                    let pages = rest_pages(id.0, rest, bookcase);
                    // 3 to 6 pages, from a hash of nothing but where and when.
                    assert!((3..=6).contains(&pages.len()), "{} pages", pages.len());
                    assert_eq!(pages, rest_pages(id.0, rest, bookcase));
                    assert!(pages.iter().all(|page| (-0.5..0.5).contains(&page.along) && (0.0..TAU).contains(&page.turn) && page.fall > 0.0));
                    seen.insert(format!("{pages:?}"));
                }
            }
        }
        assert!(seen.len() > 20, "pages differ from Bookcase to Bookcase and Rest to Rest");
        // The hash is spread over [0, 1).
        let mean = (0..2000).map(|i| hash01(i as u64)).sum::<f32>() / 2000.0;
        assert!((mean - 0.5).abs() < 0.05 && (0..2000).all(|i| (0.0..1.0).contains(&hash01(i))));
    }

    fn show(sim: &mut crate::enemies::zombie::headless::Sim, id: u8, phase: PiecePhase) {
        use bevy::ecs::system::RunSystemOnce;
        sim.world_mut()
            .run_system_once(move |mut states: Query<&mut PieceState>| {
                for mut state in &mut states {
                    if state.id() == PieceId(id) {
                        state.show(phase);
                    }
                }
            })
            .unwrap();
    }

    fn named(sim: &mut crate::enemies::zombie::headless::Sim, what: &'static str) -> Vec<String> {
        use bevy::ecs::system::RunSystemOnce;
        sim.world_mut()
            .run_system_once(move |bits: Query<(&Name, &Transform, &Visibility)>| {
                let mut out: Vec<String> = bits.iter().filter(|(name, ..)| name.as_str() == what).map(|(_, t, v)| format!("{:.4?} {:.4?} {:.4?} {v:?}", t.translation, t.rotation, t.scale)).collect();
                out.sort();
                out
            })
            .unwrap()
    }

    fn rolled_range() -> crate::enemies::zombie::headless::Sim {
        use crate::enemies::zombie::headless::Sim;
        let mut sim = Sim::on(MapId::Archive, 60.0, |app| {
            app.add_plugins(StacksLookPlugin);
        });
        sim.run(3);
        show(&mut sim, 0, PiecePhase::Rest(2));
        sim.run(2);
        show(&mut sim, 0, PiecePhase::Swinging { from: 2, to: 3, elapsed: SWING_SECS / 2.0 });
        sim.run(2);
        show(&mut sim, 0, PiecePhase::Cooling { at: 3, elapsed: 0.0 });
        sim.run(3);
        sim
    }

    #[test]
    fn every_machine_draws_the_same_lamps_dust_and_pages_from_the_phase() {
        use bevy::ecs::system::RunSystemOnce;
        let (mut host, mut client) = (rolled_range(), rolled_range());
        // The Great Range moved one Bookcase from Rest 2 to Rest 3: it came to Rest with a burst of dust along its top and
        // 3 to 6 pages, and the same on both.
        let layout = MapId::Archive.layout();
        let moved_count = moved(&layout.shifting[0], 2, 3).len();
        assert_eq!(moved_count, 1);
        let (pages, other) = (named(&mut host, "Page"), named(&mut client, "Page"));
        assert!((3..=6).contains(&pages.len()), "{} pages", pages.len());
        assert_eq!(pages, other);
        let (dust, other_dust) = (named(&mut host, "Dust"), named(&mut client, "Dust"));
        assert!(dust.len() >= REST_PUFFS, "{} puffs", dust.len());
        assert_eq!(dust, other_dust);
        let wheels = (named(&mut host, "Hand wheel"), named(&mut client, "Hand wheel"));
        assert_eq!(wheels.0.len(), 38);
        assert_eq!(wheels.0, wheels.1);
        // The Aisle lamps: the Range is cooling at Rest 3, so every lamp of it is dim amber and the others are at Rest.
        let lamps = host
            .world_mut()
            .run_system_once(|lamps: Query<&AisleLamp>| lamps.iter().map(|lamp| (lamp.piece.0, lamp.rest, lamp.shows)).collect::<Vec<_>>())
            .unwrap();
        assert_eq!(lamps.len(), 38);
        for (piece, rest, shows) in lamps {
            let start = MapId::Archive.layout().shifting[usize::from(piece)].start;
            let want = if piece == 0 { Bulb::AmberDim } else if rest == start { Bulb::Green } else { Bulb::DarkAmber };
            assert_eq!(shows, want, "Range {piece}'s lamp for Rest {rest}");
        }
        // Back at Rest, the lamp of the open Aisle is green.
        show(&mut host, 0, PiecePhase::Rest(3));
        host.run(2);
        let green = host.world_mut().run_system_once(|lamps: Query<&AisleLamp>| lamps.iter().filter(|lamp| lamp.piece.0 == 0 && lamp.shows == Bulb::Green).map(|lamp| lamp.rest).collect::<Vec<_>>()).unwrap();
        assert_eq!(green, [3, 3]);
    }

    #[test]
    fn a_roll_says_when_it_rests_and_who_it_shoved() {
        use crate::enemies::zombie::headless::Sim;
        use bevy::ecs::system::RunSystemOnce;
        let mut sim = Sim::on(MapId::Archive, 60.0, |app| {
            app.add_plugins(StacksLookPlugin);
        });
        sim.run(3);
        show(&mut sim, 0, PiecePhase::Rest(2));
        sim.run(2);
        // A Zombie in the Bookcase's way as it starts to roll.
        let zombie = sim.spawn(Vec3::new(0.3, 8.0, 12.0), None);
        sim.world_mut().get_mut::<Transform>(zombie).unwrap().translation = Vec3::new(0.3, 8.0 + 1.0, 12.0);
        show(&mut sim, 0, PiecePhase::Swinging { from: 2, to: 3, elapsed: 0.1 });
        sim.run(1);
        let shoved = sim.world_mut().resource::<Messages<RangeShoved>>().iter_current_update_messages().count();
        assert_eq!(shoved, 1, "the Zombie in the sweep is Shoved, once");
        sim.run(5);
        assert!(sim.world_mut().run_system_once(|dust: Query<&Name>| dust.iter().filter(|name| name.as_str() == "Dust").count()).unwrap() > 0, "dust at its feet and at the Bookcase's foot");
        show(&mut sim, 0, PiecePhase::Cooling { at: 3, elapsed: 0.0 });
        sim.run(1);
        let rested: Vec<_> = sim.world_mut().resource::<Messages<RangeRested>>().iter_current_update_messages().copied().collect();
        assert_eq!(rested.len(), 1);
        assert_eq!((rested[0].piece, rested[0].rest), (PieceId(0), 3));
        // At the top of the Bookcase that moved (3 m up from the Reading level's floor, on the Range's line).
        assert!((rested[0].at.y - 11.0).abs() < 0.01 && (rested[0].at.z - 12.0).abs() < 0.01, "{:?}", rested[0].at);
    }

    #[test]
    fn a_sabotaged_range_shows_its_chain_and_a_red_lamp_and_holds_its_wheel() {
        use crate::enemies::zombie::headless::Sim;
        use bevy::ecs::system::RunSystemOnce;
        let mut sim = Sim::on(MapId::Archive, 60.0, |app| {
            app.add_plugins(StacksLookPlugin);
        });
        sim.run(3);
        let chains = |sim: &mut Sim| sim.world_mut().run_system_once(|chains: Query<(&WheelChain, &Visibility)>| chains.iter().filter(|(chain, v)| chain.piece == PieceId(0) && **v != Visibility::Hidden).count()).unwrap();
        assert_eq!(chains(&mut sim), 0, "no chain while it works");
        let range = sim.world_mut().run_system_once(|states: Query<(Entity, &PieceState)>| states.iter().find(|(_, state)| state.id() == PieceId(0)).unwrap().0).unwrap();
        sim.world_mut().entity_mut(range).insert(Sabotaged);
        sim.run(2);
        assert_eq!(chains(&mut sim), 10, "a chain on each of its ten Wheels");
        let red = sim.world_mut().run_system_once(|lamps: Query<&AisleLamp>| lamps.iter().filter(|lamp| lamp.piece.0 == 0 && lamp.shows == Bulb::Red).count()).unwrap();
        assert_eq!(red, 10);
        sim.world_mut().entity_mut(range).remove::<Sabotaged>();
        sim.run(2);
        assert_eq!(chains(&mut sim), 0, "restored, the chain is gone");
    }
}
