// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod arsenal;
mod audio;
mod barrier;
mod buy;
mod burn;
mod combat;
mod debug;
mod door;
mod equipment;
mod explosive;
mod hud;
mod last_stand;
mod layers;
mod map;
mod menu;
mod modifier;
mod mode;
mod pack_a_punch;
mod perk;
mod mystery_box;
mod nav;
mod player;
mod points;
mod records;
mod power;
mod power_up;
mod round;
mod save;
mod settings;
mod state;
mod stats;
mod trap;
mod tuning;
mod wall_weapon;
mod weapon;
mod wonder;
mod zombie;
mod zone;

use avian3d::prelude::*;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Zombies".into(),
                    ..default()
                }),
                ..default()
            }),
            PhysicsPlugins::default(),
        ))
        .add_plugins((
            state::StatePlugin,
            settings::SettingsPlugin,
            menu::MenuPlugin,
        ))
        // The Map and everything played on it.
        .add_plugins((
            map::MapPlugin,
            zone::ZonePlugin,
            nav::NavPlugin,
            barrier::BarrierPlugin,
            door::DoorPlugin,
            buy::BuyPlugin,
            player::PlayerPlugin,
            weapon::WeaponPlugin,
            // Buying and carrying beyond the starting Weapon.
            (
                wall_weapon::WallWeaponPlugin,
                mystery_box::MysteryBoxPlugin,
                equipment::EquipmentPlugin,
                explosive::ExplosivePlugin,
                wonder::WonderPlugin,
            ),
            zombie::ZombiePlugin,
            round::RoundPlugin,
            points::PointsPlugin,
            stats::StatsPlugin,
            records::RecordsPlugin,
        ))
        // How the Run looks and sounds, and the debug keys.
        .add_plugins((
            hud::HudPlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
        ))
        .add_plugins((
            power::PowerPlugin,
            perk::PerkPlugin,
            last_stand::LastStandPlugin,
            pack_a_punch::PackAPunchPlugin,
            burn::BurnPlugin,
            trap::TrapPlugin,
            power_up::PowerUpPlugin,
        ))
        .run();
}
