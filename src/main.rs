// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod audio;
mod debug;
mod hud;
mod hud_prototype;
mod layers;
mod map;
mod player;
mod points;
mod round;
mod state;
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
            map::MapPlugin,
            player::PlayerPlugin,
            weapon::WeaponPlugin,
            zombie::ZombiePlugin,
            round::RoundPlugin,
            points::PointsPlugin,
            hud::HudPlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
            hud_prototype::HudPrototypePlugin,
        ))
        .run();
}
