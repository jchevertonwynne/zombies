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

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use super::map::MapId;
use super::map::textures::{Material, Polish, Shape, Textured, Tint, box_uv_mesh, on_disk};
use super::shifting::{Driver, LeafBody};
use crate::state::world_runs;
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

pub struct StacksLookPlugin;

impl Plugin for StacksLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (dress_bookcases, load_book_rows).chain().run_if(world_runs).run_if(resource_exists::<MapId>));
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
}
