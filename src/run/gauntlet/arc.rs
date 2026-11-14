//! The Run arc: a Gauntlet Run is `GAUNTLET_ACTS` Acts of `GAUNTLET_ACT_ROUNDS` Rounds, each ending
//! in a Boss Round. A Boss Round's Boss comes after a warning, holds the Round open, and pays out
//! when it dies; the cleared Boss Round's Offer is a Boss Offer. Clearing the last Act's Boss
//! Round is the Win, after which the Player ends the Run or carries on into Endless, where a Boss
//! Round comes every tenth Round.
//!
//! Everything reaches Classic through seams (ADRs 0002 and 0007): `ReservedRounds` keeps Classic's
//! own schedules off Boss Rounds, a `HordeSize` entry halves the Boss Round's Horde, the Boss is a
//! `SpawnZombie` request marked `Heavyweight`, and Classic's messages pay it out.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};
use bevy::picking::hover::Hovered;

use super::boon::{BoonDef, HeldBoons, Rarity, RunFacts, draw, eligible, family_bias, offer_weight};
use super::duos::duo_pool;
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::points::BonusPoints;
use crate::economy::power_up::{DropPowerUp, PowerUp};
use crate::enemies::request::{Requested, RequestedKind, SpawnAt, SpawnProfile, SpawnZombie, Trail};
use crate::enemies::target::Players;
use crate::enemies::zombie::{Dying, Heavyweight, Pay, Zombie};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::players::player::{Player, PlayerHit};
use crate::run::round::{ReservedRounds, Round, RoundEnded, RoundPhase, RoundStarted};
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
            BossKind::PackAlpha | BossKind::Warden => &[],
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

/// The Boss that actually comes in Boss Round `round`. Until the Pack Alpha and the Warden are
/// built (slices 4b and 4c), the Bloater stands in for every Act.
fn spawned_boss(round: u32) -> Option<BossKind> {
    boss_of(round).map(|_| BossKind::Bloater)
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
pub fn arc_entries(round: u32, phase: &RoundPhase) -> Vec<Modifier> {
    let next = match phase {
        RoundPhase::Active => round,
        RoundPhase::Intermission(_) => round.saturating_add(1),
    };
    if is_boss_round(next) {
        vec![Modifier {
            stat: Stat::HordeSize,
            source: BOSS_ROUND_SOURCE,
            effect: Effect::Mult(GAUNTLET_BOSS_ROUND_HORDE_FACTOR),
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
    mut roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    let eligible_duos = duo_pool(duos, held, run);
    let guaranteed = (!eligible_duos.is_empty()).then(|| {
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
    let rest = BOON_OFFER_SIZE - usize::from(guaranteed.is_some());
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

/// Seconds to wait for an asked-for Boss before asking again (there was no window to come in by).
const BOSS_ASK_AGAIN_SECS: f32 = 2.0;

/// The Boss of the Boss Round under way, and the phase marks it has yet to cross.
#[derive(Component, Debug)]
pub struct Boss {
    pub kind: BossKind,
    marks_left: Vec<f32>,
    last_share: f32,
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
    if let Some(kind) = spawned_boss(round.number) {
        plan.kind = Some(kind);
        plan.warning = Some(GAUNTLET_BOSS_WARNING_SECS);
        cues.write(PlayCue::new(Cue::BossWarning, None));
    }
}

/// Once the warning is over, the Boss is asked for: the Bloater, through a window in an Active
/// Zone, holding the Round open. Asked again if it doesn't come.
pub fn spawn_boss(
    mut plan: ResMut<BossPlan>,
    bosses: Query<(), With<Boss>>,
    round: Res<Round>,
    mut spawns: MessageWriter<SpawnZombie>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    if let Some(left) = plan.warning.as_mut() {
        *left -= dt;
        if *left > 0.0 {
            return;
        }
        plan.warning = None;
        plan.asked = Some(BOSS_ASK_AGAIN_SECS);
    }
    let Some(waited) = plan.asked.as_mut() else {
        return;
    };
    if !bosses.is_empty() {
        plan.asked = None;
        return;
    }
    *waited += dt;
    if *waited < BOSS_ASK_AGAIN_SECS || round.phase != RoundPhase::Active {
        return;
    }
    *waited = 0.0;
    spawns.write(SpawnZombie {
        kind: RequestedKind::Napalm,
        health_factor: boss_health_factor(plan.round),
        scale: GAUNTLET_BLOATER_SCALE,
        at: SpawnAt::BarrierInActiveZone,
        holds_round: true,
        profile: SpawnProfile {
            detonates: false,
            trail: Some(Trail { every_secs: GAUNTLET_BLOATER_POOL_EVERY_SECS, pool_secs: GAUNTLET_BLOATER_POOL_SECS }),
        },
        heavyweight: Some(BOSS_SHARE),
    });
}

/// The Heavyweight the Boss request asked for is the Boss: it's named, and its phases tracked.
pub fn tag_boss(
    mut commands: Commands,
    new: Query<Entity, (Added<Requested>, With<Heavyweight>, Without<Boss>)>,
    mut plan: ResMut<BossPlan>,
) {
    for entity in &new {
        let Some(kind) = plan.kind else {
            continue;
        };
        plan.asked = None;
        commands.entity(entity).insert(Boss { kind, marks_left: kind.phase_marks().to_vec(), last_share: 1.0 });
    }
}

/// The Bloater bursts as its health crosses each phase mark.
pub fn bloater_bursts(
    mut commands: Commands,
    mut bosses: Query<(&mut Boss, &Zombie, &Transform), Without<Dying>>,
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
            commands.spawn((InRun, Burst { centre: feet_of(transform), age: 0.0, hit: Vec::new() }));
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
        let ring = Isometry3d::new(centre + Vec3::Y * 0.1, Quat::from_rotation_x(FRAC_PI_2));
        gizmos.circle(ring, radius, Color::srgb(1.0, 0.45, 0.1));
        gizmos.circle(ring, radius * 0.97, Color::srgb(1.0, 0.8, 0.3));
        if burst.age >= GAUNTLET_BLOATER_BURST_SECS {
            commands.entity(entity).despawn();
        }
    }
}

/// Where a standing Zombie's feet are, whatever its scale.
fn feet_of(transform: &Transform) -> Vec3 {
    transform.translation - Vec3::Y * transform.scale.y * (ZOMBIE_RADIUS + ZOMBIE_CAPSULE_LENGTH / 2.0)
}

/// A Boss's death pays every Player its Points and drops a Max Ammo where it fell.
pub fn boss_deaths(
    deaths: Query<&Transform, (With<Boss>, Added<Dying>)>,
    round: Res<Round>,
    players: Query<Entity, With<Player>>,
    mut bonus: MessageWriter<BonusPoints>,
    mut drops: MessageWriter<DropPowerUp>,
) {
    for transform in &deaths {
        for player in &players {
            bonus.write(BonusPoints { player, amount: boss_points(round.number) });
        }
        let feet = feet_of(transform);
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
pub fn update_boss_hud(
    plan: Res<BossPlan>,
    boss: Query<(&Boss, &Zombie), Without<Dying>>,
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
    let Some((boss, zombie)) = boss.iter().next() else {
        bar.set_if_neq(Visibility::Hidden);
        return;
    };
    bar.set_if_neq(Visibility::Inherited);
    if name.0 != boss.kind.name() {
        name.0 = boss.kind.name().to_string();
    }
    let share = (zombie.health() / zombie.max_health()).clamp(0.0, 1.0);
    fill.width = percent(share * 100.0);
    let phase_marks = boss.kind.phase_marks();
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
    fn the_bloater_stands_in_for_every_boss_until_the_others_are_built() {
        for round in [10, 20, 30, 40, 50, 60] {
            assert_eq!(spawned_boss(round), Some(BossKind::Bloater), "round {round}");
        }
        assert_eq!(spawned_boss(15), None);
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
