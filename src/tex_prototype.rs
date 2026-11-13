//! PROTOTYPE, throwaway (wayfinder ticket #278 "How should one Zone look with real PBR textures?").
//! Not for main.
//!
//! Textures the Greybox's Start Room (floor, wall faces, skirting and cornice) with CC0 Poly Haven
//! sets, the way the texturing research chose: each piece gets its own cuboid mesh with world-space
//! box UVs (metres ÷ tile size) and generated tangents, and a plain `StandardMaterial`. A few
//! ambientCG grime streaks go on the walls as alpha-blended quads. Images are 1K JPGs with a mip
//! chain box-filtered on load, standing in for offline KTX2.
//!
//! Keys: `.` / `,` cycle the material set (flat, kept, neglected, ruined); `-` cycles the tiling
//! scale (×0.5, ×1, ×2 of each texture's real-world size); `=` toggles textured or flat trim.
//! `ZOMBIES_TEX=<set>` picks the set at start; `ZOMBIES_TEX_ALL=1` textures every Zone with it (for
//! the bench's worst case); `ZOMBIES_TEX_TOUR=<dir>` shoots the comparison set and quits.

use bevy::asset::AssetEvent;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::look_prototype::Look;
use crate::maps::map::MapId;
use crate::maps::zone::ZoneId;
use crate::players::player::{Health, LocalPlayer, Player, PlayerCamera, Teleport};
use crate::run::mode::Mode;
use crate::state::{GameState, InRun};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SurfaceKind {
    Floor,
    Wall,
    Trim,
}

/// What a Map piece is, for the prototype to find it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Surface {
    pub zone: Option<ZoneId>,
    pub kind: SurfaceKind,
}

/// How a piece was drawn before the prototype touched it.
#[derive(Component)]
struct Original {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    scale: Vec3,
}

#[derive(Component)]
struct Decal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Set {
    Flat,
    Kept,
    Neglected,
    Ruined,
}

const SETS: [Set; 4] = [Set::Flat, Set::Kept, Set::Neglected, Set::Ruined];
const SCALES: [f32; 3] = [0.5, 1.0, 2.0];

impl Set {
    fn name(self) -> &'static str {
        match self {
            Set::Flat => "flat",
            Set::Kept => "kept",
            Set::Neglected => "neglected",
            Set::Ruined => "ruined",
        }
    }

    /// The Poly Haven texture for each kind of surface, and its real-world size in metres.
    fn texture(self, kind: SurfaceKind) -> Option<(&'static str, f32)> {
        Some(match (self, kind) {
            (Set::Flat, _) => return None,
            (Set::Kept, SurfaceKind::Floor) => ("wood_floor_worn", 2.0),
            (Set::Kept, SurfaceKind::Wall) => ("yellow_plaster", 2.0),
            (Set::Kept, SurfaceKind::Trim) => ("red_plaster_weathered", 2.0),
            (Set::Neglected, SurfaceKind::Floor) => ("old_wood_floor", 3.0),
            (Set::Neglected, SurfaceKind::Wall) => ("decrepit_wallpaper", 2.5),
            (Set::Neglected, SurfaceKind::Trim) => ("dark_wooden_planks", 2.0),
            (Set::Ruined, SurfaceKind::Floor) => ("dark_wooden_planks", 2.0),
            (Set::Ruined, SurfaceKind::Wall) => ("damaged_plaster", 1.85),
            (Set::Ruined, SurfaceKind::Trim) => ("black_painted_planks", 1.6),
        })
    }
}

#[derive(Resource, Clone, Copy, PartialEq, Debug)]
struct TexState {
    set: Set,
    scale: usize,
    trim_textured: bool,
    all_zones: bool,
}

#[derive(Resource, Default)]
struct Library {
    materials: std::collections::HashMap<&'static str, Handle<StandardMaterial>>,
    images: Vec<AssetId<Image>>,
}

pub struct TexPrototypePlugin;

impl Plugin for TexPrototypePlugin {
    fn build(&self, app: &mut App) {
        let set = std::env::var("ZOMBIES_TEX")
            .ok()
            .map(|name| *SETS.iter().find(|set| set.name() == name).unwrap_or_else(|| panic!("ZOMBIES_TEX={name}: expected flat, kept, neglected or ruined")))
            .unwrap_or(Set::Flat);
        app.insert_resource(TexState {
            set,
            scale: 1,
            trim_textured: true,
            all_zones: std::env::var("ZOMBIES_TEX_ALL").is_ok(),
        })
        .init_resource::<Library>()
        .add_systems(OnEnter(GameState::Playing), (spawn_decals, preload))
        .add_systems(Update, (keys, apply, show_decals).chain().run_if(in_state(GameState::Playing)))
        .add_systems(Update, mip_on_load);
        if let Ok(dir) = std::env::var("ZOMBIES_TEX_TOUR") {
            std::fs::create_dir_all(&dir).unwrap();
            app.insert_resource(Tour { dir, ..default() })
                .add_systems(OnEnter(GameState::MainMenu), start_run)
                .add_systems(Update, (keep_unhurt, tour).run_if(in_state(GameState::Playing)));
        }
    }
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<TexState>) {
    let at = SETS.iter().position(|&set| set == state.set).unwrap();
    if keys.just_pressed(KeyCode::Period) {
        state.set = SETS[(at + 1) % SETS.len()];
    } else if keys.just_pressed(KeyCode::Comma) {
        state.set = SETS[(at + SETS.len() - 1) % SETS.len()];
    } else if keys.just_pressed(KeyCode::Minus) {
        state.scale = (state.scale + 1) % SCALES.len();
    } else if keys.just_pressed(KeyCode::Equal) {
        state.trim_textured = !state.trim_textured;
    } else {
        return;
    }
    info!("tex: {} ×{} trim {}", state.set.name(), SCALES[state.scale], if state.trim_textured { "textured" } else { "flat" });
}

fn load(asset_server: &AssetServer, library: &mut Library, path: String, srgb: bool) -> Handle<Image> {
    let handle = asset_server
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                anisotropy_clamp: 8,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(path);
    library.images.push(handle.id());
    handle
}

fn material(
    id: &'static str,
    asset_server: &AssetServer,
    library: &mut Library,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    if let Some(material) = library.materials.get(id) {
        return material.clone();
    }
    let dir = "prototype_textures";
    let diff = load(asset_server, library, format!("{dir}/{id}_diff.png"), true);
    let nor = load(asset_server, library, format!("{dir}/{id}_nor.png"), false);
    let arm = load(asset_server, library, format!("{dir}/{id}_arm.png"), false);
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(diff),
        normal_map_texture: Some(nor),
        metallic_roughness_texture: Some(arm.clone()),
        occlusion_texture: Some(arm),
        perceptual_roughness: 1.0,
        metallic: 1.0,
        ..default()
    });
    library.materials.insert(id, material.clone());
    material
}

/// A cuboid `size` across centred on `at`, with its UVs in world metres ÷ `tile`.
fn box_uv_mesh(size: Vec3, at: Vec3, tile: f32) -> Mesh {
    let mut mesh = Cuboid::from_size(size).mesh().build();
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => unreachable!(),
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => unreachable!(),
    };
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .zip(&normals)
        .map(|(p, n)| {
            let w = at + Vec3::from(*p);
            let n = Vec3::from(*n);
            let uv = if n.y.abs() > 0.5 {
                Vec2::new(w.x, w.z)
            } else if n.x.abs() > 0.5 {
                Vec2::new(w.z, -w.y)
            } else {
                Vec2::new(w.x, -w.y)
            };
            (uv / tile).to_array()
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.with_generated_tangents().unwrap()
}

fn apply(
    mut commands: Commands,
    state: Res<TexState>,
    asset_server: Res<AssetServer>,
    mut library: ResMut<Library>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pieces: Query<(Entity, &Surface, &mut Transform, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, Option<&Original>)>,
    added: Query<(), Added<Surface>>,
) {
    if !state.is_changed() && added.is_empty() {
        return;
    }
    for (entity, surface, mut transform, mut mesh, mut paint, original) in &mut pieces {
        if !state.all_zones && surface.zone != Some(ZoneId::StartRoom) {
            continue;
        }
        let (orig_mesh, orig_material, orig_scale) = match original {
            Some(o) => (o.mesh.clone(), o.material.clone(), o.scale),
            None => {
                commands.entity(entity).insert(Original {
                    mesh: mesh.0.clone(),
                    material: paint.0.clone(),
                    scale: transform.scale,
                });
                (mesh.0.clone(), paint.0.clone(), transform.scale)
            }
        };
        let texture = state.set.texture(surface.kind).filter(|_| surface.kind != SurfaceKind::Trim || state.trim_textured);
        match texture {
            None => {
                mesh.0 = orig_mesh;
                paint.0 = orig_material;
                transform.scale = orig_scale;
            }
            Some((id, size)) => {
                let tile = size * SCALES[state.scale];
                mesh.0 = meshes.add(box_uv_mesh(orig_scale, transform.translation, tile));
                paint.0 = material(id, &asset_server, &mut library, &mut materials);
                transform.scale = Vec3::ONE;
            }
        }
    }
}

/// A box-filtered mip chain for each of the prototype's images, once it has loaded.
fn mip_on_load(mut events: MessageReader<AssetEvent<Image>>, library: Res<Library>, mut images: ResMut<Assets<Image>>) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else { continue };
        if !library.images.contains(id) {
            continue;
        }
        let Some(mut image) = images.get_mut(*id) else { continue };
        if image.texture_descriptor.mip_level_count > 1 || image.texture_descriptor.format.block_copy_size(None) != Some(4) {
            continue;
        }
        let (mut w, mut h) = (image.width() as usize, image.height() as usize);
        let mut data = image.data.clone().unwrap();
        let mut level: Vec<u8> = data.clone();
        let mut count = 1;
        while w > 1 || h > 1 {
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut next = vec![0u8; nw * nh * 4];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let mut sum = 0u32;
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let sx = (x * 2 + dx).min(w - 1);
                            let sy = (y * 2 + dy).min(h - 1);
                            sum += level[(sy * w + sx) * 4 + c] as u32;
                        }
                        next[(y * nw + x) * 4 + c] = (sum / 4) as u8;
                    }
                }
            }
            data.extend_from_slice(&next);
            level = next;
            w = nw;
            h = nh;
            count += 1;
        }
        image.data = Some(data);
        image.texture_descriptor.mip_level_count = count;
    }
}

/// Grime streaks on the Start Room's walls: (centre, the way the face looks, width, height, decal).
const DECALS: [(Vec3, Vec3, f32, f32, &str); 4] = [
    (Vec3::new(-9.5, 2.3, 14.212), Vec3::Z, 2.2, 2.8, "Leaking012A"),
    (Vec3::new(9.0, 2.6, 14.212), Vec3::Z, 1.6, 2.2, "Leaking017B"),
    (Vec3::new(7.0, 2.3, 25.788), Vec3::NEG_Z, 2.4, 2.8, "Leaking017B"),
    (Vec3::new(13.788, 2.4, 19.0), Vec3::NEG_X, 2.0, 2.6, "Leaking012A"),
];

/// Loads every set's images up front, so the first frame a set is shown it has them all.
fn preload(asset_server: Res<AssetServer>, mut library: ResMut<Library>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for set in SETS {
        for kind in [SurfaceKind::Floor, SurfaceKind::Wall, SurfaceKind::Trim] {
            if let Some((id, _)) = set.texture(kind) {
                material(id, &asset_server, &mut library, &mut materials);
            }
        }
    }
}

fn spawn_decals(
    mut commands: Commands,
    map: Res<MapId>,
    asset_server: Res<AssetServer>,
    mut library: ResMut<Library>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if *map != MapId::Greybox {
        return;
    }
    for (at, normal, width, height, id) in DECALS {
        let colour = load(&asset_server, &mut library, format!("prototype_textures/{id}_decal.png"), true);
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(colour),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.6,
            depth_bias: 50.0,
            ..default()
        });
        commands.spawn((
            InRun,
            Decal,
            Mesh3d(meshes.add(Rectangle::new(width, height))),
            MeshMaterial3d(material),
            Transform::from_translation(at).looking_to(-normal, Vec3::Y),
            Visibility::Hidden,
        ));
    }
}

fn show_decals(state: Res<TexState>, mut decals: Query<&mut Visibility, With<Decal>>, added: Query<(), Added<Decal>>) {
    if !state.is_changed() && added.is_empty() {
        return;
    }
    for mut visibility in &mut decals {
        *visibility = if state.set == Set::Flat { Visibility::Hidden } else { Visibility::Inherited };
    }
}

// --- The screenshot tour ---------------------------------------------------------------------

/// (name, feet, looking at, pitch)
const VIEWS: [(&str, Vec3, Vec3, f32); 3] = [
    ("room", Vec3::new(-4.0, 0.0, 25.4), Vec3::new(9.0, 0.0, 14.5), -0.1),
    ("wall", Vec3::new(-6.5, 0.0, 17.0), Vec3::new(-10.0, 0.0, 14.0), -0.12),
    ("floor", Vec3::new(2.0, 0.0, 24.0), Vec3::new(-2.0, 0.0, 17.0), -0.55),
];

/// Every shot: (view, set, scale index, trim textured).
fn shots() -> Vec<(usize, Set, usize, bool)> {
    let mut shots = Vec::new();
    for view in 0..VIEWS.len() {
        for set in SETS {
            shots.push((view, set, 1, true));
        }
    }
    for view in 0..2 {
        for scale in [0, 2] {
            shots.push((view, Set::Kept, scale, true));
            shots.push((view, Set::Ruined, scale, true));
        }
    }
    for set in [Set::Kept, Set::Neglected, Set::Ruined] {
        shots.push((1, set, 1, false));
    }
    shots
}

#[derive(Resource, Default)]
struct Tour {
    dir: String,
    frames: u32,
    shot: usize,
    settle: u32,
}

const TOUR_WARM_UP: u32 = 240;

fn start_run(mut map: ResMut<MapId>, mut mode: ResMut<Mode>, mut next: ResMut<NextState<GameState>>) {
    if std::env::var("ZOMBIES_BENCH").is_ok() {
        return;
    }
    *map = MapId::Greybox;
    *mode = Mode::Classic;
    next.set(GameState::Playing);
}

fn keep_unhurt(mut commands: Commands, mut players: Query<&mut Health, With<Player>>, zombies: Query<Entity, With<crate::enemies::zombie::Zombie>>) {
    for mut health in &mut players {
        health.current = health.max;
    }
    // Keep the shots clear.
    for zombie in &zombies {
        commands.entity(zombie).despawn();
    }
}

fn tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut state: ResMut<TexState>,
    mut look: ResMut<Look>,
    player: Query<Entity, With<LocalPlayer>>,
    mut camera: Query<&mut Transform, (With<PlayerCamera>, Without<LocalPlayer>)>,
    mut body: Query<&mut Transform, (With<LocalPlayer>, Without<PlayerCamera>)>,
    mut teleports: MessageWriter<Teleport>,
    mut exit: MessageWriter<AppExit>,
) {
    tour.frames += 1;
    if tour.frames < TOUR_WARM_UP {
        return;
    }
    let shots = shots();
    if tour.shot == shots.len() {
        tour.settle += 1;
        if tour.settle > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let Ok(player) = player.single() else { return };
    let (view, set, scale, trim) = shots[tour.shot];
    let (name, feet, at, pitch) = VIEWS[view];
    let facing = Transform::from_translation(feet).looking_at(Vec3::new(at.x, feet.y, at.z), Vec3::Y).rotation;
    for mut transform in &mut body {
        transform.rotation = facing;
    }
    for mut transform in &mut camera {
        transform.rotation = Quat::from_rotation_x(pitch);
    }
    if tour.settle == 0 {
        teleports.write(Teleport {
            player,
            feet,
            facing: Some(facing),
        });
        if *look != Look::Dusk {
            *look = Look::Dusk;
        }
        let want = TexState {
            set,
            scale,
            trim_textured: trim,
            all_zones: state.all_zones,
        };
        if *state != want {
            *state = want;
        }
    }
    tour.settle += 1;
    let wait = if tour.shot < 4 { 150 } else { 45 };
    if tour.settle < wait {
        return;
    }
    let path = format!(
        "{}/{}-{}-x{}{}.png",
        tour.dir,
        name,
        set.name(),
        SCALES[scale],
        if trim { "" } else { "-flattrim" }
    );
    info!("tour: {path}");
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    tour.settle = 0;
    tour.shot += 1;
}
