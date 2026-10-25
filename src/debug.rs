use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

use crate::arsenal::WeaponId;
use crate::equipment::Equipment;
use crate::player::Player;
use crate::state::Pause;
use crate::weapon::{Loadout, WeaponCopy};

/// The Weapons F2 hands over in turn: one to try switching with, then the special Box Weapons,
/// so each can be tried without spinning the Box for it.
const GRANTED_WEAPONS: [WeaponId; 5] = [
    WeaponId::Mp40,
    WeaponId::RayGun,
    WeaponId::Thundergun,
    WeaponId::Crossbow,
    WeaponId::BallisticKnife,
];

/// Whether the F1 debug view is on: collider outlines, Zone volumes, the Active Zones,
/// the path-node graph and each Zombie's route.
#[derive(Resource, Default)]
pub struct DebugView(pub bool);

/// F1 toggles the debug view. F2 grants the next debug Weapon. F3 grants Monkey Bombs, as the
/// Mystery Box would. F6 Pack-a-Punches the Weapon in hand.
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
            )
            .add_systems(
                Update,
                grant_upgrade
                    .run_if(in_state(Pause::Running))
                    .run_if(input_just_pressed(KeyCode::F6)),
            );
    }
}

fn toggle(mut view: ResMut<DebugView>, mut store: ResMut<GizmoConfigStore>) {
    view.0 = !view.0;
    store.config_mut::<PhysicsGizmos>().0.enabled = view.0;
}

fn grant_weapon(mut loadout: Single<&mut Loadout, With<Player>>, mut next: Local<usize>) {
    loadout.give(GRANTED_WEAPONS[*next % GRANTED_WEAPONS.len()]);
    *next += 1;
}

/// Swaps the Weapon in hand for its Upgraded copy, to try Upgraded Weapons without the Power.
fn grant_upgrade(mut loadout: Single<&mut Loadout, With<Player>>) {
    if !loadout.held().upgraded {
        let id = loadout.held().id;
        loadout.give_copy(WeaponCopy::new(id).upgrade());
    }
}

fn grant_monkey_bombs(mut equipment: Single<&mut Equipment, With<Player>>) {
    equipment.take_monkey_bombs();
}
