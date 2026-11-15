use avian3d::prelude::*;
use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;

use crate::weapons::arsenal::WeaponId;
use crate::weapons::equipment::Equipment;
use crate::ui::hud::overlay::{Overlay, Overlays};
use crate::players::player::{LocalPlayer, spawn_debug_player};
use crate::economy::power_up::DropPowerUp;
use crate::enemies::sapper::SummonSapper;
use crate::authority::HostSimulation;
use crate::maps::generator::Generators;
use crate::state::input_runs;
use crate::weapons::weapon::{Loadout, WeaponCopy};

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
/// Mystery Box would. F6 Pack-a-Punches the Weapon in hand. F7 drops the next Power-up from the
/// bag just ahead of the Player. F8 pulses the next effect overlay. F4 (hold fn on a Mac) puts a
/// second Player in Seat 1. F10 summons the Sapper, after
/// its warning, once the Round is under way. [ (beside it, and not an F-key, which a Mac's
/// keyboard takes) Claims every Generator at once, on the Host, and ] Loses the Held one nearest
/// the Player.
pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsDebugPlugin)
            .init_resource::<DebugView>()
            .add_systems(Startup, |mut store: ResMut<GizmoConfigStore>| {
                store.config_mut::<PhysicsGizmos>().0.enabled = false;
            })
            .add_systems(Update, toggle.run_if(input_just_pressed(KeyCode::F1)))
            // Backslash (not an F-key) Swings the nearest bridge now: no price, warning or cooldown.
            .add_systems(
                Update,
                crate::maps::shifting::debug_swing
                    .in_set(HostSimulation)
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::Backslash)),
            )
            .add_systems(
                Update,
                grant_weapon
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::F2)),
            )
            .add_systems(
                Update,
                grant_monkey_bombs
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::F3)),
            )
            .add_systems(
                Update,
                grant_upgrade
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::F6)),
            )
            .add_systems(
                Update,
                drop_power_up
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::F7)),
            )
            .add_systems(
                Update,
                (|mut summons: MessageWriter<SummonSapper>| {
                    summons.write(SummonSapper);
                })
                .run_if(input_runs)
                .run_if(input_just_pressed(KeyCode::F10)),
            )
            .add_systems(
                Update,
                claim_every_generator
                    .in_set(HostSimulation)
                    .run_if(resource_exists::<Generators>)
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::BracketLeft)),
            )
            .add_systems(
                Update,
                lose_nearest_generator
                    .in_set(HostSimulation)
                    .run_if(resource_exists::<Generators>)
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::BracketRight)),
            )
            .add_systems(
                Update,
                spawn_debug_player
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::F4)),
            )
            .add_systems(
                Update,
                pulse_overlay
                    .run_if(input_runs)
                    .run_if(input_just_pressed(KeyCode::F8)),
            );
    }
}

fn claim_every_generator(mut generators: ResMut<Generators>) {
    for index in 0..generators.states().len() {
        generators.hold(index);
    }
}

/// Loses the Held Generator nearest the local Player, as a Sapper would.
fn lose_nearest_generator(mut generators: ResMut<Generators>, player: Query<&Transform, With<LocalPlayer>>) {
    let Ok(player) = player.single() else { return };
    if let Some(index) = generators.nearest_held(player.translation) {
        generators.lose(index);
    }
}

fn toggle(mut view: ResMut<DebugView>, mut store: ResMut<GizmoConfigStore>) {
    view.0 = !view.0;
    store.config_mut::<PhysicsGizmos>().0.enabled = view.0;
}

fn grant_weapon(mut loadout: Single<&mut Loadout, With<LocalPlayer>>, mut next: Local<usize>) {
    loadout.give(GRANTED_WEAPONS[*next % GRANTED_WEAPONS.len()]);
    *next += 1;
}

/// Swaps the Weapon in hand for its Upgraded copy, to try Upgraded Weapons without the Power.
fn grant_upgrade(mut loadout: Single<&mut Loadout, With<LocalPlayer>>) {
    if !loadout.held().upgraded {
        let id = loadout.held().id;
        loadout.give_copy(WeaponCopy::new(id).upgrade());
    }
}

/// Drops a Power-up 3 m ahead of the Player, at their feet.
fn drop_power_up(player: Single<&Transform, With<LocalPlayer>>, mut drops: MessageWriter<DropPowerUp>) {
    let ahead = player.forward() * 3.0;
    let feet = player.translation.y - (crate::tuning::PLAYER_RADIUS + crate::tuning::PLAYER_CAPSULE_LENGTH / 2.0);
    drops.write(DropPowerUp {
        at: Vec3::new(player.translation.x + ahead.x, feet, player.translation.z + ahead.z),
        power_up: None,
    });
}

fn grant_monkey_bombs(mut equipment: Single<&mut Equipment, With<LocalPlayer>>) {
    equipment.take_monkey_bombs();
}

/// Pulses the effect overlays whose sources aren't built yet, one per press, to see each.
fn pulse_overlay(mut overlays: ResMut<Overlays>, mut next: Local<usize>) {
    const TRIED: [Overlay; 3] = [Overlay::Shellshock, Overlay::NapalmHeat, Overlay::TeleporterDaze];
    overlays.pulse(TRIED[*next % TRIED.len()], 1.0, 4.0);
    *next += 1;
}
