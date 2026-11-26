//! The `map.ron` model: everything about a Map, as data (ADR 0011).
//!
//! Every section is `#[serde(default)]`, so a file that leaves a section out has it empty, and a
//! later ticket that fills a section adds fields with defaults without a `format` bump. A change
//! that can't be read that way (a field renamed, removed or re-meant) bumps [`FORMAT`] and adds a
//! migration step (see `load::migrate`).
//!
//! Positions are `[x, z]` on the ground plane and heights are `y`, in the Map's own units, with the
//! Map built at the world origin (ADR 0009). Zones, windows and Barriers are numbered by their place
//! in their list: the game numbers them in file order (ADR 0011), so list order is meaningful.
//! Names of things in the game's own catalogue (a Perk, a material, a Prop) are strings the later
//! tickets resolve; this ticket only carries them.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The `format` this build writes and reads without migrating.
pub const FORMAT: u32 = 1;

/// A position on the ground plane.
pub type Xz = [f32; 2];
/// A position in the world.
pub type Xyz = [f32; 3];

/// A Map File's `map.ron`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MapFile {
    /// Which `format` of `map.ron` this is. Read first, before anything else is trusted.
    pub format: u32,
    /// Made once when the Map is created; a Fork keeps it.
    pub id: Uuid,
    /// The Player's Name when the Map was made (or Forked), written here and never read from
    /// Settings again.
    pub creator: String,
    /// The Maps this one came from, oldest first, so every earlier creator stays credited.
    #[serde(default)]
    pub lineage: Vec<LineageEntry>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The hash of a PNG in `assets`, shown on the Map's card. `None` for no picture.
    #[serde(default)]
    pub thumbnail: Option<String>,
    /// The Map's own models, textures and sounds, each named by content hash.
    #[serde(default)]
    pub assets: Vec<AssetRef>,

    /// The Zones, in the order the game numbers them. The first is where the Run starts.
    #[serde(default)]
    pub zones: Vec<Zone>,
    #[serde(default)]
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub windows: Vec<Window>,
    #[serde(default)]
    pub doors: Vec<Door>,
    #[serde(default)]
    pub openings: Vec<Opening>,
    #[serde(default)]
    pub spawn_points: Vec<SpawnPoint>,
    /// Where each Player's Seat is.
    #[serde(default)]
    pub seats: Vec<Placement>,
    #[serde(default)]
    pub props: Vec<Prop>,
    #[serde(default)]
    pub lamps: Vec<Lamp>,
    /// Where each system's machines stand.
    #[serde(default)]
    pub machines: Machines,
    /// Hand edits to the generated path-node graph, reapplied after every regeneration.
    #[serde(default)]
    pub nav: NavEdits,
    #[serde(default)]
    pub look: Look,
    #[serde(default)]
    pub ambient: Ambient,
    /// Which built-in mechanics the Map uses, with their numbers and placed parts.
    #[serde(default)]
    pub mechanics: Mechanics,
}

impl MapFile {
    /// A new empty Map: no Zones yet (so it doesn't validate for play), saved as it stands.
    pub fn new(id: Uuid, creator: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            format: FORMAT,
            id,
            creator: creator.into(),
            lineage: Vec::new(),
            name: name.into(),
            description: String::new(),
            thumbnail: None,
            assets: Vec::new(),
            zones: Vec::new(),
            walls: Vec::new(),
            windows: Vec::new(),
            doors: Vec::new(),
            openings: Vec::new(),
            spawn_points: Vec::new(),
            seats: Vec::new(),
            props: Vec::new(),
            lamps: Vec::new(),
            machines: Machines::default(),
            nav: NavEdits::default(),
            look: Look::default(),
            ambient: Ambient::default(),
            mechanics: Mechanics::default(),
        }
    }
}

/// One Map a Fork came from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LineageEntry {
    pub id: Uuid,
    pub creator: String,
    pub name: String,
}

/// What kind of file a custom asset is. Never code.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Model,
    Texture,
    Sound,
}

impl AssetKind {
    /// The file extensions each kind may have: glTF/GLB, PNG/KTX2, OGG.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            AssetKind::Model => &["gltf", "glb"],
            AssetKind::Texture => &["png", "ktx2"],
            AssetKind::Sound => &["ogg"],
        }
    }
}

/// A custom asset, stored at `assets/<hash>.<ext>` in the Map File. There is no free-form file
/// name: the name is made from the content hash and an allowed extension, so no name can point
/// outside `assets/`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AssetRef {
    /// The SHA-256 of the file's bytes, 64 lowercase hex digits.
    pub hash: String,
    pub kind: AssetKind,
    /// `gltf`, `glb`, `png`, `ktx2` or `ogg`, matching the kind.
    pub ext: String,
}

impl AssetRef {
    /// The file's name inside `assets/`.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.hash, self.ext)
    }
}

/// A mesh of triangles with shared vertices laid out on the ground plane, each vertex with its own
/// height. Zones, Infills, Raised Floors, Sunken areas, Pools and ramps are all Footprints.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Footprint {
    /// `[x, z, y]`: where on the ground plane and how high.
    #[serde(default)]
    pub vertices: Vec<Xyz>,
    /// Three indices into `vertices` each.
    #[serde(default)]
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Zone {
    /// How the Zone reads in the editor and on the Map's own screens.
    pub name: String,
    #[serde(default)]
    pub floor: Footprint,
    /// Raised Floors, Sunken areas, Pools and Infills inside or on the Zone.
    #[serde(default)]
    pub infills: Vec<Footprint>,
    /// The height its volume is pushed up to.
    #[serde(default)]
    pub top: f32,
    /// An indoor Zone gets a ceiling from its Footprint at its top.
    #[serde(default)]
    pub indoor: bool,
    /// The material set of its floor, a name in the catalogue or a custom asset's hash.
    #[serde(default)]
    pub material: Option<String>,
}

/// A hand-drawn straight segment with its own base height at each end.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Wall {
    pub from: Xz,
    pub to: Xz,
    /// The base height at each end.
    #[serde(default)]
    pub base: [f32; 2],
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub thickness: f32,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub look: Option<String>,
}

/// Where along a Wall a window, Door or Opening is placed.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct OnWall {
    /// Index into `walls`.
    pub wall: u32,
    /// Distance along the Wall from its `from` end.
    pub at: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Window {
    pub on: OnWall,
    /// The Riser outside it; a window without one fails validation.
    #[serde(default)]
    pub riser: Option<Footprint>,
    #[serde(default)]
    pub boards: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Door {
    pub on: OnWall,
    /// What it costs to open, in Points.
    #[serde(default)]
    pub cost: u32,
}

/// An archway: a gap in a Wall with no Door.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Opening {
    pub on: OnWall,
    #[serde(default)]
    pub width: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Placement {
    pub at: Xyz,
    /// Which way it faces, in radians about the vertical.
    #[serde(default)]
    pub yaw: f32,
}

/// Where Zombies come in.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct SpawnPoint {
    pub at: Placement,
    /// Index into `zones`.
    #[serde(default)]
    pub zone: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Prop {
    /// A name in the Prop catalogue, or a custom model's hash.
    pub model: String,
    pub at: Placement,
    #[serde(default)]
    pub scale: f32,
    /// `box`, `hull` or none.
    #[serde(default)]
    pub collider: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Lamp {
    pub at: Xyz,
    #[serde(default)]
    pub color: [f32; 3],
    #[serde(default)]
    pub intensity: f32,
    #[serde(default)]
    pub range: f32,
    #[serde(default)]
    pub shadows: bool,
}

/// A placed machine or Buyable with its own settings.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Machine {
    pub at: Placement,
    /// Which one: the Perk of a Perk Machine, the weapon of an Equipment wall, and so on.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub cost: u32,
    /// Its settings beyond that, as `key = value` text the owning system reads.
    #[serde(default)]
    pub settings: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Machines {
    #[serde(default)]
    pub perk_machines: Vec<Machine>,
    #[serde(default)]
    pub pack_a_punch: Vec<Machine>,
    #[serde(default)]
    pub power_switch: Option<Machine>,
    #[serde(default)]
    pub box_locations: Vec<Machine>,
    #[serde(default)]
    pub wall_weapons: Vec<Machine>,
    #[serde(default)]
    pub equipment_walls: Vec<Machine>,
    #[serde(default)]
    pub traps: Vec<Machine>,
    /// Teleporters come in pairs: `settings` names the pair.
    #[serde(default)]
    pub teleporters: Vec<Machine>,
    #[serde(default)]
    pub ziplines: Vec<Machine>,
    #[serde(default)]
    pub jump_pads: Vec<Machine>,
    #[serde(default)]
    pub generators: Vec<Machine>,
}

/// The path-node graph is generated; these are the hand edits to it.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct NavEdits {
    #[serde(default)]
    pub pinned_nodes: Vec<Xyz>,
    #[serde(default)]
    pub added_nodes: Vec<Xyz>,
    #[serde(default)]
    pub removed_nodes: Vec<Xyz>,
    #[serde(default)]
    pub added_edges: Vec<(Xyz, Xyz)>,
    #[serde(default)]
    pub removed_edges: Vec<(Xyz, Xyz)>,
}

/// How the Map looks: its sky, light and fog.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Look {
    #[serde(default)]
    pub sky: Option<String>,
    #[serde(default)]
    pub ambient_light: [f32; 3],
    #[serde(default)]
    pub fog_color: [f32; 3],
    #[serde(default)]
    pub fog_density: f32,
}

/// The Map's ambient sound slots: a name in the sound catalogue or a custom sound's hash.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Ambient {
    #[serde(default)]
    pub bed: Option<String>,
    #[serde(default)]
    pub stings: Vec<String>,
}

/// One built-in mechanic: whether it's on, its numbers and its placed parts.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Mechanic {
    #[serde(default)]
    pub numbers: Vec<(String, f32)>,
    #[serde(default)]
    pub parts: Vec<Placement>,
}

/// Each is `None` when the Map doesn't use that mechanic.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Mechanics {
    #[serde(default)]
    pub darkness: Option<Mechanic>,
    #[serde(default)]
    pub tide: Option<Mechanic>,
    #[serde(default)]
    pub shifting: Option<Mechanic>,
    #[serde(default)]
    pub sections: Option<Mechanic>,
    #[serde(default)]
    pub sapper: Option<Mechanic>,
    #[serde(default)]
    pub quarantine: Option<Mechanic>,
    #[serde(default)]
    pub surge: Option<Mechanic>,
}
