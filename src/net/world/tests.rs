//! Two or three headless games on the real Map, joined by a `Hub`: a client uses things in the
//! world and the Host confirms, and every game shows the result.

use super::*;
use crate::buy::resolve_use;
use crate::door::DoorPlugin;
use crate::gun_model::GunModels;
use crate::mystery_box::MysteryBoxPlugin;
use crate::net::link::Hub;
use crate::net::play::{install_client, install_host};
use crate::pack_a_punch::PackAPunchPlugin;
use crate::power_up::PowerUpAssets;
use crate::trap::TrapPlugin;
use crate::net::movement::machine;
use crate::power::PowerPlugin;
use crate::wall_weapon::{WallWeapon, WallWeaponPlugin};

/// What a game has been told so far.
#[derive(Resource, Default)]
pub(super) struct Seen {
    pub(super) cant_afford: Vec<Entity>,
    pub(super) refused: u32,
    pub(super) bought: u32,
    pub(super) torn: u32,
    pub(super) rounds_started: u32,
    pub(super) round_ends: u32,
}

fn count(
    mut seen: ResMut<Seen>,
    mut cant_afford: MessageReader<CantAfford>,
    mut refused: MessageReader<BuyRefused>,
    mut bought: MessageReader<Purchased>,
    mut torn: MessageReader<PlankTorn>,
    mut started: MessageReader<RoundStarted>,
    mut ended: MessageReader<RoundEnded>,
) {
    seen.cant_afford.extend(cant_afford.read().map(|tried| tried.player));
    seen.refused += refused.read().count() as u32;
    seen.bought += bought.read().count() as u32;
    seen.torn += torn.read().count() as u32;
    seen.rounds_started += started.read().count() as u32;
    seen.round_ends += ended.read().count() as u32;
}

/// One game of the Run: the Map and all `seats` Players, the one in `local` the `LocalPlayer`, and
/// the machines these tests use.
fn game(local: u8, seats: u8, extra: &dyn Fn(&mut App), install: impl FnOnce(&mut App)) -> App {
    machine(local, seats, |app| {
        app.init_resource::<Seen>()
            .init_resource::<GunModels>()
            .init_resource::<crate::modifier::RunModifiers>()
            .init_resource::<PowerUpAssets>()
            .add_plugins((DoorPlugin, PowerPlugin, MysteryBoxPlugin, PackAPunchPlugin, WallWeaponPlugin, TrapPlugin))
            .add_systems(Update, (resolve_use.in_set(HostSimulation), count))
            .add_systems(Update, give_perks);
        extra(app);
        install(app);
    })
}

/// A Host and `clients` clients over a quick, clean network.
pub(super) struct Run {
    hub: Hub,
    apps: Vec<App>,
}

impl Run {
    pub(super) fn new(clients: u8) -> Self {
        Self::with(clients, &|_| {})
    }

    /// The same, with `extra` added to every game before it starts.
    pub(super) fn with(clients: u8, extra: &dyn Fn(&mut App)) -> Self {
        let hub = Hub::new(1, 0.0);
        let seats = clients + 1;
        let host_link = hub.host();
        let links: Vec<_> = (0..clients).map(|_| hub.join()).collect();
        let remotes: Vec<(Peer, u8)> = links.iter().map(|link| (link.peer(), link.peer() as u8)).collect();
        let mut apps = vec![game(0, seats, extra, |app| install_host(app, host_link, remotes))];
        for link in links {
            let seat = link.peer() as u8;
            apps.push(game(seat, seats, extra, |app| install_client(app, link)));
        }
        let mut run = Run { hub, apps };
        run.step(30);
        run
    }

    pub(super) fn step(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.hub.step();
            for app in &mut self.apps {
                app.update();
            }
        }
    }

    pub(super) fn world(&mut self, on: usize) -> &mut World {
        self.apps[on].world_mut()
    }

    /// The Player in `seat`, as game `on` has them.
    pub(super) fn player(&mut self, on: usize, seat: u8) -> Entity {
        let world = self.world(on);
        let mut query = world.query_filtered::<(Entity, &PlayerSeat), With<Player>>();
        query.iter(world).find(|(_, held)| held.0 == seat).unwrap().0
    }

    pub(super) fn give_points(&mut self, seat: u8, balance: u64) {
        let player = self.player(0, seat);
        self.world(0).get_mut::<Points>(player).unwrap().balance = balance;
    }

    /// The Points the Host has for the Player in `seat`.
    pub(super) fn balance(&mut self, seat: u8) -> u64 {
        let player = self.player(0, seat);
        self.world(0).get::<Points>(player).unwrap().balance
    }

    /// Game `on`'s `LocalPlayer` uses `buyable`, as the Use key does.
    pub(super) fn use_buyable(&mut self, on: usize, buyable: Entity, hold: bool) {
        let player = self.player(on, on as u8);
        self.world(on).write_message(UseRequest { player, buyable, hold });
    }

    /// The first entity with `T` that `with` takes, as game `on` has it.
    pub(super) fn find<T: Component>(&mut self, on: usize, with: impl Fn(&T) -> bool) -> Entity {
        let world = self.world(on);
        let mut query = world.query::<(Entity, &T)>();
        query.iter(world).find(|(_, item)| with(item)).unwrap().0
    }

    /// What game `on` has been told so far.
    pub(super) fn seen(&mut self, on: usize) -> &Seen {
        self.world(on).resource::<Seen>()
    }
}

/// The Door of a Gate that costs 750 or less, as game `on` has it.
fn cheap_door(run: &mut Run, on: usize) -> Entity {
    run.find::<Door>(on, |door| door.price.is_some_and(|price| price <= 1000))
}

#[test]
fn a_client_buys_a_door_and_every_machine_sees_it_open() {
    let mut run = Run::new(2);
    run.give_points(1, 5000);
    let others = run.balance(2);
    let on_client = cheap_door(&mut run, 1);
    let cost = run.world(1).get::<Door>(on_client).unwrap().price.unwrap();
    let id = *run.world(1).get::<NetId>(on_client).unwrap();
    run.use_buyable(1, on_client, false);
    run.step(20);
    for on in 0..3 {
        let door = run.world(on).resource::<NetIds>().entity(id.0).expect("every game names the Door alike");
        assert!(run.world(on).get::<Door>(door).unwrap().open, "game {on} still has the Door shut");
    }
    assert_eq!(run.balance(1), 5000 - cost, "the Host charged the buyer");
    assert_eq!(run.balance(2), others, "and no one else");
    assert_eq!(run.seen(1).bought, 1, "the buyer is told the buy went through");
    assert_eq!(run.seen(2).bought, 0);
}

#[test]
fn a_client_who_cant_afford_a_door_gets_the_refusal_and_the_door_stays_shut() {
    let mut run = Run::new(1);
    run.give_points(1, 10);
    let door = cheap_door(&mut run, 1);
    run.use_buyable(1, door, false);
    run.step(20);
    let client = run.player(1, 1);
    assert_eq!(run.seen(1).cant_afford, [client]);
    assert_eq!(run.seen(1).bought, 0);
    assert_eq!(run.balance(1), 10);
    let id = *run.world(1).get::<NetId>(door).unwrap();
    for on in 0..2 {
        let door = run.world(on).resource::<NetIds>().entity(id.0).unwrap();
        assert!(!run.world(on).get::<Door>(door).unwrap().open);
    }
    // The Host's own game hears the request it judged too, but it names the teammate.
    let teammate = run.player(0, 1);
    assert_eq!(run.seen(0).cant_afford, [teammate]);
    assert_ne!(teammate, run.player(0, 0));
}

/// The first spin of a Box gives no Teddy, so a test can count on a Weapon.
fn spin_box(run: &mut Run, spinner: usize) {
    let mystery_box = run.find::<MysteryBox>(spinner, |_| true);
    run.use_buyable(spinner, mystery_box, false);
}

#[test]
fn the_box_weapon_is_offered_to_the_spinner_alone() {
    let mut run = Run::new(2);
    // No Teddy Bear, so the spin always offers a Weapon.
    let host_box = run.find::<MysteryBox>(0, |_| true);
    run.world(0).get_mut::<MysteryBox>(host_box).unwrap().teddy = false;
    run.give_points(1, 5000);
    run.give_points(2, 5000);
    spin_box(&mut run, 1);
    run.step((BOX_SPIN_SECS * f32::from(FIXED_HZ as u16)) as u32 + 30);
    // Every game sees the Weapon in the Box and who spun it.
    for on in 0..3 {
        let spinner = run.player(on, 1);
        let mystery_box = run.find::<MysteryBox>(on, |_| true);
        let state = run.world(on).get::<MysteryBox>(mystery_box).unwrap().state;
        assert!(
            matches!(state, BoxState::Offering { spinner: held, .. } if held == spinner),
            "game {on}: {state:?}"
        );
    }
    let before: Vec<WeaponId> = {
        let teammate = run.player(0, 2);
        run.world(0).get::<Loadout>(teammate).unwrap().weapons().map(|weapon| weapon.id).collect()
    };
    // The other Player takes it: nothing happens.
    spin_box(&mut run, 2);
    run.step(20);
    let teammate = run.player(2, 2);
    let held: Vec<WeaponId> = run.world(2).get::<Loadout>(teammate).unwrap().weapons().map(|weapon| weapon.id).collect();
    assert_eq!(held, before, "the Weapon went to someone who didn't spin");
    // The spinner takes it, and gets it by the Host's word alone.
    let spinner = run.player(1, 1);
    let had = run.world(1).get::<Loadout>(spinner).unwrap().weapons().count();
    spin_box(&mut run, 1);
    run.step(20);
    assert_eq!(run.world(1).get::<Loadout>(spinner).unwrap().weapons().count(), had + 1, "the spinner has the Weapon");
    let spinner_on_host = run.player(0, 1);
    assert_eq!(run.world(0).get::<Loadout>(spinner_on_host).unwrap().weapons().count(), had + 1);
    for on in 0..3 {
        let mystery_box = run.find::<MysteryBox>(on, |_| true);
        assert!(
            matches!(run.world(on).get::<MysteryBox>(mystery_box).unwrap().state, BoxState::Rearming { .. }),
            "game {on} still offers it"
        );
    }
}

#[test]
fn a_full_ammo_wall_weapon_buy_is_refused_and_charges_nothing() {
    let mut run = Run::new(1);
    run.give_points(1, 5000);
    let chalk = run.find::<WallWeapon>(1, |wall| wall.0 == WeaponId::M14);
    run.use_buyable(1, chalk, false);
    run.step(20);
    let player = run.player(1, 1);
    assert!(run.world(1).get::<Loadout>(player).unwrap().slot_of(WeaponId::M14).is_some(), "the Weapon came");
    let paid = 5000 - run.balance(1);
    assert!(paid > 0);
    // Its ammo is full, so the client's own view refuses a second buy.
    run.step(5);
    run.use_buyable(1, chalk, false);
    run.step(20);
    assert_eq!(run.balance(1), 5000 - paid, "a full-ammo buy was charged");
    assert_eq!(run.seen(1).refused, 1);
    // After firing some, the ammo buy goes through and fills it.
    {
        let mut loadout = run.world(1).get_mut::<Loadout>(player).unwrap();
        loadout.held_mut().reserve = 3;
    }
    run.step(5);
    run.use_buyable(1, chalk, false);
    run.step(20);
    assert!(run.balance(1) < 5000 - paid, "the ammo buy wasn't charged");
    let loadout = run.world(1).get::<Loadout>(player).unwrap();
    assert!(loadout.held().reserve > 3, "the Host's refill reached the client");
}

/// Gives the Host's copy of the Player in `seat` a Weapon, as a buy would.
pub(super) fn give_weapon(run: &mut Run, seat: u8, id: WeaponId) {
    let player = run.player(0, seat);
    run.world(0).get_mut::<Loadout>(player).unwrap().give(id);
}

/// Every Player holds Perks, as the Perk machines' plugin gives them.
fn give_perks(mut commands: Commands, players: Query<Entity, (With<Player>, Without<Perks>)>) {
    for player in &players {
        commands.entity(player).insert(Perks::default());
    }
}
