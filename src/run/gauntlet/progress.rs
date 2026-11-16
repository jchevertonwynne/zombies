//! Gauntlet progress between Runs, kept in `gauntlet.ron`: the Scrip each Run pays, the Boons and
//! Pacts Unlocked with it, the Upgrade Tracks' Ranks and toggles, the Commendations earned, the
//! chosen Kit and the across-Run counts. The pure rules for paying, Unlocking and buying Ranks live
//! here, with the v1 to v2 migration; the Armory and the plugin call them.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::boon::{BOONS, BoonDef, Rarity};
use super::pact::PACTS;
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::storage::save::SaveFile;
use crate::tuning::*;

/// The Scrip a Run pays for completing `rounds` Rounds: each completed Round pays its number.
pub fn scrip_for_rounds(rounds: u32) -> u64 {
    let n = u64::from(rounds);
    n * (n + 1) / 2
}

/// How many Rounds a Run completed, from the Round it was on and whether that Round had ended
/// (the Intermission after it).
pub fn rounds_completed(round: u32, in_intermission: bool) -> u32 {
    if in_intermission {
        round
    } else {
        round.saturating_sub(1)
    }
}

/// A permanent Gauntlet bonus bought one Rank at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    StartingPoints,
    WeaponDamage,
    MoveSpeed,
    BoxPrice,
    PowerUpDuration,
    /// Starting Banish charges. Not a stat entry: the Run's Banishes read its Ranks.
    Banish,
}

impl Track {
    pub const ALL: [Track; 6] = [
        Track::StartingPoints,
        Track::WeaponDamage,
        Track::MoveSpeed,
        Track::BoxPrice,
        Track::PowerUpDuration,
        Track::Banish,
    ];

    /// The Track's Rank prices in Scrip, first Rank first.
    pub fn rank_prices(self) -> &'static [u64] {
        match self {
            Track::Banish => &TRACK_BANISH_RANK_PRICES,
            _ => &UPGRADE_TRACK_RANK_PRICES,
        }
    }

    /// Ranks on the Track.
    pub fn max_ranks(self) -> u32 {
        self.rank_prices().len() as u32
    }

    /// The price of the Rank after `ranks`, or `None` once the Track is full.
    pub fn rank_price(self, ranks: u32) -> Option<u64> {
        self.rank_prices().get(ranks as usize).copied()
    }

    /// Ranks on every Track together.
    pub fn total_ranks() -> u32 {
        Track::ALL.iter().map(|track| track.max_ranks()).sum()
    }

    /// The stable ID used in `gauntlet.ron`. The display name can change.
    pub fn id(self) -> &'static str {
        match self {
            Track::StartingPoints => "starting_points",
            Track::WeaponDamage => "weapon_damage",
            Track::MoveSpeed => "move_speed",
            Track::BoxPrice => "box_price",
            Track::PowerUpDuration => "power_up_duration",
            Track::Banish => "banish",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Track::StartingPoints => "Starting Points",
            Track::WeaponDamage => "Weapon damage",
            Track::MoveSpeed => "Move speed",
            Track::BoxPrice => "Box spin price",
            Track::PowerUpDuration => "Power-up duration",
            Track::Banish => "Banish charges",
        }
    }

    /// The Track's stat-modifier source: each Track is its own.
    fn source(self) -> Source {
        Source(match self {
            Track::StartingPoints => "Track: Starting Points",
            Track::WeaponDamage => "Track: Weapon damage",
            Track::MoveSpeed => "Track: Move speed",
            Track::BoxPrice => "Track: Box spin price",
            Track::PowerUpDuration => "Track: Power-up duration",
            Track::Banish => "Track: Banish charges",
        })
    }

    /// The Track's stat-modifier entry at `ranks` Ranks, or `None` at none (and always for the
    /// Banish Track, which is no stat).
    pub fn entry(self, ranks: u32) -> Option<Modifier> {
        if ranks == 0 {
            return None;
        }
        let n = ranks as f32;
        let (stat, effect) = match self {
            Track::StartingPoints => (
                Stat::StartingPoints,
                Effect::Flat(TRACK_STARTING_POINTS_PER_RANK as f32 * n),
            ),
            Track::WeaponDamage => (
                Stat::Damage,
                Effect::Mult(1.0 + TRACK_WEAPON_DAMAGE_INCREASE_PER_RANK * n),
            ),
            Track::MoveSpeed => (
                Stat::MoveSpeed,
                Effect::Mult(1.0 + TRACK_MOVE_SPEED_INCREASE_PER_RANK * n),
            ),
            Track::BoxPrice => (
                Stat::BoxPrice,
                Effect::Reduction(TRACK_BOX_PRICE_FACTOR_PER_RANK, ranks),
            ),
            Track::PowerUpDuration => (
                Stat::PowerUpDuration,
                Effect::Flat(TRACK_POWER_UP_DURATION_SECS_PER_RANK * n),
            ),
            Track::Banish => return None,
        };
        Some(Modifier {
            stat,
            source: self.source(),
            effect,
        })
    }

    /// The Track's whole effect at `ranks` Ranks, as the Armory shows it.
    pub fn effect(self, ranks: u32) -> String {
        let n = f64::from(ranks);
        match self {
            Track::StartingPoints => {
                format!(
                    "+{} Points",
                    TRACK_STARTING_POINTS_PER_RANK * u64::from(ranks)
                )
            }
            Track::WeaponDamage => format!(
                "+{:.0}%",
                f64::from(TRACK_WEAPON_DAMAGE_INCREASE_PER_RANK) * n * 100.0
            ),
            Track::MoveSpeed => format!(
                "+{:.0}%",
                f64::from(TRACK_MOVE_SPEED_INCREASE_PER_RANK) * n * 100.0
            ),
            Track::BoxPrice => {
                format!("x{:.3}", f64::from(TRACK_BOX_PRICE_FACTOR_PER_RANK).powf(n))
            }
            Track::PowerUpDuration => format!(
                "+{:.0} s",
                f64::from(TRACK_POWER_UP_DURATION_SECS_PER_RANK) * n
            ),
            Track::Banish => format!("+{} Banish", TRACK_BANISHES_PER_RANK * ranks),
        }
    }
}

/// What can be Unlocked, by stable ID.
#[derive(Clone, Copy)]
pub enum Card {
    Boon(&'static BoonDef),
    Pact(&'static str),
}

impl Card {
    /// The Unlock price in Scrip, or `None` for a Boon Unlocked from the first launch.
    pub fn unlock_price(self) -> Option<u64> {
        match self {
            Card::Boon(boon) if boon.starts_unlocked => None,
            Card::Boon(boon) => Some(match boon.rarity {
                Rarity::Common | Rarity::Rare => UNLOCK_PRICE_RARE,
                Rarity::Legendary => UNLOCK_PRICE_LEGENDARY,
            }),
            Card::Pact(_) => Some(UNLOCK_PRICE_PACT),
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Card::Boon(boon) => boon.id,
            Card::Pact(id) => id,
        }
    }

    /// The Boon or Pact with this stable ID.
    pub fn find(id: &str) -> Option<Card> {
        BOONS
            .iter()
            .find(|boon| boon.id == id)
            .map(Card::Boon)
            .or_else(|| {
                PACTS
                    .iter()
                    .find(|pact| pact.id() == id)
                    .map(|pact| Card::Pact(pact.id()))
            })
    }
}

/// Why the Armory refused a purchase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Unlocked already, or Unlocked from the first launch.
    AlreadyUnlocked,
    /// No Boon or Pact has that ID.
    Unknown,
    /// Every Rank on the Track is bought.
    TrackFull,
    TooPoor,
}

/// One Upgrade Track's saved state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TrackSave {
    /// The Track's stable ID.
    pub id: String,
    pub ranks: u32,
    /// Toggled on: the Track's Ranks apply to the next Run. Free to change.
    pub on: bool,
}

/// The v1 Rares v2 makes Common. A v1 file that bought one is refunded `UNLOCK_PRICE_RARE` for it.
pub const V1_RARES_NOW_COMMON: [&str; 6] = [
    "loaded_dice",
    "field_engineer",
    "iron_stomach",
    "last_rites",
    "bounty",
    "butchers_bill",
];

/// `gauntlet.ron`: Scrip, Unlocks, Upgrade Track Ranks and toggles, Commendations, the Kit, Heat
/// and the across-Run counts.
#[derive(Resource, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GauntletFile {
    pub version: u32,
    /// Scrip to spend.
    pub scrip: u64,
    /// Every Scrip ever earned, before spending.
    pub all_time_scrip: u64,
    /// The stable IDs of the Boons and Pacts bought in the Armory. Those Unlocked from the first
    /// launch aren't listed.
    pub unlocked: Vec<String>,
    /// Only Tracks with a Rank bought or toggled off are listed; the rest are at 0 Ranks, on.
    pub tracks: Vec<TrackSave>,
    /// The stable IDs of the Commendations earned.
    #[serde(default)]
    pub commendations: Vec<String>,
    /// The stable ID of the Kit picked for the next Run; none means Standard.
    #[serde(default)]
    pub kit: Option<String>,
    /// The Degree set for each Heat Condition by stable ID, once Heat exists. Conditions at 0 aren't
    /// listed.
    #[serde(default)]
    pub heat: Vec<(String, u32)>,
    /// A Run has been Won: the one thing that unlocks Heat.
    #[serde(default)]
    pub has_won: bool,
    /// Elites killed over every Run, for their Commendation.
    #[serde(default)]
    pub elites_killed: u64,
}

impl Default for GauntletFile {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            scrip: 0,
            all_time_scrip: 0,
            unlocked: Vec::new(),
            tracks: Vec::new(),
            commendations: Vec::new(),
            kit: None,
            heat: Vec::new(),
            has_won: false,
            elites_killed: 0,
        }
    }
}

impl SaveFile for GauntletFile {
    const FILE_NAME: &'static str = "gauntlet.ron";
    const VERSION: u32 = 2;

    fn version(&self) -> u32 {
        self.version
    }

    fn migrate(text: &str) -> Option<Self> {
        let old = ron::from_str::<GauntletFile>(text).ok()?;
        (old.version == 1).then(|| old.migrated_from_v1())
    }
}

impl GauntletFile {
    /// A v1 file as v2: Scrip, all-time Scrip, Unlocks, Ranks and toggles carry over, and each
    /// bought v1 Rare that v2 makes Common is refunded `UNLOCK_PRICE_RARE` Scrip. The refund isn't
    /// earnings, so the all-time total stays.
    fn migrated_from_v1(mut self) -> Self {
        let before = self.unlocked.len();
        self.unlocked.retain(|id| !V1_RARES_NOW_COMMON.contains(&id.as_str()));
        let refunded = (before - self.unlocked.len()) as u64;
        self.scrip = self.scrip.saturating_add(refunded * UNLOCK_PRICE_RARE);
        self.version = Self::VERSION;
        self
    }

    /// Pays a finished Run's Scrip.
    pub fn pay(&mut self, scrip: u64) {
        self.scrip = self.scrip.saturating_add(scrip);
        self.all_time_scrip = self.all_time_scrip.saturating_add(scrip);
    }

    /// Whether a Boon or Pact can appear on Offers: Unlocked from the first launch or bought.
    pub fn is_unlocked(&self, card: Card) -> bool {
        card.unlock_price().is_none() || self.unlocked.iter().any(|id| id == card.id())
    }

    /// Unlocks the Boon or Pact with stable ID `id` for its price in Scrip.
    pub fn unlock(&mut self, id: &str) -> Result<(), Refusal> {
        let card = Card::find(id).ok_or(Refusal::Unknown)?;
        if self.is_unlocked(card) {
            return Err(Refusal::AlreadyUnlocked);
        }
        let price = card.unlock_price().ok_or(Refusal::AlreadyUnlocked)?;
        self.scrip = self.scrip.checked_sub(price).ok_or(Refusal::TooPoor)?;
        self.unlocked.push(card.id().to_string());
        Ok(())
    }

    fn track(&self, track: Track) -> Option<&TrackSave> {
        self.tracks.iter().find(|saved| saved.id == track.id())
    }

    fn track_mut(&mut self, track: Track) -> &mut TrackSave {
        let at = match self.tracks.iter().position(|saved| saved.id == track.id()) {
            Some(at) => at,
            None => {
                self.tracks.push(TrackSave {
                    id: track.id().to_string(),
                    ranks: 0,
                    on: true,
                });
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[at]
    }

    /// Ranks bought on a Track, on or off.
    pub fn ranks(&self, track: Track) -> u32 {
        self.track(track)
            .map_or(0, |saved| saved.ranks.min(track.max_ranks()))
    }

    pub fn is_on(&self, track: Track) -> bool {
        self.track(track).is_none_or(|saved| saved.on)
    }

    /// Buys a Track's next Rank.
    pub fn buy_rank(&mut self, track: Track) -> Result<(), Refusal> {
        let ranks = self.ranks(track);
        let price = track.rank_price(ranks).ok_or(Refusal::TrackFull)?;
        self.scrip = self.scrip.checked_sub(price).ok_or(Refusal::TooPoor)?;
        self.track_mut(track).ranks = ranks + 1;
        Ok(())
    }

    /// Switches a Track on or off, for free.
    pub fn toggle(&mut self, track: Track) {
        let saved = self.track_mut(track);
        saved.on = !saved.on;
    }

    /// Ranks that will apply to the next Run: those on Tracks toggled on (0 to 27).
    pub fn active_ranks(&self) -> u32 {
        Track::ALL
            .iter()
            .filter(|&&track| self.is_on(track))
            .map(|&track| self.ranks(track))
            .sum()
    }

    /// Banish charges the Banish Track gives a Run's start, if it's on.
    pub fn track_banishes(&self) -> u32 {
        if self.is_on(Track::Banish) {
            self.ranks(Track::Banish) * TRACK_BANISHES_PER_RANK
        } else {
            0
        }
    }

    /// Whether the Commendation with stable ID `id` is earned.
    pub fn has_commendation(&self, id: &str) -> bool {
        self.commendations.iter().any(|earned| earned == id)
    }

    /// Earns a Commendation, paying its Scrip at once (never multiplied by Heat). `false` if it
    /// was earned already, when nothing is paid.
    pub fn earn(&mut self, id: &str, scrip: u64) -> bool {
        if self.has_commendation(id) {
            return false;
        }
        self.commendations.push(id.to_string());
        self.pay(scrip);
        true
    }

    /// The stat-modifier entries the Tracks toggled on give a Gauntlet Run's Player.
    pub fn track_entries(&self) -> Vec<Modifier> {
        Track::ALL
            .iter()
            .filter(|&&track| self.is_on(track))
            .filter_map(|&track| track.entry(self.ranks(track)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::{resolve, resolve_points};
    use crate::storage::save::tests::TempDir;
    use crate::storage::save::{load, store};

    fn rich() -> GauntletFile {
        GauntletFile {
            scrip: 10_000,
            ..default()
        }
    }

    #[test]
    fn each_completed_round_pays_its_number() {
        assert_eq!(scrip_for_rounds(0), 0);
        assert_eq!(scrip_for_rounds(1), 1);
        assert_eq!(scrip_for_rounds(3), 6);
        // A Round 20 Run completed 19 Rounds and earns 190.
        assert_eq!(scrip_for_rounds(rounds_completed(20, false)), 190);
        // Ending in the Intermission after Round 20, it was completed.
        assert_eq!(scrip_for_rounds(rounds_completed(20, true)), 210);
        assert_eq!(rounds_completed(1, false), 0);
        assert_eq!(rounds_completed(0, false), 0);
        assert!(scrip_for_rounds(u32::MAX) > 0);
    }

    #[test]
    fn paying_adds_to_the_balance_and_the_all_time_total() {
        let mut file = GauntletFile::default();
        file.pay(190);
        file.unlock("third_hand").unwrap_err();
        file.pay(60);
        assert_eq!((file.scrip, file.all_time_scrip), (250, 250));
        file.unlock("third_hand").unwrap();
        assert_eq!((file.scrip, file.all_time_scrip), (0, 250));
    }

    #[test]
    fn starting_cards_are_unlocked_from_the_first_launch() {
        let file = GauntletFile::default();
        let unlocked: Vec<_> = BOONS
            .iter()
            .filter(|boon| file.is_unlocked(Card::Boon(boon)))
            .collect();
        assert_eq!(unlocked.len(), 24 + 3);
        for id in ["war_bonds", "barbed_wire", "shrapnel"] {
            assert!(unlocked.iter().any(|boon| boon.id == id), "{id}");
        }
        assert!(
            PACTS
                .iter()
                .all(|pact| !file.is_unlocked(Card::Pact(pact.id())))
        );
    }

    #[test]
    fn unlock_prices_go_by_rarity() {
        let price = |id| Card::find(id).unwrap().unlock_price();
        assert_eq!(price("arc_light"), Some(UNLOCK_PRICE_RARE));
        assert_eq!(price("third_hand"), Some(UNLOCK_PRICE_LEGENDARY));
        assert_eq!(price("glass_jaw"), Some(UNLOCK_PRICE_PACT));
        assert_eq!(price("war_bonds"), None);
        assert_eq!(price("blood_money"), None);
        assert_eq!(price("loaded_dice"), None, "a v1 Rare now Common is free");
        assert_eq!(
            (UNLOCK_PRICE_RARE, UNLOCK_PRICE_LEGENDARY, UNLOCK_PRICE_PACT),
            (100, 250, 200)
        );
    }

    #[test]
    fn unlocking_spends_scrip_once() {
        let mut file = rich();
        file.unlock("glass_jaw").unwrap();
        assert_eq!(file.scrip, 10_000 - 200);
        assert!(file.is_unlocked(Card::Pact("glass_jaw")));
        assert_eq!(file.unlock("glass_jaw"), Err(Refusal::AlreadyUnlocked));
        assert_eq!(file.unlock("war_bonds"), Err(Refusal::AlreadyUnlocked));
        assert_eq!(file.unlock("nope"), Err(Refusal::Unknown));
        assert_eq!(file.scrip, 10_000 - 200);
    }

    #[test]
    fn an_unaffordable_unlock_is_refused_and_costs_nothing() {
        let mut file = GauntletFile {
            scrip: 99,
            ..default()
        };
        assert_eq!(file.unlock("arc_light"), Err(Refusal::TooPoor));
        assert_eq!(file.scrip, 99);
        assert!(file.unlocked.is_empty());
        file.scrip = 100;
        assert_eq!(file.unlock("arc_light"), Ok(()));
        assert_eq!(file.scrip, 0);
    }

    #[test]
    fn ranks_cost_more_each_and_stop_at_five() {
        let mut file = GauntletFile {
            scrip: 750,
            ..default()
        };
        for expected in [700, 600, 450, 250, 0] {
            file.buy_rank(Track::MoveSpeed).unwrap();
            assert_eq!(file.scrip, expected);
        }
        assert_eq!(file.ranks(Track::MoveSpeed), 5);
        file.scrip = 1000;
        assert_eq!(file.buy_rank(Track::MoveSpeed), Err(Refusal::TrackFull));
        assert_eq!(file.scrip, 1000);
    }

    #[test]
    fn an_unaffordable_rank_is_refused() {
        let mut file = GauntletFile {
            scrip: 49,
            ..default()
        };
        assert_eq!(file.buy_rank(Track::BoxPrice), Err(Refusal::TooPoor));
        assert_eq!((file.scrip, file.ranks(Track::BoxPrice)), (49, 0));
    }

    #[test]
    fn a_track_toggled_off_gives_nothing_but_keeps_its_ranks() {
        let mut file = rich();
        file.buy_rank(Track::StartingPoints).unwrap();
        file.buy_rank(Track::StartingPoints).unwrap();
        file.buy_rank(Track::WeaponDamage).unwrap();
        assert_eq!(file.active_ranks(), 3);
        let starting = |file: &GauntletFile| {
            resolve_points(Stat::StartingPoints, STARTING_POINTS, &file.track_entries())
        };
        assert_eq!(starting(&file), 700);

        let scrip = file.scrip;
        file.toggle(Track::StartingPoints);
        assert_eq!(file.scrip, scrip, "toggling is free");
        assert_eq!(file.active_ranks(), 1);
        assert_eq!(starting(&file), 500);
        assert_eq!(file.ranks(Track::StartingPoints), 2);

        file.toggle(Track::StartingPoints);
        assert_eq!(starting(&file), 700);
    }

    #[test]
    fn full_tracks_give_appendix_b_maximums() {
        let mut file = rich();
        for track in Track::ALL {
            for _ in 0..track.max_ranks() {
                file.buy_rank(track).unwrap();
            }
        }
        assert_eq!(file.active_ranks(), 27);
        let entries = file.track_entries();
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(
            resolve(Stat::StartingPoints, 500.0, &entries),
            1000.0
        ));
        assert!(close(resolve(Stat::Damage, 1.0, &entries), 1.15));
        assert!(close(resolve(Stat::MoveSpeed, 1.0, &entries), 1.10));
        assert!(close(
            resolve(Stat::BoxPrice, 1.0, &entries),
            0.95_f32.powi(5)
        ));
        assert!(close(resolve(Stat::PowerUpDuration, 0.0, &entries), 10.0));
        assert_eq!(Track::BoxPrice.effect(5), "x0.774");
    }

    #[test]
    fn no_ranks_means_no_entries() {
        assert!(GauntletFile::default().track_entries().is_empty());
        assert_eq!(GauntletFile::default().active_ranks(), 0);
    }

    #[test]
    fn a_hand_edited_rank_count_is_capped() {
        let file = GauntletFile {
            tracks: vec![TrackSave {
                id: Track::MoveSpeed.id().into(),
                ranks: 99,
                on: true,
            }],
            ..default()
        };
        assert_eq!(file.ranks(Track::MoveSpeed), 5);
        assert_eq!(file.active_ranks(), 5);
    }

    #[test]
    fn gauntlet_ron_round_trips() {
        let dir = TempDir::new("gauntlet-round-trip");
        let mut file = rich();
        file.pay(190);
        file.unlock("lucky_strike").unwrap();
        file.buy_rank(Track::PowerUpDuration).unwrap();
        file.toggle(Track::PowerUpDuration);
        store(&dir.0, &file).unwrap();
        assert_eq!(load::<GauntletFile>(&dir.0), file);
    }

    #[test]
    fn a_corrupt_gauntlet_ron_falls_back_to_defaults() {
        let dir = TempDir::new("gauntlet-corrupt");
        std::fs::write(dir.0.join("gauntlet.ron"), "(version: 1, scrip: lots").unwrap();
        assert_eq!(load::<GauntletFile>(&dir.0), GauntletFile::default());
        assert!(dir.0.join("gauntlet.ron.bak").exists());

        let missing = TempDir::new("gauntlet-missing");
        assert_eq!(load::<GauntletFile>(&missing.0), GauntletFile::default());
    }

    #[test]
    fn unknown_ids_in_the_file_are_ignored() {
        let file = GauntletFile {
            unlocked: vec!["retired_boon".into()],
            tracks: vec![TrackSave {
                id: "retired_track".into(),
                ranks: 3,
                on: true,
            }],
            ..default()
        };
        assert_eq!(file.active_ranks(), 0);
        assert!(file.track_entries().is_empty());
    }
    #[test]
    fn the_banish_track_has_two_ranks_and_adds_starting_charges_while_on() {
        assert_eq!(Track::Banish.max_ranks(), 2);
        assert_eq!(Track::StartingPoints.max_ranks(), 5);
        assert_eq!(Track::total_ranks(), 27);
        let mut file = rich();
        assert_eq!(file.track_banishes(), 0);
        file.buy_rank(Track::Banish).unwrap();
        assert_eq!(file.scrip, 10_000 - 300);
        file.buy_rank(Track::Banish).unwrap();
        assert_eq!(file.scrip, 10_000 - 900);
        assert_eq!(file.buy_rank(Track::Banish), Err(Refusal::TrackFull));
        assert_eq!(file.track_banishes(), 2);
        assert!(file.track_entries().is_empty(), "Banish is no stat");
        file.toggle(Track::Banish);
        assert_eq!(file.track_banishes(), 0, "switched off");
        assert_eq!(file.ranks(Track::Banish), 2);
    }

    #[test]
    fn a_runs_starting_banishes_are_the_usual_one_plus_the_tracks() {
        use crate::run::gauntlet::tools::Banishes;
        assert_eq!(Banishes::default().charges, GAUNTLET_BANISHES_START);
        assert_eq!(Banishes::starting(2).charges, GAUNTLET_BANISHES_START + 2);
    }

    fn v1_text(scrip: u64, unlocked: &[&str]) -> String {
        let ids: Vec<String> = unlocked.iter().map(|id| format!("\"{id}\"")).collect();
        format!(
            "(version: 1, scrip: {scrip}, all_time_scrip: 900, unlocked: [{}], tracks: [(id: \"move_speed\", ranks: 3, on: false)])",
            ids.join(", ")
        )
    }

    #[test]
    fn a_v1_file_migrates_to_v2_keeping_scrip_unlocks_ranks_and_toggles() {
        let file: GauntletFile = crate::storage::save::parse(&v1_text(250, &["arc_light", "third_hand"])).unwrap();
        assert_eq!(file.version, 2);
        assert_eq!((file.scrip, file.all_time_scrip), (250, 900));
        assert_eq!(file.unlocked, ["arc_light", "third_hand"]);
        assert_eq!(file.ranks(Track::MoveSpeed), 3);
        assert!(!file.is_on(Track::MoveSpeed));
        // The v2 additions start empty.
        assert!(file.commendations.is_empty() && file.kit.is_none() && file.heat.is_empty());
        assert!(!file.has_won);
        assert_eq!(file.elites_killed, 0);
    }

    #[test]
    fn each_bought_v1_rare_now_common_is_refunded_and_dropped() {
        let bought = ["loaded_dice", "arc_light", "bounty", "butchers_bill", "field_engineer", "iron_stomach", "last_rites"];
        let file: GauntletFile = crate::storage::save::parse(&v1_text(0, &bought)).unwrap();
        assert_eq!(file.scrip, 6 * UNLOCK_PRICE_RARE, "six refunds of 100");
        assert_eq!(file.unlocked, ["arc_light"], "the Rare still Rare stays bought");
        assert_eq!(file.all_time_scrip, 900, "a refund isn't earnings");
        assert!(file.is_unlocked(Card::find("bounty").unwrap()), "now free");
        assert!(file.is_unlocked(Card::find("arc_light").unwrap()));

        let none: GauntletFile = crate::storage::save::parse(&v1_text(40, &["arc_light"])).unwrap();
        assert_eq!(none.scrip, 40, "nothing to refund");
    }

    #[test]
    fn a_migrated_file_loads_from_disk_as_v2_and_is_written_as_v2() {
        let dir = TempDir::new("gauntlet-migrate");
        std::fs::write(dir.0.join("gauntlet.ron"), v1_text(10, &["loaded_dice"])).unwrap();
        let file = load::<GauntletFile>(&dir.0);
        assert_eq!((file.version, file.scrip), (2, 10 + UNLOCK_PRICE_RARE));
        assert!(!dir.0.join("gauntlet.ron.bak").exists(), "a v1 file isn't corrupt");
        store(&dir.0, &file).unwrap();
        assert_eq!(load::<GauntletFile>(&dir.0), file);
    }

    #[test]
    fn a_v2_file_with_its_new_fields_round_trips() {
        let dir = TempDir::new("gauntlet-v2-round-trip");
        let mut file = rich();
        file.buy_rank(Track::Banish).unwrap();
        file.earn("family_spark", 100);
        file.kit = Some("spark".into());
        file.heat = vec![("inflation".into(), 2)];
        file.has_won = true;
        file.elites_killed = 42;
        store(&dir.0, &file).unwrap();
        assert_eq!(load::<GauntletFile>(&dir.0), file);
    }

    #[test]
    fn an_unknown_version_still_falls_back_as_corrupt() {
        let dir = TempDir::new("gauntlet-v9");
        std::fs::write(dir.0.join("gauntlet.ron"), "(version: 9, scrip: 5, all_time_scrip: 5, unlocked: [], tracks: [])").unwrap();
        assert_eq!(load::<GauntletFile>(&dir.0), GauntletFile::default());
        assert!(dir.0.join("gauntlet.ron.bak").exists());
    }

    #[test]
    fn a_commendation_pays_scrip_once_and_is_remembered() {
        let mut file = GauntletFile::default();
        assert!(file.earn("win", 300));
        assert_eq!((file.scrip, file.all_time_scrip), (300, 300));
        assert!(!file.earn("win", 300));
        assert_eq!(file.scrip, 300);
        assert!(file.has_commendation("win") && !file.has_commendation("hold_duo"));
    }
}
