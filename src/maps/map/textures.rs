//! Real materials on the Map's pieces: the material library, the per-piece meshes whose UVs tile
//! each material at its real-world size, and the swap from a piece's flat palette colour to its
//! material once every file has loaded.
//!
//! A piece the builder gives a material is spawned in its palette colour, as ever, with a
//! `Textured` naming the material. `TexturesPlugin` (in the real game only, never in headless
//! runs) loads the material's KTX2 files if they're all on disk, and once they've loaded bakes the
//! piece its own mesh: the box (or drum) at its full size, its UVs in world metres over the
//! material's tile size, so textures run on across neighbouring pieces without seams. A missing
//! or broken file leaves every piece wearing it in the palette.

use std::collections::HashMap;

use bevy::asset::LoadState;
use bevy::asset::io::file::FileAssetReader;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::audio::cues::FloorFamily;

/// A tiling PBR material: a Poly Haven (or other CC0) set under `assets/materials/<dir>/`, as
/// `color.ktx2`, `normal.ktx2` and `orm.ktx2` (made by `tools/texpack`), and how many metres one
/// repeat of it covers in the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Material {
    DarkWoodenPlanks,
    DamagedPlaster,
    FloorTiles06,
    WornMossyPlasterwall,
    DiagonalParquet,
    PeelingPaintedWall,
    WornBrickFloor,
    BrickWall001,
    ConcreteFloor02,
    RustyCorrugatedIron,
    MossyCobblestone,
    PlasteredStoneWall,
    // The Props'.
    WeatheredPlanks,
    RockWall08,
    FabricPattern07,
    BrokenWall,
    RustyMetal02,
    BarkBrown02,
    Hessian230,
    AsphaltFloor,
    ConcreteSlabWall02,
    ConcreteWall006,
    PaintedConcrete,
    ConcreteWall007,
    DamagedConcreteFloor03,
    RibbedConcreteWall,
    SmoothConcreteFloor,
    ConcreteLayers02,
    BrushedConcrete04,
    ConcreteWall008,
    OldMosaicFloor,
    BlueMetalPlate,
    MetalPlate02,
    GreenMetalRust,
    GranularConcrete,
    RockEmbeddedConcreteWall,
    HangarConcreteFloor,
    ConcreteTileFacade,
    WornConcreteFloor,
    ConcreteBlockWall02,
    ConcreteMoss,
    CrackedConcrete,
    WhiteSandstoneBricks03,
    WhitePlasterRough01,
    // The Village's.
    MuddyTracks,
    LeafyGrass,
    RockWall05,
    GrassGround,
    SlateFloor02,
    WhiteStucco,
    YellowStoneWall,
    TerracottaFloorTiles,
    CoastSand04,
    SandyGravel02,
    CobblestoneFloor02,
    DarkRock,
    GravelGround01,
    GrayRocks,
    BlackPaintedPlanks,
    CheckeredPavementTiles,
    // The Polder's.
    DarkBrickWall,
    CastleBrick02Red,
    CobblestoneFloor06,
    BeigeWall001,
    // The Sanatorium's: tile, linoleum, painted plaster and steel (the last four from ambientCG).
    OldLinoleumFlooring01,
    DirtyTiles,
    LongWhiteTiles,
    DecrepitWallpaper,
    WornTileFloor,
    PaintedMetalShutter,
    PaintedPlaster003,
    Tiles133D,
    Tiles132D,
    Tiles053,
}

impl Material {
    #[cfg(test)]
    pub const ALL: [Material; 74] = [
        Material::DarkWoodenPlanks,
        Material::DamagedPlaster,
        Material::FloorTiles06,
        Material::WornMossyPlasterwall,
        Material::DiagonalParquet,
        Material::PeelingPaintedWall,
        Material::WornBrickFloor,
        Material::BrickWall001,
        Material::ConcreteFloor02,
        Material::RustyCorrugatedIron,
        Material::MossyCobblestone,
        Material::PlasteredStoneWall,
        Material::WeatheredPlanks,
        Material::RockWall08,
        Material::FabricPattern07,
        Material::BrokenWall,
        Material::RustyMetal02,
        Material::BarkBrown02,
        Material::Hessian230,
        Material::AsphaltFloor,
        Material::ConcreteSlabWall02,
        Material::ConcreteWall006,
        Material::PaintedConcrete,
        Material::ConcreteWall007,
        Material::DamagedConcreteFloor03,
        Material::RibbedConcreteWall,
        Material::SmoothConcreteFloor,
        Material::ConcreteLayers02,
        Material::BrushedConcrete04,
        Material::ConcreteWall008,
        Material::OldMosaicFloor,
        Material::BlueMetalPlate,
        Material::MetalPlate02,
        Material::GreenMetalRust,
        Material::GranularConcrete,
        Material::RockEmbeddedConcreteWall,
        Material::HangarConcreteFloor,
        Material::ConcreteTileFacade,
        Material::WornConcreteFloor,
        Material::ConcreteBlockWall02,
        Material::ConcreteMoss,
        Material::CrackedConcrete,
        Material::WhiteSandstoneBricks03,
        Material::WhitePlasterRough01,
        Material::MuddyTracks,
        Material::LeafyGrass,
        Material::RockWall05,
        Material::GrassGround,
        Material::SlateFloor02,
        Material::WhiteStucco,
        Material::YellowStoneWall,
        Material::TerracottaFloorTiles,
        Material::CoastSand04,
        Material::SandyGravel02,
        Material::CobblestoneFloor02,
        Material::DarkRock,
        Material::GravelGround01,
        Material::GrayRocks,
        Material::BlackPaintedPlanks,
        Material::CheckeredPavementTiles,
        Material::DarkBrickWall,
        Material::CastleBrick02Red,
        Material::CobblestoneFloor06,
        Material::BeigeWall001,
        Material::OldLinoleumFlooring01,
        Material::DirtyTiles,
        Material::LongWhiteTiles,
        Material::DecrepitWallpaper,
        Material::WornTileFloor,
        Material::PaintedMetalShutter,
        Material::PaintedPlaster003,
        Material::Tiles133D,
        Material::Tiles132D,
        Material::Tiles053,
    ];

    /// Its directory under `assets/materials/`, and the real-world size of one repeat in metres,
    /// as its source gives it.
    pub fn def(self) -> (&'static str, f32) {
        match self {
            Material::DarkWoodenPlanks => ("dark_wooden_planks", 2.0),
            Material::DamagedPlaster => ("damaged_plaster", 1.85),
            Material::FloorTiles06 => ("floor_tiles_06", 3.0),
            Material::WornMossyPlasterwall => ("worn_mossy_plasterwall", 1.8),
            Material::DiagonalParquet => ("diagonal_parquet", 2.36),
            Material::PeelingPaintedWall => ("peeling_painted_wall", 1.8),
            Material::WornBrickFloor => ("worn_brick_floor", 2.0),
            Material::BrickWall001 => ("brick_wall_001", 3.0),
            Material::ConcreteFloor02 => ("concrete_floor_02", 2.0),
            Material::RustyCorrugatedIron => ("rusty_corrugated_iron", 2.0),
            Material::MossyCobblestone => ("mossy_cobblestone", 2.0),
            Material::PlasteredStoneWall => ("plastered_stone_wall", 2.5),
            Material::WeatheredPlanks => ("weathered_planks", 2.0),
            Material::RockWall08 => ("rock_wall_08", 1.8),
            Material::FabricPattern07 => ("fabric_pattern_07", 0.4),
            Material::BrokenWall => ("broken_wall", 3.0),
            Material::RustyMetal02 => ("rusty_metal_02", 1.0),
            Material::BarkBrown02 => ("bark_brown_02", 1.0),
            Material::AsphaltFloor => ("asphalt_floor", 2.35),
            Material::ConcreteSlabWall02 => ("concrete_slab_wall_02", 2.1),
            Material::ConcreteWall006 => ("concrete_wall_006", 2.0),
            Material::PaintedConcrete => ("painted_concrete", 2.0),
            Material::ConcreteWall007 => ("concrete_wall_007", 2.16),
            Material::DamagedConcreteFloor03 => ("damaged_concrete_floor_03", 2.0),
            Material::RibbedConcreteWall => ("ribbed_concrete_wall", 2.0),
            Material::SmoothConcreteFloor => ("smooth_concrete_floor", 2.0),
            Material::ConcreteLayers02 => ("concrete_layers_02", 2.0),
            Material::BrushedConcrete04 => ("brushed_concrete_04", 2.0),
            Material::ConcreteWall008 => ("concrete_wall_008", 2.71),
            Material::OldMosaicFloor => ("old_mosaic_floor", 2.0),
            Material::BlueMetalPlate => ("blue_metal_plate", 2.5),
            Material::MetalPlate02 => ("metal_plate_02", 2.0),
            Material::GreenMetalRust => ("green_metal_rust", 1.0),
            Material::GranularConcrete => ("granular_concrete", 2.4),
            Material::RockEmbeddedConcreteWall => ("rock_embedded_concrete_wall", 1.76),
            Material::HangarConcreteFloor => ("hangar_concrete_floor", 2.0),
            Material::ConcreteTileFacade => ("concrete_tile_facade", 2.09),
            Material::WornConcreteFloor => ("worn_concrete_floor", 2.0),
            Material::ConcreteBlockWall02 => ("concrete_block_wall_02", 2.005),
            Material::ConcreteMoss => ("concrete_moss", 3.0),
            Material::CrackedConcrete => ("cracked_concrete", 2.0),
            Material::WhiteSandstoneBricks03 => ("white_sandstone_bricks_03", 2.0),
            Material::WhitePlasterRough01 => ("white_plaster_rough_01", 2.0),
            Material::MuddyTracks => ("muddy_tracks", 2.25),
            Material::LeafyGrass => ("leafy_grass", 2.0),
            Material::RockWall05 => ("rock_wall_05", 2.26),
            Material::GrassGround => ("grass_ground", 2.51),
            Material::SlateFloor02 => ("slate_floor_02", 2.0),
            Material::WhiteStucco => ("white_stucco", 1.998),
            Material::YellowStoneWall => ("yellow_stone_wall", 2.0),
            Material::TerracottaFloorTiles => ("terracotta_floor_tiles", 2.08),
            Material::CoastSand04 => ("coast_sand_04", 3.94),
            Material::SandyGravel02 => ("sandy_gravel_02", 2.53),
            Material::CobblestoneFloor02 => ("cobblestone_floor_02", 2.0),
            Material::DarkRock => ("dark_rock", 2.42),
            Material::GravelGround01 => ("gravel_ground_01", 3.0),
            Material::GrayRocks => ("gray_rocks", 1.8),
            Material::BlackPaintedPlanks => ("black_painted_planks", 1.6),
            Material::CheckeredPavementTiles => ("checkered_pavement_tiles", 2.0),
            Material::DarkBrickWall => ("dark_brick_wall", 1.05),
            Material::CastleBrick02Red => ("castle_brick_02_red", 1.5),
            Material::CobblestoneFloor06 => ("cobblestone_floor_06", 2.0),
            Material::BeigeWall001 => ("beige_wall_001", 3.0),
            Material::Hessian230 => ("hessian_230", 0.2688),
            Material::OldLinoleumFlooring01 => ("old_linoleum_flooring_01", 1.99),
            Material::DirtyTiles => ("dirty_tiles", 2.26),
            Material::LongWhiteTiles => ("long_white_tiles", 1.27),
            Material::DecrepitWallpaper => ("decrepit_wallpaper", 2.5),
            Material::WornTileFloor => ("worn_tile_floor", 2.0),
            Material::PaintedMetalShutter => ("painted_metal_shutter", 2.0),
            Material::PaintedPlaster003 => ("paintedplaster003", 2.0),
            Material::Tiles133D => ("tiles133d", 1.8),
            Material::Tiles132D => ("tiles132d", 1.8),
            Material::Tiles053 => ("tiles053", 1.6),
        }
    }

    /// What the albedo is multiplied by: white, but for the sets that read pale under the lamps.
    pub fn tint(self) -> Color {
        match self {
            Material::RustyMetal02 | Material::BrokenWall => Color::srgb(0.62, 0.62, 0.62),
            // White tile gone grey-green with damp.
            Material::LongWhiteTiles => Color::srgb(0.7, 0.82, 0.72),
            // The Quarantine Leaves' cream-green paint.
            Material::PaintedMetalShutter => Color::srgb(0.66, 0.72, 0.6),
            // Burlap is a muddy khaki once it's a sandbag.
            Material::Hessian230 => Color::srgb(0.62, 0.58, 0.46),
            _ => Color::WHITE,
        }
    }

    pub fn tile(self) -> f32 {
        self.def().1
    }

    /// What it sounds like underfoot, if it's a floor: the family whose footstep slot plays on it.
    pub fn family(self) -> FloorFamily {
        use self::Material::*;
        match self {
            DarkWoodenPlanks | WeatheredPlanks | DiagonalParquet | BlackPaintedPlanks | BarkBrown02 => FloorFamily::Wood,
            FloorTiles06 | TerracottaFloorTiles | CheckeredPavementTiles | OldMosaicFloor | SlateFloor02 | ConcreteTileFacade
            | OldLinoleumFlooring01 | DirtyTiles | LongWhiteTiles | WornTileFloor | Tiles133D | Tiles132D | Tiles053 => {
                FloorFamily::Tile
            }
            WornBrickFloor | BrickWall001 | DarkBrickWall | CastleBrick02Red | WhiteSandstoneBricks03 | ConcreteBlockWall02 => {
                FloorFamily::Brick
            }
            RustyCorrugatedIron | RustyMetal02 | BlueMetalPlate | MetalPlate02 | GreenMetalRust | PaintedMetalShutter => FloorFamily::Metal,
            MossyCobblestone | CobblestoneFloor02 | CobblestoneFloor06 | RockWall08 | RockWall05 | DarkRock => FloorFamily::Cobble,
            MuddyTracks | LeafyGrass | GrassGround | CoastSand04 | SandyGravel02 | GravelGround01 | GrayRocks => FloorFamily::Dirt,
            DamagedPlaster | WornMossyPlasterwall | PeelingPaintedWall | PlasteredStoneWall | ConcreteFloor02 | FabricPattern07
            | BrokenWall | AsphaltFloor | ConcreteSlabWall02 | ConcreteWall006 | PaintedConcrete | ConcreteWall007
            | DamagedConcreteFloor03 | RibbedConcreteWall | SmoothConcreteFloor | ConcreteLayers02 | BrushedConcrete04
            | ConcreteWall008 | GranularConcrete | RockEmbeddedConcreteWall | HangarConcreteFloor | WornConcreteFloor
            | ConcreteMoss | CrackedConcrete | WhitePlasterRough01 | WhiteStucco | YellowStoneWall | BeigeWall001 | Hessian230 | PaintedPlaster003
            | DecrepitWallpaper => {
                FloorFamily::Concrete
            }
        }
    }

    /// Its three files, as asset paths.
    pub fn files(self) -> [String; 3] {
        let dir = self.def().0;
        ["color", "normal", "orm"].map(|map| format!("materials/{dir}/{map}.ktx2"))
    }
}

/// The shape a textured piece is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Cuboid,
    /// Upright, its diameter its size's x (and z). For the Props' drums.
    Cylinder,
    /// Its mesh is already baked with UVs (a Door, a Plank), so only its material is swapped in.
    Baked,
}

/// A Map piece that wears `material` once it has loaded. Its `Transform`'s scale is its size
/// until then.
#[derive(Component, Debug, Clone, Copy)]
pub struct Textured {
    pub material: Material,
    pub shape: Shape,
}

/// A `Textured` piece whose material is multiplied by a paint colour once it has loaded (the Perk
/// Machines' paint on one shared metal).
#[derive(Component, Debug, Clone, Copy)]
pub struct Tint(pub Color);

/// A piece of decoration (a Door's frame, a Barrier's jambs) that is hidden until `Material` has
/// loaded, so with the files missing the Map keeps its plain look.
#[derive(Component, Debug, Clone, Copy)]
pub struct Veneer(pub Material);

/// A `Veneer` that has been shown.
#[derive(Component)]
struct Revealed;

/// A `Textured` piece whose material has been settled: it wears it, or stays in the palette for
/// good because a file is missing or failed.
#[derive(Component)]
struct Dressed;

/// How far one of a material's files has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    Loading,
    Loaded,
    Failed,
}

/// What a piece wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dress {
    /// Its palette colour, for good.
    Palette,
    /// Its palette colour, for now: the files are still loading.
    Wait,
    /// Its material.
    Textured,
}

/// What a piece wears, given whether all its material's files are on disk and how far each has
/// loaded. Only when every one has loaded does it leave the palette.
pub fn dress(files_present: bool, loads: &[Load]) -> Dress {
    if !files_present || loads.contains(&Load::Failed) {
        Dress::Palette
    } else if loads.iter().all(|&load| load == Load::Loaded) {
        Dress::Textured
    } else {
        Dress::Wait
    }
}

/// The world-space box UV of a point on a face whose outward normal is `normal`, in repeats of
/// `tile` metres. Each face projects onto its own plane, the right way round as seen from outside,
/// with v running down walls.
pub fn box_uv(world: Vec3, normal: Vec3, tile: f32) -> Vec2 {
    let a = normal.abs();
    let uv = if a.y >= a.x && a.y >= a.z {
        Vec2::new(world.x, world.z * normal.y.signum())
    } else if a.x >= a.z {
        Vec2::new(-world.z * normal.x.signum(), -world.y)
    } else {
        Vec2::new(world.x * normal.z.signum(), -world.y)
    };
    uv / tile
}

fn attribute(mesh: &Mesh, id: bevy::mesh::MeshVertexAttribute) -> &Vec<[f32; 3]> {
    match mesh.attribute(id) {
        Some(VertexAttributeValues::Float32x3(values)) => values,
        _ => unreachable!("built meshes carry positions and normals"),
    }
}

/// A box `size` across, set `rotation` and centred on `at` (its transform, with a scale of one),
/// whose UVs tile a material of `tile` metres in world space.
pub fn box_uv_mesh(size: Vec3, at: Vec3, rotation: Quat, tile: f32) -> Mesh {
    let mut mesh = Cuboid::from_size(size).mesh().build();
    let uvs: Vec<[f32; 2]> = attribute(&mesh, Mesh::ATTRIBUTE_POSITION)
        .iter()
        .zip(attribute(&mesh, Mesh::ATTRIBUTE_NORMAL))
        .map(|(&p, &n)| box_uv(at + rotation * Vec3::from(p), rotation * Vec3::from(n), tile).to_array())
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.with_generated_tangents().expect("a cuboid has everything tangents need")
}

/// An upright drum `size` across (its diameter `size.x`), set `rotation` and centred on `at`, whose
/// UVs tile a material of `tile` metres: round its side by arc length, across its ends in world
/// space.
pub fn cylinder_uv_mesh(size: Vec3, at: Vec3, rotation: Quat, tile: f32) -> Mesh {
    let mut mesh = Cylinder::new(0.5, 1.0).mesh().build();
    let positions: Vec<[f32; 3]> = attribute(&mesh, Mesh::ATTRIBUTE_POSITION).iter().map(|&p| (Vec3::from(p) * size).to_array()).collect();
    let around = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uvs)) => uvs.clone(),
        _ => unreachable!("a cylinder carries UVs"),
    };
    let circumference = std::f32::consts::PI * size.x;
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .zip(attribute(&mesh, Mesh::ATTRIBUTE_NORMAL))
        .zip(&around)
        .map(|((&p, &n), uv)| {
            let world = at + rotation * Vec3::from(p);
            let normal = rotation * Vec3::from(n);
            if n[1].abs() > 0.5 {
                box_uv(world, normal, tile).to_array()
            } else {
                [uv[0] * circumference / tile, -world.y / tile]
            }
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.with_generated_tangents().expect("a cylinder has everything tangents need")
}

/// What's known of each material asked for so far.
enum Entry {
    /// A file isn't on disk.
    Missing,
    Loading { material: Handle<StandardMaterial>, images: [Handle<Image>; 3] },
}

#[derive(Resource, Default)]
struct Library(HashMap<Material, Entry>);

pub struct TexturesPlugin;

impl Plugin for TexturesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Library>()
            .init_resource::<super::decals::DecalLibrary>()
            .add_systems(Update, ((request, put_on, reveal).chain(), (super::decals::request_decals, super::decals::place_decals).chain(), (super::fitting::request_models, super::fitting::put_on_models).chain()));
    }
}

/// Starts loading each material a new piece asks for, if all its files are on disk.
fn request(
    mut library: ResMut<Library>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    pieces: Query<(Option<&Textured>, Option<&Veneer>), Or<(Added<Textured>, Added<Veneer>)>>,
) {
    for material in pieces.iter().filter_map(|(textured, veneer)| textured.map(|t| t.material).or(veneer.map(|v| v.0))) {
        library.0.entry(material).or_insert_with(|| {
            let tint = material.tint();
            let files = material.files();
            if !files.iter().all(|file| on_disk(file)) {
                debug!("{material:?}: files missing, staying in the palette");
                return Entry::Missing;
            }
            let [color, normal, orm] = files.map(|file| load_tiling(&asset_server, file));
            let material = materials.add(StandardMaterial {
                base_color: tint,
                base_color_texture: Some(color.clone()),
                normal_map_texture: Some(normal.clone()),
                metallic_roughness_texture: Some(orm.clone()),
                occlusion_texture: Some(orm.clone()),
                // The ORM map's green and blue are the roughness and metal, as they are.
                perceptual_roughness: 1.0,
                metallic: 1.0,
                ..default()
            });
            Entry::Loading { material, images: [color, normal, orm] }
        });
    }
}

/// Whether the asset at `path` (under `assets/`) is on disk.
pub(crate) fn on_disk(path: &str) -> bool {
    FileAssetReader::get_base_path().join("assets").join(path).is_file()
}

/// An image that repeats across the piece, sharp at a glancing angle.
fn load_tiling(asset_server: &AssetServer, path: String) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                anisotropy_clamp: 8,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(path)
}

pub(crate) fn load_of(asset_server: &AssetServer, image: &Handle<Image>) -> Load {
    match asset_server.load_state(image) {
        LoadState::Failed(_) => Load::Failed,
        LoadState::Loaded if asset_server.is_loaded_with_dependencies(image) => Load::Loaded,
        _ => Load::Loading,
    }
}

/// Puts each waiting piece in its material once every file has loaded, baking it its own mesh at
/// its full size; or leaves it in the palette for good if a file is missing or failed.
fn put_on(
    mut commands: Commands,
    library: Res<Library>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tinted: Local<HashMap<(Material, [u8; 4]), Handle<StandardMaterial>>>,
    mut pieces: Query<(Entity, &Textured, Option<&Tint>, &mut Transform, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>), Without<Dressed>>,
) {
    for (entity, textured, tint, mut transform, mut mesh, mut paint) in &mut pieces {
        let Some(entry) = library.0.get(&textured.material) else { continue };
        let choice = match entry {
            Entry::Missing => dress(false, &[]),
            Entry::Loading { images, .. } => dress(true, &images.each_ref().map(|image| load_of(&asset_server, image))),
        };
        match (choice, entry) {
            (Dress::Wait, _) => continue,
            (Dress::Textured, Entry::Loading { material, .. }) => {
                let tile = textured.material.tile();
                let (size, at, rotation) = (transform.scale, transform.translation, transform.rotation);
                match textured.shape {
                    Shape::Cuboid => mesh.0 = meshes.add(box_uv_mesh(size, at, rotation, tile)),
                    Shape::Cylinder => mesh.0 = meshes.add(cylinder_uv_mesh(size, at, rotation, tile)),
                    Shape::Baked => {}
                }
                paint.0 = match tint {
                    None => material.clone(),
                    Some(Tint(colour)) => tinted
                        .entry((textured.material, colour.to_srgba().to_u8_array()))
                        .or_insert_with(|| {
                            let mut dyed = materials.get(material).cloned().unwrap_or_default();
                            dyed.base_color = *colour;
                            // Paint isn't metal: a painted piece keeps the set's roughness and shows its colour.
                            dyed.metallic = 0.0;
                            materials.add(dyed)
                        })
                        .clone(),
                };
                if textured.shape != Shape::Baked {
                    transform.scale = Vec3::ONE;
                }
            }
            _ => {}
        }
        commands.entity(entity).try_insert(Dressed);
    }
}

/// Shows each `Veneer` once its material has loaded.
fn reveal(
    mut commands: Commands,
    library: Res<Library>,
    asset_server: Res<AssetServer>,
    mut pieces: Query<(Entity, &Veneer, &mut Visibility), Without<Revealed>>,
) {
    for (entity, veneer, mut visibility) in &mut pieces {
        let Some(Entry::Loading { images, .. }) = library.0.get(&veneer.0) else { continue };
        if dress(true, &images.each_ref().map(|image| load_of(&asset_server, image))) == Dress::Textured {
            *visibility = Visibility::Inherited;
            commands.entity(entity).try_insert(Revealed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uvs(mesh: &Mesh) -> Vec<Vec2> {
        match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(VertexAttributeValues::Float32x2(uvs)) => uvs.iter().map(|&uv| Vec2::from(uv)).collect(),
            _ => panic!("no UVs"),
        }
    }

    /// The UVs of the vertices on the face of `mesh` looking along `normal`.
    fn face(mesh: &Mesh, normal: Vec3) -> Vec<Vec2> {
        attribute(mesh, Mesh::ATTRIBUTE_NORMAL)
            .iter()
            .zip(uvs(mesh))
            .filter(|(n, _)| Vec3::from(**n).dot(normal) > 0.99)
            .map(|(_, uv)| uv)
            .collect()
    }

    fn span(uvs: &[Vec2]) -> Vec2 {
        let (lo, hi) = uvs.iter().fold((Vec2::MAX, Vec2::MIN), |(lo, hi), &uv| (lo.min(uv), hi.max(uv)));
        hi - lo
    }

    #[test]
    fn a_face_repeats_once_per_tile_of_its_size() {
        // A wall 4 m long, 3 m tall and 0.2 m thick, in a 2 m material.
        let mesh = box_uv_mesh(Vec3::new(4.0, 3.0, 0.2), Vec3::new(10.0, 1.5, -6.0), Quat::IDENTITY, 2.0);
        assert!(span(&face(&mesh, Vec3::Z)).abs_diff_eq(Vec2::new(2.0, 1.5), 1e-5));
        assert!(span(&face(&mesh, Vec3::NEG_Z)).abs_diff_eq(Vec2::new(2.0, 1.5), 1e-5));
        assert!(span(&face(&mesh, Vec3::Y)).abs_diff_eq(Vec2::new(2.0, 0.1), 1e-5));
        assert!(span(&face(&mesh, Vec3::X)).abs_diff_eq(Vec2::new(0.1, 1.5), 1e-5));
        // Half the tile, twice the repeats.
        let finer = box_uv_mesh(Vec3::new(4.0, 3.0, 0.2), Vec3::ZERO, Quat::IDENTITY, 1.0);
        assert!(span(&face(&finer, Vec3::Z)).abs_diff_eq(Vec2::new(4.0, 3.0), 1e-5));
    }

    #[test]
    fn neighbouring_pieces_meet_without_a_seam() {
        // Two pieces of one wall either side of x = 2: where they meet, their UVs agree.
        let left = box_uv_mesh(Vec3::new(4.0, 3.0, 0.2), Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, 1.85);
        let right = box_uv_mesh(Vec3::new(2.0, 3.0, 0.2), Vec3::new(3.0, 1.5, 0.0), Quat::IDENTITY, 1.85);
        let edge = |mesh: &Mesh, x: f32| -> Vec<Vec2> {
            let mut at: Vec<_> = attribute(mesh, Mesh::ATTRIBUTE_POSITION)
                .iter()
                .zip(attribute(mesh, Mesh::ATTRIBUTE_NORMAL))
                .zip(uvs(mesh))
                .filter(|((p, n), _)| n[2] > 0.99 && (p[0] - x).abs() < 1e-5)
                .map(|(_, uv)| uv)
                .collect();
            at.sort_by(|a, b| a.y.total_cmp(&b.y));
            at
        };
        let (a, b) = (edge(&left, 2.0), edge(&right, -1.0));
        assert_eq!(a.len(), 2);
        for (a, b) in a.iter().zip(&b) {
            assert!(a.abs_diff_eq(*b, 1e-5), "{a} != {b}");
        }
    }

    #[test]
    fn a_face_is_not_mirrored_seen_from_outside() {
        // u grows to the viewer's right on every wall face.
        for normal in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
            let right = normal.cross(Vec3::Y).normalize() * -1.0;
            let du = box_uv(right, normal, 1.0).x - box_uv(Vec3::ZERO, normal, 1.0).x;
            assert!((du - 1.0).abs() < 1e-5, "{normal}: {du}");
        }
    }

    #[test]
    fn a_turned_piece_tiles_on_its_own_faces() {
        // A ramp's slab, turned 30 degrees about x: its top still repeats once per tile of its length.
        let mesh = box_uv_mesh(Vec3::new(2.0, 0.3, 4.0), Vec3::new(0.0, 1.0, 0.0), Quat::from_rotation_x(0.5), 2.0);
        let top = face(&mesh, Vec3::Y);
        assert_eq!(top.len(), 4);
        assert!((span(&top).x - 1.0).abs() < 1e-5);
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    #[test]
    fn a_drum_wraps_its_side_by_arc_length() {
        let mesh = cylinder_uv_mesh(Vec3::new(0.6, 0.9, 0.6), Vec3::new(5.0, 0.45, 5.0), Quat::IDENTITY, 1.0);
        let side: Vec<Vec2> = attribute(&mesh, Mesh::ATTRIBUTE_NORMAL)
            .iter()
            .zip(uvs(&mesh))
            .filter(|(n, _)| n[1].abs() < 0.5)
            .map(|(_, uv)| uv)
            .collect();
        let around = span(&side);
        assert!((around.x - std::f32::consts::PI * 0.6).abs() < 1e-4, "{around}");
        assert!((around.y - 0.9).abs() < 1e-5);
        // Its ends are planar, 0.6 m across.
        assert!(span(&face(&mesh, Vec3::Y)).abs_diff_eq(Vec2::splat(0.6), 1e-3));
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    #[test]
    fn a_piece_leaves_the_palette_only_once_every_file_has_loaded() {
        use Load::*;
        assert_eq!(dress(false, &[]), Dress::Palette);
        assert_eq!(dress(true, &[Loaded, Loading, Loaded]), Dress::Wait);
        assert_eq!(dress(true, &[Loaded, Failed, Loading]), Dress::Palette);
        assert_eq!(dress(true, &[Loaded, Loaded, Loaded]), Dress::Textured);
    }

    #[test]
    fn every_material_names_three_files_and_a_real_tile() {
        for material in Material::ALL {
            assert!(material.tile() > 0.2 && material.tile() < 10.0, "{material:?}");
            assert!(material.files().iter().all(|f| f.starts_with("materials/") && f.ends_with(".ktx2")));
        }
    }
}
