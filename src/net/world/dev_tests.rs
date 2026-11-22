//! Developer mode over the wire: a client's Cheat request is judged by the Host, who taints the
//! Run for everyone, and the Host's setting (and Seat-kept Cheats) reach and survive Rejoins.

use super::downs::coop;
use super::tests::Run;
use super::*;
use crate::economy::points::PointsPlugin;
use crate::net::away::{Reach, Rejoin};
use crate::players::player::Noclip;
use crate::enemies::request::{RequestedKind, SpawnAt, SpawnZombie};
use crate::maps::power::Power;
use crate::run::dev::oneshots::{RoundJump, Spawn, Spawnable};
use crate::run::dev::{CHEAT_POINTS, Cheat, Cheats, DevPlugin, SetDevMode};
use crate::run::gauntlet::arc::SummonBoss;
use crate::run::mode::Mode;
use crate::run::round::{BeginRound, ReservedRounds, begin_requested_round};
use crate::storage::settings::Settings;
use crate::economy::perk::{GivePerk, Perk, Perks, hand_over_perks};
use crate::economy::power_up::{GivePowerUp, PowerUp};
use crate::modifier::{Modifiers, Source};
use crate::run::gauntlet::{GrantBoon, GrantPact, HeldBoons, HeldPacts, apply_grants, boon_catalog, pact_catalog};
use crate::storage::records::{RunTainted, TaintRun};
use crate::weapons::arsenal::WeaponId;
use crate::maps::generator::{Generators, LoseNearestGenerator};
use crate::maps::map::MapId;
use crate::maps::shifting::SwingNearest;
use crate::maps::tide::TurnTide;
use crate::run::gauntlet::ToggleUnlockAll;
use crate::weapons::equipment::{Equipment, GiveMonkeyBombs};
use crate::weapons::weapon::{GiveWeapon, Loadout, give_weapons};

/// The Power-ups given, as a Run has them (the pickup itself is Classic's own).
#[derive(Resource, Default)]
struct PowerUpsGiven(Vec<(PowerUp, Option<Entity>)>);

fn note_power_ups(mut given: MessageReader<GivePowerUp>, mut seen: ResMut<PowerUpsGiven>) {
    seen.0.extend(given.read().map(|&GivePowerUp(power_up, taker)| (power_up, taker)));
}

fn with_dev(app: &mut App) {
    coop(app);
    app.add_plugins((PointsPlugin, DevPlugin))
        .init_resource::<Settings>()
        .init_resource::<ReservedRounds>()
        .init_resource::<Asked>()
        .add_systems(Update, (begin_requested_round, note_asked).in_set(HostSimulation))
        .init_resource::<PowerUpsGiven>()
        .add_message::<GivePowerUp>()
        .add_message::<GiveWeapon>()
        .add_message::<GivePerk>()
        .add_message::<GrantBoon>()
        .add_message::<GrantPact>()
        .add_systems(Update, (give_weapons, hand_over_perks, apply_grants, note_power_ups).in_set(HostSimulation))
        .init_resource::<RunTainted>()
        .add_message::<TaintRun>()
        .add_systems(Update, |mut asked: MessageReader<TaintRun>, mut tainted: ResMut<RunTainted>| {
            if asked.read().count() > 0 {
                tainted.0 = true;
            }
        });
}

/// The Zombies and Bosses the Host was asked for.
#[derive(Resource, Default)]
struct Asked {
    spawns: Vec<SpawnZombie>,
    bosses: Vec<SummonBoss>,
    rounds: Vec<u32>,
    monkey_bombs: usize,
    generators: Vec<Vec3>,
    swings: Vec<Vec3>,
    tides: usize,
    unlocks: usize,
}

fn note_asked(
    mut asked: ResMut<Asked>,
    mut spawns: MessageReader<SpawnZombie>,
    mut bosses: MessageReader<SummonBoss>,
    mut rounds: MessageReader<BeginRound>,
    mut monkey_bombs: MessageReader<GiveMonkeyBombs>,
    mut generators: MessageReader<LoseNearestGenerator>,
    mut swings: MessageReader<SwingNearest>,
    mut tides: MessageReader<TurnTide>,
    mut unlocks: MessageReader<ToggleUnlockAll>,
) {
    asked.monkey_bombs += monkey_bombs.read().count();
    asked.generators.extend(generators.read().map(|request| request.at));
    asked.swings.extend(swings.read().map(|request| request.at));
    asked.tides += tides.read().count();
    asked.unlocks += unlocks.read().count();
    asked.spawns.extend(spawns.read().copied());
    asked.bosses.extend(bosses.read().copied());
    asked.rounds.extend(rounds.read().map(|round| round.0));
}

/// A Co-op Run of the Host and `clients`, with Developer mode set to `dev` on every machine.
fn run(clients: u8, dev: DevMode) -> Run {
    run_in(clients, dev, Mode::Classic)
}

/// The same under `mode`.
fn run_in(clients: u8, dev: DevMode, mode: Mode) -> Run {
    let mut run = Run::with(clients, &|app| {
        with_dev(app);
        app.insert_resource(mode);
    });
    run.step(5);
    for on in 0..=usize::from(clients) {
        run.world(on).resource_mut::<DevAccess>().0 = dev;
    }
    run.step(5);
    run
}

fn ask(run: &mut Run, on: usize) {
    ask_for(run, on, Cheat::Points);
}

fn ask_for(run: &mut Run, on: usize, cheat: Cheat) {
    let player = run.player(on, on as u8);
    run.world(on).write_message(CheatRequest { player, cheat });
    run.step(20);
}

fn round_number(run: &mut Run, on: usize) -> u32 {
    run.world(on).resource::<Round>().number
}

fn tainted(run: &mut Run, on: usize) -> bool {
    run.world(on).resource::<RunTainted>().0
}

#[test]
fn a_clients_cheat_goes_through_when_developer_mode_is_on_and_taints_everyone() {
    let mut run = run(2, DevMode::On);
    run.give_points(1, 0);
    run.step(5);
    for on in 0..3 {
        assert!(!tainted(&mut run, on), "opening the panel alone taints nothing");
    }
    ask(&mut run, 1);
    assert_eq!(run.balance(1), CHEAT_POINTS);
    for on in 0..3 {
        assert!(tainted(&mut run, on), "game {on}");
    }
}

#[test]
fn a_request_the_setting_forbids_is_dropped_silently() {
    for (dev, on) in [(DevMode::Off, 1), (DevMode::HostOnly, 1), (DevMode::Off, 0)] {
        let mut run = run(1, dev);
        run.give_points(on as u8, 0);
        run.step(5);
        ask(&mut run, on);
        assert_eq!(run.balance(on as u8), 0, "{dev:?} game {on}");
        for game in 0..2 {
            assert!(!tainted(&mut run, game), "{dev:?} game {game}");
        }
    }
}

#[test]
fn host_only_still_lets_the_host_cheat() {
    let mut run = run(1, DevMode::HostOnly);
    run.give_points(0, 0);
    run.step(5);
    ask(&mut run, 0);
    assert_eq!(run.balance(0), CHEAT_POINTS);
    assert!(tainted(&mut run, 1));
}

#[test]
fn the_hosts_change_of_setting_reaches_every_client() {
    let mut run = run(2, DevMode::On);
    run.world(0).write_message(SetDevMode(DevMode::HostOnly));
    run.step(20);
    for on in 0..3 {
        assert_eq!(run.world(on).resource::<DevAccess>().0, DevMode::HostOnly, "game {on}");
    }
}

#[test]
fn off_takes_every_players_cheats_away_and_a_rejoiner_hears_the_setting_and_keeps_theirs() {
    let mut run = run(2, DevMode::On);
    ask(&mut run, 1);
    let guest = run.player(0, 1);
    assert_eq!(run.world(0).get::<Cheats>(guest).unwrap().points_given, CHEAT_POINTS);

    // A Player whose way breaks keeps their Cheats, by Seat, and hears the setting on return.
    run.cut(1);
    run.step(2);
    run.world(0).write_message(SetDevMode(DevMode::HostOnly));
    run.step(40);
    assert_eq!(run.world(1).resource::<Rejoin>().reach, Reach::Up);
    run.world(0).write_message(RoundStarted);
    run.step(30);
    assert_eq!(run.world(1).resource::<DevAccess>().0, DevMode::HostOnly, "the Rejoiner hears the setting");
    assert!(tainted(&mut run, 1), "and that the Run is Tainted");
    let guest = run.player(0, 1);
    assert_eq!(run.world(0).get::<Cheats>(guest).unwrap().points_given, CHEAT_POINTS, "kept by Seat");

    run.world(0).write_message(SetDevMode(DevMode::Off));
    run.step(20);
    assert!(run.world(0).get::<Cheats>(guest).is_none());
    assert!(tainted(&mut run, 0), "the Run stays Tainted");
    assert_eq!(run.world(2).resource::<DevAccess>().0, DevMode::Off);
}

fn toggle(run: &mut Run, on: usize, cheat: Cheat) {
    let player = run.player(on, on as u8);
    run.world(on).write_message(CheatRequest { player, cheat });
    run.step(20);
}

fn toggles_of(run: &mut Run, on: usize, seat: u8) -> u8 {
    let player = run.player(on, seat);
    Cheats::toggles(run.world(on).get::<Cheats>(player))
}

fn health(run: &mut Run, seat: u8) -> f32 {
    let player = run.player(0, seat);
    run.world(0).get::<Health>(player).unwrap().current
}

fn hit(run: &mut Run, seat: u8, damage: f32) {
    let player = run.player(0, seat);
    run.world(0).write_message(PlayerHit { player, damage, from: None });
    run.step(5);
}

#[test]
fn a_clients_toggle_is_judged_by_the_host_and_told_to_that_client_alone() {
    let mut run = run(2, DevMode::On);
    toggle(&mut run, 1, Cheat::InfiniteAmmo);
    assert_eq!(toggles_of(&mut run, 0, 1), 4, "the Host holds it by Seat");
    assert_eq!(toggles_of(&mut run, 1, 1), 4, "the client is told its own");
    assert_eq!(toggles_of(&mut run, 2, 2), 0);
    assert_eq!(toggles_of(&mut run, 2, 1), 0, "a teammate is not told");
    toggle(&mut run, 1, Cheat::InfiniteAmmo);
    assert_eq!(toggles_of(&mut run, 0, 1), 0);
    assert_eq!(toggles_of(&mut run, 1, 1), 0, "and told it is off");
}

#[test]
fn a_forbidden_toggle_is_dropped_and_nothing_is_told() {
    let mut run = run(1, DevMode::HostOnly);
    toggle(&mut run, 1, Cheat::Invincible);
    assert_eq!(toggles_of(&mut run, 0, 1), 0);
    assert_eq!(toggles_of(&mut run, 1, 1), 0);
    let player = run.player(0, 1);
    assert!(run.world(0).get::<Invincible>(player).is_none());
    assert!(!tainted(&mut run, 0));
}

#[test]
fn an_invincible_client_takes_no_hit_and_a_vulnerable_one_does() {
    let mut run = run(1, DevMode::On);
    hit(&mut run, 1, 30.0);
    assert_eq!(health(&mut run, 1), 70.0);
    toggle(&mut run, 1, Cheat::Invincible);
    let player = run.player(0, 1);
    assert!(run.world(0).get::<Invincible>(player).is_some(), "the Host judges harm");
    hit(&mut run, 1, 500.0);
    assert_eq!(health(&mut run, 1), 70.0);
    toggle(&mut run, 1, Cheat::Invincible);
    hit(&mut run, 1, 30.0);
    assert_eq!(health(&mut run, 1), 40.0);
}

#[test]
fn turning_invincibility_on_in_last_stand_gets_the_player_up_with_their_points_back() {
    let mut run = run(1, DevMode::On);
    run.give_points(1, 1000);
    run.step(5);
    super::downs::knock_down(&mut run, 1);
    run.step(5);
    let player = run.player(0, 1);
    assert!(run.world(0).get::<LastStand>(player).is_some(), "Down");
    let after_down = run.balance(1);
    assert!(after_down < 1000);
    toggle(&mut run, 1, Cheat::Invincible);
    assert!(run.world(0).get::<LastStand>(player).is_none(), "up again");
    assert_eq!(health(&mut run, 1), 100.0);
    assert_eq!(run.balance(1), 1000, "the Down's cost is given back");
    let on_client = run.player(1, 1);
    assert!(run.world(1).get::<LastStand>(on_client).is_none(), "and the client sees it");
}

#[test]
fn free_purchases_cost_a_client_nothing_and_stop_when_switched_off() {
    let mut run = run(1, DevMode::On);
    run.give_points(1, 5000);
    run.step(5);
    toggle(&mut run, 1, Cheat::FreePurchases);
    let door = run.find::<Door>(1, |door| door.price.is_some_and(|price| price <= 1000 && price > 0) && !door.open);
    run.use_buyable(1, door, false);
    run.step(20);
    assert!(run.world(1).get::<Door>(door).unwrap().open, "bought");
    assert_eq!(run.balance(1), 5000, "for nothing");
    // The client's prompts show the free price too.
    let on_client = run.player(1, 1);
    let entries = &run.world(1).get::<Modifiers>(on_client).unwrap().0;
    assert_eq!(crate::economy::buy::charged(Some(750), entries), Some(0));
    toggle(&mut run, 1, Cheat::FreePurchases);
    let on_client = run.player(1, 1);
    let entries = &run.world(1).get::<Modifiers>(on_client).unwrap().0;
    assert_eq!(crate::economy::buy::charged(Some(750), entries), Some(750));
}

#[test]
fn infinite_ammo_is_the_clients_own_to_resolve_and_fills_its_magazines() {
    let mut run = run(1, DevMode::On);
    let player = run.player(1, 1);
    {
        let mut loadout = run.world(1).get_mut::<Loadout>(player).unwrap();
        loadout.held_mut().magazine = 0;
        loadout.held_mut().reserve = 0;
    }
    toggle(&mut run, 1, Cheat::InfiniteAmmo);
    let loadout = run.world(1).get::<Loadout>(player).unwrap();
    assert!(loadout.held().magazine > 0, "filled as it is switched on");
    let entries = run.world(1).get::<Modifiers>(player).unwrap().0.clone();
    assert!(crate::modifier::ammo_is_free(&entries), "the entry is on the client that resolves ammo");
    let mut loadout = run.world(1).get_mut::<Loadout>(player).unwrap();
    let (magazine, reserve) = (loadout.held().magazine, loadout.held().reserve);
    for _ in 0..20 {
        loadout.tick(1.0);
        assert!(loadout.try_fire(true, true, &entries));
    }
    assert_eq!((loadout.held().magazine, loadout.held().reserve), (magazine, reserve));
}

#[test]
fn a_rejoiner_is_told_the_toggles_the_host_kept_for_their_seat() {
    let mut run = run(1, DevMode::On);
    toggle(&mut run, 1, Cheat::InfiniteAmmo);
    toggle(&mut run, 1, Cheat::Invincible);
    run.cut(1);
    run.step(2);
    // The Rejoiner's game starts over without them.
    let player = run.player(1, 1);
    run.world(1).entity_mut(player).remove::<Cheats>();
    run.step(40);
    assert_eq!(run.world(1).resource::<Rejoin>().reach, Reach::Up);
    run.world(0).write_message(RoundStarted);
    run.step(30);
    let guest = run.player(0, 1);
    assert_eq!(Cheats::toggles(run.world(0).get::<Cheats>(guest)), 5, "kept by Seat");
    assert_eq!(toggles_of(&mut run, 1, 1), 5, "and told again");
}

fn noclip(run: &mut Run, on: usize) {
    let player = run.player(on, on as u8);
    run.world(on).write_message(CheatRequest { player, cheat: Cheat::Noclip });
    run.step(20);
}

fn flying(run: &mut Run, on: usize) -> bool {
    let player = run.player(on, on as u8);
    run.world(on).get::<Noclip>(player).is_some()
}

#[test]
fn a_noclip_request_flies_the_askers_own_player_and_taints_everyone() {
    let mut run = run(2, DevMode::On);
    assert!(!flying(&mut run, 1));
    noclip(&mut run, 1);
    assert!(flying(&mut run, 1), "predicted on the client at once");
    assert!(!flying(&mut run, 2) && !flying(&mut run, 0), "it is the Player's own");
    for on in 0..3 {
        assert!(tainted(&mut run, on), "game {on}");
    }
    noclip(&mut run, 1);
    assert!(!flying(&mut run, 1), "a second request lands them");
}

#[test]
fn noclip_is_refused_where_the_setting_forbids_and_ends_when_it_does() {
    for (dev, on) in [(DevMode::Off, 1), (DevMode::HostOnly, 1), (DevMode::Off, 0)] {
        let mut run = run(1, dev);
        noclip(&mut run, on);
        assert!(!flying(&mut run, on), "{dev:?} game {on}");
        assert!(!tainted(&mut run, on));
    }
    let mut run = run(1, DevMode::On);
    noclip(&mut run, 1);
    assert!(flying(&mut run, 1));
    run.world(0).write_message(SetDevMode(DevMode::HostOnly));
    run.step(20);
    assert!(!flying(&mut run, 1), "the Host took flight away");
}

#[test]
fn noclip_is_kept_by_seat_across_a_rejoin() {
    let mut run = run(2, DevMode::On);
    noclip(&mut run, 1);
    assert!(flying(&mut run, 1));
    // Their game is lost: the new one has neither flight nor toggles until the Host tells it.
    let player = run.player(1, 1);
    run.world(1).entity_mut(player).remove::<(Cheats, Noclip)>();
    run.cut(1);
    run.step(2);
    run.world(0).write_message(SetDevMode(DevMode::On));
    run.step(40);
    run.world(0).write_message(RoundStarted);
    run.step(30);
    // The Rejoiner's game is new: told they were flying, they fly again.
    assert!(flying(&mut run, 1), "the Rejoiner stopped flying");
}

#[test]
fn a_clients_round_jump_moves_every_games_round_and_taints() {
    let mut run = run(2, DevMode::On);
    ask_for(&mut run, 1, Cheat::RoundJump(RoundJump::Forward5));
    for on in 0..3 {
        assert_eq!(round_number(&mut run, on), 8, "game {on}");
        assert!(tainted(&mut run, on), "game {on}");
    }
    ask_for(&mut run, 2, Cheat::RoundJump(RoundJump::Back5));
    for on in 0..3 {
        assert_eq!(round_number(&mut run, on), 3, "back, game {on}");
    }
    ask_for(&mut run, 1, Cheat::RoundJump(RoundJump::NextTenth));
    assert_eq!(round_number(&mut run, 0), 10);
}

#[test]
fn a_jump_nowhere_is_no_cheat_and_taints_nothing() {
    let mut run = run(1, DevMode::On);
    run.world(0).resource_mut::<Round>().number = 1;
    run.step(5);
    ask_for(&mut run, 1, Cheat::RoundJump(RoundJump::Back));
    assert!(run.world(0).resource::<Asked>().rounds.is_empty());
    for on in 0..2 {
        assert!(!tainted(&mut run, on), "game {on}");
    }
}

#[test]
fn a_clients_weapon_grant_reaches_its_game_and_the_hosts_copy_alike() {
    let mut run = run(2, DevMode::On);
    ask_for(&mut run, 1, Cheat::Weapon(WeaponId::RayGun, true));
    let host_copy = run.player(0, 1);
    let own = run.player(1, 1);
    for (on, player) in [(0, host_copy), (1, own)] {
        let loadout = run.world(on).get::<Loadout>(player).unwrap();
        let copy = loadout.slot_of(WeaponId::RayGun).map(|slot| loadout.slot(slot).unwrap());
        assert!(copy.is_some_and(|copy| copy.upgraded), "game {on} lacks the Pack-a-Punched Weapon");
    }
    let held = |run: &mut Run, on, player| {
        let loadout = run.world(on).get::<Loadout>(player).unwrap();
        (loadout.held_slot(), loadout.weapons().map(|weapon| (weapon.id, weapon.upgraded)).collect::<Vec<_>>())
    };
    assert_eq!(held(&mut run, 0, host_copy), held(&mut run, 1, own), "the client's Weapons match the Host's");
    assert!(tainted(&mut run, 2), "and everyone hears the Run is Tainted");
}

#[test]
fn a_weapon_grant_to_a_full_hand_replaces_the_one_held_on_both_machines() {
    let mut run = run(1, DevMode::On);
    for id in [WeaponId::Ak74u, WeaponId::M14, WeaponId::Galil, WeaponId::Hk21] {
        ask_for(&mut run, 1, Cheat::Weapon(id, false));
    }
    let host_copy = run.player(0, 1);
    let own = run.player(1, 1);
    let ids = |run: &mut Run, on, player| {
        let loadout = run.world(on).get::<Loadout>(player).unwrap();
        (loadout.held().id, loadout.weapons().map(|weapon| weapon.id).collect::<Vec<_>>())
    };
    let (held, all) = ids(&mut run, 1, own);
    assert_eq!(held, WeaponId::Hk21, "the newest is in hand");
    assert_eq!((held, all.clone()), ids(&mut run, 0, host_copy));
    assert!(all.len() <= 3, "no more slots than a Player has: {all:?}");
}

#[test]
fn a_perk_grant_is_held_at_once_by_every_game_and_gives_its_entries() {
    let mut run = run(2, DevMode::On);
    ask_for(&mut run, 1, Cheat::Perk(Perk::Juggernog));
    ask_for(&mut run, 1, Cheat::Perk(Perk::Juggernog));
    for on in 0..3 {
        let player = run.player(on, 1);
        let perks = run.world(on).get::<Perks>(player).unwrap();
        assert_eq!(perks.0, [Perk::Juggernog], "game {on}");
    }
    let host_copy = run.player(0, 1);
    let entries = run
        .world(0)
        .get::<Modifiers>(host_copy)
        .unwrap()
        .0
        .iter()
        .filter(|entry| entry.source == Source("Juggernog"))
        .count();
    assert_eq!(entries, Perk::Juggernog.modifiers().len(), "held once, with its entries once");
}

#[test]
fn a_power_up_grant_goes_to_the_asker() {
    let mut run = run(1, DevMode::On);
    ask_for(&mut run, 1, Cheat::PowerUp(PowerUp::Nuke));
    let asker = run.player(0, 1);
    assert_eq!(run.world(0).resource::<PowerUpsGiven>().0, [(PowerUp::Nuke, Some(asker))]);
}

#[test]
fn boons_and_pacts_are_granted_in_the_gauntlet_and_reach_the_clients() {
    let mut run = run_in(2, DevMode::On, Mode::Gauntlet);
    let boon = 3;
    ask_for(&mut run, 1, Cheat::Boon(boon));
    ask_for(&mut run, 1, Cheat::Boon(boon));
    ask_for(&mut run, 2, Cheat::Pact(1));
    let id = boon_catalog().nth(boon.into()).unwrap().id;
    let own = run.player(1, 1);
    assert_eq!(run.world(1).get::<HeldBoons>(own).unwrap().stacks(id), 2, "a second grant is a second Stack");
    let host_copy = run.player(0, 1);
    assert_eq!(run.world(0).get::<HeldBoons>(host_copy).unwrap().stacks(id), 2);
    let pact = pact_catalog()[1].id();
    for on in 0..3 {
        let held = run.world(on).resource::<HeldPacts>();
        assert_eq!(held.holder(pact), Some(crate::players::player::Seat(2)), "game {on} sees who holds the Pact");
    }
    assert!(tainted(&mut run, 0));
}

#[test]
fn boons_and_pacts_are_refused_in_classic_without_a_taint() {
    let mut run = run_in(1, DevMode::On, Mode::Classic);
    ask_for(&mut run, 1, Cheat::Boon(0));
    ask_for(&mut run, 1, Cheat::Pact(0));
    let host_copy = run.player(0, 1);
    assert!(run.world(0).get::<HeldBoons>(host_copy).unwrap().0.is_empty());
    assert!(run.world(0).resource::<HeldPacts>().0.is_empty());
    for on in 0..2 {
        assert!(!tainted(&mut run, on), "game {on}");
    }
}

#[test]
fn a_clients_open_all_doors_and_power_on_reach_every_game() {
    let mut run = run(2, DevMode::On);
    for on in 0..3 {
        assert!(!run.world(on).resource::<Power>().is_on());
    }
    ask_for(&mut run, 1, Cheat::OpenAllDoors);
    ask_for(&mut run, 2, Cheat::PowerOn);
    run.step(60);
    for on in 0..3 {
        let world = run.world(on);
        let doors: Vec<bool> = world.query::<&Door>().iter(world).map(|door| door.open).collect();
        assert!(!doors.is_empty() && doors.iter().all(|&open| open), "game {on}: {doors:?}");
        assert!(world.resource::<Power>().is_on(), "game {on}");
        assert!(tainted(&mut run, on));
    }
}

#[test]
fn a_clients_spawn_is_asked_of_classic_ahead_of_them_outside_the_horde() {
    let mut run = run(1, DevMode::On);
    ask_for(&mut run, 1, Cheat::Spawn(Spawn { kind: Spawnable::Sprinter, elite: true }));
    let asked = &run.world(0).resource::<Asked>().spawns;
    assert_eq!(asked.len(), 1);
    let request = asked[0];
    assert_eq!(request.kind, RequestedKind::Sprinter);
    assert!(request.outside_cap && !request.holds_round);
    assert_eq!(request.tag, None, "the Elite tick is Gauntlet's: in Classic it does nothing");
    assert!(matches!(request.at, SpawnAt::Point(_)));
    assert!(tainted(&mut run, 0));
}

#[test]
fn a_boss_is_gauntlets_and_dropped_in_classic_but_summoned_in_gauntlet() {
    let mut run = run(1, DevMode::On);
    ask_for(&mut run, 1, Cheat::Spawn(Spawn { kind: Spawnable::Warden, elite: false }));
    assert!(run.world(0).resource::<Asked>().bosses.is_empty());
    assert!(!tainted(&mut run, 0), "a request Classic can't meet taints nothing");
    for on in 0..2 {
        run.world(on).insert_resource(Mode::Gauntlet);
    }
    ask_for(&mut run, 1, Cheat::Spawn(Spawn { kind: Spawnable::Warden, elite: true }));
    assert_eq!(run.world(0).resource::<Asked>().bosses.len(), 1);
    ask_for(&mut run, 1, Cheat::Spawn(Spawn { kind: Spawnable::Walker, elite: true }));
    assert_eq!(run.world(0).resource::<Asked>().spawns[0].tag, Some(crate::run::gauntlet::elite::ELITE_TAG));
    assert!(tainted(&mut run, 1));
}

#[test]
fn the_world_one_shots_obey_the_setting_too() {
    for cheat in [Cheat::KillAll, Cheat::OpenAllDoors, Cheat::PowerOn, Cheat::RoundJump(RoundJump::Forward)] {
        let mut run = run(1, DevMode::HostOnly);
        ask_for(&mut run, 1, cheat);
        assert!(!tainted(&mut run, 0), "{cheat:?}");
        assert_eq!(round_number(&mut run, 0), 3);
        assert!(!run.world(0).resource::<Power>().is_on());
    }
}

#[test]
fn a_grant_the_setting_forbids_is_dropped() {
    let mut run = run(1, DevMode::HostOnly);
    ask_for(&mut run, 1, Cheat::Weapon(WeaponId::RayGun, false));
    ask_for(&mut run, 1, Cheat::Perk(Perk::Juggernog));
    let own = run.player(1, 1);
    assert!(run.world(1).get::<Loadout>(own).unwrap().slot_of(WeaponId::RayGun).is_none());
    assert!(run.world(1).get::<Perks>(own).unwrap().0.is_empty());
    assert!(!tainted(&mut run, 0));
}

/// A Run of the Host and a client on `map`, Developer mode On.
fn run_on(map: MapId, mode: Mode) -> Run {
    let mut run = run_in(1, DevMode::On, mode);
    for on in 0..2 {
        run.world(on).insert_resource(map);
    }
    run.world(0).insert_resource(Generators::for_map(map));
    run
}

#[test]
fn a_clients_monkey_bombs_land_on_the_hosts_copy_and_taint() {
    let mut run = run(1, DevMode::On);
    ask_for(&mut run, 1, Cheat::MonkeyBombs);
    assert_eq!(run.world(0).resource::<Asked>().monkey_bombs, 1);
    let host_copy = run.player(0, 1);
    assert!(run.world(0).get::<Equipment>(host_copy).is_some());
    assert!(tainted(&mut run, 0));
}

#[test]
fn a_clients_swing_tide_and_generator_asks_come_from_the_player_and_taint() {
    let tide_map = MapId::ALL.into_iter().find(|map| !map.layout().canals.is_empty()).expect("a Map with a Tide");
    let mut run = run_on(tide_map, Mode::Classic);
    ask_for(&mut run, 1, Cheat::TurnTide);
    assert_eq!(run.world(0).resource::<Asked>().tides, 1);
    assert!(tainted(&mut run, 0));

    let swing_map = MapId::ALL.into_iter().find(|map| !map.layout().shifting.is_empty()).expect("a Map with a bridge");
    let mut run = run_on(swing_map, Mode::Classic);
    ask_for(&mut run, 1, Cheat::SwingBridge);
    let body = run.player(0, 1);
    let at = run.world(0).get::<Transform>(body).unwrap().translation;
    assert_eq!(run.world(0).resource::<Asked>().swings, [at]);
    assert!(tainted(&mut run, 0));

    let mut run = run_on(MapId::Dam, Mode::Classic);
    ask_for(&mut run, 1, Cheat::LoseGenerator);
    assert_eq!(run.world(0).resource::<Asked>().generators.len(), 1);
    assert!(tainted(&mut run, 0));
}

#[test]
fn a_map_without_the_thing_drops_the_ask_without_a_taint() {
    let bare = MapId::ALL.into_iter().find(|map| map.layout().canals.is_empty() && map.layout().shifting.is_empty()).unwrap_or(MapId::Greybox);
    let mut run = run_on(bare, Mode::Classic);
    run.world(0).insert_resource(Generators::for_map(bare));
    if bare.layout().canals.is_empty() {
        ask_for(&mut run, 1, Cheat::TurnTide);
        assert_eq!(run.world(0).resource::<Asked>().tides, 0);
    }
    if !Generators::for_map(bare).on_map() {
        ask_for(&mut run, 1, Cheat::LoseGenerator);
        assert!(run.world(0).resource::<Asked>().generators.is_empty());
    }
    if bare.layout().shifting.is_empty() {
        ask_for(&mut run, 1, Cheat::SwingBridge);
        assert!(run.world(0).resource::<Asked>().swings.is_empty());
    }
    assert!(!tainted(&mut run, 0));
}

#[test]
fn unlock_all_is_gauntlets_and_the_setting_applies_to_the_debug_ones_too() {
    let mut run = run(1, DevMode::On);
    ask_for(&mut run, 1, Cheat::UnlockAll);
    assert_eq!(run.world(0).resource::<Asked>().unlocks, 0, "Classic has no Boons to unlock");
    assert!(!tainted(&mut run, 0));
    let mut run = run_in(1, DevMode::On, Mode::Gauntlet);
    ask_for(&mut run, 1, Cheat::UnlockAll);
    assert_eq!(run.world(0).resource::<Asked>().unlocks, 1);
    assert!(tainted(&mut run, 0));
    for cheat in Cheat::DEBUG {
        let mut run = run_in(1, DevMode::HostOnly, Mode::Gauntlet);
        ask_for(&mut run, 1, cheat);
        assert!(!tainted(&mut run, 0), "{cheat:?}");
    }
}
