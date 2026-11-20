//! The Hellhound's rigid-part rig: head, jaw, torso, four legs and a tail, each a rigid piece on
//! its own pivot, posed every frame from what the hound is doing. It trots and gallops with its
//! speed, snaps its jaw and lunges as it bites, sprawls as it dies, and crouches in its lightning
//! strike. Purely visual, like the Zombie's `rig.rs`: the hit areas never move, so Headshots stay
//! as they were.
//!
//! The pieces come from `assets/models/hellhound/hellhound.gltf` (built by
//! `tools/hellhound/build_hellhound.py`) when that file is on disk and has loaded, found by node
//! name. Without it, today's box hound wears the same legs and head pivots, so it moves too.
//! Headless runs never load the model.
//!
//! The rig reads `Shown` (what the Host told) on a client's puppet and works the activity out from
//! the hound's own state on the Host, so Co-op shows the same animation as single player.

use std::f32::consts::{PI, TAU};

use bevy::prelude::*;

use crate::enemies::hellhound::{HellhoundAssets, Telegraph};
use crate::enemies::zombie::rig::{Activity, Shown, describe};
use crate::enemies::zombie::{Dying, KnockedDown, Stunned, Zombie};
use crate::maps::map::textures::on_disk;

/// The model, under `assets/`.
pub const MODEL_PATH: &str = "models/hellhound/hellhound.gltf";

/// Node names the model must carry. Each part's origin is its joint.
pub const TORSO: &str = "Torso";
pub const HEAD: &str = "Head";
pub const JAW: &str = "Jaw";
pub const TAIL: &str = "Tail";
/// Front left, front right, back left, back right.
pub const LEGS: [&str; 4] = ["LegFL", "LegFR", "LegBL", "LegBR"];
/// Eye nodes start with this and are drawn in the emissive eye material.
pub const EYE_PREFIX: &str = "Eye";

/// How quickly parts catch up with the pose they're heading for.
const FOLLOW_RATE: f32 = 20.0;
/// The fastest a hound is taken to move, so a teleport doesn't spin its legs.
const MAX_GAIT_SPEED: f32 = 12.0;

/// A piece on a pivot, with the transform it rests at.
#[derive(Debug, Clone, Copy)]
pub struct Part {
    pub entity: Entity,
    pub rest: Transform,
}

/// The pivots of one hound, posed by `animate`. Jaw, tail and torso exist only on the model.
#[derive(Component, Debug, Clone, Copy)]
#[require(HoundGait)]
pub struct HoundRig {
    /// Front left, front right, back left, back right.
    pub legs: [Part; 4],
    pub head: Part,
    pub jaw: Option<Part>,
    pub tail: Option<Part>,
    pub torso: Option<Part>,
}

/// A box piece that draws the hound until its model has loaded.
#[derive(Component)]
pub struct BoxPiece;

/// How far through its stride a hound is, and how fast it's going.
#[derive(Component, Debug, Default)]
pub struct HoundGait {
    phase: f32,
    speed: f32,
    last: Option<Vec3>,
    clock: f32,
}

/// A hound waiting on its model, then wearing it.
#[derive(Component)]
pub(super) struct HoundModel {
    scene: Handle<WorldAsset>,
    child: Option<Entity>,
}

/// A hound whose model is settled: bound to its parts, or left a box for good.
#[derive(Component)]
pub(super) struct Settled;

/// A whole pose, in radians except where noted.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HoundPose {
    /// Each leg's swing: forward when positive.
    pub legs: [f32; 4],
    /// The head tipped down from the neck.
    pub nod: f32,
    /// How far the head thrusts forward, in metres.
    pub lunge: f32,
    /// The jaw open.
    pub jaw: f32,
    /// The tail raised, and swung to the side.
    pub tail_raise: f32,
    pub tail_wag: f32,
    /// The body lowered, in metres, and pitched nose-down.
    pub bob: f32,
    pub pitch: f32,
}

/// The pose for `activity` at stride `phase` (0 to TAU), `stride` (0 still, 1 at full swing),
/// `run` (0 a trot, 1 a full gallop) and `clock` seconds.
pub fn pose(activity: Activity, phase: f32, stride: f32, run: f32, clock: f32) -> HoundPose {
    let wag = (clock * 6.0).sin();
    let moving = || {
        // A trot swings diagonal pairs together; a gallop staggers all four.
        let trot = [0.0, PI, PI, 0.0];
        let gallop = [0.0, 0.3 * PI, 0.9 * PI, 1.2 * PI];
        let amplitude = (0.55 + 0.4 * run) * stride;
        let legs = std::array::from_fn(|i| amplitude * (phase + trot[i].lerp(gallop[i], run)).sin());
        HoundPose {
            legs,
            nod: 0.05 + 0.1 * run * (phase * 2.0).sin(),
            lunge: 0.0,
            jaw: 0.04 + 0.12 * run,
            tail_raise: 0.2 + 0.4 * run,
            tail_wag: 0.5 * wag * (1.0 - stride),
            bob: 0.03 * stride * (phase * 2.0).sin(),
            pitch: 0.1 * run + 0.04 * stride * (phase * 2.0).cos(),
        }
    };
    match activity {
        Activity::Moving | Activity::Screaming | Activity::Tearing { .. } | Activity::Vaulting { .. } => moving(),
        Activity::Swinging { progress, .. } => {
            let p = progress.clamp(0.0, 1.0);
            // The jaw gapes through the snap and shuts as the bite lands.
            let open = if p < 0.7 { p / 0.7 } else { 1.0 - (p - 0.7) / 0.3 };
            HoundPose {
                legs: [0.5, 0.5, -0.4, -0.4].map(|l| l * p),
                nod: 0.35 * p,
                lunge: 0.32 * p,
                jaw: 0.9 * open,
                tail_raise: 0.6,
                tail_wag: 0.0,
                bob: -0.04 * p,
                pitch: 0.12 * p,
            }
        }
        // Coming out of the lightning: crouched low, head down, shuddering.
        Activity::Rising => HoundPose {
            legs: [0.9, 0.9, -0.9, -0.9],
            nod: 0.45,
            lunge: 0.0,
            jaw: 0.05,
            tail_raise: -0.4,
            tail_wag: 0.0,
            bob: -0.12,
            pitch: 0.05 * (clock * 40.0).sin(),
        },
        Activity::Stunned | Activity::KnockedDown => HoundPose {
            legs: [0.4, 0.4, -0.4, -0.4],
            nod: 0.3,
            jaw: 0.3,
            tail_raise: -0.3,
            bob: -0.08,
            pitch: 0.06 * (clock * 18.0).sin(),
            ..default()
        },
        Activity::Dying { t, .. } | Activity::Tipped { t } => {
            let t = t.clamp(0.0, 1.0);
            // Legs buckle and stick out, the head lolls and the jaw drops slack.
            HoundPose {
                legs: [1.3, 1.1, -1.2, -1.4].map(|l| l * t),
                nod: 0.6 * t,
                lunge: 0.0,
                jaw: 0.5 * t,
                tail_raise: -0.8 * t,
                tail_wag: 0.0,
                bob: -0.1 * t,
                pitch: 0.0,
            }
        }
    }
}

/// Poses every hound's pivots from what it's doing and how fast it's moving.
pub(super) fn animate(
    mut hounds: Query<(
        &HoundRig,
        &mut HoundGait,
        &Transform,
        Option<&Zombie>,
        Option<&Telegraph>,
        Has<Stunned>,
        Has<KnockedDown>,
        Option<&Dying>,
        Option<&Shown>,
    )>,
    mut parts: Query<&mut Transform, Without<HoundRig>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let follow = 1.0 - (-FOLLOW_RATE * dt).exp();
    for (rig, mut gait, transform, zombie, telegraph, stunned, down, dying, shown) in &mut hounds {
        let at = transform.translation;
        let moved = gait.last.map_or(0.0, |last| Vec2::new(at.x - last.x, at.z - last.z).length());
        gait.last = Some(at);
        let speed = (moved / dt).min(MAX_GAIT_SPEED);
        gait.speed = gait.speed.lerp(speed, 1.0 - (-10.0 * dt).exp());
        gait.clock += dt;
        // A full stride covers more ground the faster it goes.
        let cycle = 1.1 + 0.12 * gait.speed;
        gait.phase = (gait.phase + moved.min(MAX_GAIT_SPEED * dt) / cycle * TAU) % TAU;
        let stride = (gait.speed / 2.0).min(1.0);
        let run = ((gait.speed - 3.0) / 4.0).clamp(0.0, 1.0);
        let activity = match shown {
            Some(shown) => shown.0,
            None => describe(0, zombie, None, telegraph.is_some(), stunned, down, dying, false),
        };
        let HoundPose { legs, nod, lunge, jaw, tail_raise, tail_wag, bob, pitch } = pose(activity, gait.phase, stride, run, gait.clock);

        let mut set = |part: Part, delta: Transform| {
            if let Ok(mut transform) = parts.get_mut(part.entity) {
                let want_at = part.rest.translation + part.rest.rotation * delta.translation;
                let want_rotation = part.rest.rotation * delta.rotation;
                transform.translation = transform.translation.lerp(want_at, follow);
                transform.rotation = transform.rotation.slerp(want_rotation, follow);
            }
        };
        for (leg, swing) in rig.legs.into_iter().zip(legs) {
            set(leg, Transform::from_rotation(Quat::from_rotation_x(swing)));
        }
        set(rig.head, Transform::from_translation(Vec3::NEG_Z * lunge).with_rotation(Quat::from_rotation_x(-nod)));
        if let Some(jaw_part) = rig.jaw {
            set(jaw_part, Transform::from_rotation(Quat::from_rotation_x(-jaw)));
        }
        if let Some(tail) = rig.tail {
            set(tail, Transform::from_rotation(Quat::from_rotation_y(tail_wag) * Quat::from_rotation_x(-tail_raise)));
        }
        if let Some(torso) = rig.torso {
            set(torso, Transform::from_translation(Vec3::Y * bob).with_rotation(Quat::from_rotation_x(-pitch)));
        }
    }
}

/// Starts loading the model for each new hound if it's on disk. Without the file, or without an
/// `AssetServer` (a headless run), the hound stays a box.
pub(super) fn request_model(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    scenes: Option<Res<Assets<WorldAsset>>>,
    hounds: Query<Entity, (Added<HoundRig>, Without<HoundModel>)>,
) {
    if hounds.is_empty() {
        return;
    }
    // Scenes only load where the scene asset type is registered: headless and test apps have an
    // `AssetServer` but no scenes, and loading one there panics.
    let Some(asset_server) = asset_server.filter(|_| scenes.is_some() && on_disk(MODEL_PATH)) else {
        for hound in &hounds {
            commands.entity(hound).insert(Settled);
        }
        return;
    };
    let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL_PATH));
    for hound in &hounds {
        commands.entity(hound).insert(HoundModel { scene: scene.clone(), child: None });
    }
}

/// Once the scene has loaded, spawns it under the hound, then binds the rig to its named nodes
/// when they exist and swaps the box pieces for it. A model missing a required node, or one that
/// fails to load, leaves the box.
pub(super) fn put_on_model(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    assets: Option<Res<HellhoundAssets>>,
    mut hounds: Query<(Entity, &mut HoundModel), Without<Settled>>,
    names: Query<&Name>,
    children: Query<&Children>,
    boxes: Query<(), With<BoxPiece>>,
    transforms: Query<&Transform>,
    materials: Query<(), With<MeshMaterial3d<StandardMaterial>>>,
) {
    let (Some(asset_server), Some(assets)) = (asset_server, assets) else { return };
    for (hound, mut model) in &mut hounds {
        let Some(child) = model.child else {
            match asset_server.load_state(&model.scene) {
                bevy::asset::LoadState::Failed(_) => {
                    commands.entity(hound).insert(Settled);
                }
                bevy::asset::LoadState::Loaded if asset_server.is_loaded_with_dependencies(&model.scene) => {
                    let child = commands.spawn((WorldAssetRoot(model.scene.clone()), Transform::default(), ChildOf(hound))).id();
                    model.child = Some(child);
                }
                _ => {}
            }
            continue;
        };
        // The scene's nodes appear a frame after the root does.
        let nodes: Vec<(Entity, String)> = children
            .iter_descendants(child)
            .filter_map(|node| names.get(node).ok().map(|name| (node, name.as_str().to_owned())))
            .collect();
        if nodes.is_empty() {
            continue;
        }
        let find = |wanted: &str| nodes.iter().find(|(_, name)| name == wanted).map(|(node, _)| *node);
        let part = |wanted: &str| {
            let entity = find(wanted)?;
            Some(Part { entity, rest: *transforms.get(entity).ok()? })
        };
        let legs: Option<Vec<Part>> = LEGS.iter().map(|leg| part(leg)).collect();
        let (Some(legs), Some(head)) = (legs, part(HEAD)) else {
            warn!("{MODEL_PATH}: missing a required node ({HEAD} and {LEGS:?}), staying a box");
            commands.entity(child).despawn();
            commands.entity(hound).insert(Settled);
            continue;
        };
        let bound = HoundRig {
            legs: [legs[0], legs[1], legs[2], legs[3]],
            head,
            jaw: part(JAW),
            tail: part(TAIL),
            torso: part(TORSO),
        };
        // Eyes glow whatever the file says.
        for (node, name) in &nodes {
            if name.starts_with(EYE_PREFIX) {
                for piece in std::iter::once(*node).chain(children.iter_descendants(*node)) {
                    if materials.contains(piece) {
                        commands.entity(piece).insert(MeshMaterial3d(assets.eyes.clone()));
                    }
                }
            }
        }
        // The box pieces go: the root's body, and every piece under the hound's own pivots.
        commands.entity(hound).remove::<Mesh3d>().insert((bound, Settled));
        for piece in children.iter_descendants(hound).filter(|piece| boxes.contains(*piece)) {
            commands.entity(piece).insert(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::zombie::rig::Swing;

    fn moving(phase: f32, stride: f32, run: f32) -> HoundPose {
        pose(Activity::Moving, phase, stride, run, 0.0)
    }

    #[test]
    fn a_hound_standing_still_does_not_swing_its_legs() {
        for phase in [0.0, 1.0, 2.5, 4.0] {
            assert_eq!(moving(phase, 0.0, 0.0).legs, [0.0; 4]);
        }
    }

    #[test]
    fn a_trot_swings_diagonal_pairs_together_and_opposite_to_the_other_pair() {
        let p = moving(PI / 2.0, 1.0, 0.0);
        assert!((p.legs[0] - p.legs[3]).abs() < 1e-5, "front left and back right together");
        assert!((p.legs[1] - p.legs[2]).abs() < 1e-5);
        assert!(p.legs[0] > 0.3 && p.legs[1] < -0.3, "{p:?}");
    }

    #[test]
    fn a_gallop_swings_wider_than_a_trot() {
        let widest = |run| (0..64).map(|i| moving(i as f32 / 64.0 * TAU, 1.0, run).legs[0].abs()).fold(0.0, f32::max);
        assert!(widest(1.0) > widest(0.0));
    }

    #[test]
    fn the_stride_follows_the_phase() {
        assert_ne!(moving(0.5, 1.0, 0.0).legs, moving(1.5, 1.0, 0.0).legs);
    }

    #[test]
    fn a_bite_opens_the_jaw_then_shuts_it_as_it_lands() {
        let bite = |progress| pose(Activity::Swinging { progress, arm: Swing::Arm(0) }, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(bite(0.0).jaw, 0.0);
        assert!(bite(0.7).jaw > 0.8);
        assert!(bite(1.0).jaw < 1e-5, "shut on landing");
        assert!(bite(1.0).lunge > bite(0.3).lunge, "the head thrusts out as it lands");
    }

    #[test]
    fn a_dying_hound_sprawls_and_its_jaw_goes_slack() {
        let start = pose(Activity::Dying { t: 0.0, headless: false }, 0.0, 0.0, 0.0, 0.0);
        let end = pose(Activity::Dying { t: 1.0, headless: false }, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(start.legs, [0.0; 4]);
        assert!(end.legs.iter().all(|l| l.abs() > 1.0));
        assert!(end.jaw > start.jaw && end.tail_raise < 0.0);
        assert_eq!(pose(Activity::Tipped { t: 1.0 }, 0.0, 0.0, 0.0, 0.0), end);
    }

    #[test]
    fn a_hound_in_its_lightning_strike_is_crouched() {
        let p = pose(Activity::Rising, 0.0, 0.0, 0.0, 1.0);
        assert!(p.bob < -0.05 && p.nod > 0.3);
        assert!(p.legs.iter().all(|l| l.abs() > 0.5));
    }

    #[test]
    fn every_pose_is_finite() {
        let activities = [
            Activity::Moving,
            Activity::Rising,
            Activity::Stunned,
            Activity::KnockedDown,
            Activity::Screaming,
            Activity::Swinging { progress: 0.4, arm: Swing::Both },
            Activity::Dying { t: 0.5, headless: false },
            Activity::Tipped { t: 2.0 },
        ];
        for activity in activities {
            let p = pose(activity, 3.0, 1.0, 1.0, 9.0);
            let all = [p.legs.to_vec(), vec![p.nod, p.lunge, p.jaw, p.tail_raise, p.tail_wag, p.bob, p.pitch]].concat();
            assert!(all.iter().all(|v| v.is_finite()), "{activity:?}");
        }
    }
}
