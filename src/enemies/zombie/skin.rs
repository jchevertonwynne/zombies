//! The Walker's skinned body: a MakeHuman man in sweater and trousers on the UAL skeleton
//! (`assets/enemies/walker/walker.glb`, made by `tools/walker`). It stands in for the box rig of a
//! plain Zombie, and only once the glTF and its textures are on disk and have loaded: with the
//! asset missing, failed, or in headless runs (which never add this plugin), the box rig stays.
//!
//! Purely visual, like the box rig: the hit areas, colliders and Headshot rules are untouched, and
//! the skin reads only what the box rig already works out (`Gait`), which on a client is told by
//! the Host's `Activity`. So a puppet animates the way the Host's Zombie does.
//!
//! Hybrid animation: walking and standing about play UAL2's `Zombie_Walk_Fwd_Loop` and
//! `Zombie_Idle_Loop`. Every other `Activity` (and a Crawler) is posed procedurally from
//! `rig::pose` and `rig::crawl`, written onto the same bones after the clip has played (see
//! `procedural`).
//!
//! The head and each leg are separate parts: a headless kill hides the head and the legs gib away.

use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::animation::{AnimationPlayer, transition::AnimationTransitions};
use bevy::app::AnimationSystems;
use bevy::asset::io::file::FileAssetReader;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use super::rig::{Activity, Gait, Limb, Pose, Rig, crawl, pose};
use super::{Zombie, body_half_height};
use crate::tuning::{CRAWLER_BODY_LENGTH, CRAWLER_RADIUS};

const GLB: &str = "enemies/walker/walker.glb";
const MAPS: [&str; 3] = ["enemies/walker/skin/color.ktx2", "enemies/walker/shirt/color.ktx2", "enemies/walker/pants/color.ktx2"];
const WALK: &str = "Zombie_Walk_Fwd_Loop";
const IDLE: &str = "Zombie_Idle_Loop";

/// How much bigger the model is drawn than modelled (1.75 m), so its head tops the head hit area.
const SKIN_SCALE: f32 = 1.03;
/// How fast the walk clip goes at 1x: the ground speed it is authored for.
const WALK_SPEED: f32 = 0.9;
/// Under this speed (metres a second) a Zombie stands about rather than walks.
const STANDING_SPEED: f32 = 0.25;
/// How long the walk and idle blend into each other.
const BLEND: Duration = Duration::from_millis(250);
/// How far the middle of a prone body is above the floor, and how long a prone model's head
/// reaches from its hips: it lies along the facing.
const PRONE_LIFT: f32 = 0.14;
const MODEL_HEAD_HEIGHT: f32 = 1.65;

pub struct SkinPlugin;

impl Plugin for SkinPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Walker>()
            .add_systems(Startup, load_walker)
            .add_systems(
                Update,
                (prepare_walker, attach, dress_parts, setup_players, drive_clips, show_parts, hide_boxes, sync_frame).chain(),
            )
            .add_systems(
                PostUpdate,
                (build_skeletons.before(AnimationSystems), pose_skins.after(AnimationSystems).before(TransformSystems::Propagate)),
            );
    }
}

/// What the Walker's files have come to.
#[derive(Resource, Default)]
struct Walker(State);

#[derive(Default)]
enum State {
    /// Not on disk, or failed: the box rig, for good.
    #[default]
    Absent,
    Loading {
        gltf: Handle<Gltf>,
        images: [Handle<Image>; 3],
    },
    Ready(Ready),
}

struct Ready {
    scene: Handle<WorldAsset>,
    /// Skin, sweater, trousers.
    materials: [Handle<StandardMaterial>; 3],
    graph: Handle<AnimationGraph>,
    walk: AnimationNodeIndex,
    idle: AnimationNodeIndex,
}

/// Whether `path` (under `assets/`) is on disk.
fn on_disk(path: &str) -> bool {
    FileAssetReader::get_base_path().join("assets").join(path).is_file()
}

/// Whether the Walker's files are all on disk.
fn files_present(present: impl Fn(&str) -> bool) -> bool {
    present(GLB) && MAPS.iter().all(|map| present(map))
}

fn load_walker(mut walker: ResMut<Walker>, asset_server: Res<AssetServer>) {
    if !files_present(on_disk) {
        debug!("the Walker's files are missing, staying boxes");
        return;
    }
    walker.0 = State::Loading { gltf: asset_server.load(GLB), images: MAPS.map(|map| asset_server.load(map)) };
}

/// Builds the Walker's materials and clip graph once everything has loaded.
fn prepare_walker(
    mut walker: ResMut<Walker>,
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    let State::Loading { gltf, images } = &walker.0 else { return };
    let failed = |id: bevy::asset::UntypedAssetId| matches!(asset_server.load_state(id), bevy::asset::LoadState::Failed(_));
    if failed(gltf.id().untyped()) || images.iter().any(|image| failed(image.id().untyped())) {
        warn!("the Walker failed to load, staying boxes");
        walker.0 = State::Absent;
        return;
    }
    let loaded = asset_server.is_loaded_with_dependencies(gltf) && images.iter().all(|image| asset_server.is_loaded_with_dependencies(image));
    if !loaded {
        return;
    }
    let Some(file) = gltfs.get(gltf) else { return };
    let (Some(scene), Some(walk), Some(idle)) = (file.scenes.first(), file.named_animations.get(WALK), file.named_animations.get(IDLE)) else {
        warn!("the Walker's glTF has no scene or clips, staying boxes");
        walker.0 = State::Absent;
        return;
    };
    let mut graph = AnimationGraph::new();
    let walk = graph.add_clip(walk.clone(), 1.0, graph.root);
    let idle = graph.add_clip(idle.clone(), 1.0, graph.root);
    let cloth = |image: &Handle<Image>| StandardMaterial {
        base_color_texture: Some(image.clone()),
        perceptual_roughness: 0.92,
        reflectance: 0.2,
        ..default()
    };
    let materials = [0, 1, 2].map(|i| materials.add(cloth(&images[i])));
    walker.0 = State::Ready(Ready { scene: scene.clone(), materials, graph: graphs.add(graph), walk, idle });
}

/// A Zombie that wears the Walker's body.
#[derive(Component)]
struct Skinned;

/// The scene root of a Zombie's skinned body.
#[derive(Component)]
struct SkinRoot {
    zombie: Entity,
}

/// Which part of the body a mesh is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Head,
    Legs,
    Rest,
}

impl Part {
    /// From the node's name in the glTF: `Walker_Head`, `Walker_LegL`, `Walker_PantsLegR`...
    fn of(name: &str) -> Self {
        if name.ends_with("Head") {
            Part::Head
        } else if name.ends_with("LegL") || name.ends_with("LegR") {
            Part::Legs
        } else {
            Part::Rest
        }
    }

    /// Which of skin, sweater and trousers it wears.
    fn material(name: &str) -> usize {
        if name.contains("Shirt") {
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

/// Whether a part shows: the head goes with a headshot kill, and the legs with a gib.
fn part_shows(part: Part, headless: bool, crawler: bool) -> bool {
    match part {
        Part::Head => !headless,
        Part::Legs => !crawler,
        Part::Rest => true,
    }
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

/// Puts the Walker on each plain Zombie once it's ready.
fn attach(mut commands: Commands, walker: Res<Walker>, zombies: Query<(Entity, &Zombie), (With<Rig>, Without<Skinned>)>) {
    let State::Ready(ready) = &walker.0 else { return };
    for (entity, zombie) in &zombies {
        if zombie.is_special() {
            continue;
        }
        commands.entity(entity).insert(Skinned);
        commands.spawn((
            Name::new("Walker"),
            SkinRoot { zombie: entity },
            WorldAssetRoot(ready.scene.clone()),
            skin_transform(zombie.is_crawler()),
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
        let wanted = skin_transform(zombie.is_crawler());
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
    walker: Res<Walker>,
    meshes: Query<(Entity, &Name), Added<Mesh3d>>,
    parents: Query<&ChildOf>,
    roots: Query<&SkinRoot>,
) {
    let State::Ready(ready) = &walker.0 else { return };
    for (entity, name) in &meshes {
        let Some((_, zombie)) = skin_root_of(entity, &parents, &roots) else { continue };
        commands.entity(entity).insert((
            MeshMaterial3d(ready.materials[Part::material(name)].clone()),
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
    walker: Res<Walker>,
    players: Query<Entity, Added<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    roots: Query<&SkinRoot>,
) {
    let State::Ready(ready) = &walker.0 else { return };
    for entity in &players {
        let Some((_, zombie)) = skin_root_of(entity, &parents, &roots) else { continue };
        commands.entity(entity).insert((AnimationGraphHandle(ready.graph.clone()), AnimationTransitions::new(), SkinPlayer { zombie }));
    }
}

/// Plays the walk while the Zombie moves, at the pace it moves, and the idle while it stands.
fn drive_clips(
    mut commands: Commands,
    walker: Res<Walker>,
    mut players: Query<(Entity, &SkinPlayer, &mut AnimationPlayer, &mut AnimationTransitions, Option<&Playing>)>,
    zombies: Query<&Gait>,
) {
    let State::Ready(ready) = &walker.0 else { return };
    for (entity, skin, mut player, mut transitions, playing) in &mut players {
        let Ok(gait) = zombies.get(skin.zombie) else { continue };
        let walking = gait.speed() > STANDING_SPEED;
        let clip = if walking { ready.walk } else { ready.idle };
        if playing.is_none_or(|playing| playing.0 != clip) {
            transitions.play(&mut player, clip, if playing.is_some() { BLEND } else { Duration::ZERO }).repeat();
            commands.entity(entity).insert(Playing(clip));
        }
        if walking && let Some(active) = player.animation_mut(clip) {
            active.set_speed((gait.speed() / WALK_SPEED).clamp(0.4, 3.5));
        }
    }
}

/// Hides the head after a headshot kill and the legs after a gib.
fn show_parts(mut parts: Query<(&SkinPart, &mut Visibility)>, zombies: Query<(&Zombie, &Gait)>) {
    for (part, mut visibility) in &mut parts {
        let Ok((zombie, gait)) = zombies.get(part.zombie) else { continue };
        let headless = matches!(gait.activity(), Activity::Dying { headless: true, .. });
        let wanted = if part_shows(part.part, headless, zombie.is_crawler()) { Visibility::Inherited } else { Visibility::Hidden };
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

/// Poses each body that isn't playing a clip: every `Activity` but walking and standing about,
/// and a Crawler always.
fn pose_skins(
    skins: Query<(&SkinRoot, &Skeleton, &Transform)>,
    zombies: Query<(&Zombie, &Gait)>,
    mut bones: Query<&mut Transform, Without<SkinRoot>>,
) {
    for (root, skeleton, transform) in &skins {
        let Ok((zombie, gait)) = zombies.get(root.zombie) else { continue };
        let activity = gait.activity();
        let crawler = zombie.is_crawler();
        if activity == Activity::Moving && !crawler {
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
        assert!(!part_shows(Part::Head, true, false));
        assert!(part_shows(Part::Head, false, true));
        assert!(!part_shows(Part::Legs, false, true));
        assert!(part_shows(Part::Legs, true, false));
        assert!(part_shows(Part::Rest, true, true));
    }

    #[test]
    fn parts_and_materials_come_from_node_names() {
        assert_eq!(Part::of("Walker_Head"), Part::Head);
        assert_eq!(Part::of("Walker_LegR"), Part::Legs);
        assert_eq!(Part::of("Walker_PantsLegL"), Part::Legs);
        assert_eq!(Part::of("Walker_Shirt"), Part::Rest);
        assert_eq!(Part::material("Walker_Torso"), 0);
        assert_eq!(Part::material("Walker_Shirt"), 1);
        assert_eq!(Part::material("Walker_PantsLegL"), 2);
    }

    #[test]
    fn the_box_rig_stays_unless_every_file_is_on_disk() {
        assert!(files_present(|_| true));
        assert!(!files_present(|_| false));
        assert!(!files_present(|path| path != MAPS[1]));
        assert!(!files_present(|path| path != GLB));
    }
}
