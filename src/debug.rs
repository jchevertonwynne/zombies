use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

use crate::arsenal::WeaponId;
use crate::equipment::Equipment;
use crate::player::Player;
use crate::state::Pause;
use crate::weapon::Loadout;

/// The Weapon F2 hands over, to try switching before Wall Weapons exist.
const GRANTED_WEAPON: WeaponId = WeaponId::Mp40;

/// Whether the F1 debug view is on: collider outlines, Zone volumes and the Active Zones.
#[derive(Resource, Default)]
pub struct DebugView(pub bool);

/// F1 toggles the debug view. F2 grants a second Weapon. F3 grants Monkey Bombs, as the
/// Mystery Box would.
pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsDebugPlugin)
            .init_resource::<DebugView>()
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                store.config_mut::<PhysicsGizmos>().0.enabled = false;
            })
            .add_systems(Update, toggle.run_if(input_just_pressed(KeyCode::F1)))
            .add_systems(
                Update,
                grant_weapon
                    .run_if(in_state(Pause::Running))
                    .run_if(input_just_pressed(KeyCode::F2)),
            )
            .add_systems(
                Update,
                grant_monkey_bombs
                    .run_if(in_state(Pause::Running))
                    .run_if(input_just_pressed(KeyCode::F3)),
            );
    }
}

fn toggle(mut view: ResMut<DebugView>, mut store: ResMut<GizmoConfigStore>) {
    view.0 = !view.0;
    store.config_mut::<PhysicsGizmos>().0.enabled = view.0;
}

fn grant_weapon(mut loadout: Single<&mut Loadout, With<Player>>) {
    loadout.give(GRANTED_WEAPON);
}

fn grant_monkey_bombs(mut equipment: Single<&mut Equipment, With<Player>>) {
    equipment.take_monkey_bombs();
}
