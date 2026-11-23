//! The worst-case benchmark: a late Round at the Alive Cap, in Co-op, with effects running, timed
//! frame by frame at 1080p. `ZOMBIES_BENCH=<map>` (greybox, quarry, village, dam, cove, polder or sanatorium) turns it on: the
//! menu is skipped, four Players stand at their spawns and can't be hurt, the local one turns
//! slowly and holds the trigger, and after a warm-up the frame times are measured, reported and
//! the game quits. Build with `--release`, and run with a scratch `HOME` so no Run Record is kept.
//!
//! To get a number worth trusting: keep the window frontmost and visible for the whole ~105 s (don't
//! click away, don't let the display sleep or another window cover it), close other heavy programs
//! (builds, browsers, other benches), and let the machine cool. The bench checks for all of this
//! and ends its line with `verdict=ok` or `verdict=UNTRUSTED(...)`: a frame time pinned at a
//! display refresh interval (the OS capping a window that isn't frontmost), a window that lost focus
//! or was occluded, other processes using a quarter of a core or more at the start or end, or a CPU
//! speed limit from thermal pressure. An UNTRUSTED run says nothing about the game's own speed: rerun it.
//!
//! The Co-op is local: the other three Players are bodies with nothing driving them, so the cost
//! of the network itself isn't in the figure. Everything else a Co-op Run draws and simulates is.

use std::process::Command;
use std::time::Duration;

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow, WindowOccluded};

use crate::enemies::zombie::Zombie;
use crate::maps::map::MapId;
use crate::players::player::{Health, LocalPlayer, Player, Seat, SpawnPlayers, player_body};
use crate::run::mode::Mode;
use crate::run::round::Round;
use crate::state::GameState;
use crate::weapons::weapon::Loadout;

/// The Round the benchmark plays: past the Alive Cap's growth to its most, with a Horde far bigger
/// than the measurement can thin.
const ROUND: u32 = 30;
/// Seconds before measuring, for the Horde to fill the Alive Cap.
const WARM_UP_SECS: f32 = 45.0;
/// Seconds measured.
const MEASURE_SECS: f32 = 60.0;
/// How fast the local Player turns, in radians a second, so the view sweeps the whole horde.
const TURN_RATE: f32 = 0.35;
/// What the Players' Health is held at.
const UNHURT: f32 = 1.0e7;
/// The frame time the performance bar asks for, 60 fps.
const BUDGET: Duration = Duration::from_micros(16_667);
/// Display refresh intervals, in ms, that a frame time pinned to means the OS is pacing the window:
/// 120 Hz, 60 Hz and 30 Hz.
const REFRESH_INTERVALS_MS: [f64; 3] = [1000.0 / 120.0, 1000.0 / 60.0, 1000.0 / 30.0];
/// How close the median must be to a refresh interval to count as pinned, in ms.
const PIN_TOLERANCE_MS: f64 = 0.1;
/// How tight the middle 90% of frames must be to count as pinned, in ms. A real game run that
/// happens to sit near 16.7 ms still wobbles by more than this.
const PIN_SPREAD_MS: f64 = 1.0;
/// Another process using this much of one core (percent) is heavy enough to disturb the bench.
const HEAVY_CPU_PERCENT: f32 = 25.0;
/// The share of measured frames that may be unfocused or occluded before the run is distrusted.
const MAX_UNSEEN_SHARE: f64 = 0.02;

/// The benchmark's settings, present only when it was asked for.
#[derive(Resource, Clone, Copy)]
pub struct Bench {
    map: MapId,
}

impl Bench {
    /// The benchmark asked for by `ZOMBIES_BENCH`, if any. An unknown Map name is a mistake worth
    /// stopping for.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var("ZOMBIES_BENCH").ok()?;
        let map = match name.to_ascii_lowercase().as_str() {
            "greybox" => MapId::Greybox,
            "quarry" => MapId::Quarry,
            "village" => MapId::Village,
            "dam" => MapId::Dam,
            "cove" => MapId::Cove,
            "polder" => MapId::Polder,
            "sanatorium" => MapId::Sanatorium,
            other => panic!("ZOMBIES_BENCH={other}: expected greybox, quarry, village, dam, cove, polder or sanatorium"),
        };
        Some(Self { map })
    }
}

#[derive(Resource, Default)]
struct Measure {
    /// Real seconds since the Run began.
    elapsed: f32,
    round_set: bool,
    frames: Vec<Duration>,
    /// The Zombies in play, summed over the measured frames.
    zombies: u64,
    most_zombies: usize,
    /// Measured frames in which the window wasn't focused.
    unfocused: usize,
    /// Measured frames in which the window was occluded (covered or minimised).
    occluded: usize,
    is_occluded: bool,
    /// Other processes that were heavy at the start, and the thermal limit then.
    start_load: Vec<Busy>,
    start_thermal: Option<u32>,
}

pub struct BenchPlugin(pub Bench);

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0)
            .init_resource::<Measure>()
            .add_systems(Startup, (set_window, check_start_conditions))
            .add_systems(OnEnter(GameState::MainMenu), start_run)
            .add_systems(OnEnter(GameState::Playing), spawn_teammates.in_set(SpawnPlayers))
            .add_systems(PreUpdate, hold_trigger.after(InputSystems))
            .add_systems(
                Update,
                (set_round, keep_unhurt, keep_loaded, turn, measure).run_if(in_state(GameState::Playing)),
            );
    }
}

/// 1920×1080 physical pixels whatever the display's scale, without vsync, so the frame time is
/// the game's own and not the display's.
fn set_window(mut window: Single<&mut Window, With<PrimaryWindow>>) {
    window.resolution.set_scale_factor_override(Some(1.0));
    window.resolution.set(1920.0, 1080.0);
    window.present_mode = PresentMode::AutoNoVsync;
}

/// Warns up front about other heavy processes and thermal pressure, and remembers them for the verdict.
fn check_start_conditions(mut measure: ResMut<Measure>) {
    measure.start_load = busy_processes();
    measure.start_thermal = thermal_speed_limit();
    for busy in &measure.start_load {
        eprintln!("bench WARNING at start: {busy} is using a lot of CPU; close it for a trustworthy number");
    }
    if let Some(limit) = measure.start_thermal.filter(|&l| l < 100) {
        eprintln!("bench WARNING at start: CPU speed limit is {limit}% (thermal); let the machine cool");
    }
    eprintln!("bench: keep this window frontmost and visible, and the machine idle, for ~105 s");
}

fn start_run(
    bench: Res<Bench>,
    mut map: ResMut<MapId>,
    mut mode: ResMut<Mode>,
    mut next: ResMut<NextState<GameState>>,
) {
    *map = bench.map;
    *mode = Mode::Classic;
    next.set(GameState::Playing);
}

/// Seats 1 to 3, beside the solo Player in Seat 0, so the Run is Co-op from its start.
fn spawn_teammates(mut commands: Commands, map: Res<MapId>) {
    for seat in 1..4 {
        commands.spawn(player_body(Seat(seat), &format!("Player {}", seat + 1), &[], *map));
    }
}

/// Jumps the first Round straight to `ROUND`, once it has begun.
fn set_round(mut commands: Commands, mut measure: ResMut<Measure>, round: Option<Res<Round>>) {
    if !measure.round_set && round.is_some() {
        commands.insert_resource(Round::for_bench(ROUND, 4));
        measure.round_set = true;
    }
}

fn keep_unhurt(mut players: Query<&mut Health, With<Player>>) {
    for mut health in &mut players {
        health.max = UNHURT;
        health.current = UNHURT;
    }
}

fn keep_loaded(mut loadout: Single<&mut Loadout, With<LocalPlayer>>) {
    let held = loadout.held_mut();
    if held.reserve < 1000 {
        held.reserve = 1000;
    }
}

fn hold_trigger(mut mouse: ResMut<ButtonInput<MouseButton>>) {
    mouse.press(MouseButton::Left);
}

fn turn(time: Res<Time<Real>>, mut player: Single<&mut Transform, With<LocalPlayer>>) {
    player.rotate_y(TURN_RATE * time.delta_secs());
}

fn measure(
    time: Res<Time<Real>>,
    bench: Res<Bench>,
    mut measure: ResMut<Measure>,
    zombies: Query<(), With<Zombie>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut occlusions: MessageReader<WindowOccluded>,
    mut exit: MessageWriter<AppExit>,
) {
    for event in occlusions.read() {
        measure.is_occluded = event.occluded;
    }
    measure.elapsed += time.delta_secs();
    if measure.elapsed < WARM_UP_SECS {
        return;
    }
    if measure.elapsed < WARM_UP_SECS + MEASURE_SECS {
        let alive = zombies.iter().count();
        measure.frames.push(time.delta());
        measure.zombies += alive as u64;
        measure.most_zombies = measure.most_zombies.max(alive);
        measure.unfocused += usize::from(!window.focused);
        measure.occluded += usize::from(measure.is_occluded);
        return;
    }
    let conditions = Conditions {
        unfocused: measure.unfocused,
        occluded: measure.occluded,
        start_load: measure.start_load.clone(),
        end_load: busy_processes(),
        start_thermal: measure.start_thermal,
        end_thermal: thermal_speed_limit(),
    };
    for busy in &conditions.end_load {
        eprintln!("bench WARNING at end: {busy} is using a lot of CPU");
    }
    let report = report(bench.map, &measure, &conditions);
    println!("{report}");
    if let Ok(path) = std::env::var("ZOMBIES_BENCH_OUT")
        && let Err(error) = std::fs::write(&path, format!("{report}\n")) {
            eprintln!("couldn't write {path}: {error}");
        }
    exit.write(AppExit::Success);
}

/// What the machine was doing around the measurement.
struct Conditions {
    unfocused: usize,
    occluded: usize,
    start_load: Vec<Busy>,
    end_load: Vec<Busy>,
    start_thermal: Option<u32>,
    end_thermal: Option<u32>,
}

/// A process using a lot of CPU.
#[derive(Clone, Debug, PartialEq)]
struct Busy {
    name: String,
    percent: f32,
}

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:.0}%)", self.name, self.percent)
    }
}

/// The reasons a run can't be trusted, empty when it can. `frames_ms` is ascending.
fn untrusted_reasons(frames_ms: &[f64], conditions: &Conditions) -> Vec<String> {
    let mut reasons = Vec::new();
    if let Some(interval) = pinned_to_refresh(frames_ms) {
        reasons.push(format!("frame times pinned at the {interval:.2} ms display refresh, so the OS is pacing the window"));
    }
    let n = frames_ms.len().max(1) as f64;
    if conditions.unfocused as f64 / n > MAX_UNSEEN_SHARE {
        reasons.push(format!("window unfocused for {} of {} frames", conditions.unfocused, frames_ms.len()));
    }
    if conditions.occluded as f64 / n > MAX_UNSEEN_SHARE {
        reasons.push(format!("window occluded for {} of {} frames", conditions.occluded, frames_ms.len()));
    }
    for (when, load) in [("start", &conditions.start_load), ("end", &conditions.end_load)] {
        if !load.is_empty() {
            let names: Vec<String> = load.iter().map(Busy::to_string).collect();
            reasons.push(format!("busy processes at {when}: {}", names.join(", ")));
        }
    }
    for limit in [conditions.start_thermal, conditions.end_thermal].into_iter().flatten() {
        if limit < 100 {
            reasons.push(format!("thermal CPU speed limit {limit}%"));
            break;
        }
    }
    reasons
}

/// The refresh interval the frame times are pinned to, if they are: the median within
/// `PIN_TOLERANCE_MS` of one, and the middle 90% of frames within `PIN_SPREAD_MS` of each other.
/// `sorted_ms` is ascending.
fn pinned_to_refresh(sorted_ms: &[f64]) -> Option<f64> {
    if sorted_ms.len() < 20 {
        return None;
    }
    let at = |q: f64| sorted_ms[((sorted_ms.len() - 1) as f64 * q).round() as usize];
    if at(0.95) - at(0.05) > PIN_SPREAD_MS {
        return None;
    }
    let p50 = at(0.5);
    REFRESH_INTERVALS_MS.into_iter().find(|i| (p50 - i).abs() <= PIN_TOLERANCE_MS)
}

/// Other processes using at least `HEAVY_CPU_PERCENT` of a core, from `ps`. Empty where `ps`
/// isn't there.
fn busy_processes() -> Vec<Busy> {
    let Ok(output) = Command::new("ps").args(["-A", "-o", "pid=,pcpu=,comm="]).output() else {
        return Vec::new();
    };
    parse_busy(&String::from_utf8_lossy(&output.stdout), std::process::id())
}

/// Parses `ps -o pid=,pcpu=,comm=` lines, skipping `own_pid` and `ps` itself.
fn parse_busy(text: &str, own_pid: u32) -> Vec<Busy> {
    let mut busy: Vec<Busy> = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid: u32 = parts.next()?.parse().ok()?;
            let percent: f32 = parts.next()?.parse().ok()?;
            let path = parts.collect::<Vec<_>>().join(" ");
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            (pid != own_pid && percent >= HEAVY_CPU_PERCENT && name != "ps").then_some(Busy { name, percent })
        })
        .collect();
    busy.sort_by(|a, b| b.percent.total_cmp(&a.percent));
    busy.truncate(5);
    busy
}

/// The CPU speed limit in percent macOS reports under thermal pressure, where it does.
fn thermal_speed_limit() -> Option<u32> {
    let output = Command::new("pmset").args(["-g", "therm"]).output().ok()?;
    parse_speed_limit(&String::from_utf8_lossy(&output.stdout))
}

fn parse_speed_limit(text: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "CPU_Speed_Limit").then(|| value.trim().parse().ok()).flatten()
    })
}

fn report(map: MapId, measure: &Measure, conditions: &Conditions) -> String {
    let mut frames = measure.frames.clone();
    frames.sort();
    let count = frames.len().max(1);
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |q: f64| ms(frames[((count - 1) as f64 * q).round() as usize]);
    let total: Duration = frames.iter().sum();
    let mean = ms(total) / count as f64;
    let sorted_ms: Vec<f64> = frames.iter().map(|&d| ms(d)).collect();
    let reasons = untrusted_reasons(&sorted_ms, conditions);
    let verdict = if reasons.is_empty() { "ok".to_string() } else { format!("UNTRUSTED({})", reasons.join("; ")) };
    let over = frames.iter().filter(|&&d| d > BUDGET).count();
    format!(
        "bench map={map:?} round={ROUND} frames={} mean_ms={mean:.2} fps={:.1} p50_ms={:.2} p95_ms={:.2} p99_ms={:.2} max_ms={:.2} over_16.7ms={:.1}% zombies_mean={:.1} zombies_max={} verdict={verdict}",
        frames.len(),
        1000.0 / mean,
        at(0.5),
        at(0.95),
        at(0.99),
        at(1.0),
        100.0 * over as f64 / count as f64,
        measure.zombies as f64 / count as f64,
        measure.most_zombies,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calm() -> Conditions {
        Conditions { unfocused: 0, occluded: 0, start_load: vec![], end_load: vec![], start_thermal: Some(100), end_thermal: None }
    }

    fn ramp(from: f64, to: f64) -> Vec<f64> {
        (0..1000).map(|i| from + (to - from) * i as f64 / 999.0).collect()
    }

    #[test]
    fn frames_pinned_at_60_hz_are_untrusted() {
        let frames = ramp(16.66, 16.68);
        assert!(pinned_to_refresh(&frames).is_some());
        assert_eq!(untrusted_reasons(&frames, &calm()).len(), 1);
    }

    #[test]
    fn a_genuinely_fast_or_wobbly_run_is_not_pinned() {
        assert!(pinned_to_refresh(&ramp(4.0, 11.8)).is_none());
        assert!(pinned_to_refresh(&ramp(14.0, 19.5)).is_none());
        assert!(untrusted_reasons(&ramp(4.0, 11.8), &calm()).is_empty());
    }

    #[test]
    fn pinning_at_120_and_30_hz_is_caught() {
        assert!(pinned_to_refresh(&ramp(8.30, 8.37)).is_some());
        assert!(pinned_to_refresh(&ramp(33.3, 33.4)).is_some());
    }

    #[test]
    fn too_few_frames_are_never_called_pinned() {
        assert!(pinned_to_refresh(&[16.67; 5]).is_none());
    }

    #[test]
    fn focus_load_and_heat_are_each_reported() {
        let frames = ramp(4.0, 11.8);
        let mut c = calm();
        c.unfocused = 100;
        c.occluded = 100;
        c.end_load = vec![Busy { name: "cargo".into(), percent: 300.0 }];
        c.end_thermal = Some(80);
        assert_eq!(untrusted_reasons(&frames, &c).len(), 4);
        c.unfocused = 5;
        c.occluded = 0;
        assert_eq!(untrusted_reasons(&frames, &c).len(), 2);
    }

    #[test]
    fn busy_processes_skip_ourselves_and_the_idle() {
        let text = "  1  0.0 /sbin/launchd\n 10 85.5 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n 11 99.0 /x/zombies\n 12 30.0 ps\n 13 24.9 /usr/bin/foo\n";
        let busy = parse_busy(text, 11);
        assert_eq!(busy, vec![Busy { name: "Google Chrome".into(), percent: 85.5 }]);
    }

    #[test]
    fn the_speed_limit_is_read_from_pmset() {
        let text = "Note: No thermal warning level has been recorded\nCPU_Scheduler_Limit \t= 100\nCPU_Available_CPUs \t= 8\nCPU_Speed_Limit \t= 75\n";
        assert_eq!(parse_speed_limit(text), Some(75));
        assert_eq!(parse_speed_limit("nothing"), None);
    }
}
