//! Co-op's menus: the Co-op screen, the Name prompt, Join and the Room.
//!
//! The Room's rules live in `net::room` and the lobby's calls in `net::lobby`; this is the
//! screens over them. A Room is open only while the Room screen is, or while the Armory
//! opened from it is: leaving either closes it. Solo never builds any of this.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use crate::gauntlet::progress::GauntletFile;
use crate::hud::{ROUND_RED, text};
use crate::map::MapId;
use crate::menu::{
    BackTo, HINT_GREY, MENU_BACKGROUND, MenuButtonLook, NEW_BEST_GOLD, button, colored_button,
    screen,
};
use crate::mode::Mode;
use crate::net::lobby::{Listed, Listing, LobbyClient, Status, read_status_line, status_line};
use crate::net::link::Link;
use crate::net::protocol::{Refusal, RoomView, TrackState, Unlocks, clean_name};
use crate::net::room::{Hosting, Joined, Joining, Room};
use crate::net::transport::Transport;
use crate::player::Seat;
use crate::settings::Settings;
use crate::state::MainMenuScreen;
use crate::tuning::*;

const BUTTON_IDLE: Color = Color::srgba(0.15, 0.15, 0.15, 0.9);
const READY_GREEN: Color = Color::srgb(0.45, 0.9, 0.45);

/// The Room this machine is in, if it is in one.
#[derive(Resource, Default)]
pub enum CoopSession {
    #[default]
    Off,
    Host(Box<Hosting<Box<dyn Link>>>),
    Join(Box<Joining<Box<dyn Link>>>),
}

impl CoopSession {
    /// Whether the Host has this Player down as Ready.
    fn is_ready(&self) -> bool {
        self.view().is_some_and(|(view, seat)| {
            view.seats[usize::from(seat)]
                .as_ref()
                .is_some_and(|seat| seat.ready)
        })
    }

    fn is_off(&self) -> bool {
        matches!(self, CoopSession::Off)
    }

    /// The Room as this Player sees it, and the Seat they sit in.
    fn view(&self) -> Option<(RoomView, u8)> {
        match self {
            CoopSession::Off => None,
            CoopSession::Host(hosting) => Some((hosting.room.view(), 0)),
            CoopSession::Join(joining) => match &joining.state {
                Joined::In { seat, view: Some(view) } => Some((view.clone(), *seat)),
                _ => None,
            },
        }
    }
}

/// What the Co-op and Join screens say went wrong or happened: a refusal and its reason, "Host
/// closed the room".
#[derive(Resource, Default)]
struct Notice(String);

/// The Room the Join screen is asking a password for.
#[derive(Resource, Default)]
struct JoinTarget(Option<Listed>);

/// Whether the Join screen is waiting on a code it asked the lobby about.
#[derive(Resource, Default)]
struct Finding(bool);

/// The text input being typed into.
#[derive(Resource, Default)]
struct Typing(Option<Entity>);

/// How long until the Join screen asks the lobby for the list afresh.
#[derive(Resource, Default)]
struct ListTimer(f32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputKind {
    Name,
    Code,
    JoinPassword,
    RoomPassword,
}

/// A box of text the Player types into.
#[derive(Component)]
struct TextInput {
    kind: InputKind,
    value: String,
    max: usize,
    hint: &'static str,
    /// Made upper case as it is typed.
    upper: bool,
    /// Typed into as soon as it appears.
    autofocus: bool,
}

/// What a Co-op button does when clicked.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
enum CoopButton {
    Host,
    Join,
    /// Looks up the code typed on the Join screen.
    Find,
    Refresh,
    /// The Room in this row of the list.
    Pick(usize),
    /// Joins the Room asking for a password, with the one typed.
    JoinLocked,
    CancelPassword,
    /// Keeps the Name typed and moves on.
    Continue,
    ChangeName,
    /// Starts typing into a text input.
    Focus,
    ToggleMode,
    CycleMap,
    ToggleListed,
    SetPassword,
    Ready,
    Kick(u8),
    Armory,
    Leave,
}

/// Something a Player did that the screens act on together.
#[derive(Message)]
struct Do(CoopButton);

/// The Room's text and buttons that change as the Room does.
#[derive(Component, Clone, Copy, PartialEq)]
enum RoomLabel {
    Code,
    Summary,
    Mode,
    Map,
    Listed,
    Ready,
    Lobby,
}

/// The Room's four Seat rows, rebuilt when the Room changes.
#[derive(Component)]
struct RoomBoard;

/// The Armory button, shown only in a Gauntlet Room.
#[derive(Component)]
struct ArmoryRow;

/// The Join screen's list, rebuilt when the lobby's answer changes.
#[derive(Component)]
struct JoinBoard;

/// The Join screen's password prompt, shown while a locked Room is picked.
#[derive(Component)]
struct PasswordPrompt;

#[derive(Component)]
struct NoticeText;

pub struct CoopPlugin;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CoopSession>()
            .init_resource::<LobbyClient>()
            .init_resource::<Notice>()
            .init_resource::<JoinTarget>()
            .init_resource::<Finding>()
            .init_resource::<Typing>()
            .init_resource::<ListTimer>()
            .add_message::<Do>()
            .add_observer(click)
            .add_systems(OnEnter(MainMenuScreen::Coop), spawn_coop)
            .add_systems(OnEnter(MainMenuScreen::Name), spawn_name)
            .add_systems(OnEnter(MainMenuScreen::Join), spawn_join)
            .add_systems(OnEnter(MainMenuScreen::Room), spawn_room)
            .add_systems(
                Update,
                (
                    focus_new_input,
                    type_text,
                    show_inputs,
                    run_actions,
                    pump_session,
                    keep_lobby,
                    close_unused_session,
                    refresh_notice,
                    (refresh_join, found_room, relist).run_if(in_state(MainMenuScreen::Join)),
                    refresh_room.run_if(in_state(MainMenuScreen::Room)),
                    resend_armory.run_if(resource_changed::<GauntletFile>),
                )
                    .chain(),
            )
            .add_systems(Last, farewell);
    }
}

/// What a Player brings from their own Armory.
pub fn armory_of(file: &GauntletFile) -> Unlocks {
    Unlocks {
        unlocked: file.unlocked.clone(),
        tracks: file
            .tracks
            .iter()
            .map(|track| TrackState {
                id: track.id.clone(),
                ranks: track.ranks,
                on: track.on,
            })
            .collect(),
    }
}

// Screens

fn spawn_coop(mut commands: Commands, settings: Res<Settings>) {
    let name = settings.name.clone();
    let coop = screen(&mut commands, MENU_BACKGROUND, |coop| {
        coop.spawn(text("CO-OP", 64.0, Color::WHITE));
        coop.spawn(text(format!("Playing as {name}"), 24.0, HINT_GREY));
        coop.spawn((NoticeText, text("", 24.0, ROUND_RED)));
        button(coop, "Host", CoopButton::Host, 280.0);
        button(coop, "Join", CoopButton::Join, 280.0);
        button(coop, "Change Name", CoopButton::ChangeName, 280.0);
        button(coop, "Back", crate::menu::MenuButton::Back, 280.0);
    });
    commands
        .entity(coop)
        .insert(DespawnOnExit(MainMenuScreen::Coop));
}

fn spawn_name(mut commands: Commands, settings: Res<Settings>) {
    let current = settings.name.clone();
    let name = screen(&mut commands, MENU_BACKGROUND, |name| {
        name.spawn(text("YOUR NAME", 64.0, Color::WHITE));
        name.spawn(text(
            format!("Up to {PLAYER_NAME_MAX_CHARS} characters. Teammates see it over your head."),
            22.0,
            HINT_GREY,
        ));
        input(name, InputKind::Name, &current, "Type a name", PLAYER_NAME_MAX_CHARS, 440.0, true);
        colored_button(name, "Continue", CoopButton::Continue, 280.0, NEW_BEST_GOLD);
        button(name, "Back", crate::menu::MenuButton::Back, 280.0);
    });
    commands
        .entity(name)
        .insert(DespawnOnExit(MainMenuScreen::Name));
}

fn spawn_join(mut commands: Commands, lobby: Res<LobbyClient>, mut timer: ResMut<ListTimer>) {
    lobby.list();
    timer.0 = LOBBY_LIST_SECS;
    let join = screen(&mut commands, MENU_BACKGROUND, |join| {
        join.spawn(text("JOIN", 64.0, Color::WHITE));
        join.spawn((NoticeText, text("", 24.0, ROUND_RED)));
        join.spawn(Node {
            column_gap: px(12.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            input(row, InputKind::Code, "", "Room code", 5, 240.0, false);
            button(row, "Find", CoopButton::Find, 160.0);
            button(row, "Refresh", CoopButton::Refresh, 200.0);
        });
        join.spawn((
            PasswordPrompt,
            Node {
                column_gap: px(12.0),
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|row| {
            input(
                row,
                InputKind::JoinPassword,
                "",
                "Password",
                ROOM_PASSWORD_MAX_CHARS,
                320.0,
                false,
            );
            button(row, "Join", CoopButton::JoinLocked, 140.0);
            button(row, "Cancel", CoopButton::CancelPassword, 160.0);
        });
        join.spawn((
            JoinBoard,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8.0),
                min_height: px(200.0),
                ..default()
            },
        ));
        button(join, "Back", crate::menu::MenuButton::Back, 280.0);
    });
    commands
        .entity(join)
        .insert(DespawnOnExit(MainMenuScreen::Join));
}

fn spawn_room(mut commands: Commands, session: Res<CoopSession>) {
    let host = matches!(*session, CoopSession::Host(_));
    let room = screen(&mut commands, MENU_BACKGROUND, |room| {
        room.spawn(text("ROOM", 64.0, Color::WHITE));
        room.spawn((RoomLabel::Code, text("Code: ", 36.0, NEW_BEST_GOLD)));
        room.spawn((RoomLabel::Summary, text("", 22.0, HINT_GREY)));
        room.spawn((RoomLabel::Lobby, text("", 20.0, ROUND_RED)));
        if host {
            room.spawn(Node {
                column_gap: px(12.0),
                ..default()
            })
            .with_children(|row| {
                label_button(row, RoomLabel::Mode, "Mode", CoopButton::ToggleMode, 260.0);
                label_button(row, RoomLabel::Map, "Map", CoopButton::CycleMap, 260.0);
                label_button(row, RoomLabel::Listed, "Listed", CoopButton::ToggleListed, 260.0);
            });
            room.spawn(Node {
                column_gap: px(12.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                input(
                    row,
                    InputKind::RoomPassword,
                    "",
                    "Password (optional)",
                    ROOM_PASSWORD_MAX_CHARS,
                    420.0,
                    false,
                );
                button(row, "Set", CoopButton::SetPassword, 140.0);
            });
        }
        room.spawn((
            RoomBoard,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                margin: UiRect::vertical(px(12.0)),
                ..default()
            },
        ));
        if !host {
            label_button(room, RoomLabel::Ready, "Ready", CoopButton::Ready, 280.0);
        }
        room.spawn((
            ArmoryRow,
            Node {
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|row| button(row, "Armory", CoopButton::Armory, 280.0));
        button(room, if host { "Close Room" } else { "Leave" }, CoopButton::Leave, 280.0);
    });
    commands
        .entity(room)
        .insert(DespawnOnExit(MainMenuScreen::Room));
}

/// A button whose text is kept up to date by `refresh_room`.
fn label_button(
    parent: &mut ChildSpawnerCommands,
    label: RoomLabel,
    start: &str,
    action: CoopButton,
    width: f32,
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
        .with_child((label, text(start, 26.0, Color::WHITE)));
}

// Text input

fn input(
    parent: &mut ChildSpawnerCommands,
    kind: InputKind,
    value: &str,
    hint: &'static str,
    max: usize,
    width: f32,
    autofocus: bool,
) {
    parent
        .spawn((
            Button,
            Hovered::default(),
            MenuButtonLook,
            CoopButton::Focus,
            TextInput {
                kind,
                value: value.to_string(),
                max,
                hint,
                upper: kind == InputKind::Code,
                autofocus,
            },
            Node {
                width: px(width),
                height: px(48.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BUTTON_IDLE),
        ))
        .with_child(text("", 26.0, Color::WHITE));
}

fn focus_new_input(new: Query<(Entity, &TextInput), Added<TextInput>>, mut typing: ResMut<Typing>) {
    for (entity, input) in &new {
        if input.autofocus {
            typing.0 = Some(entity);
        }
    }
}

/// Types into the focused input. Enter does what the input's screen would on its button.
fn type_text(
    mut keys: MessageReader<KeyboardInput>,
    typing: Res<Typing>,
    mut inputs: Query<&mut TextInput>,
    mut actions: MessageWriter<Do>,
) {
    let Some(mut input) = typing.0.and_then(|entity| inputs.get_mut(entity).ok()) else {
        keys.clear();
        return;
    };
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                input.value.pop();
            }
            Key::Enter => {
                actions.write(Do(match input.kind {
                    InputKind::Name => CoopButton::Continue,
                    InputKind::Code => CoopButton::Find,
                    InputKind::JoinPassword => CoopButton::JoinLocked,
                    InputKind::RoomPassword => CoopButton::SetPassword,
                }));
            }
            _ => {
                let Some(typed) = &key.text else { continue };
                let upper = input.upper;
                for c in typed.chars().filter(|c| !c.is_control()) {
                    if input.value.chars().count() < input.max {
                        if upper {
                            input.value.extend(c.to_uppercase());
                        } else {
                            input.value.push(c);
                        }
                    }
                }
            }
        }
    }
}

/// An input shows what is typed, a cursor while it has the keys, or its hint.
fn show_inputs(
    typing: Res<Typing>,
    inputs: Query<(Entity, &TextInput, &Children)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    for (entity, input, children) in &inputs {
        let focused = typing.0 == Some(entity);
        let (shown, color) = match (input.value.is_empty(), focused) {
            (true, false) => (input.hint.to_string(), HINT_GREY),
            (true, true) => ("_".to_string(), Color::WHITE),
            (false, true) => (format!("{}_", input.value), Color::WHITE),
            (false, false) => (input.value.clone(), Color::WHITE),
        };
        for child in children.iter() {
            if let Ok((mut text, mut text_color)) = texts.get_mut(child) {
                if text.0 != shown {
                    text.0 = shown.clone();
                }
                text_color.set_if_neq(TextColor(color));
            }
        }
    }
}

fn typed(inputs: &Query<&TextInput>, kind: InputKind) -> String {
    inputs
        .iter()
        .find(|input| input.kind == kind)
        .map(|input| input.value.clone())
        .unwrap_or_default()
}

// Actions

fn click(
    activate: On<Activate>,
    buttons: Query<&CoopButton>,
    mut typing: ResMut<Typing>,
    mut actions: MessageWriter<Do>,
) {
    let Ok(&button) = buttons.get(activate.entity) else {
        return;
    };
    match button {
        CoopButton::Focus => typing.0 = Some(activate.entity),
        other => {
            actions.write(Do(other));
        }
    }
}

/// Joins the Room `listed`, saying Hello with `password`.
fn dial(
    listed: &Listed,
    password: &str,
    settings: &Settings,
    file: &GauntletFile,
    session: &mut CoopSession,
    next: &mut NextState<MainMenuScreen>,
) {
    let joining = Joining::new(
        Box::new(Transport::join(&listed.endpoint)) as Box<dyn Link>,
        &settings.name,
        password,
        armory_of(file),
    );
    *session = CoopSession::Join(Box::new(joining));
    next.set(MainMenuScreen::Room);
}

/// A Room has been picked: asks for its password if it has one, or joins it.
fn choose(
    listed: Listed,
    settings: &Settings,
    file: &GauntletFile,
    session: &mut CoopSession,
    target: &mut JoinTarget,
    notice: &mut Notice,
    next: &mut NextState<MainMenuScreen>,
) {
    notice.0.clear();
    if listed.locked {
        target.0 = Some(listed);
    } else {
        target.0 = None;
        dial(&listed, "", settings, file, session, next);
    }
}

fn run_actions(
    mut actions: MessageReader<Do>,
    inputs: Query<&TextInput>,
    (mut session, mut lobby, mut notice, mut target, mut finding): (
        ResMut<CoopSession>,
        ResMut<LobbyClient>,
        ResMut<Notice>,
        ResMut<JoinTarget>,
        ResMut<Finding>,
    ),
    (mut settings, file, mut back_to): (ResMut<Settings>, Res<GauntletFile>, ResMut<BackTo>),
    mut next: ResMut<NextState<MainMenuScreen>>,
) {
    for Do(action) in actions.read() {
        match *action {
            CoopButton::Focus => {}
            CoopButton::Host => {
                notice.0.clear();
                let room = Room::new(&settings.name, armory_of(&file), settings.last_mode);
                *session = CoopSession::Host(Box::new(Hosting::new(Box::new(Transport::host()) as Box<dyn Link>, room)));
                next.set(MainMenuScreen::Room);
            }
            CoopButton::Join => {
                notice.0.clear();
                target.0 = None;
                next.set(MainMenuScreen::Join);
            }
            CoopButton::Find => {
                let code = typed(&inputs, InputKind::Code);
                if !code.trim().is_empty() {
                    notice.0.clear();
                    finding.0 = true;
                    lobby.find(&code);
                }
            }
            CoopButton::Refresh => lobby.list(),
            CoopButton::Pick(at) => {
                if let Some(listed) = lobby.sessions.get(at).cloned() {
                    choose(listed, &settings, &file, &mut session, &mut target, &mut notice, &mut next);
                }
            }
            CoopButton::JoinLocked => {
                if let Some(listed) = target.0.clone() {
                    let password = typed(&inputs, InputKind::JoinPassword);
                    dial(&listed, &password, &settings, &file, &mut session, &mut next);
                }
            }
            CoopButton::CancelPassword => target.0 = None,
            CoopButton::Continue => {
                let name = typed(&inputs, InputKind::Name);
                if !name.trim().is_empty() {
                    settings.name = clean_name(&name);
                    next.set(back_to.name);
                }
            }
            CoopButton::ChangeName => {
                back_to.name = MainMenuScreen::Coop;
                next.set(MainMenuScreen::Name);
            }
            CoopButton::ToggleMode => {
                if let CoopSession::Host(hosting) = &mut *session {
                    let mode = match hosting.room.mode() {
                        Mode::Classic => Mode::Gauntlet,
                        Mode::Gauntlet => Mode::Classic,
                    };
                    hosting.edit(|room| room.set_mode(mode));
                }
            }
            CoopButton::CycleMap => {
                if let CoopSession::Host(hosting) = &mut *session {
                    let at = MapId::ALL.iter().position(|&map| map == hosting.room.map());
                    let map = MapId::ALL[(at.unwrap_or(0) + 1) % MapId::ALL.len()];
                    hosting.edit(|room| room.set_map(map));
                }
            }
            CoopButton::ToggleListed => {
                if let CoopSession::Host(hosting) = &mut *session {
                    let listed = !hosting.room.is_listed();
                    hosting.edit(|room| room.set_listed(listed));
                }
            }
            CoopButton::SetPassword => {
                if let CoopSession::Host(hosting) = &mut *session {
                    let password = typed(&inputs, InputKind::RoomPassword);
                    hosting.edit(|room| room.set_password(&password));
                }
            }
            CoopButton::Ready => {
                let now = session.is_ready();
                if let CoopSession::Join(joining) = &mut *session {
                    joining.ready(!now);
                }
            }
            CoopButton::Kick(seat) => {
                if let CoopSession::Host(hosting) = &mut *session {
                    hosting.kick(seat);
                }
            }
            CoopButton::Armory => {
                back_to.armory = MainMenuScreen::Room;
                next.set(MainMenuScreen::Armory);
            }
            CoopButton::Leave => next.set(MainMenuScreen::Coop),
        }
    }
}

// The Room, live

/// Takes in what the other end has said, and sends the Player back to the Co-op screens
/// when the Room is over for them.
fn pump_session(
    mut session: ResMut<CoopSession>,
    mut notice: ResMut<Notice>,
    mut target: ResMut<JoinTarget>,
    screen: Option<Res<State<MainMenuScreen>>>,
    mut next: ResMut<NextState<MainMenuScreen>>,
) {
    let in_menu = screen.is_some();
    match &mut *session {
        CoopSession::Off => {}
        CoopSession::Host(hosting) => {
            if let Some(why) = hosting.poll() {
                notice.0 = format!("Couldn't open the Room: {why}");
                *session = CoopSession::Off;
                if in_menu {
                    next.set(MainMenuScreen::Coop);
                }
            }
        }
        CoopSession::Join(joining) => {
            joining.poll();
            let (message, back, keep_target) = match &joining.state {
                Joined::Refused(why) => (
                    why.reason().to_string(),
                    MainMenuScreen::Join,
                    *why == Refusal::WrongPassword,
                ),
                Joined::Kicked => ("You were kicked from the room".into(), MainMenuScreen::Join, false),
                Joined::Closed => ("Host closed the room".into(), MainMenuScreen::Coop, false),
                Joined::Lost(why) => (why.clone(), MainMenuScreen::Coop, false),
                _ => return,
            };
            notice.0 = message;
            if !keep_target {
                target.0 = None;
            }
            *session = CoopSession::Off;
            if in_menu {
                next.set(back);
            }
        }
    }
}

/// Keeps a hosted Room on the lobby's list while it is open and not yet started.
fn keep_lobby(
    time: Res<Time<Real>>,
    mut session: ResMut<CoopSession>,
    mut lobby: ResMut<LobbyClient>,
) {
    let listing = match &*session {
        CoopSession::Host(hosting) if !hosting.room.is_started() => {
            hosting.address().map(|endpoint| Listing {
                host: hosting.room.host_name().to_string(),
                endpoint,
                locked: hosting.room.has_password(),
                unlisted: !hosting.room.is_listed(),
                status: Status {
                    players: hosting.room.players() as u8,
                    circuit: status_line(
                        hosting.room.map(),
                        hosting.room.mode(),
                        hosting.room.heat(),
                    ),
                    racing: false,
                },
            })
        }
        _ => None,
    };
    lobby.tick(time.delta_secs(), listing.as_ref());
    if let CoopSession::Host(hosting) = &mut *session {
        let code = lobby.code().unwrap_or_default();
        if hosting.room.code() != code {
            let code = code.to_string();
            hosting.edit(|room| room.set_code(&code));
        }
    }
}

/// A Room is open only while its screen is, or the Armory opened from it. Once a Run begins
/// there is no Main Menu screen, and the Room stays.
fn close_unused_session(
    mut session: ResMut<CoopSession>,
    screen: Option<Res<State<MainMenuScreen>>>,
    back_to: Res<BackTo>,
) {
    let Some(screen) = screen else { return };
    let wanted = match *screen.get() {
        MainMenuScreen::Room => true,
        MainMenuScreen::Armory => back_to.armory == MainMenuScreen::Room,
        _ => false,
    };
    if !wanted && !session.is_off() {
        match &mut *session {
            CoopSession::Host(hosting) => hosting.close(),
            CoopSession::Join(joining) => joining.leave(),
            CoopSession::Off => {}
        }
        *session = CoopSession::Off;
    }
}

/// The game is closing: a Room it hosts comes off the lobby's list.
fn farewell(mut closing: MessageReader<AppExit>, mut lobby: ResMut<LobbyClient>) {
    if closing.read().next().is_some() {
        lobby.farewell();
    }
}

/// A changed Armory is re-sent to the Host, and clears that Player's Ready.
fn resend_armory(file: Res<GauntletFile>, mut session: ResMut<CoopSession>) {
    match &mut *session {
        CoopSession::Join(joining) => joining.armory(&armory_of(&file)),
        CoopSession::Host(hosting) => {
            let armory = armory_of(&file);
            hosting.edit(|room| room.set_host_unlocks(armory));
        }
        CoopSession::Off => {}
    }
}

fn refresh_notice(notice: Res<Notice>, mut texts: Query<&mut Text, With<NoticeText>>) {
    for mut text in &mut texts {
        if text.0 != notice.0 {
            text.0 = notice.0.clone();
        }
    }
}

// Join

fn refresh_join(
    mut commands: Commands,
    board: Query<Entity, With<JoinBoard>>,
    mut prompt: Query<&mut Node, With<PasswordPrompt>>,
    lobby: Res<LobbyClient>,
    target: Res<JoinTarget>,
    mut shown: Local<Option<(u32, Option<String>, Option<String>)>>,
) {
    for mut node in &mut prompt {
        let display = if target.0.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    let now = (
        lobby.revision,
        lobby.trouble.clone(),
        target.0.as_ref().map(|t| t.id.clone()),
    );
    if shown.as_ref() == Some(&now) {
        return;
    }
    *shown = Some(now);
    let Ok(board) = board.single() else { return };
    commands
        .entity(board)
        .despawn_children()
        .with_children(|board| {
            if let Some(why) = &lobby.trouble {
                board.spawn(text(format!("The lobby couldn't be reached: {why}"), 20.0, ROUND_RED));
            }
            if let Some(target) = &target.0 {
                board.spawn(text(
                    format!("{}'s Room is locked.", target.host),
                    24.0,
                    NEW_BEST_GOLD,
                ));
            }
            if lobby.sessions.is_empty() {
                board.spawn(text("No Rooms listed. Host one, or type a code.", 22.0, HINT_GREY));
            }
            for (at, room) in lobby.sessions.iter().enumerate() {
                let (map, mode, heat) = read_status_line(&room.status.circuit);
                let mut line = format!("{}  {}  {}", room.host, map, mode);
                if heat > 0 {
                    line.push_str(&format!("  Heat {heat}"));
                }
                line.push_str(&format!("  {}/{}", room.status.players, room.max));
                if room.locked {
                    line.push_str("  [locked]");
                }
                if room.status.racing {
                    line.push_str("  (in a Run)");
                }
                button(board, &line, CoopButton::Pick(at), 720.0);
            }
        });
}

/// A code the lobby has answered for: joins that Room, or says there is none.
fn found_room(
    mut lobby: ResMut<LobbyClient>,
    mut finding: ResMut<Finding>,
    (mut session, mut target, mut notice): (ResMut<CoopSession>, ResMut<JoinTarget>, ResMut<Notice>),
    (settings, file): (Res<Settings>, Res<GauntletFile>),
    mut next: ResMut<NextState<MainMenuScreen>>,
) {
    if !finding.0 {
        return;
    }
    let Some(found) = lobby.found.take() else {
        if lobby.trouble.is_some() {
            finding.0 = false;
        }
        return;
    };
    finding.0 = false;
    match found {
        Some(listed) => choose(listed, &settings, &file, &mut session, &mut target, &mut notice, &mut next),
        None => notice.0 = "No room has that code".into(),
    }
}

/// The Join screen asks the lobby for the list again now and then.
fn relist(time: Res<Time<Real>>, mut timer: ResMut<ListTimer>, lobby: Res<LobbyClient>) {
    timer.0 -= time.delta_secs();
    if timer.0 <= 0.0 {
        timer.0 = LOBBY_LIST_SECS;
        lobby.list();
    }
}

// The Room screen

fn refresh_room(
    mut commands: Commands,
    session: Res<CoopSession>,
    lobby: Res<LobbyClient>,
    board: Query<Entity, With<RoomBoard>>,
    mut labels: Query<(&RoomLabel, &mut Text)>,
    mut armory: Query<&mut Node, With<ArmoryRow>>,
    mut shown: Local<Option<(Option<(RoomView, u8)>, bool)>>,
) {
    let host = matches!(*session, CoopSession::Host(_));
    let current = session.view();
    let mode = current.as_ref().map(|(view, _)| view.mode);
    for mut node in &mut armory {
        let display = if mode == Some(Mode::Gauntlet) { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    let ready = session.is_ready();
    let (code, summary, mode_text, map_text, listed_text) = match &current {
        Some((view, _)) => {
            let mut summary = status_line(view.map, view.mode, view.heat);
            if view.locked {
                summary.push_str("  [locked]");
            }
            (
                view.code.clone(),
                summary,
                format!("Mode: {:?}", view.mode),
                format!("Map: {}", view.map.name()),
                match &*session {
                    CoopSession::Host(hosting) if hosting.room.is_listed() => "Listed".to_string(),
                    _ => "Unlisted".to_string(),
                },
            )
        }
        None => (String::new(), "Joining...".into(), "Mode".into(), "Map".into(), "Listed".into()),
    };
    let lobby_note = if host && lobby.code().is_none() {
        match &lobby.trouble {
            Some(why) => format!("The lobby couldn't be reached ({why}). Retrying."),
            None => "Opening the Room on the lobby...".to_string(),
        }
    } else {
        String::new()
    };
    for (label, mut text) in &mut labels {
        let value = match label {
            RoomLabel::Code if code.is_empty() => "Code: ...".to_string(),
            RoomLabel::Code => format!("Code: {code}"),
            RoomLabel::Summary => summary.clone(),
            RoomLabel::Mode => mode_text.clone(),
            RoomLabel::Map => map_text.clone(),
            RoomLabel::Listed => listed_text.clone(),
            RoomLabel::Ready => if ready { "Ready" } else { "Not Ready" }.to_string(),
            RoomLabel::Lobby => lobby_note.clone(),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
    let now = (current, host);
    if shown.as_ref() == Some(&now) {
        return;
    }
    let Ok(board) = board.single() else { return };
    let (view, mine) = (now.0.clone(), now.0.as_ref().map(|(_, seat)| *seat));
    *shown = Some(now);
    commands
        .entity(board)
        .despawn_children()
        .with_children(|board| {
            for at in 0..ROOM_SEATS {
                let seat = view.as_ref().and_then(|(view, _)| view.seats[at].clone());
                board
                    .spawn(Node {
                        column_gap: px(14.0),
                        align_items: AlignItems::Center,
                        height: px(40.0),
                        ..default()
                    })
                    .with_children(|row| {
                        let colour = Seat(at as u8).colour();
                        row.spawn((
                            Node {
                                width: px(24.0),
                                height: px(24.0),
                                ..default()
                            },
                            BackgroundColor(if seat.is_some() { colour } else { BUTTON_IDLE }),
                        ));
                        let (name, name_colour) = match &seat {
                            Some(seat) if mine == Some(at as u8) => (format!("{} (you)", seat.name), colour),
                            Some(seat) => (seat.name.clone(), colour),
                            None => ("Empty".to_string(), HINT_GREY),
                        };
                        row.spawn((
                            text(name, 28.0, name_colour),
                            Node {
                                width: px(300.0),
                                ..default()
                            },
                        ));
                        let ready = match &seat {
                            Some(seat) if at == 0 || seat.ready => ("READY", READY_GREEN),
                            Some(_) => ("not ready", HINT_GREY),
                            None => ("", HINT_GREY),
                        };
                        row.spawn((
                            text(ready.0, 24.0, ready.1),
                            Node {
                                width: px(130.0),
                                ..default()
                            },
                        ));
                        if host && at > 0 && seat.is_some() {
                            button_small(row, "Kick", CoopButton::Kick(at as u8));
                        }
                    });
            }
        });
}

/// A short button for a row.
fn button_small(parent: &mut ChildSpawnerCommands, label: &str, action: CoopButton) {
    parent
        .spawn((
            Button,
            Hovered::default(),
            MenuButtonLook,
            action,
            Node {
                width: px(100.0),
                height: px(36.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BUTTON_IDLE),
        ))
        .with_child(text(label, 22.0, Color::WHITE));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gauntlet::progress::TrackSave;
    use crate::net::link::{Hub, Memory};
    use crate::net::lobby::{LobbyApi, Reply, Registered, Register};
    use crate::state::GameState;
    use bevy::state::app::StatesPlugin;
    use std::sync::Arc;

    /// A lobby with nobody on it, so no test goes near the internet.
    struct Empty;

    impl LobbyApi for Empty {
        fn list(&self) -> Reply<Vec<Listed>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn find(&self, _: &str) -> Reply<Option<Listed>> {
            Box::pin(async { Ok(None) })
        }
        fn register(&self, _: Register) -> Reply<Registered> {
            Box::pin(async { Err(crate::net::lobby::LobbyError::Unreached("test".into())) })
        }
        fn beat(&self, _: &Registered, _: Status) -> Reply<()> {
            Box::pin(async { Ok(()) })
        }
        fn close(&self, _: &Registered) -> Reply<()> {
            Box::pin(async { Ok(()) })
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .add_sub_state::<MainMenuScreen>()
            .init_resource::<BackTo>()
            .init_resource::<GauntletFile>()
            .insert_resource(Settings {
                name: "Hostess".into(),
                ..default()
            })
            .insert_resource(LobbyClient::new(Arc::new(Empty)))
            .add_message::<KeyboardInput>()
            .add_plugins(CoopPlugin);
        app.update();
        app
    }

    fn go(app: &mut App, screen: MainMenuScreen) {
        app.world_mut().resource_mut::<NextState<MainMenuScreen>>().set(screen);
        app.update();
        app.update();
    }

    fn texts(app: &mut App) -> Vec<String> {
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect()
    }

    fn press(app: &mut App, typed: &str) {
        for c in typed.chars() {
            app.world_mut().write_message(KeyboardInput {
                key_code: KeyCode::KeyA,
                logical_key: Key::Character(c.to_string().into()),
                state: ButtonState::Pressed,
                text: Some(c.to_string().into()),
                repeat: false,
                window: Entity::PLACEHOLDER,
            });
        }
        app.update();
    }

    fn joiner(hub: &Hub, name: &str) -> Joining<Memory> {
        Joining::new(hub.join(), name, "", Unlocks::default())
    }

    #[test]
    fn the_room_screen_shows_the_seats_and_the_host_can_kick() {
        let mut app = app();
        let hub = Hub::new(1, 0.0);
        let room = Room::new("Hostess", Unlocks::default(), Mode::Classic);
        *app.world_mut().resource_mut::<CoopSession>() =
            CoopSession::Host(Box::new(Hosting::new(Box::new(hub.host()), room)));
        go(&mut app, MainMenuScreen::Room);
        let shown = texts(&mut app);
        assert!(shown.contains(&"Hostess (you)".to_string()), "{shown:?}");
        assert!(shown.contains(&"Code: ...".to_string()));
        assert!(shown.contains(&"Mode: Classic".to_string()));

        let mut bo = joiner(&hub, "Bo");
        for _ in 0..8 {
            hub.step();
            bo.poll();
            app.update();
        }
        assert!(texts(&mut app).contains(&"Bo".to_string()));
        assert!(matches!(bo.state, Joined::In { seat: 1, .. }));

        app.world_mut().write_message(Do(CoopButton::ToggleMode));
        app.world_mut().write_message(Do(CoopButton::Kick(1)));
        for _ in 0..8 {
            hub.step();
            bo.poll();
            app.update();
        }
        assert_eq!(bo.state, Joined::Kicked);
        assert!(!texts(&mut app).contains(&"Bo".to_string()));
        assert!(texts(&mut app).contains(&"Mode: Gauntlet".to_string()));

        // Leaving the Room screen closes the Room.
        go(&mut app, MainMenuScreen::Coop);
        assert!(app.world().resource::<CoopSession>().is_off());
    }

    #[test]
    fn a_joiner_sees_the_host_close_the_room_and_goes_back_to_co_op() {
        let mut app = app();
        let hub = Hub::new(1, 0.0);
        let mut host = Hosting::new(
            Box::new(hub.host()) as Box<dyn Link>,
            Room::new("Hostess", Unlocks::default(), Mode::Classic),
        );
        *app.world_mut().resource_mut::<CoopSession>() =
            CoopSession::Join(Box::new(Joining::new(Box::new(hub.join()), "Ada", "", Unlocks::default())));
        go(&mut app, MainMenuScreen::Room);
        for _ in 0..8 {
            hub.step();
            host.poll();
            app.update();
        }
        assert!(texts(&mut app).contains(&"Ada (you)".to_string()));
        host.close();
        for _ in 0..8 {
            hub.step();
            app.update();
        }
        assert!(app.world().resource::<CoopSession>().is_off());
        assert_eq!(app.world().resource::<Notice>().0, "Host closed the room");
        assert_eq!(**app.world().resource::<State<MainMenuScreen>>(), MainMenuScreen::Coop);
    }

    #[test]
    fn the_join_screen_builds_without_the_internet_and_the_name_screen_keeps_twelve() {
        let mut app = app();
        go(&mut app, MainMenuScreen::Join);
        assert!(texts(&mut app).iter().any(|t| t.starts_with("No Rooms listed")));

        // The first time Co-op opens there is no Name yet.
        app.world_mut().resource_mut::<Settings>().name.clear();
        go(&mut app, MainMenuScreen::Name);
        press(&mut app, "Bartholomew The Third");
        press(&mut app, "");
        app.world_mut().write_message(KeyboardInput {
            key_code: KeyCode::Enter,
            logical_key: Key::Enter,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        });
        app.update();
        app.update();
        assert_eq!(app.world().resource::<Settings>().name, "Bartholomew");
        assert_eq!(**app.world().resource::<State<MainMenuScreen>>(), MainMenuScreen::Coop);
    }

    #[test]
    fn an_armory_is_sent_as_its_unlocks_and_tracks() {
        let file = GauntletFile {
            unlocked: vec!["boon:x".into()],
            tracks: vec![TrackSave {
                id: "move_speed".into(),
                ranks: 3,
                on: false,
            }],
            ..default()
        };
        let sent = armory_of(&file);
        assert_eq!(sent.unlocked, ["boon:x"]);
        assert_eq!(
            sent.tracks,
            [TrackState {
                id: "move_speed".into(),
                ranks: 3,
                on: false
            }]
        );
    }
}
