use bevy::prelude::*;

use crate::players::player::{LocalPlayer, Player, SpawnPlayers};

#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    MainMenu,
    Playing,
    GameOver,
}

/// Which screen of the Main Menu is showing.
#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(GameState = GameState::MainMenu)]
pub enum MainMenuScreen {
    #[default]
    Root,
    /// Picking Classic or Gauntlet before a Run.
    Mode,
    Armory,
    Records,
    Settings,
    /// Choosing to Host or Join a Co-op Room.
    Coop,
    /// Typing the Player's Name, asked the first time Co-op opens.
    Name,
    /// Picking a Room to join, from the lobby's list or by its code.
    Join,
    /// A Co-op Room, as its Host or as a joiner.
    Room,
}

#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(GameState = GameState::Playing)]
pub enum Pause {
    #[default]
    Running,
    Paused,
    /// The world stands still for a screen layered over the Run, without the Pause menu, while
    /// [`Freeze`] is held. Esc opens Pause over it, and resuming comes back to it.
    Frozen,
}

/// Held by anything layered on the Run that needs the world to stand still for a while (a choice
/// on screen, say) without opening the Pause menu. While held, the world sits in
/// [`Pause::Frozen`] instead of running.
#[derive(Resource, Default)]
pub struct Freeze(pub bool);

/// Where leaving the Pause menu goes: back to a frozen world if something still holds it. A Co-op
/// Run is never frozen.
pub fn resume_to(freeze: &Freeze, run_type: RunType) -> Pause {
    if freeze.0 && !run_type.is_coop() {
        Pause::Frozen
    } else {
        Pause::Running
    }
}

/// Which screen of the Pause menu is showing.
#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(Pause = Pause::Paused)]
pub enum PauseScreen {
    #[default]
    Root,
    Settings,
    /// Co-op's "Leave Run?" confirm.
    LeaveRun,
}

/// A Player who has the Pause menu open. In Co-op the world runs on around them, so this is what
/// the HUD and the network read; a teammate's is set from what the Host sends. Solo, the whole
/// world is paused and nothing needs it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Paused;

/// A Player with a choice open over a running Co-op world (a Gauntlet Offer). Like the Pause menu
/// it stops their input (no moving, firing, Knife or Use), but the world runs on and they can be
/// hurt. Solo, the world is frozen for the choice instead and nothing needs it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Choosing;

/// Everything spawned for a Run. Despawned when leaving Game Over or returning to the Main Menu,
/// so the next Run starts clean.
#[derive(Component, Default)]
pub struct InRun;

/// Whether the Run is solo or Co-op, fixed at Run start by how many Players it starts with and
/// never changed during it: a Co-op Run that drops to one Player keeps Co-op rules, and a Player
/// added later (the debug key's) doesn't make a solo Run Co-op. Of the current (or just-ended) Run.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RunType {
    #[default]
    Solo,
    Coop,
}

impl RunType {
    pub fn is_coop(self) -> bool {
        self == RunType::Coop
    }
}

/// Whether the world runs, which is what every system that advances it asks instead of
/// `in_state(Pause::Running)`: always while the Run is Co-op (Pause is only a local overlay there),
/// otherwise only while nothing pauses or freezes it. This and [`pause_time`] are the one switch on
/// [`RunType`] that Co-op Pause is.
pub fn world_runs(pause: Option<Res<State<Pause>>>, run_type: Option<Res<RunType>>) -> bool {
    let Some(pause) = pause else { return false };
    runs(*pause.get(), run_type.is_some_and(|run_type| run_type.is_coop()))
}

fn runs(pause: Pause, coop: bool) -> bool {
    pause == Pause::Running || coop
}

/// Whether this machine's own Player takes input: only while nothing is open over the Run. In
/// Co-op that is what Pause and an open Offer stop; the world keeps running (see [`world_runs`]).
pub fn input_runs(pause: Option<Res<State<Pause>>>, choosing: Query<(), (With<LocalPlayer>, With<Choosing>)>) -> bool {
    pause.is_some_and(|pause| *pause.get() == Pause::Running) && choosing.is_empty()
}

/// Fixes the Run's type from the Players it starts with. The one place the Player count decides
/// it; it runs once, as the Run begins, after the Players are spawned.
fn fix_run_type(mut commands: Commands, players: Query<(), With<Player>>) {
    let run_type = if players.iter().count() >= 2 { RunType::Coop } else { RunType::Solo };
    commands.insert_resource(run_type);
}

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<Freeze>()
            .init_resource::<RunType>()
            .add_sub_state::<MainMenuScreen>()
            .add_sub_state::<Pause>()
            .add_sub_state::<PauseScreen>()
            .add_systems(OnExit(GameState::GameOver), despawn_run)
            .add_systems(OnEnter(GameState::MainMenu), despawn_run)
            // Co-op's Pause leaves time alone: the world runs on under the menu.
            .add_systems(OnEnter(Pause::Paused), pause_time.run_if(solo))
            .add_systems(OnEnter(Pause::Paused), mark_paused)
            .add_systems(OnExit(Pause::Paused), unmark_paused)
            .add_systems(OnExit(Pause::Paused), resume_time)
            .add_systems(OnEnter(Pause::Frozen), pause_time)
            .add_systems(OnExit(Pause::Frozen), resume_time)
            .add_systems(OnEnter(GameState::Playing), fix_run_type.after(SpawnPlayers))
            .add_systems(OnEnter(GameState::Playing), |mut freeze: ResMut<Freeze>| {
                freeze.0 = false
            })
            .add_systems(
                Update,
                // Esc in the frame a Freeze starts still opens Pause.
                (apply_freeze, toggle_pause)
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

pub(crate) fn solo(run_type: Res<RunType>) -> bool {
    !run_type.is_coop()
}

fn mark_paused(mut commands: Commands, local: Query<Entity, With<LocalPlayer>>) {
    for player in &local {
        commands.entity(player).insert(Paused);
    }
}

fn unmark_paused(mut commands: Commands, local: Query<Entity, With<LocalPlayer>>) {
    for player in &local {
        commands.entity(player).remove::<Paused>();
    }
}

fn despawn_run(mut commands: Commands, run: Query<Entity, With<InRun>>) {
    for entity in &run {
        commands.entity(entity).despawn();
    }
}

/// The world freezes while a [`Freeze`] is held and runs again once it's let go. Pause is left
/// alone: leaving it checks the Freeze itself. A Co-op Run is never frozen, whatever holds it.
fn apply_freeze(
    freeze: Res<Freeze>,
    run_type: Res<RunType>,
    pause: Res<State<Pause>>,
    mut next: ResMut<NextState<Pause>>,
) {
    match (pause.get(), freeze.0) {
        (Pause::Running, true) if !run_type.is_coop() => next.set(Pause::Frozen),
        (Pause::Frozen, false) => next.set(Pause::Running),
        _ => {}
    }
}

/// Esc pauses and resumes. From Pause's Settings it goes back to the Pause menu instead.
fn toggle_pause(
    input: Res<ButtonInput<KeyCode>>,
    pause: Res<State<Pause>>,
    (freeze, run_type): (Res<Freeze>, Res<RunType>),
    screen: Option<Res<State<PauseScreen>>>,
    mut next: ResMut<NextState<Pause>>,
    mut next_screen: ResMut<NextState<PauseScreen>>,
) {
    if !input.just_pressed(KeyCode::Escape) {
        return;
    }
    match (pause.get(), screen.map(|screen| *screen.get())) {
        (Pause::Paused, Some(PauseScreen::Settings | PauseScreen::LeaveRun)) => {
            next_screen.set(PauseScreen::Root)
        }
        (Pause::Paused, _) => next.set(resume_to(&freeze, *run_type)),
        (Pause::Running | Pause::Frozen, _) => next.set(Pause::Paused),
    }
}

pub(crate) fn pause_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn resume_time(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// Co-op's world runs under every Pause state; solo's only when nothing pauses it.
    #[test]
    fn only_a_solo_run_stops_its_world_for_pause() {
        for pause in [Pause::Running, Pause::Paused, Pause::Frozen] {
            assert!(runs(pause, true), "{pause:?}");
            assert_eq!(runs(pause, false), pause == Pause::Running, "{pause:?}");
        }
    }

    /// Resuming from a Co-op Pause never lands in a frozen world.
    #[test]
    fn a_co_op_run_resumes_to_running_even_with_a_freeze_held() {
        assert_eq!(resume_to(&Freeze(true), RunType::Solo), Pause::Frozen);
        assert_eq!(resume_to(&Freeze(true), RunType::Coop), Pause::Running);
    }

    #[test]
    fn one_player_makes_a_solo_run_and_two_a_co_op_one() {
        let mut world = World::new();
        world.spawn(Player);
        world.run_system_once(fix_run_type).unwrap();
        assert_eq!(*world.resource::<RunType>(), RunType::Solo);
        world.spawn(Player);
        world.run_system_once(fix_run_type).unwrap();
        assert_eq!(*world.resource::<RunType>(), RunType::Coop);
    }

    /// The type is read once, as the Run begins: Players added or lost after don't change it.
    #[test]
    fn the_runs_type_is_fixed_at_its_start() {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<GameState>()
            .init_resource::<RunType>()
            .add_systems(OnEnter(GameState::Playing), fix_run_type);
        app.world_mut().spawn(Player);
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
        app.update();
        assert_eq!(*app.world().resource::<RunType>(), RunType::Solo);
        // A second Player joins mid-Run, as the debug key's does.
        app.world_mut().spawn(Player);
        app.update();
        assert_eq!(*app.world().resource::<RunType>(), RunType::Solo);

        // And a Co-op Run that drops to one Player stays Co-op.
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<GameState>()
            .init_resource::<RunType>()
            .add_systems(OnEnter(GameState::Playing), fix_run_type);
        app.world_mut().spawn(Player);
        let second = app.world_mut().spawn(Player).id();
        app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::Playing);
        app.update();
        app.world_mut().entity_mut(second).despawn();
        app.update();
        assert_eq!(*app.world().resource::<RunType>(), RunType::Coop);
    }
}
