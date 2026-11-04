// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod ammo_mod;
mod arsenal;
mod attachment;
mod audio;
mod authority;
mod barrier;
mod buy;
mod burn;
mod combat;
mod controls;
mod coop;
mod debug;
mod door;
mod equipment;
mod explosive;
mod gauntlet;
mod gun_model;
mod hands;
mod hellhound;
mod hud;
mod last_stand;
mod layers;
mod leave_run;
mod map;
mod menu;
mod modifier;
mod mode;
mod muzzle;
mod pack_a_punch;
mod party;
mod perk;
mod mystery_box;
mod nav;
mod net;
mod nova;
mod player;
mod points;
mod records;
mod power;
mod power_up;
mod revive;
mod round;
mod sapper;
mod save;
mod settings;
mod special;
mod shellshock;
mod state;
mod stats;
mod sway;
mod target;
mod teammate;
mod teleporter;
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
            authority::AuthorityPlugin,
            leave_run::LeaveRunPlugin,
            party::PartyPlugin,
            settings::SettingsPlugin,
            menu::MenuPlugin,
            coop::CoopPlugin,
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
            attachment::AttachmentPlugin,
            // Buying and carrying beyond the starting Weapon.
            (
                wall_weapon::WallWeaponPlugin,
                mystery_box::MysteryBoxPlugin,
                equipment::EquipmentPlugin,
                explosive::ExplosivePlugin,
                wonder::WonderPlugin,
            ),
            (
                zombie::ZombiePlugin,
                hellhound::HellhoundPlugin,
                nova::NovaPlugin,
                special::SpecialPlugin,
                sapper::SapperPlugin,
                shellshock::ShellshockPlugin,
            ),
            round::RoundPlugin,
            points::PointsPlugin,
            stats::StatsPlugin,
            records::RecordsPlugin,
        ))
        // How the Run looks and sounds, and the debug keys.
        .add_plugins((
            hud::HudPlugin,
            sway::SwayPlugin,
            muzzle::MuzzlePlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
        ))
        .add_plugins((
            power::PowerPlugin,
            perk::PerkPlugin,
            last_stand::LastStandPlugin,
            revive::RevivePlugin,
            pack_a_punch::PackAPunchPlugin,
            burn::BurnPlugin,
            trap::TrapPlugin,
            teleporter::TeleporterPlugin,
            ammo_mod::AmmoModPlugin,
            power_up::PowerUpPlugin,
        ))
        // Gauntlet, layered on top of everything above.
        .add_plugins(gauntlet::GauntletPlugin)
        .add_plugins(teammate::TeammatePlugin)
        .run();
}
