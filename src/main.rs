// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod arsenal;
mod audio;
mod combat;
mod debug;
mod hud;
mod layers;
mod map;
mod menu;
mod modifier;
mod mode;
mod player;
mod points;
mod round;
mod save;
mod settings;
mod state;
mod stats;
mod tuning;
mod weapon;
mod zombie;

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
            map::MapPlugin,
            player::PlayerPlugin,
            weapon::WeaponPlugin,
            zombie::ZombiePlugin,
            round::RoundPlugin,
            points::PointsPlugin,
            stats::StatsPlugin,
            hud::HudPlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
        ))
        .run();
}
