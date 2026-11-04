//! The Sapper: an original mini-boss on a counter of its own. After a warning it comes through a
//! window, tearing every Plank off in one blow, and walks to the system nearest the Player to
//! Sabotage it: the Box, a powered Perk machine (never Quick Revive), a Trap, the Teleporter or
//! Pack-a-Punch. A Sabotaged system goes dead, and its Prompt becomes a hold to restore it for
//! `SAPPER_RESTORE_PRICE`, until the Sapper dies and everything comes back.
//!
//! It wears a helmet that turns Headshots into body hits until it has lost a share of its health,
//! slams the Player in front of it, shrugs off the Nuke, Insta-Kill, Monkey Bombs and the
//! Teleporter's kills, takes only a share of its health from a Trap or an Ammo Mod, and is
//! stunned by the Thundergun. It's never cleaned up: stuck, it's moved on toward its target. It
//! carries over between Rounds with its health and helmet, and every death drops a Power-up.

use bevy::prelude::*;
use bevy_transform_interpolation::TranslationEasingState;

use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::barrier::{Barriers, Entering};
use crate::buy::{Buyable, HoldBuy, HoldPurchased};
use crate::door::Door;
use crate::map::{SpawnKind, SpawnPoint};
use crate::mystery_box::{BoxState, MysteryBox, box_use_point};
use crate::nav::{NavGraph, Route, zone_at};
use crate::target::Players;
use crate::pack_a_punch::{self, PackAPunch};
use crate::perk::{Drinking, Perk, PerkMachine};
use crate::player::Player;
use crate::points::BonusPoints;
use crate::power::Power;
use crate::power_up::DropPowerUp;
use crate::round::{CarriesOver, Round, RoundPhase, zombie_health};
use crate::settings::Settings;
use crate::special::{SpecialCue, SpecialKind, special_spawn_at};
use crate::state::{GameState, InRun, world_runs};
use crate::teleporter::{PadId, ResistsTeleporter, Teleporter, mainframe_use_point};
use crate::trap::{ResistsTraps, Trap, TrapStruck, sapper_trap_hit};
use crate::tuning::*;
use crate::zombie::{
    Attack, DamageZombie, Dying, HitArea, HitZone, Pay, Rising, Seeking, SpeedTier, StunZombie,
    Stunned, Zombie, ZombieAssetsParam, apply_damage, spawn_zombie,
};
use crate::zone::{ActiveZones, ZoneId, ZoneSet};

/// The Sapper's cadence: the Round it's next due in. Each time one appears, the next is a drawn
/// gap after that Round.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SapperCadence {
    next: u32,
}

impl SapperCadence {
    /// A Run's cadence, with the first Sapper Round.
    pub fn new(first: u32) -> Self {
        Self { next: first }
    }

    /// Whether a Sapper comes in Round `round`: once it's due, never on a Hellhound Round (it
    /// waits for the next Round that isn't one), never while one is alive, and never while it's
    /// switched off in Settings (`on`). The counter only advances on appearance, so switched off
    /// it stays where it is.
    pub fn due(&self, round: u32, hellhound_round: bool, one_alive: bool, on: bool) -> bool {
        on && !hellhound_round && !one_alive && round >= self.next
    }

    /// A Sapper appeared in Round `round`: the next is due `gap` Rounds later.
    pub fn appeared(&mut self, round: u32, gap: u32) {
        self.next = round.saturating_add(gap);
    }
}

/// The window Spawn Points the Sapper picks from, uniformly: those in Active Zones, the Player's
/// own Zone's first.
pub fn window_candidates<T>(
    points: impl IntoIterator<Item = (ZoneId, T)>,
    player_zone: ZoneId,
    active: ZoneSet,
) -> Vec<T> {
    let open: Vec<(ZoneId, T)> = points
        .into_iter()
        .filter(|(zone, _)| active.contains(*zone))
        .collect();
    if open.iter().any(|(zone, _)| *zone == player_zone) {
        open.into_iter()
            .filter(|(zone, _)| *zone == player_zone)
            .map(|(_, at)| at)
            .collect()
    } else {
        open.into_iter().map(|(_, at)| at).collect()
    }
}

/// The kinds of system the Sapper Sabotages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SabotageKind {
    MysteryBox,
    Perk(Perk),
    Trap,
    Teleporter,
    PackAPunch,
}

/// Whether the Sapper will go for a system: never Quick Revive, only a powered one (the Box needs
/// no Power), and never one that's mid-use, though a running Trap is fair game.
pub fn can_sabotage(kind: SabotageKind, powered: bool, in_use: bool) -> bool {
    match kind {
        SabotageKind::Perk(Perk::QuickRevive) => false,
        SabotageKind::Trap => powered,
        SabotageKind::MysteryBox => !in_use,
        SabotageKind::Perk(_) | SabotageKind::Teleporter | SabotageKind::PackAPunch => {
            powered && !in_use
        }
    }
}

/// One system the Sapper could go for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate<T> {
    pub target: T,
    pub kind: SabotageKind,
    /// Where it stands to work, on the floor.
    pub spot: Vec3,
    /// Where the restore is used from once it's Sabotaged.
    pub use_point: Vec3,
    pub powered: bool,
    pub in_use: bool,
}

/// The system the Sapper goes for: of those it can Sabotage, the one nearest the Player.
pub fn pick_target<T>(
    candidates: impl IntoIterator<Item = Candidate<T>>,
    player: Vec3,
) -> Option<Candidate<T>> {
    candidates
        .into_iter()
        .filter(|c| can_sabotage(c.kind, c.powered, c.in_use))
        .min_by(|a, b| a.spot.distance(player).total_cmp(&b.spot.distance(player)))
}

/// Whether the helmet still holds on a Sapper with `health` of `max_health`: until it has lost
/// `SAPPER_HELMET_FRACTION` of it.
pub fn helmet_holds(max_health: f32, health: f32) -> bool {
    health > max_health * (1.0 - SAPPER_HELMET_FRACTION)
}

/// What a hit on the Sapper's head counts as: a body hit while the helmet holds, a Headshot after.
pub fn head_zone(helmet: bool) -> HitZone {
    if helmet {
        HitZone::Torso
    } else {
        HitZone::Head
    }
}

/// The stuck rule for the Sapper: no progress toward its goal for `SAPPER_STUCK_SECS`. Being at
/// its goal (or busy there) is never stuck.
#[derive(Debug, Clone, Copy)]
pub struct SapperWatch {
    anchor: Vec3,
    secs: f32,
}

impl SapperWatch {
    pub fn new(at: Vec3) -> Self {
        Self {
            anchor: at,
            secs: 0.0,
        }
    }

    /// Advances by `dt` with the Sapper at `at`. True once it's stuck and should be moved.
    pub fn update(&mut self, at: Vec3, at_goal: bool, dt: f32) -> bool {
        if at_goal || at.distance(self.anchor) >= ZOMBIE_PROGRESS_DIST {
            *self = Self::new(at);
            false
        } else {
            self.secs += dt;
            self.secs >= SAPPER_STUCK_SECS
        }
    }
}

/// What the Sapper Sabotages: a system's entity, or the Teleporter (a resource).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Entity(Entity),
    Teleporter,
}

/// A system the Sapper has Sabotaged: it's dead, and shows no Prompt of its own until it's
/// restored. The Teleporter keeps its own flag.
#[derive(Component)]
pub struct Sabotaged;

/// The Sapper, while it's working on a Sabotage, for its sound.
#[derive(Component)]
pub struct Sabotaging;

/// The hold Buyable that restores a Sabotaged system.
#[derive(Component)]
struct Restore {
    target: Target,
}

/// Ask for a Sapper now, as the debug keys do: it comes after its warning, whatever the cadence.
#[derive(Message)]
pub struct SummonSapper;

/// The Sapper.
#[derive(Component)]
pub struct Sapper {
    helmet: bool,
    /// Its head (whose hit area the helmet turns into a body hit) and the helmet on it, once dressed.
    head: Option<Entity>,
    helmet_mesh: Option<Entity>,
    /// The system it's heading for.
    goal: Option<Candidate<Target>>,
    /// Seconds left of the Sabotage under way.
    working: Option<f32>,
    watch: SapperWatch,
    step_in: f32,
    last_at: Vec3,
}

impl Sapper {
    fn new(at: Vec3) -> Self {
        Self {
            helmet: true,
            head: None,
            helmet_mesh: None,
            goal: None,
            working: None,
            watch: SapperWatch::new(at),
            step_in: SAPPER_STEP_SECS,
            last_at: at,
        }
    }
}

/// The Round's Sapper, if it brings one: how much of the Horde spawns first, and the warning.
#[derive(Resource, Default)]
struct SapperPlan {
    round: u32,
    at: Option<u32>,
    warning: Option<f32>,
    summoned: bool,
}

#[derive(Resource)]
struct SapperAssets {
    armour: Handle<StandardMaterial>,
    steel: Handle<StandardMaterial>,
    helmet: Handle<Mesh>,
    plate: Handle<Mesh>,
    sparks: Handle<Mesh>,
    spark: Handle<StandardMaterial>,
}

pub struct SapperPlugin;

impl Plugin for SapperPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SummonSapper>()
            .init_resource::<SapperPlan>()
            .insert_resource(SapperCadence::new(*SAPPER_FIRST_ROUNDS.start()))
            .add_systems(Startup, load_assets)
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(FixedUpdate, unstick.in_set(HostSimulation).run_if(world_runs))
            .add_systems(
                Update,
                (
                    (plan_sapper, spawn_sapper).chain().in_set(HostSimulation),
                    dress_sappers,
                    (trap_hits, helmets.after(apply_damage)).in_set(HostSimulation),
                    (
                        stun_restarts,
                        sabotage,
                        restore_bought,
                        sapper_deaths,
                        prune_restores,
                    )
                        .chain()
                        .in_set(HostSimulation),
                    footsteps,
                )
                    .run_if(world_runs),
            );
    }
}

fn load_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(SapperAssets {
        armour: materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.17, 0.15),
            metallic: 0.6,
            perceptual_roughness: 0.5,
            ..default()
        }),
        steel: materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.46, 0.48),
            metallic: 0.9,
            perceptual_roughness: 0.35,
            ..default()
        }),
        helmet: meshes.add(Cylinder::new(
            ZOMBIE_HEAD_RADIUS * 1.2,
            ZOMBIE_HEAD_RADIUS * 0.8,
        )),
        plate: meshes.add(Cuboid::new(0.8, 0.6, 0.5)),
        sparks: meshes.add(Sphere::new(0.12)),
        spark: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.2, 0.1),
            emissive: LinearRgba::rgb(10.0, 1.5, 0.5),
            unlit: true,
            ..default()
        }),
    });
}

fn reset(mut commands: Commands) {
    commands.insert_resource(SapperCadence::new(rand::random_range(SAPPER_FIRST_ROUNDS)));
    commands.insert_resource(SapperPlan::default());
}

/// Each new Round decides whether the Sapper comes, and how far into the Horde.
fn plan_sapper(
    round: Option<Res<Round>>,
    cadence: Res<SapperCadence>,
    mut plan: ResMut<SapperPlan>,
    mut summons: MessageReader<SummonSapper>,
    sappers: Query<(), (With<Sapper>, Without<Dying>)>,
    settings: Res<Settings>,
) {
    let Some(round) = round else {
        return;
    };
    if summons.read().count() > 0 && sappers.is_empty() && plan.at.is_none() {
        plan.at = Some(0);
        plan.warning = None;
        plan.summoned = true;
    }
    if plan.round == round.number {
        return;
    }
    let due = cadence.due(
        round.number,
        round.hellhound.is_some(),
        !sappers.is_empty(),
        settings.special_zombies.sapper,
    );
    *plan = SapperPlan {
        round: round.number,
        at: due.then(|| {
            special_spawn_at(
                round.horde(),
                rand::random_range(SPECIAL_SPAWN_HORDE_FRACTION),
            )
        }),
        warning: None,
        summoned: false,
    };
}

/// Once enough of the Horde has spawned the warning plays, and then the Sapper smashes in through
/// a window in a Zombie's place. One switched off since the Round was planned doesn't come, and
/// its cadence doesn't advance.
fn spawn_sapper(
    mut commands: Commands,
    mut plan: ResMut<SapperPlan>,
    mut round: ResMut<Round>,
    mut cadence: ResMut<SapperCadence>,
    mut barriers: ResMut<Barriers>,
    alive: Query<(), (With<Zombie>, Without<Dying>)>,
    spawn_points: Query<(&Transform, &SpawnPoint)>,
    players: Players,
    active: Res<ActiveZones>,
    zombie_assets: ZombieAssetsParam,
    assets: Res<SapperAssets>,
    mut cues: MessageWriter<SpecialCue>,
    settings: Res<Settings>,
    time: Res<Time>,
) {
    let Some(at) = plan.at else {
        return;
    };
    if !settings.special_zombies.sapper && !plan.summoned {
        plan.at = None;
        plan.warning = None;
        return;
    }
    if round.phase != RoundPhase::Active || round.horde_spawned() < at {
        return;
    }
    match plan.warning {
        None => {
            plan.warning = Some(SAPPER_WARNING_SECS);
            cues.write(SpecialCue {
                cue: Cue::SapperWarning,
                at: None,
            });
            return;
        }
        Some(left) if left > 0.0 => {
            plan.warning = Some(left - time.delta_secs());
            return;
        }
        Some(_) => {}
    }
    if alive.iter().count() as u32 >= round.alive_cap() {
        return;
    }
    let windows = window_candidates(
        spawn_points
            .iter()
            .filter_map(|(t, point)| match point.kind {
                SpawnKind::Window(barrier) => Some((point.zone, (t.translation, barrier))),
                _ => None,
            }),
        players.first().map_or(ZoneId::StartRoom, |player| player.zone),
        active.0,
    );
    if windows.is_empty() || !(round.take_from_horde() || plan.summoned) {
        return;
    }
    let (point, barrier) = windows[rand::random_range(0..windows.len())];
    let health = SpecialKind::Sapper.health(zombie_health(round.number) as f32);
    let zombie = spawn_zombie(
        &mut commands,
        &zombie_assets,
        point,
        health,
        SpeedTier::Walker,
        None,
    );
    commands.entity(zombie).insert((
        Name::new("Sapper"),
        Zombie::special(SpecialKind::Sapper, health, point),
        Sapper::new(point),
        Entering::new(barrier),
        CarriesOver,
        ResistsTraps,
        ResistsTeleporter,
        MeshMaterial3d(assets.armour.clone()),
    ));
    // Every Plank off in one blow.
    barriers.smash(barrier);
    cues.write(SpecialCue {
        cue: Cue::SapperSmash,
        at: Some(point),
    });
    cadence.appeared(round.number, rand::random_range(SAPPER_ROUND_GAPS));
    *plan = SapperPlan {
        round: round.number,
        ..default()
    };
}

/// A new Sapper gets its helmet and chest plate, and its head's hit area counts as the body.
fn dress_sappers(
    mut commands: Commands,
    mut sappers: Query<(Entity, &mut Sapper, &Children), Added<Sapper>>,
    mut hit_areas: Query<&mut HitArea>,
    assets: Res<SapperAssets>,
) {
    for (entity, mut sapper, children) in &mut sappers {
        for child in children.iter() {
            if let Ok(mut area) = hit_areas.get_mut(child)
                && area.0 == HitZone::Head
            {
                area.0 = head_zone(sapper.helmet);
                sapper.head = Some(child);
                let helmet = commands
                    .spawn((
                        Mesh3d(assets.helmet.clone()),
                        MeshMaterial3d(assets.steel.clone()),
                        Transform::from_xyz(0.0, ZOMBIE_HEAD_RADIUS * 0.6, 0.0),
                        ChildOf(child),
                    ))
                    .id();
                sapper.helmet_mesh = Some(helmet);
            }
        }
        commands.entity(entity).with_child((
            Mesh3d(assets.plate.clone()),
            MeshMaterial3d(assets.steel.clone()),
            Transform::from_xyz(0.0, 0.25, 0.0),
        ));
    }
}

/// A running Trap takes a share of the Sapper's max health once per entry, and stuns it.
fn trap_hits(
    mut struck: MessageReader<TrapStruck>,
    sappers: Query<&Zombie, (With<Sapper>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
    mut stuns: MessageWriter<StunZombie>,
) {
    for &TrapStruck { player, zombie } in struck.read() {
        let Ok(body) = sappers.get(zombie) else {
            continue;
        };
        let (amount, secs) = sapper_trap_hit(body.max_health());
        damage.write(DamageZombie {
            player,
            zombie,
            damage: amount,
            zone: HitZone::Torso,
            attack: Attack::Trap,
        });
        stuns.write(StunZombie { zombie, secs });
    }
}

/// The helmet comes off once the Sapper has lost enough health: from then on a hit to its head
/// is a Headshot.
fn helmets(
    mut commands: Commands,
    mut sappers: Query<(&mut Sapper, &Zombie, &Transform), Without<Dying>>,
    mut hit_areas: Query<&mut HitArea>,
    mut cues: MessageWriter<SpecialCue>,
) {
    for (mut sapper, body, transform) in &mut sappers {
        if !sapper.helmet || helmet_holds(body.max_health(), body.health()) {
            continue;
        }
        sapper.helmet = false;
        if let Some(head) = sapper.head
            && let Ok(mut area) = hit_areas.get_mut(head)
        {
            area.0 = head_zone(false);
        }
        if let Some(helmet) = sapper.helmet_mesh.take() {
            commands.entity(helmet).despawn();
        }
        cues.write(SpecialCue {
            cue: Cue::HelmetOff,
            at: Some(transform.translation),
        });
    }
}

/// A stun starts a Sabotage under way over.
fn stun_restarts(
    mut sappers: Query<(&mut Sapper, &Transform), (Added<Stunned>, Without<Dying>)>,
    mut cues: MessageWriter<SpecialCue>,
) {
    for (mut sapper, transform) in &mut sappers {
        if sapper.working.is_some() {
            sapper.working = Some(SAPPER_SABOTAGE_SECS);
        }
        cues.write(SpecialCue {
            cue: Cue::SapperStun,
            at: Some(transform.translation),
        });
    }
}

/// Every system the Sapper could go for right now, those already Sabotaged left out. Only
/// systems in Active Zones count, so it never heads somewhere it can't reach.
#[allow(clippy::too_many_arguments)]
fn candidates(
    boxes: &Query<(Entity, &MysteryBox, &Transform), Without<Sabotaged>>,
    perks: &Query<(Entity, &PerkMachine), Without<Sabotaged>>,
    traps: &Query<(Entity, &Trap, &Transform), Without<Sabotaged>>,
    machines: &Query<(Entity, &PackAPunch, &Transform), Without<Sabotaged>>,
    doors: &Query<&Door>,
    teleporter: &Teleporter,
    power_on: bool,
    drinking: Option<Perk>,
    active: ZoneSet,
) -> Vec<Candidate<Target>> {
    let candidate = |target, kind, use_point: Vec3, powered, in_use| Candidate {
        target,
        kind,
        spot: use_point.with_y(0.0),
        use_point,
        powered,
        in_use,
    };
    let mut all = Vec::new();
    for (entity, mystery_box, transform) in boxes {
        let in_use = mystery_box.state != BoxState::Idle;
        all.push(candidate(
            Target::Entity(entity),
            SabotageKind::MysteryBox,
            box_use_point(transform),
            true,
            in_use,
        ));
    }
    for (entity, machine) in perks {
        let kind = SabotageKind::Perk(machine.perk);
        all.push(candidate(
            Target::Entity(entity),
            kind,
            machine.use_point(),
            power_on,
            drinking == Some(machine.perk),
        ));
    }
    for (entity, trap, transform) in traps {
        let door_open = doors
            .iter()
            .any(|door| door.open && door.joins == Some(trap.joins()));
        let use_point = transform.translation.with_y(1.3);
        all.push(candidate(
            Target::Entity(entity),
            SabotageKind::Trap,
            use_point,
            power_on && door_open,
            false,
        ));
    }
    for (entity, machine, transform) in machines {
        let use_point = pack_a_punch::use_point(transform.translation);
        let in_use = machine.stage().is_some();
        all.push(candidate(
            Target::Entity(entity),
            SabotageKind::PackAPunch,
            use_point,
            power_on,
            in_use,
        ));
    }
    if !teleporter.sabotaged() {
        let spots = PadId::ALL
            .map(|pad| pad.at() + Vec3::Y * 1.0)
            .into_iter()
            .chain([mainframe_use_point()]);
        for use_point in spots {
            all.push(candidate(
                Target::Teleporter,
                SabotageKind::Teleporter,
                use_point,
                power_on,
                teleporter.in_use(),
            ));
        }
    }
    all.retain(|c| zone_at(c.spot + Vec3::Y * 0.5).is_none_or(|zone| active.contains(zone)));
    all
}

/// The Sapper heads for the system nearest the Player, or turns on the Player close by, or hunts
/// them when there's nothing left to Sabotage. At its target it works for `SAPPER_SABOTAGE_SECS`
/// (damage doesn't stop it), and then the system goes dead and a restore Buyable takes its place.
#[allow(clippy::too_many_arguments)]
fn sabotage(
    mut commands: Commands,
    mut sappers: Query<
        (
            Entity,
            &mut Sapper,
            &Transform,
            Option<&Route>,
            Option<&Seeking>,
            Has<Stunned>,
        ),
        (Without<Dying>, Without<Entering>, Without<Rising>),
    >,
    players: Players,
    drinking: Query<&Drinking, With<Player>>,
    boxes: Query<(Entity, &MysteryBox, &Transform), Without<Sabotaged>>,
    perks: Query<(Entity, &PerkMachine), Without<Sabotaged>>,
    traps: Query<(Entity, &Trap, &Transform), Without<Sabotaged>>,
    machines: Query<(Entity, &PackAPunch, &Transform), Without<Sabotaged>>,
    doors: Query<&Door>,
    mut teleporter: ResMut<Teleporter>,
    power: Res<Power>,
    active: Res<ActiveZones>,
    assets: Res<SapperAssets>,
    time: Res<Time>,
) {
    if sappers.is_empty() {
        return;
    }
    let open = candidates(
        &boxes,
        &perks,
        &traps,
        &machines,
        &doors,
        &teleporter,
        power.is_on(),
        drinking.iter().next().map(|drinking| drinking.perk),
        active.0,
    );
    let dt = time.delta_secs();
    for (entity, mut sapper, transform, route, seeking, stunned) in &mut sappers {
        if let Some(left) = sapper.working {
            if stunned {
                continue;
            }
            let Some(goal) = sapper.goal else {
                sapper.working = None;
                continue;
            };
            // Something it can't Sabotage any more (a machine someone started using) is let go.
            if !open.iter().any(|c| c.target == goal.target) {
                sapper.working = None;
                commands.entity(entity).remove::<Sabotaging>();
                continue;
            }
            if left - dt > 0.0 {
                sapper.working = Some(left - dt);
                continue;
            }
            sapper.working = None;
            sapper.goal = None;
            commands.entity(entity).remove::<Sabotaging>();
            match goal.target {
                Target::Entity(target) => {
                    commands.entity(target).insert(Sabotaged);
                }
                Target::Teleporter => teleporter.sabotage(),
            }
            let restore = "Hold F to restore".to_string();
            commands.spawn((
                InRun,
                Name::new("Sabotage"),
                Restore {
                    target: goal.target,
                },
                Buyable {
                    use_point: goal.use_point,
                    prompt: restore.clone(),
                    price: Some(SAPPER_RESTORE_PRICE),
                    range: USE_RANGE,
                },
                HoldBuy {
                    prompt: restore,
                    price: Some(SAPPER_RESTORE_PRICE),
                    tap: false,
                },
                Mesh3d(assets.sparks.clone()),
                MeshMaterial3d(assets.spark.clone()),
                Transform::from_translation(goal.use_point),
            ));
            continue;
        }

        let at = transform.translation;
        let Some(player) = players.target_of(route.and_then(|route| route.target), at) else {
            continue;
        };
        let player_close = player.up
            && (player.centre - at).with_y(0.0).length() <= SAPPER_SLAM_REACH
            && (player.centre.y - at.y).abs() < 1.5;
        sapper.goal = if player_close {
            None
        } else {
            pick_target(open.iter().copied(), player.feet)
        };
        match sapper.goal {
            Some(goal) => {
                if seeking.is_none_or(|seek| seek.0 != goal.spot) {
                    commands.entity(entity).insert(Seeking(goal.spot));
                }
                let feet = at - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
                if !stunned
                    && (goal.spot - feet).with_y(0.0).length() <= SAPPER_SABOTAGE_REACH
                    && (goal.spot.y - feet.y).abs() < 1.5
                {
                    sapper.working = Some(SAPPER_SABOTAGE_SECS);
                    commands.entity(entity).insert(Sabotaging);
                }
            }
            None => {
                if seeking.is_some() {
                    commands.entity(entity).remove::<Seeking>();
                }
            }
        }
    }
}

/// Undoes a Sabotage.
fn restore(commands: &mut Commands, teleporter: &mut Teleporter, target: Target) {
    match target {
        Target::Entity(entity) => {
            if let Ok(mut system) = commands.get_entity(entity) {
                system.remove::<Sabotaged>();
            }
        }
        Target::Teleporter => teleporter.restore(),
    }
}

/// Holding the Use key on a Sabotaged system (and paying) restores it.
fn restore_bought(
    mut commands: Commands,
    mut holds: MessageReader<HoldPurchased>,
    restores: Query<&Restore>,
    mut teleporter: ResMut<Teleporter>,
) {
    for &HoldPurchased { buyable, .. } in holds.read() {
        if let Ok(bought) = restores.get(buyable) {
            restore(&mut commands, &mut teleporter, bought.target);
            commands.entity(buyable).despawn();
        }
    }
}

/// The Sapper's death restores everything it Sabotaged. The Player's kill pays its bonus, and
/// every death drops a Power-up outside the cap.
fn sapper_deaths(
    mut commands: Commands,
    deaths: Query<(Entity, &Zombie, &Transform, &Dying), Added<Dying>>,
    restores: Query<(Entity, &Restore)>,
    mut teleporter: ResMut<Teleporter>,
    mut bonus: MessageWriter<BonusPoints>,
    mut drops: MessageWriter<DropPowerUp>,
    mut cues: MessageWriter<SpecialCue>,
) {
    for (entity, zombie, transform, dying) in &deaths {
        if !zombie.is_sapper() {
            continue;
        }
        commands.entity(entity).remove::<(Sabotaging, Seeking)>();
        for (restore_entity, pending) in &restores {
            restore(&mut commands, &mut teleporter, pending.target);
            commands.entity(restore_entity).despawn();
        }
        if dying.killed_by().map_or(Pay::Unpaid, Attack::pay) == Pay::Paid
            && let Some(killer) = dying.killer()
        {
            bonus.write(BonusPoints {
                player: killer,
                amount: SAPPER_KILL_BONUS,
            });
        }
        let feet = transform.translation - Vec3::Y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0);
        drops.write(DropPowerUp {
            at: feet,
            power_up: None,
        });
        cues.write(SpecialCue {
            cue: Cue::SapperDeath,
            at: Some(transform.translation),
        });
    }
}

/// A restore whose system is gone, or no longer Sabotaged, goes too.
fn prune_restores(
    mut commands: Commands,
    restores: Query<(Entity, &Restore)>,
    sabotaged: Query<(), With<Sabotaged>>,
    teleporter: Res<Teleporter>,
) {
    for (entity, pending) in &restores {
        let still = match pending.target {
            Target::Entity(target) => sabotaged.contains(target),
            Target::Teleporter => teleporter.sabotaged(),
        };
        if !still {
            commands.entity(entity).despawn();
        }
    }
}

/// A Sapper that makes no progress toward its goal for `SAPPER_STUCK_SECS` is moved to the path
/// node nearest its target instead of being cleaned up. Moved in the fixed step, like the chase,
/// as a clean cut: the easing starts from the new spot too.
fn unstick(
    mut sappers: Query<
        (
            &mut Sapper,
            &mut Transform,
            &mut TranslationEasingState,
            &mut Route,
            Option<&Seeking>,
            Has<Stunned>,
        ),
        (Without<Dying>, Without<Entering>, Without<Rising>, Without<Player>),
    >,
    players: Players,
    graph: Res<NavGraph>,
    time: Res<Time>,
) {
    let half = ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0;
    for (mut sapper, mut transform, mut easing, mut route, seeking, stunned) in &mut sappers {
        let Some(player) = players.target_of(route.target, transform.translation) else {
            continue;
        };
        let goal = seeking.map_or(player.feet, |seek| seek.0);
        let feet = transform.translation - Vec3::Y * half;
        let at_goal = stunned
            || sapper.working.is_some()
            || (goal - feet).with_y(0.0).length() <= SAPPER_SABOTAGE_REACH + 0.5;
        if !sapper
            .watch
            .update(transform.translation, at_goal, time.delta_secs())
        {
            continue;
        }
        let node = graph.nodes[graph.nearest(goal)].at;
        transform.translation = node + Vec3::Y * (half + 0.02);
        easing.start = Some(transform.translation);
        *route = Route::default();
        sapper.watch = SapperWatch::new(transform.translation);
    }
}

/// Heavy footsteps while it walks.
fn footsteps(
    mut sappers: Query<(&mut Sapper, &Transform), (Without<Dying>, Without<Stunned>)>,
    mut cues: MessageWriter<SpecialCue>,
    time: Res<Time>,
) {
    for (mut sapper, transform) in &mut sappers {
        let at = transform.translation;
        let moved = at.distance(sapper.last_at) > 0.01;
        sapper.last_at = at;
        if !moved {
            continue;
        }
        sapper.step_in -= time.delta_secs();
        if sapper.step_in <= 0.0 {
            sapper.step_in = SAPPER_STEP_SECS;
            cues.write(SpecialCue {
                cue: Cue::SapperStep,
                at: Some(at),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sapper_comes_in_round_eight_to_ten_then_every_four_to_six() {
        assert_eq!(SAPPER_FIRST_ROUNDS, 8..=10);
        assert_eq!(SAPPER_ROUND_GAPS, 4..=6);
        let mut cadence = SapperCadence::new(9);
        assert!(!cadence.due(8, false, false, true));
        assert!(cadence.due(9, false, false, true));
        // It advances on appearance, not on death.
        cadence.appeared(9, 5);
        assert!(!cadence.due(10, false, false, true));
        assert!(!cadence.due(13, false, false, true));
        assert!(cadence.due(14, false, false, true));
    }

    #[test]
    fn a_switched_off_sapper_never_comes_and_its_cadence_stays_put() {
        let cadence = SapperCadence::new(9);
        assert!((1..=60).all(|round| !cadence.due(round, false, false, false)));
        assert_eq!(cadence, SapperCadence::new(9), "it only advances on appearance");
        assert!(cadence.due(20, false, false, true), "switched back on, it's due at once");
    }

    #[test]
    fn a_due_sapper_waits_through_a_hellhound_round_and_while_one_is_alive() {
        let cadence = SapperCadence::new(8);
        assert!(!cadence.due(8, true, false, true));
        assert!(cadence.due(9, false, false, true), "still due the Round after");
        assert!(!cadence.due(9, false, true, true), "one alive at most");
    }

    #[test]
    fn the_sapper_comes_through_a_window_in_the_players_zone_first() {
        use ZoneId::*;
        let active = ZoneSet::from_iter([StartRoom, WestHall]);
        let points = [(StartRoom, 1), (WestHall, 2), (Yard, 3), (StartRoom, 4)];
        assert_eq!(window_candidates(points, StartRoom, active), [1, 4]);
        assert_eq!(window_candidates(points, EastHall, active), [1, 2, 4]);
        assert!(window_candidates([(Yard, 3)], StartRoom, active).is_empty());
    }

    fn at(target: u32, kind: SabotageKind, x: f32, powered: bool, in_use: bool) -> Candidate<u32> {
        let spot = Vec3::new(x, 0.0, 0.0);
        Candidate {
            target,
            kind,
            spot,
            use_point: spot,
            powered,
            in_use,
        }
    }

    #[test]
    fn the_sapper_goes_for_the_system_nearest_the_player() {
        let player = Vec3::new(10.0, 0.0, 0.0);
        let picked = pick_target(
            [
                at(1, SabotageKind::MysteryBox, 0.0, true, false),
                at(2, SabotageKind::PackAPunch, 8.0, true, false),
                at(3, SabotageKind::Teleporter, 20.0, true, false),
            ],
            player,
        );
        assert_eq!(picked.map(|c| c.target), Some(2));
        assert_eq!(pick_target(Vec::<Candidate<u32>>::new(), player), None);
    }

    #[test]
    fn never_quick_revive_never_unpowered_and_never_mid_use_but_a_running_trap() {
        assert!(!can_sabotage(
            SabotageKind::Perk(Perk::QuickRevive),
            true,
            false
        ));
        assert!(can_sabotage(
            SabotageKind::Perk(Perk::Juggernog),
            true,
            false
        ));
        assert!(
            !can_sabotage(SabotageKind::Perk(Perk::Juggernog), false, false),
            "unpowered"
        );
        assert!(
            !can_sabotage(SabotageKind::Perk(Perk::Juggernog), true, true),
            "being drunk"
        );
        assert!(
            can_sabotage(SabotageKind::MysteryBox, false, false),
            "the Box needs no Power"
        );
        assert!(
            !can_sabotage(SabotageKind::MysteryBox, true, true),
            "mid-spin"
        );
        assert!(
            !can_sabotage(SabotageKind::Teleporter, true, true),
            "mid-trip"
        );
        assert!(
            !can_sabotage(SabotageKind::PackAPunch, true, true),
            "mid-upgrade"
        );
        assert!(
            can_sabotage(SabotageKind::Trap, true, true),
            "a running Trap is fair game"
        );
        let player = Vec3::ZERO;
        let picked = pick_target(
            [
                at(1, SabotageKind::Perk(Perk::QuickRevive), 1.0, true, false),
                at(2, SabotageKind::MysteryBox, 2.0, true, true),
                at(3, SabotageKind::Trap, 3.0, true, true),
            ],
            player,
        );
        assert_eq!(picked.map(|c| c.target), Some(3));
    }

    #[test]
    fn headshots_are_body_hits_until_the_helmet_comes_off_at_fifteen_percent() {
        assert!(helmet_holds(1000.0, 1000.0));
        assert!(helmet_holds(1000.0, 851.0));
        assert!(!helmet_holds(1000.0, 850.0));
        assert_eq!(head_zone(true), HitZone::Torso);
        assert_eq!(head_zone(false), HitZone::Head);
    }

    #[test]
    fn a_sapper_stuck_ten_seconds_short_of_its_goal_is_moved() {
        let mut watch = SapperWatch::new(Vec3::ZERO);
        let mut t = 0.0;
        while !watch.update(Vec3::ZERO, false, 0.1) {
            t += 0.1;
            assert!(t < 20.0);
        }
        assert!((t - SAPPER_STUCK_SECS).abs() < 0.2, "moved at {t}");
        let mut busy = SapperWatch::new(Vec3::ZERO);
        assert!(
            (0..300).all(|_| !busy.update(Vec3::ZERO, true, 0.1)),
            "never stuck at its goal"
        );
    }

    /// A stuck Sapper is moved in the physics step, but the easing used to start from where it was
    /// stuck, so frames between steps showed it partway there. It cuts straight to the path node,
    /// on every frame offset and frame rate.
    #[test]
    fn a_stuck_sapper_moves_to_a_path_node_cleanly() {
        use crate::map::PLAYER_START;
        use crate::zombie::headless::Sim;
        let graph = NavGraph::new();
        let node = graph.nodes[graph.nearest(PLAYER_START)].at;
        let near = |a: Vec3, b: Vec3| a.with_y(0.0).distance(b.with_y(0.0)) < 0.5;
        for fps in [144.0, 60.0, 90.0] {
            for offset in 0..3 {
                let mut sim = Sim::new(fps, |app| {
                    app.add_systems(FixedUpdate, unstick);
                });
                let zombie = sim.spawn(Vec3::new(0.0, 0.0, 12.0), None);
                sim.run(5 + offset);
                let from = sim.frame(zombie).pos;
                let mut sapper = Sapper::new(from);
                sapper.watch.secs = SAPPER_STUCK_SECS - 0.05;
                sim.world_mut().entity_mut(zombie).insert(sapper);
                let samples = sim.track(zombie, sim.frames(0.3));
                for s in &samples {
                    assert!(near(s.pos, from) || near(s.pos, node), "{fps} fps, offset {offset}: a frame at {}", s.pos);
                }
                let last = samples.last().unwrap().pos;
                assert!(near(last, node), "{fps} fps, offset {offset}: ended at {last}");
            }
        }
    }
}
