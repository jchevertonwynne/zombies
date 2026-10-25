//! The Main Menu, the Pause menu and the Settings screen they share.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{Activate, Button};

use crate::hud::{ROUND_RED, full_screen, text};
use crate::settings::{Setting, Settings};
use crate::state::{GameState, MainMenuScreen, Pause, PauseScreen};

const BUTTON_IDLE: Color = Color::srgba(0.15, 0.15, 0.15, 0.9);
const BUTTON_HOVER: Color = Color::srgba(0.3, 0.08, 0.06, 0.95);
const BUTTON_PRESSED: Color = Color::srgba(0.5, 0.1, 0.06, 1.0);
const HINT_GREY: Color = Color::srgb(0.7, 0.7, 0.7);

/// What a menu button does when clicked.
#[derive(Component, Debug, Clone, Copy)]
enum MenuButton {
    Play,
    Records,
    Settings,
    Quit,
    /// Back to the menu the current screen was opened from.
    Back,
    Resume,
    QuitToMainMenu,
    /// Step a Setting up (`true`) or down.
    Adjust(Setting, bool),
}

/// The text showing a Setting's current value.
#[derive(Component)]
struct SettingValue(Setting);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::MainMenu), spawn_menu_camera)
            .add_systems(OnEnter(MainMenuScreen::Root), spawn_main_menu)
            .add_systems(OnEnter(MainMenuScreen::Records), spawn_records)
            .add_systems(OnEnter(MainMenuScreen::Settings), spawn_main_menu_settings)
            .add_systems(OnEnter(PauseScreen::Root), spawn_pause_menu)
            .add_systems(OnEnter(PauseScreen::Settings), spawn_pause_settings)
            .add_observer(activate_button)
            .add_systems(Update, highlight_buttons)
            .add_systems(Update, setting_values.run_if(resource_changed::<Settings>))
            .add_systems(Update, main_menu_back.run_if(in_state(GameState::MainMenu)));
    }
}

/// There's no Player camera outside a Run, and UI needs a camera to draw on.
fn spawn_menu_camera(mut commands: Commands) {
    commands.spawn((Camera2d, DespawnOnExit(GameState::MainMenu)));
}

/// A full-screen column of centred widgets. The Main Menu is opaque; Pause dims the frozen world.
fn screen(
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

fn button(parent: &mut ChildSpawnerCommands, label: &str, action: MenuButton, width: f32) {
    parent
        .spawn((
            Button,
            Hovered::default(),
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
        .with_child(text(label, 26.0, Color::WHITE));
}

fn spawn_main_menu(mut commands: Commands) {
    let menu = screen(&mut commands, Color::srgb(0.02, 0.02, 0.04), |menu| {
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

fn spawn_records(mut commands: Commands) {
    let records = screen(&mut commands, Color::srgb(0.02, 0.02, 0.04), |records| {
        records.spawn(text("RECORDS", 64.0, Color::WHITE));
        records.spawn((
            text("No Records yet. Coming soon.", 24.0, HINT_GREY),
            Node {
                margin: UiRect::vertical(px(24.0)),
                ..default()
            },
        ));
        button(records, "Back", MenuButton::Back, 280.0);
    });
    commands
        .entity(records)
        .insert(DespawnOnExit(MainMenuScreen::Records));
}

fn spawn_pause_menu(mut commands: Commands) {
    let menu = screen(&mut commands, Color::srgba(0.0, 0.0, 0.0, 0.6), |menu| {
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
        for setting in Setting::ALL {
            screen
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
        screen.spawn(Node {
            height: px(16.0),
            ..default()
        });
        button(screen, "Back", MenuButton::Back, 280.0);
    })
}

fn spawn_main_menu_settings(mut commands: Commands, settings: Res<Settings>) {
    let screen = settings_screen(&mut commands, &settings, Color::srgb(0.02, 0.02, 0.04));
    commands
        .entity(screen)
        .insert(DespawnOnExit(MainMenuScreen::Settings));
}

fn spawn_pause_settings(mut commands: Commands, settings: Res<Settings>) {
    let screen = settings_screen(&mut commands, &settings, Color::srgba(0.0, 0.0, 0.0, 0.6));
    commands
        .entity(screen)
        .insert(DespawnOnExit(PauseScreen::Settings));
}

/// A button was clicked (pressed and released over it).
fn activate_button(
    activate: On<Activate>,
    buttons: Query<&MenuButton>,
    game: Res<State<GameState>>,
    mut settings: ResMut<Settings>,
    mut next_game: ResMut<NextState<GameState>>,
    mut next_main_menu: ResMut<NextState<MainMenuScreen>>,
    mut next_pause: ResMut<NextState<Pause>>,
    mut next_pause_screen: ResMut<NextState<PauseScreen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let in_main_menu = *game.get() == GameState::MainMenu;
    let Ok(action) = buttons.get(activate.entity) else {
        return;
    };
    match *action {
        // Straight into a Classic Run until the Mode screen exists.
        MenuButton::Play => next_game.set(GameState::Playing),
        MenuButton::Records => next_main_menu.set(MainMenuScreen::Records),
        MenuButton::Settings if in_main_menu => next_main_menu.set(MainMenuScreen::Settings),
        MenuButton::Settings => next_pause_screen.set(PauseScreen::Settings),
        MenuButton::Back if in_main_menu => next_main_menu.set(MainMenuScreen::Root),
        MenuButton::Back => next_pause_screen.set(PauseScreen::Root),
        MenuButton::Quit => {
            exit.write(AppExit::Success);
        }
        MenuButton::Resume => next_pause.set(Pause::Running),
        MenuButton::QuitToMainMenu => next_game.set(GameState::MainMenu),
        MenuButton::Adjust(setting, up) => settings.adjust(setting, up),
    }
}

fn highlight_buttons(
    mut buttons: Query<(&Hovered, Has<Pressed>, &mut BackgroundColor), With<MenuButton>>,
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

/// Esc backs out of the Main Menu's Records and Settings screens. (Pause's Esc lives in `state`.)
fn main_menu_back(
    input: Res<ButtonInput<KeyCode>>,
    screen: Res<State<MainMenuScreen>>,
    mut next: ResMut<NextState<MainMenuScreen>>,
) {
    if input.just_pressed(KeyCode::Escape) && *screen.get() != MainMenuScreen::Root {
        next.set(MainMenuScreen::Root);
    }
}
