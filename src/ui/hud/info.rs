//! The Info Screen: held on the Info key (Tab by default), it lays the Run's standing over the
//! middle of the screen. Map, Mode, Heat (Gauntlet), Round, time survived, and a row per Player
//! with their Seat colour, Points and whether they are up, in Last Stand or Bled Out. Once the Run
//! is Tainted it adds one line saying so; before that it says nothing about developer mode.
//!
//! It reads only what every machine already has, so it is the same solo and in Co-op and adds
//! nothing to the wire. It shows at any time in a Run (Last Stand, spectating, Intermission, with an
//! Offer open), sits below the Offer and the Pause menu, and hides while the Pause menu is open.
//! The world keeps running and the Player keeps control.

use bevy::prelude::*;

use super::{column, full_screen, row, text, with_commas};
use crate::maps::map::MapId;
use crate::players::controls::{Action, Binding};
use crate::players::last_stand::LastStand;
use crate::players::player::{LocalPlayer, Player, PlayerName, Seat};
use crate::players::respawn::AwaitingRespawn;
use crate::economy::points::Points;
use crate::run::mode::Mode;
use crate::run::round::Round;
use crate::run::stats::RunStats;
use crate::state::{GameState, InRun, Pause};
use crate::storage::records::{RunHeatRecord, RunTainted, format_time};
use crate::storage::settings::Settings;

/// Over the HUD, under the Pause menu (10) and any Offer (15, 20).
const INFO_Z: i32 = 5;
const GREY: Color = Color::srgb(0.7, 0.7, 0.7);
const DOWN_RED: Color = Color::srgb(1.0, 0.2, 0.15);
const BLED_OUT_GREY: Color = Color::srgb(0.5, 0.5, 0.5);

/// The line a Tainted Run adds.
pub const TAINT_LINE: &str = "Developer mode used: this Run won't be recorded";

/// Where a Player stands, as their row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Up,
    LastStand,
    BledOut,
}

impl Standing {
    /// Bleeding out outranks the Last Stand a Player was in.
    pub fn of(last_stand: bool, awaiting_respawn: bool) -> Self {
        if awaiting_respawn {
            Standing::BledOut
        } else if last_stand {
            Standing::LastStand
        } else {
            Standing::Up
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Standing::Up => "up",
            Standing::LastStand => "Last Stand",
            Standing::BledOut => "Bled Out",
        }
    }

    fn colour(self) -> Color {
        match self {
            Standing::Up => Color::WHITE,
            Standing::LastStand => DOWN_RED,
            Standing::BledOut => BLED_OUT_GREY,
        }
    }
}

/// One Player's row.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoRow {
    pub seat: Seat,
    pub name: String,
    pub points: u64,
    pub standing: Standing,
}

/// Everything the Info Screen shows.
#[derive(Debug, Clone, PartialEq)]
pub struct InfoView {
    pub map: MapId,
    pub mode: Mode,
    /// The Heat, shown for a Gauntlet Run only.
    pub heat: u32,
    pub round: u32,
    /// Whole seconds survived.
    pub survived_secs: u64,
    pub players: Vec<InfoRow>,
    pub tainted: bool,
}

impl InfoView {
    /// The lines over the Player rows: Map and Mode (with Heat for a Gauntlet Run), Round and time
    /// survived, then the taint line if the Run is Tainted.
    pub fn header(&self) -> Vec<String> {
        let mode = match self.mode {
            Mode::Classic => "Classic".to_string(),
            Mode::Gauntlet => format!("Gauntlet · Heat {}", self.heat),
        };
        vec![
            format!("{} · {mode}", self.map.name()),
            format!("Round {} · {}", self.round, format_time(self.survived_secs as f64)),
        ]
    }
}

/// The Info Screen's root, a full-screen layer shown while the Info key is held.
#[derive(Component)]
struct InfoScreen;

/// What the root last showed, so the rows are rebuilt only on a change.
#[derive(Default)]
struct Shown(Option<InfoView>);

fn spawn_info(mut commands: Commands) {
    commands.spawn((
        InfoScreen,
        InRun,
        GlobalZIndex(INFO_Z),
        Node {
            display: Display::None,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..full_screen()
        },
    ));
}

/// Whether the Info key is down. Read raw rather than through `Controls`, which lets go while an
/// Offer is open: the Info Screen still shows then.
fn held(settings: &Settings, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    match settings.bindings.get(Action::Info) {
        Binding::Key(key) => keys.pressed(key),
        Binding::Mouse(button) => mouse.pressed(button),
    }
}

#[allow(clippy::too_many_arguments)]
fn show_info(
    mut commands: Commands,
    (settings, keys, mouse): (Res<Settings>, Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    pause: Option<Res<State<Pause>>>,
    (map, mode, round, heat, tainted): (Res<MapId>, Res<Mode>, Option<Res<Round>>, Res<RunHeatRecord>, Res<RunTainted>),
    local: Single<&RunStats, With<LocalPlayer>>,
    players: Query<(&Seat, &PlayerName, &Points, Has<LastStand>, Has<AwaitingRespawn>), With<Player>>,
    mut screen: Single<(Entity, &mut Node), With<InfoScreen>>,
    mut shown: Local<Shown>,
) {
    let paused = pause.is_some_and(|pause| *pause.get() == Pause::Paused);
    if paused || !held(&settings, &keys, &mouse) {
        if screen.1.display != Display::None {
            screen.1.display = Display::None;
        }
        return;
    }
    let mut rows: Vec<InfoRow> = players
        .iter()
        .map(|(&seat, name, points, last_stand, awaiting)| InfoRow {
            seat,
            name: name.0.clone(),
            points: points.balance,
            standing: Standing::of(last_stand, awaiting),
        })
        .collect();
    rows.sort_by_key(|row| row.seat.0);
    let view = InfoView {
        map: *map,
        mode: *mode,
        heat: heat.heat.unwrap_or(0),
        round: round.map_or(0, |round| round.number),
        survived_secs: local.survived_secs.max(0.0) as u64,
        players: rows,
        tainted: tainted.0,
    };
    if screen.1.display != Display::Flex {
        screen.1.display = Display::Flex;
    }
    if shown.0.as_ref() == Some(&view) {
        return;
    }
    commands.entity(screen.0).despawn_children().with_children(|root| {
        root.spawn((
            Node { padding: UiRect::all(px(24.0)), ..column(8.0, AlignItems::Center) },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|panel| {
            for (i, line) in view.header().into_iter().enumerate() {
                panel.spawn(text(line, if i == 0 { 26.0 } else { 20.0 }, Color::WHITE));
            }
            for player in &view.players {
                panel.spawn(row(14.0)).with_children(|row| {
                    row.spawn(text(player.name.clone(), 20.0, player.seat.colour()));
                    row.spawn(text(with_commas(player.points), 20.0, player.seat.colour()));
                    row.spawn(text(player.standing.text(), 16.0, player.standing.colour()));
                });
            }
            if view.tainted {
                panel.spawn(text(TAINT_LINE, 16.0, GREY));
            }
        });
    });
    shown.0 = Some(view);
}

pub struct InfoPlugin;

impl Plugin for InfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_info)
            .add_systems(Update, show_info.run_if(in_state(GameState::Playing)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::controls::Bindings;

    fn view(mode: Mode) -> InfoView {
        InfoView {
            map: MapId::Quarry,
            mode,
            heat: 3,
            round: 7,
            survived_secs: 125,
            players: vec![],
            tainted: false,
        }
    }

    #[test]
    fn the_header_names_map_mode_round_and_time() {
        assert_eq!(view(Mode::Classic).header(), ["Quarry · Classic", "Round 7 · 2:05"]);
    }

    #[test]
    fn heat_shows_for_a_gauntlet_run_only() {
        assert_eq!(view(Mode::Gauntlet).header()[0], "Quarry · Gauntlet · Heat 3");
        assert!(!view(Mode::Classic).header().join(" ").contains("Heat"));
    }

    #[test]
    fn nothing_about_developer_mode_shows_before_the_taint() {
        let untainted = view(Mode::Classic);
        assert!(untainted.header().iter().all(|line| !line.to_lowercase().contains("developer")));
        assert_eq!(TAINT_LINE, "Developer mode used: this Run won't be recorded");
    }

    #[test]
    fn bleeding_out_outranks_the_last_stand() {
        assert_eq!(Standing::of(false, false), Standing::Up);
        assert_eq!(Standing::of(true, false), Standing::LastStand);
        assert_eq!(Standing::of(true, true), Standing::BledOut);
        assert_eq!(Standing::of(false, true), Standing::BledOut);
    }

    #[test]
    fn the_info_control_defaults_to_tab_and_rebinds() {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.get(Action::Info), Binding::Key(KeyCode::Tab));
        bindings.set(Action::Info, Binding::Key(KeyCode::KeyI));
        assert_eq!(bindings.get(Action::Info), Binding::Key(KeyCode::KeyI));
    }

    /// An app with the Info Screen over one Player, the Info key not yet held.
    fn app(tainted: bool) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<GameState>()
            .add_sub_state::<Pause>()
            .insert_resource(Settings::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .insert_resource(MapId::Greybox)
            .insert_resource(Mode::Classic)
            .insert_resource(RunHeatRecord::default())
            .insert_resource(RunTainted(tainted))
            .add_systems(Startup, spawn_info)
            .add_systems(Update, show_info);
        app.world_mut().spawn((
            LocalPlayer,
            Player,
            Seat(0),
            PlayerName("Dempsey".into()),
            Points { balance: 1500, ..default() },
            RunStats::default(),
        ));
        app.world_mut().spawn((Player, Seat(1), PlayerName("Nikolai".into()), Points::default(), AwaitingRespawn));
        app
    }

    fn texts(app: &mut App) -> Vec<String> {
        app.world_mut().query::<&Text>().iter(app.world()).map(|text| text.0.clone()).collect()
    }

    #[test]
    fn it_shows_while_held_and_lists_every_player() {
        let mut app = app(false);
        app.update();
        assert!(texts(&mut app).is_empty(), "nothing shows until the key is held");
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Tab);
        app.update();
        app.update();
        let shown = texts(&mut app);
        for want in ["Greybox · Classic", "Dempsey", "1,500", "up", "Nikolai", "Bled Out"] {
            assert!(shown.iter().any(|line| line == want), "{want} in {shown:?}");
        }
        assert!(shown.iter().all(|line| !line.contains("Developer")));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::Tab);
        app.update();
        let display = app.world_mut().query_filtered::<&Node, With<InfoScreen>>().single(app.world()).unwrap().display;
        assert_eq!(display, Display::None);
    }

    #[test]
    fn a_tainted_run_adds_the_line() {
        let mut app = app(true);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Tab);
        app.update();
        app.update();
        assert!(texts(&mut app).iter().any(|line| line == TAINT_LINE));
    }

    #[test]
    fn the_pause_menu_hides_it() {
        let mut app = app(false);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Tab);
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
        app.update();
        app.world_mut().resource_mut::<NextState<Pause>>().set(Pause::Paused);
        app.update();
        app.update();
        let display = app.world_mut().query_filtered::<&Node, With<InfoScreen>>().single(app.world()).unwrap().display;
        assert_eq!(display, Display::None);
    }
}
