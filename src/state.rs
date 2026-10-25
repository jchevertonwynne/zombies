use bevy::prelude::*;

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

/// Where leaving the Pause menu goes: back to a frozen world if something still holds it.
pub fn resume_to(freeze: &Freeze) -> Pause {
    if freeze.0 {
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
}

/// Everything spawned for a Run. Despawned when leaving Game Over or returning to the Main Menu,
/// so the next Run starts clean.
#[derive(Component, Default)]
pub struct InRun;

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .init_resource::<Freeze>()
            .add_sub_state::<MainMenuScreen>()
            .add_sub_state::<Pause>()
            .add_sub_state::<PauseScreen>()
            .add_systems(OnExit(GameState::GameOver), despawn_run)
            .add_systems(OnEnter(GameState::MainMenu), despawn_run)
            .add_systems(OnEnter(Pause::Paused), pause_time)
            .add_systems(OnExit(Pause::Paused), resume_time)
            .add_systems(OnEnter(Pause::Frozen), pause_time)
            .add_systems(OnExit(Pause::Frozen), resume_time)
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

fn despawn_run(mut commands: Commands, run: Query<Entity, With<InRun>>) {
    for entity in &run {
        commands.entity(entity).despawn();
    }
}

/// The world freezes while a [`Freeze`] is held and runs again once it's let go. Pause is left
/// alone: leaving it checks the Freeze itself.
fn apply_freeze(freeze: Res<Freeze>, pause: Res<State<Pause>>, mut next: ResMut<NextState<Pause>>) {
    match (pause.get(), freeze.0) {
        (Pause::Running, true) => next.set(Pause::Frozen),
        (Pause::Frozen, false) => next.set(Pause::Running),
        _ => {}
    }
}

/// Esc pauses and resumes. From Pause's Settings it goes back to the Pause menu instead.
fn toggle_pause(
    input: Res<ButtonInput<KeyCode>>,
    pause: Res<State<Pause>>,
    freeze: Res<Freeze>,
    screen: Option<Res<State<PauseScreen>>>,
    mut next: ResMut<NextState<Pause>>,
    mut next_screen: ResMut<NextState<PauseScreen>>,
) {
    if !input.just_pressed(KeyCode::Escape) {
        return;
    }
    match (pause.get(), screen.map(|screen| *screen.get())) {
        (Pause::Paused, Some(PauseScreen::Settings)) => next_screen.set(PauseScreen::Root),
        (Pause::Paused, _) => next.set(resume_to(&freeze)),
        (Pause::Running | Pause::Frozen, _) => next.set(Pause::Paused),
    }
}

fn pause_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn resume_time(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}
