//! A client whose way to the Host has broken, and getting back.
//!
//! The client redials every `RECONNECT_INTERVAL_SECS` for `RECONNECT_REDIAL_SECS`, saying
//! "Reconnecting…", with the token the Host gave it. After that it offers Rejoin or Leave until the
//! game is closed. The Run's own world stands still for it meanwhile: its Player takes no input.

use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use super::play::Wire;
use super::protocol::ToHost;
use crate::hud::text;
use crate::menu::{button, screen};
use crate::player::LocalPlayer;
use crate::records::SaveRecord;
use crate::state::{Choosing, GameState};
use crate::tuning::*;

/// How a client's way to the Host stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reach {
    /// The Host has been heard from.
    Up,
    /// Redialling: seconds of it left, seconds until the next dial, and whether a dial is under way.
    Redialling { left: f32, next: f32, dialling: bool },
    /// The redialling is over: the Player is asked whether to Rejoin or Leave.
    Offered,
}

/// What a client needs to get back to the Host, and where it has got to.
#[derive(Resource)]
pub struct Rejoin {
    /// What the Host last gave to come back by.
    pub token: u64,
    hello: ToHost,
    pub reach: Reach,
}

impl Rejoin {
    pub fn new(token: u64, hello: ToHost) -> Self {
        Rejoin { token, hello, reach: Reach::Up }
    }

    /// Whether the way to the Host is down.
    pub fn is_away(&self) -> bool {
        self.reach != Reach::Up
    }

    /// The way has broken, or a dial has failed.
    pub fn lost(&mut self) {
        match &mut self.reach {
            Reach::Up => self.start(),
            Reach::Redialling { dialling, .. } => *dialling = false,
            Reach::Offered => {}
        }
    }

    /// Starts redialling, the first dial at once.
    pub fn start(&mut self) {
        self.reach = Reach::Redialling { left: RECONNECT_REDIAL_SECS, next: 0.0, dialling: false };
    }

    /// What is said to the Host on reaching it again.
    pub fn greeting(&self) -> ToHost {
        let mut hello = self.hello.clone();
        if let ToHost::Hello { back, .. } = &mut hello {
            *back = Some(self.token);
        }
        hello
    }

    /// The Host has taken the Player back, and gave a new token.
    pub fn welcomed(&mut self, token: u64) {
        self.token = token;
        self.reach = Reach::Up;
    }
}

/// Dials the Host again while the redialling lasts, and offers Rejoin once it is over.
pub fn redial(time: Res<Time<Real>>, mut wire: ResMut<Wire>, mut rejoin: ResMut<Rejoin>) {
    let dt = time.delta_secs();
    let Reach::Redialling { left, next, dialling } = &mut rejoin.reach else { return };
    *left -= dt;
    *next -= dt;
    if !*dialling && *next <= 0.0 {
        *next = RECONNECT_INTERVAL_SECS;
        *dialling = wire.0.redial();
    }
    if *left <= 0.0 {
        rejoin.reach = Reach::Offered;
    }
}

/// Stops the `LocalPlayer`'s input while the way to the Host is down.
pub fn hold_local(
    mut commands: Commands,
    rejoin: Res<Rejoin>,
    local: Query<(Entity, Has<Choosing>), With<LocalPlayer>>,
    mut held: Local<bool>,
) {
    let Ok((player, choosing)) = local.single() else { return };
    if rejoin.is_away() {
        if !choosing {
            commands.entity(player).insert(Choosing);
        }
        *held = true;
    } else if std::mem::take(&mut *held) {
        commands.entity(player).remove::<Choosing>();
    }
}

#[derive(Component)]
struct AwayScreen(bool);

#[derive(Component, Clone, Copy)]
enum AwayButton {
    Rejoin,
    Leave,
}

/// Shows "Reconnecting…", then Rejoin and Leave.
fn show_away(mut commands: Commands, rejoin: Option<Res<Rejoin>>, shown: Query<(Entity, &AwayScreen)>) {
    let wanted = rejoin.and_then(|rejoin| match rejoin.reach {
        Reach::Up => None,
        Reach::Redialling { .. } => Some(false),
        Reach::Offered => Some(true),
    });
    let current = shown.iter().next();
    if current.map(|(_, kind)| kind.0) == wanted {
        return;
    }
    if let Some((entity, _)) = current {
        commands.entity(entity).despawn();
    }
    let Some(offered) = wanted else { return };
    let away = screen(&mut commands, Color::srgba(0.0, 0.0, 0.0, 0.6), |menu| {
        if offered {
            menu.spawn(text("Lost the connection to the Host", 36.0, Color::WHITE));
            button(menu, "Rejoin", AwayButton::Rejoin, 320.0);
            button(menu, "Leave", AwayButton::Leave, 320.0);
        } else {
            menu.spawn(text("Reconnecting…", 48.0, Color::WHITE));
        }
    });
    commands.entity(away).insert((AwayScreen(offered), DespawnOnExit(GameState::Playing)));
}

fn away_click(
    activate: On<Activate>,
    buttons: Query<&AwayButton>,
    rejoin: Option<ResMut<Rejoin>>,
    mut saves: MessageWriter<SaveRecord>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Ok(button) = buttons.get(activate.entity) else { return };
    match button {
        AwayButton::Rejoin => {
            if let Some(mut rejoin) = rejoin {
                rejoin.start();
            }
        }
        // The Run is over for this Player: their Record is saved as they go.
        AwayButton::Leave => {
            saves.write(SaveRecord);
            next.set(GameState::MainMenu);
        }
    }
}

pub struct AwayPlugin;

impl Plugin for AwayPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(away_click).add_systems(Update, show_away.run_if(in_state(GameState::Playing)));
    }
}
