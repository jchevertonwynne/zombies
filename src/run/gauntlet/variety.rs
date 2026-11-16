//! Round variety: most Intermissions end in a Round Choice between a safe next Round and one with
//! a Twist, and either may carry an optional Challenge. Surviving a Twist (clearing its Round) or
//! meeting a Challenge raises the Rarity of the next Offer; both also give an extra pick; a met
//! Challenge guarantees one card from its Family.
//!
//! This module holds the pure rules (what's eligible, the draws, each Challenge's breach rule, the
//! rewards and the Boosted draw) and the Run's `RoundVariety`. The systems that run them and the
//! Round Choice screen are below; the Twists reach Classic only through stat entries (ADR 0002),
//! the Round-start cue through `RoundStartCue`, and the Fog through the camera's own fog.
//!
//! Round Choices are Solo only for now: a Co-op client can't see a Twist or a Challenge (issue
//! #161). Swarm's Alive Cap fits the Co-op snapshot (`run::gauntlet::snapshot_tests`).

use bevy::ecs::system::SystemParam;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::arc::{RunArc, act_of, is_boss_round};
use super::boon::{BOONS, BoonDef, Family, HeldBoons, RunFacts, draw, offer_pool};
use super::{Offer, PendingOffer};
use crate::audio::cues::Cue;
use crate::audio::{PlayCue, RoundStartCue};
use crate::economy::points::Points;
use crate::enemies::zombie::{Attack, ZombieKilled};
use crate::maps::zone::{PlayerZone, ZoneId};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::players::player::{LocalPlayer, PlayerCamera};
use crate::run::round::{ReservedRounds, Round, RoundEnded, RoundPhase, RoundStarted};
use crate::state::{InRun, RunType};
use crate::storage::records::{RecordExtra, RecordExtras};
use crate::storage::settings::Settings;
use crate::tuning::*;
use crate::ui::hud::{full_screen, text};
use crate::weapons::weapon::Reloaded;

// ---------------------------------------------------------------------------------------------
// The Twists and Challenges
// ---------------------------------------------------------------------------------------------

/// A Round's twist: one stat changed for the whole Round, always for a reward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Twist {
    Fog,
    Stampede,
    Swarm,
    LowGravity,
    Drought,
}

impl Twist {
    pub const ALL: [Twist; 5] = [Twist::Fog, Twist::Stampede, Twist::Swarm, Twist::LowGravity, Twist::Drought];

    pub fn name(self) -> &'static str {
        match self {
            Twist::Fog => "Fog",
            Twist::Stampede => "Stampede",
            Twist::Swarm => "Swarm",
            Twist::LowGravity => "Low Gravity",
            Twist::Drought => "Drought",
        }
    }

    /// What it does, as the Round Choice's panel says it.
    pub fn blurb(self) -> &'static str {
        match self {
            Twist::Fog => "Thick fog closes in",
            Twist::Stampede => "Every Zombie sprints",
            Twist::Swarm => "A bigger Horde, more alive at once",
            Twist::LowGravity => "You fall and jump as on the Moon",
            Twist::Drought => "No Power-ups drop from kills",
        }
    }
}

/// The Source of every Twist's stat entries. A Twist is one source, so it multiplies with Heat
/// Conditions and Pact Drawbacks.
pub const TWIST_SOURCE: Source = Source("Twist");

/// Whether a stat entry is a Twist's.
pub fn is_twist_source(source: Source) -> bool {
    source == TWIST_SOURCE
}

/// `twist`'s stat entries: while its Round is `active`, or (only the Horde's size, which is set
/// as the Round begins) in the Intermission before it.
pub fn twist_entries(twist: Twist, active: bool) -> Vec<Modifier> {
    let entry = |stat, effect| Modifier { stat, source: TWIST_SOURCE, effect };
    match twist {
        Twist::Fog => Vec::new(),
        Twist::Stampede if active => vec![entry(Stat::SprinterChance, Effect::Flat(1.0))],
        Twist::Swarm => {
            let mut entries = vec![entry(Stat::HordeSize, Effect::Mult(GAUNTLET_SWARM_HORDE_FACTOR))];
            if active {
                entries.push(entry(Stat::AliveCap, Effect::Flat(GAUNTLET_SWARM_ALIVE_CAP_BONUS as f32)));
            }
            entries
        }
        Twist::LowGravity if active => vec![entry(Stat::PlayerGravity, Effect::Mult(GAUNTLET_LOW_GRAVITY_FACTOR))],
        Twist::Drought if active => vec![entry(Stat::PowerUpDropChance, Effect::Mult(0.0))],
        _ => Vec::new(),
    }
}

/// An optional goal of a Round that costs nothing to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeKind {
    ColdSteel,
    CleanShots,
    FullMag,
    HoldTheLine,
    TightFist,
    Blitz,
}

impl ChallengeKind {
    pub const ALL: [ChallengeKind; 6] = [
        ChallengeKind::ColdSteel,
        ChallengeKind::CleanShots,
        ChallengeKind::FullMag,
        ChallengeKind::HoldTheLine,
        ChallengeKind::TightFist,
        ChallengeKind::Blitz,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ChallengeKind::ColdSteel => "Cold Steel",
            ChallengeKind::CleanShots => "Clean Shots",
            ChallengeKind::FullMag => "Full Mag",
            ChallengeKind::HoldTheLine => "Hold the Line",
            ChallengeKind::TightFist => "Tight Fist",
            ChallengeKind::Blitz => "Blitz",
        }
    }

    /// The goal in a few words, for the HUD line.
    pub fn goal(self) -> &'static str {
        match self {
            ChallengeKind::ColdSteel => "Knife kills only",
            ChallengeKind::CleanShots => "Headshot kills only",
            ChallengeKind::FullMag => "Never reload",
            ChallengeKind::HoldTheLine => "Hold your Zone",
            ChallengeKind::TightFist => "Spend no Points",
            ChallengeKind::Blitz => "Clear it fast",
        }
    }

    /// The Family whose card a met Challenge guarantees on the next Offer.
    pub fn family(self) -> Family {
        match self {
            ChallengeKind::ColdSteel => Family::Blade,
            ChallengeKind::CleanShots => Family::Lead,
            ChallengeKind::FullMag => Family::Spark,
            ChallengeKind::HoldTheLine => Family::Grit,
            ChallengeKind::TightFist => Family::Gold,
            ChallengeKind::Blitz => Family::Ember,
        }
    }
}

/// Whether a kill by `attack` (a headshot or not) breaks `kind`. Only Player kills, which are
/// `Attack::Bullet` (Weapons and Lethals) and `Attack::Knife`, count toward a Challenge or break
/// it: every other kill is neutral.
pub fn kill_breaks(kind: ChallengeKind, attack: Attack, headshot: bool) -> bool {
    match (kind, attack) {
        (ChallengeKind::ColdSteel, Attack::Bullet) => true,
        (ChallengeKind::CleanShots, Attack::Knife) => true,
        (ChallengeKind::CleanShots, Attack::Bullet) => !headshot,
        _ => false,
    }
}

/// Whether a kill by `attack` counts toward `kind`'s progress against the Horde.
pub fn kill_counts(kind: ChallengeKind, attack: Attack, headshot: bool) -> bool {
    match (kind, attack) {
        (_, Attack::Bullet | Attack::Knife) if kill_breaks(kind, attack, headshot) => false,
        (ChallengeKind::ColdSteel, attack) => attack == Attack::Knife,
        (ChallengeKind::CleanShots, attack) => attack == Attack::Bullet && headshot,
        (_, attack) => matches!(attack, Attack::Bullet | Attack::Knife),
    }
}

/// Blitz's allowance, in seconds from the Round's first spawn, for a Horde of `horde`.
pub fn blitz_limit_secs(horde: u32) -> f32 {
    (GAUNTLET_BLITZ_SECS_PER_ZOMBIE * horde as f32).max(GAUNTLET_BLITZ_MIN_SECS)
}

/// Whether a Challenge is still on, broken, or met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Active,
    Broken,
    Met,
}

/// A Challenge under way in a Round.
#[derive(Debug, Clone, PartialEq)]
pub struct ChallengeRun {
    pub kind: ChallengeKind,
    pub standing: Standing,
    /// Kills counted toward it so far.
    pub kills: u32,
    /// The Zone the Player held at the Round's start, for Hold the Line.
    zone: Option<ZoneId>,
    /// Blitz's seconds since the Round's first spawn.
    clock: Option<f32>,
}

impl ChallengeRun {
    pub fn new(kind: ChallengeKind) -> Self {
        Self { kind, standing: Standing::Active, kills: 0, zone: None, clock: None }
    }

    /// Breaks it, if it's still on. Returns whether it just broke.
    fn breach(&mut self) -> bool {
        let was_on = self.standing == Standing::Active;
        if was_on {
            self.standing = Standing::Broken;
        }
        was_on
    }

    /// A Player kill, which counts toward it or breaks it. Returns whether it just broke.
    pub fn killed(&mut self, attack: Attack, headshot: bool) -> bool {
        if kill_breaks(self.kind, attack, headshot) {
            return self.breach();
        }
        if self.standing == Standing::Active && kill_counts(self.kind, attack, headshot) {
            self.kills = self.kills.saturating_add(1);
        }
        false
    }

    /// A Reload finished, the automatic one included. Returns whether it just broke.
    pub fn reloaded(&mut self) -> bool {
        self.kind == ChallengeKind::FullMag && self.breach()
    }

    /// Points spent on something with a price. Returns whether it just broke.
    pub fn spent(&mut self, price: u64) -> bool {
        self.kind == ChallengeKind::TightFist && price > 0 && self.breach()
    }

    /// The Player's Zone now. Returns whether it just broke.
    pub fn in_zone(&mut self, zone: ZoneId) -> bool {
        if self.kind != ChallengeKind::HoldTheLine {
            return false;
        }
        match self.zone {
            None => {
                self.zone = Some(zone);
                false
            }
            Some(held) => held != zone && self.breach(),
        }
    }

    /// `dt` seconds pass in a Round with `horde` Zombies, whose first has spawned or not. Returns
    /// whether it just broke.
    pub fn tick(&mut self, dt: f32, horde: u32, spawned: bool) -> bool {
        if self.kind != ChallengeKind::Blitz || !spawned {
            return false;
        }
        let clock = self.clock.get_or_insert(0.0);
        *clock += dt;
        *clock > blitz_limit_secs(horde) && self.breach()
    }

    /// The seconds Blitz has left, once it's started.
    pub fn time_left(&self, horde: u32) -> Option<f32> {
        (self.kind == ChallengeKind::Blitz).then(|| (blitz_limit_secs(horde) - self.clock.unwrap_or(0.0)).max(0.0))
    }

    /// The Round was cleared: an unbroken Challenge is met.
    pub fn cleared(&mut self) {
        if self.standing == Standing::Active {
            self.standing = Standing::Met;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The Round Choice
// ---------------------------------------------------------------------------------------------

/// Whether the Round `next` is decided by a Round Choice: every Round but the first two, Boss
/// Rounds and Hellhound Rounds (`hellhound`, as Classic's scheduler places them). Pact Rounds are
/// eligible.
pub fn round_choice_applies(next: u32, hellhound: bool) -> bool {
    next >= GAUNTLET_ROUND_CHOICE_FIRST_ROUND && !is_boss_round(next) && !hellhound
}

/// Whether a Run of this type has Round Choices. Co-op doesn't yet: a client can't see a Twist or
/// a Challenge (issue #161).
pub fn has_round_choices(run_type: RunType) -> bool {
    !run_type.is_coop()
}

/// One side of a Round Choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundOption {
    pub twist: Option<Twist>,
    pub challenge: Option<ChallengeKind>,
}

/// The Twists and Challenges used in the Act so far, which no Round Choice repeats.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActHistory {
    pub act: u32,
    pub twists: Vec<Twist>,
    pub challenges: Vec<ChallengeKind>,
}

impl ActHistory {
    /// This history for `act`: a new Act starts it over.
    pub fn for_act(&mut self, act: u32) -> &Self {
        if self.act != act {
            *self = Self { act, ..Self::default() };
        }
        self
    }

    /// The Round played was `option`.
    pub fn used(&mut self, option: RoundOption) {
        self.twists.extend(option.twist);
        self.challenges.extend(option.challenge);
    }
}

/// One of `all` not in `used` and not `skip`, uniformly. If all of them are used, the pool starts
/// over: the Act has more Rounds than Twists and Challenges.
fn pick<T: Copy + PartialEq>(all: &[T], used: &[T], skip: Option<T>, roll: &mut impl FnMut(f64) -> f64) -> T {
    let fresh: Vec<T> = all.iter().copied().filter(|x| !used.contains(x) && Some(*x) != skip).collect();
    let pool = if fresh.is_empty() { all.iter().copied().filter(|x| Some(*x) != skip).collect() } else { fresh };
    pool[(roll(pool.len() as f64) as usize).min(pool.len() - 1)]
}

/// A Round Choice's two options for an Act with `history`: the safe one (a Challenge
/// `GAUNTLET_SAFE_CHALLENGE_CHANCE` of the time, never a Twist) and the Twist one (always a Twist,
/// a Challenge `GAUNTLET_TWIST_CHALLENGE_CHANCE` of the time). No Twist or Challenge repeats
/// within the Act, and the two never share a Challenge.
pub fn draw_round_choice(history: &ActHistory, mut roll: impl FnMut(f64) -> f64) -> [RoundOption; 2] {
    let safe = (roll(1.0) < GAUNTLET_SAFE_CHALLENGE_CHANCE)
        .then(|| pick(&ChallengeKind::ALL, &history.challenges, None, &mut roll));
    let twist = pick(&Twist::ALL, &history.twists, None, &mut roll);
    let twisted =
        (roll(1.0) < GAUNTLET_TWIST_CHALLENGE_CHANCE).then(|| pick(&ChallengeKind::ALL, &history.challenges, safe, &mut roll));
    [RoundOption { twist: None, challenge: safe }, RoundOption { twist: Some(twist), challenge: twisted }]
}

/// What a Round's variety pays on the next Offer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Boost {
    /// Every slot is a Rarity higher (a Legendary stays Legendary).
    pub rarity_bump: bool,
    /// Picks from the Offer beyond the first.
    pub extra_picks: u32,
    /// Families a card of is guaranteed.
    pub families: Vec<Family>,
}

impl Boost {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// Takes `other` on top of this one, for a reward the Pact Offer carried over.
    pub fn merge(&mut self, other: Boost) {
        self.rarity_bump |= other.rarity_bump;
        self.extra_picks += other.extra_picks;
        for family in other.families {
            if !self.families.contains(&family) {
                self.families.push(family);
            }
        }
    }
}

/// The reward for a cleared Round: `survived_twist` if it had a Twist, and the Challenge it met,
/// if any. Either gives +1 Rarity; both also give `GAUNTLET_BOTH_EXTRA_PICKS` extra picks; a met
/// Challenge guarantees a card of its Family.
pub fn reward(survived_twist: bool, met: Option<ChallengeKind>) -> Boost {
    Boost {
        rarity_bump: survived_twist || met.is_some(),
        extra_picks: if survived_twist && met.is_some() { GAUNTLET_BOTH_EXTRA_PICKS } else { 0 },
        families: met.map(ChallengeKind::family).into_iter().collect(),
    }
}

/// What a Round Choice panel promises for `option`.
pub fn reward_line(option: RoundOption) -> &'static str {
    match (option.twist.is_some(), option.challenge.is_some()) {
        (true, true) => "+1 Rarity next Offer, and an extra pick for both",
        (false, false) => "No reward",
        _ => "+1 Rarity next Offer",
    }
}

/// The HUD's Challenge line: its goal and kills against the Horde, with Blitz's seconds left.
pub fn challenge_line(run: &ChallengeRun, horde: u32) -> String {
    let kills = run.kills.min(horde.max(run.kills));
    match run.time_left(horde) {
        Some(left) if run.standing == Standing::Active => {
            format!("{} ({:.0}s): {}/{}", run.kind.goal(), left.ceil(), kills, horde)
        }
        _ => format!("{}: {}/{}", run.kind.goal(), kills, horde),
    }
}

/// The word that shows how a Challenge stands once it's decided.
pub fn standing_word(standing: Standing) -> &'static str {
    match standing {
        Standing::Active => "",
        Standing::Broken => "FAILED",
        Standing::Met => "MET",
    }
}

// ---------------------------------------------------------------------------------------------
// Boosted draws
// ---------------------------------------------------------------------------------------------

/// A Rarity higher; a Legendary stays Legendary.
fn bumped(rarity: super::boon::Rarity) -> super::boon::Rarity {
    use super::boon::Rarity;
    match rarity {
        Rarity::Common => Rarity::Rare,
        Rarity::Rare | Rarity::Legendary => Rarity::Legendary,
    }
}

/// The index of the entry a `roll` in `[0, total)` lands on, by weight.
fn weighted_index(weights: impl Iterator<Item = f64> + Clone, roll: f64) -> Option<usize> {
    let mut left = roll;
    let last = weights.clone().count().checked_sub(1)?;
    Some(
        weights
            .enumerate()
            .find(|(_, weight)| {
                left -= weight;
                left < 0.0
            })
            .map_or(last, |(index, _)| index),
    )
}

/// A Boon Offer's cards, drawn as `draw_offer_sized` does, then Boosted: with `rarity_bump` each
/// slot's draw is swapped for a Boon one Rarity up, chosen from those that remain by the usual
/// weights (if there's none, the original stays), and each Family in `boost.families` is
/// guaranteed a card.
pub fn draw_boosted(
    table: &'static [BoonDef],
    held: &HeldBoons,
    run: &RunFacts,
    size: usize,
    boost: &Boost,
    mut roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    if !boost.rarity_bump {
        let cards = draw(offer_pool(table, held, run), size, &mut roll);
        return guarantee_families(cards, boost, false, table, held, run, roll);
    }
    let mut pool = offer_pool(table, held, run);
    let mut cards = Vec::new();
    while cards.len() < size && !pool.is_empty() {
        let total: f64 = pool.iter().map(|(_, weight)| weight).sum();
        if total <= 0.0 {
            break;
        }
        let Some(mut index) = weighted_index(pool.iter().map(|(_, weight)| *weight), roll(total)) else {
            break;
        };
        let up = bumped(pool[index].0.rarity);
        if up != pool[index].0.rarity {
            let same: Vec<usize> = (0..pool.len()).filter(|&i| pool[i].0.rarity == up).collect();
            let weights = same.iter().map(|&i| pool[i].1);
            let sum: f64 = weights.clone().sum();
            if sum > 0.0
                && let Some(pos) = weighted_index(weights, roll(sum))
            {
                index = same[pos];
            }
        }
        cards.push(pool.remove(index).0);
    }
    guarantee_families(cards, boost, false, table, held, run, roll)
}

/// `cards` with a card of each Family in `boost.families`, swapping one slot where there is none:
/// a slot that isn't a Duo and doesn't already hold a guaranteed Family's card, for a Boon of the
/// Family at the slot's Rarity (any Rarity if there is none, unless `boss`: a Boss Offer stays all
/// Legendary). Left as it was where nothing fits.
pub fn guarantee_families(
    mut cards: Vec<&'static BoonDef>,
    boost: &Boost,
    boss: bool,
    table: &'static [BoonDef],
    held: &HeldBoons,
    run: &RunFacts,
    mut roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    let in_family = |card: &BoonDef, family: Family| match card.duo {
        Some(duo) => duo.families.contains(&family),
        None => card.family == family,
    };
    let mut guaranteed: Vec<usize> = Vec::new();
    for &family in &boost.families {
        if let Some(index) = cards.iter().position(|card| in_family(card, family)) {
            guaranteed.push(index);
            continue;
        }
        let slots: Vec<usize> =
            (0..cards.len()).filter(|i| cards[*i].duo.is_none() && !guaranteed.contains(i)).collect();
        if slots.is_empty() {
            continue;
        }
        let slot = slots[(roll(slots.len() as f64) as usize).min(slots.len() - 1)];
        let rarity = cards[slot].rarity;
        let candidates = |same_rarity: bool| -> Vec<(&'static BoonDef, f64)> {
            offer_pool(table, held, run)
                .into_iter()
                .filter(|(boon, _)| {
                    boon.duo.is_none()
                        && boon.family == family
                        && (!same_rarity || boon.rarity == rarity)
                        && !cards.iter().any(|card| card.id == boon.id)
                })
                .collect()
        };
        let mut pool = candidates(true);
        if pool.is_empty() && !boss {
            pool = candidates(false);
        }
        if let Some(&card) = draw(pool, 1, &mut roll).first() {
            cards[slot] = card;
            guaranteed.push(slot);
        }
    }
    cards
}

/// An Offer's cards under the usual rules, Boosted: a Boss Round's all-Legendary Offer with its
/// guaranteed Duo where it can be, an ordinary one otherwise.
pub fn draw_cards_boosted(
    round: u32,
    held: &HeldBoons,
    run: &RunFacts,
    boost: &Boost,
    mut roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    use super::arc::draw_boss_offer;
    use super::duos::DUOS;
    let boss = if is_boss_round(round) { draw_boss_offer(BOONS, DUOS, held, run, &mut roll) } else { Vec::new() };
    if boss.is_empty() {
        draw_boosted(BOONS, held, run, BOON_OFFER_SIZE, boost, roll)
    } else {
        guarantee_families(boss, boost, true, BOONS, held, run, roll)
    }
}

// ---------------------------------------------------------------------------------------------
// The Run's variety
// ---------------------------------------------------------------------------------------------

/// A Round Choice waiting for its pick.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingChoice {
    /// The Round it decides.
    pub round: u32,
    pub options: [RoundOption; 2],
}

/// The Round picked, and its Challenge under way.
#[derive(Debug, Clone, PartialEq)]
pub struct Chosen {
    pub round: u32,
    pub option: RoundOption,
    pub challenge: Option<ChallengeRun>,
    /// The Round has begun and not yet been cleared, so events count.
    live: bool,
}

/// The Run's Round variety: the Round Choice waiting, the Round chosen, the Act's history, the
/// reward the next Offer will carry, and the counts for the Run Record.
#[derive(Resource, Debug, Default)]
pub struct RoundVariety {
    pub pending: Option<PendingChoice>,
    pub chosen: Option<Chosen>,
    pub history: ActHistory,
    /// The reward earned and not yet on an Offer: a Pact Offer carries it to the next Boon or
    /// Boss Offer.
    pub reward: Boost,
    pub challenges_met: u32,
    pub twists_survived: u32,
    /// Whether the pending Round Choice can be picked: nothing else of the Intermission is open.
    pub lit: bool,
    /// When the pending Round Choice was lit, in real seconds, for the input lockout.
    lit_at: f32,
    /// Seconds left of the Twist banner and of a broken Challenge's flash.
    banner_left: f32,
    flash_left: f32,
}

impl RoundVariety {
    /// The Twist of the Round, if one is chosen for `round`.
    fn twist_for(&self, round: u32) -> Option<Twist> {
        self.chosen.as_ref().filter(|chosen| chosen.round == round).and_then(|chosen| chosen.option.twist)
    }

    /// The Twists' stat entries while Round `round` is in `phase`: from the Intermission before
    /// its Round to its end.
    pub fn entries(&self, round: u32, phase: &RoundPhase) -> Vec<Modifier> {
        let (next, active) = match phase {
            RoundPhase::Active => (round, true),
            RoundPhase::Intermission(_) => (round.saturating_add(1), false),
        };
        self.twist_for(next).map_or_else(Vec::new, |twist| twist_entries(twist, active))
    }

    /// The Round `round` is cleared: its Twist is survived and its Challenge met or not, paying
    /// the reward onto the next Offer. Returns whether a Challenge was met.
    pub fn cleared(&mut self, round: u32) -> bool {
        let Some(chosen) = self.chosen.as_mut().filter(|chosen| chosen.round == round && chosen.live) else {
            return false;
        };
        chosen.live = false;
        let mut met = None;
        if let Some(run) = chosen.challenge.as_mut() {
            run.cleared();
            if run.standing == Standing::Met {
                met = Some(run.kind);
            }
        }
        let survived = chosen.option.twist.is_some();
        self.challenges_met += u32::from(met.is_some());
        self.twists_survived += u32::from(survived);
        self.reward.merge(reward(survived, met));
        met.is_some()
    }

    /// The reward onto an Offer opening now: all of it for a Boon or Boss Offer, none for a Pact
    /// Offer, which leaves it for the next.
    pub fn reward_for(&self, boons: bool) -> Boost {
        if boons { self.reward.clone() } else { Boost::default() }
    }

    /// Takes the reward `reward_for` showed, once an Offer carries it.
    pub fn take_reward(&mut self, boons: bool) -> Boost {
        if boons { std::mem::take(&mut self.reward) } else { Boost::default() }
    }
}

/// The Round Choice's pick: `0` the safe option, `1` the Twist one.
#[derive(Message, Debug, Clone, Copy)]
pub struct PickRound {
    pub option: usize,
}

/// The Round was cleared: its reward is paid, and a Round Choice for the next Round is drawn if
/// it has one.
pub fn end_round(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    (reserved, settings, run_type): (Res<ReservedRounds>, Res<Settings>, Res<RunType>),
    mut variety: ResMut<RoundVariety>,
    mut standing_in: ResMut<RoundStartCue>,
    mut cues: MessageWriter<PlayCue>,
) {
    if ended.read().count() == 0 {
        return;
    }
    standing_in.0 = None;
    if variety.cleared(round.number) {
        cues.write(PlayCue::new(Cue::ChallengeMet, None));
    }
    if !has_round_choices(*run_type) {
        return;
    }
    let next = round.number.saturating_add(1);
    if !round_choice_applies(next, round.next_is_hellhound(&reserved, settings.special_zombies.hellhounds)) {
        return;
    }
    let options = {
        let history = variety.history.for_act(act_of(next)).clone();
        draw_round_choice(&history, |total| rand::random_range(0.0..total))
    };
    variety.pending = Some(PendingChoice { round: next, options });
}

/// Works out whether the Round Choice can be picked: one is waiting, and no Offer is open or due.
pub fn light_choice(
    players: Query<(&PendingOffer, Has<Offer>)>,
    arc: Res<RunArc>,
    real: Res<Time<Real>>,
    mut variety: ResMut<RoundVariety>,
) {
    let lit = variety.pending.is_some()
        && !arc.choosing
        && players.iter().all(|(pending, open)| pending.left.is_none() && !open);
    if variety.lit != lit {
        variety.lit = lit;
        if lit {
            variety.lit_at = real.elapsed_secs();
        }
    }
}

/// Keys 1 and 2 pick a Round Choice's option.
pub fn pick_keys(keys: Res<ButtonInput<KeyCode>>, variety: Res<RoundVariety>, mut picks: MessageWriter<PickRound>, real: Res<Time<Real>>) {
    if !variety.lit || real.elapsed_secs() - variety.lit_at < BOON_OFFER_INPUT_LOCKOUT_SECS {
        return;
    }
    for (index, key) in [KeyCode::Digit1, KeyCode::Digit2].into_iter().enumerate() {
        if keys.just_pressed(key) {
            picks.write(PickRound { option: index });
        }
    }
}

/// A click on a Round Choice panel picks it, once the input lockout is over.
pub fn click_panel(
    activate: On<Activate>,
    panels: Query<&RoundPanel>,
    variety: Res<RoundVariety>,
    real: Res<Time<Real>>,
    mut picks: MessageWriter<PickRound>,
) {
    let Ok(&RoundPanel(option)) = panels.get(activate.entity) else {
        return;
    };
    if variety.lit && real.elapsed_secs() - variety.lit_at >= BOON_OFFER_INPUT_LOCKOUT_SECS {
        picks.write(PickRound { option });
    }
}

/// A pick sets the next Round's variety, files it in the Act's history, and closes the choice.
pub fn apply_pick(
    mut picks: MessageReader<PickRound>,
    mut variety: ResMut<RoundVariety>,
    mut standing_in: ResMut<RoundStartCue>,
    mut cues: MessageWriter<PlayCue>,
) {
    for &PickRound { option } in picks.read() {
        if !variety.lit {
            continue;
        }
        let Some(pending) = variety.pending.as_ref() else {
            continue;
        };
        let Some(&picked) = pending.options.get(option) else {
            continue;
        };
        let round = pending.round;
        variety.pending = None;
        variety.lit = false;
        variety.history.for_act(act_of(round));
        variety.history.used(picked);
        variety.chosen = Some(Chosen { round, option: picked, challenge: picked.challenge.map(ChallengeRun::new), live: false });
        standing_in.0 = picked.twist.map(|_| Cue::TwistRoundStart);
        cues.write(PlayCue::new(Cue::OfferPick, None));
    }
}

/// The chosen Round begins: its Challenge starts counting, and a Twist shows its banner.
pub fn begin_round(
    mut started: MessageReader<RoundStarted>,
    round: Res<Round>,
    zone: Query<&PlayerZone, With<LocalPlayer>>,
    mut variety: ResMut<RoundVariety>,
) {
    if started.read().count() == 0 {
        return;
    }
    let number = round.number;
    // A Round that wasn't chosen for has no variety, and the last one's result goes.
    if variety.chosen.as_ref().is_some_and(|chosen| chosen.round != number) {
        variety.chosen = None;
    }
    let Some(chosen) = variety.chosen.as_mut() else {
        return;
    };
    chosen.live = true;
    let twist = chosen.option.twist.is_some();
    if let (Some(run), Ok(&PlayerZone(zone))) = (chosen.challenge.as_mut(), zone.single()) {
        run.in_zone(zone);
    }
    variety.flash_left = 0.0;
    variety.banner_left = if twist { GAUNTLET_TWIST_BANNER_SECS } else { 0.0 };
}

/// Everything the live Challenge watches, as it happens.
#[derive(SystemParam)]
pub struct Watched<'w, 's> {
    kills: MessageReader<'w, 's, ZombieKilled>,
    reloads: MessageReader<'w, 's, Reloaded>,
    points: Query<'w, 's, &'static Points, With<LocalPlayer>>,
    /// The Player's `Points::spent` as last seen, so the Points actually deducted since count.
    seen_spent: Local<'s, u64>,
}

/// Feeds the live Challenge the Player's kills, Reloads, purchases, Zone and the clock, and sounds
/// the buzz when it breaks. Never on a Down: only these break one.
pub fn track_challenge(
    mut watched: Watched,
    round: Res<Round>,
    zone: Query<&PlayerZone, With<LocalPlayer>>,
    time: Res<Time>,
    mut variety: ResMut<RoundVariety>,
    mut cues: MessageWriter<PlayCue>,
) {
    let mut broke = false;
    let live = variety.chosen.as_ref().is_some_and(|chosen| chosen.live && chosen.round == round.number);
    let kills: Vec<_> = watched.kills.read().copied().collect();
    let reloads = watched.reloads.read().count();
    // What was really deducted, whatever was bought (a Door, a Repack or Pack-a-Punch price lives on
    // no `Buyable`) and wherever the Points went.
    let total = watched.points.single().map_or(*watched.seen_spent, |points| points.spent);
    let spent = total.saturating_sub(*watched.seen_spent);
    *watched.seen_spent = total;
    if !live {
        return;
    }
    let Some(run) = variety.chosen.as_mut().and_then(|chosen| chosen.challenge.as_mut()) else {
        return;
    };
    for kill in kills.iter().filter(|kill| kill.player.is_some()) {
        broke |= run.killed(kill.attack, kill.headshot);
    }
    for _ in 0..reloads {
        broke |= run.reloaded();
    }
    broke |= run.spent(spent);
    if let Ok(&PlayerZone(zone)) = zone.single() {
        broke |= run.in_zone(zone);
    }
    broke |= run.tick(time.delta_secs(), round.horde(), round.horde_spawned() > 0);
    if broke {
        variety.flash_left = GAUNTLET_CHALLENGE_BREACH_FLASH_SECS;
        cues.write(PlayCue::new(Cue::ChallengeBreach, None));
    }
}

/// The Run Record's two counts, once there's something to count.
pub fn record_counts(variety: Res<RoundVariety>, mut extras: ResMut<RecordExtras>) {
    for (id, count) in [("challenges_met", variety.challenges_met), ("twists_survived", variety.twists_survived)] {
        if count == 0 {
            continue;
        }
        match extras.0.iter().position(|extra| extra.id == id) {
            Some(at) if extras.0[at].count == count => {}
            Some(at) => extras.0[at].count = count,
            None => extras.0.push(RecordExtra { id: id.to_string(), count }),
        }
    }
}

/// Fog: while the Round with the Fog Twist is on, the camera sees no farther than
/// `GAUNTLET_FOG_DISTANCE`. Runs after everything that moves the fog, so it only ever pulls it in.
pub fn draw_fog(
    variety: Res<RoundVariety>,
    round: Option<Res<Round>>,
    mut fogs: Query<&mut DistanceFog, With<PlayerCamera>>,
) {
    let foggy = round.is_some_and(|round| {
        round.phase == RoundPhase::Active && variety.twist_for(round.number) == Some(Twist::Fog)
    });
    if !foggy {
        return;
    }
    for mut fog in &mut fogs {
        let FogFalloff::Linear { start, end } = fog.falloff else {
            continue;
        };
        let new_end = end.min(GAUNTLET_FOG_DISTANCE);
        let new_start = start.min(new_end * 0.2);
        if (new_start, new_end) != (start, end) {
            fog.falloff = FogFalloff::Linear { start: new_start, end: new_end };
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The Round Choice screen and the HUD
// ---------------------------------------------------------------------------------------------

const GREY: Color = Color::srgb(0.65, 0.65, 0.65);
const PANEL_IDLE: Color = Color::srgba(0.07, 0.07, 0.09, 0.95);
const PANEL_HOVER: Color = Color::srgba(0.22, 0.22, 0.26, 0.97);
const PANEL_DIM: Color = Color::srgba(0.07, 0.07, 0.09, 0.45);
const FLASH_RED: Color = Color::srgb(0.95, 0.2, 0.2);

/// The Twist colour.
pub fn twist_colour() -> Color {
    let [r, g, b] = GAUNTLET_TWIST_COLOUR;
    Color::srgb_u8(r, g, b)
}

/// The Round Choice's root, over the Offer's cards.
#[derive(Component)]
pub struct RoundChoiceScreen {
    /// The Round it decides, so a new one rebuilds it.
    round: u32,
}

/// A Round Choice panel and its option.
#[derive(Component)]
pub struct RoundPanel(pub usize);

/// Text on a panel, dimmed with it.
#[derive(Component)]
pub struct PanelText(Color);

/// The Round Choice's heading, which only shows once the choice is lit.
#[derive(Component)]
pub struct ChoiceHeading;

/// The Challenge line and the Twist tag on the HUD, and the Twist banner.
#[derive(Component)]
pub struct ChallengeBadge;
#[derive(Component)]
pub struct ChallengeText;
#[derive(Component)]
pub struct TwistTag;
#[derive(Component)]
pub struct TwistBanner;

/// Spawns the Challenge line (with its Family badge), the Twist tag and the Twist banner.
pub fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                left: px(32.0),
                bottom: px(164.0),
                flex_direction: FlexDirection::ColumnReverse,
                row_gap: px(2.0),
                ..default()
            },
        ))
        .with_children(|column| {
            column.spawn((TwistTag, text("", 16.0, twist_colour()), Visibility::Hidden));
            column.spawn(Node { column_gap: px(8.0), ..default() }).with_children(|row| {
                row.spawn((ChallengeBadge, text("", 16.0, Color::WHITE), Visibility::Hidden));
                row.spawn((ChallengeText, text("", 20.0, Color::WHITE), Visibility::Hidden));
            });
        });
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(24.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((TwistBanner, text("", 40.0, twist_colour()), Visibility::Hidden));
}

/// The HUD's Challenge line, Twist tag and banner follow the Round's variety.
pub fn update_hud(
    mut variety: ResMut<RoundVariety>,
    round: Option<Res<Round>>,
    real: Res<Time<Real>>,
    mut badge: Single<(&mut Text, &mut TextColor, &mut Visibility), (With<ChallengeBadge>, Without<ChallengeText>, Without<TwistTag>, Without<TwistBanner>)>,
    mut line: Single<(&mut Text, &mut TextColor, &mut Visibility), (With<ChallengeText>, Without<ChallengeBadge>, Without<TwistTag>, Without<TwistBanner>)>,
    mut tag: Single<(&mut Text, &mut Visibility), (With<TwistTag>, Without<ChallengeBadge>, Without<ChallengeText>, Without<TwistBanner>)>,
    mut banner: Single<(&mut Text, &mut Visibility), (With<TwistBanner>, Without<ChallengeBadge>, Without<ChallengeText>, Without<TwistTag>)>,
) {
    let dt = real.delta_secs();
    if variety.banner_left > 0.0 {
        variety.banner_left = (variety.banner_left - dt).max(0.0);
    }
    if variety.flash_left > 0.0 {
        variety.flash_left = (variety.flash_left - dt).max(0.0);
    }
    let horde = round.as_ref().map_or(0, |round| round.horde());
    let chosen = variety.chosen.as_ref();
    // The Round's own variety shows from its start, and its result stays through the Intermission.
    let shown = chosen.filter(|chosen| round.as_ref().is_some_and(|round| round.number == chosen.round));
    let twist = shown.and_then(|chosen| chosen.option.twist);
    if let Some(twist) = twist {
        tag.0.0 = twist.name().to_string();
        tag.1.set_if_neq(Visibility::Inherited);
        banner.0.0 = format!("TWIST: {}", twist.name());
        banner.1.set_if_neq(if variety.banner_left > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
    } else {
        tag.1.set_if_neq(Visibility::Hidden);
        banner.1.set_if_neq(Visibility::Hidden);
    }
    match shown.and_then(|chosen| chosen.challenge.as_ref()) {
        Some(run) => {
            let family = run.kind.family().colour();
            let word = standing_word(run.standing);
            let mut say = challenge_line(run, horde);
            let colour = match run.standing {
                Standing::Active => Color::WHITE,
                Standing::Met => family,
                Standing::Broken if variety.flash_left > 0.0 => FLASH_RED,
                Standing::Broken => GREY,
            };
            if !word.is_empty() {
                say = format!("{word}: {say}");
            }
            badge.0.0 = run.kind.family().name().to_uppercase();
            badge.1.0 = family;
            badge.2.set_if_neq(Visibility::Inherited);
            line.0.0 = say;
            line.1.0 = colour;
            line.2.set_if_neq(Visibility::Inherited);
        }
        None => {
            badge.2.set_if_neq(Visibility::Hidden);
            line.2.set_if_neq(Visibility::Hidden);
        }
    }
}

/// The name of an option's Round: "Round N", with its Twist.
fn round_title(round: u32, option: RoundOption) -> String {
    match option.twist {
        Some(twist) => format!("Round {round}: {}", twist.name()),
        None => format!("Round {round}"),
    }
}

/// Shows the pending Round Choice as two panels along the bottom, dimmed over the Offer's cards
/// until the Boon is picked, then lit over the frozen world. Rebuilt when a new one is drawn.
pub fn show_choice(
    mut commands: Commands,
    variety: Res<RoundVariety>,
    arc: Res<RunArc>,
    screens: Query<(Entity, &RoundChoiceScreen)>,
) {
    let wanted = variety.pending.as_ref().filter(|_| !arc.choosing);
    match (wanted, screens.iter().next()) {
        (Some(pending), Some((_, screen))) if screen.round == pending.round => {}
        (None, None) => {}
        (wanted, shown) => {
            if let Some((screen, _)) = shown {
                commands.entity(screen).despawn();
            }
            if let Some(pending) = wanted {
                spawn_choice(&mut commands, pending);
            }
        }
    }
}

fn spawn_choice(commands: &mut Commands, pending: &PendingChoice) {
    commands
        .spawn((
            RoundChoiceScreen { round: pending.round },
            InRun,
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                row_gap: px(10.0),
                padding: UiRect::bottom(px(28.0)),
                ..full_screen()
            },
            BackgroundColor(Color::NONE),
            GlobalZIndex(22),
            Pickable::IGNORE,
        ))
        .with_children(|root| {
            root.spawn((ChoiceHeading, PanelText(Color::WHITE), text("CHOOSE THE NEXT ROUND  (1 or 2)", 24.0, Color::WHITE), Visibility::Hidden));
            root.spawn((Node { column_gap: px(24.0), ..default() }, Pickable::IGNORE)).with_children(|row| {
                for (index, option) in pending.options.iter().enumerate() {
                    panel(row, index, pending.round, *option);
                }
            });
        });
}

fn panel(row: &mut ChildSpawnerCommands, index: usize, round: u32, option: RoundOption) {
    let accent = if option.twist.is_some() { twist_colour() } else { GREY };
    row.spawn((
        RoundPanel(index),
        Button,
        Hovered::default(),
        Node {
            width: px(330.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            padding: UiRect::all(px(12.0)),
            border: UiRect::all(px(3.0)),
            ..default()
        },
        BorderColor::all(accent),
        BackgroundColor(PANEL_DIM),
    ))
    .with_children(|panel| {
        panel.spawn((PanelText(accent), text(round_title(round, option), 24.0, accent)));
        if let Some(twist) = option.twist {
            panel.spawn((PanelText(GREY), text(twist.blurb(), 15.0, GREY)));
        }
        match option.challenge {
            Some(challenge) => {
                let family = challenge.family();
                panel.spawn((
                    PanelText(family.colour()),
                    text(format!("{}: {} [{}]", challenge.name(), challenge.goal(), family.name().to_uppercase()), 16.0, family.colour()),
                ));
            }
            None => {
                panel.spawn((PanelText(GREY), text("No Challenge", 16.0, GREY)));
            }
        }
        panel.spawn((PanelText(Color::WHITE), text(reward_line(option), 15.0, Color::WHITE)));
    });
}

/// The panels light up when the choice can be picked, and show hover.
pub fn light_panels(
    variety: Res<RoundVariety>,
    mut panels: Query<(&Hovered, &mut BackgroundColor), With<RoundPanel>>,
    mut texts: Query<(&PanelText, &mut TextColor)>,
    mut heading: Query<&mut Visibility, With<ChoiceHeading>>,
    mut roots: Query<&mut BackgroundColor, (With<RoundChoiceScreen>, Without<RoundPanel>)>,
) {
    for (hovered, mut background) in &mut panels {
        let colour = match (variety.lit, hovered.get()) {
            (false, _) => PANEL_DIM,
            (true, true) => PANEL_HOVER,
            (true, false) => PANEL_IDLE,
        };
        background.set_if_neq(BackgroundColor(colour));
    }
    for (PanelText(colour), mut text) in &mut texts {
        text.set_if_neq(TextColor(if variety.lit { *colour } else { colour.with_alpha(0.5) }));
    }
    for mut visibility in &mut heading {
        visibility.set_if_neq(if variety.lit { Visibility::Inherited } else { Visibility::Hidden });
    }
    // Over the frozen world on its own, the choice dims it as an Offer does.
    for mut root in &mut roots {
        root.set_if_neq(BackgroundColor(if variety.lit { Color::srgba(0.0, 0.0, 0.0, 0.72) } else { Color::NONE }));
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::{Rarity, find};

    fn seeded(seed: u64) -> impl FnMut(f64) -> f64 {
        let mut n = seed;
        move |total| {
            n = n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (n >> 11) as f64 / (1u64 << 53) as f64 * total
        }
    }

    fn everything() -> RunFacts {
        RunFacts { unlock_all: true, ..default() }
    }

    fn option(twist: Option<Twist>, challenge: Option<ChallengeKind>) -> RoundOption {
        RoundOption { twist, challenge }
    }

    // --- eligibility ---

    #[test]
    fn the_first_two_rounds_are_plain_and_the_third_is_a_choice() {
        assert!(!round_choice_applies(1, false));
        assert!(!round_choice_applies(2, false));
        assert!(round_choice_applies(3, false));
    }

    #[test]
    fn boss_rounds_and_hellhound_rounds_are_plain_but_pact_rounds_are_a_choice() {
        for boss in [10, 20, 30, 40, 50] {
            assert!(!round_choice_applies(boss, false), "Round {boss}");
        }
        assert!(!round_choice_applies(7, true));
        for pact in [5, 15, 25, 35] {
            assert!(round_choice_applies(pact, false), "Round {pact}");
        }
    }

    #[test]
    fn co_op_has_no_round_choices_until_clients_can_see_them() {
        assert!(has_round_choices(RunType::Solo));
        assert!(!has_round_choices(RunType::Coop));
    }

    // --- the draw ---

    #[test]
    fn the_safe_option_never_has_a_twist_and_the_twist_option_always_does() {
        for seed in 0..200 {
            let [safe, twisted] = draw_round_choice(&ActHistory::default(), seeded(seed));
            assert_eq!(safe.twist, None);
            assert!(twisted.twist.is_some());
        }
    }

    #[test]
    fn the_challenge_chances_are_the_tuned_ones() {
        // Every roll lands the same share of the way along: under a chance gives a Challenge.
        let fixed = |share: f64| move |total: f64| total * share;
        let [safe, twisted] = draw_round_choice(&ActHistory::default(), fixed(0.45));
        assert!(safe.challenge.is_some() && twisted.challenge.is_some());
        let [safe, twisted] = draw_round_choice(&ActHistory::default(), fixed(0.55));
        assert!(safe.challenge.is_some(), "0.55 is under the safe chance of 0.6");
        assert!(twisted.challenge.is_none(), "and over the Twist chance of 0.5");
        let [safe, twisted] = draw_round_choice(&ActHistory::default(), fixed(0.65));
        assert!(safe.challenge.is_none() && twisted.challenge.is_none());
        assert_eq!((GAUNTLET_SAFE_CHALLENGE_CHANCE, GAUNTLET_TWIST_CHALLENGE_CHANCE), (0.6, 0.5));
    }

    #[test]
    fn the_two_options_never_share_a_challenge() {
        for seed in 0..500 {
            let [safe, twisted] = draw_round_choice(&ActHistory::default(), seeded(seed));
            if let (Some(a), Some(b)) = (safe.challenge, twisted.challenge) {
                assert_ne!(a, b, "seed {seed}");
            }
        }
    }

    #[test]
    fn nothing_used_in_the_act_is_drawn_again() {
        let history = ActHistory {
            act: 1,
            twists: vec![Twist::Fog, Twist::Swarm, Twist::Drought],
            challenges: vec![ChallengeKind::ColdSteel, ChallengeKind::Blitz, ChallengeKind::FullMag],
        };
        for seed in 0..500 {
            let [safe, twisted] = draw_round_choice(&history, seeded(seed));
            assert!(matches!(twisted.twist, Some(Twist::Stampede | Twist::LowGravity)), "seed {seed}");
            for challenge in [safe.challenge, twisted.challenge].into_iter().flatten() {
                assert!(!history.challenges.contains(&challenge), "seed {seed}: {challenge:?}");
            }
        }
    }

    #[test]
    fn a_used_up_pool_starts_over_rather_than_failing_the_draw() {
        let history = ActHistory { act: 1, twists: Twist::ALL.to_vec(), challenges: ChallengeKind::ALL.to_vec() };
        let [_, twisted] = draw_round_choice(&history, seeded(1));
        assert!(twisted.twist.is_some());
    }

    #[test]
    fn a_new_act_clears_the_history() {
        let mut history = ActHistory::default();
        history.for_act(1);
        history.used(option(Some(Twist::Fog), Some(ChallengeKind::Blitz)));
        assert_eq!(history.twists, vec![Twist::Fog]);
        history.for_act(1);
        assert_eq!(history.twists, vec![Twist::Fog], "the same Act keeps it");
        history.for_act(act_of(11));
        assert!(history.twists.is_empty() && history.challenges.is_empty());
        // Endless counts each ten-Round stretch as an Act.
        assert_ne!(act_of(31), act_of(41));
    }

    // --- Challenges ---

    #[test]
    fn only_player_kills_count_or_break() {
        use Attack::*;
        for neutral in [Burn, Trap, Barrier, Nuke, Teleporter, AmmoMod(crate::enemies::zombie::Pay::Paid), Napalm, Scream, NovaBlast, Swing] {
            for kind in ChallengeKind::ALL {
                assert!(!kill_breaks(kind, neutral, false), "{kind:?} {neutral:?}");
                assert!(!kill_counts(kind, neutral, true), "{kind:?} {neutral:?}");
            }
        }
    }

    #[test]
    fn cold_steel_is_broken_by_a_bullet_and_counts_knife_kills() {
        assert!(kill_breaks(ChallengeKind::ColdSteel, Attack::Bullet, true));
        assert!(!kill_breaks(ChallengeKind::ColdSteel, Attack::Knife, false));
        assert!(kill_counts(ChallengeKind::ColdSteel, Attack::Knife, false));
    }

    #[test]
    fn clean_shots_is_broken_by_a_knife_or_a_body_shot() {
        assert!(kill_breaks(ChallengeKind::CleanShots, Attack::Knife, true));
        assert!(kill_breaks(ChallengeKind::CleanShots, Attack::Bullet, false));
        assert!(!kill_breaks(ChallengeKind::CleanShots, Attack::Bullet, true));
        assert!(kill_counts(ChallengeKind::CleanShots, Attack::Bullet, true));
    }

    #[test]
    fn the_other_challenges_count_every_player_kill_and_never_break_on_one() {
        for kind in [ChallengeKind::FullMag, ChallengeKind::HoldTheLine, ChallengeKind::TightFist, ChallengeKind::Blitz] {
            for attack in [Attack::Bullet, Attack::Knife] {
                assert!(!kill_breaks(kind, attack, false));
                assert!(kill_counts(kind, attack, false));
            }
        }
    }

    #[test]
    fn full_mag_breaks_on_any_reload_and_only_then() {
        let mut run = ChallengeRun::new(ChallengeKind::FullMag);
        assert!(!run.killed(Attack::Bullet, false));
        assert!(run.reloaded());
        assert_eq!(run.standing, Standing::Broken);
        assert!(!run.reloaded(), "it only breaks once");
        assert!(!ChallengeRun::new(ChallengeKind::ColdSteel).reloaded());
    }

    #[test]
    fn tight_fist_breaks_on_a_purchase_with_a_price() {
        let mut run = ChallengeRun::new(ChallengeKind::TightFist);
        assert!(!run.spent(0), "a free use costs nothing");
        assert!(run.spent(750));
        assert!(!ChallengeRun::new(ChallengeKind::FullMag).spent(750));
    }

    #[test]
    fn hold_the_line_breaks_when_the_zone_changes() {
        let mut run = ChallengeRun::new(ChallengeKind::HoldTheLine);
        assert!(!run.in_zone(ZoneId::StartRoom), "the first Zone is the one to hold");
        assert!(!run.in_zone(ZoneId::StartRoom));
        assert!(run.in_zone(ZoneId::WestHall));
        assert_eq!(run.standing, Standing::Broken);
    }

    #[test]
    fn blitz_allows_one_and_a_half_seconds_a_zombie_with_a_floor() {
        assert_eq!(blitz_limit_secs(10), GAUNTLET_BLITZ_MIN_SECS);
        assert_eq!(blitz_limit_secs(40), 60.0);
        let mut run = ChallengeRun::new(ChallengeKind::Blitz);
        assert!(!run.tick(1000.0, 10, false), "the clock waits for the first spawn");
        assert!(!run.tick(40.0, 10, true));
        assert!(run.tick(6.0, 10, true), "past 45 s it breaks");
        assert_eq!(run.standing, Standing::Broken);
    }

    #[test]
    fn a_challenge_unbroken_at_the_rounds_end_is_met_and_a_broken_one_is_not() {
        let mut met = ChallengeRun::new(ChallengeKind::ColdSteel);
        met.killed(Attack::Knife, false);
        met.cleared();
        assert_eq!(met.standing, Standing::Met);
        let mut failed = ChallengeRun::new(ChallengeKind::ColdSteel);
        failed.killed(Attack::Bullet, false);
        failed.cleared();
        assert_eq!(failed.standing, Standing::Broken);
    }

    #[test]
    fn a_broken_challenge_stops_counting() {
        let mut run = ChallengeRun::new(ChallengeKind::CleanShots);
        run.killed(Attack::Bullet, true);
        run.killed(Attack::Bullet, false);
        run.killed(Attack::Bullet, true);
        assert_eq!(run.kills, 1);
    }

    #[test]
    fn the_hud_line_shows_kills_against_the_horde() {
        let mut run = ChallengeRun::new(ChallengeKind::ColdSteel);
        for _ in 0..12 {
            run.killed(Attack::Knife, false);
        }
        assert_eq!(challenge_line(&run, 20), "Knife kills only: 12/20");
    }

    // --- rewards ---

    #[test]
    fn surviving_a_twist_or_meeting_a_challenge_bumps_rarity() {
        assert!(reward(true, None).rarity_bump);
        assert!(reward(false, Some(ChallengeKind::Blitz)).rarity_bump);
        assert!(!reward(false, None).rarity_bump);
    }

    #[test]
    fn both_in_one_round_also_give_an_extra_pick() {
        assert_eq!(reward(true, Some(ChallengeKind::Blitz)).extra_picks, GAUNTLET_BOTH_EXTRA_PICKS);
        assert_eq!(reward(true, None).extra_picks, 0);
        assert_eq!(reward(false, Some(ChallengeKind::Blitz)).extra_picks, 0);
    }

    #[test]
    fn a_met_challenge_guarantees_a_card_of_its_family() {
        assert_eq!(reward(false, Some(ChallengeKind::ColdSteel)).families, vec![Family::Blade]);
        assert!(reward(true, None).families.is_empty());
    }

    #[test]
    fn the_pact_offer_carries_a_reward_to_the_next_boon_offer() {
        let mut variety = RoundVariety { reward: reward(true, None), ..default() };
        assert!(variety.take_reward(false).is_none(), "a Pact Offer takes none");
        assert!(variety.reward.rarity_bump, "and it's still owed");
        assert!(variety.take_reward(true).rarity_bump);
        assert!(variety.reward.is_none(), "paid once");
    }

    #[test]
    fn a_carried_reward_adds_to_the_next_rounds() {
        let mut boost = reward(true, None);
        boost.merge(reward(false, Some(ChallengeKind::TightFist)));
        assert!(boost.rarity_bump);
        assert_eq!(boost.families, vec![Family::Gold]);
    }

    fn live(round: u32, twist: Option<Twist>, challenge: Option<ChallengeKind>) -> RoundVariety {
        RoundVariety {
            chosen: Some(Chosen {
                round,
                option: option(twist, challenge),
                challenge: challenge.map(ChallengeRun::new),
                live: true,
            }),
            ..default()
        }
    }

    #[test]
    fn tight_fist_breaks_on_any_points_actually_spent_even_with_no_buyable_to_price() {
        let mut app = App::new();
        app.add_message::<ZombieKilled>()
            .add_message::<Reloaded>()
            .add_message::<PlayCue>()
            .init_resource::<Time>()
            .insert_resource(Round::in_progress(7))
            .insert_resource(live(7, None, Some(ChallengeKind::TightFist)))
            .add_systems(Update, track_challenge);
        let player = app.world_mut().spawn((LocalPlayer, Points { balance: 5000, ..default() })).id();
        app.update();
        let standing = |app: &App| app.world().resource::<RoundVariety>().chosen.as_ref().unwrap().challenge.as_ref().unwrap().standing;
        assert_eq!(standing(&app), Standing::Active);
        app.world_mut().get_mut::<Points>(player).unwrap().spend(1000);
        app.update();
        assert_eq!(standing(&app), Standing::Broken);
    }

    #[test]
    fn looking_at_the_reward_leaves_it_to_be_taken_once_an_offer_carries_it() {
        let mut variety = RoundVariety { reward: reward(true, None), ..default() };
        assert!(variety.reward_for(true).rarity_bump);
        assert!(variety.reward_for(false).is_none());
        assert!(variety.reward_for(true).rarity_bump, "not consumed");
        assert!(variety.take_reward(true).rarity_bump);
        assert!(variety.reward_for(true).is_none());
    }

    #[test]
    fn clearing_a_twisted_round_counts_the_twist_and_pays_the_reward() {
        let mut variety = live(7, Some(Twist::Fog), None);
        assert!(!variety.cleared(7));
        assert_eq!(variety.twists_survived, 1);
        assert!(variety.reward.rarity_bump);
    }

    #[test]
    fn clearing_with_the_challenge_met_counts_it_and_both_pay_an_extra_pick() {
        let mut variety = live(7, Some(Twist::Swarm), Some(ChallengeKind::FullMag));
        assert!(variety.cleared(7));
        assert_eq!((variety.challenges_met, variety.twists_survived), (1, 1));
        assert_eq!(variety.reward.extra_picks, GAUNTLET_BOTH_EXTRA_PICKS);
        assert_eq!(variety.reward.families, vec![Family::Spark]);
    }

    #[test]
    fn a_broken_challenge_costs_only_its_reward() {
        let mut variety = live(7, None, Some(ChallengeKind::FullMag));
        variety.chosen.as_mut().unwrap().challenge.as_mut().unwrap().reloaded();
        assert!(!variety.cleared(7));
        assert_eq!(variety.challenges_met, 0);
        assert!(variety.reward.is_none());
    }

    #[test]
    fn a_round_is_only_cleared_once() {
        let mut variety = live(7, Some(Twist::Fog), None);
        variety.cleared(7);
        variety.cleared(7);
        assert_eq!(variety.twists_survived, 1);
        assert!(!live(7, Some(Twist::Fog), None).cleared(8), "another Round isn't this one");
    }

    // --- Twists ---

    #[test]
    fn each_twists_entries_are_the_spec_ones() {
        use crate::modifier::resolve;
        let active = |twist| twist_entries(twist, true);
        assert_eq!(resolve(Stat::SprinterChance, 0.0, &active(Twist::Stampede)), 1.0);
        assert_eq!(resolve(Stat::HordeSize, 20.0, &active(Twist::Swarm)), 30.0);
        assert_eq!(resolve(Stat::AliveCap, 24.0, &active(Twist::Swarm)), 32.0);
        assert!((resolve(Stat::PlayerGravity, GRAVITY, &active(Twist::LowGravity)) - GRAVITY * 0.4).abs() < 1e-4);
        assert_eq!(resolve(Stat::PowerUpDropChance, 1.0, &active(Twist::Drought)), 0.0);
        assert!(active(Twist::Fog).is_empty(), "Fog is presentation only");
    }

    #[test]
    fn a_swarms_horde_is_sized_in_the_intermission_before_it_and_nothing_else_is_early() {
        let variety = live(8, Some(Twist::Swarm), None);
        let early = variety.entries(7, &RoundPhase::Intermission(5.0));
        assert!(early.iter().any(|entry| entry.stat == Stat::HordeSize));
        assert!(early.iter().all(|entry| entry.stat == Stat::HordeSize));
        let on = variety.entries(8, &RoundPhase::Active);
        assert!(on.iter().any(|entry| entry.stat == Stat::AliveCap));
        assert!(variety.entries(8, &RoundPhase::Intermission(5.0)).is_empty(), "gone once the Round is over");
        for stat in [Twist::LowGravity, Twist::Drought, Twist::Stampede] {
            let variety = live(8, Some(stat), None);
            assert!(variety.entries(7, &RoundPhase::Intermission(5.0)).is_empty(), "{stat:?} waits for its Round");
            assert!(!variety.entries(8, &RoundPhase::Active).is_empty());
        }
    }

    #[test]
    fn a_twists_entries_are_one_source_that_multiplies_with_other_sources() {
        use crate::modifier::resolve;
        let mut entries = twist_entries(Twist::Swarm, true);
        entries.push(Modifier { stat: Stat::HordeSize, source: Source("Pact"), effect: Effect::Mult(2.0) });
        assert_eq!(resolve(Stat::HordeSize, 10.0, &entries), 30.0);
        assert!(entries.iter().filter(|entry| is_twist_source(entry.source)).count() >= 2);
    }

    // --- Boosted draws ---

    #[test]
    fn a_rarity_bump_never_lowers_a_slot_and_lifts_it_one_step() {
        let held = HeldBoons::default();
        let run = everything();
        let weights = |bump| {
            (0..400u64)
                .map(|seed| {
                    let boost = Boost { rarity_bump: bump, ..default() };
                    let cards = draw_boosted(BOONS, &held, &run, 3, &boost, seeded(seed));
                    assert_eq!(cards.len(), 3);
                    cards.iter().map(|card| card.rarity as u32).sum::<u32>()
                })
                .sum::<u32>()
        };
        assert!(weights(true) > weights(false), "Offers lean up a Rarity");
    }

    #[test]
    fn a_bumped_common_slot_is_a_rare_and_a_legendary_stays_legendary() {
        assert_eq!(bumped(Rarity::Common), Rarity::Rare);
        assert_eq!(bumped(Rarity::Rare), Rarity::Legendary);
        assert_eq!(bumped(Rarity::Legendary), Rarity::Legendary);
    }

    #[test]
    fn a_boosted_offer_has_distinct_cards() {
        let held = HeldBoons::default();
        for seed in 0..200 {
            let boost = Boost { rarity_bump: true, ..default() };
            let cards = draw_boosted(BOONS, &held, &everything(), 3, &boost, seeded(seed));
            let mut ids: Vec<_> = cards.iter().map(|card| card.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), cards.len(), "seed {seed}");
        }
    }

    #[test]
    fn a_family_guarantee_puts_a_card_of_that_family_in_every_offer() {
        let held = HeldBoons::default();
        for family in Family::ALL {
            for seed in 0..100 {
                let boost = Boost { rarity_bump: seed % 2 == 0, families: vec![family], ..default() };
                let cards = draw_boosted(BOONS, &held, &everything(), 3, &boost, seeded(seed));
                assert!(
                    cards.iter().any(|card| card.duo.is_some_and(|duo| duo.families.contains(&family)) || card.family == family),
                    "{family:?} seed {seed}"
                );
                assert_eq!(cards.len(), 3);
            }
        }
    }

    #[test]
    fn a_guarantee_never_swaps_out_a_duo_or_breaks_a_boss_offer_of_legendaries() {
        use crate::run::gauntlet::duos::DUOS;
        let held = HeldBoons::default();
        let boost = Boost { families: vec![Family::Blade], ..default() };
        for seed in 0..100 {
            let cards = draw_cards_boosted(10, &held, &everything(), &boost, seeded(seed));
            assert!(cards.iter().all(|card| card.rarity == Rarity::Legendary), "seed {seed}");
        }
        let duo = DUOS.first().unwrap();
        let cards = guarantee_families(vec![duo, find("long_arm").unwrap()], &boost, false, BOONS, &held, &everything(), seeded(1));
        assert!(cards[0].duo.is_some(), "the Duo stays");
    }

    // --- the systems ---

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<RoundVariety>()
            .init_resource::<RoundStartCue>()
            .init_resource::<ReservedRounds>()
            .init_resource::<Settings>()
            .init_resource::<RunArc>()
            .init_resource::<RunType>()
            .insert_resource(Round::in_progress(4))
            .add_message::<RoundEnded>()
            .add_message::<PickRound>()
            .add_message::<PlayCue>()
            .add_systems(Update, (end_round, light_choice, apply_pick).chain());
        app
    }

    #[test]
    fn a_cleared_round_draws_a_choice_for_the_next_and_a_pick_sets_its_variety() {
        let mut app = app();
        app.world_mut().write_message(RoundEnded);
        app.update();
        let pending = app.world().resource::<RoundVariety>().pending.clone().expect("Round 5 has a choice");
        assert_eq!(pending.round, 5);
        assert!(app.world().resource::<RoundVariety>().lit, "no Offer is open, so it can be picked");
        app.world_mut().write_message(PickRound { option: 1 });
        app.update();
        let variety = app.world().resource::<RoundVariety>();
        assert!(variety.pending.is_none());
        let chosen = variety.chosen.as_ref().expect("picked");
        assert_eq!((chosen.round, chosen.option), (5, pending.options[1]));
        assert_eq!(app.world().resource::<RoundStartCue>().0, Some(Cue::TwistRoundStart), "a Twist Round starts with its own cue");
        assert_eq!(variety.history.twists.len(), 1);
    }

    #[test]
    fn a_round_that_isnt_eligible_has_no_choice() {
        let mut app = app();
        app.insert_resource(Round::in_progress(9));
        app.world_mut().write_message(RoundEnded);
        app.update();
        assert!(app.world().resource::<RoundVariety>().pending.is_none(), "Round 10 is a Boss Round");
    }

    #[test]
    fn co_op_draws_no_choice() {
        let mut app = app();
        app.insert_resource(RunType::Coop);
        app.world_mut().write_message(RoundEnded);
        app.update();
        assert!(app.world().resource::<RoundVariety>().pending.is_none());
    }

    #[test]
    fn a_pick_made_before_the_choice_is_lit_is_ignored() {
        let mut app = app();
        app.world_mut().spawn(PendingOffer { left: Some(2.0), owed: vec![4] });
        app.world_mut().write_message(RoundEnded);
        app.update();
        assert!(!app.world().resource::<RoundVariety>().lit, "an Offer is still due");
        app.world_mut().write_message(PickRound { option: 0 });
        app.update();
        assert!(app.world().resource::<RoundVariety>().pending.is_some());
    }
}
