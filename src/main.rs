// Bevy systems routinely take many params and complex query types.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod audio;
mod authority;
mod bench;
mod debug;
mod economy;
mod enemies;
mod layers;
mod maps;
mod modifier;
mod net;
mod players;
mod run;
mod state;
mod storage;
mod tuning;
mod ui;
mod weapons;

use avian3d::prelude::*;
use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app
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
            run::leave_run::LeaveRunPlugin,
            run::party::PartyPlugin,
            storage::settings::SettingsPlugin,
            ui::menu::MenuPlugin,
            ui::coop::CoopPlugin,
        ))
        // The Map and everything played on it.
        .add_plugins((
            maps::map::MapPlugin,
            maps::zone::ZonePlugin,
            enemies::nav::NavPlugin,
            maps::barrier::BarrierPlugin,
            maps::door::DoorPlugin,
            economy::buy::BuyPlugin,
            players::player::PlayerPlugin,
            weapons::weapon::WeaponPlugin,
            weapons::attachment::AttachmentPlugin,
            // Buying and carrying beyond the starting Weapon.
            (
                weapons::wall_weapon::WallWeaponPlugin,
                weapons::mystery_box::MysteryBoxPlugin,
                weapons::equipment::EquipmentPlugin,
                weapons::explosive::ExplosivePlugin,
                weapons::wonder::WonderPlugin,
            ),
            (
                enemies::zombie::ZombiePlugin,
                enemies::hellhound::HellhoundPlugin,
                enemies::nova::NovaPlugin,
                enemies::special::SpecialPlugin,
                enemies::sapper::SapperPlugin,
                enemies::request::RequestPlugin,
                players::shellshock::ShellshockPlugin,
            ),
            run::round::RoundPlugin,
            economy::points::PointsPlugin,
            run::stats::StatsPlugin,
            storage::records::RecordsPlugin,
        ))
        // How the Run looks and sounds, and the debug keys.
        .add_plugins((
            ui::hud::HudPlugin,
            players::sway::SwayPlugin,
            weapons::muzzle::MuzzlePlugin,
            audio::AudioPlugin,
            debug::DebugPlugin,
        ))
        .add_plugins((
            maps::power::PowerPlugin,
            economy::perk::PerkPlugin,
            players::last_stand::LastStandPlugin,
            players::out_of_bounds::OutOfBoundsPlugin,
            players::revive::RevivePlugin,
            players::respawn::RespawnPlugin,
            weapons::pack_a_punch::PackAPunchPlugin,
            weapons::burn::BurnPlugin,
            maps::trap::TrapPlugin,
            maps::teleporter::TeleporterPlugin,
            weapons::ammo_mod::AmmoModPlugin,
            economy::power_up::PowerUpPlugin,
        ))
        // Gauntlet, layered on top of everything above.
        .add_plugins(run::gauntlet::GauntletPlugin)
        .add_plugins(players::teammate::TeammatePlugin)
        .add_plugins(maps::generator::GeneratorPlugin)
        .add_plugins(maps::surge::SurgePlugin)
        .add_plugins(maps::darkness::DarknessPlugin);
    if let Some(bench) = bench::Bench::from_env() {
        app.add_plugins(bench::BenchPlugin(bench));
    }
    app.run();
}
