//! The Win in Co-op (spec #145, Gauntlet in Co-op: The Win and Heat). Every machine shows its own
//! Player the Win screen at `win_round()`, over the world running on as an Offer is, never frozen.
//! Each Player chooses for themselves, and the Boss Offer and the next Round wait for every choice,
//! up to `GAUNTLET_WIN_COOP_TIMEOUT_SECS`. A Player who hasn't chosen by then counts as Keep
//! going, and their screen closes.
//!
//! End the Run is said as a `WinChoice`: the Host's own ends the Run for everyone as a Win (its
//! Game Over tells every machine so), and a client's gives up their Seat for good through the
//! leave path (`net::world::win`). Solo's Win screen is `arc`'s, over the frozen world, and none of
//! this runs there.

use bevy::prelude::*;

use super::Offer;
use super::arc::{RunArc, WinScreen, win_round, win_screen};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::players::player::{LocalPlayer, Player, Seat};
use crate::players::respawn::Disconnected;
use crate::run::leave_run::PlayerLeft;
use crate::run::round::{Round, RoundEnded};
use crate::state::{Choosing, GameState, RunType};
use crate::tuning::*;

/// A Player chose on their Win screen: End the Run (`end`) or Keep going. Written on the machine
/// that clicked, for its own Player, and by the Host for a client's `Request::WinChoice`.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinChoice {
    pub seat: Seat,
    pub end: bool,
}

/// The Host's wait for every Player's Win choice: the Seats still to choose, and how long is left.
/// While anyone is still to choose, no Offer opens and the Intermission holds.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct WinWait {
    waiting: Vec<Seat>,
    left: f32,
}

impl WinWait {
    /// Waits for the Players in `seats`, for at most `GAUNTLET_WIN_COOP_TIMEOUT_SECS`.
    pub fn begin(seats: impl IntoIterator<Item = Seat>) -> Self {
        Self { waiting: seats.into_iter().collect(), left: GAUNTLET_WIN_COOP_TIMEOUT_SECS }
    }

    /// Whether some Player is still to choose.
    pub fn holds(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// The Seats still to choose.
    #[cfg(test)]
    pub fn waiting(&self) -> &[Seat] {
        &self.waiting
    }

    /// The Player in `seat` has chosen, or gone.
    pub fn chosen(&mut self, seat: Seat) {
        self.waiting.retain(|&waiting| waiting != seat);
    }

    /// `dt` seconds pass: once the time is up, everyone still to choose counts as Keep going.
    pub fn tick(&mut self, dt: f32) {
        if !self.holds() {
            return;
        }
        self.left -= dt;
        if self.left <= 0.0 {
            self.waiting.clear();
        }
    }
}

/// How long this machine's own Win screen has left open in Co-op; `None` until it opens.
#[derive(Resource, Debug, Default)]
pub struct WinClock(Option<f32>);

/// The Host, at the Win in Co-op, waits for every Player there to choose. One whose link has
/// dropped can't see the screen, so isn't waited for.
pub fn await_win_choices(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    run_type: Res<RunType>,
    players: Query<&Seat, (With<Player>, Without<Disconnected>)>,
    mut wait: ResMut<WinWait>,
) {
    if ended.read().count() == 0 || round.number != win_round() || !run_type.is_coop() {
        return;
    }
    *wait = WinWait::begin(players.iter().copied());
}

/// The Host hears each Player's choice, and each who leaves, and stops waiting once the time is up.
pub fn settle_win_choices(
    mut chosen: MessageReader<WinChoice>,
    mut left: MessageReader<PlayerLeft>,
    time: Res<Time>,
    mut wait: ResMut<WinWait>,
) {
    for choice in chosen.read() {
        wait.chosen(choice.seat);
    }
    for gone in left.read() {
        wait.chosen(gone.seat);
    }
    wait.tick(time.delta_secs());
}

/// In Co-op, this machine's Win screen opens as its Player wins, over the running world, and stops
/// their input as an open Offer does.
pub fn open_win_screen(
    mut commands: Commands,
    arc: Res<RunArc>,
    mut clock: ResMut<WinClock>,
    local: Query<Entity, With<LocalPlayer>>,
    mut cues: MessageWriter<PlayCue>,
) {
    if !arc.choosing || clock.0.is_some() {
        return;
    }
    clock.0 = Some(GAUNTLET_WIN_COOP_TIMEOUT_SECS);
    win_screen(&mut commands).insert(DespawnOnExit(GameState::Playing));
    cues.write(PlayCue::new(Cue::GauntletWin, None));
    if let Ok(player) = local.single() {
        commands.entity(player).insert(Choosing);
    }
}

/// In Co-op, this machine's Win screen closes once its Player has chosen, or when the time is up,
/// which counts as Keep going.
pub fn close_win_screen(
    mut commands: Commands,
    mut arc: ResMut<RunArc>,
    mut clock: ResMut<WinClock>,
    time: Res<Time>,
    screens: Query<Entity, With<WinScreen>>,
    local: Query<(Entity, Has<Offer>), With<LocalPlayer>>,
) {
    let Some(left) = clock.0.as_mut() else { return };
    if arc.choosing {
        *left -= time.delta_secs();
        if *left > 0.0 {
            return;
        }
        arc.choosing = false;
    }
    if screens.is_empty() {
        return;
    }
    for screen in &screens {
        commands.entity(screen).despawn();
    }
    // An Offer opened since keeps the Player's input stopped for itself.
    if let Ok((player, false)) = local.single() {
        commands.entity(player).remove::<Choosing>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_stops_waiting_once_every_player_has_chosen() {
        let mut wait = WinWait::begin([Seat(0), Seat(1)]);
        assert!(wait.holds());
        wait.chosen(Seat(1));
        assert!(wait.holds(), "the Host is still to choose");
        wait.chosen(Seat(0));
        assert!(!wait.holds());
    }

    #[test]
    fn no_choice_within_the_timeout_counts_as_keep_going() {
        let mut wait = WinWait::begin([Seat(0), Seat(1)]);
        wait.tick(GAUNTLET_WIN_COOP_TIMEOUT_SECS - 0.5);
        assert!(wait.holds(), "not yet");
        wait.tick(1.0);
        assert!(!wait.holds(), "the wait never runs past the timeout");
    }

    #[test]
    fn the_win_waits_no_longer_than_an_offer_does() {
        assert_eq!(GAUNTLET_WIN_COOP_TIMEOUT_SECS, BOON_OFFER_COOP_TIMEOUT_SECS);
    }

    /// One machine's own Win screen in Co-op, with the Win just reached.
    fn screen_app() -> (App, Entity) {
        use std::time::Duration;
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
            .insert_resource(RunArc { won: true, choosing: true, ..default() })
            .init_resource::<WinClock>()
            .add_message::<PlayCue>()
            .add_systems(Update, (open_win_screen, close_win_screen).chain());
        let player = app.world_mut().spawn((Player, LocalPlayer, Seat(1))).id();
        (app, player)
    }

    fn screens(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query_filtered::<(), With<WinScreen>>().iter(world).count()
    }

    #[test]
    fn a_co_op_win_screen_opens_over_the_running_world_and_stops_the_players_input() {
        let (mut app, player) = screen_app();
        app.update();
        assert_eq!(screens(&mut app), 1);
        assert!(app.world().get::<Choosing>(player).is_some());
    }

    #[test]
    fn a_co_op_win_screen_closes_as_keep_going_when_its_time_is_up() {
        let (mut app, player) = screen_app();
        app.update();
        let frames = (GAUNTLET_WIN_COOP_TIMEOUT_SECS / 0.1) as usize + 5;
        for _ in 0..frames {
            app.update();
        }
        assert_eq!(screens(&mut app), 0);
        assert!(!app.world().resource::<RunArc>().choosing, "counted as Keep going");
        assert!(app.world().get::<Choosing>(player).is_none());
    }

    #[test]
    fn a_co_op_win_screen_closes_once_its_player_has_chosen() {
        let (mut app, player) = screen_app();
        app.update();
        app.world_mut().resource_mut::<RunArc>().choosing = false;
        app.update();
        assert_eq!(screens(&mut app), 0);
        assert!(app.world().get::<Choosing>(player).is_none());
    }
}
