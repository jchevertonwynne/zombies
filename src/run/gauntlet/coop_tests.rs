//! Gauntlet between two Players in one Co-op world, with no netcode: each Player's own Offers,
//! the Intermission's wait for them, and Boons and Pacts that touch only their holders.

use super::boon::{BOONS, BoonDef, RunFacts};
use super::pact::{GOLD_RUSH, OVERKILL, SCORCHED_EARTH, UNDYING};
use super::*;
use crate::players::controls::{Action, Controls};
use crate::players::last_stand::LastStandPlugin;
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::players::player::Health;
use crate::economy::perk::Perks;
use crate::players::player::two_players::{balance, two_players_with};
use crate::economy::points::{BonusPoints, Points};
use crate::economy::power_up::{ActivePowerUps, Dropped};
use crate::players::respawn::{AwaitingRespawn, Disconnected, RespawnPlugin};
use crate::players::revive::Revived;
use crate::run::round::RoundStarted;
use crate::storage::settings::Settings;
use crate::state::input_runs;
use crate::run::stats::StatsPlugin;
use crate::enemies::zombie::headless::Sim;
use crate::enemies::zombie::{Attack, Pay};
use bevy::ecs::system::RunSystemOnce;

/// Two Players in a Co-op Gauntlet Run in Round 3, with the Offer, Boon and Pact systems running.
fn gauntlet_players() -> (Sim, Entity, Entity) {
    gauntlet_players_with(|_| {})
}

/// `gauntlet_players`, with `extra` set up on the App as well.
pub(super) fn gauntlet_players_with(extra: impl FnOnce(&mut App)) -> (Sim, Entity, Entity) {
    let (mut sim, local, teammate) = two_players_with(|app| {
        app.insert_resource(RunType::Coop)
            .insert_resource(Round::in_progress(3))
            .init_resource::<HeldPacts>()
            .init_resource::<Gifts>()
            .init_resource::<OfferWait>()
            .init_resource::<super::RunArc>()
            .init_resource::<RoundVariety>()
            .init_resource::<super::heat::RunHeat>()
            .init_resource::<UnlockAll>()
            .init_resource::<IntermissionHold>()
            .init_resource::<Freeze>()
            .init_resource::<Settings>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_plugins((LastStandPlugin, StatsPlugin, RespawnPlugin))
            .add_message::<RoundEnded>()
            .add_message::<Revived>()
            .add_message::<RoundStarted>()
            .add_message::<PickOffer>()
            .add_message::<RerollOffer>()
            .add_message::<BanishCard>()
            .add_message::<tools::ToolUsed>()
            .add_message::<crate::economy::points::SpendPoints>()
            .add_message::<PlayCue>()
            .add_message::<TeddyBear>()
            .add_message::<PowerUpTaken>()
            .add_message::<GivePowerUp>()
            .add_message::<DropPowerUp>()
            .add_message::<KillDamage>()
            .add_message::<ZombieKilled>()
            .add_message::<Explosion>()
            .add_observer(give_gauntlet_state)
            .add_systems(
                Update,
                (
                    (
                        tools::grant_banishes,
                        schedule_offer,
                        open_offer.run_if(world_runs),
                        withdraw_offer,
                        (tools::apply_reroll, tools::apply_banish).chain(),
                        apply_pick,
                        hold_intermission,
                    )
                        .chain(),
                    apply_entries,
                    (blood_money, teddys_tithe, bounty, butchers_bill, lucky_strike),
                    (scorched_earth, overkill),
                    crate::economy::power_up::pick_up,
                )
                    .in_set(HostSimulation),
            );
        extra(app);
    });
    let world = sim.world_mut();
    for player in [local, teammate] {
        world.entity_mut(player).insert(Perks::default());
    }
    sim.run(2);
    (sim, local, teammate)
}

fn end_round(sim: &mut Sim) {
    sim.send(RoundEnded);
    sim.run(sim.frames(BOON_OFFER_DELAY_SECS + 0.5));
}

fn has_offer(sim: &mut Sim, player: Entity) -> bool {
    sim.world_mut().get::<Offer>(player).is_some()
}

fn pick(sim: &mut Sim, player: Entity, card: Option<usize>) {
    sim.send(PickOffer { player, card });
    sim.run(2);
}

fn holding_intermission(sim: &mut Sim) -> bool {
    sim.world_mut().resource::<IntermissionHold>().0
}

/// The ids on the Player's open Offer.
fn offered(sim: &mut Sim, player: Entity) -> Vec<&'static str> {
    match &sim.world_mut().get::<Offer>(player).expect("an open Offer").choices {
        Choices::Boons(boons) => boons.iter().map(|boon| boon.id).collect(),
        Choices::Pacts(pacts) => pacts.iter().map(|pact| pact.id()).collect(),
    }
}

fn pact_def(id: &str) -> &'static PactDef {
    PACTS.iter().find(|pact| pact.id() == id).unwrap()
}

/// Opens a Pact Offer of just `pact` for `player`, as the draw would.
fn offer_pact(sim: &mut Sim, player: Entity, pact: &'static PactDef) {
    let offer = Offer::new(Choices::Pacts(vec![pact]), 5);
    sim.world_mut().entity_mut(player).insert((offer, Choosing));
}

fn hold_pact(sim: &mut Sim, pact: &'static PactDef, holder: u8) {
    sim.world_mut().resource_mut::<HeldPacts>().add(pact, Seat(holder));
    sim.run(2);
}

fn has_entry_from(sim: &mut Sim, player: Entity, pact: &'static PactDef, stat: Stat) -> bool {
    let source = Source(pact.name());
    sim.world_mut()
        .get::<Modifiers>(player)
        .unwrap()
        .0
        .iter()
        .any(|entry| entry.source == source && entry.stat == stat)
}

pub(super) fn killed(by: Entity, pay: Pay, attack: Attack) -> ZombieKilled {
    ZombieKilled {
        player: Some(by),
        pay,
        headshot: false,
        zone: HitZone::Torso,
        attack,
        at: Vec3::ZERO,
        surge: false,
        zombie: Entity::PLACEHOLDER,
    }
}

pub(super) fn give_stacks(sim: &mut Sim, player: Entity, id: &str, stacks: u32) {
    let boon = super::boon::find(id).unwrap();
    let mut held = sim.world_mut().get_mut::<HeldBoons>(player).unwrap();
    for _ in 0..stacks {
        held.add(boon);
    }
}

fn bonus_paid(sim: &mut Sim) -> Vec<(Entity, u64)> {
    sim.world_mut()
        .resource_mut::<Messages<BonusPoints>>()
        .drain()
        .map(|bonus| (bonus.player, bonus.amount))
        .collect()
}

#[test]
fn every_player_gets_their_own_offer_while_the_world_runs() {
    let (mut sim, local, teammate) = gauntlet_players();
    end_round(&mut sim);
    assert!(has_offer(&mut sim, local) && has_offer(&mut sim, teammate));
    assert!(sim.world_mut().get::<Choosing>(local).is_some());
    assert!(!sim.world_mut().resource::<Freeze>().0, "Co-op never freezes");
}

#[test]
fn each_offer_is_drawn_from_that_players_own_unlocks_and_boons() {
    let (mut sim, local, teammate) = gauntlet_players();
    let bought: Vec<&BoonDef> = BOONS
        .iter()
        .filter(|boon| !boon.starts_unlocked && boon.requirements.is_empty())
        .collect();
    assert!(!bought.is_empty());
    sim.world_mut().get_mut::<PlayerUnlocks>(teammate).unwrap().0 =
        bought.iter().map(|boon| boon.id.to_string()).collect();
    // A Duo is offered once its two Boons are held, and is never bought itself: it's as Unlocked
    // as its recipe is.
    let unlocked_for = |unlocked: &[&BoonDef], id: &str| {
        let open = |boon: &BoonDef| boon.starts_unlocked || unlocked.iter().any(|b| b.id == boon.id);
        let boon = super::boon::find(id).unwrap();
        boon.duo.map_or(open(boon), |duo| duo.recipe.iter().all(|&part| open(super::boon::find(part).unwrap())))
    };
    for round in 1..=30 {
        sim.world_mut().insert_resource(Round::in_progress(round * 5 + 1));
        end_round(&mut sim);
        for boon in offered(&mut sim, local) {
            assert!(unlocked_for(&[], boon), "{boon} was never bought");
        }
        for boon in offered(&mut sim, teammate) {
            assert!(unlocked_for(&bought, boon), "{boon} isn't theirs");
        }
        pick(&mut sim, local, Some(0));
        pick(&mut sim, teammate, Some(0));
    }
    // Which bought Boons come up is a weighted draw, and they're Legendary or Proc Boons, so rare:
    // no count of Rounds guarantees one. What the draw is built from is checked instead: the
    // teammate's Unlocks make them eligible, and without those they aren't.
    let facts = |unlocked: Vec<String>| RunFacts {
        power_on: true,
        held_pacts: Vec::new(),
        used_box: true,
        bought_quick_revive: true,
        holds_upgraded: true,
        holds_ammo_mod: true,
        holds_proc_boon: true,
        unlocked,
        unlock_all: false,
        kit: None,
        banished: Vec::new(),
    };
    let own = sim.world_mut().get::<PlayerUnlocks>(teammate).unwrap().0.clone();
    assert!(bought.iter().all(|boon| super::boon::eligible(boon, &facts(own.clone()))));
    assert!(bought.iter().all(|boon| !super::boon::eligible(boon, &facts(Vec::new()))));
    // Picks went to their own pickers only.
    let local_total: u32 = sim.world_mut().get::<HeldBoons>(local).unwrap().0.iter().map(|(_, n)| n).sum();
    let teammate_total: u32 = sim.world_mut().get::<HeldBoons>(teammate).unwrap().0.iter().map(|(_, n)| n).sum();
    assert_eq!((local_total, teammate_total), (30, 30));
}

#[test]
fn a_boon_offer_checks_the_requirements_of_its_own_player() {
    let (mut sim, local, teammate) = gauntlet_players();
    // Only the teammate has used a Box, so only their Offers may hold Box Requirements.
    sim.world_mut().get_mut::<Bought>(teammate).unwrap().from_box = true;
    let boxed = BOONS.iter().filter(|boon| boon.starts_unlocked && boon.requirements.iter().any(|r| matches!(r, super::boon::Requirement::UsedBox))).count();
    if boxed == 0 {
        return;
    }
    for round in 1..=20 {
        sim.world_mut().insert_resource(Round::in_progress(round * 5 + 1));
        end_round(&mut sim);
        for id in offered(&mut sim, local) {
            let boon = super::boon::find(id).unwrap();
            assert!(!boon.requirements.iter().any(|r| matches!(r, super::boon::Requirement::UsedBox)));
        }
        pick(&mut sim, local, Some(0));
        pick(&mut sim, teammate, Some(0));
    }
}

#[test]
fn the_intermission_waits_for_every_pick() {
    let (mut sim, local, teammate) = gauntlet_players();
    end_round(&mut sim);
    assert!(holding_intermission(&mut sim));
    pick(&mut sim, local, Some(0));
    assert!(holding_intermission(&mut sim), "the teammate hasn't picked");
    // Past the Offer delay and well short of the timeout.
    sim.run(sim.frames(10.0));
    assert!(holding_intermission(&mut sim));
    pick(&mut sim, teammate, Some(0));
    assert!(!holding_intermission(&mut sim));
    assert!(!has_offer(&mut sim, local) && !has_offer(&mut sim, teammate));
    assert!(sim.world_mut().get::<Choosing>(teammate).is_none());
}

#[test]
fn the_wait_ends_after_thirty_seconds_and_the_late_offer_stays_open() {
    let (mut sim, local, teammate) = gauntlet_players();
    end_round(&mut sim);
    pick(&mut sim, local, Some(0));
    sim.run(sim.frames(BOON_OFFER_COOP_TIMEOUT_SECS - 3.0));
    assert!(holding_intermission(&mut sim), "still inside the 30 s");
    sim.run(sim.frames(4.0));
    assert!(!holding_intermission(&mut sim), "the next Round may start");
    assert!(has_offer(&mut sim, teammate), "nothing is picked at random");
    assert!(sim.world_mut().get::<Choosing>(teammate).is_some(), "the late Offer still locks them out");
    // The next Round's Offer waits behind it, and the teammate picks them in turn.
    sim.world_mut().insert_resource(Round::in_progress(4));
    end_round(&mut sim);
    assert!(has_offer(&mut sim, local));
    assert_eq!(sim.world_mut().get::<Offer>(teammate).unwrap().round, 3);
    pick(&mut sim, teammate, Some(0));
    sim.run(2);
    assert_eq!(sim.world_mut().get::<Offer>(teammate).unwrap().round, 4);
}

#[test]
fn an_open_offer_blocks_the_players_input_but_not_their_being_hurt() {
    let (mut sim, local, _teammate) = gauntlet_players();
    let held_forward = |sim: &mut Sim| {
        sim.world_mut()
            .run_system_once(|controls: Controls| controls.pressed(Action::Forward))
            .unwrap()
    };
    let key = sim.world_mut().resource::<Settings>().bindings.get(Action::Forward);
    let crate::players::controls::Binding::Key(key) = key else {
        panic!("Forward is a key by default");
    };
    sim.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
    assert!(held_forward(&mut sim));
    assert!(sim.world_mut().run_system_once(input_runs).unwrap());
    end_round(&mut sim);
    assert!(!held_forward(&mut sim), "no moving, firing, Use or Knife with an Offer open");
    assert!(!sim.world_mut().run_system_once(input_runs).unwrap());
    sim.world_mut().get_mut::<Health>(local).unwrap().current = 40.0;
    sim.world_mut().get_mut::<Health>(local).unwrap().current -= 10.0;
    assert_eq!(sim.world_mut().get::<Health>(local).unwrap().current, 30.0);
    pick(&mut sim, local, Some(0));
    assert!(held_forward(&mut sim));
}

#[test]
fn a_downed_player_gets_their_offer_and_the_pistol_waits_for_the_pick() {
    let (mut sim, _local, teammate) = gauntlet_players();
    sim.world_mut().get_mut::<Health>(teammate).unwrap().current = 0.0;
    sim.run(2);
    assert!(sim.world_mut().get::<LastStand>(teammate).is_some());
    end_round(&mut sim);
    assert!(has_offer(&mut sim, teammate), "a Last Stand doesn't hold the Offer back in Co-op");
    assert!(sim.world_mut().get::<Choosing>(teammate).is_some());
    pick(&mut sim, teammate, Some(0));
    assert!(sim.world_mut().get::<Choosing>(teammate).is_none());
}

#[test]
fn a_bleed_out_carries_the_offer_to_the_next_intermission() {
    let (mut sim, local, teammate) = gauntlet_players();
    end_round(&mut sim);
    pick(&mut sim, local, Some(0));
    sim.world_mut().get_mut::<Health>(teammate).unwrap().current = 0.0;
    sim.run(sim.frames(LAST_STAND_COOP_SECS + 1.0));
    assert!(sim.world_mut().get::<AwaitingRespawn>(teammate).is_some());
    assert!(!has_offer(&mut sim, teammate), "the Offer went with the body");
    assert!(sim.world_mut().get::<Choosing>(teammate).is_none());
    assert_eq!(sim.world_mut().get::<PendingOffer>(teammate).unwrap().owed, [3]);
    // The next Round's end brings back the missed Offer first, then its own.
    sim.world_mut().insert_resource(Round::in_progress(4));
    end_round(&mut sim);
    assert_eq!(sim.world_mut().get::<Offer>(teammate).unwrap().round, 3);
    pick(&mut sim, teammate, Some(0));
    sim.run(2);
    assert_eq!(sim.world_mut().get::<Offer>(teammate).unwrap().round, 4);
    assert_eq!(sim.world_mut().get::<HeldBoons>(teammate).unwrap().0.len(), 1);
}

#[test]
fn boons_and_pacts_survive_a_bleed_out() {
    let (mut sim, _local, teammate) = gauntlet_players();
    give_stacks(&mut sim, teammate, "blood_money", 2);
    hold_pact(&mut sim, pact_def("gold_rush"), 1);
    sim.run(2);
    let entries = |sim: &mut Sim| sim.world_mut().get::<Modifiers>(teammate).unwrap().0.len();
    let before = entries(&mut sim);
    assert!(before > 0);
    sim.world_mut().get_mut::<Health>(teammate).unwrap().current = 0.0;
    sim.run(sim.frames(LAST_STAND_COOP_SECS + 1.0));
    assert!(sim.world_mut().get::<AwaitingRespawn>(teammate).is_some());
    assert_eq!(sim.world_mut().get::<HeldBoons>(teammate).unwrap().stacks("blood_money"), 2);
    assert!(sim.world_mut().resource::<HeldPacts>().holds("gold_rush"));
    assert_eq!(entries(&mut sim), before);
    sim.world_mut().resource_mut::<Messages<RoundStarted>>().clear();
    sim.send(RoundStarted);
    sim.run(2);
    assert!(sim.world_mut().get::<AwaitingRespawn>(teammate).is_none());
    assert_eq!(sim.world_mut().get::<HeldBoons>(teammate).unwrap().stacks("blood_money"), 2);
    assert_eq!(entries(&mut sim), before);
}

#[test]
fn a_pact_held_by_one_player_is_never_offered_to_another() {
    let (mut sim, local, teammate) = gauntlet_players();
    sim.world_mut().resource_mut::<UnlockAll>().0 = true;
    // The first Pact Offer: no two Players share a Pact on their Offers.
    for _ in 0..15 {
        sim.world_mut().insert_resource(Round::in_progress(5));
        end_round(&mut sim);
        let (mine, theirs) = (offered(&mut sim, local), offered(&mut sim, teammate));
        assert_eq!((mine.len(), theirs.len()), (3, 3));
        assert!(mine.iter().all(|id| !theirs.contains(id)), "{mine:?} and {theirs:?} share a Pact");
        for player in [local, teammate] {
            sim.world_mut().entity_mut(player).remove::<(Offer, Choosing)>();
        }
        sim.world_mut().get_mut::<PendingOffer>(local).unwrap().left = None;
        sim.world_mut().get_mut::<PendingOffer>(teammate).unwrap().left = None;
    }
    // Once the local Player takes one, it's off the teammate's Offers for good.
    sim.world_mut().insert_resource(Round::in_progress(5));
    end_round(&mut sim);
    let taken = offered(&mut sim, local)[0];
    pick(&mut sim, local, Some(0));
    pick(&mut sim, teammate, None);
    assert_eq!(sim.world_mut().resource::<HeldPacts>().holder(taken), Some(Seat(0)));
    for _ in 0..15 {
        sim.world_mut().insert_resource(Round::in_progress(10));
        end_round(&mut sim);
        assert!(!offered(&mut sim, teammate).contains(&taken));
        assert!(!offered(&mut sim, local).contains(&taken));
        pick(&mut sim, local, None);
        pick(&mut sim, teammate, None);
    }
}

#[test]
fn a_world_drawback_falls_on_everyone_and_the_upside_stays_with_the_picker() {
    let (mut sim, local, teammate) = gauntlet_players();
    offer_pact(&mut sim, teammate, pact_def(GOLD_RUSH.id));
    pick(&mut sim, teammate, Some(0));
    assert_eq!(sim.world_mut().resource::<HeldPacts>().holder(GOLD_RUSH.id), Some(Seat(1)));
    let gold_rush = pact_def(GOLD_RUSH.id);
    assert!(has_entry_from(&mut sim, teammate, gold_rush, Stat::PointsRate), "the picker keeps the upside");
    assert!(has_entry_from(&mut sim, teammate, gold_rush, Stat::Price));
    assert!(!has_entry_from(&mut sim, local, gold_rush, Stat::PointsRate), "a teammate's upside isn't theirs");
    assert!(has_entry_from(&mut sim, local, gold_rush, Stat::Price), "prices rise for everyone");
}

#[test]
fn a_personal_drawback_stays_with_the_picker() {
    let (mut sim, local, teammate) = gauntlet_players();
    let cold_steel = pact_def("cold_steel");
    hold_pact(&mut sim, cold_steel, 0);
    assert!(cold_steel.entries.iter().all(|(stat, _)| !cold_steel.world_drawbacks.contains(stat)));
    assert!(cold_steel.entries.iter().all(|&(stat, _)| has_entry_from(&mut sim, local, cold_steel, stat)));
    assert!(sim.world_mut().get::<Modifiers>(teammate).unwrap().0.iter().all(|entry| entry.source != Source(cold_steel.name())));
}

#[test]
fn proc_kills_pay_only_their_holder() {
    let (mut sim, local, teammate) = gauntlet_players();
    give_stacks(&mut sim, local, "blood_money", 3);
    sim.send(killed(teammate, Pay::Paid, Attack::Bullet));
    sim.run(1);
    assert!(bonus_paid(&mut sim).is_empty(), "Blood Money paid for a teammate's kill");
    sim.send(killed(local, Pay::Paid, Attack::Bullet));
    sim.run(1);
    let paid = bonus_paid(&mut sim);
    assert_eq!(paid.len(), 1);
    assert_eq!(paid[0].0, local);
    assert!(paid[0].1 > 0);
}

#[test]
fn boons_that_bend_a_shared_thing_apply_only_when_their_holder_does_it() {
    let (mut sim, local, teammate) = gauntlet_players();
    give_stacks(&mut sim, local, "teddys_tithe", 2);
    sim.send(TeddyBear { spinner: teammate });
    sim.run(1);
    assert!(bonus_paid(&mut sim).is_empty(), "Teddy's Tithe paid for a teammate's Teddy Bear");
    sim.send(TeddyBear { spinner: local });
    sim.run(1);
    assert_eq!(bonus_paid(&mut sim).iter().map(|&(player, _)| player).collect::<Vec<_>>(), [local]);

    // Bounty: the Trap's kill pays the Player it was credited to.
    give_stacks(&mut sim, local, "bounty", 2);
    sim.send(killed(teammate, Pay::Paid, Attack::Trap));
    sim.run(1);
    assert!(bonus_paid(&mut sim).iter().all(|&(player, _)| player != local));
    sim.send(killed(local, Pay::Paid, Attack::Trap));
    sim.run(1);
    assert!(bonus_paid(&mut sim).iter().any(|&(player, _)| player == local));

    // Lucky Strike: a teammate's pickup gives the holder nothing.
    give_stacks(&mut sim, local, "lucky_strike", 5);
    for _ in 0..40 {
        sim.send(PowerUpTaken(PowerUp::Carpenter, Some(teammate)));
        sim.run(1);
    }
    assert_eq!(sim.world_mut().resource_mut::<Messages<GivePowerUp>>().drain().count(), 0);
}

#[test]
fn a_pacts_procs_fire_only_for_its_holder() {
    let (mut sim, local, teammate) = gauntlet_players();
    hold_pact(&mut sim, pact_def(SCORCHED_EARTH.id), 0);
    hold_pact(&mut sim, pact_def(OVERKILL.id), 1);
    let kill = |by| KillDamage {
        player: Some(by),
        zombie: Entity::PLACEHOLDER,
        at: Vec3::ZERO,
        max_health: 100.0,
        overflow: 10.0,
        attack: Attack::Bullet,
        damage: 110.0,
        headshot: false,
    };
    sim.send(kill(teammate));
    sim.run(1);
    assert_eq!(sim.world_mut().resource_mut::<Messages<Explosion>>().drain().count(), 0);
    sim.send(kill(local));
    sim.run(1);
    let blasts: Vec<_> = sim.world_mut().resource_mut::<Messages<Explosion>>().drain().collect();
    assert_eq!(blasts.len(), 1);
    assert_eq!(blasts[0].thrower, Some(local));
    assert!(!blasts[0].hurts_player, "Procs never hurt teammates");
}

#[test]
fn longer_power_ups_follow_the_player_who_took_them() {
    let (mut sim, local, teammate) = gauntlet_players();
    sim.world_mut().insert_resource(ActivePowerUps::default());
    let longer = Modifier { stat: Stat::PowerUpDuration, source: Source("Test"), effect: Effect::Flat(10.0) };
    sim.world_mut().get_mut::<Modifiers>(teammate).unwrap().0.push(longer);
    let at = |sim: &mut Sim, player| sim.world_mut().get::<Transform>(player).unwrap().translation;
    let (local_at, teammate_at) = (at(&mut sim, local), at(&mut sim, teammate));
    sim.world_mut().spawn(Dropped::new(PowerUp::InstaKill, local_at));
    sim.run(2);
    let base = sim.world_mut().resource::<ActivePowerUps>().left(PowerUp::InstaKill);
    assert!(base > POWER_UP_TIMED_SECS - 1.0 && base <= POWER_UP_TIMED_SECS, "the local Player's lasts {base} s");
    sim.world_mut().spawn(Dropped::new(PowerUp::DoublePoints, teammate_at));
    sim.run(2);
    let longer = sim.world_mut().resource::<ActivePowerUps>().left(PowerUp::DoublePoints);
    assert!(longer > POWER_UP_TIMED_SECS + 9.0, "the teammate's lasts {longer} s");
}

#[test]
fn undying_self_revives_through_the_solo_last_stand_in_co_op() {
    let (mut sim, local, _teammate) = gauntlet_players();
    hold_pact(&mut sim, pact_def(UNDYING.id), 0);
    let before = balance(&mut sim, local);
    sim.world_mut().get_mut::<Health>(local).unwrap().current = 0.0;
    sim.run(2);
    let stand = sim.world_mut().get::<LastStand>(local).expect("a Last Stand");
    assert!(!stand.is_coop(), "the holder gets the solo one");
    assert_eq!(balance(&mut sim, local), before, "and it costs no Points");
    sim.run(sim.frames(LAST_STAND_SECS + 1.0));
    assert!(sim.world_mut().get::<LastStand>(local).is_none(), "they got up alone");
    assert!(sim.world_mut().get::<AwaitingRespawn>(local).is_none());
    assert!(sim.world_mut().get::<Health>(local).unwrap().current > 0.0);
    // Undying gives two: the second Down is the solo one too, and the third is the Co-op one.
    for down in 2..=3 {
        sim.world_mut().get_mut::<Health>(local).unwrap().current = 0.0;
        sim.run(2);
        let solo = !sim.world_mut().get::<LastStand>(local).unwrap().is_coop();
        assert_eq!(solo, down == 2, "Down {down}");
        if solo {
            sim.run(sim.frames(LAST_STAND_SECS + 1.0));
        }
    }
}

#[test]
fn a_player_still_to_self_revive_keeps_the_run_going() {
    let (mut sim, local, teammate) = gauntlet_players();
    hold_pact(&mut sim, pact_def(UNDYING.id), 0);
    // The teammate is out of the Run; the holder goes Down with a spare.
    sim.world_mut().entity_mut(teammate).insert(AwaitingRespawn);
    sim.world_mut().get_mut::<Health>(local).unwrap().current = 0.0;
    sim.run(2);
    assert!(sim.world_mut().get::<LastStand>(local).is_some());
    assert!(
        !matches!(sim.world_mut().resource::<NextState<GameState>>(), NextState::Pending(GameState::GameOver)),
        "the holder is about to get up"
    );
}

#[test]
fn a_dropped_players_missed_offers_come_in_their_first_intermission_back() {
    let (mut sim, local, teammate) = gauntlet_players();
    sim.world_mut().resource_mut::<UnlockAll>().0 = true;
    sim.world_mut().entity_mut(teammate).insert(Disconnected::default());
    for round in [3, 4] {
        sim.world_mut().insert_resource(Round::in_progress(round));
        end_round(&mut sim);
        pick(&mut sim, local, Some(0));
        assert!(!has_offer(&mut sim, teammate), "nothing opens for a Player who is away");
    }
    assert_eq!(sim.world_mut().get::<PendingOffer>(teammate).unwrap().owed, [3, 4]);
    // The Player holding a Pact keeps it off the missed Pact Offer.
    hold_pact(&mut sim, pact_def("gold_rush"), 0);
    sim.world_mut().entity_mut(teammate).remove::<Disconnected>();
    sim.world_mut().insert_resource(Round::in_progress(5));
    end_round(&mut sim);
    for round in [3, 4] {
        assert_eq!(sim.world_mut().get::<Offer>(teammate).unwrap().round, round);
        pick(&mut sim, teammate, Some(0));
        sim.run(2);
    }
    assert_eq!(sim.world_mut().get::<Offer>(teammate).unwrap().round, 5);
    assert!(!offered(&mut sim, teammate).contains(&"gold_rush"));
}

fn points_of(sim: &mut Sim, player: Entity) -> u64 {
    sim.world_mut().get::<Points>(player).unwrap().balance
}

fn give_points(sim: &mut Sim, player: Entity, balance: u64) {
    sim.world_mut().get_mut::<Points>(player).unwrap().balance = balance;
}

#[test]
fn a_reroll_swaps_the_cards_for_points_and_its_price_doubles() {
    let (mut sim, local, _) = gauntlet_players();
    give_points(&mut sim, local, 3500);
    end_round(&mut sim);
    assert!(has_offer(&mut sim, local));
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(points_of(&mut sim, local), 2500, "the first costs 1,000");
    assert_eq!(sim.world_mut().get::<Offer>(local).unwrap().rerolls, 1);
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(points_of(&mut sim, local), 500, "the second costs 2,000");
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(points_of(&mut sim, local), 500, "the third costs 4,000, which they can't pay");
    assert_eq!(sim.world_mut().get::<Offer>(local).unwrap().rerolls, 2);
    assert_eq!(offered(&mut sim, local).len(), BOON_OFFER_SIZE);
}

#[test]
fn a_reroll_starts_at_its_base_price_in_the_next_offer() {
    let (mut sim, local, _) = gauntlet_players();
    give_points(&mut sim, local, 10_000);
    end_round(&mut sim);
    sim.send(RerollOffer { player: local });
    sim.run(2);
    pick(&mut sim, local, Some(0));
    sim.world_mut().insert_resource(Round::in_progress(4));
    end_round(&mut sim);
    assert_eq!(sim.world_mut().get::<Offer>(local).unwrap().rerolls, 0);
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(points_of(&mut sim, local), 8000, "1,000 again, not 2,000");
}

#[test]
fn a_pact_offer_takes_neither_tool() {
    let (mut sim, local, _) = gauntlet_players();
    give_points(&mut sim, local, 5000);
    offer_pact(&mut sim, local, pact_def(GOLD_RUSH.id));
    sim.send(RerollOffer { player: local });
    sim.send(BanishCard { player: local, card: 0 });
    sim.run(2);
    assert_eq!(points_of(&mut sim, local), 5000);
    assert_eq!(sim.world_mut().get::<Banishes>(local).unwrap().charges, GAUNTLET_BANISHES_START);
    assert_eq!(offered(&mut sim, local), vec![GOLD_RUSH.id]);
}

#[test]
fn a_banish_spends_a_charge_and_the_boon_is_gone_for_the_run() {
    let (mut sim, local, _) = gauntlet_players();
    end_round(&mut sim);
    let before = offered(&mut sim, local);
    sim.send(BanishCard { player: local, card: 1 });
    sim.run(2);
    let after = offered(&mut sim, local);
    assert_eq!(after.len(), BOON_OFFER_SIZE);
    assert_ne!(after[1], before[1]);
    let banishes = sim.world_mut().get::<Banishes>(local).unwrap().clone();
    assert_eq!(banishes, Banishes { charges: 0, banished: vec![before[1]] });
    // No charge left: a second Banish does nothing.
    sim.send(BanishCard { player: local, card: 0 });
    sim.run(2);
    assert_eq!(offered(&mut sim, local), after);
}

#[test]
fn a_cleared_boss_round_gives_every_player_a_banish_charge() {
    let (mut sim, local, teammate) = gauntlet_players();
    sim.world_mut().insert_resource(Round::in_progress(10));
    end_round(&mut sim);
    for player in [local, teammate] {
        assert_eq!(
            sim.world_mut().get::<Banishes>(player).unwrap().charges,
            GAUNTLET_BANISHES_START + GAUNTLET_BANISHES_PER_BOSS
        );
    }
    sim.world_mut().insert_resource(Round::in_progress(7));
    end_round(&mut sim);
    assert_eq!(sim.world_mut().get::<Banishes>(local).unwrap().charges, 2, "an ordinary Round gives none");
}

#[test]
fn a_rounds_reward_lands_on_the_next_boon_offer_with_its_extra_pick() {
    use super::variety::{ChallengeKind, reward};
    let (mut sim, local, _) = gauntlet_players();
    sim.world_mut().resource_mut::<UnlockAll>().0 = true;
    sim.world_mut().resource_mut::<RoundVariety>().reward = reward(true, Some(ChallengeKind::Blitz));
    end_round(&mut sim);
    let offer = sim.world_mut().get::<Offer>(local).expect("an Offer");
    assert_eq!((offer.picks, offer.picked), (2, 0));
    assert!(offer.boost.rarity_bump);
    assert!(offer.boost.families.contains(&boon::Family::Ember));
    let Choices::Boons(cards) = &offer.choices else { panic!("a Boon Offer") };
    assert!(cards.iter().any(|card| card.family == boon::Family::Ember), "a guaranteed Ember card");
    assert!(sim.world_mut().resource::<RoundVariety>().reward.is_none(), "paid once");

    pick(&mut sim, local, Some(0));
    assert!(has_offer(&mut sim, local), "the extra pick keeps the Offer open");
    assert_eq!(offered(&mut sim, local).len(), BOON_OFFER_SIZE - 1);
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(sim.world_mut().get::<Offer>(local).unwrap().rerolls, 0, "no Reroll after a pick");
    pick(&mut sim, local, Some(0));
    assert!(!has_offer(&mut sim, local), "and the second pick closes it");
    assert_eq!(sim.world_mut().get::<HeldBoons>(local).unwrap().0.len(), 2);
}

#[test]
fn a_pact_offer_carries_the_reward_to_the_next_boon_offer() {
    use super::variety::reward;
    let (mut sim, local, _) = gauntlet_players();
    sim.world_mut().resource_mut::<UnlockAll>().0 = true;
    sim.world_mut().insert_resource(Round::in_progress(5));
    sim.world_mut().resource_mut::<RoundVariety>().reward = reward(true, None);
    end_round(&mut sim);
    let offer = sim.world_mut().get::<Offer>(local).expect("an Offer");
    assert!(matches!(offer.choices, Choices::Pacts(_)), "Round 5 is a Pact Round");
    assert!(offer.boost.is_none());
    assert!(sim.world_mut().resource::<RoundVariety>().reward.rarity_bump, "still owed");
    pick(&mut sim, local, None);
    sim.world_mut().insert_resource(Round::in_progress(6));
    end_round(&mut sim);
    let offer = sim.world_mut().get::<Offer>(local).expect("the next Offer");
    assert!(matches!(offer.choices, Choices::Boons(_)));
    assert!(offer.boost.rarity_bump, "the carried reward lands here");
}

#[test]
fn the_banish_track_and_the_kit_help_only_this_machines_player() {
    let (mut sim, local, teammate) = gauntlet_players_with(|app| {
        let mut file = GauntletFile { scrip: 5000, ..default() };
        file.buy_rank(progress::Track::Banish).unwrap();
        file.buy_rank(progress::Track::Banish).unwrap();
        app.insert_resource(file).insert_resource(ChosenKit(Kit::Blade));
    });
    let world = sim.world_mut();
    assert_eq!(world.get::<Banishes>(local).unwrap().charges, GAUNTLET_BANISHES_START + 2);
    assert_eq!(world.get::<Banishes>(teammate).unwrap().charges, GAUNTLET_BANISHES_START);
    assert_eq!(world.get::<PlayerKit>(local).unwrap().kit, Kit::Blade);
    assert_eq!(world.get::<PlayerKit>(teammate).unwrap().kit, Kit::Standard);
}

#[test]
fn a_family_kit_puts_its_rule_on_its_player_as_an_entry() {
    let (mut sim, local, teammate) = gauntlet_players_with(|app| {
        app.insert_resource(ChosenKit(Kit::Grit));
    });
    sim.run(2);
    let revives = |sim: &mut Sim, player| {
        let entries = sim.world_mut().get::<Modifiers>(player).unwrap().0.clone();
        crate::modifier::resolve(Stat::SelfRevives, 0.0, &entries)
    };
    assert_eq!(revives(&mut sim, local), 1.0);
    assert_eq!(revives(&mut sim, teammate), 0.0);
}

#[test]
fn the_blade_kits_first_boon_offer_has_a_blade_card_every_time() {
    for _ in 0..30 {
        let (mut sim, local, _) = gauntlet_players_with(|app| {
            app.insert_resource(ChosenKit(Kit::Blade));
        });
        sim.world_mut().resource_mut::<UnlockAll>().0 = true;
        end_round(&mut sim);
        let ids = offered(&mut sim, local);
        let blade = ids.iter().filter_map(|id| boon::find(id)).filter(|boon| boon.family == boon::Family::Blade).count();
        assert!(blade >= 1, "{ids:?}");
        assert!(sim.world_mut().get::<PlayerKit>(local).unwrap().offered);
    }
}

#[test]
fn a_standard_kit_leaves_offers_unbiased_and_unguaranteed() {
    let (mut sim, local, _) = gauntlet_players();
    sim.world_mut().resource_mut::<UnlockAll>().0 = true;
    end_round(&mut sim);
    let offer = sim.world_mut().get::<Offer>(local).unwrap();
    assert!(offer.boost.is_none());
}

#[test]
fn heat_conditions_put_their_entries_on_every_player_and_nothing_without_heat() {
    use super::heat::{Condition, RunHeat};
    let (mut sim, local, teammate) = gauntlet_players();
    sim.run(2);
    let price = |sim: &mut Sim, player| {
        let entries = sim.world_mut().get::<Modifiers>(player).unwrap().0.clone();
        crate::modifier::resolve(Stat::Price, 100.0, &entries)
    };
    assert_eq!((price(&mut sim, local), price(&mut sim, teammate)), (100.0, 100.0), "no Heat is neutral");
    let mut degrees = RunHeat::default();
    degrees.0[Condition::Inflation as usize] = 3;
    sim.world_mut().insert_resource(degrees);
    sim.run(2);
    assert!((price(&mut sim, local) - 135.0).abs() < 1e-3);
    assert!((price(&mut sim, teammate) - 135.0).abs() < 1e-3, "Heat is the Room's, so everyone pays");
    sim.world_mut().insert_resource(RunHeat::default());
    sim.run(2);
    assert_eq!(price(&mut sim, local), 100.0, "and lifting it takes the entries away");
}

#[test]
fn tight_offers_raises_every_players_reroll_price_and_trims_their_starting_banishes() {
    use super::heat::{Condition, RunHeat};
    let (mut sim, local, teammate) = gauntlet_players_with(|app| {
        let mut heat = RunHeat::default();
        heat.0[Condition::TightOffers as usize] = 2;
        app.insert_resource(heat);
    });
    sim.world_mut().resource_mut::<UnlockAll>().0 = true;
    for player in [local, teammate] {
        assert_eq!(sim.world_mut().get::<Banishes>(player).unwrap().charges, 0);
    }
    end_round(&mut sim);
    sim.world_mut().get_mut::<Points>(local).unwrap().balance = 1500;
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(sim.world_mut().get::<Offer>(local).unwrap().rerolls, 0, "1,500 can't buy a 2,000 Reroll");
    sim.world_mut().get_mut::<Points>(local).unwrap().balance = 2000;
    sim.send(RerollOffer { player: local });
    sim.run(2);
    assert_eq!(sim.world_mut().get::<Offer>(local).unwrap().rerolls, 1);
}
