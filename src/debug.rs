use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

/// Whether the F1 debug view is on: collider outlines, Zone volumes, the Active Zones,
/// the path-node graph and each Zombie's route.
#[derive(Resource, Default)]
pub struct DebugView(pub bool);

/// F1 toggles the debug view, which only reads state. Every other debug key is gone: what they did
/// to a Run is now a Cheat on the Cheat Panel (`run::dev`), which taints the Run and, in Co-op,
/// goes to the Host as a request.
pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsDebugPlugin)
            .init_resource::<DebugView>()
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                store.config_mut::<PhysicsGizmos>().0.enabled = false;
            })
            .add_systems(Update, toggle.run_if(input_just_pressed(KeyCode::F1)));
    }
}

fn toggle(mut view: ResMut<DebugView>, mut store: ResMut<GizmoConfigStore>) {
    view.0 = !view.0;
    store.config_mut::<PhysicsGizmos>().0.enabled = view.0;
}
