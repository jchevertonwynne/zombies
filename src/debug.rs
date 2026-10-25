use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

use crate::arsenal::WeaponId;
use crate::player::Player;
use crate::state::Pause;
use crate::weapon::Loadout;

/// The Weapon F2 hands over, to try switching before Wall Weapons exist.
const GRANTED_WEAPON: WeaponId = WeaponId::Mp40;

/// F1 toggles collider outlines. F2 grants a second Weapon.
pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsDebugPlugin)
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                store.config_mut::<PhysicsGizmos>().0.enabled = false;
            })
            .add_systems(Update, toggle.run_if(input_just_pressed(KeyCode::F1)))
            .add_systems(
                Update,
                grant_weapon
                    .run_if(in_state(Pause::Running))
                    .run_if(input_just_pressed(KeyCode::F2)),
            );
    }
}

fn toggle(mut store: ResMut<GizmoConfigStore>) {
    let config = store.config_mut::<PhysicsGizmos>().0;
    config.enabled = !config.enabled;
}

fn grant_weapon(mut loadout: Single<&mut Loadout, With<Player>>) {
    loadout.give(GRANTED_WEAPON);
}
