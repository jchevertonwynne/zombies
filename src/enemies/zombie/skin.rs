//! The Zombies' skinned bodies: MakeHuman men in shirt and trousers on the UAL skeleton
//! (`assets/enemies/zombie/`, made by `tools/zombie`), three bodies (labourer, soldier, civilian)
//! in two colourways each. They stand in for the box rig of a plain Zombie, and only once every file
//! is on disk and has loaded: with an asset missing, failed, or in headless runs (which never add
//! this plugin), the box rig stays.
//!
//! Purely visual, like the box rig: the hit areas, colliders and Headshot rules are untouched, and
//! the skin reads only what the box rig already works out (`Gait`), which on a client is told by the
//! Host's `Activity`. So a puppet animates the way the Host's Zombie does, and its body and
//! colourway come from the Zombie's wire id, so every machine shows the same one.
//!
//! Hybrid animation, as the Spec maps it. Clips drive standing about (`Zombie_Idle_Loop`), walking,
//! jogging and sprinting by the Zombie's speed (`Zombie_Walk_Fwd_Loop`, UAL1's `Jog_Fwd_Loop` and
//! `Sprint_Loop`), a swing (`Zombie_Scratch`, scrubbed by the swing's progress so the hit lands with
//! the blow), a stun (UAL1's `Hit_Chest`) and death (UAL1's `Death01`). Every other `Activity`
//! (a Plank's tear and yank, a vault, rising, a scream, a knockdown, a Swing Bridge's tip) and a
//! Crawler always are posed procedurally from `rig::pose` and `rig::crawl`, written onto the same
//! bones after the clips have run (see `procedural`).
//!
//! The head and each leg are separate parts: a headless kill hides the head and the legs gib away.
//! The glTF caps the neck and thigh stumps, so what is left is closed.

use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::{AnimationClip, AnimationPlayer, transition::AnimationTransitions};
use bevy::app::AnimationSystems;
use bevy::asset::io::file::FileAssetReader;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use super::rig::{Activity, Gait, Pose, Rig, Swing, crawl, pose};
use super::{Zombie, body_half_height};
use crate::enemies::sapper::Sapper;
use crate::enemies::special::SpecialKind;
use crate::net::zombies::{ZombieId, ZombieIds};
use crate::tuning::{CRAWLER_BODY_LENGTH, CRAWLER_RADIUS};

const CLIPS: &str = "enemies/zombie/clips.glb";
/// The bodies' glTFs, by `Look::body`: the three plain Zombies', then the specials' (Napalm,
/// Shrieker, Sapper, Nova Crawler). A special's body has parts of its own (a tank, a helmet, a gas
/// mask) and wears the clothes maps of a plain body.
const BODIES: [&str; BODY_COUNT] = [
    "enemies/zombie/labourer.glb",
    "enemies/zombie/soldier.glb",
    "enemies/zombie/civilian.glb",
    "enemies/zombie/napalm.glb",
    "enemies/zombie/shrieker.glb",
    "enemies/zombie/sapper.glb",
    "enemies/zombie/nova.glb",
];
/// Which of `MAPS` each body's shirt and trousers are, by `Look::body`.
const CLOTHES: [[usize; 2]; BODY_COUNT] = [[1, 2], [3, 4], [5, 6], [5, 6], [1, 2], [3, 4], [3, 4]];
/// The first of the special bodies in `BODIES`, which a Zombie's own kind picks.
const NAPALM_BODY: usize = 3;
const SHRIEKER_BODY: usize = 4;
const SAPPER_BODY: usize = 5;
const NOVA_BODY: usize = 6;
/// The colour maps: the skin every body shares, then each body's shirt and trousers.
const MAPS: [&str; 7] = [
    "enemies/zombie/skin/color.ktx2",
    "enemies/zombie/labourer_shirt/color.ktx2",
    "enemies/zombie/labourer_pants/color.ktx2",
    "enemies/zombie/soldier_shirt/color.ktx2",
    "enemies/zombie/soldier_pants/color.ktx2",
    "enemies/zombie/civilian_shirt/color.ktx2",
    "enemies/zombie/civilian_pants/color.ktx2",
];

/// How many bodies there are (the plain ones, then the specials'), and colourways of each.
const BODY_COUNT: usize = 7;
const WAY_COUNT: usize = 2;
/// How many of them a plain Zombie wears.
const PLAIN_BODIES: usize = 3;

/// Colour multipliers on the baked maps, by body and colourway: skin, shirt, trousers. They make a
/// second colourway of each body from the same maps, for no more files.
const TINTS: [[[[f32; 3]; 3]; WAY_COUNT]; BODY_COUNT] = [
    // Labourer: as baked, then a paler man in a bluer sweater.
    [[[1.0, 1.0, 1.0]; 3], [[1.1, 1.1, 1.16], [0.8, 0.9, 1.15], [0.92, 0.88, 0.82]]],
    // Soldier: olive drab, then desert sand.
    [[[1.0, 1.0, 1.0]; 3], [[1.1, 1.04, 0.92], [1.5, 1.2, 0.8], [1.5, 1.25, 0.85]]],
    // Civilian: as baked, then a red polo and darker shorts.
    [[[1.0, 1.0, 1.0]; 3], [[1.06, 1.0, 1.0], [1.5, 0.7, 0.65], [0.7, 0.72, 0.8]]],
    // The specials have one colourway, in both. Napalm: soot-black clothes on scorched skin.
    [[[0.6, 0.5, 0.45], [0.4, 0.34, 0.3], [0.5, 0.45, 0.4]]; WAY_COUNT],
    // Shrieker: washed-out, bloodless.
    [[[1.2, 1.2, 1.3], [1.25, 1.25, 1.3], [1.1, 1.1, 1.15]]; WAY_COUNT],
    // Sapper: dark, under its armour.
    [[[0.8, 0.8, 0.8], [0.5, 0.5, 0.45], [0.55, 0.55, 0.5]]; WAY_COUNT],
    // Nova Crawler: a sickly yellow-green, gassed.
    [[[0.9, 1.0, 0.75], [1.3, 1.35, 0.6], [1.1, 1.2, 0.6]]; WAY_COUNT],
];

/// A special body's gear (a tank, armour, a mask) and its glow (the flame, the canisters, the lenses):
/// the materials of a body's `...Gear` and `...Glow` parts. A plain body has neither.
fn gear(body: usize) -> [StandardMaterial; 2] {
    let metal = |color: Color, metallic: f32, roughness: f32| StandardMaterial {
        base_color: color,
        metallic,
        perceptual_roughness: roughness,
        ..default()
    };
    let glow = |color: Color, emissive: LinearRgba| StandardMaterial { base_color: color, emissive, perceptual_roughness: 0.6, ..default() };
    match body {
        // A scorched, rust-red tank, its flame burning at the top.
        NAPALM_BODY => [
            metal(Color::srgb(0.32, 0.09, 0.06), 0.6, 0.55),
            glow(Color::srgb(0.9, 0.35, 0.05), LinearRgba::rgb(7.0, 2.2, 0.15)),
        ],
        // Dull armour plate, and a visor's red slit that no part uses yet.
        SAPPER_BODY => [metal(Color::srgb(0.3, 0.31, 0.33), 0.85, 0.4), glow(Color::srgb(0.5, 0.1, 0.05), LinearRgba::rgb(4.0, 0.4, 0.1))],
        // A rubber mask and green canisters.
        NOVA_BODY => [metal(Color::srgb(0.06, 0.08, 0.06), 0.1, 0.55), glow(Color::srgb(0.3, 0.8, 0.2), LinearRgba::rgb(0.6, 3.5, 0.5))],
        _ => [metal(Color::srgb(0.3, 0.3, 0.3), 0.5, 0.6), glow(Color::srgb(0.5, 0.5, 0.5), LinearRgba::BLACK)],
    }
}

const WALK: &str = "Zombie_Walk_Fwd_Loop";
const IDLE: &str = "Zombie_Idle_Loop";
const JOG: &str = "Jog_Fwd_Loop";
const SPRINT: &str = "Sprint_Loop";
const SCRATCH: &str = "Zombie_Scratch";
const DEATH: &str = "Death01";
const HIT: &str = "Hit_Chest";

/// How much bigger the model is drawn than modelled (1.75 m), so its head tops the head hit area.
const SKIN_SCALE: f32 = 1.03;
/// The ground speed (metres a second) each locomotion clip is authored for, from how fast the
/// planted feet slide in them (`build_clips.py` prints it as `FOOT SPEED`): the clip plays at the
/// Zombie's speed over this.
const WALK_SPEED: f32 = 1.0;
const JOG_SPEED: f32 = 3.6;
const SPRINT_SPEED: f32 = 5.0;
/// Under this speed (metres a second) a Zombie stands about rather than walks, and from these it
/// jogs and sprints (the Speed Tiers' speeds fall between them).
const STANDING_SPEED: f32 = 0.25;
const JOG_FROM: f32 = 2.5;
const SPRINT_FROM: f32 = 4.5;
/// Where in `Zombie_Scratch` the blow lands, as a share of the clip: a swing's progress runs the
/// clip up to there.
const SCRATCH_LANDS: f32 = 15.0 / 43.0;
/// How long the walk, jog and idle blend into each other.
const BLEND: Duration = Duration::from_millis(250);
/// How far the middle of a prone body is above the floor, and how long a prone model's head
/// reaches from its hips: it lies along the facing.
const PRONE_LIFT: f32 = 0.14;
const MODEL_HEAD_HEIGHT: f32 = 1.65;

pub struct SkinPlugin;

impl Plugin for SkinPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bodies>()
            .add_systems(Startup, load_bodies)
            .add_systems(
                Update,
                (prepare_bodies, attach, dress_parts, setup_players, drive_clips, show_parts, hide_boxes, hide_late_boxes, sync_frame).chain(),
            )
            .add_systems(
                PostUpdate,
                (build_skeletons.before(AnimationSystems), pose_skins.after(AnimationSystems).before(TransformSystems::Propagate)),
            );
    }
}

/// What the bodies' files have come to.
#[derive(Resource, Default)]
struct Bodies(State);

#[derive(Default)]
enum State {
    /// Not on disk, or failed: the box rig, for good.
    #[default]
    Absent,
    Loading {
        clips: Handle<Gltf>,
        bodies: [Handle<Gltf>; BODY_COUNT],
        images: [Handle<Image>; MAPS.len()],
    },
    Ready(Ready),
}

/// The clip nodes of the graph.
struct Nodes {
    idle: AnimationNodeIndex,
    walk: AnimationNodeIndex,
    jog: AnimationNodeIndex,
    sprint: AnimationNodeIndex,
    scratch: AnimationNodeIndex,
    death: AnimationNodeIndex,
    hit: AnimationNodeIndex,
}

struct Ready {
    scenes: [Handle<WorldAsset>; BODY_COUNT],
    /// Skin, shirt, trousers, gear and glow, for each body and colourway (`body * WAY_COUNT + way`).
    materials: Vec<[Handle<StandardMaterial>; 5]>,
    graph: Handle<AnimationGraph>,
    nodes: Nodes,
    /// How long `Zombie_Scratch` runs, in seconds.
    scratch_secs: f32,
}

/// Whether `path` (under `assets/`) is on disk.
fn on_disk(path: &str) -> bool {
    FileAssetReader::get_base_path().join("assets").join(path).is_file()
}

/// Whether every file is on disk.
fn files_present(present: impl Fn(&str) -> bool) -> bool {
    present(CLIPS) && BODIES.iter().all(|body| present(body)) && MAPS.iter().all(|map| present(map))
}

fn load_bodies(mut bodies: ResMut<Bodies>, asset_server: Res<AssetServer>) {
    if !files_present(on_disk) {
        debug!("the Zombie bodies' files are missing, staying boxes");
        return;
    }
    bodies.0 = State::Loading {
        clips: asset_server.load(CLIPS),
        bodies: BODIES.map(|body| asset_server.load(body)),
        images: MAPS.map(|map| asset_server.load(map)),
    };
}

/// Builds the materials and clip graph once everything has loaded.
fn prepare_bodies(
    mut state: ResMut<Bodies>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    animations: Res<Assets<AnimationClip>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let State::Loading { clips, bodies, images } = &state.0 else { return };
    let failed = |id: bevy::asset::UntypedAssetId| matches!(asset_server.load_state(id), bevy::asset::LoadState::Failed(_));
    if failed(clips.id().untyped()) || bodies.iter().any(|body| failed(body.id().untyped())) || images.iter().any(|image| failed(image.id().untyped())) {
        warn!("a Zombie body failed to load, staying boxes");
        state.0 = State::Absent;
        return;
    }
    let loaded = asset_server.is_loaded_with_dependencies(clips)
        && bodies.iter().all(|body| asset_server.is_loaded_with_dependencies(body))
        && images.iter().all(|image| asset_server.is_loaded_with_dependencies(image));
    if !loaded {
        return;
    }
    let Some(clips) = gltfs.get(clips) else { return };
    let clip = |name: &str| clips.named_animations.get(name).cloned();
    let (Some(idle), Some(walk), Some(jog), Some(sprint), Some(scratch), Some(death), Some(hit)) =
        (clip(IDLE), clip(WALK), clip(JOG), clip(SPRINT), clip(SCRATCH), clip(DEATH), clip(HIT))
    else {
        warn!("the Zombie clips file lacks a clip, staying boxes");
        state.0 = State::Absent;
        return;
    };
    let scenes: Vec<Option<Handle<WorldAsset>>> = bodies.iter().map(|body| gltfs.get(body).and_then(|file| file.scenes.first().cloned())).collect();
    let Some(scenes) = scenes.into_iter().collect::<Option<Vec<_>>>().and_then(|scenes| <[Handle<WorldAsset>; BODY_COUNT]>::try_from(scenes).ok()) else {
        warn!("a Zombie body's glTF has no scene, staying boxes");
        state.0 = State::Absent;
        return;
    };
    let scratch_secs = animations.get(&scratch).map_or(1.8, AnimationClip::duration);
    let mut graph = AnimationGraph::new();
    let nodes = Nodes {
        idle: graph.add_clip(idle, 1.0, graph.root),
        walk: graph.add_clip(walk, 1.0, graph.root),
        jog: graph.add_clip(jog, 1.0, graph.root),
        sprint: graph.add_clip(sprint, 1.0, graph.root),
        scratch: graph.add_clip(scratch, 1.0, graph.root),
        death: graph.add_clip(death, 1.0, graph.root),
        hit: graph.add_clip(hit, 1.0, graph.root),
    };
    let cloth = |image: &Handle<Image>, tint: [f32; 3]| StandardMaterial {
        base_color: Color::linear_rgb(tint[0], tint[1], tint[2]),
        base_color_texture: Some(image.clone()),
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        ..default()
    };
    let mut made = Vec::new();
    for body in 0..BODY_COUNT {
        let [metal, glow] = gear(body).map(|material| materials.add(material));
        for way in 0..WAY_COUNT {
            let tint = TINTS[body][way];
            let [shirt, pants] = CLOTHES[body];
            made.push([
                materials.add(cloth(&images[0], tint[0])),
                materials.add(cloth(&images[shirt], tint[1])),
                materials.add(cloth(&images[pants], tint[2])),
                metal.clone(),
                glow.clone(),
            ]);
        }
    }
    state.0 = State::Ready(Ready { scenes, materials: made, graph: graphs.add(graph), nodes, scratch_secs });
}

/// Which body and colourway a Zombie wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub body: usize,
    pub way: usize,
}

impl Look {
    /// From a number every machine agrees on (the Zombie's wire id): the same Zombie wears the
    /// same body on the Host and on every client.
    pub fn of(key: u32) -> Self {
        // A cheap integer hash, so consecutive ids don't wear consecutive bodies.
        let mut h = key.wrapping_mul(0x9E37_79B1);
        h ^= h >> 15;
        h = h.wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
        Self { body: h as usize % PLAIN_BODIES, way: (h >> 8) as usize % WAY_COUNT }
    }

    /// The body a Zombie wears: its own if it is a special, else one of the plain bodies by `key`.
    /// `None` for a Hellhound, which has a body of its own elsewhere.
    fn of_zombie(zombie: &Zombie, key: u32) -> Option<Self> {
        let body = match zombie.special_kind() {
            Some(SpecialKind::Napalm) => NAPALM_BODY,
            Some(SpecialKind::Shrieker) => SHRIEKER_BODY,
            Some(SpecialKind::Sapper) => SAPPER_BODY,
            None if zombie.is_nova_crawler() => NOVA_BODY,
            None if zombie.is_hellhound() => return None,
            None => return Some(Self::of(key)),
        };
        Some(Self { body, way: 0 })
    }

    fn materials(self) -> usize {
        self.body * WAY_COUNT + self.way
    }
}

/// A Zombie that wears a skinned body.
#[derive(Component)]
pub(super) struct Skinned;

/// The scene root of a Zombie's skinned body.
#[derive(Component)]
struct SkinRoot {
    zombie: Entity,
    look: Look,
}

/// Which part of the body a mesh is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Head,
    Legs,
    /// The Sapper's helmet, which comes off.
    Helmet,
    Rest,
}

impl Part {
    /// From a mesh's name in the glTF, which is its mesh then its material (`Soldier_Head.ZombieSkin`,
    /// `Labourer_PantsLegR.ZombiePants`...).
    fn of(name: &str) -> Self {
        let name = name.split('.').next().unwrap_or(name);
        if name.ends_with("Head") {
            Part::Head
        } else if name.ends_with("LegL") || name.ends_with("LegR") {
            Part::Legs
        } else if name.ends_with("Helmet") {
            Part::Helmet
        } else {
            Part::Rest
        }
    }

    /// Which of skin, shirt, trousers, gear and glow it wears, by its material's name (`...Shirt`,
    /// `...Pants`, `...Gear`, `...Glow`).
    fn material(name: &str) -> usize {
        if name.contains("Gear") {
            3
        } else if name.contains("Glow") {
            4
        } else if name.contains("Shirt") {
            1
        } else if name.contains("Pants") {
            2
        } else {
            0
        }
    }
}

#[derive(Component)]
struct SkinPart {
    zombie: Entity,
    part: Part,
}

/// Whether a part shows: the head goes with a headshot kill, the legs with a gib (a Nova Crawler
/// has none to show either), and the helmet with the Sapper's helmet or its head.
fn part_shows(part: Part, headless: bool, prone: bool, helmet: bool) -> bool {
    match part {
        Part::Head => !headless,
        Part::Legs => !prone,
        Part::Helmet => helmet && !headless,
        Part::Rest => true,
    }
}

/// Whether a Zombie's body lies prone, along its facing: a Crawler's and a Nova Crawler's.
fn is_prone(zombie: &Zombie) -> bool {
    zombie.is_crawler() || zombie.is_nova_crawler()
}

/// Where the model sits on its Zombie: standing with its feet on the floor and its face to -Z, or,
/// for a Crawler, lying along its facing, belly down, head at the front.
fn skin_transform(crawler: bool) -> Transform {
    let facing = Quat::from_rotation_y(PI);
    if !crawler {
        return Transform::from_xyz(0.0, -body_half_height(), 0.0).with_rotation(facing).with_scale(Vec3::splat(SKIN_SCALE));
    }
    let front = -(CRAWLER_BODY_LENGTH / 2.0 + CRAWLER_RADIUS);
    Transform::from_xyz(0.0, -CRAWLER_RADIUS + PRONE_LIFT, front + MODEL_HEAD_HEIGHT * SKIN_SCALE)
        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2) * facing)
        .with_scale(Vec3::splat(SKIN_SCALE))
}

/// Puts a body on each plain Zombie once it's ready. Its look comes from its wire id, so on a
/// Host in a co-op room it waits for the id to be given.
fn attach(
    mut commands: Commands,
    bodies: Res<Bodies>,
    ids: Option<Res<ZombieIds>>,
    zombies: Query<(Entity, &Zombie, Option<&ZombieId>), (With<Rig>, Without<Skinned>)>,
) {
    let State::Ready(ready) = &bodies.0 else { return };
    for (entity, zombie, id) in &zombies {
        // A special's body is its own whatever its id, so it never waits for one.
        let key = match id {
            Some(id) => u32::from(id.0),
            None if ids.is_some() && !zombie.is_special() => continue,
            None => entity.index().index(),
        };
        let Some(look) = Look::of_zombie(zombie, key) else { continue };
        commands.entity(entity).insert(Skinned);
        commands.spawn((
            Name::new("Zombie body"),
            SkinRoot { zombie: entity, look },
            WorldAssetRoot(ready.scenes[look.body].clone()),
            skin_transform(is_prone(zombie)),
            ChildOf(entity),
        ));
    }
}

/// Despawns a Zombie's box parts and hit areas, leaving its skinned body (if it has one) to be
/// re-posed: the first step in swapping a standing body for a Crawler's.
pub(super) fn despawn_body(mut zombie: EntityWorldMut) {
    let children: Vec<Entity> = zombie.get::<Children>().map(|children| children.iter().collect()).unwrap_or_default();
    zombie.world_scope(|world| {
        for child in children {
            if world.get::<SkinRoot>(child).is_none() {
                world.despawn(child);
            }
        }
    });
}

/// Keeps the body lying down once its Zombie loses its legs.
fn sync_frame(mut roots: Query<(&SkinRoot, &mut Transform)>, zombies: Query<&Zombie>) {
    for (root, mut transform) in &mut roots {
        let Ok(zombie) = zombies.get(root.zombie) else { continue };
        let wanted = skin_transform(is_prone(zombie));
        if *transform != wanted {
            *transform = wanted;
        }
    }
}

fn skin_root_of(mut at: Entity, parents: &Query<&ChildOf>, roots: &Query<&SkinRoot>) -> Option<(Entity, Entity)> {
    loop {
        if let Ok(root) = roots.get(at) {
            return Some((at, root.zombie));
        }
        at = parents.get(at).ok()?.parent();
    }
}

/// Dresses each mesh of a new body in its material, and notes which part it is.
fn dress_parts(
    mut commands: Commands,
    bodies: Res<Bodies>,
    meshes: Query<(Entity, &Name), Added<Mesh3d>>,
    parents: Query<&ChildOf>,
    roots: Query<&SkinRoot>,
) {
    let State::Ready(ready) = &bodies.0 else { return };
    for (entity, name) in &meshes {
        let Some((root, zombie)) = skin_root_of(entity, &parents, &roots) else { continue };
        let Ok(root) = roots.get(root) else { continue };
        commands.entity(entity).insert((
            MeshMaterial3d(ready.materials[root.look.materials()][Part::material(name)].clone()),
            SkinPart { zombie, part: Part::of(name) },
        ));
    }
}

/// Which clip a body's player is playing.
#[derive(Component)]
struct Playing(AnimationNodeIndex);

#[derive(Component)]
struct SkinPlayer {
    zombie: Entity,
}

/// Gives each new body's animation player the clip graph.
fn setup_players(
    mut commands: Commands,
    bodies: Res<Bodies>,
    players: Query<Entity, Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    roots: Query<&SkinRoot>,
) {
    let State::Ready(ready) = &bodies.0 else { return };
    for entity in &players {
        let Some((_, zombie)) = skin_root_of(entity, &parents, &roots) else { continue };
        commands.entity(entity).insert((AnimationGraphHandle(ready.graph.clone()), AnimationTransitions::new(), SkinPlayer { zombie }));
    }
}

/// What a clip-driven Zombie plays.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Want {
    Idle,
    /// A locomotion clip at this multiple of its authored pace.
    Walk(f32),
    Jog(f32),
    Sprint(f32),
    /// The swing's scratch, scrubbed to `progress` of the way to landing.
    Scratch(f32),
    /// Death, played through from where it began.
    Death,
    Flinch,
}

/// Which clip (if any) drives a Zombie doing `activity` at `speed`. Everything else (and every
/// Crawler) is posed procedurally.
fn want(activity: Activity, speed: f32) -> Option<Want> {
    match activity {
        Activity::Moving if speed <= STANDING_SPEED => Some(Want::Idle),
        Activity::Moving if speed < JOG_FROM => Some(Want::Walk((speed / WALK_SPEED).clamp(0.5, 2.5))),
        Activity::Moving if speed < SPRINT_FROM => Some(Want::Jog((speed / JOG_SPEED).clamp(0.6, 1.8))),
        Activity::Moving => Some(Want::Sprint((speed / SPRINT_SPEED).clamp(0.6, 1.8))),
        // The Sapper's slam, with both arms overhead, is posed by `rig::pose`: no clip does it.
        Activity::Swinging { arm: Swing::Both, .. } => None,
        Activity::Swinging { progress, .. } => Some(Want::Scratch(progress.clamp(0.0, 1.0))),
        Activity::Dying { .. } => Some(Want::Death),
        Activity::Stunned => Some(Want::Flinch),
        _ => None,
    }
}

/// Plays the clip each standing Zombie's `Activity` calls for. A Crawler plays none.
fn drive_clips(
    mut commands: Commands,
    bodies: Res<Bodies>,
    mut players: Query<(Entity, &SkinPlayer, &mut AnimationPlayer, &mut AnimationTransitions, Option<&Playing>)>,
    zombies: Query<(&Zombie, &Gait)>,
) {
    let State::Ready(ready) = &bodies.0 else { return };
    let nodes = &ready.nodes;
    for (entity, skin, mut player, mut transitions, playing) in &mut players {
        let Ok((zombie, gait)) = zombies.get(skin.zombie) else { continue };
        if is_prone(zombie) {
            continue;
        }
        let Some(wanted) = want(gait.activity(), gait.speed()) else { continue };
        let (node, pace, repeat) = match wanted {
            Want::Idle => (nodes.idle, 1.0, true),
            Want::Walk(pace) => (nodes.walk, pace, true),
            Want::Jog(pace) => (nodes.jog, pace, true),
            Want::Sprint(pace) => (nodes.sprint, pace, true),
            // Scrubbed by hand, so frozen between seeks.
            Want::Scratch(_) => (nodes.scratch, 0.0, true),
            Want::Death => (nodes.death, 1.0, false),
            Want::Flinch => (nodes.hit, 0.6, true),
        };
        if playing.is_none_or(|playing| playing.0 != node) {
            let blend = match wanted {
                Want::Death | Want::Scratch(_) => Duration::from_millis(80),
                _ if playing.is_some() => BLEND,
                _ => Duration::ZERO,
            };
            let active = transitions.play(&mut player, node, blend);
            if repeat {
                active.repeat();
            }
            commands.entity(entity).insert(Playing(node));
        }
        if let Some(active) = player.animation_mut(node) {
            match wanted {
                Want::Scratch(progress) => {
                    active.set_speed(0.0).seek_to(progress * SCRATCH_LANDS * ready.scratch_secs);
                }
                Want::Death => {}
                _ => {
                    active.set_speed(pace);
                }
            }
        }
    }
}

/// Hides the head after a headshot kill and the legs after a gib.
fn show_parts(mut parts: Query<(&SkinPart, &mut Visibility)>, zombies: Query<(&Zombie, &Gait, Option<&Sapper>)>) {
    for (part, mut visibility) in &mut parts {
        let Ok((zombie, gait, sapper)) = zombies.get(part.zombie) else { continue };
        let headless = matches!(gait.activity(), Activity::Dying { headless: true, .. });
        let helmet = sapper.is_some_and(|sapper| !sapper.helmet_lost());
        let wanted = if part_shows(part.part, headless, is_prone(zombie), helmet) { Visibility::Inherited } else { Visibility::Hidden };
        visibility.set_if_neq(wanted);
    }
}

/// Hides the box rig of a skinned Zombie: the hit areas stay, only their boxes go.
fn hide_boxes(
    mut commands: Commands,
    skinned: Query<(Entity, Has<Mesh3d>), (With<Skinned>, Or<(Changed<Rig>, Added<Skinned>)>)>,
    children: Query<&Children>,
    boxes: Query<(), With<Mesh3d>>,
    roots: Query<(), With<SkinRoot>>,
    mut visibility: Query<&mut Visibility>,
) {
    for (entity, has_mesh) in &skinned {
        // A Crawler's body is a mesh of the Zombie's own.
        if has_mesh {
            commands.entity(entity).remove::<Mesh3d>();
        }
        let mut stack: Vec<Entity> = children.get(entity).map(|c| c.iter().collect()).unwrap_or_default();
        while let Some(at) = stack.pop() {
            if roots.contains(at) {
                continue;
            }
            if boxes.contains(at)
                && let Ok(mut visibility) = visibility.get_mut(at)
            {
                visibility.set_if_neq(Visibility::Hidden);
            }
            if let Ok(below) = children.get(at) {
                stack.extend(below.iter());
            }
        }
    }
}

/// Hides a box that joins a skinned Zombie after it was skinned: the Sapper's helmet and plate, which
/// are dressed (and a puppet's helmet re-formed) whenever the Sapper's own systems get to it.
fn hide_late_boxes(
    mut new: Query<(Entity, &mut Visibility), Added<Mesh3d>>,
    parents: Query<&ChildOf>,
    skinned: Query<(), With<Skinned>>,
    roots: Query<(), With<SkinRoot>>,
) {
    for (entity, mut visibility) in &mut new {
        let mut at = entity;
        loop {
            if roots.contains(at) {
                break;
            }
            if skinned.contains(at) {
                visibility.set_if_neq(Visibility::Hidden);
                break;
            }
            let Ok(parent) = parents.get(at) else { break };
            at = parent.parent();
        }
    }
}

/// Whether a skinned Zombie dies by its clip, which falls the body over itself: a standing one.
/// Its root then stays upright, where a box rig's tips over.
pub(super) fn dies_by_clip(skinned: bool, zombie: Option<&Zombie>) -> bool {
    skinned && zombie.is_some_and(|zombie| !is_prone(zombie))
}

/// A node of the body's skeleton, as it stands at rest.
#[derive(Debug, Clone)]
pub struct Joint {
    pub name: String,
    pub parent: Option<usize>,
    /// Its local transform at rest.
    pub rest: Transform,
    /// Its rotation at rest in the body's own space.
    pub rest_global: Quat,
    /// The way its first child lies from it, in its own frame: where the bone points.
    pub axis: Vec3,
}

/// Every node of a body, parents first, with its rest pose. Built from the body as it spawns, before
/// any clip has played.
#[derive(Component)]
struct Skeleton {
    entities: Vec<Entity>,
    joints: Vec<Joint>,
}

fn build_skeletons(
    mut commands: Commands,
    roots: Query<(Entity, &Children), (With<SkinRoot>, Without<Skeleton>)>,
    nodes: Query<(&Name, &Transform, Option<&Children>)>,
) {
    for (root, top) in &roots {
        let mut entities = Vec::new();
        let mut joints: Vec<Joint> = Vec::new();
        let mut stack: Vec<(Entity, Option<usize>)> = top.iter().rev().map(|e| (e, None)).collect();
        while let Some((entity, parent)) = stack.pop() {
            let Ok((name, transform, below)) = nodes.get(entity) else { continue };
            let parent_global = parent.map_or(Quat::IDENTITY, |p| joints[p].rest_global);
            let at = joints.len();
            joints.push(Joint {
                name: name.as_str().to_string(),
                parent,
                rest: *transform,
                rest_global: parent_global * transform.rotation,
                axis: Vec3::X,
            });
            entities.push(entity);
            if let Some(below) = below {
                for child in below.iter().rev() {
                    stack.push((child, Some(at)));
                }
            }
        }
        // The pelvis is there once the scene has spawned: until then, try again.
        if !joints.iter().any(|joint| joint.name == "pelvis") {
            continue;
        }
        for i in 0..joints.len() {
            if let Some(child) = (0..joints.len()).find(|&c| joints[c].parent == Some(i)) {
                let along = joints[child].rest.translation;
                if along.length() > 1e-4 {
                    joints[i].axis = along.normalize();
                }
            }
        }
        commands.entity(root).insert(Skeleton { entities, joints });
    }
}

/// The local rotation of every joint for a body posed as `pose` (arms and legs as `rig::pose`
/// says them, in the Zombie's own frame), `frame` being the body's rotation on its Zombie.
/// Joints the pose doesn't name keep their rest rotation, following their parents.
pub fn procedural(joints: &[Joint], frame: Quat, pose: &Pose) -> Vec<Quat> {
    let to_body = |turn: Quat| frame.inverse() * turn * frame;
    let upper = to_body(Quat::from_rotation_y(pose.twist) * Quat::from_rotation_x(-pose.lean));
    let nod = to_body(Quat::from_rotation_x(-pose.nod));
    let down = frame.inverse() * Vec3::NEG_Y;
    // A limb that hangs from its joint, turned as `turn` says in the Zombie's frame.
    let swung = |joint: &Joint, turn: Quat| {
        let along = (joint.rest_global * joint.axis).normalize();
        to_body(turn) * Quat::from_rotation_arc(along, down) * joint.rest_global
    };
    let mut global: Vec<Quat> = Vec::with_capacity(joints.len());
    let mut local = Vec::with_capacity(joints.len());
    for joint in joints {
        let parent = joint.parent.map_or(Quat::IDENTITY, |p| global[p]);
        let natural = parent * joint.rest.rotation;
        let side = |name: &str| usize::from(name.ends_with("_r"));
        let spread = |share: f32| Quat::IDENTITY.slerp(upper, share) * joint.rest_global;
        let desired = match joint.name.as_str() {
            "spine_01" => spread(1.0 / 3.0),
            "spine_02" => spread(2.0 / 3.0),
            "spine_03" => spread(1.0),
            "Head" => nod * natural,
            "upperarm_l" | "upperarm_r" => {
                let side = side(&joint.name);
                upper * swung(joint, pose.arms[side].rotation(side))
            }
            "thigh_l" | "thigh_r" => swung(joint, Quat::from_rotation_x(pose.legs[side(&joint.name)])),
            _ => natural,
        };
        global.push(desired);
        local.push(parent.inverse() * desired);
    }
    local
}

/// Poses each body that isn't playing a clip (see `want`), and a Crawler always.
fn pose_skins(
    skins: Query<(&SkinRoot, &Skeleton, &Transform)>,
    zombies: Query<(&Zombie, &Gait)>,
    mut bones: Query<&mut Transform, Without<SkinRoot>>,
) {
    for (root, skeleton, transform) in &skins {
        let Ok((zombie, gait)) = zombies.get(root.zombie) else { continue };
        let activity = gait.activity();
        let crawler = is_prone(zombie);
        if !crawler && want(activity, gait.speed()).is_some() {
            continue;
        }
        let stride = (gait.speed() / 1.2).min(1.0);
        let run = ((gait.speed() - 2.5) / 2.5).clamp(0.0, 1.0);
        let posed = if crawler {
            Pose { arms: crawl(activity, gait.phase(), stride, gait.clock()), ..default() }
        } else {
            pose(activity, gait.phase(), stride, run, gait.clock())
        };
        let rotations = procedural(&skeleton.joints, transform.rotation, &posed);
        for ((entity, joint), rotation) in skeleton.entities.iter().zip(&skeleton.joints).zip(rotations) {
            if let Ok(mut bone) = bones.get_mut(*entity) {
                bone.rotation = rotation;
                bone.translation = joint.rest.translation;
                bone.scale = joint.rest.scale;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::rig::Limb;
    use super::*;

    /// A tiny T-pose skeleton in the body's space (+Y up, facing +Z, its left at +X): a root, a
    /// pelvis, three spine bones and a left arm of two bones lying out along +X, and a left leg.
    fn t_pose() -> Vec<Joint> {
        let mut joints: Vec<Joint> = Vec::new();
        let mut add = |name: &str, parent: Option<usize>, at: Vec3| {
            let rest = Transform::from_translation(at);
            let rest_global = parent.map_or(Quat::IDENTITY, |p| joints[p].rest_global);
            joints.push(Joint { name: name.into(), parent, rest, rest_global, axis: Vec3::X });
            joints.len() - 1
        };
        let root = add("root", None, Vec3::ZERO);
        let pelvis = add("pelvis", Some(root), Vec3::Y * 0.9);
        let s1 = add("spine_01", Some(pelvis), Vec3::Y * 0.15);
        let s2 = add("spine_02", Some(s1), Vec3::Y * 0.15);
        let s3 = add("spine_03", Some(s2), Vec3::Y * 0.15);
        let arm = add("upperarm_l", Some(s3), Vec3::new(0.2, 0.1, 0.0));
        add("lowerarm_l", Some(arm), Vec3::new(0.3, 0.0, 0.0));
        let thigh = add("thigh_l", Some(pelvis), Vec3::new(0.1, 0.0, 0.0));
        add("calf_l", Some(thigh), Vec3::new(0.0, -0.4, 0.0));
        let n = joints.len();
        for i in 0..n {
            if let Some(child) = (0..n).find(|&c| joints[c].parent == Some(i)) {
                joints[i].axis = joints[child].rest.translation.normalize();
            }
        }
        joints
    }

    /// Where a joint's bone points, in the Zombie's frame, with the body turned `frame` on it.
    fn bone_dir(joints: &[Joint], locals: &[Quat], name: &str, frame: Quat) -> Vec3 {
        let mut global = Vec::new();
        for (joint, local) in joints.iter().zip(locals) {
            global.push(joint.parent.map_or(Quat::IDENTITY, |p| global[p]) * *local);
        }
        let i = joints.iter().position(|joint| joint.name == name).unwrap();
        frame * (global[i] * joints[i].axis)
    }

    /// The model faces +Z; its Zombie's -Z is forward, so it's turned half way about Y.
    fn facing() -> Quat {
        Quat::from_rotation_y(PI)
    }

    #[test]
    fn an_arm_with_no_turn_hangs_straight_down() {
        let joints = t_pose();
        let locals = procedural(&joints, facing(), &Pose::default());
        let dir = bone_dir(&joints, &locals, "upperarm_l", facing());
        assert!(dir.abs_diff_eq(Vec3::NEG_Y, 1e-4), "{dir}");
    }

    #[test]
    fn a_pitched_arm_reaches_forward_where_the_box_rig_does() {
        let joints = t_pose();
        let posed = Pose { arms: [Limb::pitch(FRAC_PI_2); 2], ..default() };
        let locals = procedural(&joints, facing(), &posed);
        let dir = bone_dir(&joints, &locals, "upperarm_l", facing());
        assert!(dir.abs_diff_eq(Vec3::NEG_Z, 1e-4), "forward is -Z: {dir}");
        // The same turn on the box rig's own hanging arm.
        let box_arm = posed.arms[0].rotation(0) * Vec3::NEG_Y;
        assert!(dir.abs_diff_eq(box_arm, 1e-4));
    }

    #[test]
    fn a_leg_swings_forward_with_its_pitch() {
        let joints = t_pose();
        let posed = Pose { legs: [FRAC_PI_2, 0.0], ..default() };
        let locals = procedural(&joints, facing(), &posed);
        let dir = bone_dir(&joints, &locals, "thigh_l", facing());
        assert!(dir.abs_diff_eq(Vec3::NEG_Z, 1e-4), "{dir}");
    }

    #[test]
    fn leaning_tips_the_chest_forward() {
        let joints = t_pose();
        let posed = Pose { lean: 0.3, ..default() };
        let locals = procedural(&joints, facing(), &posed);
        let up = {
            let mut global = Vec::new();
            for (joint, local) in joints.iter().zip(&locals) {
                global.push(joint.parent.map_or(Quat::IDENTITY, |p| global[p]) * *local);
            }
            facing() * (global[4] * Vec3::Y)
        };
        assert!(up.z < -0.2, "the spine leans toward -Z: {up}");
    }

    #[test]
    fn a_lying_body_reaches_its_arms_along_the_floor() {
        // Prone: the body lies along the Zombie's -Z, belly down.
        let frame = Quat::from_rotation_x(-FRAC_PI_2) * facing();
        let joints = t_pose();
        let posed = Pose { arms: [Limb::pitch(1.45); 2], ..default() };
        let locals = procedural(&joints, frame, &posed);
        let dir = bone_dir(&joints, &locals, "upperarm_l", frame);
        assert!(dir.z < -0.9, "out in front along the floor: {dir}");
    }

    #[test]
    fn a_headshot_takes_the_head_and_a_gib_the_legs() {
        assert!(!part_shows(Part::Head, true, false, false));
        assert!(part_shows(Part::Head, false, true, false));
        assert!(!part_shows(Part::Legs, false, true, false));
        assert!(part_shows(Part::Legs, true, false, false));
        assert!(part_shows(Part::Rest, true, true, false));
    }

    #[test]
    fn the_sappers_helmet_shows_only_while_it_holds_and_the_head_is_on() {
        assert!(part_shows(Part::Helmet, false, false, true));
        assert!(!part_shows(Part::Helmet, false, false, false), "the helmet is lost");
        assert!(!part_shows(Part::Helmet, true, false, true), "it goes with the head");
        assert_eq!(Part::of("Sapper_Helmet.ZombieGear"), Part::Helmet);
        // A mask on a Nova Crawler's head is part of the head, so it goes with it.
        assert_eq!(Part::of("Nova_MaskHead.ZombieGear"), Part::Head);
        assert_eq!(Part::of("Nova_LensHead.ZombieGlow"), Part::Head);
        assert_eq!(Part::of("Sapper_PauldronL.ZombieGear"), Part::Rest);
    }

    #[test]
    fn gear_and_glow_have_their_own_materials() {
        assert_eq!(Part::material("Napalm_Tank.ZombieGear"), 3);
        assert_eq!(Part::material("Napalm_Tank.001.ZombieGear"), 3);
        assert_eq!(Part::material("Napalm_Flame.ZombieGlow"), 4);
        assert_eq!(Part::material("Nova_Canister.ZombieGlow"), 4);
        assert_eq!(Part::material("Napalm_PantsLegL.ZombiePants"), 2);
    }

    #[test]
    fn each_special_wears_its_own_body_and_a_plain_zombie_one_of_three() {
        let at = Vec3::ZERO;
        let body = |zombie: Zombie| Look::of_zombie(&zombie, 7).map(|look| look.body);
        assert_eq!(body(Zombie::special(SpecialKind::Napalm, 100.0, at)), Some(NAPALM_BODY));
        assert_eq!(body(Zombie::special(SpecialKind::Shrieker, 100.0, at)), Some(SHRIEKER_BODY));
        assert_eq!(body(Zombie::special(SpecialKind::Sapper, 100.0, at)), Some(SAPPER_BODY));
        assert_eq!(body(Zombie::nova_crawler(100.0, at)), Some(NOVA_BODY));
        assert_eq!(body(Zombie::hellhound(100.0, at)), None, "a hound has its own rig");
        assert!(body(Zombie::new_for_test(100.0)).is_some_and(|body| body < PLAIN_BODIES));
        // Every special's files are listed, so a missing one leaves the box rig.
        for special in NAPALM_BODY..=NOVA_BODY {
            assert!(!files_present(|path| path != BODIES[special]));
        }
    }

    #[test]
    fn a_nova_crawler_lies_prone_like_a_crawler_and_a_special_never_loses_its_legs() {
        assert!(is_prone(&Zombie::nova_crawler(100.0, Vec3::ZERO)));
        assert!(!is_prone(&Zombie::new_for_test(100.0)));
        assert!(dies_by_clip(true, Some(&Zombie::new_for_test(100.0))));
        assert!(!dies_by_clip(true, Some(&Zombie::nova_crawler(100.0, Vec3::ZERO))));
    }

    #[test]
    fn parts_and_materials_come_from_node_names() {
        assert_eq!(Part::of("Soldier_Head"), Part::Head);
        assert_eq!(Part::of("Labourer_LegR"), Part::Legs);
        assert_eq!(Part::of("Civilian_PantsLegL"), Part::Legs);
        assert_eq!(Part::of("Soldier_Shirt"), Part::Rest);
        assert_eq!(Part::material("Labourer_Torso"), 0);
        assert_eq!(Part::material("Soldier_Shirt"), 1);
        assert_eq!(Part::material("Civilian_PantsLegL"), 2);
        assert_eq!(Part::material("Civilian_Pants"), 2);
        // As the glTF loader names them: the mesh, then its material.
        assert_eq!(Part::of("Soldier_Head.ZombieSkin"), Part::Head);
        assert_eq!(Part::of("Civilian_PantsLegR.ZombiePants"), Part::Legs);
        assert_eq!(Part::of("base.ZombieSkin"), Part::Rest);
        assert_eq!(Part::material("sweater_fisherman.ZombieShirt"), 1);
        assert_eq!(Part::material("cargo_pants.ZombiePants"), 2);
        assert_eq!(Part::material("Soldier_LegL.ZombieSkin"), 0);
    }

    #[test]
    fn the_box_rig_stays_unless_every_file_is_on_disk() {
        assert!(files_present(|_| true));
        assert!(!files_present(|_| false));
        assert!(!files_present(|path| path != MAPS[1]));
        assert!(!files_present(|path| path != BODIES[2]));
        assert!(!files_present(|path| path != CLIPS));
    }

    #[test]
    fn every_look_comes_up_and_the_same_id_wears_the_same_one() {
        let mut seen = [[0u32; WAY_COUNT]; PLAIN_BODIES];
        for id in 0..600u32 {
            let look = Look::of(id);
            assert_eq!(look, Look::of(id), "a look is a function of the id");
            seen[look.body][look.way] += 1;
        }
        for body in seen {
            for count in body {
                assert!(count > 50, "every body and colourway is worn, evenly enough: {seen:?}");
            }
        }
    }

    #[test]
    fn the_clip_follows_what_the_zombie_does() {
        let standing = want(Activity::Moving, 0.0);
        assert_eq!(standing, Some(Want::Idle));
        assert!(matches!(want(Activity::Moving, 1.5), Some(Want::Walk(_))));
        assert!(matches!(want(Activity::Moving, 3.5), Some(Want::Jog(_))));
        assert!(matches!(want(Activity::Moving, 5.5), Some(Want::Sprint(_))));
        let swing = Activity::Swinging { progress: 0.4, arm: super::super::rig::Swing::Arm(0) };
        assert_eq!(want(swing, 0.0), Some(Want::Scratch(0.4)));
        assert_eq!(want(Activity::Dying { t: 0.2, headless: false }, 0.0), Some(Want::Death));
        assert_eq!(want(Activity::Stunned, 0.0), Some(Want::Flinch));
    }

    #[test]
    fn what_has_no_clip_is_posed_procedurally() {
        for activity in [
            Activity::Tearing { cycle: 0.5 },
            Activity::Vaulting { variant: 1 },
            Activity::Rising,
            Activity::Screaming,
            Activity::KnockedDown,
            Activity::Tipped { t: 0.5 },
            // The Sapper's two-armed slam.
            Activity::Swinging { progress: 0.5, arm: Swing::Both },
        ] {
            assert_eq!(want(activity, 0.0), None, "{activity:?}");
        }
    }

    #[test]
    fn the_pace_matches_the_zombies_speed() {
        let Some(Want::Walk(pace)) = want(Activity::Moving, WALK_SPEED * 1.5) else { panic!() };
        assert!((pace - 1.5).abs() < 1e-4);
        let Some(Want::Jog(pace)) = want(Activity::Moving, JOG_SPEED) else { panic!() };
        assert!((pace - 1.0).abs() < 1e-4);
    }
}
