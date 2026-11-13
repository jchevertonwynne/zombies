//! PROTOTYPE, throwaway (wayfinder ticket #288 "Which material set does each of the slice's other
//! Zones get?", building on #278). Not for main.
//!
//! Textures the vertical slice's six Greybox Zones with CC0 Poly Haven sets. The Start Room keeps
//! the approved Ruined set; each of the other five has three candidate sets (floor + wall faces) to
//! choose from, or its flat palette. Trim stays flat paint in the Zone's trim colour (#278).
//! Same technique as #278: per-piece cuboid meshes with world-space box UVs at each texture's
//! real-world size, a plain `StandardMaterial`, and a box-filtered mip chain made on load.
//!
//! Keys: `.` / `,` cycle the candidate for the Zone the Player stands in (0 is flat).
//! `ZOMBIES_ZONESETS=west_hall=2,dining_room=1,...` picks at start (default: candidate 1
//! everywhere). Run with `ZOMBIES_LOOK=dusk`.
//! `ZOMBIES_TEX_TOUR=<dir>` shoots every candidate of every Zone and quits;
//! `ZOMBIES_TEX_TOUR=<dir> ZOMBIES_TEX_ROUTE=1` instead shoots the route through the Doors (open,
//! hidden for the shot) with the `ZOMBIES_ZONESETS` picks.

use std::collections::HashMap;

use bevy::asset::AssetEvent;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::look_prototype::Look;
use crate::maps::door::Door;
use crate::maps::map::MapId;
use crate::maps::zone::ZoneId;
use crate::players::player::{Health, LocalPlayer, Player, PlayerCamera, Teleport};
use crate::run::mode::Mode;
use crate::state::GameState;

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

/// A candidate set: its name, then (Poly Haven id, real-world size in metres) for floor and walls.
type Candidate = (&'static str, (&'static str, f32), (&'static str, f32));

const SLICE: [ZoneId; 6] = [ZoneId::StartRoom, ZoneId::WestHall, ZoneId::DiningRoom, ZoneId::Passage, ZoneId::Workshop, ZoneId::Yard];

fn key(zone: ZoneId) -> &'static str {
    match zone {
        ZoneId::StartRoom => "start_room",
        ZoneId::WestHall => "west_hall",
        ZoneId::DiningRoom => "dining_room",
        ZoneId::Passage => "passage",
        ZoneId::Workshop => "workshop",
        ZoneId::Yard => "yard",
        _ => "other",
    }
}

/// Each slice Zone's candidates. Index 0 in the picks is flat; 1.. index into this.
fn candidates(zone: ZoneId) -> &'static [Candidate] {
    match zone {
        ZoneId::StartRoom => &[("ruined", ("dark_wooden_planks", 2.0), ("damaged_plaster", 1.85))],
        ZoneId::WestHall => &[
            ("tiled", ("floor_tiles_06", 3.0), ("worn_mossy_plasterwall", 1.8)),
            ("panelled", ("herringbone_parquet", 3.4), ("dark_paneled_wood", 1.4)),
            ("bare", ("floor_tiles_08", 1.5), ("worn_plaster_wall", 1.8)),
        ],
        ZoneId::DiningRoom => &[
            ("peeling", ("diagonal_parquet", 2.36), ("peeling_painted_wall", 1.8)),
            ("terracotta", ("terracotta_floor_tiles", 2.08), ("rough_plaster_broken", 1.5)),
            ("panelled", ("rectangular_parquet", 2.25), ("wooden_panels", 2.1)),
        ],
        ZoneId::Passage => &[
            ("brick", ("worn_brick_floor", 2.0), ("brick_wall_001", 3.0)),
            ("whitewash", ("concrete_floor_worn_001", 3.0), ("whitewashed_brick", 2.0)),
            ("service", ("dirty_tiles", 2.26), ("painted_worn_brick", 1.8)),
        ],
        ZoneId::Workshop => &[
            ("rusty-tin", ("concrete_floor_02", 2.0), ("rusty_corrugated_iron", 2.0)),
            ("brick-works", ("dirty_concrete", 3.0), ("factory_brick", 1.5)),
            ("grey-tin", ("hangar_concrete_floor", 2.0), ("corrugated_iron_02", 2.7)),
        ],
        ZoneId::Yard => &[
            ("garden", ("forrest_ground_01", 2.0), ("red_brick_03", 1.0)),
            ("muddy", ("brown_mud_leaves_01", 1.3), ("brick_wall_02", 2.0)),
            ("cobbled", ("mossy_cobblestone", 2.0), ("plastered_stone_wall", 2.5)),
        ],
        _ => &[],
    }
}

fn candidate(zone: ZoneId, pick: usize) -> Option<&'static Candidate> {
    pick.checked_sub(1).and_then(|i| candidates(zone).get(i))
}

fn pick_name(zone: ZoneId, pick: usize) -> &'static str {
    candidate(zone, pick).map_or("flat", |c| c.0)
}

#[derive(Resource, Clone, PartialEq, Debug)]
struct Picks(HashMap<ZoneId, usize>);

#[derive(Resource, Default)]
struct Library {
    materials: HashMap<&'static str, Handle<StandardMaterial>>,
    images: Vec<AssetId<Image>>,
}

pub struct TexPrototypePlugin;

impl Plugin for TexPrototypePlugin {
    fn build(&self, app: &mut App) {
        let mut picks: HashMap<ZoneId, usize> = SLICE.iter().map(|&z| (z, 1)).collect();
        if let Ok(spec) = std::env::var("ZOMBIES_ZONESETS") {
            for part in spec.split(',').filter(|p| !p.is_empty()) {
                let (name, pick) = part.split_once('=').expect("ZOMBIES_ZONESETS: zone=n,...");
                let zone = *SLICE.iter().find(|&&z| key(z) == name).unwrap_or_else(|| panic!("unknown zone {name}"));
                picks.insert(zone, pick.parse().unwrap());
            }
        }
        app.insert_resource(Picks(picks))
            .init_resource::<Library>()
            .add_systems(OnEnter(GameState::Playing), preload)
            .add_systems(Update, (keys, apply).chain().run_if(in_state(GameState::Playing)))
            .add_systems(Update, mip_on_load);
        if let Ok(dir) = std::env::var("ZOMBIES_TEX_TOUR") {
            std::fs::create_dir_all(&dir).unwrap();
            let route = std::env::var("ZOMBIES_TEX_ROUTE").is_ok();
            app.insert_resource(Tour { dir, route, ..default() })
                .add_systems(OnEnter(GameState::MainMenu), start_run)
                .add_systems(Update, (keep_unhurt, tour).run_if(in_state(GameState::Playing)));
        }
    }
}

fn zone_at(at: Vec3) -> Option<ZoneId> {
    SLICE.into_iter().find(|zone| {
        let (lo, hi) = zone.volume();
        at.x >= lo.x && at.x <= hi.x && at.z >= lo.z && at.z <= hi.z && at.y >= lo.y - 1.0 && at.y <= hi.y
    })
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut picks: ResMut<Picks>, player: Query<&GlobalTransform, With<LocalPlayer>>) {
    let step = if keys.just_pressed(KeyCode::Period) {
        1
    } else if keys.just_pressed(KeyCode::Comma) {
        -1
    } else {
        return;
    };
    let Ok(player) = player.single() else { return };
    let Some(zone) = zone_at(player.translation()) else { return };
    let count = candidates(zone).len() as i32 + 1;
    let pick = picks.0.entry(zone).or_insert(0);
    *pick = (*pick as i32 + step).rem_euclid(count) as usize;
    info!("tex: {} = {} ({})", key(zone), *pick, pick_name(zone, *pick));
    let all: Vec<_> = SLICE.iter().map(|&z| format!("{}={}", key(z), picks.0[&z])).collect();
    info!("tex: ZOMBIES_ZONESETS={}", all.join(","));
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

fn material(id: &'static str, asset_server: &AssetServer, library: &mut Library, materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
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
    picks: Res<Picks>,
    asset_server: Res<AssetServer>,
    mut library: ResMut<Library>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pieces: Query<(Entity, &Surface, &mut Transform, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>, Option<&Original>)>,
    added: Query<(), Added<Surface>>,
) {
    if !picks.is_changed() && added.is_empty() {
        return;
    }
    for (entity, surface, mut transform, mut mesh, mut paint, original) in &mut pieces {
        let Some(zone) = surface.zone.filter(|z| SLICE.contains(z)) else { continue };
        if surface.kind == SurfaceKind::Trim {
            continue;
        }
        let (orig_mesh, orig_material, orig_scale) = match original {
            Some(o) => (o.mesh.clone(), o.material.clone(), o.scale),
            None => {
                commands.entity(entity).insert(Original { mesh: mesh.0.clone(), material: paint.0.clone(), scale: transform.scale });
                (mesh.0.clone(), paint.0.clone(), transform.scale)
            }
        };
        let texture = candidate(zone, picks.0[&zone]).map(|c| if surface.kind == SurfaceKind::Floor { c.1 } else { c.2 });
        match texture {
            None => {
                mesh.0 = orig_mesh;
                paint.0 = orig_material;
                transform.scale = orig_scale;
            }
            Some((id, size)) => {
                mesh.0 = meshes.add(box_uv_mesh(orig_scale, transform.translation, size));
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

/// Loads every candidate's images up front, so the first frame a set is shown it has them all.
fn preload(asset_server: Res<AssetServer>, mut library: ResMut<Library>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for zone in SLICE {
        for c in candidates(zone) {
            material(c.1.0, &asset_server, &mut library, &mut materials);
            material(c.2.0, &asset_server, &mut library, &mut materials);
        }
    }
}

// --- The screenshot tour ---------------------------------------------------------------------

/// (name, zone, feet, looking at, pitch)
type View = (&'static str, ZoneId, Vec3, Vec3, f32);

const ZONE_VIEWS: [View; 10] = [
    ("west-hall-room", ZoneId::WestHall, Vec3::new(-20.0, 0.0, 24.0), Vec3::new(-20.0, 0.0, -10.0), 0.08),
    ("west-hall-wall", ZoneId::WestHall, Vec3::new(-16.0, 0.0, 4.0), Vec3::new(-26.0, 0.0, 1.0), -0.15),
    ("dining-room-room", ZoneId::DiningRoom, Vec3::new(-24.0, 0.0, -16.0), Vec3::new(-2.0, 0.0, -24.0), -0.05),
    ("dining-room-wall", ZoneId::DiningRoom, Vec3::new(-14.0, 0.0, -17.0), Vec3::new(-11.0, 0.0, -26.0), -0.2),
    ("passage-room", ZoneId::Passage, Vec3::new(-12.0, 0.0, -26.5), Vec3::new(-12.0, 0.0, -38.0), -0.1),
    ("passage-wall", ZoneId::Passage, Vec3::new(-10.9, 0.0, -30.0), Vec3::new(-13.5, 0.0, -33.0), -0.25),
    ("workshop-room", ZoneId::Workshop, Vec3::new(-32.0, 0.0, -40.0), Vec3::new(-6.0, 0.0, -48.0), -0.05),
    ("workshop-wall", ZoneId::Workshop, Vec3::new(-24.0, 0.0, -42.0), Vec3::new(-20.0, 0.0, -50.0), -0.2),
    ("yard-room", ZoneId::Yard, Vec3::new(10.0, 0.0, 11.0), Vec3::new(-8.0, 0.0, -10.0), 0.05),
    ("yard-house", ZoneId::Yard, Vec3::new(4.0, 0.0, 2.0), Vec3::new(-4.0, 0.0, 14.0), 0.1),
];

const ROUTE_VIEWS: [View; 6] = [
    ("1-start-to-west", ZoneId::StartRoom, Vec3::new(-4.0, 0.0, 20.0), Vec3::new(-26.0, 0.0, 20.0), 0.0),
    ("2-west-to-dining", ZoneId::WestHall, Vec3::new(-20.0, 0.0, 0.0), Vec3::new(-20.0, 0.0, -24.0), 0.0),
    ("3-dining-to-passage", ZoneId::DiningRoom, Vec3::new(-12.0, 0.0, -15.0), Vec3::new(-12.0, 0.0, -36.0), -0.05),
    ("4-passage-to-workshop", ZoneId::Passage, Vec3::new(-12.0, 0.0, -28.0), Vec3::new(-14.0, 0.0, -48.0), -0.15),
    ("5-yard-to-start", ZoneId::Yard, Vec3::new(0.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 24.0), 0.0),
    ("6-yard-to-dining", ZoneId::Yard, Vec3::new(-7.0, 0.0, 8.0), Vec3::new(-7.0, 0.0, -24.0), 0.0),
];

/// Every shot: (view, picks override for its zone).
fn shots(route: bool) -> Vec<(View, Option<usize>)> {
    if route {
        return ROUTE_VIEWS.iter().map(|&v| (v, None)).collect();
    }
    let mut shots = Vec::new();
    for view in ZONE_VIEWS {
        for pick in 0..=candidates(view.1).len() {
            shots.push((view, Some(pick)));
        }
    }
    shots
}

#[derive(Resource, Default)]
struct Tour {
    dir: String,
    route: bool,
    frames: u32,
    shot: usize,
    settle: u32,
}

const TOUR_WARM_UP: u32 = 300;

fn start_run(mut map: ResMut<MapId>, mut mode: ResMut<Mode>, mut next: ResMut<NextState<GameState>>) {
    *map = MapId::Greybox;
    *mode = Mode::Classic;
    next.set(GameState::Playing);
}

fn keep_unhurt(mut commands: Commands, mut players: Query<&mut Health, With<Player>>, zombies: Query<Entity, With<crate::enemies::zombie::Zombie>>) {
    for mut health in &mut players {
        health.current = health.max;
    }
    for zombie in &zombies {
        commands.entity(zombie).despawn();
    }
}

fn tour(
    mut commands: Commands,
    mut tour: ResMut<Tour>,
    mut picks: ResMut<Picks>,
    mut look: ResMut<Look>,
    mut doors: Query<&mut Visibility, With<Door>>,
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
    let shots = shots(tour.route);
    if tour.shot == shots.len() {
        tour.settle += 1;
        if tour.settle > 30 {
            exit.write(AppExit::Success);
        }
        return;
    }
    let Ok(player) = player.single() else { return };
    let ((name, zone, feet, at, pitch), pick) = shots[tour.shot];
    let facing = Transform::from_translation(feet).looking_at(Vec3::new(at.x, feet.y, at.z), Vec3::Y).rotation;
    for mut transform in &mut body {
        transform.rotation = facing;
    }
    for mut transform in &mut camera {
        transform.rotation = Quat::from_rotation_x(pitch);
    }
    if tour.settle == 0 {
        teleports.write(Teleport { player, feet, facing: Some(facing) });
        if *look != Look::Dusk {
            *look = Look::Dusk;
        }
        if tour.route {
            for mut visibility in &mut doors {
                *visibility = Visibility::Hidden;
            }
        }
        if let Some(pick) = pick
            && picks.0[&zone] != pick
        {
            picks.0.insert(zone, pick);
        }
    }
    tour.settle += 1;
    let wait = if tour.shot == 0 { 200 } else { 50 };
    if tour.settle < wait {
        return;
    }
    let path = match pick {
        Some(pick) => format!("{}/{}-{}-{}.png", tour.dir, name, pick, pick_name(zone, pick)),
        None => format!("{}/{}.png", tour.dir, name),
    };
    info!("tour: {path}");
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    tour.settle = 0;
    tour.shot += 1;
}
