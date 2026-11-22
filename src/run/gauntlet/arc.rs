//! The Run arc: a Gauntlet Run is `GAUNTLET_ACTS` Acts of `GAUNTLET_ACT_ROUNDS` Rounds, each ending
//! in a Boss Round. A Boss Round's Boss comes after a warning, holds the Round open, and pays out
//! when it dies; the cleared Boss Round's Offer is a Boss Offer. Clearing the last Act's Boss
//! Round is the Win, after which the Player ends the Run or carries on into Endless, where a Boss
//! Round comes every tenth Round.
//!
//! Everything reaches Classic through seams (ADRs 0002 and 0007): `ReservedRounds` keeps Classic's
//! own schedules off Boss Rounds, a `HordeSize` entry halves the Boss Round's Horde, the Boss is a
//! `SpawnZombie` request marked `Heavyweight`, and Classic's messages pay it out.

use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};
use bevy::picking::hover::Hovered;

use super::boon::{BoonDef, HeldBoons, Rarity, RunFacts, draw, eligible, family_bias, offer_weight};
use super::duos::duo_pool;
use super::heat::RunHeat;
use super::sets::SetFlashes;
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::points::BonusPoints;
use crate::economy::power_up::{DropPowerUp, PowerUp};
use crate::enemies::hellhound::{Telegraph, hellhound_health};
use crate::enemies::request::{Requested, RequestedKind, SpawnAt, SpawnProfile, SpawnZombie, Trail};
use crate::enemies::target::Players;
use crate::enemies::zombie::{Attack, DamageZombie, Dying, Heavyweight, HitZone, Pay, Zombie};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::players::player::{Player, PlayerHit};
use crate::run::round::{ReservedRounds, Round, RoundEnded, RoundPhase, RoundStarted, zombie_health};
use crate::state::{GameState, InRun, Pause, RunType};
use crate::storage::records::RunWon;
use crate::tuning::*;
use crate::ui::hud::{full_screen, text};
use crate::weapons::ammo_mod::{CastEffect, CastTarget, Element};

/// The three Bosses, one per Act, cycling in Endless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossKind {
    Bloater,
    PackAlpha,
    Warden,
}

impl BossKind {
    pub fn name(self) -> &'static str {
        match self {
            BossKind::Bloater => "THE BLOATER",
            BossKind::PackAlpha => "THE PACK ALPHA",
            BossKind::Warden => "THE WARDEN",
        }
    }

    /// The share of its health at which each of its phases begins, for the marks on its bar.
    pub fn phase_marks(self) -> &'static [f32] {
        match self {
            BossKind::Bloater => &GAUNTLET_BLOATER_BURST_AT,
            BossKind::Warden => &GAUNTLET_WARDEN_HELMET_REFORMS_AT,
            BossKind::PackAlpha => &[],
        }
    }
}

/// The last Round of the last Act, whose clearing is the Win.
pub fn win_round() -> u32 {
    GAUNTLET_ACTS * GAUNTLET_ACT_ROUNDS
}

/// Whether Round `round` is a Boss Round: the last of each Act, and every tenth in Endless.
pub fn is_boss_round(round: u32) -> bool {
    round > 0 && round.is_multiple_of(GAUNTLET_ACT_ROUNDS)
}

/// Which Act Round `round` is in, from 1, counting Endless's ten-Round stretches on (4, 5…).
pub fn act_of(round: u32) -> u32 {
    round.saturating_sub(1) / GAUNTLET_ACT_ROUNDS + 1
}

/// The index into a per-Act table for Round `round`: Endless uses the last Act's.
fn act_index(round: u32) -> usize {
    (act_of(round).min(GAUNTLET_ACTS) - 1) as usize
}

/// Whether the Offer after Round `round` is a Pact Offer: Rounds 5, 15 and 25, then each Endless
/// Act's midpoint (35, 45…). Whether an unheld Pact remains is the Offer's to find out.
pub fn is_pact_round(round: u32) -> bool {
    if round <= win_round() {
        GAUNTLET_PACT_ROUNDS.contains(&round)
    } else {
        round % GAUNTLET_ACT_ROUNDS == GAUNTLET_ACT_ROUNDS / 2
    }
}

/// The Boss of Boss Round `round`: the Bloater, the Pack Alpha and the Warden by Act, cycling on
/// in Endless. `None` for any other Round.
pub fn boss_of(round: u32) -> Option<BossKind> {
    if !is_boss_round(round) {
        return None;
    }
    Some(match (act_of(round) - 1) % 3 {
        0 => BossKind::Bloater,
        1 => BossKind::PackAlpha,
        _ => BossKind::Warden,
    })
}

/// The request for `kind`'s Boss in Boss Round `round`: through a window in an Active Zone (a
/// Hellhound Spawn Point, for a hound), holding the Round open, carrying the Boss Share.
fn boss_request(kind: BossKind, round: u32) -> SpawnZombie {
    let (requested, scale, profile) = match kind {
        BossKind::PackAlpha => (RequestedKind::Hellhound, GAUNTLET_PACK_ALPHA_SCALE, SpawnProfile::default()),
        BossKind::Warden => (
            RequestedKind::Sapper,
            GAUNTLET_WARDEN_SCALE,
            SpawnProfile {
                helmet_reforms_at: &GAUNTLET_WARDEN_HELMET_REFORMS_AT,
                sabotages_at_once: GAUNTLET_WARDEN_SABOTAGES,
                ..SpawnProfile::default()
            },
        ),
        BossKind::Bloater => (
            RequestedKind::Napalm,
            GAUNTLET_BLOATER_SCALE,
            SpawnProfile {
                detonates: false,
                trail: Some(Trail { every_secs: GAUNTLET_BLOATER_POOL_EVERY_SECS, pool_secs: GAUNTLET_BLOATER_POOL_SECS }),
                ..SpawnProfile::default()
            },
        ),
    };
    SpawnZombie {
        kind: requested,
        health_factor: boss_health_factor(round),
        scale,
        at: SpawnAt::BarrierInActiveZone,
        holds_round: true,
        profile,
        heavyweight: Some(BOSS_SHARE),
        tag: None,
        outside_cap: false,
    }
}

/// How many Hellhounds a howl calls with `alive` of the Alpha's pack still alive: a full pack, but
/// never past `GAUNTLET_PACK_ALPHA_MAX_HOUNDS` alive.
pub fn pack_to_call(alive: u32) -> u32 {
    GAUNTLET_PACK_ALPHA_PACK_SIZE.min(GAUNTLET_PACK_ALPHA_MAX_HOUNDS.saturating_sub(alive))
}

/// A howl's clock `left` seconds from the next howl, after `dt` seconds: the clock to carry on
/// with, and whether the Alpha howls now.
pub fn tick_howl(left: f32, dt: f32) -> (f32, bool) {
    let left = left - dt;
    if left <= 0.0 { (left + GAUNTLET_PACK_ALPHA_HOWL_SECS, true) } else { (left, false) }
}

/// The request for one of the Alpha's pack, struck in at `at`: a Hellhound at the latest Hellhound
/// health tier (`tier`, the Hellhound Rounds played) in Round `round`, outside the Horde and the
/// Round's holding, with no Boss Share.
fn pack_request(round: u32, tier: u32, at: Vec3) -> SpawnZombie {
    SpawnZombie {
        kind: RequestedKind::Hellhound,
        health_factor: hellhound_health(tier) / zombie_health(round) as f32,
        scale: 1.0,
        at: SpawnAt::Point(at),
        holds_round: false,
        profile: SpawnProfile::default(),
        heavyweight: None,
        tag: Some(PACK_TAG),
        outside_cap: false,
    }
}

/// What the Alpha's pack is tagged with (`SpawnZombie::tag`), so it can tell its own hounds from
/// any other requested Hellhound.
pub const PACK_TAG: u32 = 0x5041_434B;

/// Whether a requested Zombie is one of the Alpha's pack.
fn is_pack(requested: &Requested) -> bool {
    requested.tag == Some(PACK_TAG)
}

/// A Boss's health in Round `round`, as a factor on the Round's base Zombie health.
pub fn boss_health_factor(round: u32) -> f32 {
    GAUNTLET_BOSS_HEALTH_MULTIPLIER[act_index(round)]
}

/// The Scrip a cleared Boss Round adds to the Run's total.
pub fn act_scrip_bonus(round: u32) -> u64 {
    GAUNTLET_ACT_SCRIP_BONUS[act_index(round)]
}

/// The Points a Boss's death pays each Player.
pub fn boss_points(round: u32) -> u64 {
    GAUNTLET_BOSS_POINTS_PER_ACT * (act_index(round) as u64 + 1)
}

/// The Boss Rounds up to and including `through`, for `ReservedRounds`.
pub fn boss_rounds_through(through: u32) -> impl Iterator<Item = u32> {
    (1..=through / GAUNTLET_ACT_ROUNDS).map(|act| act * GAUNTLET_ACT_ROUNDS)
}

/// A Boss's shares of its health for the always-kills it shrugs off (the Boss Share).
pub const BOSS_SHARE: Heavyweight = Heavyweight {
    trap_share: TRAP_SAPPER_HEALTH_FRACTION,
    ammo_mod_share: SAPPER_AMMO_MOD_HEALTH_FRACTION,
    knife_kill_share: GAUNTLET_BOSS_KNIFE_KILL_SHARE,
};

/// The source of the Boss Round's Horde entry.
const BOSS_ROUND_SOURCE: Source = Source("Boss Round");

/// Whether a stat entry is the Run arc's.
pub fn is_arc_source(source: Source) -> bool {
    source == BOSS_ROUND_SOURCE
}

/// The Run arc's stat entries while Round `round` is in `phase`: the Boss Round's smaller Horde,
/// from the Intermission before it (when the Horde is sized) to its end.
#[cfg(test)]
pub fn arc_entries(round: u32, phase: &RoundPhase) -> Vec<Modifier> {
    arc_entries_with(round, phase, GAUNTLET_BOSS_ROUND_HORDE_FACTOR)
}

/// `arc_entries` with the Boss Round's Horde factor `horde_factor` (Iron Bosses changes it).
pub fn arc_entries_with(round: u32, phase: &RoundPhase, horde_factor: f32) -> Vec<Modifier> {
    let next = match phase {
        RoundPhase::Active => round,
        RoundPhase::Intermission(_) => round.saturating_add(1),
    };
    if is_boss_round(next) {
        vec![Modifier {
            stat: Stat::HordeSize,
            source: BOSS_ROUND_SOURCE,
            effect: Effect::Mult(horde_factor),
        }]
    } else {
        Vec::new()
    }
}

/// How many of a Boss's phase marks its health crossing from `before` to `after` (shares of its
/// max health) passes.
pub fn marks_crossed(marks: &[f32], before: f32, after: f32) -> usize {
    marks.iter().filter(|&&mark| before > mark && after <= mark).count()
}

/// A Boss Offer: one Duo drawn uniformly from those the Player is eligible for, if any, and the
/// rest Legendaries (other eligible Duos among them) weighted as a Boon Offer weights them, up to
/// `BOON_OFFER_SIZE`. Empty means no Boss Offer; a Boon Offer goes in its place.
pub fn draw_boss_offer(
    table: &'static [BoonDef],
    duos: &'static [BoonDef],
    held: &HeldBoons,
    run: &RunFacts,
    roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    draw_boss_cards(table, duos, held, run, BOON_OFFER_SIZE, true, roll)
}

/// `draw_boss_offer` for `size` cards, with or without the guaranteed Duo slot: a Banish redraws
/// one card, and only wants the guarantee if no Duo is left among the others.
pub fn draw_boss_cards(
    table: &'static [BoonDef],
    duos: &'static [BoonDef],
    held: &HeldBoons,
    run: &RunFacts,
    size: usize,
    guarantee: bool,
    mut roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    let eligible_duos = duo_pool(duos, held, run);
    let guaranteed = (guarantee && !eligible_duos.is_empty()).then(|| {
        let index = (roll(eligible_duos.len() as f64) as usize).min(eligible_duos.len() - 1);
        eligible_duos[index]
    });
    let pool = table
        .iter()
        .filter(|boon| boon.rarity == Rarity::Legendary && eligible(boon, run))
        .map(|boon| {
            let bias = family_bias(held.in_family(boon.family), run.kit == Some(boon.family));
            (boon, offer_weight(boon.rarity, held.stacks(boon.id)) * bias)
        })
        .chain(
            eligible_duos
                .iter()
                .copied()
                .filter(|duo| guaranteed.is_none_or(|picked| picked.id != duo.id))
                .map(|duo| (duo, offer_weight(duo.rarity, 0))),
        )
        .collect();
    let rest = size - usize::from(guaranteed.is_some());
    guaranteed.into_iter().chain(draw(pool, rest, roll)).collect()
}

/// The Run's arc so far.
#[derive(Resource, Default)]
pub struct RunArc {
    /// Scrip the cleared Boss Rounds have added to the Run's total.
    pub act_scrip: u64,
    /// Whether the Run has reached the Win.
    pub won: bool,
    /// The Win screen is waiting for the Player's choice.
    pub choosing: bool,
}

/// The Boss Round under way: its warning, and the request once it's made.
#[derive(Resource, Default)]
pub struct BossPlan {
    round: u32,
    kind: Option<BossKind>,
    /// Seconds left of the warning.
    warning: Option<f32>,
    /// Seconds since the Boss was asked for, until it's seen; asked again if it doesn't come.
    asked: Option<f32>,
}

/// What the Boss HUD shows: which Boss lives and how much of its health is left. The Host fills it
/// from the Boss itself (`fill_boss_view`), a client from the Host's `WorldEvent::BossHealth`.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct BossView {
    /// The Boss that is alive, if one is.
    pub kind: Option<BossKind>,
    /// Its health as a share of its maximum, from 0 to 1.
    pub share: f32,
}

/// The Host's Boss HUD reads what the Boss is.
pub fn fill_boss_view(boss: Query<(&Boss, &Zombie), Without<Dying>>, mut view: ResMut<BossView>) {
    let now = match boss.iter().next() {
        Some((boss, zombie)) => BossView { kind: Some(boss.kind), share: (zombie.health() / zombie.max_health()).clamp(0.0, 1.0) },
        None => BossView::default(),
    };
    view.set_if_neq(now);
}

/// Seconds to wait for an asked-for Boss before asking again (there was no window to come in by).
const BOSS_ASK_AGAIN_SECS: f32 = 2.0;

/// The Boss of the Boss Round under way, and the phase marks it has yet to cross.
#[derive(Component, Debug)]
pub struct Boss {
    pub kind: BossKind,
    marks_left: Vec<f32>,
    last_share: f32,
    /// Seconds to the Pack Alpha's next howl.
    howl_left: f32,
}

impl Boss {
    /// The Boss `kind`, at full health and with all its phase marks ahead.
    pub fn new(kind: BossKind) -> Self {
        Self { kind, marks_left: kind.phase_marks().to_vec(), last_share: 1.0, howl_left: GAUNTLET_PACK_ALPHA_HOWL_SECS }
    }
}

/// A Bloater's burst: a ring of fire growing from where it burst.
#[derive(Component, Debug)]
pub struct Burst {
    centre: Vec3,
    age: f32,
    hit: Vec<Entity>,
}

/// In Endless, each new Round keeps the next Boss Rounds reserved.
pub fn extend_reserved(mut started: MessageReader<RoundStarted>, round: Res<Round>, mut reserved: ResMut<ReservedRounds>) {
    if started.read().count() > 0 {
        let through = round.number.saturating_add(2 * GAUNTLET_ACT_ROUNDS);
        reserved.0.extend(boss_rounds_through(through));
    }
}

/// A Boss Round begins with its Boss's warning cue and banner.
pub fn plan_boss(
    mut started: MessageReader<RoundStarted>,
    round: Res<Round>,
    mut plan: ResMut<BossPlan>,
    mut cues: MessageWriter<PlayCue>,
) {
    if started.read().count() == 0 {
        return;
    }
    *plan = BossPlan { round: round.number, ..default() };
    if let Some(kind) = boss_of(round.number) {
        plan.kind = Some(kind);
        plan.warning = Some(GAUNTLET_BOSS_WARNING_SECS);
        cues.write(PlayCue::new(Cue::BossWarning, None));
    }
}

/// The Boss's warning counts down on every machine, for its banner; when it is over the Boss is
/// due, which only the Host acts on (`spawn_boss`).
pub fn count_boss_warning(mut plan: ResMut<BossPlan>, time: Res<Time>) {
    let Some(left) = plan.warning.as_mut() else {
        return;
    };
    *left -= time.delta_secs();
    if *left <= 0.0 {
        plan.warning = None;
        plan.asked = Some(BOSS_ASK_AGAIN_SECS);
    }
}

/// Once the warning is over, the Boss is asked for, through a window in an Active Zone, holding
/// the Round open. Asked again if it doesn't come.
pub fn spawn_boss(
    mut plan: ResMut<BossPlan>,
    bosses: Query<(), With<Boss>>,
    round: Res<Round>,
    mut spawns: MessageWriter<SpawnZombie>,
    time: Res<Time>,
    heat: Res<RunHeat>,
) {
    let Some(waited) = plan.asked.as_mut() else {
        return;
    };
    if !bosses.is_empty() {
        plan.asked = None;
        return;
    }
    *waited += time.delta_secs();
    if *waited < BOSS_ASK_AGAIN_SECS || round.phase != RoundPhase::Active {
        return;
    }
    *waited = 0.0;
    let mut request = boss_request(plan.kind.unwrap_or(BossKind::Bloater), plan.round);
    request.health_factor *= heat.boss_health_factor();
    spawns.write(request);
}

/// The Heavyweight the Boss request asked for is the Boss: it's named, and its phases tracked. A
/// summoned Boss says which it is in its tag; the Round's own is the Round's, and a Heavyweight
/// with any other share (a Sapper someone asked for) is no Boss.
pub fn tag_boss(
    mut commands: Commands,
    new: Query<(Entity, &Heavyweight, &Requested), (Added<Requested>, Without<Boss>)>,
    mut plan: ResMut<BossPlan>,
) {
    for (entity, heavyweight, requested) in &new {
        let summoned = requested.tag.and_then(boss_of_tag);
        let Some(kind) = summoned.or_else(|| (*heavyweight == BOSS_SHARE).then_some(plan.kind).flatten()) else {
            continue;
        };
        if summoned.is_none() {
            plan.asked = None;
        }
        commands.entity(entity).insert(Boss::new(kind));
    }
}

/// What a summoned Boss's `SpawnZombie::tag` starts with; its kind is added.
const BOSS_TAG: u32 = 0x4253_5300;

fn boss_tag(kind: BossKind) -> u32 {
    BOSS_TAG + kind as u32
}

fn boss_of_tag(tag: u32) -> Option<BossKind> {
    [BossKind::Bloater, BossKind::PackAlpha, BossKind::Warden].into_iter().find(|&kind| boss_tag(kind) == tag)
}

/// Summon `kind`'s Boss at `at` now, whatever the Round: at the Round's Boss health, outside the
/// Horde and the Alive Cap, and not holding the Round open. A Boss Round's own Boss is not affected.
/// The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SummonBoss {
    pub kind: BossKind,
    pub at: Vec3,
}

/// Each `SummonBoss` asks Classic for that Boss, as the Round's own is, tagged so `tag_boss` knows it.
pub fn summon_boss(
    mut summons: MessageReader<SummonBoss>,
    round: Res<Round>,
    heat: Res<RunHeat>,
    mut spawns: MessageWriter<SpawnZombie>,
) {
    for summon in summons.read() {
        let mut request = boss_request(summon.kind, round.number);
        request.health_factor *= heat.boss_health_factor();
        request.at = SpawnAt::Point(summon.at);
        request.holds_round = false;
        request.outside_cap = true;
        request.tag = Some(boss_tag(summon.kind));
        spawns.write(request);
    }
}

/// The Bloater bursts as its health crosses each phase mark.
pub fn bloater_bursts(
    mut commands: Commands,
    mut bosses: Query<(&mut Boss, &Zombie, &Transform), Without<Dying>>,
    mut flashes: ResMut<SetFlashes>,
) {
    for (mut boss, zombie, transform) in &mut bosses {
        let share = (zombie.health() / zombie.max_health()).max(0.0);
        let crossed = marks_crossed(&boss.marks_left, boss.last_share, share);
        boss.last_share = share;
        if crossed == 0 {
            continue;
        }
        boss.marks_left.retain(|&mark| mark < share);
        if boss.kind == BossKind::Bloater {
            let centre = feet_of(transform, zombie);
            commands.spawn((InRun, Burst { centre, age: 0.0, hit: Vec::new() }));
            flashes.tell_burst_ring(centre);
        }
    }
}

/// A burst's ring grows to its full radius, hurting each Player it reaches once and setting alight
/// each Zombie it reaches, the Boss aside.
pub fn grow_bursts(
    mut commands: Commands,
    mut bursts: Query<(Entity, &mut Burst)>,
    players: Players,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>, Without<Boss>)>,
    mut hits: MessageWriter<PlayerHit>,
    mut casts: MessageWriter<CastEffect>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    for (entity, mut burst) in &mut bursts {
        burst.age += time.delta_secs();
        let radius = GAUNTLET_BLOATER_BURST_RADIUS_M * (burst.age / GAUNTLET_BLOATER_BURST_SECS).min(1.0);
        let centre = burst.centre;
        for player in players.iter() {
            if player.feet.distance(centre) <= radius && !burst.hit.contains(&player.entity) {
                burst.hit.push(player.entity);
                hits.write(PlayerHit { player: player.entity, damage: GAUNTLET_BLOATER_BURST_DAMAGE, from: Some(centre) });
            }
        }
        // The fire is the Boss's, but a cast has to credit someone: the first Player, Unpaid.
        if let Some(credit) = players.first() {
            for (zombie, at) in &zombies {
                if at.translation().distance(centre) <= radius && !burst.hit.contains(&zombie) {
                    burst.hit.push(zombie);
                    casts.write(CastEffect {
                        player: credit.entity,
                        element: Element::Fire,
                        target: CastTarget::Zombie(zombie),
                        size: 0.1,
                        reach: None,
                        lasts: 0.0,
                        pay: Pay::Unpaid,
                        accent: None,
                        show: false,
                        unbounded: false,
                        tint: None,
                        ignites: false,
                    });
                }
            }
        }
        draw_burst_ring(&mut gizmos, centre, radius);
        if burst.age >= GAUNTLET_BLOATER_BURST_SECS {
            commands.entity(entity).despawn();
        }
    }
}

/// A burst's ring of fire, `radius` wide round `centre`.
fn draw_burst_ring(gizmos: &mut Gizmos, centre: Vec3, radius: f32) {
    let ring = Isometry3d::new(centre + Vec3::Y * 0.1, Quat::from_rotation_x(FRAC_PI_2));
    gizmos.circle(ring, radius, Color::srgb(1.0, 0.45, 0.1));
    gizmos.circle(ring, radius * 0.97, Color::srgb(1.0, 0.8, 0.3));
}

/// A Bloater's burst as a client sees it: the ring the Host told of, growing and gone. It hurts no
/// one; the Host's `Burst` does that.
#[derive(Component, Debug)]
pub struct BurstView {
    centre: Vec3,
    radius: f32,
    secs: f32,
    age: f32,
}

impl BurstView {
    pub fn new(centre: Vec3, radius: f32, secs: f32) -> Self {
        Self { centre, radius, secs, age: 0.0 }
    }
}

/// Grows and draws each burst ring the Host told of, and lets it go when it has grown.
pub fn draw_burst_views(mut commands: Commands, mut views: Query<(Entity, &mut BurstView)>, mut gizmos: Gizmos, time: Res<Time>) {
    for (entity, mut view) in &mut views {
        view.age += time.delta_secs();
        let radius = view.radius * (view.age / view.secs.max(f32::EPSILON)).min(1.0);
        draw_burst_ring(&mut gizmos, view.centre, radius);
        if view.age >= view.secs {
            commands.entity(entity).despawn();
        }
    }
}

/// Where a standing Zombie's feet are, whatever its kind and scale.
fn feet_of(transform: &Transform, zombie: &Zombie) -> Vec3 {
    transform.translation - Vec3::Y * zombie.half_height()
}

/// The Pack Alpha howls every `GAUNTLET_PACK_ALPHA_HOWL_SECS`, striking in a pack of Hellhounds
/// around it, as many as keep its pack to `GAUNTLET_PACK_ALPHA_MAX_HOUNDS` alive.
pub fn pack_howls(
    mut bosses: Query<(&mut Boss, &Transform, &Zombie), Without<Dying>>,
    hounds: Query<(&Zombie, &Requested), (Without<Heavyweight>, Without<Dying>)>,
    round: Res<Round>,
    mut spawns: MessageWriter<SpawnZombie>,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time>,
) {
    for (mut boss, transform, zombie) in &mut bosses {
        if boss.kind != BossKind::PackAlpha {
            continue;
        }
        let (left, howls) = tick_howl(boss.howl_left, time.delta_secs());
        boss.howl_left = left;
        if !howls {
            continue;
        }
        let alive = hounds.iter().filter(|(hound, requested)| hound.is_hellhound() && is_pack(requested)).count() as u32;
        let calling = pack_to_call(alive);
        if calling == 0 {
            continue;
        }
        let feet = feet_of(transform, zombie);
        cues.write(PlayCue::new(Cue::BossHowl, Some(transform.translation)));
        for i in 0..calling {
            // Spread evenly round the Alpha, with a little jitter so they don't line up.
            let angle = TAU * (i as f32 + rand::random_range(0.0..0.5)) / calling as f32;
            let dist = rand::random_range(HELLHOUND_AROUND_MIN_DIST..HELLHOUND_AROUND_MAX_DIST);
            let at = feet + Vec3::new(angle.cos(), 0.0, angle.sin()) * dist;
            spawns.write(pack_request(round.number, round.hellhound_tier(), at));
        }
    }
}

/// With the Pack Alpha dead, its pack dies with it, Unpaid, through `DamageZombie`. A hound still
/// in its lightning can't be hurt, so it goes the moment its strike is over.
pub fn pack_dies_with_alpha(
    bosses: Query<&Boss, Without<Dying>>,
    pack: Query<(Entity, &Zombie, &Requested), (Without<Heavyweight>, Without<Dying>, Without<Telegraph>)>,
    mut damage: MessageWriter<DamageZombie>,
) {
    if bosses.iter().any(|boss| boss.kind == BossKind::PackAlpha) {
        return;
    }
    for (hound, zombie, requested) in &pack {
        if zombie.is_hellhound() && is_pack(requested) {
            damage.write(DamageZombie {
                player: None,
                zombie: hound,
                damage: f32::INFINITY,
                zone: HitZone::Torso,
                attack: Attack::AmmoMod(Pay::Unpaid),
            });
        }
    }
}

/// A Boss's death pays every Player its Points and drops a Max Ammo where it fell.
pub fn boss_deaths(
    deaths: Query<(&Transform, &Zombie), (With<Boss>, Added<Dying>)>,
    round: Res<Round>,
    players: Query<Entity, With<Player>>,
    mut bonus: MessageWriter<BonusPoints>,
    mut drops: MessageWriter<DropPowerUp>,
) {
    for (transform, zombie) in &deaths {
        for player in &players {
            bonus.write(BonusPoints { player, amount: boss_points(round.number) });
        }
        let feet = feet_of(transform, zombie);
        drops.write(DropPowerUp { at: feet, power_up: Some(PowerUp::MaxAmmo) });
    }
}

/// A cleared Boss Round adds its Act's Scrip. Clearing the last Act's is the Win: solo the world
/// freezes for the Player's choice; in Co-op the Run carries on into Endless.
pub fn clear_boss_rounds(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    mut arc: ResMut<RunArc>,
    mut won: ResMut<RunWon>,
    run_type: Res<RunType>,
    mut cues: MessageWriter<PlayCue>,
) {
    if ended.read().count() == 0 || !is_boss_round(round.number) {
        return;
    }
    arc.act_scrip = arc.act_scrip.saturating_add(act_scrip_bonus(round.number));
    if round.number == win_round() && !arc.won {
        arc.won = true;
        won.0 = true;
        if !run_type.is_coop() {
            arc.choosing = true;
            cues.write(PlayCue::new(Cue::GauntletWin, None));
        }
    }
}

/// A client's own Win: the Host's `RoundEnded` at `win_round()` is the Win, so the client's
/// `RunArc.won` is set from it, as `clear_boss_rounds` sets the Host's. Co-op has no Win screen.
pub fn infer_win(mut ended: MessageReader<RoundEnded>, round: Res<Round>, mut arc: ResMut<RunArc>, mut won: ResMut<RunWon>) {
    if ended.read().count() > 0 && round.number == win_round() && !arc.won {
        arc.won = true;
        won.0 = true;
    }
}

/// The Win screen's End the Run button.
#[derive(Component)]
pub struct EndRunButton;

/// The Win screen's Keep going button.
#[derive(Component)]
pub struct KeepGoingButton;

const WIN_GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const BUTTON_IDLE: Color = Color::srgba(0.07, 0.07, 0.09, 0.95);

/// The Win screen, over the world frozen for it, whenever the world freezes while it waits.
pub fn spawn_win_screen(mut commands: Commands, arc: Res<RunArc>) {
    if !arc.choosing {
        return;
    }
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(28.0),
                ..full_screen()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.78)),
            GlobalZIndex(21),
            DespawnOnExit(Pause::Frozen),
        ))
        .with_children(|root| {
            root.spawn(text(format!("ROUND {} CLEARED", win_round()), 22.0, Color::srgb(0.65, 0.65, 0.65)));
            root.spawn(text("WIN", 96.0, WIN_GOLD));
            root.spawn(Node { column_gap: px(28.0), ..default() }).with_children(|row| {
                for (label, end) in [("END THE RUN", true), ("KEEP GOING", false)] {
                    let mut button = row.spawn((
                        Button,
                        Hovered::default(),
                        Node {
                            padding: UiRect::axes(px(36.0), px(14.0)),
                            border: UiRect::all(px(3.0)),
                            ..default()
                        },
                        BorderColor::all(WIN_GOLD),
                        BackgroundColor(BUTTON_IDLE),
                    ));
                    button.with_child(text(label, 26.0, Color::WHITE));
                    if end {
                        button.insert(EndRunButton);
                    } else {
                        button.insert(KeepGoingButton);
                    }
                }
            });
            root.spawn(text("Keep going into Endless: a Boss every tenth Round. The Run stays a Win.", 18.0, Color::srgb(0.65, 0.65, 0.65)));
        });
}

/// End the Run ends it as a Win, to Game Over; Keep going lets the Boss Offer and Endless follow.
pub fn click_win(
    activate: On<Activate>,
    ends: Query<(), With<EndRunButton>>,
    keeps: Query<(), With<KeepGoingButton>>,
    mut arc: ResMut<RunArc>,
    mut next: ResMut<NextState<GameState>>,
    mut cues: MessageWriter<PlayCue>,
) {
    if !arc.choosing {
        return;
    }
    if ends.contains(activate.entity) {
        arc.choosing = false;
        cues.write(PlayCue::new(Cue::OfferPick, None));
        next.set(GameState::GameOver);
    } else if keeps.contains(activate.entity) {
        arc.choosing = false;
        cues.write(PlayCue::new(Cue::OfferPick, None));
    }
}

/// The Boss's name and health bar across the top of the HUD, with its phase marks.
#[derive(Component)]
pub struct BossBar;

#[derive(Component)]
pub struct BossBarName;

#[derive(Component)]
pub struct BossBarFill;

/// A phase mark on the Boss's bar, at its share of the bar's width.
#[derive(Component)]
pub struct BossBarMark(usize);

/// The Boss's warning banner.
#[derive(Component)]
pub struct BossWarningBanner;

const BOSS_BAR_WIDTH_PX: f32 = 520.0;
const BOSS_RED: Color = Color::srgb(0.85, 0.15, 0.1);

pub fn spawn_boss_hud(mut commands: Commands) {
    commands
        .spawn((
            InRun,
            BossBar,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: px(18.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4.0),
                ..default()
            },
        ))
        .with_children(|bar| {
            bar.spawn((BossBarName, text("", 22.0, Color::WHITE)));
            bar.spawn((
                Node {
                    width: px(BOSS_BAR_WIDTH_PX),
                    height: px(14.0),
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.6)),
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .with_children(|track| {
                track.spawn((BossBarFill, Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(BOSS_RED)));
                for index in 0..2 {
                    track.spawn((
                        BossBarMark(index),
                        Visibility::Hidden,
                        Node {
                            position_type: PositionType::Absolute,
                            width: px(2.0),
                            height: percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::WHITE),
                    ));
                }
            });
        });
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(22.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((BossWarningBanner, text("", 40.0, BOSS_RED), Visibility::Hidden));
}

/// The warning banner shows while the Boss's warning runs; the bar shows while the Boss lives.
/// It reads only `BossPlan` (set from the Round) and `BossView`, whichever machine it is on.
pub fn update_boss_hud(
    plan: Res<BossPlan>,
    view: Res<BossView>,
    mut bar: Single<&mut Visibility, (With<BossBar>, Without<BossWarningBanner>, Without<BossBarMark>)>,
    mut name: Single<&mut Text, (With<BossBarName>, Without<BossWarningBanner>)>,
    mut fill: Single<&mut Node, (With<BossBarFill>, Without<BossBarMark>)>,
    mut marks: Query<(&BossBarMark, &mut Node, &mut Visibility), (Without<BossBarFill>, Without<BossBar>, Without<BossWarningBanner>)>,
    mut banner: Single<(&mut Text, &mut Visibility), (With<BossWarningBanner>, Without<BossBarName>, Without<BossBar>, Without<BossBarMark>)>,
) {
    let (banner_text, banner_visibility) = &mut *banner;
    match (plan.warning, plan.kind) {
        (Some(_), Some(kind)) => {
            banner_text.0 = format!("{} APPROACHES", kind.name());
            banner_visibility.set_if_neq(Visibility::Inherited);
        }
        _ => {
            banner_visibility.set_if_neq(Visibility::Hidden);
        }
    }
    let Some(kind) = view.kind else {
        bar.set_if_neq(Visibility::Hidden);
        return;
    };
    bar.set_if_neq(Visibility::Inherited);
    if name.0 != kind.name() {
        name.0 = kind.name().to_string();
    }
    fill.width = percent(view.share.clamp(0.0, 1.0) * 100.0);
    let phase_marks = kind.phase_marks();
    for (&BossBarMark(index), mut node, mut visibility) in &mut marks {
        match phase_marks.get(index) {
            Some(&mark) => {
                node.left = percent(mark * 100.0);
                visibility.set_if_neq(Visibility::Inherited);
            }
            None => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_is_three_acts_of_ten_with_a_boss_round_ending_each() {
        let bosses: Vec<u32> = (1..=30).filter(|&r| is_boss_round(r)).collect();
        assert_eq!(bosses, [10, 20, 30]);
        assert_eq!(win_round(), 30);
        assert_eq!([act_of(1), act_of(10), act_of(11), act_of(20), act_of(21), act_of(30)], [1, 1, 2, 2, 3, 3]);
        assert!(!is_boss_round(0));
    }

    #[test]
    fn pact_offers_come_after_rounds_five_fifteen_and_twenty_five_then_each_endless_midpoint() {
        let pacts: Vec<u32> = (0..=60).filter(|&r| is_pact_round(r)).collect();
        assert_eq!(pacts, [5, 15, 25, 35, 45, 55]);
        assert!(!is_pact_round(10) && !is_pact_round(20) && !is_pact_round(30), "never on a Boss Round's Offer");
    }

    #[test]
    fn endless_has_a_boss_round_every_tenth_round_cycling_the_three_bosses() {
        assert_eq!(boss_of(10), Some(BossKind::Bloater));
        assert_eq!(boss_of(20), Some(BossKind::PackAlpha));
        assert_eq!(boss_of(30), Some(BossKind::Warden));
        assert_eq!(boss_of(40), Some(BossKind::Bloater));
        assert_eq!(boss_of(50), Some(BossKind::PackAlpha));
        assert_eq!(boss_of(60), Some(BossKind::Warden));
        assert_eq!(boss_of(35), None);
        assert!(is_boss_round(1000));
    }

    #[test]
    fn act_three_comes_the_warden_and_each_act_its_own_boss() {
        for round in [10, 40] {
            assert_eq!(boss_of(round), Some(BossKind::Bloater), "round {round}");
        }
        for round in [20, 50] {
            assert_eq!(boss_of(round), Some(BossKind::PackAlpha), "round {round}");
        }
        for round in [30, 60] {
            assert_eq!(boss_of(round), Some(BossKind::Warden), "round {round}");
        }
        assert_eq!(boss_of(15), None);
    }

    #[test]
    fn the_warden_is_a_boss_sapper_with_reforming_helmet_and_double_sabotage() {
        let request = boss_request(BossKind::Warden, 30);
        assert_eq!(request.kind, RequestedKind::Sapper);
        assert_eq!(request.scale, GAUNTLET_WARDEN_SCALE);
        assert_eq!(request.health_factor, boss_health_factor(30));
        assert_eq!(request.at, SpawnAt::BarrierInActiveZone);
        assert!(request.holds_round);
        assert_eq!(request.heavyweight, Some(BOSS_SHARE));
        assert_eq!(request.profile.helmet_reforms_at, &GAUNTLET_WARDEN_HELMET_REFORMS_AT);
        assert_eq!(request.profile.sabotages_at_once, GAUNTLET_WARDEN_SABOTAGES);
        assert_eq!(BossKind::Warden.phase_marks(), &GAUNTLET_WARDEN_HELMET_REFORMS_AT);
    }

    #[test]
    fn the_pack_alpha_is_a_boss_hellhound_that_holds_the_round() {
        let request = boss_request(BossKind::PackAlpha, 20);
        assert_eq!(request.kind, RequestedKind::Hellhound);
        assert_eq!(request.scale, GAUNTLET_PACK_ALPHA_SCALE);
        assert_eq!(request.health_factor, 40.0, "the Act 2 multiplier on the Round's base health");
        assert_eq!(request.at, SpawnAt::BarrierInActiveZone);
        assert!(request.holds_round);
        assert_eq!(request.heavyweight, Some(BOSS_SHARE));
        assert_eq!(request.profile, SpawnProfile::default(), "a Hellhound has no Napalm options");
    }

    #[test]
    fn the_bloaters_request_is_unchanged() {
        let request = boss_request(BossKind::Bloater, 10);
        assert_eq!((request.kind, request.scale, request.health_factor), (RequestedKind::Napalm, GAUNTLET_BLOATER_SCALE, 30.0));
        assert!(request.holds_round && !request.profile.detonates);
    }

    #[test]
    fn a_howl_calls_a_full_pack_but_never_more_than_eight_alive() {
        let called: Vec<u32> = [0, 1, 4, 5, 6, 8, 9].into_iter().map(pack_to_call).collect();
        assert_eq!(called, [4, 4, 4, 3, 2, 0, 0]);
    }

    #[test]
    fn the_alpha_howls_every_twenty_seconds() {
        let (mut left, mut howls) = (GAUNTLET_PACK_ALPHA_HOWL_SECS, Vec::new());
        for second in 1..=60 {
            let (next, howled) = tick_howl(left, 1.0);
            left = next;
            if howled {
                howls.push(second);
            }
        }
        assert_eq!(howls, [20, 40, 60]);
        assert_eq!(tick_howl(1.0, 0.5), (0.5, false));
    }

    #[test]
    fn the_pack_sits_outside_the_horde_at_the_latest_hellhound_health() {
        use crate::enemies::request::requested_health;
        let at = Vec3::new(1.0, 0.0, 2.0);
        for (round, tier) in [(20, 3), (20, 9), (50, 12)] {
            let request = pack_request(round, tier, at);
            assert_eq!(request.kind, RequestedKind::Hellhound);
            assert!(!request.holds_round, "the pack never holds the Round");
            assert_eq!(request.heavyweight, None, "no Boss Share");
            assert_eq!(request.at, SpawnAt::Point(at));
            let health = requested_health(round, request.health_factor);
            assert!((health - hellhound_health(tier)).abs() < 0.01, "round {round} tier {tier}: {health}");
        }
        assert_eq!(hellhound_health(9), 1600.0, "the latest tier covers every later Round");
    }

    fn pack_hound() -> Requested {
        Requested { tag: Some(PACK_TAG), outside_cap: false }
    }

    /// The pack's damage messages once the system has run over `alpha` and a pack of one hound.
    fn pack_damage(alpha: impl FnOnce(&mut EntityWorldMut)) -> Vec<(Entity, Attack, f32)> {
        let mut app = App::new();
        app.add_message::<DamageZombie>().add_systems(Update, pack_dies_with_alpha);
        let boss = Boss { kind: BossKind::PackAlpha, marks_left: Vec::new(), last_share: 1.0, howl_left: 5.0 };
        alpha(&mut app.world_mut().spawn(boss));
        let hound = app.world_mut().spawn((pack_hound(), Zombie::hellhound(100.0, Vec3::ZERO))).id();
        // A Hellhound of the Horde, and a Heavyweight one, are never the pack.
        app.world_mut().spawn(Zombie::hellhound(100.0, Vec3::ZERO));
        app.world_mut().spawn((pack_hound(), BOSS_SHARE, Zombie::hellhound(100.0, Vec3::ZERO)));
        // A Hellhound someone else asked for is not the pack.
        app.world_mut().spawn((Requested::default(), Zombie::hellhound(100.0, Vec3::ZERO)));
        app.update();
        let hits: Vec<_> = app.world_mut().resource_mut::<Messages<DamageZombie>>().drain().collect();
        assert!(hits.iter().all(|hit| hit.zombie == hound && hit.player.is_none()));
        hits.iter().map(|hit| (hit.zombie, hit.attack, hit.damage)).collect()
    }

    #[test]
    fn the_pack_lives_while_the_alpha_does() {
        assert!(pack_damage(|_| {}).is_empty());
    }

    #[test]
    fn the_pack_dies_unpaid_when_the_alpha_dies() {
        let hits = pack_damage(|alpha| {
            alpha.insert(Dying::default());
        });
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].1, hits[0].2), (Attack::AmmoMod(Pay::Unpaid), f32::INFINITY));
    }

    #[test]
    fn a_pack_hound_left_with_no_living_alpha_dies_too() {
        let mut app = App::new();
        app.add_message::<DamageZombie>().add_systems(Update, pack_dies_with_alpha);
        app.world_mut().spawn((pack_hound(), Zombie::hellhound(100.0, Vec3::ZERO)));
        // No living Alpha, whether it's dying or already gone, is what ends the pack.
        app.update();
        assert_eq!(app.world_mut().resource_mut::<Messages<DamageZombie>>().drain().count(), 1);
    }

    #[test]
    fn iron_bosses_changes_the_boss_rounds_horde_factor_and_nothing_outside_a_boss_round() {
        let horde = |round, factor| arc_entries_with(round, &RoundPhase::Active, factor);
        assert_eq!(horde(10, GAUNTLET_BOSS_ROUND_HORDE_FACTOR)[0].effect, Effect::Mult(0.5));
        assert_eq!(horde(10, 1.0)[0].effect, Effect::Mult(1.0));
        assert!(horde(9, 1.0).is_empty());
        assert_eq!(arc_entries(10, &RoundPhase::Active), horde(10, GAUNTLET_BOSS_ROUND_HORDE_FACTOR));
    }

    #[test]
    fn boss_health_scrip_and_points_grow_by_act_and_endless_pays_as_act_three() {
        assert_eq!([boss_health_factor(10), boss_health_factor(20), boss_health_factor(30)], [30.0, 40.0, 50.0]);
        assert_eq!(boss_health_factor(70), 50.0);
        assert_eq!([act_scrip_bonus(10), act_scrip_bonus(20), act_scrip_bonus(30), act_scrip_bonus(40)], [50, 100, 200, 200]);
        assert_eq!([boss_points(10), boss_points(20), boss_points(30), boss_points(90)], [1000, 2000, 3000, 3000]);
    }

    /// Boss health is the Round's base Zombie health × the Act's multiplier.
    #[test]
    fn boss_health_comes_from_the_base_round_health() {
use crate::enemies::request::requested_health;
        assert_eq!(requested_health(10, boss_health_factor(10)), 1045.0 * 30.0);
        assert_eq!(requested_health(30, boss_health_factor(30)), 7000.0 * 50.0);
    }

    #[test]
    fn every_boss_round_is_reserved_up_to_the_horizon() {
        assert_eq!(boss_rounds_through(30).collect::<Vec<_>>(), [10, 20, 30]);
        assert_eq!(boss_rounds_through(59).collect::<Vec<_>>(), [10, 20, 30, 40, 50]);
        assert_eq!(boss_rounds_through(9).count(), 0);
    }

    #[test]
    fn a_boss_rounds_horde_is_halved_from_the_intermission_before_it() {
        let halved = |round, phase: RoundPhase| !arc_entries(round, &phase).is_empty();
        assert!(halved(9, RoundPhase::Intermission(5.0)), "sized as Round 10 begins");
        assert!(halved(10, RoundPhase::Active));
        assert!(!halved(10, RoundPhase::Intermission(5.0)));
        assert!(!halved(9, RoundPhase::Active));
        let entry = arc_entries(20, &RoundPhase::Active)[0];
        assert_eq!((entry.stat, entry.effect), (Stat::HordeSize, Effect::Mult(GAUNTLET_BOSS_ROUND_HORDE_FACTOR)));
        assert!(is_arc_source(entry.source));
    }

    #[test]
    fn the_bloater_bursts_once_at_each_phase_mark() {
        let marks = GAUNTLET_BLOATER_BURST_AT;
        assert_eq!(marks_crossed(&marks, 1.0, 0.7), 0);
        assert_eq!(marks_crossed(&marks, 0.7, 0.66), 1);
        assert_eq!(marks_crossed(&marks, 0.66, 0.5), 0, "already past it");
        assert_eq!(marks_crossed(&marks, 0.9, 0.1), 2, "one big hit passes both");
        assert_eq!(marks_crossed(&marks, 0.2, 0.0), 0);
    }

    fn boss_app() -> App {
        let mut app = App::new();
        app.add_message::<RoundStarted>()
            .add_message::<PlayCue>()
            .init_resource::<BossPlan>()
            .init_resource::<Time>()
            .insert_resource(Round::shown(20, None))
            .add_systems(Update, (plan_boss, count_boss_warning).chain());
        app
    }

    #[test]
    fn a_boss_rounds_warning_and_name_come_from_the_round_alone_on_any_machine() {
        let mut app = boss_app();
        app.world_mut().write_message(RoundStarted);
        app.update();
        let plan = app.world().resource::<BossPlan>();
        assert_eq!((plan.kind, plan.warning.is_some()), (Some(BossKind::PackAlpha), true));
        assert_eq!(app.world().resource::<Messages<PlayCue>>().iter_current_update_messages().filter(|cue| cue.cue == Cue::BossWarning).count(), 1);
    }

    #[test]
    fn a_plain_rounds_start_plans_no_boss_and_no_warning() {
        let mut app = boss_app();
        app.insert_resource(Round::shown(7, None));
        app.world_mut().write_message(RoundStarted);
        app.update();
        let plan = app.world().resource::<BossPlan>();
        assert_eq!((plan.kind, plan.warning), (None, None));
        assert_eq!(app.world().resource::<Messages<PlayCue>>().len(), 0);
    }

    #[test]
    fn the_warning_counts_down_to_asking_for_the_boss() {
        let mut app = boss_app();
        app.world_mut().write_message(RoundStarted);
        app.update();
        app.world_mut().resource_mut::<BossPlan>().warning = Some(0.0);
        app.update();
        let plan = app.world().resource::<BossPlan>();
        assert_eq!((plan.warning, plan.asked), (None, Some(BOSS_ASK_AGAIN_SECS)));
    }

    #[test]
    fn the_boss_view_follows_the_boss_and_empties_when_it_dies() {
        let mut app = App::new();
        app.init_resource::<BossView>().add_systems(Update, fill_boss_view);
        let mut zombie = Zombie::hellhound(100.0, Vec3::ZERO);
        zombie.scale_health(1.0);
        let boss = app.world_mut().spawn((Boss::new(BossKind::PackAlpha), zombie)).id();
        app.update();
        assert_eq!(*app.world().resource::<BossView>(), BossView { kind: Some(BossKind::PackAlpha), share: 1.0 });
        app.world_mut().entity_mut(boss).insert(Dying::default());
        app.update();
        assert_eq!(*app.world().resource::<BossView>(), BossView::default());
    }

    #[test]
    fn a_bloaters_burst_is_told_as_one_growing_ring_for_the_whole_burst() {
        use crate::net::fx::Fx;
        let mut flashes = SetFlashes::default();
        flashes.tell_burst_ring(Vec3::new(1.0, 0.0, 2.0));
        assert_eq!(
            flashes.sent,
            [Fx::GrowingRing { at: Vec3::new(1.0, 0.0, 2.0), radius: GAUNTLET_BLOATER_BURST_RADIUS_M, secs: GAUNTLET_BLOATER_BURST_SECS }]
        );
    }

    /// Bevy checks a system's queries for clashing access as it's set up, which would otherwise
    /// only show as a panic when a Gauntlet Run starts.
    #[test]
    fn the_arcs_systems_have_no_clashing_queries() {
        fn set_up<M>(system: impl IntoSystem<(), (), M>) {
            let mut world = World::new();
            let mut system = IntoSystem::into_system(system);
            system.initialize(&mut world);
        }
        set_up(update_boss_hud);
        set_up(grow_bursts);
        set_up(bloater_bursts);
        set_up(spawn_boss);
        set_up(count_boss_warning);
        set_up(plan_boss);
        set_up(fill_boss_view);
        set_up(draw_burst_views);
        set_up(pack_howls);
        set_up(pack_dies_with_alpha);
        set_up(boss_deaths);
        set_up(clear_boss_rounds);
    }

    fn holding(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(super::super::boon::find(id).expect(id));
        }
        held
    }

    fn everything() -> RunFacts {
        RunFacts { unlock_all: true, ..default() }
    }

    #[test]
    fn a_boss_offer_is_three_legendaries_one_an_eligible_duo() {
        use super::super::boon::BOONS;
        use super::super::duos::{DUOS, GUN_AND_BLADE};
        let held = holding(&["marksmans_eye", "trench_raider"]);
        for seed in 0..20u64 {
            let mut n = seed;
            let offer = draw_boss_offer(BOONS, DUOS, &held, &everything(), |total| {
                n = n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (n >> 11) as f64 / (1u64 << 53) as f64 * total
            });
            assert_eq!(offer.len(), BOON_OFFER_SIZE);
            assert_eq!(offer[0].id, GUN_AND_BLADE, "the only eligible Duo is guaranteed");
            assert!(offer.iter().all(|boon| boon.rarity == Rarity::Legendary));
            let mut ids: Vec<_> = offer.iter().map(|boon| boon.id).collect();
            ids.dedup();
            assert_eq!(ids.len(), offer.len(), "no card twice");
        }
    }

    #[test]
    fn a_boss_offer_with_no_eligible_duo_is_all_legendary_boons() {
        use super::super::boon::BOONS;
        use super::super::duos::DUOS;
        let offer = draw_boss_offer(BOONS, DUOS, &HeldBoons::default(), &everything(), |total| total * 0.5);
        assert_eq!(offer.len(), BOON_OFFER_SIZE);
        assert!(offer.iter().all(|boon| boon.rarity == Rarity::Legendary && boon.duo.is_none()));
        let locked = draw_boss_offer(BOONS, DUOS, &HeldBoons::default(), &RunFacts::default(), |total| total * 0.5);
        assert!(locked.is_empty(), "no Legendary Unlocked: a Boon Offer goes in its place");
    }

    /// The Boss Share: a Boss takes the Sapper's trap and Ammo Mod shares and a Knife share of its
    /// own, so no always-kill kills it outright.
    #[test]
    fn a_boss_takes_its_share_from_each_always_kill() {
        use crate::enemies::zombie::{Attack, heavyweight_damage};
        let max = 31_350.0;
        assert_eq!(BOSS_SHARE.trap_share * max, max * 0.2);
        assert_eq!(heavyweight_damage(f32::INFINITY, Attack::AmmoMod(Pay::Paid), max, false, &BOSS_SHARE), max * 0.05);
        assert_eq!(heavyweight_damage(f32::INFINITY, Attack::Knife, max, false, &BOSS_SHARE), max * 0.02);
        assert_eq!(heavyweight_damage(5_000.0, Attack::Bullet, max, false, &BOSS_SHARE), 5_000.0);
    }
}
