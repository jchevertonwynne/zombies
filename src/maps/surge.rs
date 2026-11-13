//! The Surge: while a Generator is actually Charging, extra normal Zombies come through its
//! Section's Barriers, so a Claim is a fight rather than a wait. They aren't part of the Horde:
//! the Round doesn't wait for them, and those alive when the Surge stops stay and fight. The
//! Host spawns them, and they reach every Player as any other Zombie does.

use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::enemies::target::Players;
use crate::enemies::zombie::{Dying, Zombie, ZombieAssetsParam};
use crate::maps::generator::{Generators, charge};
use crate::maps::map::{MapId, SpawnKind, SpawnPoint};
use crate::maps::zone::{ActiveZones, ZoneId, ZoneSet};
use crate::modifier::{Modifiers, Stat, resolve_count};
use crate::players::player::LocalPlayer;
use crate::run::round::{CarriesOver, Round, spawn_normal_zombie};
use crate::state::{GameState, world_runs};
use crate::tuning::*;

/// A Surge Zombie. It also carries over, so the Round never waits for it.
#[derive(Component)]
pub struct Surge;

/// The most Surge Zombies alive at once with `up` Players up.
pub fn surge_cap(up: u32) -> u32 {
    SURGE_ALIVE_MAX + SURGE_ALIVE_PER_EXTRA_PLAYER * up.saturating_sub(1)
}

/// Where a Surge Zombie for the Generator in `own` comes in, from spawn points by Zone: the ones
/// in its own Zone while that's Active, or else those in the other Active Zones of its `section`.
pub fn surge_entries<T>(points: impl IntoIterator<Item = (ZoneId, T)>, own: ZoneId, section: &[ZoneId], active: ZoneSet) -> Vec<T> {
    let (home, rest): (Vec<_>, Vec<_>) = points
        .into_iter()
        .filter(|(zone, _)| active.contains(*zone) && (*zone == own || section.contains(zone)))
        .partition(|(zone, _)| *zone == own);
    let chosen = if home.is_empty() { rest } else { home };
    chosen.into_iter().map(|(_, point)| point).collect()
}

/// Seconds until each Generator's next Surge Zombie, in `Layout` order. It counts down only while
/// that Generator is Charging, so a paused Charge pauses its Surge too.
#[derive(Resource, Default)]
struct SurgeClocks(Vec<f32>);

pub struct SurgePlugin;

impl Plugin for SurgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurgeClocks>()
            .add_systems(OnEnter(GameState::Playing), |mut clocks: ResMut<SurgeClocks>| clocks.0.clear())
            .add_systems(Update, surge.after(charge).in_set(HostSimulation).run_if(world_runs));
    }
}

/// Each Generator that's Charging sends a normal Zombie, at the Round's health and speed, through
/// its Section's Barriers every `SURGE_SPAWN_GAP_SECS`, while there's room under both the Surge's
/// cap and the Round's Alive Cap.
fn surge(
    mut commands: Commands,
    mut clocks: ResMut<SurgeClocks>,
    generators: Res<Generators>,
    round: Option<Res<Round>>,
    alive: Query<Has<Surge>, (With<Zombie>, Without<Dying>)>,
    spawn_points: Query<(&Transform, &SpawnPoint)>,
    modifiers: Single<&Modifiers, With<LocalPlayer>>,
    players: Players,
    active: Res<ActiveZones>,
    map: Res<MapId>,
    assets: ZombieAssetsParam,
    time: Res<Time>,
) {
    let Some(round) = round else { return };
    let states = generators.states();
    clocks.0.resize(states.len(), 0.0);
    let mut surging = alive.iter().filter(|&surge| surge).count() as u32;
    let mut all = alive.iter().count() as u32;
    let surge_most = surge_cap(players.up_count());
    let alive_most = resolve_count(Stat::AliveCap, round.alive_cap(), &modifiers.0);
    for (index, state) in states.iter().enumerate() {
        if !state.live() {
            continue;
        }
        let clock = &mut clocks.0[index];
        *clock -= time.delta_secs();
        if *clock > 0.0 || surging >= surge_most || all >= alive_most {
            continue;
        }
        let generator = &generators.layout()[index];
        let section = map
            .layout()
            .sections
            .iter()
            .find(|(section, _)| *section == generator.section)
            .map_or(&[][..], |(_, zones)| zones);
        let windows = spawn_points
            .iter()
            .filter(|(_, point)| matches!(point.kind, SpawnKind::Window(_)))
            .map(|(transform, point)| (point.zone, (transform.translation, point.kind)));
        let entries = surge_entries(windows, generator.zone, section, active.0);
        if entries.is_empty() {
            continue;
        }
        let (at, kind) = entries[rand::random_range(0..entries.len())];
        let zombie = spawn_normal_zombie(&mut commands, &assets, round.number, &modifiers.0, at, kind);
        commands.entity(zombie).insert((Surge, CarriesOver));
        *clock = SURGE_SPAWN_GAP_SECS;
        surging += 1;
        all += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_surge_zombies_alive_at_most_and_two_more_for_each_player_past_the_first() {
        assert_eq!(surge_cap(1), 6);
        assert_eq!(surge_cap(2), 8);
        assert_eq!(surge_cap(3), 10);
        assert_eq!(surge_cap(4), 12);
        assert_eq!(surge_cap(0), 6, "nobody up means nobody Charging, but the cap never wraps");
    }

    #[test]
    fn surge_zombies_come_through_the_generators_own_zone_first_then_its_sections_active_zones() {
        use ZoneId::*;
        let section = [PenstockTunnel, TurbineHall, SwitchgearRoom, DrainageGallery];
        let points = [(TurbineHall, 1), (SwitchgearRoom, 2), (CrestRoad, 3), (TurbineHall, 4), (DrainageGallery, 5)];
        let all: ZoneSet = ZoneId::ALL.into_iter().collect();
        assert_eq!(surge_entries(points, TurbineHall, &section, all), vec![1, 4]);
        let away: ZoneSet = [CrestRoad, PenstockTunnel, SwitchgearRoom].into_iter().collect();
        assert_eq!(surge_entries(points, TurbineHall, &section, away), vec![2], "never outside the Section, nor an inactive Zone");
    }
}

/// The Surge on the built Dam: a Player Charging the Turbine Hall's Generator.
#[cfg(test)]
mod surge_tests {
    use super::*;
    use crate::audio::PlayCue;
    use crate::economy::buy::Purchased;
    use crate::enemies::hellhound::HellhoundPlugin;
    use crate::enemies::nova::NovaPlugin;
    use crate::enemies::target::half_height_above_feet;
    use crate::enemies::zombie::headless::Sim;
    use crate::maps::barrier::Entering;
    use crate::maps::generator::GeneratorPlugin;
    use crate::maps::power::Power;
    use crate::players::player::Player;
    use crate::run::round::{RoundPhase, RoundPlugin};
    use crate::storage::settings::Settings;

    const TURBINE: usize = 2;

    fn dam(configure: impl FnOnce(&mut App)) -> Sim {
        Sim::on(MapId::Dam, 30.0, |app| {
            app.add_plugins((GeneratorPlugin, SurgePlugin))
                .insert_resource(ActiveZones(ZoneId::ALL.into_iter().collect()))
                .add_message::<Purchased>()
                .add_message::<PlayCue>();
            configure(app);
        })
    }

    /// The Turbine Hall's Generator bought, and the Player standing up on its plate.
    fn charge_the_turbine_hall(sim: &mut Sim) {
        let generator = MapId::Dam.layout().generators[TURBINE];
        let world = sim.world_mut();
        world.resource_mut::<Generators>().claim(TURBINE);
        let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.get_mut::<Transform>(player).unwrap().translation = generator.at + Vec3::Y * half_height_above_feet(None);
    }

    fn step_off(sim: &mut Sim) {
        let world = sim.world_mut();
        let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
        world.get_mut::<Transform>(player).unwrap().translation = MapId::Dam.layout().seat_start(0) + Vec3::Y * half_height_above_feet(None);
    }

    fn surge_zombies(sim: &mut Sim) -> Vec<Entity> {
        let world = sim.world_mut();
        world.query_filtered::<Entity, (With<Zombie>, With<Surge>)>().iter(world).collect()
    }

    #[test]
    fn a_charging_generator_surges_through_its_own_zones_barriers_up_to_the_cap_and_stops_when_the_plate_empties() {
        let mut sim = dam(|app| {
            app.insert_resource(Round::in_progress(1));
        });
        sim.run(sim.frames(5.0));
        assert!(surge_zombies(&mut sim).is_empty(), "no Surge before anyone Charges");

        charge_the_turbine_hall(&mut sim);
        sim.run(sim.frames(1.0));
        assert_eq!(surge_zombies(&mut sim).len(), 1, "the first comes at once");
        sim.run(sim.frames(1.5));
        assert_eq!(surge_zombies(&mut sim).len(), 2, "then one every {SURGE_SPAWN_GAP_SECS} s");
        sim.run(sim.frames(8.0));
        assert_eq!(surge_zombies(&mut sim).len(), surge_cap(1) as usize, "no more than the cap alive");

        let world = sim.world_mut();
        let turbine_hall: Vec<_> = world
            .query::<&SpawnPoint>()
            .iter(world)
            .filter_map(|point| match point.kind {
                SpawnKind::Window(barrier) if point.zone == ZoneId::TurbineHall => Some(barrier),
                _ => None,
            })
            .collect();
        for zombie in surge_zombies(&mut sim) {
            let entering = sim.world_mut().get::<Entering>(zombie).map(|entering| entering.barrier);
            assert!(entering.is_none_or(|barrier| turbine_hall.contains(&barrier)), "{entering:?}");
        }

        // Off the plate, the Surge stops; those alive stay.
        step_off(&mut sim);
        for zombie in surge_zombies(&mut sim) {
            sim.world_mut().despawn(zombie);
        }
        sim.run(sim.frames(5.0));
        assert!(surge_zombies(&mut sim).is_empty(), "no Surge while the plate is empty");
    }

    #[test]
    fn the_round_ends_with_surge_zombies_still_alive() {
        let mut sim = dam(|app| {
            app.add_plugins((RoundPlugin, HellhoundPlugin, NovaPlugin))
                .insert_resource(Settings::default())
                .insert_resource(Power::default())
                .init_resource::<crate::ui::hud::overlay::Overlays>()
                .add_message::<crate::economy::power_up::DropPowerUp>()
                .add_message::<crate::players::shellshock::Shellshock>();
        });
        // The whole Horde has spawned bar one Zombie still alive.
        let world = sim.world_mut();
        let horde: Vec<Entity> = world.query_filtered::<Entity, With<Zombie>>().iter(world).collect();
        for zombie in horde {
            world.despawn(zombie);
        }
        while world.resource_mut::<Round>().take_from_horde() {}
        let last = sim.spawn(MapId::Dam.layout().generators[TURBINE].at + Vec3::new(-3.0, 0.0, 0.0), None);

        charge_the_turbine_hall(&mut sim);
        sim.run(sim.frames(3.0));
        assert!(!surge_zombies(&mut sim).is_empty());
        assert_eq!(sim.world_mut().resource::<Round>().phase, RoundPhase::Active, "the Horde's last Zombie is alive");

        sim.world_mut().despawn(last);
        sim.run(2);
        assert!(!surge_zombies(&mut sim).is_empty(), "the Surge Zombies are still alive");
        assert!(
            matches!(sim.world_mut().resource::<Round>().phase, RoundPhase::Intermission(_)),
            "the Round doesn't wait for the Surge"
        );
    }
}
