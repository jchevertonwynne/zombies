use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    Playing,
    GameOver,
}

#[derive(SubStates, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[source(GameState = GameState::Playing)]
pub enum Pause {
    #[default]
    Running,
    Paused,
    /// PROTOTYPE: a Boon or Pact Offer is on screen, freezing the world.
    Offer,
}

/// Everything spawned for a run. Despawned when leaving Game Over so a restart starts clean.
#[derive(Component, Default)]
pub struct InRun;

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GameState>()
            .add_sub_state::<Pause>()
            .add_systems(OnExit(GameState::GameOver), despawn_run)
            .add_systems(OnEnter(Pause::Paused), pause_time)
            .add_systems(OnExit(Pause::Paused), resume_time)
            .add_systems(OnEnter(Pause::Offer), pause_time)
            .add_systems(OnExit(Pause::Offer), resume_time)
            .add_systems(Update, toggle_pause.run_if(in_state(GameState::Playing)));
    }
}

fn despawn_run(mut commands: Commands, run: Query<Entity, With<InRun>>) {
    for entity in &run {
        commands.entity(entity).despawn();
    }
}

fn toggle_pause(
    input: Res<ButtonInput<KeyCode>>,
    pause: Res<State<Pause>>,
    mut next: ResMut<NextState<Pause>>,
) {
    if input.just_pressed(KeyCode::Escape) {
        next.set(match pause.get() {
            Pause::Running => Pause::Paused,
            Pause::Paused => Pause::Running,
            Pause::Offer => return,
        });
    }
}

fn pause_time(mut time: ResMut<Time<Virtual>>) {
    time.pause();
}

fn resume_time(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}
