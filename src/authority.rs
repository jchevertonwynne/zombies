//! Who runs the world. Solo and a Co-op Host run all of it; a Co-op client runs only what its own
//! Player does and shows what the Host sends (see ADR 0003).
//!
//! Avian3d runs on every machine, `Client` included. Zombies are `Kinematic` bodies moved by
//! `MoveAndSlide` from their chase systems, so with none on a `Client` the physics step has
//! nothing to simulate there, and `HostSimulation` is what keeps them off it. The step is still
//! needed: it updates the collider tree that `MoveAndSlide` and every `SpatialQuery` read, which
//! the `LocalPlayer`'s movement and its Weapons' hit scans depend on, and it eases the
//! `LocalPlayer`'s transform. Pausing `Time<Physics>` on a `Client` would freeze that tree.

use bevy::prelude::*;

use crate::state::GameState;

/// Whether this machine runs the simulation. Solo and the Co-op Host are `Host`.
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Authority {
    #[default]
    Host,
    #[allow(
        dead_code,
        reason = "nothing joins a Room as a client until the netcode lands"
    )]
    Client,
}

/// Everything only the `Host` runs: Zombie, Hellhound and special AI and spawning, Rounds, Points
/// awards, Buyables resolving `UseRequest`s and the machines acting on what was bought, Doors,
/// Power, Barriers, Traps, Teleporters, Power-ups, Downs and Revives, Gauntlet Offers, and all
/// harm to Players. A plugin puts such a system `.in_set(HostSimulation)`; the one place that
/// gates the set is `AuthorityPlugin`. What every machine runs (the `LocalPlayer`'s movement,
/// Weapons, Equipment and Knife, view model, HUD, audio and menus) stays out of it.
///
/// A system set's condition belongs to the schedule it is configured in, so every schedule a
/// `HostSimulation` system is added to is configured below.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostSimulation;

pub struct AuthorityPlugin;

impl Plugin for AuthorityPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Authority>();
        let host = || HostSimulation.run_if(in_state(Authority::Host));
        app.configure_sets(Update, host())
            .configure_sets(FixedUpdate, host())
            .configure_sets(OnEnter(GameState::Playing), host());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemies::hellhound::HellhoundPlugin;
    use crate::enemies::nova::NovaPlugin;
    use crate::players::player::Player;
    use crate::economy::points::{Points, PointsPlugin};
    use crate::maps::power::Power;
    use crate::run::round::{Round, RoundPlugin};
    use crate::storage::settings::Settings;
    use crate::enemies::zombie::headless::Sim;
    use crate::enemies::zombie::{Zombie, ZombieHit};
    use crate::maps::zone::{ActiveZones, ZoneId};

    /// A Run as `authority` for a few seconds: how many Zombies there are, the Round and how much
    /// of its Horde has spawned, and what the Player earned from a hit that pays.
    fn run_as(authority: Authority) -> (usize, Option<(u32, u32)>, u64) {
        let mut sim = Sim::new_as(authority, 60.0, |app| {
            app.add_plugins((RoundPlugin, PointsPlugin, HellhoundPlugin, NovaPlugin))
                .insert_resource(Settings::default())
                .insert_resource(Power::default())
                .insert_resource(ActiveZones(ZoneId::ALL.into_iter().collect()))
                .init_resource::<crate::ui::hud::overlay::Overlays>()
                .add_message::<crate::economy::power_up::DropPowerUp>()
                .add_message::<crate::players::shellshock::Shellshock>();
        });
        let world = sim.world_mut();
        let player = world
            .query_filtered::<Entity, With<Player>>()
            .single(world)
            .unwrap();
        world.entity_mut(player).insert(Points::default());
        sim.send(ZombieHit {
            player: Some(player),
            pays: true,
            headshot: false,
        });
        sim.run(sim.frames(5.0));
        let world = sim.world_mut();
        let zombies = world.query::<&Zombie>().iter(world).count();
        let round = world
            .get_resource::<Round>()
            .map(|round| (round.number, round.horde_spawned()));
        let earned = world.get::<Points>(player).unwrap().earned;
        (zombies, round, earned)
    }

    #[test]
    fn the_host_spawns_zombies_starts_the_round_and_awards_points() {
        let (zombies, round, earned) = run_as(Authority::Host);
        assert!(zombies > 0, "no Zombies spawned");
        assert!(
            matches!(round, Some((1, spawned)) if spawned > 0),
            "{round:?}"
        );
        assert!(earned > 0, "no Points awarded");
    }

    #[test]
    fn a_client_spawns_no_zombies_makes_no_round_progress_and_awards_no_points() {
        let (zombies, round, earned) = run_as(Authority::Client);
        assert_eq!(zombies, 0, "a Client spawned Zombies");
        assert!(round.is_none(), "a Client started a Round: {round:?}");
        assert_eq!(earned, 0, "a Client awarded Points");
    }
}
