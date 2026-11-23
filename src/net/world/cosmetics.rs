//! Cosmetic facts told over the reliable stream: Frosted Zombies, the Capstones teammates hold, and
//! Host-made cues clients would miss. Effects drawn in a frame are in `crate::net::fx`.

use bevy::prelude::*;

use super::{Heard, Mirror, PlayerSeat, SeatReturned, Tell, WorldEvent};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::net::protocol::Seat;
use crate::net::state::{pack_position, unpack_position, wire_origin};
use crate::net::zombies::{Tag, ZombieId, ZombieIds};
use crate::players::player::{LocalPlayer, Player};
use crate::run::gauntlet::procs::Frosted;
use crate::run::gauntlet::sets::SetBonuses;

/// Frost has to outlast what was last told by more than this for it to be told again.
const FROST_RETELL_SECS: f32 = 0.5;

/// The cues the Host makes for what only it simulates, and so tells: an Ammo Mod's sound, the
/// Pack Alpha's howl.
const SHARED_CUES: [Cue; 8] = [
    Cue::CrashBar,
    Cue::DoorPound,
    Cue::BossHowl,
    Cue::AmmoModBlastFurnace,
    Cue::AmmoModDeadWire,
    Cue::AmmoModFireworks,
    Cue::AmmoModThunderWall,
    Cue::AmmoModTurned,
];

/// Tells everyone of each Frosted Zombie whose frost now lasts more than `FROST_RETELL_SECS` past
/// what was last told.
pub fn tell_frosts(mut tell: Tell, mut mirror: ResMut<Mirror>, frosted: Query<(&ZombieId, &Frosted)>, time: Res<Time>) {
    let now = time.elapsed_secs();
    let mut fresh = Vec::new();
    for (id, frost) in &frosted {
        let until = now + frost.left();
        if mirror.frosts.get(&id.0).is_some_and(|&told| until <= told + FROST_RETELL_SECS) {
            continue;
        }
        let tenths = (frost.left() * 10.0).ceil().clamp(1.0, 255.0) as u8;
        mirror.frosts.insert(id.0, now + f32::from(tenths) / 10.0);
        fresh.push((id.0, Tag::Frosted { tenths }));
    }
    mirror.frosts.retain(|_, told| *told > now);
    if !fresh.is_empty() {
        fresh.sort_by_key(|(id, _)| *id);
        tell.all(&WorldEvent::Tags(fresh));
    }
}

/// Shows each Frosted Zombie the Host told of on its puppet.
pub fn hear_frosts(mut heard: MessageReader<Heard>, mut commands: Commands, ids: Res<ZombieIds>) {
    for Heard(event) in heard.read() {
        let WorldEvent::Tags(tags) = event else { continue };
        for &(id, tag) in tags {
            let Tag::Frosted { tenths } = tag else { continue };
            if let Some(puppet) = ids.entity(id) {
                commands.entity(puppet).insert(Frosted::showing(f32::from(tenths) / 10.0));
            }
        }
    }
}

/// Tells everyone each Seat's Capstones when they change.
pub fn tell_capstones(mut tell: Tell, mut mirror: ResMut<Mirror>, players: Query<(&PlayerSeat, &SetBonuses), With<Player>>) {
    for (seat, sets) in &players {
        let families = sets.capstones();
        if mirror.capstones.get(&seat.0).copied().unwrap_or(0) != families {
            mirror.capstones.insert(seat.0, families);
            tell.all(&WorldEvent::Capstones { seat: seat.0, families });
        }
    }
}

/// Tells a Player who has come back every Capstone held.
pub fn catch_up_capstones(mut tell: Tell, mut returned: MessageReader<SeatReturned>, mirror: Res<Mirror>) {
    for &SeatReturned { seat } in returned.read() {
        let Some(&(peer, _)) = tell.roster.0.iter().find(|&&(_, held)| held == seat.0) else { continue };
        let mut held: Vec<(Seat, u8)> = mirror.capstones.iter().map(|(&seat, &families)| (seat, families)).collect();
        held.sort_by_key(|&(seat, _)| seat);
        for (seat, families) in held.into_iter().filter(|&(_, families)| families != 0) {
            tell.to(peer, &WorldEvent::Capstones { seat, families });
        }
    }
}

/// Gives each teammate the Capstones the Host says they hold, so their rings are drawn.
pub fn hear_capstones(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerSeat), (With<Player>, Without<LocalPlayer>)>,
) {
    for Heard(event) in heard.read() {
        let WorldEvent::Capstones { seat, families } = *event else { continue };
        for (player, held) in &players {
            if held.0 == seat {
                commands.entity(player).insert(SetBonuses::from_capstones(families));
            }
        }
    }
}

/// Tells everyone the shared cues the Host makes.
pub fn tell_cues(mut tell: Tell, mut cues: MessageReader<PlayCue>, map: Option<Res<crate::maps::map::MapId>>) {
    let Some(map) = map else { return };
    let origin = wire_origin(*map);
    for cue in cues.read() {
        if !SHARED_CUES.contains(&cue.cue) {
            continue;
        }
        let Some(code) = Cue::ALL.iter().position(|&known| known == cue.cue) else { continue };
        tell.all(&WorldEvent::Cue { cue: code as u8, at: cue.at.map(|at| pack_position(origin, at)) });
    }
}

/// Plays each cue the Host told of.
pub fn hear_cues(mut heard: MessageReader<Heard>, mut cues: MessageWriter<PlayCue>, map: Option<Res<crate::maps::map::MapId>>) {
    let Some(map) = map else { return };
    let origin = wire_origin(*map);
    for Heard(event) in heard.read() {
        let WorldEvent::Cue { cue, at } = *event else { continue };
        let Some(&cue) = Cue::ALL.get(usize::from(cue)) else { continue };
        if SHARED_CUES.contains(&cue) {
            cues.write(PlayCue::new(cue, at.map(|at| unpack_position(origin, at))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Run, spawn_elite};
    use super::*;
    use crate::net::fx::Fx;
    use crate::run::gauntlet::duos::ShellView;
    use crate::run::gauntlet::procs::ProcFlashes;
    use crate::run::gauntlet::sets::{FrostPatch, SetFlashes};
    use crate::weapons::ammo_mod::Flashes;

    fn run(clients: u8) -> Run {
        Run::with(clients, &|app| {
            app.init_resource::<Flashes>()
                .init_resource::<ProcFlashes>()
                .init_resource::<SetFlashes>()
                .add_message::<PlayCue>();
        })
    }

    /// The Host draws `fx`, as its own systems would, in the resource that holds it.
    fn draw_on_host(run: &mut Run, fx: Fx) {
        let world = run.world(0);
        match fx {
            Fx::AmmoLine { .. } | Fx::AmmoBurst { .. } | Fx::AmmoRing { .. } => {
                world.resource_mut::<Flashes>().sent.push(fx);
            }
            Fx::Proc { .. } => world.resource_mut::<ProcFlashes>().sent.push(fx),
            _ => world.resource_mut::<SetFlashes>().sent.push(fx),
        }
    }

    /// How many of the kinds of thing `fx` is are showing on game `on`.
    fn showing(run: &mut Run, on: usize, fx: Fx) -> usize {
        let world = run.world(on);
        match fx {
            Fx::AmmoLine { .. } | Fx::AmmoBurst { .. } | Fx::AmmoRing { .. } => world.resource::<Flashes>().showing(),
            Fx::Proc { .. } => world.resource::<ProcFlashes>().showing(),
            Fx::Ring { .. } => world.query::<&FrostPatch>().iter(world).count(),
            Fx::Arc { .. } => world.query::<&ShellView>().iter(world).count(),
            Fx::GrowingRing { .. } => world.query::<&crate::run::gauntlet::BurstView>().iter(world).count(),
            _ => world.resource::<SetFlashes>().showing(),
        }
    }

    fn reaches_every_client(fx: Fx) {
        let mut run = run(2);
        draw_on_host(&mut run, fx);
        run.step(10);
        for on in 1..3 {
            assert_eq!(showing(&mut run, on, fx), 1, "game {on}");
        }
        // The Host told them, and kept nothing to tell again.
        run.step(10);
        for on in 1..3 {
            assert_eq!(showing(&mut run, on, fx), 1, "game {on} shows it once");
        }
    }

    const AT: Vec3 = Vec3::new(4.0, 1.0, 2.0);

    #[test]
    fn an_ammo_mod_line_shows_on_every_client() {
        reaches_every_client(Fx::AmmoLine { from: AT, to: AT + Vec3::X, colour: Color::WHITE });
    }

    #[test]
    fn an_ammo_mod_burst_shows_on_every_client() {
        reaches_every_client(Fx::AmmoBurst { at: AT, radius: 2.0, colour: Color::WHITE });
    }

    #[test]
    fn an_ammo_mod_ring_shows_on_every_client() {
        reaches_every_client(Fx::AmmoRing { at: AT, radius: 2.0, colour: Color::WHITE, secs: 5.0 });
    }

    #[test]
    fn a_proc_sphere_shows_on_every_client() {
        reaches_every_client(Fx::Proc { at: AT, radius: 2.0, colour: Color::WHITE, secs: 5.0, sphere: true });
    }

    #[test]
    fn a_ricochet_tracer_shows_on_every_client() {
        reaches_every_client(Fx::Tracer { from: AT, to: AT + Vec3::X, colour: Color::WHITE, secs: 5.0 });
    }

    #[test]
    fn a_cleave_arc_shows_on_every_client() {
        reaches_every_client(Fx::Cleave { feet: AT, forward: Vec3::X, radius: 2.0, colour: Color::WHITE, secs: 5.0 });
    }

    #[test]
    fn a_jackpot_burst_shows_on_every_client() {
        reaches_every_client(Fx::Jackpot { at: AT, radius: 1.0, colour: Color::WHITE, secs: 5.0 });
    }

    #[test]
    fn a_scorched_disc_shows_on_every_client() {
        reaches_every_client(Fx::Scorched { at: AT, radius: 2.0, colour: Color::WHITE, secs: 5.0 });
    }

    #[test]
    fn a_cleave_frost_patch_shows_on_every_client() {
        reaches_every_client(Fx::Ring { at: AT, secs: 5.0 });
    }

    #[test]
    fn a_supply_drop_shell_flies_on_every_client() {
        reaches_every_client(Fx::Arc { from: AT, to: AT + Vec3::new(8.0, 0.0, 0.0) });
    }

    #[test]
    fn a_bloaters_burst_ring_shows_on_every_client() {
        reaches_every_client(Fx::GrowingRing { at: AT, radius: 6.0, secs: 5.0 });
    }

    #[test]
    fn a_classic_coop_client_sees_ammo_mod_flashes_with_no_gauntlet_resources() {
        let mut run = Run::with(1, &|app| {
            app.init_resource::<Flashes>();
        });
        run.world(0).resource_mut::<Flashes>().burst(AT, 2.0, Color::WHITE);
        run.step(10);
        assert_eq!(run.world(1).resource::<Flashes>().showing(), 1);
    }

    #[test]
    fn the_hosts_own_flash_is_not_drawn_twice_by_telling() {
        let mut run = run(1);
        run.world(0).resource_mut::<Flashes>().burst(AT, 2.0, Color::WHITE);
        run.step(5);
        assert_eq!(run.world(0).resource::<Flashes>().showing(), 1);
        assert!(run.world(0).resource::<Flashes>().sent.is_empty());
    }

    #[test]
    fn a_frosted_zombie_is_told_again_only_when_its_frost_outlasts_the_telling_by_half_a_second() {
        let mut run = run(1);
        let zombie = spawn_elite(&mut run, crate::run::gauntlet::EliteTrait::Splitter, crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0));
        run.world(0).entity_mut(zombie).insert(Frosted::showing(3.0));
        run.step(3);
        let id = run.world(0).get::<ZombieId>(zombie).unwrap().0;
        let told = run.world(0).resource::<Mirror>().frosts[&id];
        // A little more frost than was told does not tell again.
        run.world(0).entity_mut(zombie).insert(Frosted::showing(3.2));
        run.step(1);
        assert_eq!(run.world(0).resource::<Mirror>().frosts[&id], told);
        // Much more does.
        run.world(0).entity_mut(zombie).insert(Frosted::showing(6.0));
        run.step(1);
        assert!(run.world(0).resource::<Mirror>().frosts[&id] > told + 1.0);
    }

    #[test]
    fn a_frosted_zombie_shows_its_frost_on_the_clients_puppet() {
        let mut run = run(1);
        let zombie = spawn_elite(&mut run, crate::run::gauntlet::EliteTrait::Splitter, crate::maps::map::PLAYER_START + Vec3::new(8.0, 0.0, 4.0));
        run.step(30);
        run.world(0).entity_mut(zombie).insert(Frosted::showing(2.0));
        run.step(5);
        let id = run.world(0).get::<ZombieId>(zombie).unwrap().0;
        let puppet = run.world(1).resource::<ZombieIds>().entity(id).unwrap();
        assert!(run.world(1).get::<Frosted>(puppet).is_some());
    }

    #[test]
    fn a_teammates_capstones_reach_every_client_and_change_with_them() {
        let mut run = run(2);
        let player = run.player(0, 1);
        run.world(0).entity_mut(player).insert(SetBonuses::from_capstones(0b000100));
        run.step(10);
        for on in [0, 2] {
            let teammate = run.player(on, 1);
            assert_eq!(run.world(on).get::<SetBonuses>(teammate).map(SetBonuses::capstones), Some(0b000100), "game {on}");
        }
        run.world(0).entity_mut(player).insert(SetBonuses::from_capstones(0));
        run.step(10);
        let teammate = run.player(2, 1);
        assert_eq!(run.world(2).get::<SetBonuses>(teammate).map(SetBonuses::capstones), Some(0));
    }

    #[test]
    fn capstones_round_trip_through_the_bitmask() {
        for bits in [0u8, 0b1, 0b100, 0b101010, 0b111111] {
            assert_eq!(SetBonuses::from_capstones(bits).capstones(), bits);
        }
    }

    #[test]
    fn a_hosts_ammo_mod_cue_plays_on_the_clients_and_others_do_not() {
        #[derive(Resource, Default)]
        struct Heard(Vec<Cue>);
        fn note(mut cues: MessageReader<PlayCue>, mut heard: ResMut<Heard>) {
            heard.0.extend(cues.read().map(|cue| cue.cue));
        }
        let mut run = Run::with(1, &|app| {
            app.add_message::<PlayCue>().init_resource::<Heard>().add_systems(Update, note);
        });
        run.step(5);
        let before = run.world(1).resource::<Heard>().0.len();
        run.world(0).write_message(PlayCue::new(Cue::AmmoModDeadWire, Some(AT)));
        run.world(0).write_message(PlayCue::new(Cue::EliteSpawn, None));
        run.world(0).write_message(PlayCue::new(Cue::BossHowl, Some(AT)));
        run.world(0).write_message(PlayCue::new(Cue::BossWarning, None));
        run.step(5);
        let heard = &run.world(1).resource::<Heard>().0[before..];
        assert!(heard.contains(&Cue::AmmoModDeadWire));
        assert!(heard.contains(&Cue::BossHowl), "the howl is the Host's alone to tell");
        assert!(!heard.contains(&Cue::BossWarning), "the warning is inferred from the Round, not told");
        assert!(!heard.contains(&Cue::EliteSpawn));
    }
}
