//! Downs, Revives, Bleed Outs, Respawns, Pause flags, Offers and Leaving over the reliable stream:
//! the Host judges, and every game shows the result.

use avian3d::prelude::Position;

use super::tests::Run;
use super::*;
use crate::run::gauntlet::BanishCard;
use crate::run::gauntlet::{HeldBoons, HeldPacts, Offer, OfferWire, PickOffer, apply_pick, sample_boon_ids};
use crate::players::last_stand::{Down as WentDown, LastStandPlugin, go_down};
use crate::run::leave_run::LeaveRunPlugin;
use crate::net::protocol::{ToHost, encode};
use crate::players::player::{Locomotion, MoveInput, Stance, StanceKeys, take_hits};
use crate::players::respawn::RespawnPlugin;
use crate::players::revive::revive;
use crate::state::RunType;
use crate::run::stats::{RunStats, StatsPlugin};

/// What a game has been told of Downs.
#[derive(Resource, Default)]
struct Downs(u32);

fn count_downs(mut downs: ResMut<Downs>, mut told: MessageReader<WentDown>) {
    downs.0 += told.read().count() as u32;
}

/// Every game of a Co-op Run in Round 3 with the rules of Downs, Revives and Offers on the Host.
pub(super) fn coop(app: &mut App) {
    app.insert_resource(RunType::Coop)
        .insert_resource(Round::in_progress(3))
        .init_resource::<HeldPacts>()
        .init_resource::<Downs>()
        .add_plugins((LastStandPlugin, RespawnPlugin, StatsPlugin, LeaveRunPlugin))
        .add_systems(Update, (take_hits, revive.after(go_down), apply_pick).in_set(HostSimulation))
        .add_systems(Update, (count_downs, give_boons));
}

/// Every Player holds Boons, as Gauntlet gives them.
fn give_boons(mut commands: Commands, players: Query<Entity, (With<Player>, Without<HeldBoons>)>) {
    for player in &players {
        commands.entity(player).insert(HeldBoons::default());
    }
}

fn boon_wire(cards: Vec<String>) -> OfferWire {
    OfferWire { round: 3, pacts: false, cards, rerolls: 0, charges: 1, picks: 1, picked: 0 }
}

fn run(clients: u8) -> Run {
    Run::with(clients, &coop)
}

/// Drops the Player in `seat` to nothing, as the Host has them.
pub(super) fn knock_down(run: &mut Run, seat: u8) {
    let player = run.player(0, seat);
    run.world(0).get_mut::<Health>(player).unwrap().current = 0.0;
}

/// Whether game `on` has the Player in `seat` in Last Stand.
fn down(run: &mut Run, on: usize, seat: u8) -> bool {
    let player = run.player(on, seat);
    run.world(on).get::<LastStand>(player).is_some()
}

fn waiting(run: &mut Run, on: usize, seat: u8) -> bool {
    let player = run.player(on, seat);
    run.world(on).get::<AwaitingRespawn>(player).is_some()
}

#[test]
fn a_respawned_player_comes_back_with_their_own_kits_loadout() {
    use crate::run::gauntlet::kits::Kit;
    let mut run = run(2);
    let lead = run.player(0, 1);
    run.world(0).entity_mut(lead).insert(Kit::Lead.starting_loadout());
    run.give_points(1, 100);
    run.step(5);
    knock_down(&mut run, 1);
    run.step((LAST_STAND_COOP_SECS * FIXED_HZ as f32) as u32 + 35);
    assert!(waiting(&mut run, 0, 1));
    run.world(0).write_message(RoundStarted);
    run.step(10);
    assert!(!waiting(&mut run, 0, 1));
    assert_eq!(run.world(0).get::<Loadout>(lead).unwrap().held().id, Kit::Lead.starting_loadout().weapon);
    // A Player with no Kit's start comes back with Classic's.
    let usual = run.player(0, 2);
    assert_eq!(run.world(0).get::<Loadout>(usual).unwrap().held().id, crate::tuning::STARTING_WEAPON);
}

#[test]
fn a_down_on_the_host_is_a_last_stand_on_every_game() {
    let mut run = run(2);
    run.give_points(1, 1_000);
    run.step(5);
    knock_down(&mut run, 1);
    run.step(10);
    for on in 0..3 {
        assert!(down(&mut run, on, 1), "game {on} doesn't see the Down");
        let player = run.player(on, 1);
        assert!(run.world(on).get::<LastStand>(player).unwrap().is_coop());
    }
    // The client goes down with the Weapons the Host gave it: Mustang & Sally, Upgraded.
    let own = run.player(1, 1);
    let loadout = run.world(1).get::<Loadout>(own).unwrap();
    assert!(loadout.held().upgraded, "the Last Stand pistol");
    assert_eq!(run.world(1).get::<Health>(own).unwrap().current, 0.0);
    assert_eq!(run.world(1).resource::<Downs>().0, 1, "the Down is heard");
    // And it cost the 5% the Host took.
    assert_eq!(run.balance(1), 950);
    assert_eq!(run.world(2).resource::<Downs>().0, 1, "so is it in the third game");
    let on_host = run.player(0, 1);
    assert_eq!(run.world(0).get::<RunStats>(on_host).unwrap().downs, 1);
}

/// A client's Player who gets Out of Bounds is Downed by the Host, which judges it from where it
/// has them, and every game sees the Last Stand.
#[test]
fn a_client_out_of_bounds_is_downed_by_the_host() {
    use crate::players::out_of_bounds::OutOfBoundsPlugin;
    let mut run = Run::with(2, &|app| {
        coop(app);
        app.add_message::<crate::players::player::Teleport>().add_plugins(OutOfBoundsPlugin);
    });
    run.step(5);
    let (min, _) = crate::maps::map::MapId::Greybox.layout().plan;
    let out = Vec3::new(min.x - 5.0, 1.0, 20.0);
    let on_host = run.player(0, 1);
    run.world(0).get_mut::<Transform>(on_host).unwrap().translation = out;
    run.world(0).entity_mut(on_host).insert(Position(out));
    run.step(10);
    for on in 0..3 {
        assert!(down(&mut run, on, 1), "game {on} doesn't see the Down");
    }
    assert!(!down(&mut run, 0, 0), "the Host's own Player went Down too");
    assert_eq!(run.world(1).resource::<Downs>().0, 1, "the Down is heard");
}

#[test]
fn a_client_is_hurt_by_the_hosts_word() {
    let mut run = run(1);
    run.step(5);
    let on_host = run.player(0, 1);
    run.world(0).get_mut::<Health>(on_host).unwrap().current = 40.0;
    run.step(10);
    let own = run.player(1, 1);
    assert_eq!(run.world(1).get::<Health>(own).unwrap().current, 40.0);
}

#[test]
fn a_client_revives_a_downed_host_player_and_both_get_up_in_every_game() {
    let mut run = run(2);
    run.give_points(0, 2_000);
    run.give_points(1, 100);
    run.step(5);
    // The reviver stands beside them, as the Host has it.
    let host_at = {
        let player = run.player(0, 0);
        run.world(0).get::<Transform>(player).unwrap().translation
    };
    let reviver = run.player(0, 1);
    run.world(0).get_mut::<Transform>(reviver).unwrap().translation = host_at + Vec3::X;
    run.world(0).entity_mut(reviver).insert(Position(host_at + Vec3::X));
    knock_down(&mut run, 0);
    run.step(10);
    assert!(down(&mut run, 1, 0), "the client sees its teammate Down");
    let held_loadout = {
        let player = run.player(0, 0);
        run.world(0).get::<Loadout>(player).unwrap().held().upgraded
    };
    assert!(held_loadout);
    // The client holds Use over the downed puppet, as its own game asks.
    let local = run.player(1, 1);
    let puppet = run.player(1, 0);
    for _ in 0..(REVIVE_SECS * FIXED_HZ as f32) as u32 / 2 {
        run.world(1).write_message(UseRequest { player: local, buyable: puppet, hold: true });
        run.step(1);
    }
    // Half way: still Down, and the reviver's own game shows the bar filling.
    assert!(down(&mut run, 0, 0));
    assert!(run.world(1).get::<Reviving>(local).is_some(), "the reviver sees the Revive under way");
    let seen_by_other = run.player(2, 1);
    assert!(run.world(2).get::<Reviving>(seen_by_other).is_some(), "so does the third game");
    for _ in 0..(REVIVE_SECS * FIXED_HZ as f32) as u32 {
        run.world(1).write_message(UseRequest { player: local, buyable: puppet, hold: true });
        run.step(1);
    }
    run.step(10);
    for on in 0..3 {
        assert!(!down(&mut run, on, 0), "game {on} still has them Down");
    }
    let revived = run.player(0, 0);
    let health = run.world(0).get::<Health>(revived).unwrap();
    assert_eq!(health.current, health.max);
    assert_eq!(run.world(0).get::<RunStats>(reviver).unwrap().revives, 1);
    // The Down's cost (5% of 2000) went to the reviver.
    assert_eq!(run.balance(1), 100 + 100);
    // The Host's player is on their feet on their own game with their Health and Weapons back.
    let own = run.player(0, 0);
    assert!(!run.world(0).get::<Loadout>(own).unwrap().held().upgraded, "their own Weapons are back");
    // And the reviver's game counted its Revive.
    assert_eq!(run.world(1).get::<RunStats>(local).unwrap().revives, 1);
}

#[test]
fn a_bleed_out_takes_a_client_out_of_every_game_until_the_next_round() {
    let mut run = run(2);
    run.give_points(0, 1_000);
    run.give_points(1, 100);
    run.step(5);
    knock_down(&mut run, 1);
    run.step(5);
    run.step((LAST_STAND_COOP_SECS * FIXED_HZ as f32) as u32 + 30);
    for on in 0..3 {
        assert!(waiting(&mut run, on, 1), "game {on} doesn't have them waiting to Respawn");
        assert!(!down(&mut run, on, 1), "game {on} still has them Down");
    }
    // Everyone else paid a tenth of what they had.
    assert_eq!(run.balance(0), 900);
    // While they wait, their own game has them wanting Prone (a Toggle press, say).
    let own = run.player(1, 1);
    run.world(1).get_mut::<StanceKeys>(own).unwrap().target = Stance::Prone;
    // The next Round brings them back beside a teammate who is up.
    run.world(0).write_message(RoundStarted);
    run.step(10);
    for on in 0..3 {
        assert!(!waiting(&mut run, on, 1), "game {on} still has them gone");
    }
    // The Respawn put them Standing on their own game, the Stance they want included, so their
    // game doesn't ask the Host to lay them down again.
    assert_eq!(run.world(1).get::<StanceKeys>(own).unwrap().target, Stance::Standing);
    assert_eq!(run.world(1).get::<Locomotion>(own).unwrap().stance, Stance::Standing);
    let loadout = run.world(1).get::<Loadout>(own).unwrap();
    assert!(!loadout.held().upgraded, "the starting Weapon, not the Last Stand pistol");
    assert_eq!(loadout.weapons().count(), 1);
    let health = run.world(1).get::<Health>(own).unwrap();
    assert_eq!(health.current, health.max);
    let returned = run.player(0, 1);
    let them = run.world(0).get::<Transform>(returned).unwrap().translation;
    let nearest = [0, 2]
        .into_iter()
        .map(|seat| {
            let mate = run.player(0, seat);
            run.world(0).get::<Transform>(mate).unwrap().translation.distance(them)
        })
        .fold(f32::MAX, f32::min);
    assert!(nearest < 2.0, "beside a teammate: {nearest}");
}

#[test]
fn a_paused_flag_reaches_every_game() {
    let mut run = run(2);
    run.step(5);
    let own = run.player(1, 1);
    run.world(1).entity_mut(own).insert(Paused);
    run.step(10);
    for on in [0, 2] {
        let player = run.player(on, 1);
        assert!(run.world(on).get::<Paused>(player).is_some(), "game {on} doesn't see them Paused");
    }
    run.world(1).entity_mut(own).remove::<Paused>();
    run.step(10);
    for on in [0, 2] {
        let player = run.player(on, 1);
        assert!(run.world(on).get::<Paused>(player).is_none(), "game {on} still sees them Paused");
    }
    // The Host's own Pause goes to the clients too.
    let host = run.player(0, 0);
    run.world(0).entity_mut(host).insert(Paused);
    run.step(10);
    for on in [1, 2] {
        let player = run.player(on, 0);
        assert!(run.world(on).get::<Paused>(player).is_some(), "game {on} doesn't see the Host Paused");
    }
}

#[test]
fn an_offer_goes_to_its_player_alone_and_their_pick_applies_on_the_host() {
    let mut run = run(2);
    run.step(5);
    let on_host = run.player(0, 1);
    let ids = sample_boon_ids(3);
    let offer = Offer::from_wire(&boon_wire(ids.clone())).unwrap();
    run.world(0).entity_mut(on_host).insert((offer, Choosing));
    run.step(10);
    let own = run.player(1, 1);
    assert!(run.world(1).get::<Offer>(own).is_some(), "the Player has their Offer");
    assert!(run.world(1).get::<Choosing>(own).is_some());
    // The other client sees that they are choosing, and none of the cards.
    let seen = run.player(2, 1);
    assert!(run.world(2).get::<Choosing>(seen).is_some());
    assert!(run.world(2).get::<Offer>(seen).is_none());
    let mine = run.player(2, 2);
    assert!(run.world(2).get::<Offer>(mine).is_none());
    // They pick the second card.
    run.world(1).write_message(PickOffer { player: own, card: Some(1) });
    run.step(10);
    let held = run.world(0).get::<HeldBoons>(on_host).unwrap();
    assert_eq!(held.stacks(&ids[1]), 1, "the pick applied on the Host");
    assert_eq!(held.0.len(), 1);
    assert!(run.world(0).get::<Offer>(on_host).is_none());
    assert!(run.world(0).get::<Choosing>(on_host).is_none());
    assert!(run.world(1).get::<Offer>(own).is_none(), "the Offer closed on their game");
    assert!(run.world(1).get::<Choosing>(own).is_none());
    assert_eq!(run.world(1).get::<HeldBoons>(own).unwrap().stacks(&ids[1]), 1, "and they hold the Boon");
    assert!(run.world(2).get::<Choosing>(seen).is_none());
}

#[test]
fn a_remote_player_with_an_offer_open_is_not_stepped() {
    let mut run = run(1);
    run.step(5);
    let on_host = run.player(0, 1);
    let offer = Offer::from_wire(&boon_wire(sample_boon_ids(3))).unwrap();
    run.world(0).entity_mut(on_host).insert((offer, Choosing));
    let before = run.world(0).get::<Transform>(on_host).unwrap().translation;
    // The client keeps walking on its own game.
    let own = run.player(1, 1);
    for _ in 0..30 {
        run.world(1).get_mut::<MoveInput>(own).unwrap().axes = Vec2::Y;
        run.step(1);
    }
    let after = run.world(0).get::<Transform>(on_host).unwrap().translation;
    assert!(before.distance(after) < 0.1, "moved {} on the Host", before.distance(after));
}

#[test]
fn a_joiner_who_leaves_is_gone_from_the_host_and_every_other_game() {
    let mut run = run(2);
    run.step(5);
    let wire = &mut run.world(1).resource_mut::<crate::net::play::Wire>().0;
    wire.send(crate::net::protocol::HOST, encode(&ToHost::LeftRun));
    run.step(10);
    for on in [0, 2] {
        let world = run.world(on);
        let mut query = world.query_filtered::<&PlayerSeat, With<Player>>();
        let mut seats: Vec<u8> = query.iter(world).map(|seat| seat.0).collect();
        seats.sort_unstable();
        assert_eq!(seats, [0, 2], "game {on}");
    }
    assert_eq!(run.world(0).resource::<Roster>().0.len(), 1);
}

#[test]
fn a_clients_reroll_and_banish_apply_on_the_host_and_come_back_in_the_offer() {
    use crate::run::gauntlet::{Banishes, RerollOffer};
    let mut run = Run::with(1, &|app| {
        coop(app);
        crate::run::gauntlet::host_offer_tools(app);
    });
    run.step(5);
    let on_host = run.player(0, 1);
    let own = run.player(1, 1);
    run.give_points(1, 5000);
    let ids = sample_boon_ids(3);
    let offer = Offer::from_wire(&boon_wire(ids.clone())).unwrap();
    run.world(0).entity_mut(on_host).insert((offer, Choosing));
    let charges = run.world(0).get::<Banishes>(on_host).unwrap().charges;
    run.step(10);
    assert_eq!(run.world(1).get::<Offer>(own).unwrap().rerolls(), 0);
    assert_eq!(run.world(1).get::<Banishes>(own).unwrap().charges, charges, "the Host's charges");

    // A Reroll on the client: the Host charges 1,000 and the Offer comes back with one made.
    run.world(1).write_message(RerollOffer { player: own });
    run.step(10);
    assert_eq!(run.world(0).get::<Offer>(on_host).unwrap().rerolls(), 1);
    assert_eq!(run.world(0).get::<crate::economy::points::Points>(on_host).unwrap().balance, 4000);
    assert_eq!(run.world(1).get::<Offer>(own).unwrap().rerolls(), 1, "the client's price follows");

    // A Banish spends one of the Host's charges and shows on the client.
    run.world(1).write_message(BanishCard { player: own, card: 0 });
    run.step(10);
    assert_eq!(run.world(0).get::<Banishes>(on_host).unwrap().charges, charges - 1);
    assert_eq!(run.world(1).get::<Banishes>(own).unwrap().charges, charges - 1);
    assert_eq!(run.world(1).get::<Offer>(own).unwrap().card_ids(), run.world(0).get::<Offer>(on_host).unwrap().card_ids());
}
