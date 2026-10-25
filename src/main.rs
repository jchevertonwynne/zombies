// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod arsenal;
mod audio;
mod buy;
mod combat;
mod debug;
mod door;
mod hud;
mod layers;
mod map;
mod menu;
mod modifier;
mod mode;
mod player;
mod points;
mod records;
mod round;
mod save;
mod settings;
mod state;
mod stats;
mod tuning;
mod wall_weapon;
mod weapon;
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
            door::DoorPlugin,
            buy::BuyPlugin,
            player::PlayerPlugin,
            weapon::WeaponPlugin,
            wall_weapon::WallWeaponPlugin,
            zombie::ZombiePlugin,
            round::RoundPlugin,
            points::PointsPlugin,
            stats::StatsPlugin,
            records::RecordsPlugin,
            hud::HudPlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
        ))
        .run();
}
