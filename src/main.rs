// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod audio;
mod buy;
mod debug;
mod door;
mod hud;
mod layers;
mod map;
mod player;
mod points;
mod round;
mod state;
mod stats;
mod tuning;
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
            map::MapPlugin,
            zone::ZonePlugin,
            door::DoorPlugin,
            buy::BuyPlugin,
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
