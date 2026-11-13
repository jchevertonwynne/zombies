//! What the Host says of the rest of the shared world reaches every game: Planks, the Power, Rounds,
//! Pack-a-Punch, a teammate's switches, Power-ups, Perks and Traps.

use super::tests::{Run, give_weapon};
use super::*;
use crate::economy::power_up::Dropped;

#[test]
fn planks_torn_on_the_host_are_torn_everywhere() {
    let mut run = Run::new(2);
    run.world(0).resource_mut::<Barriers>().tear(BarrierId(3));
    run.step(10);
    for on in 1..3 {
        assert_eq!(run.world(on).resource::<Barriers>().planks(BarrierId(3)), BARRIER_PLANKS - 1);
        assert_eq!(run.seen(on).torn, 1, "game {on} hears the Plank go");
    }
    run.world(0).resource_mut::<Barriers>().restore_all();
    run.step(10);
    assert_eq!(run.world(2).resource::<Barriers>().planks(BarrierId(3)), BARRIER_PLANKS);
}

#[test]
fn the_power_comes_on_everywhere_when_the_host_throws_the_switch() {
    let mut run = Run::new(1);
    let switch = run.find::<PowerSwitch>(0, |_| true);
    run.world(0).get_mut::<PowerSwitch>(switch).unwrap().start();
    run.step(5);
    let shown = run.find::<PowerSwitch>(1, |switch| switch.started());
    assert!(run.world(1).get::<Buyable>(shown).is_none(), "the thrown switch offers no Prompt");
    assert!(!run.world(1).resource::<Power>().is_on());
    run.step((POWER_SWITCH_FLIP_SECS * FIXED_HZ as f32) as u32 + 20);
    for on in 0..2 {
        assert!(run.world(on).resource::<Power>().is_on(), "game {on} has no Power");
    }
}

/// Every game on the Dam's Generators, all Unclaimed.
fn on_the_dams_generators(run: &mut Run, games: usize) {
    for on in 0..games {
        run.world(on).insert_resource(Generators::for_map(crate::maps::map::MapId::Dam));
    }
}

#[test]
fn every_game_sees_a_generator_charge_and_then_be_held() {
    let mut run = Run::new(2);
    on_the_dams_generators(&mut run, 3);
    let mut host = run.world(0).resource_mut::<Generators>();
    host.claim(1);
    host.charge(1, 5.0, true);
    run.step(10);
    for on in 1..3 {
        let shown = run.world(on).resource::<Generators>().states()[1];
        assert!(shown.live(), "game {on} doesn't see the Charge running: {shown:?}");
        assert!((shown.fraction() - 0.25).abs() < 0.05, "game {on} shows {shown:?}");
    }
    // Stepping off pauses it everywhere.
    run.world(0).resource_mut::<Generators>().charge(1, 0.1, false);
    run.step(10);
    assert!(!run.world(1).resource::<Generators>().states()[1].live());
    run.world(0).resource_mut::<Generators>().hold(1);
    run.step(10);
    for on in 1..3 {
        let generators = run.world(on).resource::<Generators>();
        assert_eq!(generators.states()[1], GeneratorState::Held, "game {on}");
        assert!(generators.ever_held());
    }
}

#[test]
fn every_game_sees_a_generator_lost_and_reclaimed() {
    let mut run = Run::new(2);
    on_the_dams_generators(&mut run, 3);
    run.world(0).resource_mut::<Generators>().hold(2);
    run.step(10);
    run.world(0).resource_mut::<Generators>().lose(2);
    run.step(10);
    for on in 1..3 {
        let generators = run.world(on).resource::<Generators>();
        assert_eq!(generators.states()[2], GeneratorState::Lost, "game {on}");
        assert!(generators.ever_held());
    }
    run.world(0).resource_mut::<Generators>().reclaim(2);
    run.world(0).resource_mut::<Generators>().charge(2, 2.0, true);
    run.step(10);
    for on in 1..3 {
        let shown = run.world(on).resource::<Generators>().states()[2];
        assert!(shown.live(), "game {on} doesn't see the Reclaim: {shown:?}");
    }
}

/// The Host doesn't say how far a Charge has come every frame, only every so often.
#[test]
fn the_host_tells_a_charge_at_a_low_rate() {
    let mut run = Run::new(1);
    on_the_dams_generators(&mut run, 2);
    run.world(0).resource_mut::<Generators>().claim(0);
    run.step(5);
    let mut told = Vec::new();
    for _ in 0..FIXED_HZ {
        run.world(0).resource_mut::<Generators>().charge(0, 1.0 / FIXED_HZ as f32, true);
        run.step(1);
        told.push(run.world(1).resource::<Generators>().states()[0].fraction());
    }
    told.dedup();
    let most = (1.0 / GENERATOR_CHARGE_TELL_SECS).ceil() as usize + 2;
    assert!(told.len() <= most, "told {} times in a second: {told:?}", told.len());
}

#[test]
fn the_host_tells_every_game_when_a_round_begins_and_ends() {
    let mut run = Run::new(2);
    run.world(0).insert_resource(Round::in_progress(7));
    run.step(10);
    for on in 1..3 {
        assert_eq!(run.world(on).resource::<Round>().number, 7, "game {on} has no Round");
        assert_eq!(run.seen(on).rounds_started, 1);
    }
    run.world(0).resource_mut::<Round>().phase = RoundPhase::Intermission(INTERMISSION_SECS);
    run.step(10);
    for on in 1..3 {
        assert!(matches!(run.world(on).resource::<Round>().phase, RoundPhase::Intermission(_)));
        assert_eq!(run.seen(on).round_ends, 1);
    }
}

#[test]
fn a_teammate_can_pack_a_punch_when_the_hosts_own_player_has_nothing_to_offer() {
    let mut run = Run::new(2);
    run.world(0).resource_mut::<Power>().switch_on();
    run.give_points(1, 20_000);
    give_weapon(&mut run, 1, WeaponId::M14);
    run.step(10);
    // The Host's own Player has only the one Weapon, so the machine refuses them.
    let host_player = run.player(0, 0);
    assert_eq!(run.world(0).get::<Loadout>(host_player).unwrap().weapons().count(), 1);
    let machine = run.find::<PackAPunch>(1, |_| true);
    run.use_buyable(1, machine, false);
    run.step(20);
    let id = *run.world(1).get::<NetId>(machine).unwrap();
    for on in 0..3 {
        let entity = run.world(on).resource::<NetIds>().entity(id.0).unwrap();
        assert_eq!(
            run.world(on).get::<PackAPunch>(entity).unwrap().inside(),
            Some(WeaponId::M14),
            "game {on} doesn't show the Weapon inside"
        );
    }
    assert!(run.balance(1) < 20_000);
    // The client lost the Weapon from its hands, by the Host's word.
    let client = run.player(1, 1);
    assert!(run.world(1).get::<Loadout>(client).unwrap().slot_of(WeaponId::M14).is_none());
    // And takes it back, upgraded, when it is ready.
    run.step((PAP_PROCESS_SECS * FIXED_HZ as f32) as u32 + 60);
    run.use_buyable(1, machine, false);
    run.step(20);
    let loadout = run.world(1).get::<Loadout>(client).unwrap();
    let slot = loadout.slot_of(WeaponId::M14).expect("the Upgraded Weapon came back");
    assert!(loadout.slot(slot).unwrap().upgraded);
}

#[test]
fn a_clients_own_switches_are_reported_and_shown_on_the_other_clients() {
    let mut run = Run::new(2);
    give_weapon(&mut run, 1, WeaponId::M14);
    run.step(10);
    let client = run.player(1, 1);
    assert!(run.world(1).get::<Loadout>(client).unwrap().slot_of(WeaponId::M14).is_some());
    let puppet = run.player(2, 1);
    let shown = |run: &mut Run| run.world(2).get::<HeldWeapon>(puppet).unwrap().id;
    run.step(10);
    assert_eq!(shown(&mut run), WeaponId::M14, "the buy was relayed");
    // They switch back to the starting Weapon.
    run.world(1).get_mut::<Loadout>(client).unwrap().set_held(0);
    run.step(10);
    assert_eq!(shown(&mut run), STARTING_WEAPON);
    let on_host = run.player(0, 1);
    assert_eq!(run.world(0).get::<Loadout>(on_host).unwrap().held_slot(), 0);
}

#[test]
fn power_ups_on_the_ground_show_on_every_client_until_they_are_taken() {
    let mut run = Run::new(2);
    let at = Vec3::new(40.0, 1.0, 40.0);
    let drop = run.world(0).spawn(Dropped::new(PowerUp::Carpenter, at)).id();
    run.step(10);
    for on in 1..3 {
        let world = run.world(on);
        let mut query = world.query::<&Dropped>();
        assert_eq!(query.iter(world).count(), 1, "game {on} has no Power-up on the ground");
    }
    run.world(0).entity_mut(drop).despawn();
    run.step(10);
    for on in 1..3 {
        let world = run.world(on);
        let mut query = world.query::<&Dropped>();
        assert_eq!(query.iter(world).count(), 0, "game {on} still has it");
    }
}

#[test]
fn a_perk_bought_by_a_client_is_drunk_and_held_on_its_game() {
    let mut run = Run::new(2);
    let on_host = run.player(0, 1);
    run.world(0).entity_mut(on_host).insert(Drinking::new(Perk::Juggernog));
    run.step(10);
    let client = run.player(1, 1);
    assert!(run.world(1).get::<Drinking>(client).is_some());
    run.world(0).entity_mut(on_host).remove::<Drinking>();
    run.world(0).get_mut::<Perks>(on_host).unwrap().0.push(Perk::Juggernog);
    run.step(10);
    assert!(run.world(1).get::<Drinking>(client).is_none());
    assert!(run.world(1).get::<Perks>(client).unwrap().has(Perk::Juggernog));
    let bystander = run.player(2, 1);
    assert!(run.world(2).get::<Perks>(bystander).unwrap().has(Perk::Juggernog));
}

#[test]
fn a_trap_the_host_runs_runs_on_every_client() {
    let mut run = Run::new(1);
    let trap = run.find::<Trap>(0, |_| true);
    run.world(0).get_mut::<Trap>(trap).unwrap().show(TrapPhase::Running(0.0));
    run.step(10);
    let id = *run.world(0).get::<NetId>(trap).unwrap();
    let shown = run.world(1).resource::<NetIds>().entity(id.0).unwrap();
    assert!(run.world(1).get::<Trap>(shown).unwrap().running());
}

#[test]
fn a_link_the_host_starts_is_under_way_on_every_client() {
    use crate::maps::teleporter::{PadId, Teleporter};
    let mut run = Run::new(1);
    run.world(0).resource_mut::<Teleporter>().activate(PadId::Generator);
    run.step(10);
    assert_eq!(run.world(1).resource::<Teleporter>().linking(), Some(PadId::Generator));
    run.world(0).resource_mut::<Teleporter>().link();
    run.step(10);
    assert!(run.world(1).resource::<Teleporter>().linked(PadId::Generator));
    assert_eq!(run.world(1).resource::<Teleporter>().linking(), None);
}

#[test]
fn events_and_requests_survive_the_wire() {
    use crate::net::protocol::{decode, encode};
    let copy = CopyWire::of(&WeaponCopy::new(WeaponId::M14).upgrade());
    let hands = ToPlayer::Event(WorldEvent::Hands {
        ops: vec![HandOp::Put { slot: 1, copy: copy.clone() }, HandOp::MaxAmmo, HandOp::Refill(0)],
        held: 1,
        gear: None,
    });
    assert_eq!(decode::<ToPlayer>(&encode(&hands)).unwrap(), hands);
    let boxed = ToPlayer::Event(WorldEvent::Box {
        id: 3,
        location: 1,
        phase: BoxPhase::Offering { spinner: 2, prize: Prize::Weapon(WeaponId::Ak74u), upgraded: true },
    });
    assert_eq!(decode::<ToPlayer>(&encode(&boxed)).unwrap(), boxed);
    let request = ToHost::Request(Request::Use { buyable: 9, hold: true, refused: false });
    assert_eq!(decode::<ToHost>(&encode(&request)).unwrap(), request);
    // A copy built from the wire is the copy that went on it.
    assert_eq!(CopyWire::of(&copy.build()), copy);
}
