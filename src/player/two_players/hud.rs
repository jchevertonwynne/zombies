//! What the Co-op HUD reads off two Players in one world.

use super::revive::{co_op, down, hold};
use super::*;
use crate::arsenal::WeaponId;
use crate::hud::coop::{CoopView, SeatHeld, Tag, is_coop, read_coop_view};
use crate::mystery_box::{BoxState, MysteryBox, Prize};
use crate::pack_a_punch::{PackAPunch, Work};
use crate::respawn::AwaitingRespawn;
use crate::state::{Choosing, Paused, RunType};
use crate::weapon::WeaponCopy;

fn view(sim: &mut Sim) -> CoopView {
    let world = sim.world_mut();
    world.init_resource::<CoopView>();
    world.run_system_once(read_coop_view).unwrap();
    world.resource::<CoopView>().clone()
}

fn tag_of(sim: &mut Sim) -> Option<String> {
    view(sim).rows[0].tag.as_ref().map(Tag::text)
}

#[test]
fn a_teammates_row_carries_their_name_seat_and_points() {
    let (mut sim, _, teammate) = co_op();
    let points = balance(&mut sim, teammate);
    let view = view(&mut sim);
    assert_eq!(view.rows.len(), 1, "the local Player has no row");
    assert_eq!((view.rows[0].seat, view.rows[0].name.as_str(), view.rows[0].points), (Seat(1), "Player 2", points));
    assert_eq!(view.rows[0].tag, None);
    assert!(view.icons.is_empty() && view.waiting.is_empty() && view.revive.is_none() && view.bleeding.is_none());
}

#[test]
fn only_a_co_op_run_shows_the_co_op_hud() {
    let (mut sim, ..) = co_op();
    assert!(sim.world_mut().run_system_once(is_coop).unwrap());
    sim.world_mut().insert_resource(RunType::Solo);
    assert!(!sim.world_mut().run_system_once(is_coop).unwrap());
}

#[test]
fn a_downed_teammate_is_a_tag_and_a_revive_icon_with_their_bleed_out() {
    let (mut sim, _, teammate) = co_op();
    down(&mut sim, teammate);
    let view = view(&mut sim);
    let tag = view.rows[0].tag.as_ref().unwrap().text();
    assert!(tag.starts_with("DOWN ") && tag.ends_with('s'), "{tag}");
    assert_eq!(view.icons.len(), 1);
    assert_eq!(view.icons[0].seat, Seat(1));
    assert!(view.icons[0].left > 40.0 && view.icons[0].reviver.is_none());
    assert!(view.bleeding.is_none(), "the local Player is up");
}

#[test]
fn a_revive_shows_on_both_players_and_the_local_bar() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, teammate);
    hold(&mut sim, local, teammate, 1.0);
    let view = view(&mut sim);
    assert_eq!(view.rows[0].tag.as_ref().unwrap().text(), "being Revived by Player");
    let (by, progress) = view.icons[0].reviver.expect("the icon shows the reviver");
    assert_eq!(by, Seat(0));
    assert!(progress > 0.0 && progress < 1.0);
    let bar = view.revive.expect("the local Revive bar");
    assert_eq!((bar.label.as_str(), bar.other), ("Reviving Player 2", Seat(1)));
}

#[test]
fn the_local_player_being_revived_and_bleeding_out_shows_the_bar_and_the_timer() {
    let (mut sim, local, teammate) = co_op();
    down(&mut sim, local);
    assert!(view(&mut sim).bleeding.unwrap() > 40.0);
    hold(&mut sim, teammate, local, 1.0);
    let view = view(&mut sim);
    let bar = view.revive.expect("the local Revive bar");
    assert_eq!((bar.label.as_str(), bar.other), ("Being Revived by Player 2", Seat(1)));
    assert_eq!(view.rows[0].tag.as_ref().unwrap().text(), "Reviving Player");
    assert!(view.icons.is_empty(), "the local Player gets no icon of their own");
}

#[test]
fn paused_choosing_waiting_and_held_teammates_each_read_so() {
    let (mut sim, _, teammate) = co_op();
    sim.world_mut().entity_mut(teammate).insert(Paused);
    assert_eq!(tag_of(&mut sim).as_deref(), Some("PAUSED"));
    sim.world_mut().entity_mut(teammate).remove::<Paused>().insert(Choosing);
    assert_eq!(tag_of(&mut sim).as_deref(), Some("choosing a Boon"));
    sim.world_mut().entity_mut(teammate).remove::<Choosing>().insert(AwaitingRespawn);
    assert_eq!(tag_of(&mut sim).as_deref(), Some("waiting to Respawn"));
    sim.world_mut().entity_mut(teammate).insert(SeatHeld);
    assert_eq!(tag_of(&mut sim).as_deref(), Some("Reconnecting… Seat held"));
    assert!(view(&mut sim).rows[0].held);
}

fn offering(spinner: Entity) -> MysteryBox {
    MysteryBox {
        location: 0,
        price: 950,
        teddy: true,
        state: BoxState::Offering {
            spinner,
            prize: Prize::Weapon(WeaponId::M14),
            left: 8.5,
            counts: true,
            upgraded: false,
        },
    }
}

#[test]
fn a_teammates_prize_is_labelled_over_the_box_but_the_local_players_is_not() {
    let (mut sim, local, teammate) = co_op();
    let weapon = Prize::Weapon(WeaponId::M14).shown_name(false);
    let at = Vec3::new(1.0, 0.0, 2.0);
    let mystery_box = sim.world_mut().spawn((offering(teammate), GlobalTransform::from_translation(at))).id();
    let shown = view(&mut sim);
    assert_eq!(shown.waiting.len(), 1);
    assert_eq!(shown.waiting[0].label, format!("Player 2's {weapon} 9"));
    assert_eq!(shown.waiting[0].seat, Seat(1));
    assert!(shown.waiting[0].at.y > at.y && shown.waiting[0].at.x == at.x, "it floats over the Box");
    sim.world_mut().get_mut::<MysteryBox>(mystery_box).unwrap().state = offering(local).state;
    assert!(view(&mut sim).waiting.is_empty());
}

#[test]
fn a_teammates_weapon_is_labelled_over_pack_a_punch_once_it_is_out() {
    let (mut sim, _, teammate) = co_op();
    let mut machine = PackAPunch::default();
    machine.insert_for(teammate, WeaponCopy::new(WeaponId::M14), Work::Upgrade);
    assert_eq!(machine.waiting(), None, "still processing");
    machine.advance(PAP_PROCESS_SECS + 1.0);
    let name = machine.waiting().unwrap().weapon;
    sim.world_mut().spawn((machine, GlobalTransform::default()));
    let shown = view(&mut sim);
    assert_eq!(shown.waiting.len(), 1);
    assert_eq!(shown.waiting[0].label, format!("Player 2's {name} {}", PAP_PICKUP_SECS as u32 - 1));
}
