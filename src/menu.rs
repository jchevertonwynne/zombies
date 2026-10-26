//! The Main Menu, the Pause menu, the Settings screen they share, Records and Game Over.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button};

use crate::controls::{Action, Binding, Bindings};
use crate::hud::{ROUND_RED, full_screen, text, with_commas};
use crate::map::MapId;
use crate::mode::Mode;
use crate::records::{
    LastRun, RecordsFile, RunRecord, extra_line, format_date, format_time, off_types, rank_line,
    record_run,
};
use crate::settings::{Setting, Settings};
use crate::state::{Freeze, GameState, MainMenuScreen, Pause, PauseScreen, resume_to};

const BUTTON_IDLE: Color = Color::srgba(0.15, 0.15, 0.15, 0.9);
const BUTTON_HOVER: Color = Color::srgba(0.3, 0.08, 0.06, 0.95);
const BUTTON_PRESSED: Color = Color::srgba(0.5, 0.1, 0.06, 1.0);
pub const HINT_GREY: Color = Color::srgb(0.7, 0.7, 0.7);
pub const NEW_BEST_GOLD: Color = Color::srgb(1.0, 0.85, 0.2);
pub const MENU_BACKGROUND: Color = Color::srgb(0.02, 0.02, 0.04);
const DIM_BACKGROUND: Color = Color::srgba(0.0, 0.0, 0.0, 0.6);

/// What a menu button does when clicked.
#[derive(Component, Debug, Clone, Copy)]
pub enum MenuButton {
    /// Opens the Mode screen.
    Play,
    /// Highlights a Mode on the Mode screen, remembering it as the last Mode.
    PickMode(Mode),
    /// Starts a Run in the highlighted Mode.
    Start,
    Armory,
    Records,
    Settings,
    Quit,
    /// Back to the menu the current screen was opened from.
    Back,
    Resume,
    QuitToMainMenu,
    /// Step a Setting up (`true`) or down.
    Adjust(Setting, bool),
    /// Show this Mode's Records.
    RecordsTab(Mode),
    /// Ask to confirm a reset.
    AskReset(Reset),
    ConfirmReset(Reset),
    CancelReset,
    /// From Game Over: a new Run in the same Mode.
    PlayAgain,
    /// From Game Over.
    MainMenu,
    /// Opens the Controls page over the Settings screen.
    Controls,
    /// Waits for the key or mouse button to bind an Action to.
    Rebind(Action),
    /// Puts every binding back to its default.
    ResetControls,
    /// Closes the Controls page.
    CloseControls,
}

/// What a Settings reset wipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reset {
    /// Records & Lifetime Stats.
    Records,
    /// Gauntlet progress, leaving Records intact.
    Gauntlet,
}

impl Reset {
    fn button(self) -> &'static str {
        match self {
            Reset::Records => "Reset Records & Lifetime Stats",
            Reset::Gauntlet => "Reset Gauntlet Progress",
        }
    }

    fn warning(self) -> &'static str {
        match self {
            Reset::Records => "Every Run Record and Lifetime total will be lost.",
            Reset::Gauntlet => "All Scrip, Unlocks and Ranks will be lost. Records are kept.",
        }
    }
}

/// Sent when the Player confirms "Reset Gauntlet Progress". Gauntlet owns that progress and wipes
/// it; Records stay.
#[derive(Message)]
pub struct ResetGauntletProgress;

/// Lines a Mode layered on Classic adds to the Game Over screen, under the Run Record. Classic
/// leaves it empty; it's cleared as each Run starts.
#[derive(Resource, Default)]
pub struct GameOverLines(pub Vec<String>);

/// Gives a button the menus' hover and press colours.
#[derive(Component)]
pub struct MenuButtonLook;

/// Which Mode's Records the Records screen shows.
#[derive(Resource, Default)]
struct RecordsTab(Mode);

/// The part of the Records screen rebuilt when the tab changes.
#[derive(Component)]
struct RecordsBoard;

/// The part of the Mode screen rebuilt when the highlighted Mode changes.
#[derive(Component)]
struct ModeBoard;

/// The root of a Settings screen, which the reset confirmation opens over.
#[derive(Component)]
struct SettingsScreen;

/// A reset's "Are you sure?" confirmation.
#[derive(Component)]
struct ResetConfirmation;

/// The text showing a Setting's current value.
#[derive(Component)]
struct SettingValue(Setting);

/// The Controls page, opened over the Settings screen.
#[derive(Component)]
struct ControlsPage;

/// The text on an Action's binding button.
#[derive(Component)]
struct BindingValue(Action);

/// The prompt over the Controls page while it waits for a key, which also keeps clicks off the
/// buttons under it.
#[derive(Component)]
struct ListeningPrompt;

/// The Action the Controls page is waiting to bind, if any.
#[derive(Resource, Default)]
struct Rebinding(Option<Action>);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Mode>()
            .init_resource::<RecordsTab>()
            .init_resource::<GameOverLines>()
            .init_resource::<Rebinding>()
            .add_message::<ResetGauntletProgress>()
            .add_systems(
                OnEnter(GameState::Playing),
                |mut lines: ResMut<GameOverLines>| lines.0.clear(),
            )
            .add_systems(OnEnter(GameState::MainMenu), spawn_menu_camera)
            .add_systems(OnEnter(MainMenuScreen::Root), spawn_main_menu)
            .add_systems(OnEnter(MainMenuScreen::Mode), spawn_mode_screen)
            .add_systems(OnEnter(MainMenuScreen::Records), spawn_records)
            .add_systems(OnEnter(MainMenuScreen::Settings), spawn_main_menu_settings)
            .add_systems(OnEnter(PauseScreen::Root), spawn_pause_menu)
            .add_systems(OnEnter(PauseScreen::Settings), spawn_pause_settings)
            .add_systems(
                OnEnter(GameState::GameOver),
                spawn_game_over.after(record_run),
            )
            .add_observer(activate_button)
            .add_systems(Update, highlight_buttons)
            .add_systems(
                Update,
                refresh_records
                    .run_if(in_state(MainMenuScreen::Records))
                    .run_if(resource_changed::<RecordsTab>),
            )
            .add_systems(
                Update,
                refresh_mode_screen
                    .run_if(in_state(MainMenuScreen::Mode))
                    .run_if(resource_changed::<Settings>),
            )
            .add_systems(Update, play_again_key.run_if(in_state(GameState::GameOver)))
            .add_systems(Update, setting_values.run_if(resource_changed::<Settings>))
            // Before anything else reads the keys, so Esc closes the page instead of backing out.
            .add_systems(PreUpdate, capture_binding.after(bevy::input::InputSystems))
            .add_systems(Update, binding_values)
            .add_systems(Update, main_menu_back.run_if(in_state(GameState::MainMenu)));
    }
}

/// There's no Player camera outside a Run, and UI needs a camera to draw on.
fn spawn_menu_camera(mut commands: Commands) {
    commands.spawn((Camera2d, DespawnOnExit(GameState::MainMenu)));
}

/// A full-screen column of centred widgets. The Main Menu is opaque; Pause dims the frozen world.
pub fn screen(
    commands: &mut Commands,
    background: Color,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) -> Entity {
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(14.0),
                ..full_screen()
            },
            BackgroundColor(background),
            GlobalZIndex(10),
        ))
        .with_children(build)
        .id()
}

pub fn button(parent: &mut ChildSpawnerCommands, label: &str, action: impl Bundle, width: f32) {
    colored_button(parent, label, action, width, Color::WHITE);
}

/// A menu button whose `action` component says what a click does.
pub fn colored_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: impl Bundle,
    width: f32,
    color: Color,
) {
    parent
        .spawn((
            Button,
            Hovered::default(),
            MenuButtonLook,
            action,
            Node {
                width: px(width),
                height: px(48.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BUTTON_IDLE),
        ))
        .with_child(text(label, 26.0, color));
}

fn spawn_main_menu(mut commands: Commands) {
    let menu = screen(&mut commands, MENU_BACKGROUND, |menu| {
        menu.spawn((
            text("ZOMBIES", 96.0, ROUND_RED),
            Node {
                margin: UiRect::bottom(px(32.0)),
                ..default()
            },
        ));
        button(menu, "Play", MenuButton::Play, 280.0);
        button(menu, "Records", MenuButton::Records, 280.0);
        button(menu, "Settings", MenuButton::Settings, 280.0);
        button(menu, "Quit", MenuButton::Quit, 280.0);
    });
    commands
        .entity(menu)
        .insert(DespawnOnExit(MainMenuScreen::Root));
}

fn spawn_mode_screen(mut commands: Commands, settings: Res<Settings>) {
    let mode = settings.last_mode;
    let screen = screen(&mut commands, MENU_BACKGROUND, |screen| {
        screen.spawn((
            text("MODE", 64.0, Color::WHITE),
            Node {
                margin: UiRect::bottom(px(24.0)),
                ..default()
            },
        ));
        screen
            .spawn((
                ModeBoard,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(14.0),
                    ..default()
                },
            ))
            .with_children(|board| mode_board(board, mode));
        button(screen, "Back", MenuButton::Back, 280.0);
    });
    commands
        .entity(screen)
        .insert(DespawnOnExit(MainMenuScreen::Mode));
}

/// The Mode buttons with the highlighted one in red, what it plays like, the Armory while
/// Gauntlet is highlighted, and Start.
fn mode_board(board: &mut ChildSpawnerCommands, highlighted: Mode) {
    board
        .spawn(Node {
            column_gap: px(16.0),
            ..default()
        })
        .with_children(|row| {
            for (label, mode) in [("Classic", Mode::Classic), ("Gauntlet", Mode::Gauntlet)] {
                let color = if mode == highlighted {
                    ROUND_RED
                } else {
                    Color::WHITE
                };
                colored_button(row, label, MenuButton::PickMode(mode), 240.0, color);
            }
        });
    let blurb = match highlighted {
        Mode::Classic => "The original: survive as many Rounds as you can.",
        Mode::Gauntlet => "Classic, with a build that grows Round by Round.",
    };
    board.spawn(text(blurb, 22.0, HINT_GREY));
    if highlighted == Mode::Gauntlet {
        button(board, "Armory", MenuButton::Armory, 280.0);
    }
    colored_button(board, "Start", MenuButton::Start, 280.0, NEW_BEST_GOLD);
}

fn refresh_mode_screen(
    mut commands: Commands,
    board: Query<Entity, With<ModeBoard>>,
    settings: Res<Settings>,
) {
    let mode = settings.last_mode;
    for board in &board {
        commands
            .entity(board)
            .despawn_children()
            .with_children(|board| mode_board(board, mode));
    }
}

fn spawn_records(mut commands: Commands, mode: Res<Mode>, mut tab: ResMut<RecordsTab>) {
    // Always written, so the board below is built on this first frame.
    tab.0 = *mode;
    let records = screen(&mut commands, MENU_BACKGROUND, |records| {
        records.spawn(text("RECORDS", 48.0, Color::WHITE));
        records.spawn((
            RecordsBoard,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6.0),
                ..default()
            },
        ));
        button(records, "Back", MenuButton::Back, 280.0);
    });
    commands
        .entity(records)
        .insert(DespawnOnExit(MainMenuScreen::Records));
}

/// The Records table's columns: heading and width.
const RECORD_COLUMNS: [(&str, f32); 8] = [
    ("#", 40.0),
    ("Round", 80.0),
    ("Time", 100.0),
    ("Kills", 80.0),
    ("Headshots", 110.0),
    ("Downs", 80.0),
    ("Points Earned", 150.0),
    ("Date", 130.0),
];

fn table_row(parent: &mut ChildSpawnerCommands, cells: [String; 8], color: Color) {
    parent.spawn(Node::default()).with_children(|row| {
        for (cell, (_, width)) in cells.into_iter().zip(RECORD_COLUMNS) {
            row.spawn((
                text(cell, 20.0, color),
                Node {
                    width: px(width),
                    ..default()
                },
            ));
        }
    });
}

fn record_cells(rank: usize, run: &RunRecord) -> [String; 8] {
    [
        (rank + 1).to_string(),
        run.round.to_string(),
        format_time(run.survived_secs),
        with_commas(run.kills.into()),
        with_commas(run.headshots.into()),
        with_commas(run.downs.into()),
        with_commas(run.points_earned),
        format_date(run.date),
    ]
}

/// Rebuilds the tabs, the tab's boards and Lifetime Stats.
fn refresh_records(
    mut commands: Commands,
    board: Query<Entity, With<RecordsBoard>>,
    tab: Res<RecordsTab>,
    file: Res<RecordsFile>,
) {
    let Ok(board) = board.single() else {
        return;
    };
    let mode = tab.0;
    commands
        .entity(board)
        .despawn_children()
        .with_children(|board| {
            board
                .spawn(Node {
                    column_gap: px(12.0),
                    margin: UiRect::bottom(px(8.0)),
                    ..default()
                })
                .with_children(|tabs| {
                    for (label, tab) in [("Classic", Mode::Classic), ("Gauntlet", Mode::Gauntlet)] {
                        let color = if tab == mode { ROUND_RED } else { Color::WHITE };
                        colored_button(tabs, label, MenuButton::RecordsTab(tab), 200.0, color);
                    }
                });

            for map in MapId::ALL {
                board.spawn(text(map.name(), 28.0, Color::WHITE));
                let headings = RECORD_COLUMNS.map(|(heading, _)| heading.to_string());
                table_row(board, headings, HINT_GREY);
                let mut runs = file.records.board(map, mode).peekable();
                if runs.peek().is_none() {
                    board.spawn(text("No Records yet.", 20.0, HINT_GREY));
                }
                for (rank, run) in runs.enumerate() {
                    table_row(board, record_cells(rank, run), Color::WHITE);
                    let off = off_types(run);
                    if !off.is_empty() {
                        let labels: Vec<_> = off.iter().map(|kind| kind.label()).collect();
                        board.spawn(text(format!("Off: {}", labels.join(", ")), 16.0, HINT_GREY));
                    }
                }
            }

            let lifetime = &file.lifetime;
            board.spawn((
                text("LIFETIME STATS", 28.0, Color::WHITE),
                Node {
                    margin: UiRect::top(px(12.0)),
                    ..default()
                },
            ));
            board.spawn(text(
                format!(
                    "Runs {}   Kills {}   Headshots {}   Downs {}",
                    with_commas(lifetime.runs.into()),
                    with_commas(lifetime.kills),
                    with_commas(lifetime.headshots),
                    with_commas(lifetime.downs),
                ),
                20.0,
                Color::WHITE,
            ));
            board.spawn(text(
                format!(
                    "Points Earned {}   Time Played {}",
                    with_commas(lifetime.points_earned),
                    format_time(lifetime.time_played_secs),
                ),
                20.0,
                Color::WHITE,
            ));
            let best: Vec<_> = MapId::ALL
                .iter()
                .map(|&map| match lifetime.best_round(map, mode) {
                    Some(round) => format!("{} {round}", map.name()),
                    None => format!("{} -", map.name()),
                })
                .collect();
            board.spawn(text(
                format!("Highest Round: {}", best.join("   ")),
                20.0,
                Color::WHITE,
            ));
        });
}

fn spawn_game_over(mut commands: Commands, last: Res<LastRun>, more: Res<GameOverLines>) {
    let LastRun { record, rank } = &*last;
    let screen = screen(&mut commands, DIM_BACKGROUND, |screen| {
        screen.spawn(text("GAME OVER", 80.0, ROUND_RED));
        screen.spawn(text(
            format!("You reached Round {}", record.round),
            40.0,
            Color::WHITE,
        ));
        let rank_color = if *rank == Some(0) {
            NEW_BEST_GOLD
        } else {
            HINT_GREY
        };
        screen.spawn(text(rank_line(*rank), 32.0, rank_color));
        screen
            .spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(4.0),
                margin: UiRect::vertical(px(12.0)),
                ..default()
            })
            .with_children(|lines| {
                lines.spawn(text(
                    format!(
                        "Kills {}   Headshots {}   Downs {}",
                        with_commas(record.kills.into()),
                        with_commas(record.headshots.into()),
                        with_commas(record.downs.into()),
                    ),
                    24.0,
                    Color::WHITE,
                ));
                lines.spawn(text(
                    format!(
                        "Points Earned {}   Time {}",
                        with_commas(record.points_earned),
                        format_time(record.survived_secs),
                    ),
                    24.0,
                    Color::WHITE,
                ));
                for extra in &record.extras {
                    lines.spawn(text(extra_line(extra), 20.0, Color::WHITE));
                }
                for line in &more.0 {
                    lines.spawn(text(line, 24.0, NEW_BEST_GOLD));
                }
                lines.spawn(text(
                    format!(
                        "{}  ·  {:?}  ·  {}",
                        record.map.name(),
                        record.mode,
                        format_date(record.date)
                    ),
                    20.0,
                    HINT_GREY,
                ));
            });
        button(screen, "Play Again", MenuButton::PlayAgain, 280.0);
        button(screen, "Main Menu", MenuButton::MainMenu, 280.0);
    });
    commands
        .entity(screen)
        .insert(DespawnOnExit(GameState::GameOver));
}

/// Enter plays again from Game Over, as it always has.
fn play_again_key(input: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<GameState>>) {
    if input.just_pressed(KeyCode::Enter) {
        next.set(GameState::Playing);
    }
}

fn spawn_pause_menu(mut commands: Commands) {
    let menu = screen(&mut commands, DIM_BACKGROUND, |menu| {
        menu.spawn((
            text("PAUSED", 64.0, Color::WHITE),
            Node {
                margin: UiRect::bottom(px(24.0)),
                ..default()
            },
        ));
        button(menu, "Resume", MenuButton::Resume, 320.0);
        button(menu, "Settings", MenuButton::Settings, 320.0);
        button(menu, "Quit to Main Menu", MenuButton::QuitToMainMenu, 320.0);
    });
    commands
        .entity(menu)
        .insert(DespawnOnExit(PauseScreen::Root));
}

fn settings_screen(commands: &mut Commands, settings: &Settings, background: Color) -> Entity {
    screen(commands, background, |screen| {
        screen.spawn((
            text("SETTINGS", 64.0, Color::WHITE),
            Node {
                margin: UiRect::bottom(px(24.0)),
                ..default()
            },
        ));
        // The general Settings on the left, the Special Zombies group on the right.
        screen
            .spawn(Node {
                column_gap: px(48.0),
                align_items: AlignItems::FlexStart,
                ..default()
            })
            .with_children(|columns| {
                settings_column(columns, settings, None, &Setting::ALL);
                settings_column(
                    columns,
                    settings,
                    Some("Special Zombies"),
                    &Setting::SPECIAL_ZOMBIES,
                );
            });
        screen.spawn(Node {
            height: px(16.0),
            ..default()
        });
        button(screen, "Controls", MenuButton::Controls, 440.0);
        for reset in [Reset::Records, Reset::Gauntlet] {
            button(screen, reset.button(), MenuButton::AskReset(reset), 440.0);
        }
        button(screen, "Back", MenuButton::Back, 280.0);
    })
}

/// A column of Settings rows, under an optional group heading.
fn settings_column(
    parent: &mut ChildSpawnerCommands,
    settings: &Settings,
    heading: Option<&str>,
    rows: &[Setting],
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(14.0),
            ..default()
        })
        .with_children(|column| {
            if let Some(heading) = heading {
                column.spawn(text(heading, 26.0, HINT_GREY));
            }
            for &setting in rows {
                setting_row(column, settings, setting);
            }
        });
}

/// One Setting: its label, "<", its value and ">".
fn setting_row(parent: &mut ChildSpawnerCommands, settings: &Settings, setting: Setting) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                text(setting.label(), 26.0, Color::WHITE),
                Node {
                    width: px(220.0),
                    ..default()
                },
            ));
            button(row, "<", MenuButton::Adjust(setting, false), 48.0);
            row.spawn((Node {
                width: px(110.0),
                justify_content: JustifyContent::Center,
                ..default()
            },))
                .with_child((
                    SettingValue(setting),
                    text(settings.display(setting), 26.0, Color::WHITE),
                ));
            button(row, ">", MenuButton::Adjust(setting, true), 48.0);
        });
}

/// Opens the reset confirmation over the Settings screen.
fn spawn_reset_confirmation(commands: &mut Commands, settings_screen: Entity, reset: Reset) {
    commands.entity(settings_screen).with_children(|parent| {
        parent
            .spawn((
                ResetConfirmation,
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: px(14.0),
                    ..full_screen()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
                GlobalZIndex(20),
            ))
            .with_children(|confirm| {
                confirm.spawn(text(format!("{}?", reset.button()), 40.0, Color::WHITE));
                confirm.spawn((
                    text(reset.warning(), 24.0, HINT_GREY),
                    Node {
                        margin: UiRect::bottom(px(16.0)),
                        ..default()
                    },
                ));
                colored_button(
                    confirm,
                    "Reset",
                    MenuButton::ConfirmReset(reset),
                    280.0,
                    ROUND_RED,
                );
                button(confirm, "Cancel", MenuButton::CancelReset, 280.0);
            });
    });
}

/// Opens the Controls page over the Settings screen: every Action and the key it's on.
fn spawn_controls_page(commands: &mut Commands, settings_screen: Entity, bindings: &Bindings) {
    commands.entity(settings_screen).with_children(|parent| {
        parent
            .spawn((
                ControlsPage,
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: px(10.0),
                    ..full_screen()
                },
                // Opaque: the Settings rows under it would read through any dimming.
                BackgroundColor(MENU_BACKGROUND),
                GlobalZIndex(20),
            ))
            .with_children(|page| {
                page.spawn(text("CONTROLS", 52.0, Color::WHITE));
                page.spawn((
                    text("Click a binding, then press the key or mouse button to put it on.", 20.0, HINT_GREY),
                    Node {
                        margin: UiRect::bottom(px(8.0)),
                        ..default()
                    },
                ));
                page.spawn(Node {
                    column_gap: px(48.0),
                    ..default()
                })
                .with_children(|columns| {
                    let (left, right) = Action::ALL.split_at(Action::ALL.len().div_ceil(2));
                    for actions in [left, right] {
                        columns
                            .spawn(Node {
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8.0),
                                ..default()
                            })
                            .with_children(|column| {
                                for &action in actions {
                                    binding_row(column, action, bindings.get(action));
                                }
                            });
                    }
                });
                page.spawn(Node {
                    column_gap: px(16.0),
                    margin: UiRect::top(px(12.0)),
                    ..default()
                })
                .with_children(|row| {
                    button(row, "Reset to Defaults", MenuButton::ResetControls, 280.0);
                    button(row, "Back", MenuButton::CloseControls, 200.0);
                });
            });
    });
}

/// One Action: its label and a button showing what it's bound to.
fn binding_row(parent: &mut ChildSpawnerCommands, action: Action, binding: Binding) {
    parent
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(12.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                text(action.label(), 22.0, Color::WHITE),
                Node {
                    width: px(200.0),
                    ..default()
                },
            ));
            row.spawn((
                Button,
                Hovered::default(),
                MenuButtonLook,
                MenuButton::Rebind(action),
                Node {
                    width: px(170.0),
                    height: px(36.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(BUTTON_IDLE),
            ))
            .with_child((BindingValue(action), text(binding.name(), 22.0, Color::WHITE)));
        });
}

/// Shows the waiting prompt over the Controls page.
fn spawn_listening_prompt(commands: &mut Commands, page: Entity, action: Action) {
    commands.entity(page).with_children(|parent| {
        parent
            .spawn((
                ListeningPrompt,
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: px(12.0),
                    ..full_screen()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                GlobalZIndex(30),
            ))
            .with_children(|prompt| {
                prompt.spawn(text(format!("Press a key or mouse button for {}", action.label()), 32.0, Color::WHITE));
                prompt.spawn(text("Esc to cancel", 22.0, HINT_GREY));
            });
    });
}

/// While the Controls page is open, Esc closes it (or cancels a rebind) rather than backing out
/// of Settings. While it waits, the next bindable key press, or mouse button release, is bound.
/// A mouse button binds on release, so letting go of it doesn't click a button underneath.
fn capture_binding(
    mut commands: Commands,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pages: Query<Entity, With<ControlsPage>>,
    prompts: Query<Entity, With<ListeningPrompt>>,
    mut rebinding: ResMut<Rebinding>,
    mut settings: ResMut<Settings>,
) {
    let Ok(page) = pages.single() else {
        if rebinding.0.is_some() {
            rebinding.0 = None;
        }
        return;
    };
    let stop_listening = |commands: &mut Commands, rebinding: &mut Rebinding| {
        rebinding.0 = None;
        for prompt in &prompts {
            commands.entity(prompt).despawn();
        }
    };
    if keys.clear_just_pressed(KeyCode::Escape) {
        if rebinding.0.is_some() {
            stop_listening(&mut commands, &mut rebinding);
        } else {
            commands.entity(page).despawn();
        }
        return;
    }
    let Some(action) = rebinding.0 else { return };
    let key = keys.get_just_pressed().copied().find(|&key| Binding::bindable_key(key));
    let pressed = key.map(Binding::Key).or_else(|| {
        mouse.get_just_released().copied().find(|&button| Binding::bindable_mouse(button)).map(Binding::Mouse)
    });
    if let Some(binding) = pressed {
        if let Some(key) = key {
            keys.clear_just_pressed(key);
        }
        settings.bindings.set(action, binding);
        stop_listening(&mut commands, &mut rebinding);
    }
}

/// Binding buttons show their key, or that they're waiting for one.
fn binding_values(settings: Res<Settings>, rebinding: Res<Rebinding>, mut values: Query<(&BindingValue, &mut Text)>) {
    for (BindingValue(action), mut text) in &mut values {
        let shown = if rebinding.0 == Some(*action) {
            "...".to_string()
        } else {
            settings.bindings.get(*action).name().to_string()
        };
        if text.0 != shown {
            text.0 = shown;
        }
    }
}

fn spawn_main_menu_settings(mut commands: Commands, settings: Res<Settings>) {
    let screen = settings_screen(&mut commands, &settings, MENU_BACKGROUND);
    commands
        .entity(screen)
        .insert((SettingsScreen, DespawnOnExit(MainMenuScreen::Settings)));
}

fn spawn_pause_settings(mut commands: Commands, settings: Res<Settings>) {
    let screen = settings_screen(&mut commands, &settings, DIM_BACKGROUND);
    commands
        .entity(screen)
        .insert((SettingsScreen, DespawnOnExit(PauseScreen::Settings)));
}

/// A button was clicked (pressed and released over it).
fn activate_button(
    activate: On<Activate>,
    mut commands: Commands,
    buttons: Query<&MenuButton>,
    settings_screen: Query<Entity, With<SettingsScreen>>,
    confirmation: Query<Entity, With<ResetConfirmation>>,
    mut mode: ResMut<Mode>,
    (mut tab, mut records): (ResMut<RecordsTab>, ResMut<RecordsFile>),
    (controls_page, mut rebinding): (Query<Entity, With<ControlsPage>>, ResMut<Rebinding>),
    mut reset_gauntlet: MessageWriter<ResetGauntletProgress>,
    main_menu: Option<Res<State<MainMenuScreen>>>,
    mut settings: ResMut<Settings>,
    mut next_game: ResMut<NextState<GameState>>,
    mut next_main_menu: ResMut<NextState<MainMenuScreen>>,
    (mut next_pause, freeze): (ResMut<NextState<Pause>>, Res<Freeze>),
    mut next_pause_screen: ResMut<NextState<PauseScreen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let main_menu = main_menu.map(|screen| *screen.get());
    let in_main_menu = main_menu.is_some();
    let Ok(action) = buttons.get(activate.entity) else {
        return;
    };
    match *action {
        MenuButton::Play => next_main_menu.set(MainMenuScreen::Mode),
        MenuButton::PickMode(picked) => {
            if settings.last_mode != picked {
                settings.last_mode = picked;
            }
        }
        MenuButton::Start => {
            *mode = settings.last_mode;
            next_game.set(GameState::Playing);
        }
        MenuButton::Armory => next_main_menu.set(MainMenuScreen::Armory),
        // The Mode stays as it was.
        MenuButton::PlayAgain => next_game.set(GameState::Playing),
        MenuButton::MainMenu => next_game.set(GameState::MainMenu),
        MenuButton::Records => next_main_menu.set(MainMenuScreen::Records),
        MenuButton::Settings if in_main_menu => next_main_menu.set(MainMenuScreen::Settings),
        MenuButton::Settings => next_pause_screen.set(PauseScreen::Settings),
        MenuButton::Back if in_main_menu => next_main_menu.set(back_from(main_menu)),
        MenuButton::Back => next_pause_screen.set(PauseScreen::Root),
        MenuButton::Quit => {
            exit.write(AppExit::Success);
        }
        MenuButton::Resume => next_pause.set(resume_to(&freeze)),
        MenuButton::QuitToMainMenu => next_game.set(GameState::MainMenu),
        MenuButton::Adjust(setting, up) => settings.adjust(setting, up),
        MenuButton::RecordsTab(mode) => tab.0 = mode,
        MenuButton::Controls => {
            if controls_page.is_empty()
                && let Ok(screen) = settings_screen.single()
            {
                spawn_controls_page(&mut commands, screen, &settings.bindings);
            }
        }
        MenuButton::Rebind(action) => {
            if rebinding.0.is_none()
                && let Ok(page) = controls_page.single()
            {
                rebinding.0 = Some(action);
                spawn_listening_prompt(&mut commands, page, action);
            }
        }
        MenuButton::ResetControls => {
            if settings.bindings != Bindings::default() {
                settings.bindings = Bindings::default();
            }
        }
        MenuButton::CloseControls => {
            for page in &controls_page {
                commands.entity(page).despawn();
            }
        }
        MenuButton::AskReset(reset) => {
            if confirmation.is_empty()
                && let Ok(screen) = settings_screen.single()
            {
                spawn_reset_confirmation(&mut commands, screen, reset);
            }
        }
        MenuButton::ConfirmReset(_) | MenuButton::CancelReset => {
            match *action {
                MenuButton::ConfirmReset(Reset::Records) => *records = RecordsFile::default(),
                MenuButton::ConfirmReset(Reset::Gauntlet) => {
                    reset_gauntlet.write(ResetGauntletProgress);
                }
                _ => {}
            }
            for entity in &confirmation {
                commands.entity(entity).despawn();
            }
        }
    }
}

fn highlight_buttons(
    mut buttons: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor), With<MenuButtonLook>>,
) {
    for (hovered, pressed, mut background) in &mut buttons {
        let color = match (pressed, hovered.get()) {
            (true, _) => BUTTON_PRESSED,
            (false, true) => BUTTON_HOVER,
            (false, false) => BUTTON_IDLE,
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

fn setting_values(settings: Res<Settings>, mut values: Query<(&SettingValue, &mut Text)>) {
    for (SettingValue(setting), mut text) in &mut values {
        text.0 = settings.display(*setting);
    }
}

/// The Main Menu screen Back (or Esc) leads to: the Armory backs out to the Mode screen, the
/// rest to the Main Menu itself.
fn back_from(screen: Option<MainMenuScreen>) -> MainMenuScreen {
    match screen {
        Some(MainMenuScreen::Armory) => MainMenuScreen::Mode,
        _ => MainMenuScreen::Root,
    }
}

/// Esc backs out of the Main Menu's other screens. (Pause's Esc lives in `state`.)
fn main_menu_back(
    input: Res<ButtonInput<KeyCode>>,
    screen: Res<State<MainMenuScreen>>,
    mut next: ResMut<NextState<MainMenuScreen>>,
) {
    if input.just_pressed(KeyCode::Escape) && *screen.get() != MainMenuScreen::Root {
        next.set(back_from(Some(*screen.get())));
    }
}
