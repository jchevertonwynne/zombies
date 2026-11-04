//! The Teleporter: two Pads, at the far corners of the south wing's Workshop and Generator Room,
//! that send the Player one way to the Mainframe in the Start Room, a long walk away. After Power, each Pad is Linked once by activating it and then
//! using the Mainframe in time. A trip charges at the Pad, killing Zombies around it, holds the
//! Player in a black space nothing can reach, then lands them at the Mainframe, killing every
//! Zombie around it, with a random aftereffect and an arrival drop. All its kills are Unpaid.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::render::view::ColorGrading;
use rand::RngExt;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::shellshock::Shellshock;
use crate::buy::{Buyable, Purchased, Refused};
use crate::hellhound::SpawnHellhounds;
use crate::hud::overlay::{Overlay, Overlays};
use crate::last_stand::LastStand;
use crate::layers::GameLayer;
use crate::player::{FovWarp, LocalPlayer, Player, PlayerCamera, Teleport};
use crate::power::Power;
use crate::power_up::DropPowerUp;
use crate::round::Round;
use crate::settings::Settings;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, Zombie};
use crate::zone::{PlayerZone, ZoneId};

/// One of the two Pads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PadId {
    Workshop,
    Generator,
}

impl PadId {
    pub const ALL: [PadId; 2] = [PadId::Workshop, PadId::Generator];

    fn index(self) -> usize {
        self as usize
    }

    /// The Pad's centre on the floor.
    pub fn at(self) -> Vec3 {
        match self {
            // The far corners of the south wing, a long walk from the Mainframe and each other.
            PadId::Workshop => Vec3::new(-28.0, 0.0, -44.5),
            PadId::Generator => Vec3::new(28.0, 0.0, -44.5),
        }
    }
}

/// The Mainframe's centre on the floor, against the Start Room's north wall, facing into the room.
pub const MAINFRAME_AT: Vec3 = Vec3::new(-10.5, 0.0, 25.4);
const MAINFRAME_FACING: Vec3 = Vec3::NEG_Z;
const MAINFRAME_SIZE: Vec3 = Vec3::new(1.4, 2.0, 0.8);

/// Where the Mainframe is used from: in front of its screen.
pub fn mainframe_use_point() -> Vec3 {
    MAINFRAME_AT + Vec3::Y * 1.3 + MAINFRAME_FACING * (MAINFRAME_SIZE.z / 2.0 + 0.2)
}
/// Where the Player lands, in front of the Mainframe.
const ARRIVAL_AT: Vec3 = Vec3::new(-10.5, 0.0, 23.0);
/// Teleporter hounds land in a ring round this point, which keeps the whole ring in the Start Room.
const HOUNDS_AROUND: Vec3 = Vec3::new(-8.0, 0.0, 20.0);
/// The holding space's floor centre: a sealed black box far outside the Map.
const HOLDING_AT: Vec3 = Vec3::new(0.0, 0.0, 300.0);
const HOLDING_SIZE: Vec3 = Vec3::new(4.0, 3.0, 4.0);

/// Where a trip is, with the seconds left in that phase.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TripPhase {
    /// The Pad charges; the Player is still on it.
    Charging(f32),
    /// The Player is in the holding space.
    Holding(f32),
}

/// What a trip did this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TripEvent {
    /// The Pad finished charging. The Player leaves for the holding space, if they stayed close.
    Charged(PadId),
    /// The Player arrived at the Mainframe. `since_last` is the time since the previous arrival.
    Arrived { since_last: Option<f32> },
}

/// What the Host tells the other games of the Teleporter: the parts that change rarely, whose
/// clocks each game runs on its own between the Host's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TeleporterView {
    pub linked: [bool; 2],
    pub linking: Option<PadId>,
    /// The Pad a trip is from, and whether it is already in the holding space.
    pub trip: Option<(PadId, bool)>,
    pub sabotaged: bool,
}

/// The Teleporter's state for the Run: which Pads are Linked, the Link attempt under way, the
/// trip under way, and the shared cooldown. Public so the Sapper can Sabotage it.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Teleporter {
    linked: [bool; 2],
    /// The Pad being Linked, and the seconds left to use the Mainframe.
    linking: Option<(PadId, f32)>,
    trip: Option<(PadId, TripPhase)>,
    cooldown: f32,
    /// Seconds since the last arrival, once there's been one.
    since_last_trip: Option<f32>,
    sabotaged: bool,
    /// The Player who paid for the trip under way, whose kills it credits.
    payer: Option<Entity>,
    /// The Players the trip carries: those standing on the Pad when it charged.
    riders: Vec<Entity>,
}

impl Teleporter {
    /// What the Host tells the other games.
    pub fn view(&self) -> TeleporterView {
        TeleporterView {
            linked: self.linked,
            linking: self.linking(),
            trip: self.trip.map(|(pad, phase)| (pad, matches!(phase, TripPhase::Holding(_)))),
            sabotaged: self.sabotaged,
        }
    }

    /// Shows what the Host says, each clock starting over where its phase begins.
    pub fn show(&mut self, view: TeleporterView) {
        let before = self.view();
        self.linked = view.linked;
        self.sabotaged = view.sabotaged;
        if before.linking != view.linking {
            self.linking = view.linking.map(|pad| (pad, TELEPORTER_LINK_SECS));
        }
        if before.trip != view.trip {
            self.trip = view.trip.map(|(pad, holding)| {
                let phase = if holding {
                    TripPhase::Holding(TELEPORTER_HOLDING_SECS)
                } else {
                    TripPhase::Charging(TELEPORTER_CHARGE_SECS)
                };
                (pad, phase)
            });
            if view.trip.is_none() {
                self.cooldown = TELEPORTER_COOLDOWN_SECS;
            }
        }
    }

    pub fn linked(&self, pad: PadId) -> bool {
        self.linked[pad.index()]
    }

    pub fn both_linked(&self) -> bool {
        self.linked.iter().all(|&linked| linked)
    }

    /// The Pad whose Link attempt is under way, if any.
    pub fn linking(&self) -> Option<PadId> {
        self.linking.map(|(pad, _)| pad)
    }

    /// Whether a trip is under way, so the Sapper skips it.
    pub fn in_use(&self) -> bool {
        self.trip.is_some()
    }

    pub fn sabotaged(&self) -> bool {
        self.sabotaged
    }

    /// The Sapper kills the Teleporter: no Pad or the Mainframe shows a Prompt until it's restored.
    pub fn sabotage(&mut self) {
        self.sabotaged = true;
        self.linking = None;
    }

    pub fn restore(&mut self) {
        self.sabotaged = false;
    }

    /// Activate an unlinked Pad, starting its Link attempt. One attempt at a time.
    pub fn activate(&mut self, pad: PadId) {
        if self.linking.is_none() && !self.linked(pad) {
            self.linking = Some((pad, TELEPORTER_LINK_SECS));
        }
    }

    /// Use the Mainframe: Links the Pad being Linked, for the rest of the Run.
    pub fn link(&mut self) {
        if let Some((pad, _)) = self.linking.take() {
            self.linked[pad.index()] = true;
        }
    }

    /// Whether a trip can start from a Linked Pad: none under way and the cooldown over.
    pub fn ready(&self) -> bool {
        self.trip.is_none() && self.cooldown <= 0.0
    }

    /// Start a trip from a Linked Pad.
    pub fn depart(&mut self, pad: PadId) {
        if self.linked(pad) && self.ready() {
            self.trip = Some((pad, TripPhase::Charging(TELEPORTER_CHARGE_SECS)));
        }
    }

    /// The Player left the Pad before the charge ended: the trip is off and the Points are lost.
    pub fn abort(&mut self) {
        self.trip = None;
        self.payer = None;
        self.riders.clear();
        self.cooldown = TELEPORTER_COOLDOWN_SECS;
    }

    /// Advance the Link timer, the cooldown and any trip by `dt`.
    pub fn advance(&mut self, dt: f32) -> Option<TripEvent> {
        if let Some((_, left)) = &mut self.linking {
            *left -= dt;
            if *left <= 0.0 {
                self.linking = None;
            }
        }
        self.cooldown = (self.cooldown - dt).max(0.0);
        if let Some(since) = &mut self.since_last_trip {
            *since += dt;
        }
        let (pad, phase) = self.trip.as_mut()?;
        match phase {
            TripPhase::Charging(left) => {
                *left -= dt;
                if *left > 0.0 {
                    return None;
                }
                let pad = *pad;
                *phase = TripPhase::Holding(TELEPORTER_HOLDING_SECS + *left);
                Some(TripEvent::Charged(pad))
            }
            TripPhase::Holding(left) => {
                *left -= dt;
                if *left > 0.0 {
                    return None;
                }
                self.trip = None;
                self.cooldown = TELEPORTER_COOLDOWN_SECS;
                Some(TripEvent::Arrived {
                    since_last: self.since_last_trip.replace(0.0),
                })
            }
        }
    }
}

/// What a Pad or the Mainframe shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeleporterPrompt {
    NoPrompt,
    /// "You must turn on the Power first", with no price; the Use key buzzes.
    NeedsPower,
    /// A free Pad activation, starting a Link attempt.
    Activate,
    /// The Mainframe Links the Pad being Linked.
    Link,
    /// A trip from a Linked Pad, for `TELEPORTER_PRICE`.
    Teleport,
}

/// A Pad's Prompt: the Power refusal first; then a free activation while unlinked and no attempt
/// is under way, or a trip once Linked and ready. A Sabotaged Teleporter shows nothing.
pub fn pad_prompt(power_on: bool, teleporter: &Teleporter, pad: PadId) -> TeleporterPrompt {
    if !power_on {
        TeleporterPrompt::NeedsPower
    } else if teleporter.sabotaged() {
        TeleporterPrompt::NoPrompt
    } else if !teleporter.linked(pad) {
        if teleporter.linking().is_none() {
            TeleporterPrompt::Activate
        } else {
            TeleporterPrompt::NoPrompt
        }
    } else if teleporter.ready() {
        TeleporterPrompt::Teleport
    } else {
        TeleporterPrompt::NoPrompt
    }
}

/// The Mainframe's Prompt: the Power refusal, or the Link while an attempt is under way. Trips
/// only go Pad → Mainframe, so it shows nothing otherwise.
pub fn mainframe_prompt(power_on: bool, teleporter: &Teleporter) -> TeleporterPrompt {
    if !power_on {
        TeleporterPrompt::NeedsPower
    } else if !teleporter.sabotaged() && teleporter.linking().is_some() {
        TeleporterPrompt::Link
    } else {
        TeleporterPrompt::NoPrompt
    }
}

/// The Zombies a Teleporter kill takes: those within `radius` (along the floor) of `centre`,
/// nearest first, at most `max`.
pub fn victims<T>(
    centre: Vec3,
    candidates: impl IntoIterator<Item = (T, Vec3)>,
    radius: f32,
    max: usize,
) -> Vec<T> {
    let flat = |v: Vec3| Vec2::new(v.x, v.z);
    let mut near: Vec<_> = candidates
        .into_iter()
        .map(|(item, at)| (item, flat(at).distance(flat(centre))))
        .filter(|&(_, distance)| distance <= radius)
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));
    near.into_iter().take(max).map(|(item, _)| item).collect()
}

/// Whether the Player stays for the trip: still within `TELEPORTER_STAY_RADIUS` of the Pad, and
/// not Down.
pub fn stays_for_trip(pad: Vec3, player: Vec3, last_stand: bool) -> bool {
    !last_stand && Vec2::new(player.x - pad.x, player.z - pad.z).length() <= TELEPORTER_STAY_RADIUS
}

/// What the arrival drop gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrivalDrop {
    /// A Power-up from the shuffle bag, outside the cap and needing no threshold.
    PowerUp,
    /// This many Hellhounds round the Mainframe, joining the Round.
    Hellhounds(u32),
    Nothing,
}

/// The chance an arrival in Round `round` drops nothing: `(R − 15) × 5%` from Round 16.
pub fn nothing_chance(round: u32) -> f64 {
    let past = round.saturating_sub(TELEPORTER_NOTHING_AFTER_ROUND);
    (past as f64 * TELEPORTER_NOTHING_CHANCE_PER_ROUND).min(1.0)
}

/// Whether an arrival brings the re-teleport hounds: teleporting again within
/// `TELEPORTER_RETELEPORT_SECS` of the last arrival, above Round 20, with both Pads Linked.
pub fn re_teleport_hounds(round: u32, since_last: Option<f32>, both_linked: bool) -> bool {
    both_linked
        && round > TELEPORTER_RETELEPORT_ABOVE_ROUND
        && since_last.is_some_and(|since| since < TELEPORTER_RETELEPORT_SECS)
}

/// The arrival table, for uniform rolls in [0, 1). The re-teleport hounds come first and give no
/// Power-up. Otherwise, from Round 16 the chance of nothing overrides both; from Round 15, 1 in 6
/// is a Hellhound; and the rest are Power-ups. With Hellhounds switched off (`hounds_on` false),
/// each hound result gives a Power-up instead.
pub fn arrival_drop(
    round: u32,
    re_teleport: bool,
    hounds_on: bool,
    hound_roll: f64,
    nothing_roll: f64,
) -> ArrivalDrop {
    let drop = if re_teleport {
        ArrivalDrop::Hellhounds(TELEPORTER_RETELEPORT_HOUNDS)
    } else if nothing_roll < nothing_chance(round) {
        ArrivalDrop::Nothing
    } else if round >= TELEPORTER_HOUND_FROM_ROUND && hound_roll < TELEPORTER_HOUND_CHANCE {
        ArrivalDrop::Hellhounds(1)
    } else {
        ArrivalDrop::PowerUp
    };
    match drop {
        ArrivalDrop::Hellhounds(_) if !hounds_on => ArrivalDrop::PowerUp,
        drop => drop,
    }
}

/// The random aftereffect of an arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aftereffect {
    /// The Shellshock haze, through the overlay layer.
    ShellshockBlur,
    /// The picture loses its colour.
    BlackAndWhite,
    /// The FOV swings in and out under a violet wash.
    FovWarp,
}

impl Aftereffect {
    pub const ALL: [Aftereffect; 3] = [
        Aftereffect::ShellshockBlur,
        Aftereffect::BlackAndWhite,
        Aftereffect::FovWarp,
    ];
}

/// How strongly an aftereffect with `left` seconds to go shows, 0–1: full, fading out over the
/// last `TELEPORTER_AFTEREFFECT_FADE_SECS`.
pub fn aftereffect_strength(left: f32) -> f32 {
    (left / TELEPORTER_AFTEREFFECT_FADE_SECS).clamp(0.0, 1.0)
}

/// The factor on the FOV, `left` seconds before the aftereffect ends.
pub fn fov_factor(effect: Aftereffect, left: f32) -> f32 {
    if effect != Aftereffect::FovWarp {
        return 1.0;
    }
    let into = TELEPORTER_AFTEREFFECT_SECS - left;
    let swing = (into * TELEPORTER_FOV_WARP_HZ * std::f32::consts::TAU).sin();
    1.0 + TELEPORTER_FOV_WARP * swing * aftereffect_strength(left)
}

/// The picture's saturation, `left` seconds before the aftereffect ends.
pub fn saturation(effect: Aftereffect, left: f32) -> f32 {
    if effect == Aftereffect::BlackAndWhite {
        1.0 - aftereffect_strength(left)
    } else {
        1.0
    }
}

/// What follows an arrival: the aftereffect with its seconds left, and the arrival drop with the
/// seconds until it comes.
#[derive(Resource, Debug, Default)]
struct Aftermath {
    aftereffect: Option<(Aftereffect, f32)>,
    drop: Option<(ArrivalDrop, f32)>,
}

/// A Zombie the Teleporter's kills spare: the Sapper.
#[derive(Component)]
pub struct ResistsTeleporter;

/// A Pad, the Buyable that activates it and then starts trips. `shown` is the Prompt it has now.
#[derive(Component)]
pub(crate) struct Pad {
    pad: PadId,
    shown: TeleporterPrompt,
}

/// The Mainframe, the Buyable that Links a Pad.
#[derive(Component)]
pub(crate) struct Mainframe {
    shown: TeleporterPrompt,
}

/// A Pad's lamp, or the Mainframe's screen (`None`): dark, amber while Linking, blue once Linked.
#[derive(Component)]
struct TeleporterLamp(Option<PadId>);

/// The column of light over a charging Pad.
#[derive(Component)]
struct ChargeColumn(PadId);

#[derive(Resource)]
struct LampLooks {
    dark: Handle<StandardMaterial>,
    linking: Handle<StandardMaterial>,
    linked: Handle<StandardMaterial>,
}

pub struct TeleporterPlugin;

impl Plugin for TeleporterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Teleporter>()
            .init_resource::<Aftermath>()
            .add_systems(
                OnEnter(GameState::Playing),
                (
                    |mut teleporter: ResMut<Teleporter>, mut aftermath: ResMut<Aftermath>| {
                        *teleporter = Teleporter::default();
                        *aftermath = Aftermath::default();
                    },
                    spawn_teleporter,
                ),
            )
            .add_systems(
                Update,
                (
                    (use_teleporter, run_trips, aftermath).chain().in_set(HostSimulation),
                    prompt_teleporter,
                    show_teleporter,
                )
                    .chain()
                    .run_if(world_runs),
            );
    }
}

fn spawn_teleporter(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let lamp = |materials: &mut Assets<StandardMaterial>, colour: Color, glow: f32| {
        materials.add(StandardMaterial {
            base_color: colour,
            emissive: LinearRgba::from(colour) * glow,
            ..default()
        })
    };
    let looks = LampLooks {
        dark: lamp(&mut materials, Color::srgb(0.12, 0.12, 0.14), 0.0),
        linking: lamp(&mut materials, Color::srgb(1.0, 0.6, 0.1), 4.0),
        linked: lamp(&mut materials, Color::srgb(0.3, 0.6, 1.0), 4.0),
    };
    let metal = materials.add(Color::srgb(0.3, 0.31, 0.34));
    let column = materials.add(StandardMaterial {
        base_color: Color::srgba(0.6, 0.8, 1.0, 0.3),
        emissive: LinearRgba::rgb(3.0, 5.0, 10.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    let base = meshes.add(Cylinder::new(TELEPORTER_PAD_RADIUS, 0.06));
    let ring = meshes.add(Cylinder::new(TELEPORTER_PAD_RADIUS * 0.8, 0.02));
    let beam = meshes.add(Cylinder::new(TELEPORTER_PAD_RADIUS * 0.9, 3.0));
    for pad in PadId::ALL {
        let at = pad.at();
        commands.spawn((
            InRun,
            Name::new("Teleporter Pad"),
            Pad {
                pad,
                shown: TeleporterPrompt::NoPrompt,
            },
            Mesh3d(base.clone()),
            MeshMaterial3d(metal.clone()),
            Transform::from_translation(at + Vec3::Y * 0.03),
            Visibility::default(),
            children![
                (
                    TeleporterLamp(Some(pad)),
                    Mesh3d(ring.clone()),
                    MeshMaterial3d(looks.dark.clone()),
                    Transform::from_xyz(0.0, 0.035, 0.0),
                ),
                (
                    ChargeColumn(pad),
                    Mesh3d(beam.clone()),
                    MeshMaterial3d(column.clone()),
                    Transform::from_xyz(0.0, 1.5, 0.0),
                    Visibility::Hidden,
                ),
            ],
        ));
    }

    let body = Cuboid::from_size(MAINFRAME_SIZE);
    commands.spawn((
        InRun,
        Name::new("Mainframe"),
        Mainframe {
            shown: TeleporterPrompt::NoPrompt,
        },
        Mesh3d(meshes.add(body)),
        MeshMaterial3d(metal.clone()),
        Transform::from_translation(MAINFRAME_AT + Vec3::Y * MAINFRAME_SIZE.y / 2.0)
            .looking_to(MAINFRAME_FACING, Vec3::Y),
        RigidBody::Static,
        Collider::from(body),
        CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        children![(
            TeleporterLamp(None),
            Mesh3d(meshes.add(Cuboid::new(1.0, 0.6, 0.02))),
            MeshMaterial3d(looks.dark.clone()),
            // Looking along its facing, the body's front face is its −Z.
            Transform::from_xyz(0.0, 0.4, -MAINFRAME_SIZE.z / 2.0 - 0.01),
        )],
    ));

    // The holding space: a sealed, unlit black box.
    let black = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        unlit: true,
        ..default()
    });
    let (w, h, d, t) = (HOLDING_SIZE.x, HOLDING_SIZE.y, HOLDING_SIZE.z, 0.2);
    for (size, offset) in [
        (
            Vec3::new(w + 2.0 * t, t, d + 2.0 * t),
            Vec3::new(0.0, -t / 2.0, 0.0),
        ),
        (
            Vec3::new(w + 2.0 * t, t, d + 2.0 * t),
            Vec3::new(0.0, h + t / 2.0, 0.0),
        ),
        (Vec3::new(t, h, d), Vec3::new(-(w + t) / 2.0, h / 2.0, 0.0)),
        (Vec3::new(t, h, d), Vec3::new((w + t) / 2.0, h / 2.0, 0.0)),
        (
            Vec3::new(w + 2.0 * t, h, t),
            Vec3::new(0.0, h / 2.0, -(d + t) / 2.0),
        ),
        (
            Vec3::new(w + 2.0 * t, h, t),
            Vec3::new(0.0, h / 2.0, (d + t) / 2.0),
        ),
    ] {
        let shape = Cuboid::from_size(size);
        commands.spawn((
            InRun,
            Name::new("Teleporter Holding Space"),
            Mesh3d(meshes.add(shape)),
            MeshMaterial3d(black.clone()),
            Transform::from_translation(HOLDING_AT + offset),
            RigidBody::Static,
            Collider::from(shape),
            CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        ));
    }

    commands.insert_resource(looks);
}

/// A bought Pad activates or starts a trip, paid by the Player who bought it; a used Mainframe
/// Links, for whichever Player uses it.
fn use_teleporter(
    mut purchases: MessageReader<Purchased>,
    pads: Query<&Pad>,
    mainframes: Query<(), With<Mainframe>>,
    mut teleporter: ResMut<Teleporter>,
    mut cues: MessageWriter<PlayCue>,
) {
    for &Purchased { player, buyable } in purchases.read() {
        if let Ok(pad) = pads.get(buyable) {
            if teleporter.linked(pad.pad) {
                let idle = !teleporter.in_use();
                teleporter.depart(pad.pad);
                if idle && teleporter.in_use() {
                    teleporter.payer = Some(player);
                    cues.write(PlayCue {
                        cue: Cue::TeleporterCharge,
                        at: Some(pad.pad.at()),
                    });
                }
            } else {
                teleporter.activate(pad.pad);
            }
        } else if mainframes.contains(buyable) {
            if teleporter.linking().is_some() {
                cues.write(PlayCue {
                    cue: Cue::MainframeLink,
                    at: Some(mainframe_use_point()),
                });
            }
            teleporter.link();
        }
    }
}

/// Where the `index`th Player a trip carries stands, from where the first does.
fn rider_offset(index: usize) -> Vec3 {
    Vec3::X * (index as f32 * TELEPORTER_RIDER_SPACING)
}

/// Runs the trip: the charge's kills and the leap into the holding space, then the arrival's kills,
/// aftereffect and drop.
fn run_trips(
    mut teleporter: ResMut<Teleporter>,
    mut aftermath: ResMut<Aftermath>,
    mut players: Query<(Entity, &Transform, Has<LastStand>, &mut PlayerZone, Has<LocalPlayer>), With<Player>>,
    mut teleport: MessageWriter<Teleport>,
    zombies: Query<
        (Entity, &Transform),
        (
            With<Zombie>,
            Without<Dying>,
            Without<ResistsTeleporter>,
            Without<Player>,
        ),
    >,
    mut damage: MessageWriter<DamageZombie>,
    mut overlays: ResMut<Overlays>,
    mut shellshocks: MessageWriter<Shellshock>,
    mut cues: MessageWriter<PlayCue>,
    round: Option<Res<Round>>,
    settings: Res<Settings>,
    time: Res<Time>,
) {
    let Some(event) = teleporter.advance(time.delta_secs()) else {
        return;
    };
    // The Player who paid is credited with the trip's kills.
    let payer = teleporter.payer;
    let mut kill = |centre: Vec3, radius: f32, max: usize| {
        let near = victims(
            centre,
            zombies.iter().map(|(entity, at)| (entity, at.translation)),
            radius,
            max,
        );
        for zombie in near {
            damage.write(DamageZombie {
                player: payer,
                zombie,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::Teleporter,
            });
        }
    };
    match event {
        TripEvent::Charged(pad) => {
            kill(
                pad.at(),
                TELEPORTER_CHARGE_KILL_RADIUS,
                TELEPORTER_CHARGE_KILL_MAX,
            );
            // Everyone standing on the Pad goes; the trip is off if the Player who paid isn't.
            let riders: Vec<Entity> = players
                .iter()
                .filter(|&(_, transform, last_stand, ..)| stays_for_trip(pad.at(), transform.translation, last_stand))
                .map(|(entity, ..)| entity)
                .collect();
            if payer.is_none_or(|payer| riders.contains(&payer)) && !riders.is_empty() {
                for (index, &rider) in riders.iter().enumerate() {
                    teleport.write(Teleport {
                        player: rider,
                        feet: HOLDING_AT + rider_offset(index),
                        facing: None,
                    });
                }
                teleporter.riders = riders;
                cues.write(PlayCue {
                    cue: Cue::TeleporterFire,
                    at: Some(pad.at()),
                });
            } else {
                teleporter.abort();
            }
        }
        TripEvent::Arrived { since_last } => {
            // Facing into the room, away from the Mainframe.
            let riders = std::mem::take(&mut teleporter.riders);
            teleporter.payer = None;
            let mut local_rides = false;
            for (index, &rider) in riders.iter().enumerate() {
                let Ok((.., mut zone, local)) = players.get_mut(rider) else { continue };
                teleport.write(Teleport {
                    player: rider,
                    feet: ARRIVAL_AT + rider_offset(index),
                    facing: Some(Quat::from_rotation_arc(Vec3::NEG_Z, MAINFRAME_FACING)),
                });
                zone.0 = ZoneId::StartRoom;
                local_rides |= local;
            }
            kill(MAINFRAME_AT, TELEPORTER_ARRIVAL_KILL_RADIUS, usize::MAX);
            cues.write(PlayCue {
                cue: Cue::TeleporterArrive,
                at: Some(ARRIVAL_AT),
            });

            let rng = &mut rand::rng();
            let effect = Aftereffect::ALL[rng.random_range(0..Aftereffect::ALL.len())];
            // The aftereffect is on the `LocalPlayer`'s screen, so only if they rode.
            if local_rides {
                match effect {
                    // The shared Shellshock: its haze, slow-down, muffle and cue.
                    Aftereffect::ShellshockBlur => {
                        shellshocks.write(Shellshock { secs: TELEPORTER_AFTEREFFECT_SECS });
                    }
                    Aftereffect::FovWarp => {
                        overlays.pulse(Overlay::TeleporterDaze, 1.0, TELEPORTER_AFTEREFFECT_SECS)
                    }
                    Aftereffect::BlackAndWhite => {}
                }
                aftermath.aftereffect = Some((effect, TELEPORTER_AFTEREFFECT_SECS));
            }

            let round = round.map_or(1, |round| round.number);
            let re_teleport = re_teleport_hounds(round, since_last, teleporter.both_linked());
            let hounds_on = settings.special_zombies.hellhounds;
            let drop = arrival_drop(round, re_teleport, hounds_on, rng.random(), rng.random());
            aftermath.drop = Some((drop, TELEPORTER_DROP_DELAY_SECS));
        }
    }
}

/// The aftereffect plays out on the camera, and the arrival drop comes when it's due.
fn aftermath(
    mut aftermath: ResMut<Aftermath>,
    mut warp: ResMut<FovWarp>,
    mut grading: Query<&mut ColorGrading, With<PlayerCamera>>,
    mut power_ups: MessageWriter<DropPowerUp>,
    mut hounds: MessageWriter<SpawnHellhounds>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let (fov, saturated) = match aftermath.aftereffect {
        Some((effect, left)) => {
            let left = left - dt;
            aftermath.aftereffect = (left > 0.0).then_some((effect, left));
            (fov_factor(effect, left), saturation(effect, left))
        }
        None => (1.0, 1.0),
    };
    if warp.0 != fov {
        warp.0 = fov;
    }
    for mut grading in &mut grading {
        if grading.global.post_saturation != saturated {
            grading.global.post_saturation = saturated;
        }
    }

    if let Some((drop, left)) = &mut aftermath.drop {
        *left -= dt;
        if *left > 0.0 {
            return;
        }
        match *drop {
            ArrivalDrop::PowerUp => {
                power_ups.write(DropPowerUp {
                    at: ARRIVAL_AT,
                    power_up: None,
                });
            }
            ArrivalDrop::Hellhounds(count) => {
                hounds.write(SpawnHellhounds {
                    around: HOUNDS_AROUND,
                    count,
                });
            }
            ArrivalDrop::Nothing => {}
        }
        aftermath.drop = None;
    }
}

/// The Pads and the Mainframe own their Buyables.
fn prompt_teleporter(
    mut commands: Commands,
    mut pads: Query<(Entity, &mut Pad)>,
    mut mainframes: Query<(Entity, &mut Mainframe)>,
    teleporter: Res<Teleporter>,
    power: Res<Power>,
) {
    let mut set = |entity: Entity,
                   shown: &mut TeleporterPrompt,
                   want: TeleporterPrompt,
                   use_point: Vec3,
                   range: f32| {
        if *shown == want {
            return;
        }
        *shown = want;
        let buyable = |prompt: &str, price| Buyable {
            use_point,
            prompt: prompt.into(),
            price,
            range,
        };
        let mut entity = commands.entity(entity);
        match want {
            TeleporterPrompt::NoPrompt => {
                entity.remove::<(Buyable, Refused)>();
            }
            TeleporterPrompt::NeedsPower => {
                entity.insert((buyable("You must turn on the Power first", None), Refused));
            }
            TeleporterPrompt::Activate => {
                entity
                    .remove::<Refused>()
                    .insert(buyable("Press F to activate the Teleporter Pad", None));
            }
            TeleporterPrompt::Link => {
                entity
                    .remove::<Refused>()
                    .insert(buyable("Press F to link the Teleporter", None));
            }
            TeleporterPrompt::Teleport => {
                entity.remove::<Refused>().insert(buyable(
                    "Press F to use the Teleporter",
                    Some(TELEPORTER_PRICE),
                ));
            }
        }
    };
    for (entity, mut pad) in &mut pads {
        let want = pad_prompt(power.is_on(), &teleporter, pad.pad);
        let use_point = pad.pad.at() + Vec3::Y * 0.3;
        set(
            entity,
            &mut pad.shown,
            want,
            use_point,
            TELEPORTER_PAD_USE_RANGE,
        );
    }
    for (entity, mut mainframe) in &mut mainframes {
        let want = mainframe_prompt(power.is_on(), &teleporter);
        set(entity, &mut mainframe.shown, want, mainframe_use_point(), USE_RANGE);
    }
}

/// Lamps show each Pad's Link, the Mainframe's screen any Link, and a column rises over a
/// charging Pad.
fn show_teleporter(
    teleporter: Res<Teleporter>,
    looks: Res<LampLooks>,
    mut lamps: Query<(&TeleporterLamp, &mut MeshMaterial3d<StandardMaterial>)>,
    mut columns: Query<(&ChargeColumn, &mut Visibility)>,
    time: Res<Time>,
) {
    // An attempt under way blinks.
    let blink = (time.elapsed_secs() * 4.0).fract() < 0.5;
    for (lamp, mut material) in &mut lamps {
        let linking = match lamp.0 {
            Some(pad) => teleporter.linking() == Some(pad),
            None => teleporter.linking().is_some(),
        };
        let linked = match lamp.0 {
            Some(pad) => teleporter.linked(pad),
            None => PadId::ALL.into_iter().any(|pad| teleporter.linked(pad)),
        };
        let look = if teleporter.sabotaged() {
            &looks.dark
        } else if linking && blink {
            &looks.linking
        } else if linked {
            &looks.linked
        } else {
            &looks.dark
        };
        if material.0 != *look {
            material.0 = look.clone();
        }
    }
    for (column, mut visibility) in &mut columns {
        let charging =
            matches!(teleporter.trip, Some((pad, TripPhase::Charging(_))) if pad == column.0);
        visibility.set_if_neq(if charging {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::BOX_LOCATIONS;
    use crate::nav::zone_at;

    fn powered_and_linked(pads: &[PadId]) -> Teleporter {
        let mut teleporter = Teleporter::default();
        for &pad in pads {
            teleporter.activate(pad);
            teleporter.link();
        }
        teleporter
    }

    #[test]
    fn before_power_every_part_refuses() {
        let teleporter = Teleporter::default();
        for pad in PadId::ALL {
            assert_eq!(
                pad_prompt(false, &teleporter, pad),
                TeleporterPrompt::NeedsPower
            );
        }
        assert_eq!(
            mainframe_prompt(false, &teleporter),
            TeleporterPrompt::NeedsPower
        );
    }

    #[test]
    fn a_pad_links_by_activating_it_then_using_the_mainframe() {
        let mut teleporter = Teleporter::default();
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Workshop),
            TeleporterPrompt::Activate
        );
        assert_eq!(
            mainframe_prompt(true, &teleporter),
            TeleporterPrompt::NoPrompt
        );
        teleporter.activate(PadId::Workshop);
        assert_eq!(mainframe_prompt(true, &teleporter), TeleporterPrompt::Link);
        teleporter.advance(TELEPORTER_LINK_SECS - 0.1);
        teleporter.link();
        assert!(teleporter.linked(PadId::Workshop));
        assert!(!teleporter.linked(PadId::Generator));
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Workshop),
            TeleporterPrompt::Teleport
        );
        assert_eq!(
            mainframe_prompt(true, &teleporter),
            TeleporterPrompt::NoPrompt
        );
        assert_eq!(TELEPORTER_LINK_SECS, 30.0);
    }

    #[test]
    fn a_link_attempt_runs_out_after_30_seconds() {
        let mut teleporter = Teleporter::default();
        teleporter.activate(PadId::Generator);
        teleporter.advance(TELEPORTER_LINK_SECS);
        teleporter.link();
        assert!(!teleporter.linked(PadId::Generator));
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Generator),
            TeleporterPrompt::Activate
        );
    }

    #[test]
    fn only_one_link_attempt_at_a_time() {
        let mut teleporter = Teleporter::default();
        teleporter.activate(PadId::Workshop);
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Generator),
            TeleporterPrompt::NoPrompt
        );
        teleporter.activate(PadId::Generator);
        teleporter.link();
        assert!(teleporter.linked(PadId::Workshop) && !teleporter.linked(PadId::Generator));
    }

    #[test]
    fn a_link_lasts_for_the_run() {
        let mut teleporter = powered_and_linked(&[PadId::Workshop]);
        for _ in 0..100 {
            teleporter.advance(60.0);
        }
        assert!(teleporter.linked(PadId::Workshop));
    }

    #[test]
    fn a_trip_charges_holds_then_arrives_and_cools_down_both_pads() {
        let mut teleporter = powered_and_linked(&PadId::ALL);
        teleporter.depart(PadId::Workshop);
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Generator),
            TeleporterPrompt::NoPrompt
        );
        assert_eq!(teleporter.advance(TELEPORTER_CHARGE_SECS - 0.1), None);
        assert_eq!(
            teleporter.advance(0.15),
            Some(TripEvent::Charged(PadId::Workshop))
        );
        assert_eq!(teleporter.advance(TELEPORTER_HOLDING_SECS - 0.1), None);
        assert_eq!(
            teleporter.advance(0.15),
            Some(TripEvent::Arrived { since_last: None })
        );
        for pad in PadId::ALL {
            assert_eq!(
                pad_prompt(true, &teleporter, pad),
                TeleporterPrompt::NoPrompt
            );
        }
        teleporter.advance(TELEPORTER_COOLDOWN_SECS);
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Generator),
            TeleporterPrompt::Teleport
        );
        assert_eq!((TELEPORTER_PRICE, TELEPORTER_COOLDOWN_SECS), (1500, 5.0));
    }

    #[test]
    fn the_next_arrival_knows_the_time_since_the_last() {
        let mut teleporter = powered_and_linked(&[PadId::Generator]);
        teleporter.depart(PadId::Generator);
        teleporter.advance(TELEPORTER_CHARGE_SECS);
        teleporter.advance(TELEPORTER_HOLDING_SECS);
        teleporter.advance(30.0);
        teleporter.depart(PadId::Generator);
        teleporter.advance(TELEPORTER_CHARGE_SECS);
        let arrived = teleporter.advance(TELEPORTER_HOLDING_SECS);
        let Some(TripEvent::Arrived {
            since_last: Some(since),
        }) = arrived
        else {
            panic!("{arrived:?}")
        };
        assert!((since - (30.0 + TELEPORTER_CHARGE_SECS + TELEPORTER_HOLDING_SECS)).abs() < 1e-3);
    }

    #[test]
    fn leaving_the_pad_aborts_the_trip_and_loses_the_points() {
        let mut teleporter = powered_and_linked(&[PadId::Workshop]);
        teleporter.depart(PadId::Workshop);
        teleporter.advance(TELEPORTER_CHARGE_SECS);
        teleporter.abort();
        assert_eq!(teleporter.trip, None);
        assert_eq!(teleporter.advance(TELEPORTER_HOLDING_SECS), None);
        assert!(!teleporter.ready());
    }

    #[test]
    fn an_unlinked_pad_cant_start_a_trip() {
        let mut teleporter = Teleporter::default();
        teleporter.depart(PadId::Workshop);
        assert_eq!(teleporter.trip, None);
    }

    #[test]
    fn a_sabotaged_teleporter_shows_nothing_until_restored() {
        let mut teleporter = powered_and_linked(&[PadId::Workshop]);
        teleporter.sabotage();
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Workshop),
            TeleporterPrompt::NoPrompt
        );
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Generator),
            TeleporterPrompt::NoPrompt
        );
        teleporter.restore();
        assert_eq!(
            pad_prompt(true, &teleporter, PadId::Workshop),
            TeleporterPrompt::Teleport
        );
    }

    #[test]
    fn the_player_must_stay_within_four_and_a_half_metres() {
        let pad = PadId::Workshop.at();
        assert!(stays_for_trip(pad, pad + Vec3::new(4.4, 0.9, 0.0), false));
        assert!(!stays_for_trip(pad, pad + Vec3::new(4.0, 0.9, 2.5), false));
        assert!(!stays_for_trip(pad, pad, true), "not while Down");
    }

    #[test]
    fn the_charge_kills_the_nearest_twenty_within_range() {
        let zombies: Vec<_> = (0..30)
            .map(|i| (i, Vec3::new(i as f32 * 0.25, 1.0, 0.0)))
            .collect();
        let killed = victims(
            Vec3::ZERO,
            zombies.clone(),
            TELEPORTER_CHARGE_KILL_RADIUS,
            TELEPORTER_CHARGE_KILL_MAX,
        );
        assert_eq!(killed, (0..20).collect::<Vec<_>>());
        let far = victims(
            Vec3::ZERO,
            [(0, Vec3::new(7.5, 0.0, 0.0)), (1, Vec3::new(5.4, 0.0, 5.4))],
            7.6,
            usize::MAX,
        );
        assert_eq!(far, vec![0]);
    }

    #[test]
    fn the_arrival_drop_is_always_a_power_up_before_round_15() {
        for round in 1..15 {
            for roll in [0.0, 0.1, 0.5, 0.99] {
                assert_eq!(arrival_drop(round, false, true, roll, roll), ArrivalDrop::PowerUp);
            }
        }
    }

    #[test]
    fn from_round_15_one_arrival_in_six_spawns_a_hellhound() {
        assert_eq!(
            arrival_drop(15, false, true, 0.16, 0.0),
            ArrivalDrop::Hellhounds(1)
        );
        assert_eq!(arrival_drop(15, false, true, 0.17, 0.0), ArrivalDrop::PowerUp);
        assert_eq!(nothing_chance(15), 0.0);
    }

    #[test]
    fn from_round_16_a_growing_chance_of_nothing_overrides_both() {
        assert!((nothing_chance(16) - 0.05).abs() < 1e-9);
        assert!((nothing_chance(25) - 0.5).abs() < 1e-9);
        assert_eq!(nothing_chance(40), 1.0);
        assert_eq!(arrival_drop(16, false, true, 0.0, 0.04), ArrivalDrop::Nothing);
        assert_eq!(
            arrival_drop(16, false, true, 0.0, 0.06),
            ArrivalDrop::Hellhounds(1)
        );
        assert_eq!(arrival_drop(25, false, true, 0.9, 0.49), ArrivalDrop::Nothing);
        assert_eq!(arrival_drop(25, false, true, 0.9, 0.51), ArrivalDrop::PowerUp);
        assert_eq!(arrival_drop(35, false, true, 0.9, 0.999), ArrivalDrop::Nothing);
    }

    #[test]
    fn re_teleporting_fast_above_round_20_with_both_pads_linked_brings_four_hounds() {
        assert!(re_teleport_hounds(21, Some(59.9), true));
        assert!(
            !re_teleport_hounds(20, Some(10.0), true),
            "above Round 20 only"
        );
        assert!(
            !re_teleport_hounds(21, Some(60.0), true),
            "within 60 s only"
        );
        assert!(
            !re_teleport_hounds(21, Some(10.0), false),
            "both Pads Linked only"
        );
        assert!(!re_teleport_hounds(30, None, true), "not on the first trip");
        // No Power-up, whatever the rolls.
        for roll in [0.0, 0.5, 0.99] {
            assert_eq!(
                arrival_drop(21, true, true, roll, roll),
                ArrivalDrop::Hellhounds(4)
            );
        }
    }

    #[test]
    fn with_hellhounds_off_every_hound_result_is_a_power_up() {
        assert_eq!(arrival_drop(15, false, false, 0.0, 0.0), ArrivalDrop::PowerUp);
        assert_eq!(arrival_drop(16, false, false, 0.0, 0.06), ArrivalDrop::PowerUp);
        for roll in [0.0, 0.5, 0.99] {
            assert_eq!(arrival_drop(21, true, false, roll, roll), ArrivalDrop::PowerUp, "re-teleport");
        }
        // Nothing and Power-ups are unchanged.
        assert_eq!(arrival_drop(16, false, false, 0.0, 0.04), ArrivalDrop::Nothing);
        assert_eq!(arrival_drop(25, false, false, 0.9, 0.51), ArrivalDrop::PowerUp);
    }

    #[test]
    fn aftereffects_last_four_seconds_and_fade_out() {
        let start = TELEPORTER_AFTEREFFECT_SECS;
        assert_eq!(saturation(Aftereffect::BlackAndWhite, start), 0.0);
        assert!((saturation(Aftereffect::BlackAndWhite, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(saturation(Aftereffect::BlackAndWhite, 0.0), 1.0);
        assert_eq!(saturation(Aftereffect::FovWarp, start), 1.0);
        assert_eq!(fov_factor(Aftereffect::ShellshockBlur, 2.0), 1.0);
        assert_eq!(fov_factor(Aftereffect::FovWarp, 0.0), 1.0);
        let swings: Vec<_> = (0..40)
            .map(|i| fov_factor(Aftereffect::FovWarp, start - i as f32 * 0.075))
            .collect();
        assert!(swings.iter().any(|&f| f > 1.1) && swings.iter().any(|&f| f < 0.9));
        assert!(
            swings
                .iter()
                .all(|&f| (f - 1.0).abs() <= TELEPORTER_FOV_WARP + 1e-6)
        );
    }

    #[test]
    fn the_pads_and_mainframe_stand_in_their_zones() {
        for (pad, zone) in [
            (PadId::Workshop, ZoneId::Workshop),
            (PadId::Generator, ZoneId::GeneratorRoom),
        ] {
            assert_eq!(zone_at(pad.at() + Vec3::Y * 0.5), Some(zone));
            // Clear of the Box Locations.
            for (_, at, _) in BOX_LOCATIONS {
                assert!(Vec2::new(pad.at().x, pad.at().z).distance(at) > 3.0);
            }
        }
        assert_eq!(
            zone_at(MAINFRAME_AT + Vec3::Y * 0.5),
            Some(ZoneId::StartRoom)
        );
        assert_eq!(zone_at(ARRIVAL_AT + Vec3::Y * 0.5), Some(ZoneId::StartRoom));
        assert_eq!(
            zone_at(HOLDING_AT + Vec3::Y * 0.5),
            None,
            "the holding space is outside the Map"
        );
    }

    #[test]
    fn teleporter_hounds_land_inside_the_start_room() {
        let (lo, hi) = ZoneId::StartRoom.volume();
        let reach = HELLHOUND_AROUND_MAX_DIST + 0.5;
        assert!(HOUNDS_AROUND.x - reach > lo.x && HOUNDS_AROUND.x + reach < hi.x);
        assert!(HOUNDS_AROUND.z - reach > lo.z && HOUNDS_AROUND.z + reach < hi.z);
    }
}

/// The Teleporter with two Players: a Link one Player starts any can finish, and a trip is paid by
/// one Player and carries everyone standing on the Pad.
#[cfg(test)]
mod two_player_tests {
    use super::*;
    use crate::buy::UseRequest;
    use crate::player::two_players::{balance, place, two_players_with};
    use crate::points::Points;
    use crate::shellshock::Shellshock;
    use crate::zombie::headless::Sim;

    const PAD: PadId = PadId::Workshop;

    fn setup() -> (Sim, Entity, Entity, Entity, Entity) {
        let (mut sim, local, teammate) = two_players_with(|app| {
            app.add_message::<Shellshock>()
                .add_message::<PlayCue>()
                .init_resource::<Teleporter>()
                .init_resource::<Aftermath>()
                .init_resource::<Overlays>()
                .init_resource::<Settings>()
                .add_systems(Update, (use_teleporter, run_trips).chain());
        });
        for player in [local, teammate] {
            sim.world_mut().get_mut::<Points>(player).unwrap().balance = 10_000;
        }
        let buyable = |price| Buyable { use_point: Vec3::ZERO, prompt: String::new(), price, range: USE_RANGE };
        let pad = sim.world_mut().spawn((Pad { pad: PAD, shown: TeleporterPrompt::NoPrompt }, buyable(Some(TELEPORTER_PRICE)))).id();
        let mainframe = sim.world_mut().spawn((Mainframe { shown: TeleporterPrompt::NoPrompt }, buyable(None))).id();
        (sim, local, teammate, pad, mainframe)
    }

    fn stand_on_pad(sim: &mut Sim, player: Entity) {
        place(sim, player, PAD.at());
    }

    /// Everyone a `Teleport` was sent for over `frames`, with where to.
    fn teleported(sim: &mut Sim, frames: usize) -> Vec<(Entity, Vec3)> {
        let mut sent = Vec::new();
        for _ in 0..frames {
            sim.run(1);
            sent.extend(sim.world_mut().resource_mut::<Messages<Teleport>>().drain().map(|t| (t.player, t.feet)));
        }
        sent
    }

    #[test]
    fn one_player_starts_a_link_and_the_other_finishes_it() {
        let (mut sim, local, teammate, pad, mainframe) = setup();
        sim.send(UseRequest { player: local, buyable: pad, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().resource::<Teleporter>().linking(), Some(PAD));
        sim.run(sim.frames(TELEPORTER_LINK_SECS / 2.0));
        sim.send(UseRequest { player: teammate, buyable: mainframe, hold: false });
        sim.run(2);
        assert!(sim.world_mut().resource::<Teleporter>().linked(PAD));
    }

    #[test]
    fn a_trip_is_paid_by_one_player_and_carries_everyone_on_the_pad() {
        let (mut sim, local, teammate, pad, _) = setup();
        {
            let mut teleporter = sim.world_mut().resource_mut::<Teleporter>();
            teleporter.activate(PAD);
            teleporter.link();
        }
        stand_on_pad(&mut sim, local);
        stand_on_pad(&mut sim, teammate);
        let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
        sim.send(UseRequest { player: teammate, buyable: pad, hold: false });
        let frames = sim.frames(TELEPORTER_CHARGE_SECS + 0.5);
        let sent = teleported(&mut sim, frames);
        assert_eq!(balance(&mut sim, teammate), teammate_before - TELEPORTER_PRICE);
        assert_eq!(balance(&mut sim, local), local_before, "the rider paid for the trip");
        let mut riders: Vec<Entity> = sent.iter().map(|&(player, _)| player).collect();
        riders.sort();
        let mut both = [local, teammate];
        both.sort();
        assert_eq!(riders, both, "the trip left someone standing on the Pad behind");
        assert!(sent.iter().all(|&(_, feet)| feet.z > 200.0), "they went to the holding space");

        let frames = sim.frames(TELEPORTER_HOLDING_SECS + 0.5);
        let arrived = teleported(&mut sim, frames);
        assert_eq!(arrived.len(), 2);
        assert!(arrived.iter().all(|&(_, feet)| feet.distance(ARRIVAL_AT) < 3.0), "they landed at the Mainframe");
        assert_ne!(arrived[0].1, arrived[1].1, "they landed on top of each other");
    }

    #[test]
    fn a_player_off_the_pad_stays_behind() {
        let (mut sim, local, _, pad, _) = setup();
        {
            let mut teleporter = sim.world_mut().resource_mut::<Teleporter>();
            teleporter.activate(PAD);
            teleporter.link();
        }
        stand_on_pad(&mut sim, local);
        sim.send(UseRequest { player: local, buyable: pad, hold: false });
        let frames = sim.frames(TELEPORTER_CHARGE_SECS + 0.5);
        let sent = teleported(&mut sim, frames);
        assert!(sent.iter().all(|&(player, _)| player == local));
        assert_eq!(sent.len(), 1);
    }
}
