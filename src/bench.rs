//! The worst-case benchmark: a late Round at the Alive Cap, in Co-op, with effects running, timed
//! frame by frame at 1080p. `ZOMBIES_BENCH=<map>` (greybox, quarry, village or dam) turns it on: the
//! menu is skipped, four Players stand at their spawns and can't be hurt, the local one turns
//! slowly and holds the trigger, and after a warm-up the frame times are measured, reported and
//! the game quits. Build with `--release`, and run with a scratch `HOME` so no Run Record is kept.
//!
//! The Co-op is local: the other three Players are bodies with nothing driving them, so the cost
//! of the network itself isn't in the figure. Everything else a Co-op Run draws and simulates is.

use std::time::Duration;

use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow};

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
            other => panic!("ZOMBIES_BENCH={other}: expected greybox, quarry, village or dam"),
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
}

pub struct BenchPlugin(pub Bench);

impl Plugin for BenchPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0)
            .init_resource::<Measure>()
            .add_systems(Startup, set_window)
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
    mut exit: MessageWriter<AppExit>,
) {
    measure.elapsed += time.delta_secs();
    if measure.elapsed < WARM_UP_SECS {
        return;
    }
    if measure.elapsed < WARM_UP_SECS + MEASURE_SECS {
        let alive = zombies.iter().count();
        measure.frames.push(time.delta());
        measure.zombies += alive as u64;
        measure.most_zombies = measure.most_zombies.max(alive);
        return;
    }
    let report = report(bench.map, &measure);
    println!("{report}");
    if let Ok(path) = std::env::var("ZOMBIES_BENCH_OUT") {
        if let Err(error) = std::fs::write(&path, format!("{report}\n")) {
            eprintln!("couldn't write {path}: {error}");
        }
    }
    exit.write(AppExit::Success);
}

fn report(map: MapId, measure: &Measure) -> String {
    let mut frames = measure.frames.clone();
    frames.sort();
    let count = frames.len().max(1);
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |q: f64| ms(frames[((count - 1) as f64 * q).round() as usize]);
    let total: Duration = frames.iter().sum();
    let mean = ms(total) / count as f64;
    let over = frames.iter().filter(|&&d| d > BUDGET).count();
    format!(
        "bench map={map:?} round={ROUND} frames={} mean_ms={mean:.2} fps={:.1} p50_ms={:.2} p95_ms={:.2} p99_ms={:.2} max_ms={:.2} over_16.7ms={:.1}% zombies_mean={:.1} zombies_max={}",
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
