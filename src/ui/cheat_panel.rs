//! The Cheat Panel: the floating panel of Cheat buttons that the backtick key opens and closes
//! (ADR 0010). It shows nothing in a normal Run. Open, it frees the cursor and stops the Player's
//! input like a menu (`PanelOpen`), while the world runs on. Under Host only a client's panel has
//! every button disabled and says so; under Off the key does nothing.
//!
//! The Host's Developer mode choice for a Co-op Run in progress is a row of the Pause menu
//! (`DevModeButton`), since under Off there is no panel to put it in.

use bevy::prelude::*;
use bevy::ui_widgets::Activate;
use bevy::window::{CursorGrabMode, CursorOptions};

use crate::authority::Authority;
use crate::players::player::LocalPlayer;
use crate::run::dev::{Cheat, CheatRequest, Cheats, DevAccess, DevMode, SetDevMode};
use crate::run::mode::Mode;
use crate::state::{Choosing, GameState, PanelOpen, Pause};
use crate::ui::hud::{text, ROUND_RED};
use crate::ui::menu::{HINT_GREY, button};

/// The panel on screen, and what it was built for.
#[derive(Component)]
struct CheatPanel {
    dev: DevMode,
    host: bool,
    /// The Player's toggles, as `Cheats::toggles` has them, so the buttons show them.
    toggles: u8,
    elite: bool,
    gauntlet: bool,
}

/// The panel's Elite tick: the Spawn buttons ask for an Elite while it is on. Gauntlet's.
#[derive(Resource, Default)]
struct SpawnElite(bool);

/// The Elite tick's button.
#[derive(Component)]
struct EliteTick;

/// A Cheat button: pressing it asks for the Cheat.
#[derive(Component, Clone, Copy)]
struct CheatButton(Cheat);

/// The Pause menu's button for the Host to change Developer mode during a Co-op Run.
#[derive(Component)]
pub struct DevModeButton;

/// The Pause menu's Developer mode row for the Host.
pub fn dev_mode_button(menu: &mut ChildSpawnerCommands, access: DevMode) {
    button(menu, &dev_mode_label(access), DevModeButton, 320.0);
}

fn dev_mode_label(access: DevMode) -> String {
    format!("Developer mode: {}", access.label())
}

pub struct CheatPanelPlugin;

impl Plugin for CheatPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpawnElite>()
            .add_observer(press_cheat)
            .add_observer(press_elite_tick)
            .add_observer(press_dev_mode)
            .add_systems(
                Update,
                (toggle_panel, sync_panel).chain().run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, refresh_dev_mode_label.run_if(resource_changed::<DevAccess>));
    }
}

/// Backtick opens or closes the panel, unless Developer mode is Off. It opens only over a running
/// world with nothing else open.
fn toggle_panel(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    access: Res<DevAccess>,
    pause: Res<State<Pause>>,
    local: Query<(Entity, Has<PanelOpen>, Has<Choosing>), With<LocalPlayer>>,
) {
    if !keys.just_pressed(KeyCode::Backquote) || access.0 == DevMode::Off {
        return;
    }
    let Ok((player, open, choosing)) = local.single() else { return };
    if open {
        commands.entity(player).remove::<PanelOpen>();
    } else if *pause.get() == Pause::Running && !choosing {
        commands.entity(player).insert(PanelOpen);
    }
}

/// Keeps the panel as `PanelOpen` and Developer mode say: built when it opens, rebuilt when the
/// setting or who the Player is to it changes, and gone when it closes, when the setting goes Off,
/// or when anything else takes the screen.
fn sync_panel(
    mut commands: Commands,
    access: Res<DevAccess>,
    authority: Res<State<Authority>>,
    pause: Res<State<Pause>>,
    elite: Res<SpawnElite>,
    mode: Option<Res<Mode>>,
    local: Query<(Entity, Has<PanelOpen>, Has<Choosing>, Option<&Cheats>), With<LocalPlayer>>,
    panels: Query<(Entity, &CheatPanel)>,
    mut cursor: Query<&mut CursorOptions>,
) {
    let Ok((player, open, choosing, cheats)) = local.single() else { return };
    let toggles = Cheats::toggles(cheats);
    let running = *pause.get() == Pause::Running;
    let want = open && access.0 != DevMode::Off && running && !choosing;
    if open && !want {
        commands.entity(player).remove::<PanelOpen>();
    }
    let host = *authority.get() == Authority::Host;
    let shown = panels.iter().next();
    let gauntlet = mode.is_some_and(|mode| *mode == Mode::Gauntlet);
    match (want, shown) {
        (true, None) => {
            spawn_panel(&mut commands, access.0, host, cheats, elite.0, gauntlet);
            set_cursor(&mut cursor, true);
        }
        (true, Some((panel, shown)))
            if shown.dev != access.0
                || shown.host != host
                || shown.toggles != toggles
                || shown.elite != elite.0
                || shown.gauntlet != gauntlet =>
        {
            commands.entity(panel).despawn();
            spawn_panel(&mut commands, access.0, host, cheats, elite.0, gauntlet);
        }
        (false, Some((panel, _))) => {
            commands.entity(panel).despawn();
            // Back to the game, unless Pause or an Offer has the pointer.
            if running && !choosing {
                set_cursor(&mut cursor, false);
            }
        }
        _ => {}
    }
}

fn set_cursor(cursor: &mut Query<&mut CursorOptions>, free: bool) {
    for mut cursor in cursor {
        cursor.visible = free;
        cursor.grab_mode = if free { CursorGrabMode::None } else { CursorGrabMode::Locked };
    }
}

/// A Cheat button's words: a toggle says whether it is on.
fn cheat_label(cheat: Cheat, cheats: Option<&Cheats>) -> String {
    if cheat.is_toggle() {
        let on = cheats.is_some_and(|cheats| cheats.on(cheat));
        format!("{}: {}", cheat.label(), if on { "ON" } else { "OFF" })
    } else {
        cheat.label().to_string()
    }
}

fn spawn_panel(commands: &mut Commands, dev: DevMode, host: bool, cheats: Option<&Cheats>, elite: bool, gauntlet: bool) {
    let allowed = dev.allows(host);
    commands
        .spawn((
            CheatPanel { dev, host, toggles: Cheats::toggles(cheats), elite, gauntlet },
            DespawnOnExit(GameState::Playing),
            GlobalZIndex(12),
            Node {
                position_type: PositionType::Absolute,
                right: px(24.0),
                top: px(24.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10.0),
                padding: UiRect::all(px(16.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.8)),
        ))
        .with_children(|panel| {
            panel.spawn(text("CHEAT PANEL", 28.0, Color::WHITE));
            if !allowed {
                panel.spawn(text("Host only", 22.0, ROUND_RED));
            }
            for cheat in Cheat::ALL.into_iter().chain(Cheat::WORLD) {
                cheat_button(panel, &cheat_label(cheat, cheats), cheat, allowed, 260.0);
            }
            panel.spawn(text("Spawn ahead", 22.0, Color::WHITE));
            panel
                .spawn(Node {
                    width: px(400.0),
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: px(6.0),
                    row_gap: px(6.0),
                    ..default()
                })
                .with_children(|grid| {
                    for cheat in Cheat::spawns() {
                        // The Bosses are Gauntlet's.
                        let here = gauntlet || !matches!(cheat, Cheat::Spawn(spawn) if spawn.kind.gauntlet_only());
                        cheat_button(grid, cheat.label(), cheat, allowed && here, 126.0);
                    }
                });
            let label = if elite { "Elite: on" } else { "Elite: off" };
            if allowed && gauntlet {
                button(panel, label, EliteTick, 260.0);
            } else {
                dead_button(panel, label, 260.0);
            }
            panel.spawn(text("` to close", 18.0, HINT_GREY));
        });
}

/// A Cheat's button, or a greyed one that does nothing when `enabled` is false.
fn cheat_button(parent: &mut ChildSpawnerCommands, label: &str, cheat: Cheat, enabled: bool, width: f32) {
    if enabled {
        button(parent, label, CheatButton(cheat), width);
    } else {
        dead_button(parent, label, width);
    }
}

fn dead_button(parent: &mut ChildSpawnerCommands, label: &str, width: f32) {
    parent
        .spawn((
            Node {
                width: px(width),
                height: px(48.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.6)),
        ))
        .with_child(text(label, 22.0, Color::srgb(0.4, 0.4, 0.4)));
}

/// The Elite tick was pressed.
fn press_elite_tick(activate: On<Activate>, ticks: Query<(), With<EliteTick>>, mut elite: ResMut<SpawnElite>) {
    if ticks.contains(activate.entity) {
        elite.0 = !elite.0;
    }
}

/// A Cheat button was pressed: the `LocalPlayer` asks for it. The Host judges it (solo, this
/// machine is the Host).
fn press_cheat(
    activate: On<Activate>,
    buttons: Query<&CheatButton>,
    local: Query<Entity, With<LocalPlayer>>,
    elite: Res<SpawnElite>,
    mut requests: MessageWriter<CheatRequest>,
) {
    let (Ok(&CheatButton(mut cheat)), Ok(player)) = (buttons.get(activate.entity), local.single()) else { return };
    if let Cheat::Spawn(spawn) = &mut cheat {
        spawn.elite = elite.0;
    }
    requests.write(CheatRequest { player, cheat });
}

/// The Host pressed the Pause menu's Developer mode row: on to the next setting.
fn press_dev_mode(
    activate: On<Activate>,
    buttons: Query<(), With<DevModeButton>>,
    access: Res<DevAccess>,
    mut sets: MessageWriter<SetDevMode>,
) {
    if buttons.contains(activate.entity) {
        sets.write(SetDevMode(access.0.next()));
    }
}

/// The Pause menu's Developer mode row shows the setting as it now is.
fn refresh_dev_mode_label(
    access: Res<DevAccess>,
    rows: Query<&Children, With<DevModeButton>>,
    mut texts: Query<&mut Text>,
) {
    for children in &rows {
        for &child in children {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = dev_mode_label(access.0);
            }
        }
    }
}
