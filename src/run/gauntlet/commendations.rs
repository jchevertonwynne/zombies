//! Commendations: 21 one-off goals across Runs, about 4,800 Scrip in all. Each pays its Scrip at
//! once when met (never multiplied by Heat), shows a banner, and is listed again on Game Over. Any
//! Gauntlet Run counts, whatever its Tracks, Heat or Kit, Endless included; Runs played before they
//! existed don't count after the fact. The six Family Commendations also earn that Family's Kit.
//!
//! The rules are pure functions of a [`RunLog`], the facts a Run leaves behind, so each is tested
//! on its own; the systems below only keep the log up to date and pay what it earns. They run on
//! the machine that simulates the Run and credit its `LocalPlayer`.

use bevy::prelude::*;

use super::arc::{RunArc, act_of, is_boss_round, win_round};
use super::boon::{Family, HeldBoons};
use super::elite::Elite;
use super::pact::HeldPacts;
use super::progress::GauntletFile;
use super::tools::ToolUsed;
use super::{ActiveRanks, UnlockAll};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::buy::Purchased;
use crate::economy::perk::{Perk, PerkMachine};
use crate::enemies::zombie::{Dying, Zombie};
use crate::players::player::{LocalPlayer, Seat};
use crate::run::round::{Round, RoundEnded, RoundStarted};
use crate::state::InRun;
use crate::storage::records::RunElitesKilled;
use crate::tuning::*;
use crate::ui::hud::text;

/// What a Commendation asks of a Run, for the Armory's groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Family,
    Arc,
    Build,
    Restraint,
    Elites,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Family, Group::Arc, Group::Build, Group::Restraint, Group::Elites];

    pub fn name(self) -> &'static str {
        match self {
            Group::Family => "FAMILY",
            Group::Arc => "ARC",
            Group::Build => "BUILD",
            Group::Restraint => "RESTRAINT",
            Group::Elites => "ELITES",
        }
    }
}

pub struct Commendation {
    /// The stable ID used in `gauntlet.ron`.
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    /// Scrip it pays once.
    pub scrip: u64,
}

const fn commendation(id: &'static str, name: &'static str, group: Group, scrip: u64) -> Commendation {
    Commendation { id, name, group, scrip }
}

/// All 21, in the order Appendix B7 lists them.
pub const COMMENDATIONS: [Commendation; 21] = [
    commendation("family_lead", "Reach the 4-Boon Lead Set Bonus", Group::Family, COMMENDATION_FAMILY_SCRIP),
    commendation("family_blade", "Reach the 4-Boon Blade Set Bonus", Group::Family, COMMENDATION_FAMILY_SCRIP),
    commendation("family_spark", "Reach the 4-Boon Spark Set Bonus", Group::Family, COMMENDATION_FAMILY_SCRIP),
    commendation("family_ember", "Reach the 4-Boon Ember Set Bonus", Group::Family, COMMENDATION_FAMILY_SCRIP),
    commendation("family_gold", "Reach the 4-Boon Gold Set Bonus", Group::Family, COMMENDATION_FAMILY_SCRIP),
    commendation("family_grit", "Reach the 4-Boon Grit Set Bonus", Group::Family, COMMENDATION_FAMILY_SCRIP),
    commendation("clear_act_1", "Clear the Act 1 Boss Round", Group::Arc, COMMENDATION_CLEAR_ACT_1_SCRIP),
    commendation("clear_act_2", "Clear the Act 2 Boss Round", Group::Arc, COMMENDATION_CLEAR_ACT_2_SCRIP),
    commendation("win", "Win", Group::Arc, COMMENDATION_WIN_SCRIP),
    commendation("endless_50", "Reach Round 50 in Endless", Group::Arc, COMMENDATION_ENDLESS_50_SCRIP),
    commendation("win_heat_5", "Win at Heat 5 or more", Group::Arc, COMMENDATION_HEAT_5_SCRIP),
    commendation("win_heat_10", "Win at Heat 10 or more", Group::Arc, COMMENDATION_HEAT_10_SCRIP),
    commendation("hold_duo", "Hold a Duo Boon", Group::Build, COMMENDATION_HOLD_DUO_SCRIP),
    commendation("three_duos", "Hold 3 Duo Boons in one Run", Group::Build, COMMENDATION_THREE_DUOS_SCRIP),
    commendation("set_tier_6", "Reach a 6-Boon Set Bonus", Group::Build, COMMENDATION_SET_TIER_6_SCRIP),
    commendation("win_six_family", "Win holding 6 Boons of one Family", Group::Build, COMMENDATION_FAMILY_WIN_SCRIP),
    commendation("win_no_pact", "Win without holding a Pact", Group::Build, COMMENDATION_NO_PACT_SCRIP),
    commendation(
        "no_juggernog",
        "Clear the Act 2 Boss Round without buying Juggernog",
        Group::Restraint,
        COMMENDATION_NO_JUGGERNOG_SCRIP,
    ),
    commendation("win_no_tracks", "Win with every Upgrade Track switched off", Group::Restraint, COMMENDATION_NO_TRACKS_SCRIP),
    commendation("win_no_tools", "Win without a Reroll or a Banish", Group::Restraint, COMMENDATION_NO_TOOLS_SCRIP),
    commendation("elites_100", "Kill 100 Elites", Group::Elites, COMMENDATION_ELITES_SCRIP),
];

/// The Commendation with stable ID `id`.
pub fn find(id: &str) -> Option<&'static Commendation> {
    COMMENDATIONS.iter().find(|commendation| commendation.id == id)
}

/// What a Run looked like at the moment it was Won, for the Win Commendations.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WinFacts {
    pub heat: u32,
    /// The most distinct Boons of one Family held.
    pub most_in_family: u32,
    /// Whether any Pact was held by this Player.
    pub held_pact: bool,
    /// Whether every Upgrade Track was switched off, or never bought.
    pub tracks_off: bool,
    /// Whether a Reroll or a Banish was used by then.
    pub used_tools: bool,
}

/// The facts a Run leaves behind, which Commendations are checked against.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct RunLog {
    /// The most distinct Boons of each Family held at once, by `Family` order.
    pub most_in_family: [u32; 6],
    /// The most Duo Boons held at once.
    pub most_duos: u32,
    /// The Boss Rounds cleared.
    pub cleared: Vec<u32>,
    /// Juggernog has been bought this Run.
    pub bought_juggernog: bool,
    /// The Act 2 Boss Round was cleared with Juggernog never bought.
    pub act_2_without_juggernog: bool,
    /// A Reroll or a Banish has been used this Run.
    pub used_tools: bool,
    /// The highest Round reached.
    pub highest_round: u32,
    /// The Run's Heat; Heat sets it once it exists.
    pub heat: u32,
    /// The Run's Win, once reached.
    pub win: Option<WinFacts>,
    /// Elites killed this Run, and by the Run's start in all.
    pub elites: u64,
    pub elites_before: u64,
}

impl RunLog {
    /// A new Run's log, with the Elites already killed across earlier Runs.
    pub fn new(elites_before: u64) -> Self {
        Self { elites_before, ..default() }
    }

    /// Notes the Boons held: the most of each Family and the most Duos held so far.
    pub fn note_held(&mut self, held: &HeldBoons) {
        for family in Family::ALL {
            let most = &mut self.most_in_family[family as usize];
            *most = (*most).max(held.in_family(family));
        }
        self.most_duos = self.most_duos.max(held.0.iter().filter(|(boon, _)| boon.duo.is_some()).count() as u32);
    }

    pub fn most_in_any_family(&self) -> u32 {
        self.most_in_family.into_iter().max().unwrap_or(0)
    }

    /// Notes a Boss Round cleared.
    pub fn note_cleared(&mut self, round: u32) {
        if !self.cleared.contains(&round) {
            self.cleared.push(round);
        }
        if act_of(round) == 2 && !self.bought_juggernog {
            self.act_2_without_juggernog = true;
        }
    }

    /// Elites killed in all, this Run included.
    pub fn elites_total(&self) -> u64 {
        self.elites_before.saturating_add(self.elites)
    }
}

/// The Boss Round that ends Act `act`.
fn boss_round_of_act(act: u32) -> u32 {
    act * GAUNTLET_ACT_ROUNDS
}

/// Whether `log` meets the Commendation with stable ID `id`.
pub fn is_met(id: &str, log: &RunLog) -> bool {
    let won = |check: fn(&WinFacts) -> bool| log.win.as_ref().is_some_and(check);
    if let Some(family) = Family::ALL.into_iter().find(|family| id == format!("family_{}", family.name().to_lowercase())) {
        return log.most_in_family[family as usize] >= COMMENDATION_FAMILY_TIER_BOONS;
    }
    match id {
        "clear_act_1" => log.cleared.contains(&boss_round_of_act(1)),
        "clear_act_2" => log.cleared.contains(&boss_round_of_act(2)),
        "win" => log.win.is_some(),
        "endless_50" => log.highest_round >= COMMENDATION_ENDLESS_ROUND,
        "win_heat_5" => won(|win| win.heat >= COMMENDATION_HEAT_WINS[0]),
        "win_heat_10" => won(|win| win.heat >= COMMENDATION_HEAT_WINS[1]),
        "hold_duo" => log.most_duos >= 1,
        "three_duos" => log.most_duos >= COMMENDATION_DUOS_HELD,
        "set_tier_6" => log.most_in_any_family() >= COMMENDATION_SET_CAPSTONE_BOONS,
        "win_six_family" => won(|win| win.most_in_family >= COMMENDATION_SET_CAPSTONE_BOONS),
        "win_no_pact" => won(|win| !win.held_pact),
        "no_juggernog" => log.act_2_without_juggernog,
        "win_no_tracks" => won(|win| win.tracks_off),
        "win_no_tools" => won(|win| !win.used_tools),
        "elites_100" => log.elites_total() >= COMMENDATION_ELITE_KILLS,
        _ => false,
    }
}

/// The Commendations `log` meets that `file` hasn't earned yet, in roster order.
pub fn newly_met(log: &RunLog, file: &GauntletFile) -> Vec<&'static Commendation> {
    COMMENDATIONS
        .iter()
        .filter(|commendation| !file.has_commendation(commendation.id) && is_met(commendation.id, log))
        .collect()
}

/// A Commendation was earned: its Scrip is paid already.
#[derive(Message, Debug, Clone, Copy)]
pub struct CommendationEarned {
    pub id: &'static str,
}

/// The Commendations earned this Run, for Game Over.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct RunCommendations(pub Vec<&'static str>);

/// The banner line for a Commendation: "COMMENDATION: Win at Heat 5 (+400 Scrip)".
pub fn banner_line(commendation: &Commendation) -> String {
    format!("COMMENDATION: {} (+{} Scrip)", commendation.name, commendation.scrip)
}

/// The Game Over line listing what was earned this Run, or `None` if nothing was.
pub fn game_over_line(earned: &RunCommendations) -> Option<String> {
    if earned.0.is_empty() {
        return None;
    }
    let names: Vec<_> = earned.0.iter().filter_map(|id| find(id)).map(|commendation| commendation.name).collect();
    Some(format!("Commendations earned: {}", names.join(", ")))
}

/// Starts a Run's log, with the Elites already killed, and forgets last Run's earnings.
pub fn begin_run(mut commands: Commands, file: Res<GauntletFile>) {
    commands.insert_resource(RunLog::new(file.elites_killed));
    commands.insert_resource(RunCommendations::default());
}

/// Keeps the log up to date from what the Run does: the Boons held, Juggernog bought, Rerolls and
/// Banishes used, Boss Rounds cleared, the Rounds reached, Elites killed and the Win.
#[allow(clippy::too_many_arguments)]
pub fn log_run(
    mut log: ResMut<RunLog>,
    local: Single<(Entity, &HeldBoons, &Seat), With<LocalPlayer>>,
    pacts: Res<HeldPacts>,
    arc: Res<RunArc>,
    ranks: Res<ActiveRanks>,
    round: Res<Round>,
    mut purchases: MessageReader<Purchased>,
    machines: Query<&PerkMachine>,
    mut tools: MessageReader<ToolUsed>,
    mut ended: MessageReader<RoundEnded>,
    mut started: MessageReader<RoundStarted>,
    elites: Query<(), (With<Elite>, With<Zombie>, Added<Dying>)>,
    mut file: ResMut<GauntletFile>,
    mut killed: ResMut<RunElitesKilled>,
) {
    let (player, held, seat) = *local;
    log.note_held(held);
    for &Purchased { player: buyer, buyable } in purchases.read() {
        if buyer == player && machines.get(buyable).is_ok_and(|machine| machine.perk == Perk::Juggernog) {
            log.bought_juggernog = true;
        }
    }
    if tools.read().any(|tool| tool.player == player) {
        log.used_tools = true;
    }
    if started.read().count() > 0 {
        log.highest_round = log.highest_round.max(round.number);
    }
    if ended.read().count() > 0 && is_boss_round(round.number) {
        log.note_cleared(round.number);
    }
    let kills = elites.iter().count() as u64;
    if kills > 0 {
        log.elites = log.elites.saturating_add(kills);
        killed.0 = log.elites.min(u64::from(u32::MAX)) as u32;
    }
    if log.win.is_none() && arc.won {
        debug_assert!(round.number >= win_round());
        log.win = Some(WinFacts {
            heat: log.heat,
            most_in_family: held_most_in_a_family(held),
            held_pact: pacts.0.iter().any(|held| held.holder == *seat),
            tracks_off: ranks.0 == 0,
            used_tools: log.used_tools,
        });
        // The first Win is what unlocks Heat.
        if !file.has_won {
            file.has_won = true;
        }
    }
}

/// The most distinct Boons held of any one Family.
fn held_most_in_a_family(held: &HeldBoons) -> u32 {
    Family::ALL.into_iter().map(|family| held.in_family(family)).max().unwrap_or(0)
}

/// Pays every Commendation the log newly meets, at once, and says so.
pub fn earn_commendations(
    log: Res<RunLog>,
    mut file: ResMut<GauntletFile>,
    mut earned: ResMut<RunCommendations>,
    mut announce: MessageWriter<CommendationEarned>,
    unlock_all: Res<UnlockAll>,
) {
    // The debug key's Run isn't a real one.
    if unlock_all.0 || !log.is_changed() {
        return;
    }
    for commendation in newly_met(&log, &file) {
        if file.earn(commendation.id, commendation.scrip) {
            earned.0.push(commendation.id);
            announce.write(CommendationEarned { id: commendation.id });
        }
    }
}

/// The Commendation banner, top-centre, and the seconds it has left.
#[derive(Component)]
pub struct CommendationBanner(f32);

pub fn spawn_banner(mut commands: Commands) {
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(21.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((CommendationBanner(0.0), text("", 30.0, Color::srgb(1.0, 0.85, 0.2)), Visibility::Hidden));
}

/// A Commendation earned shows its banner for `GAUNTLET_COMMENDATION_BANNER_SECS`.
pub fn show_banner(
    mut earned: MessageReader<CommendationEarned>,
    banner: Single<(&mut CommendationBanner, &mut Text, &mut Visibility)>,
    mut cues: MessageWriter<PlayCue>,
    time: Res<Time<Real>>,
) {
    let (mut banner, mut text, mut visibility) = banner.into_inner();
    for &CommendationEarned { id } in earned.read() {
        if let Some(commendation) = find(id) {
            text.0 = banner_line(commendation);
            banner.0 = GAUNTLET_COMMENDATION_BANNER_SECS;
            cues.write(PlayCue::new(Cue::Commendation, None));
        }
    }
    banner.0 -= time.delta_secs();
    visibility.set_if_neq(if banner.0 > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::find as boon;

    fn held(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(boon(id).unwrap());
        }
        held
    }

    fn won(facts: WinFacts) -> RunLog {
        RunLog { win: Some(facts), ..default() }
    }

    fn clean_win() -> WinFacts {
        WinFacts { heat: 0, most_in_family: 0, held_pact: false, tracks_off: true, used_tools: false }
    }

    #[test]
    fn the_roster_is_21_commendations_worth_4800_scrip() {
        assert_eq!(COMMENDATIONS.len(), 21);
        let total: u64 = COMMENDATIONS.iter().map(|commendation| commendation.scrip).sum();
        assert_eq!(total, 4_800);
        for (group, expected) in [
            (Group::Family, 600),
            (Group::Arc, 1_750),
            (Group::Build, 1_300),
            (Group::Restraint, 950),
            (Group::Elites, 200),
        ] {
            let sum: u64 = COMMENDATIONS.iter().filter(|c| c.group == group).map(|c| c.scrip).sum();
            assert_eq!(sum, expected, "{group:?}");
        }
        let mut ids: Vec<_> = COMMENDATIONS.iter().map(|commendation| commendation.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 21, "every ID is its own");
    }

    #[test]
    fn a_family_commendation_needs_four_distinct_boons_of_that_family() {
        let mut log = RunLog::default();
        log.note_held(&held(&["hollow_points", "marksmans_eye", "full_belt"]));
        assert!(!is_met("family_lead", &log));
        // Stacks of one Boon don't count twice.
        let mut stacked = held(&["hollow_points", "marksmans_eye", "full_belt"]);
        stacked.add(boon("hollow_points").unwrap());
        log.note_held(&stacked);
        assert!(!is_met("family_lead", &log));
        log.note_held(&held(&["hollow_points", "marksmans_eye", "full_belt", "stripper_clips"]));
        assert!(is_met("family_lead", &log));
        assert!(!is_met("family_blade", &log));
        assert!(!is_met("set_tier_6", &log));
    }

    #[test]
    fn the_log_remembers_the_most_held_even_after_a_boon_is_lost() {
        let mut log = RunLog::default();
        log.note_held(&held(&["hollow_points", "marksmans_eye", "full_belt", "stripper_clips"]));
        log.note_held(&HeldBoons::default());
        assert_eq!(log.most_in_family[Family::Lead as usize], 4);
        assert!(is_met("family_lead", &log));
    }

    #[test]
    fn a_6_boon_set_bonus_needs_six_of_one_family() {
        let lead = ["hollow_points", "marksmans_eye", "full_belt", "stripper_clips", "parting_shot", "through_and_through"];
        let mut log = RunLog::default();
        log.note_held(&held(&lead[..5]));
        assert!(!is_met("set_tier_6", &log));
        log.note_held(&held(&lead));
        assert!(is_met("set_tier_6", &log));
    }

    #[test]
    fn duo_commendations_count_the_duos_held() {
        let mut log = RunLog::default();
        assert!(!is_met("hold_duo", &log));
        log.note_held(&held(&["gun_and_blade"]));
        assert!(is_met("hold_duo", &log) && !is_met("three_duos", &log));
        log.note_held(&held(&["gun_and_blade", "ball_lightning", "gold_standard"]));
        assert!(is_met("three_duos", &log));
        assert_eq!(log.most_in_any_family(), 0, "a Duo counts toward no Family");
    }

    #[test]
    fn boss_round_commendations_need_that_acts_boss_round() {
        let mut log = RunLog::default();
        log.note_cleared(GAUNTLET_ACT_ROUNDS);
        assert!(is_met("clear_act_1", &log) && !is_met("clear_act_2", &log));
        log.note_cleared(2 * GAUNTLET_ACT_ROUNDS);
        assert!(is_met("clear_act_2", &log));
        assert!(!is_met("win", &log));
    }

    #[test]
    fn no_juggernog_needs_act_2_cleared_with_none_bought_by_then() {
        let mut log = RunLog::default();
        log.note_cleared(GAUNTLET_ACT_ROUNDS);
        assert!(!is_met("no_juggernog", &log), "Act 1 isn't Act 2");
        log.note_cleared(2 * GAUNTLET_ACT_ROUNDS);
        assert!(is_met("no_juggernog", &log));

        let mut bought = RunLog { bought_juggernog: true, ..default() };
        bought.note_cleared(2 * GAUNTLET_ACT_ROUNDS);
        assert!(is_met("clear_act_2", &bought));
        assert!(!is_met("no_juggernog", &bought));
    }

    #[test]
    fn a_win_is_met_by_a_win_and_endless_by_round_50() {
        assert!(!is_met("win", &RunLog::default()));
        assert!(is_met("win", &won(clean_win())));
        assert!(!is_met("endless_50", &RunLog { highest_round: 49, ..default() }));
        assert!(is_met("endless_50", &RunLog { highest_round: 50, ..default() }));
    }

    #[test]
    fn heat_wins_need_a_win_at_that_heat_or_more() {
        let at = |heat| won(WinFacts { heat, ..clean_win() });
        assert!(!is_met("win_heat_5", &at(4)));
        assert!(is_met("win_heat_5", &at(5)) && !is_met("win_heat_10", &at(5)));
        assert!(is_met("win_heat_10", &at(12)) && is_met("win_heat_5", &at(12)));
        assert!(!is_met("win_heat_5", &RunLog { heat: 9, ..default() }), "a Run not won earns nothing");
    }

    #[test]
    fn win_commendations_look_at_the_moment_of_the_win() {
        assert!(is_met("win_no_pact", &won(clean_win())));
        assert!(!is_met("win_no_pact", &won(WinFacts { held_pact: true, ..clean_win() })));
        assert!(is_met("win_no_tools", &won(clean_win())));
        assert!(!is_met("win_no_tools", &won(WinFacts { used_tools: true, ..clean_win() })));
        assert!(is_met("win_no_tracks", &won(clean_win())));
        assert!(!is_met("win_no_tracks", &won(WinFacts { tracks_off: false, ..clean_win() })));
        assert!(!is_met("win_six_family", &won(WinFacts { most_in_family: 5, ..clean_win() })));
        assert!(is_met("win_six_family", &won(WinFacts { most_in_family: 6, ..clean_win() })));
        // None of them without the Win itself.
        for id in ["win_no_pact", "win_no_tools", "win_no_tracks", "win_six_family"] {
            assert!(!is_met(id, &RunLog::default()), "{id}");
        }
    }

    #[test]
    fn elites_are_counted_across_runs() {
        let mut log = RunLog::new(95);
        assert!(!is_met("elites_100", &log));
        log.elites = 4;
        assert!(!is_met("elites_100", &log));
        log.elites = 5;
        assert!(is_met("elites_100", &log));
        assert!(is_met("elites_100", &RunLog::new(100)));
    }

    #[test]
    fn a_commendation_pays_once_and_earns_its_kit() {
        let mut file = GauntletFile::default();
        let log = RunLog { most_in_family: [0, 4, 0, 0, 0, 0], ..default() };
        let met = newly_met(&log, &file);
        assert_eq!(met.iter().map(|c| c.id).collect::<Vec<_>>(), ["family_blade"]);
        assert!(file.earn("family_blade", met[0].scrip));
        assert_eq!((file.scrip, file.all_time_scrip), (100, 100));
        assert!(newly_met(&log, &file).is_empty(), "already earned");
        assert!(!file.earn("family_blade", 100), "never paid twice");
        assert_eq!(file.scrip, 100);
        assert!(super::super::kits::Kit::Blade.is_earned(&file));
    }

    #[test]
    fn commendation_scrip_is_never_scaled_by_heat() {
        // The Scrip a Commendation pays is its constant, whatever the Run's Heat was.
        let mut file = GauntletFile::default();
        let log = won(WinFacts { heat: 10, ..clean_win() });
        for commendation in newly_met(&log, &file) {
            file.earn(commendation.id, commendation.scrip);
        }
        let expected: u64 = ["win", "win_heat_5", "win_heat_10", "win_no_pact", "win_no_tools", "win_no_tracks"]
            .iter()
            .map(|id| find(id).unwrap().scrip)
            .sum();
        assert_eq!(file.scrip, expected);
    }

    #[test]
    fn the_banner_and_game_over_lines_name_the_commendation() {
        assert_eq!(banner_line(find("win_heat_5").unwrap()), "COMMENDATION: Win at Heat 5 or more (+400 Scrip)");
        assert_eq!(game_over_line(&RunCommendations::default()), None);
        let earned = RunCommendations(vec!["win", "hold_duo"]);
        assert_eq!(game_over_line(&earned).unwrap(), "Commendations earned: Win, Hold a Duo Boon");
    }
    /// A Run's systems on their own: a local Player, and the messages Classic sends.
    fn running(round: u32) -> (App, Entity) {
        let mut app = App::new();
        app.add_message::<Purchased>()
            .add_message::<ToolUsed>()
            .add_message::<RoundEnded>()
            .add_message::<RoundStarted>()
            .add_message::<CommendationEarned>()
            .insert_resource(RunLog::new(0))
            .init_resource::<RunCommendations>()
            .init_resource::<GauntletFile>()
            .init_resource::<RunElitesKilled>()
            .init_resource::<HeldPacts>()
            .init_resource::<RunArc>()
            .init_resource::<ActiveRanks>()
            .init_resource::<UnlockAll>()
            .insert_resource(Round::in_progress(round))
            .add_systems(Update, (log_run, earn_commendations).chain());
        let player = app.world_mut().spawn((LocalPlayer, Seat(0), HeldBoons::default())).id();
        (app, player)
    }

    fn earned(app: &App) -> Vec<String> {
        app.world().resource::<GauntletFile>().commendations.clone()
    }

    #[test]
    fn holding_four_of_a_family_earns_its_commendation_at_once_and_only_once() {
        let (mut app, player) = running(3);
        app.update();
        assert!(earned(&app).is_empty());
        *app.world_mut().get_mut::<HeldBoons>(player).unwrap() =
            held(&["hollow_points", "marksmans_eye", "full_belt", "stripper_clips"]);
        app.update();
        app.update();
        assert_eq!(earned(&app), ["family_lead"]);
        assert_eq!(app.world().resource::<GauntletFile>().scrip, COMMENDATION_FAMILY_SCRIP);
        assert_eq!(app.world().resource::<RunCommendations>().0, ["family_lead"]);
        assert_eq!(app.world().resource::<Messages<CommendationEarned>>().len(), 1);
    }

    #[test]
    fn clearing_the_first_boss_round_earns_the_act_1_commendation() {
        let (mut app, _) = running(GAUNTLET_ACT_ROUNDS);
        app.world_mut().write_message(RoundEnded);
        app.update();
        assert_eq!(earned(&app), ["clear_act_1"]);
        assert_eq!(app.world().resource::<GauntletFile>().scrip, COMMENDATION_CLEAR_ACT_1_SCRIP);
    }

    #[test]
    fn buying_juggernog_and_using_a_tool_are_noticed_for_the_local_player_only() {
        let (mut app, player) = running(3);
        let other = app.world_mut().spawn_empty().id();
        app.world_mut().write_message(ToolUsed { player: other });
        app.update();
        assert!(!app.world().resource::<RunLog>().used_tools);
        app.world_mut().write_message(ToolUsed { player });
        app.update();
        assert!(app.world().resource::<RunLog>().used_tools);
    }

    #[test]
    fn a_win_with_a_clean_run_earns_the_win_commendations_and_unlocks_heat() {
        let (mut app, _) = running(win_round());
        app.world_mut().resource_mut::<RunArc>().won = true;
        app.update();
        let got = earned(&app);
        for id in ["win", "win_no_pact", "win_no_tools", "win_no_tracks"] {
            assert!(got.iter().any(|earned| earned == id), "{id} in {got:?}");
        }
        assert!(!got.iter().any(|id| id == "win_heat_5"));
        assert!(app.world().resource::<GauntletFile>().has_won, "the first Win gates Heat");
    }

    #[test]
    fn a_used_tool_or_an_active_track_spoils_the_win_commendations() {
        let (mut app, _) = running(win_round());
        app.world_mut().resource_mut::<RunLog>().used_tools = true;
        app.world_mut().resource_mut::<ActiveRanks>().0 = 3;
        app.world_mut().resource_mut::<RunArc>().won = true;
        app.update();
        let got = earned(&app);
        assert!(got.iter().any(|id| id == "win"));
        assert!(!got.iter().any(|id| id == "win_no_tools" || id == "win_no_tracks"), "{got:?}");
    }

    #[test]
    fn the_debug_unlock_key_earns_nothing() {
        let (mut app, player) = running(3);
        app.world_mut().resource_mut::<UnlockAll>().0 = true;
        *app.world_mut().get_mut::<HeldBoons>(player).unwrap() =
            held(&["hollow_points", "marksmans_eye", "full_belt", "stripper_clips"]);
        app.update();
        assert!(earned(&app).is_empty());
    }
}
