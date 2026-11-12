//! Run Records, the Records boards and Lifetime Stats, kept in `records.ron`.
//!
//! A Run Record is written when a Run ends for this machine's Player: Game Over, quit (to the Main
//! Menu, Leave Run, or closing the window), or the Host leaving for good. It's written once per Run
//! (see [`RunRecorded`]), so a drop and Rejoin saves only the real end. A crash loses it: there are
//! no mid-Run saves.

use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::maps::map::MapId;
use crate::run::mode::Mode;
use crate::net::play::Scoreboard;
use crate::players::player::{LocalPlayer, Player, PlayerName, Seat, SpawnPlayers};
use crate::state::RunType;
use crate::economy::points::Points;
use crate::run::round::Round;
use crate::storage::save::{self, SaveFile};
use crate::storage::settings::{SaveDir, Settings, SpecialType, SpecialZombies};
use crate::state::GameState;
use crate::run::stats::RunStats;
use crate::tuning::*;

/// The saved summary of one finished Run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RunRecord {
    pub map: MapId,
    pub mode: Mode,
    /// When the Run ended, in whole seconds since the Unix epoch (UTC).
    pub date: u64,
    /// The Round reached.
    pub round: u32,
    /// Every kill, Paid or Unpaid.
    pub kills: u32,
    pub headshots: u32,
    pub downs: u32,
    pub points_earned: u64,
    /// Game-time survived in seconds, with the world's frozen time left out.
    pub survived_secs: f64,
    /// How many Players the Run started with: Records are kept per count. Old saves are 1.
    #[serde(default = "solo_players")]
    pub players: u8,
    /// Teammates this Player got up. Always 0 in solo.
    #[serde(default)]
    pub revives: u32,
    /// The other Players, from the Host's scoreboard. Empty in solo.
    #[serde(default)]
    pub teammates: Vec<Teammate>,
    /// Whether the Run reached its Gauntlet Win while this Player was in it. Nothing sets it until
    /// the Win exists.
    #[serde(default)]
    pub won: bool,
    /// The Host's Heat, once Heat exists.
    #[serde(default)]
    pub heat: Option<u32>,
    /// Lines a Mode layered on Classic adds to its Run Records, in the order it added them (see
    /// [`RecordExtras`]), then one for each Special Zombie type switched off during the Run (see
    /// [`off_extras`]). A faithful Classic Run leaves this empty.
    #[serde(default)]
    pub extras: Vec<RecordExtra>,
}

fn solo_players() -> u8 {
    1
}

/// A teammate on a Run Record: what the Host counted for them.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Teammate {
    pub name: String,
    pub seat: u8,
    pub kills: u32,
    pub downs: u32,
    pub revives: u32,
    pub points_earned: u64,
}

/// How many Players the current Run started with, fixed when it begins.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunPlayers(pub u8);

impl Default for RunPlayers {
    fn default() -> Self {
        Self(1)
    }
}

/// Whether the current Run's Record is already saved, so each Run saves one.
#[derive(Resource, Default)]
pub struct RunRecorded(pub bool);

/// Saves this Player's Record now, if the Run's isn't saved yet. For a Run that ends for them
/// without leaving `Playing`: the Rejoin screen's "leave" writes it.
#[derive(Message)]
pub struct SaveRecord;

/// One extra line on a Run Record: a stable ID and a count.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RecordExtra {
    pub id: String,
    pub count: u32,
}

/// The current Run's extra Record lines. Classic never writes here; a Mode layered on top keeps it
/// up to date during the Run, and it's copied onto the Run Record as-is when the Run ends.
#[derive(Resource, Default)]
pub struct RecordExtras(pub Vec<RecordExtra>);

/// The Special Zombie switches of the Run so far: a type switched off at any point counts as off.
#[derive(Resource, Default)]
pub struct RunSpecialZombies(pub SpecialZombies);

/// The Record extras a Run played with Special Zombie types switched off gets: one per type off,
/// in Settings order, so a Record without Hellhounds is visibly different.
pub fn off_extras(run: &SpecialZombies) -> Vec<RecordExtra> {
    run.off()
        .map(|kind| RecordExtra {
            id: kind.off_record_id().to_string(),
            count: 1,
        })
        .collect()
}

/// The Special Zombie types a Run Record was played with switched off.
pub fn off_types(record: &RunRecord) -> Vec<SpecialType> {
    record
        .extras
        .iter()
        .filter_map(|extra| SpecialType::from_off_record_id(&extra.id))
        .collect()
}

/// How a Record extra reads on screen: "Hellhounds Off" for a switched-off type, otherwise its ID
/// and count.
pub fn extra_line(extra: &RecordExtra) -> String {
    match SpecialType::from_off_record_id(&extra.id) {
        Some(kind) => format!("{} Off", kind.label()),
        None => format!("{} x{}", extra.id, extra.count),
    }
}

/// Whether `a` ranks above (`Less`) or below (`Greater`) `b`: the higher Round first, then the
/// shorter time.
pub fn rank_order(a: &RunRecord, b: &RunRecord) -> Ordering {
    b.round
        .cmp(&a.round)
        .then(a.survived_secs.total_cmp(&b.survived_secs))
}

/// Whether two Run Records are on the same board.
fn same_board(a: &RunRecord, b: &RunRecord) -> bool {
    a.map == b.map && a.mode == b.mode && a.players == b.players
}

/// The best [`RECORDS_KEPT`] Run Records for each (Map, Mode, Player count).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Records {
    /// Best first. Within one (Map, Mode), equal Runs keep the order they were added in.
    runs: Vec<RunRecord>,
}

impl Records {
    /// One (Map, Mode, Player count)'s Records, best first.
    pub fn board(&self, map: MapId, mode: Mode, players: u8) -> impl Iterator<Item = &RunRecord> {
        self.runs
            .iter()
            .filter(move |run| run.map == map && run.mode == mode && run.players == players)
    }

    /// Adds a Run Record. Returns its rank (0 = best) if it made its board, which may push that
    /// board's last Record off. A Run that ties an older one ranks below it.
    pub fn add(&mut self, record: RunRecord) -> Option<usize> {
        let at = self
            .runs
            .partition_point(|run| rank_order(run, &record) != Ordering::Greater);
        let rank = self.runs[..at]
            .iter()
            .filter(|run| same_board(run, &record))
            .count();
        if rank >= RECORDS_KEPT {
            return None;
        }
        let (map, mode, players) = (record.map, record.mode, record.players);
        self.runs.insert(at, record);
        self.keep_best(map, mode, players);
        Some(rank)
    }

    /// Sorts the Records and trims every board, so a hand-edited file still holds a valid list.
    fn tidy(&mut self) {
        self.runs.sort_by(rank_order);
        let mut seen = Vec::new();
        for run in &self.runs {
            if !seen.contains(&(run.map, run.mode, run.players)) {
                seen.push((run.map, run.mode, run.players));
            }
        }
        for (map, mode, players) in seen {
            self.keep_best(map, mode, players);
        }
    }

    /// Drops whatever ranks below a board's last kept Record.
    fn keep_best(&mut self, map: MapId, mode: Mode, players: u8) {
        let mut kept = 0;
        self.runs.retain(|run| {
            if run.map != map || run.mode != mode || run.players != players {
                return true;
            }
            kept += 1;
            kept <= RECORDS_KEPT
        });
    }
}

/// The highest Round reached on one (Map, Mode, Player count).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BestRound {
    pub map: MapId,
    pub mode: Mode,
    #[serde(default = "solo_players")]
    pub players: u8,
    pub round: u32,
}

/// Running totals across every Run.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct LifetimeStats {
    pub runs: u32,
    pub kills: u64,
    pub headshots: u64,
    pub downs: u64,
    pub points_earned: u64,
    pub time_played_secs: f64,
    pub best_rounds: Vec<BestRound>,
    /// Teammates this Player has got up, over every Co-op Run.
    #[serde(default)]
    pub revives: u64,
    /// Runs that reached their Gauntlet Win with this Player in them.
    #[serde(default)]
    pub wins: u32,
    /// The highest Heat a Win was had at.
    #[serde(default)]
    pub highest_heat_won: u32,
}

impl LifetimeStats {
    /// Adds a finished Run to the totals.
    pub fn add(&mut self, record: &RunRecord) {
        self.runs = self.runs.saturating_add(1);
        self.kills = self.kills.saturating_add(record.kills.into());
        self.headshots = self.headshots.saturating_add(record.headshots.into());
        self.downs = self.downs.saturating_add(record.downs.into());
        self.points_earned = self.points_earned.saturating_add(record.points_earned);
        self.time_played_secs += record.survived_secs;
        self.revives = self.revives.saturating_add(record.revives.into());
        if record.won {
            self.wins = self.wins.saturating_add(1);
            self.highest_heat_won = self.highest_heat_won.max(record.heat.unwrap_or(0));
        }
        match self.best_rounds.iter_mut().find(|best| {
            best.map == record.map && best.mode == record.mode && best.players == record.players
        }) {
            Some(best) => best.round = best.round.max(record.round),
            None => self.best_rounds.push(BestRound {
                map: record.map,
                mode: record.mode,
                players: record.players,
                round: record.round,
            }),
        }
    }

    /// The highest Round reached on a (Map, Mode, Player count), if it's been played.
    pub fn best_round(&self, map: MapId, mode: Mode, players: u8) -> Option<u32> {
        self.best_rounds
            .iter()
            .find(|best| best.map == map && best.mode == mode && best.players == players)
            .map(|best| best.round)
    }
}

/// `records.ron`: the Records boards and Lifetime Stats.
#[derive(Resource, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RecordsFile {
    pub version: u32,
    pub records: Records,
    pub lifetime: LifetimeStats,
}

impl Default for RecordsFile {
    fn default() -> Self {
        Self {
            version: Self::VERSION,
            records: Records::default(),
            lifetime: LifetimeStats::default(),
        }
    }
}

impl SaveFile for RecordsFile {
    const FILE_NAME: &'static str = "records.ron";
    const VERSION: u32 = 1;

    fn version(&self) -> u32 {
        self.version
    }
}

impl RecordsFile {
    /// Files a finished Run: Lifetime Stats always, the Records if it ranks. Returns its rank.
    pub fn add(&mut self, record: RunRecord) -> Option<usize> {
        self.lifetime.add(&record);
        self.records.add(record)
    }
}

/// The Run that just ended and its rank on the Records, for the Game Over screen.
#[derive(Resource)]
pub struct LastRun {
    pub record: RunRecord,
    pub rank: Option<usize>,
}

/// The Game Over screen's line about a Run's rank.
pub fn rank_line(rank: Option<usize>) -> String {
    match rank {
        Some(0) => "New best!".to_string(),
        Some(rank) => format!("#{} on the Records", rank + 1),
        None => format!("Outside the top {RECORDS_KEPT}"),
    }
}

/// Seconds as `m:ss`, or `h:mm:ss` from an hour up. Partial seconds are dropped.
pub fn format_time(secs: f64) -> String {
    let total = secs.max(0.0) as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Seconds since the Unix epoch as a `YYYY-MM-DD` date (UTC).
pub fn format_date(unix_secs: u64) -> String {
    // Howard Hinnant's days-to-civil, for days since 1970-01-01.
    let days = (unix_secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

pub struct RecordsPlugin;

impl Plugin for RecordsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RecordExtras>()
            .init_resource::<RunSpecialZombies>()
            .init_resource::<RunPlayers>()
            .init_resource::<RunRecorded>()
            .add_message::<SaveRecord>()
            // After the Players are spawned, so the count is the Players the Run starts with.
            .add_systems(
                OnEnter(GameState::Playing),
                (|mut recorded: ResMut<RunRecorded>,
                  mut run: ResMut<RunPlayers>,
                  players: Query<(), With<Player>>| {
                    recorded.0 = false;
                    run.0 = players.iter().count().clamp(1, 4) as u8;
                })
                .after(SpawnPlayers),
            )
            .add_systems(Startup, load_records)
            .add_systems(
                OnEnter(GameState::Playing),
                |mut extras: ResMut<RecordExtras>, mut run: ResMut<RunSpecialZombies>, settings: Res<Settings>| {
                    extras.0.clear();
                    run.0 = settings.special_zombies;
                },
            )
            .add_systems(
                Update,
                track_switches
                    .run_if(in_state(GameState::Playing))
                    .run_if(resource_changed::<Settings>),
            )
            .add_systems(OnEnter(GameState::GameOver), record_run)
            .add_systems(
                OnTransition {
                    exited: GameState::Playing,
                    entered: GameState::MainMenu,
                },
                record_run,
            )
            // Closing the window mid-Run is quitting too.
            .add_systems(
                Last,
                record_run
                    .run_if(on_message::<AppExit>)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                record_run
                    .run_if(on_message::<SaveRecord>)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, save_records.run_if(resource_changed::<RecordsFile>));
    }
}

fn load_records(mut commands: Commands, dir: Res<SaveDir>) {
    let mut file = match &dir.0 {
        Some(dir) => save::load::<RecordsFile>(dir),
        None => RecordsFile::default(),
    };
    file.records.tidy();
    commands.insert_resource(file);
}

/// A switch turned off mid-Run (from Pause) marks the Run as played without that type.
fn track_switches(settings: Res<Settings>, mut run: ResMut<RunSpecialZombies>) {
    run.0 = run.0.and(settings.special_zombies);
}

/// Writes the ending Run's Record.
pub fn record_run(
    mut commands: Commands,
    map: Res<MapId>,
    mode: Res<Mode>,
    round: Option<Res<Round>>,
    player: Single<(&RunStats, &Points, Option<&Seat>), With<LocalPlayer>>,
    (players, mut recorded, scoreboard, run_type): (
        Res<RunPlayers>,
        ResMut<RunRecorded>,
        Res<Scoreboard>,
        Res<RunType>,
    ),
    others: Query<(&Seat, &PlayerName, &RunStats, &Points), (With<Player>, Without<LocalPlayer>)>,
    extras: Res<RecordExtras>,
    switches: Res<RunSpecialZombies>,
    mut file: ResMut<RecordsFile>,
    dir: Res<SaveDir>,
) {
    if std::mem::replace(&mut recorded.0, true) {
        return;
    }
    let (stats, points, seat) = *player;
    let teammates = if run_type.is_coop() {
        teammates(&scoreboard, seat, &others)
    } else {
        Vec::new()
    };
    let date = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let record = RunRecord {
        map: *map,
        mode: *mode,
        date,
        // A client has no Round of its own to read.
        round: round.map_or(0, |round| round.number),
        kills: stats.kills,
        headshots: stats.headshots,
        downs: stats.downs,
        points_earned: points.earned,
        survived_secs: stats.survived_secs,
        players: players.0,
        revives: stats.revives,
        teammates,
        won: false,
        heat: None,
        extras: extras.0.iter().cloned().chain(off_extras(&switches.0)).collect(),
    };
    let rank = file.add(record.clone());
    // Stored straight away as well as on change, so it lands even if the app is exiting.
    store(&file, &dir);
    commands.insert_resource(LastRun { record, rank });
}

/// The other Players: the Host's end-of-Run scoreboard if it has come, otherwise (a Run that ends
/// early on the Host) the Players as they stand.
fn teammates(
    scoreboard: &Scoreboard,
    own: Option<&Seat>,
    live: &Query<(&Seat, &PlayerName, &RunStats, &Points), (With<Player>, Without<LocalPlayer>)>,
) -> Vec<Teammate> {
    if !scoreboard.0.is_empty() {
        return teammates_from(&scoreboard.0, own.copied());
    }
    let mut rows: Vec<Teammate> = live
        .iter()
        .map(|(seat, name, stats, points)| Teammate {
            name: name.0.clone(),
            seat: seat.0,
            kills: stats.kills,
            downs: stats.downs,
            revives: stats.revives,
            points_earned: points.earned,
        })
        .collect();
    rows.sort_by_key(|row| row.seat);
    rows
}

/// The scoreboard's rows without this Player's own.
pub fn teammates_from(rows: &[crate::net::protocol::Score], own: Option<Seat>) -> Vec<Teammate> {
    rows.iter()
        .filter(|row| Some(row.seat) != own.map(|seat| seat.0))
        .map(|row| Teammate {
            name: row.name.clone(),
            seat: row.seat,
            kills: row.kills,
            downs: row.downs,
            revives: row.revives,
            points_earned: row.points_earned,
        })
        .collect()
}

fn save_records(file: Res<RecordsFile>, dir: Res<SaveDir>) {
    // Freshly loaded Records are already on disk (or defaults); only changes get written.
    if !file.is_added() {
        store(&file, &dir);
    }
}

fn store(file: &RecordsFile, dir: &SaveDir) {
    if let Some(dir) = &dir.0
        && let Err(err) = save::store(dir, file)
    {
        warn!("Couldn't save Records: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::save::tests::TempDir;

    fn run(round: u32, survived_secs: f64) -> RunRecord {
        RunRecord {
            map: MapId::Greybox,
            mode: Mode::Classic,
            date: 0,
            round,
            kills: 0,
            headshots: 0,
            downs: 1,
            points_earned: 0,
            survived_secs,
            players: 1,
            revives: 0,
            teammates: Vec::new(),
            won: false,
            heat: None,
            extras: Vec::new(),
        }
    }

    fn rounds(records: &Records, mode: Mode) -> Vec<(u32, f64)> {
        records
            .board(MapId::Greybox, mode, 1)
            .map(|run| (run.round, run.survived_secs))
            .collect()
    }

    #[test]
    fn records_rank_by_round_reached() {
        let mut records = Records::default();
        records.add(run(5, 100.0));
        records.add(run(12, 900.0));
        records.add(run(8, 400.0));
        assert_eq!(
            rounds(&records, Mode::Classic),
            vec![(12, 900.0), (8, 400.0), (5, 100.0)]
        );
    }

    #[test]
    fn equal_rounds_go_to_the_shorter_time() {
        let mut records = Records::default();
        records.add(run(10, 600.0));
        assert_eq!(records.add(run(10, 550.0)), Some(0));
        assert_eq!(records.add(run(10, 700.0)), Some(2));
        assert_eq!(
            rounds(&records, Mode::Classic),
            vec![(10, 550.0), (10, 600.0), (10, 700.0)]
        );
    }

    #[test]
    fn an_exact_tie_ranks_below_the_older_run() {
        let mut records = Records::default();
        records.add(run(7, 300.0));
        assert_eq!(records.add(run(7, 300.0)), Some(1));
    }

    #[test]
    fn add_returns_the_rank_and_zero_is_the_best() {
        let mut records = Records::default();
        assert_eq!(records.add(run(3, 60.0)), Some(0));
        assert_eq!(records.add(run(2, 60.0)), Some(1));
        assert_eq!(records.add(run(4, 60.0)), Some(0));
    }

    #[test]
    fn only_the_top_ten_are_kept() {
        let mut records = Records::default();
        for round in 1..=10 {
            records.add(run(round, 0.0));
        }
        // Worse than all ten: not kept.
        assert_eq!(records.add(run(1, 5.0)), None);
        // Better than the last: kept, and Round 1 drops off.
        assert_eq!(records.add(run(6, 0.0)), Some(5));
        let board = rounds(&records, Mode::Classic);
        assert_eq!(board.len(), 10);
        assert_eq!(board.last(), Some(&(2, 0.0)));
    }

    #[test]
    fn each_map_and_mode_has_its_own_board() {
        let mut records = Records::default();
        for round in 1..=10 {
            records.add(run(round, 0.0));
        }
        let gauntlet = RunRecord {
            mode: Mode::Gauntlet,
            ..run(1, 0.0)
        };
        assert_eq!(records.add(gauntlet), Some(0));
        assert_eq!(rounds(&records, Mode::Gauntlet), vec![(1, 0.0)]);
        assert_eq!(rounds(&records, Mode::Classic).len(), 10);
    }

    #[test]
    fn a_hand_edited_list_is_sorted_and_trimmed() {
        let mut records = Records {
            runs: (1..=12).map(|round| run(round, 0.0)).collect(),
        };
        records.tidy();
        let board = rounds(&records, Mode::Classic);
        assert_eq!(board.len(), 10);
        assert_eq!(board[0], (12, 0.0));
        assert_eq!(board[9], (3, 0.0));
    }

    #[test]
    fn lifetime_stats_add_up_every_run() {
        let mut lifetime = LifetimeStats::default();
        lifetime.add(&RunRecord {
            kills: 40,
            headshots: 12,
            downs: 1,
            points_earned: 5_000,
            ..run(6, 300.5)
        });
        lifetime.add(&RunRecord {
            kills: 100,
            headshots: 30,
            downs: 2,
            points_earned: 15_000,
            ..run(11, 900.25)
        });
        assert_eq!(lifetime.runs, 2);
        assert_eq!(lifetime.kills, 140);
        assert_eq!(lifetime.headshots, 42);
        assert_eq!(lifetime.downs, 3);
        assert_eq!(lifetime.points_earned, 20_000);
        assert_eq!(lifetime.time_played_secs, 1200.75);
    }

    #[test]
    fn lifetime_keeps_the_highest_round_per_map_and_mode() {
        let mut lifetime = LifetimeStats::default();
        lifetime.add(&run(9, 0.0));
        lifetime.add(&run(4, 0.0));
        lifetime.add(&RunRecord {
            mode: Mode::Gauntlet,
            ..run(2, 0.0)
        });
        assert_eq!(lifetime.best_round(MapId::Greybox, Mode::Classic, 1), Some(9));
        assert_eq!(lifetime.best_round(MapId::Greybox, Mode::Gauntlet, 1), Some(2));
    }

    #[test]
    fn an_unplayed_map_and_mode_has_no_best_round() {
        assert_eq!(
            LifetimeStats::default().best_round(MapId::Greybox, Mode::Classic, 1),
            None
        );
    }

    #[test]
    fn lifetime_totals_saturate_instead_of_wrapping() {
        let mut lifetime = LifetimeStats {
            runs: u32::MAX,
            points_earned: u64::MAX,
            ..default()
        };
        lifetime.add(&RunRecord {
            points_earned: 10,
            ..run(1, 0.0)
        });
        assert_eq!(lifetime.runs, u32::MAX);
        assert_eq!(lifetime.points_earned, u64::MAX);
    }

    #[test]
    fn every_run_counts_toward_lifetime_even_off_the_records() {
        let mut file = RecordsFile::default();
        for round in 1..=11 {
            file.add(run(round, 0.0));
        }
        assert_eq!(file.add(run(1, 0.0)), None);
        assert_eq!(file.lifetime.runs, 12);
    }

    #[test]
    fn records_round_trip_through_the_file() {
        let dir = TempDir::new("records-round-trip");
        let mut file = RecordsFile::default();
        file.add(RunRecord {
            kills: 33,
            date: 1_761_350_400,
            extras: vec![RecordExtra {
                id: "example".to_string(),
                count: 3,
            }],
            ..run(7, 432.5)
        });
        save::store(&dir.0, &file).unwrap();
        assert_eq!(save::load::<RecordsFile>(&dir.0), file);
    }

    #[test]
    fn a_record_without_extras_still_loads() {
        let text = "(version: 1, records: (runs: [(map: Greybox, mode: Classic, date: 0, round: 3, \
                    kills: 9, headshots: 2, downs: 1, points_earned: 900, survived_secs: 61.0)]), \
                    lifetime: (runs: 0, kills: 0, headshots: 0, downs: 0, points_earned: 0, \
                    time_played_secs: 0.0, best_rounds: []))";
        let file = save::parse::<RecordsFile>(text).unwrap();
        assert_eq!(file.records.board(MapId::Greybox, Mode::Classic, 1).count(), 1);
    }

    #[test]
    fn an_old_save_loads_as_one_player_records() {
        let text = "(version: 1, records: (runs: [(map: Greybox, mode: Classic, date: 0, round: 3, \
                    kills: 9, headshots: 2, downs: 1, points_earned: 900, survived_secs: 61.0)]), \
                    lifetime: (runs: 1, kills: 9, headshots: 2, downs: 1, points_earned: 900, \
                    time_played_secs: 61.0, best_rounds: [(map: Greybox, mode: Classic, round: 3)]))";
        let file = save::parse::<RecordsFile>(text).unwrap();
        let run = file.records.board(MapId::Greybox, Mode::Classic, 1).next().unwrap();
        assert_eq!((run.players, run.revives, run.won), (1, 0, false));
        assert!(run.teammates.is_empty());
        assert_eq!(file.records.board(MapId::Greybox, Mode::Classic, 2).count(), 0);
        assert_eq!(file.lifetime.best_round(MapId::Greybox, Mode::Classic, 1), Some(3));
        assert_eq!(file.lifetime.revives, 0);
    }

    #[test]
    fn records_split_by_player_count() {
        let mut records = Records::default();
        for round in 1..=10 {
            records.add(run(round, 0.0));
        }
        let duo = RunRecord { players: 2, ..run(1, 0.0) };
        assert_eq!(records.add(duo), Some(0), "a 2-Player Run isn't ranked against solo ones");
        assert_eq!(records.board(MapId::Greybox, Mode::Classic, 1).count(), 10);
        assert_eq!(records.board(MapId::Greybox, Mode::Classic, 2).count(), 1);
        assert_eq!(records.board(MapId::Greybox, Mode::Classic, 3).count(), 0);
    }

    #[test]
    fn lifetime_best_rounds_split_by_player_count_and_revives_add_up() {
        let mut lifetime = LifetimeStats::default();
        lifetime.add(&run(9, 0.0));
        lifetime.add(&RunRecord { players: 3, revives: 4, ..run(20, 0.0) });
        lifetime.add(&RunRecord { players: 3, revives: 2, won: true, heat: Some(3), ..run(25, 0.0) });
        assert_eq!(lifetime.best_round(MapId::Greybox, Mode::Classic, 1), Some(9));
        assert_eq!(lifetime.best_round(MapId::Greybox, Mode::Classic, 3), Some(25));
        assert_eq!(lifetime.best_round(MapId::Greybox, Mode::Classic, 2), None);
        assert_eq!((lifetime.revives, lifetime.wins, lifetime.highest_heat_won), (6, 1, 3));
    }

    #[test]
    fn teammates_leave_out_the_players_own_row() {
        use crate::net::protocol::Score;
        let row = |name: &str, seat: u8| Score {
            name: name.into(),
            seat,
            kills: 5,
            downs: 1,
            revives: 2,
            points_earned: 700,
        };
        let board = [row("Host", 0), row("Ann", 1), row("Bo", 2)];
        let mine = teammates_from(&board, Some(Seat(1)));
        assert_eq!(mine.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Host", "Bo"]);
        assert_eq!(mine[0].revives, 2);
    }

    #[test]
    fn a_run_saves_one_record_however_many_times_it_ends() {
        let mut app = App::new();
        app.insert_resource(RecordsFile::default())
            .insert_resource(SaveDir(None))
            .insert_resource(MapId::Greybox)
            .insert_resource(Mode::Classic)
            .init_resource::<RecordExtras>()
            .init_resource::<RunSpecialZombies>()
            .init_resource::<RunPlayers>()
            .init_resource::<RunRecorded>()
            .init_resource::<Scoreboard>()
            .init_resource::<RunType>()
            .add_systems(Update, record_run);
        app.world_mut().spawn((LocalPlayer, Player, RunStats::default(), Points::default()));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<RecordsFile>().lifetime.runs, 1);
    }

    #[test]
    fn a_run_with_types_off_lists_them_after_the_modes_extras() {
        let switches = SpecialZombies {
            hellhounds: false,
            napalm: false,
            ..default()
        };
        assert_eq!(off_extras(&SpecialZombies::default()), [], "a faithful Run lists none");
        let mut extras = vec![RecordExtra {
            id: "boon".to_string(),
            count: 2,
        }];
        extras.extend(off_extras(&switches));
        let record = RunRecord { extras, ..run(12, 0.0) };
        assert_eq!(off_types(&record), [SpecialType::Hellhounds, SpecialType::Napalm]);
        let lines: Vec<String> = record.extras.iter().map(extra_line).collect();
        assert_eq!(lines, ["boon x2", "Hellhounds Off", "Napalm Off"]);
        assert!(off_types(&run(12, 0.0)).is_empty());
    }

    #[test]
    fn the_rank_line_calls_out_a_new_best() {
        assert_eq!(rank_line(Some(0)), "New best!");
        assert_eq!(rank_line(Some(2)), "#3 on the Records");
        assert_eq!(rank_line(None), "Outside the top 10");
    }

    #[test]
    fn times_show_minutes_and_seconds_then_hours() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.9), "1:05");
        assert_eq!(format_time(3_599.0), "59:59");
        assert_eq!(format_time(3_725.0), "1:02:05");
    }

    #[test]
    fn dates_are_utc_calendar_days() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(951_782_400), "2000-02-29");
        assert_eq!(format_date(1_761_350_400 + 86_399), "2025-10-25");
    }
}
