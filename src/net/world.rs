//! The Run's shared world over the reliable stream: what a client asks of it, and what the Host
//! says has changed. Nothing is predicted.
//!
//! A client's Use at a Buyable goes to the Host as a `Request`. The Host judges it like one of its
//! own (`resolve_use`), answers the Player with `Bought`, `CantAfford` or `Refused`, and tells
//! everyone what that changed. Rarely changing state goes out as `WorldEvent`s: the Host watches
//! each machine and says so when something new about it is true. A client applies them to its own
//! copy of the world and runs the clocks of what it was told between the Host's words, so Doors,
//! the Box, Traps and the rest look and sound the same everywhere, and only the Host's word ends a
//! state. A client's own Weapons change by the Host's `Hands` alone, apart from its own switching,
//! Magazine, Reloads and fire rate; its switches are reported, and the Host passes them on.
//!
//! Buyables and machines are named across the network by a `NetId`. The Map spawns them the same
//! on every machine, so they are numbered the same way: by where they stand.

use std::collections::{HashMap, HashSet};

use avian3d::prelude::ColliderDisabled;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::play::{Prediction, Roster, Wire};
use super::protocol::{HOST, Peer, Seat, ToHost, ToPlayer, encode};
use super::remote::Remote;
use crate::ammo_mod::AmmoMods;
use crate::arsenal::WeaponId;
use crate::attachment::{Attachment, UnderbarrelAmmo};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::{Authority, HostSimulation};
use crate::barrier::{Barrier, Barriers, PlankRebuilt, PlankTorn, plank_news};
use crate::buy::{
    BuyRefused, Buyable, CantAfford, HoldPurchased, Purchased, RefusedByPlayer, Refused, UseRequest,
};
use crate::door::{Door, open};
use crate::gauntlet::{
    HeldBoons, HeldPacts, Offer, OfferWire, PickOffer, boons_wire, pacts_wire, show_boons, show_pacts,
};
use crate::last_stand::{Down as WentDown, DownOutcome, LastStand};
use crate::leave_run::PlayerLeft;
use crate::equipment::Equipment;
use crate::map::BarrierId;
use crate::mystery_box::{BoxState, MysteryBox, Prize};
use crate::pack_a_punch::PackAPunch;
use crate::perk::{Drinking, Perk, PerkMachine, Perks};
use crate::player::{Health, LocalPlayer, Player, PlayerHit, PlayerName, Seat as PlayerSeat};
use crate::respawn::{AwaitingRespawn, Respawned};
use crate::revive::{Revived, Reviving};
use crate::points::Points;
use crate::power::{Power, PowerSwitch};
use crate::power_up::{ActivePowerUps, Dropped, PowerUp, PowerUpAssets, PowerUpTaken, spawn_drop};
use crate::round::{Round, RoundEnded, RoundPhase, RoundStarted};
use crate::state::{Choosing, GameState, Paused, world_runs};
use crate::teammate::HeldWeapon;
use crate::teleporter::{Mainframe, Pad, TeleporterView, mainframe_use_point};
use crate::trap::{Trap, TrapPhase, TrapSwitch};
use crate::tuning::*;
use crate::modifier::{Modifiers, Stat, resolve};
use crate::weapon::{HostOp, Knife, Loadout, WeaponCopy};

/// What a machine or Buyable is called on the wire.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NetId(pub u16);

/// Every machine and Buyable that has a `NetId`, by id.
#[derive(Resource, Default)]
pub struct NetIds {
    by_id: HashMap<u16, Entity>,
    next: u16,
}

impl NetIds {
    /// The machine with `id`, if there is one.
    pub fn entity(&self, id: u16) -> Option<Entity> {
        self.by_id.get(&id).copied()
    }
}

/// What a client asks of the Host.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Request {
    /// Use the Buyable (or, with `hold`, its hold half). `refused` is whether their own game
    /// refuses it for them, as a full-ammo buy is.
    Use { buyable: u16, hold: bool, refused: bool },
    /// They have taken the Weapon in this slot in hand.
    Held { slot: u8 },
    /// They are holding Use over the teammate in this Seat, who is in Last Stand. Said every
    /// frame it is held.
    Revive { target: Seat },
    /// Their Pause menu is open, or has closed.
    Paused(bool),
    /// They picked this card of their open Offer, or skipped a Pact Offer.
    Pick { card: Option<u8> },
}

/// A Weapon copy on the wire: all of it that its holder's game can't work out for itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CopyWire {
    id: WeaponId,
    upgraded: bool,
    ammo_mods: AmmoMods,
    repacks: u32,
    attachments: Vec<Attachment>,
    magazine: u32,
    reserve: u32,
}

impl CopyWire {
    pub fn of(copy: &WeaponCopy) -> Self {
        Self {
            id: copy.id,
            upgraded: copy.upgraded,
            ammo_mods: copy.ammo_mods,
            repacks: copy.repacks,
            attachments: copy.attachments.clone(),
            magazine: copy.magazine,
            reserve: copy.reserve,
        }
    }

    pub fn build(&self) -> WeaponCopy {
        let mut copy = WeaponCopy::new(self.id);
        if self.upgraded {
            copy = copy.upgrade();
        }
        copy.ammo_mods = self.ammo_mods;
        copy.repacks = self.repacks;
        for &attachment in &self.attachments {
            copy.modifiers.extend(attachment.modifiers());
            if let Some(kind) = attachment.underbarrel() {
                copy.underbarrel = Some(UnderbarrelAmmo::full(kind));
            }
            copy.attachments.push(attachment);
        }
        copy.magazine = self.magazine;
        copy.reserve = self.reserve;
        copy
    }
}

/// What the Host did to a Player's Weapons.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum HandOp {
    Put { slot: u8, copy: CopyWire },
    Take,
    Refill(u8),
    MaxAmmo,
    Slots(u8),
    /// Everything they hold is swapped for this: each slot, and the one in hand.
    Replace { slots: Vec<Option<CopyWire>>, held: u8 },
}

/// What the Host gave or refilled of a Player's Equipment, as the most they now carry of each.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Gear {
    frags: u32,
    claymores_owned: bool,
    claymores: u32,
    monkey_bombs_owned: bool,
    monkey_bombs: u32,
    bowie: bool,
}

/// Where a Mystery Box is, as the Host tells it: who spun it and what it offers are the parts of
/// its state that everyone sees. How long each has gone on is each game's own clock.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum BoxPhase {
    Idle,
    Spinning { spinner: Seat },
    Offering { spinner: Seat, prize: Prize, upgraded: bool },
    Rearming,
    Leaving,
    Away { to: u8 },
}

/// A Weapon inside Pack-a-Punch.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Inside {
    weapon: CopyWire,
    elapsed: f32,
    owner: Option<Seat>,
}

/// A Player in Last Stand, as everyone is told.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Down {
    /// Seconds left of it.
    pub left: f32,
    /// Whether it ends in a teammate's Revive, not a Self-Revive.
    pub coop: bool,
}

/// A Revive in progress, as everyone is told.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Revive {
    pub target: Seat,
    pub elapsed: f32,
    pub needed: f32,
}

/// What a Player is going through that their teammates' HUD shows, and that the Host judges.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Status {
    pub down: Option<Down>,
    pub awaiting: bool,
    pub reviving: Option<Revive>,
    pub paused: bool,
    pub choosing: bool,
}

impl Status {
    /// Whether this is the same as `other`, as far as a change is worth telling: the time left of
    /// a Last Stand and the length of a hold are each game's own clock.
    fn same(&self, other: &Status) -> bool {
        self.down.map(|down| down.coop) == other.down.map(|down| down.coop)
            && self.awaiting == other.awaiting
            && self.reviving.map(|revive| revive.target) == other.reviving.map(|revive| revive.target)
            && self.paused == other.paused
            && self.choosing == other.choosing
    }
}

/// Something the Host says has changed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorldEvent {
    /// To one Player, their Use of `buyable` went through (and cost them what it cost).
    Bought { buyable: u16, hold: bool },
    /// To one Player, they can't afford what they tried to use.
    CantAfford,
    /// To one Player, what they tried to use is refused.
    Refused,
    /// A Door is open.
    Door(u16),
    /// The Power Switch has been thrown.
    SwitchThrown(u16),
    /// The Power is on.
    PowerOn,
    /// A Barrier has this many Planks.
    Planks { barrier: u8, planks: u8 },
    Box { id: u16, location: u8, phase: BoxPhase },
    /// Pack-a-Punch holds this, or nothing.
    Machine { id: u16, inside: Option<Inside> },
    Trap { id: u16, phase: TrapPhase },
    Teleporter(TeleporterView),
    /// A Power-up lies on the ground.
    DropAppeared { id: u32, power_up: PowerUp, at: [f32; 3] },
    /// A Power-up on the ground is taken, or has gone.
    DropGone { id: u32 },
    /// A Power-up was taken by the Player in `taker`, if a Player did; a timed one lasts
    /// `extra_secs` longer for their Boons.
    PowerUp { power_up: PowerUp, taker: Option<Seat>, extra_secs: f32 },
    /// Round `number` has begun.
    Round { number: u32, hellhound: Option<u32> },
    /// The Round's Horde is dead.
    RoundOver,
    /// To one Player, what the Host did to their Weapons and Equipment.
    Hands { ops: Vec<HandOp>, held: u8, gear: Option<Gear> },
    /// A Player is drinking this Perk, and holds these.
    Perks { seat: Seat, drinking: Option<Perk>, held: Vec<Perk> },
    /// A Player has this Weapon in hand.
    Held { seat: Seat, weapon: HeldWeapon },
    /// A Player is Down, is waiting to Respawn, is Reviving, has Pause open or has an Offer open.
    Status { seat: Seat, status: Status },
    /// To one Player, how much Health they have of how much.
    Health { current: f32, max: f32 },
    /// To one Player, their Offer: the Round it follows, whether its cards are Pacts, and their ids.
    Offer(OfferWire),
    /// To one Player, their Offer is closed (picked, skipped, or lost to a Bleed Out).
    OfferClosed,
    /// To one Player, the Boons they hold, with their Stacks.
    Boons(Vec<(String, u32)>),
    /// The Pacts held this Run, with their holders' Seats.
    Pacts(Vec<(String, u8)>),
    /// The Player in this Seat has left the Run for good.
    Left { seat: Seat },
}

/// What has come in, and what each end remembers of what it has said or been told.
#[derive(Resource, Default)]
pub struct Mirror {
    requests: Vec<(Peer, Request)>,
    events: Vec<WorldEvent>,
    // The Host's: what each machine was when last told.
    doors: HashSet<u16>,
    switches: HashSet<u16>,
    power: bool,
    planks: HashMap<u8, u8>,
    boxes: HashMap<u16, (u8, BoxPhase)>,
    machines: HashMap<u16, Option<(WeaponId, Option<Seat>)>>,
    traps: HashMap<u16, std::mem::Discriminant<TrapPhase>>,
    teleporter: Option<TeleporterView>,
    drops: HashMap<Entity, u32>,
    next_drop: u32,
    round: Option<(u32, Option<u32>)>,
    intermission: bool,
    perks: HashMap<Seat, (Option<Perk>, Vec<Perk>)>,
    held: HashMap<Seat, HeldWeapon>,
    status: HashMap<Seat, Status>,
    health: HashMap<Seat, f32>,
    offers: HashMap<Seat, Option<OfferWire>>,
    boons: HashMap<Seat, Vec<(String, u32)>>,
    pacts: Vec<(String, u8)>,
    /// Revive holds heard, by reviver: whom they hold Use over, and how long they still count.
    holds: HashMap<Seat, (Seat, f32)>,
    // A client's: the Power-ups lying about, and the slot it last said it holds.
    shown_drops: HashMap<u32, Entity>,
    teleporter_shown: Option<TeleporterView>,
    reported_held: Option<usize>,
    reported_paused: bool,
}

impl Mirror {
    pub fn hear_player(&mut self, peer: Peer, request: Request) {
        self.requests.push((peer, request));
    }

    pub fn hear_host(&mut self, event: WorldEvent) {
        self.events.push(event);
    }
}

/// What a client hears from the Host, once the world it is about is there to be changed.
#[derive(Message)]
struct Heard(WorldEvent);

/// Says things to the Players.
#[derive(SystemParam)]
struct Tell<'w> {
    wire: ResMut<'w, Wire>,
    roster: Res<'w, Roster>,
}

impl Tell<'_> {
    fn to(&mut self, peer: Peer, event: &WorldEvent) {
        self.wire.0.send(peer, encode(&ToPlayer::Event(event.clone())));
    }

    /// Everyone but `except`, if they are a remote Player.
    fn all_but(&mut self, except: Option<Peer>, event: &WorldEvent) {
        let bytes = encode(&ToPlayer::Event(event.clone()));
        for &(peer, _) in &self.roster.0 {
            if Some(peer) != except {
                self.wire.0.send(peer, bytes.clone());
            }
        }
    }

    fn all(&mut self, event: &WorldEvent) {
        self.all_but(None, event);
    }
}

pub fn add(app: &mut App) {
    // What the systems below read and write exists in a Run's app already; saying so again here
    // keeps them valid in an app that has only some of its machines.
    app.add_message::<Heard>()
        .add_message::<RefusedByPlayer>()
        .add_message::<UseRequest>()
        .add_message::<Purchased>()
        .add_message::<HoldPurchased>()
        .add_message::<CantAfford>()
        .add_message::<BuyRefused>()
        .add_message::<PlankTorn>()
        .add_message::<PlankRebuilt>()
        .add_message::<PowerUpTaken>()
        .add_message::<RoundStarted>()
        .add_message::<RoundEnded>()
        .add_message::<PlayCue>()
        .add_message::<PlayerHit>()
        .add_message::<PickOffer>()
        .add_message::<PlayerLeft>()
        .add_message::<WentDown>()
        .add_message::<Revived>()
        .add_message::<Respawned>()
        .init_resource::<Power>()
        .init_resource::<Barriers>()
        .init_resource::<ActivePowerUps>()
        .init_resource::<crate::teleporter::Teleporter>()
        .add_systems(Update, name_machines.run_if(resource_exists::<NetIds>).run_if(in_state(GameState::Playing))
                .run_if(world_runs))
        .add_systems(
            Update,
            (hear_requests.before(HostSimulation), hold_revives.before(HostSimulation), show_remote_held)
                .after(name_machines)
                .chain()
                .run_if(resource_exists::<Roster>)
                .run_if(resource_exists::<NetIds>)
                .run_if(in_state(GameState::Playing))
                .run_if(world_runs),
        )
        .add_systems(
            Update,
            (
                answer_requests,
                tell_doors,
                tell_planks,
                tell_boxes,
                tell_machines,
                tell_traps,
                tell_teleporter,
                tell_drops,
                tell_rounds,
                tell_hands,
                tell_perks,
                tell_held,
                tell_status,
                tell_health,
                tell_offers,
                tell_gauntlet,
                tell_left,
            )
                .after(HostSimulation)
                .after(hear_requests)
                .run_if(resource_exists::<Roster>)
                .run_if(resource_exists::<NetIds>)
                .run_if(in_state(GameState::Playing))
                .run_if(world_runs),
        )
        .add_systems(
            Update,
            (
                release_events,
                (
                    hear_answers,
                    hear_doors,
                    hear_planks,
                    hear_boxes,
                    hear_machines,
                    hear_traps,
                    hear_teleporter,
                    hear_drops,
                    hear_rounds,
                    hear_hands,
                    hear_perks,
                    hear_held,
                    hear_status,
                    hear_health,
                    hear_offers,
                    hear_gauntlet,
                    hear_left,
                ),
            )
                .chain()
                .after(name_machines)
                .run_if(resource_exists::<Prediction>)
                .run_if(resource_exists::<NetIds>)
                .run_if(in_state(GameState::Playing))
                .run_if(world_runs),
        )
        .add_systems(
            Update,
            (
                forward_requests,
                forward_picks,
                report_switches,
                report_paused,
                run_shown_clocks.run_if(in_state(Authority::Client)),
            )
                .run_if(resource_exists::<Prediction>)
                .run_if(resource_exists::<NetIds>)
                .run_if(in_state(GameState::Playing))
                .run_if(world_runs),
        );
}

/// Machines that can be used or told about.
type Machine = Or<(
    With<Buyable>,
    With<Door>,
    With<PowerSwitch>,
    With<Barrier>,
    With<MysteryBox>,
    With<PackAPunch>,
    With<Trap>,
    With<TrapSwitch>,
    With<PerkMachine>,
    With<Pad>,
    With<Mainframe>,
)>;

/// Numbers every machine not yet numbered by where it stands, so every machine numbers them alike.
fn name_machines(
    mut commands: Commands,
    mut ids: ResMut<NetIds>,
    machines: Query<(Entity, Option<&Transform>), (Without<NetId>, Machine)>,
) {
    let mut unnamed: Vec<(IVec3, Entity)> = machines
        .iter()
        .map(|(entity, transform)| {
            let at = transform.map_or(Vec3::ZERO, |transform| transform.translation);
            ((at * 100.0).round().as_ivec3(), entity)
        })
        .collect();
    unnamed.sort_by_key(|&(at, entity)| (at.x, at.y, at.z, entity));
    for (_, entity) in unnamed {
        let id = ids.next;
        ids.next += 1;
        ids.by_id.insert(id, entity);
        commands.entity(entity).insert(NetId(id));
    }
}

// The Host

/// Turns what the Players asked into what the Host does: a Use becomes a `UseRequest` from the
/// Player, a switch of Weapon is taken in hand on their copy of the Loadout, a Revive hold is kept
/// (see `hold_revives`), a Pause flag is set on their Player, and a pick is theirs of their Offer.
/// A Player with an Offer open asks nothing of the world.
fn hear_requests(
    mut commands: Commands,
    mut mirror: ResMut<Mirror>,
    roster: Res<Roster>,
    ids: Res<NetIds>,
    seats: Query<(Entity, &PlayerSeat, Has<Choosing>), With<Player>>,
    mut loadouts: Query<&mut Loadout>,
    mut uses: MessageWriter<UseRequest>,
    mut claims: MessageWriter<RefusedByPlayer>,
    mut picks: MessageWriter<PickOffer>,
) {
    for (peer, request) in std::mem::take(&mut mirror.requests) {
        let Some(&(_, seat)) = roster.0.iter().find(|(held, _)| *held == peer) else { continue };
        let Some((player, _, choosing)) = seats.iter().find(|(_, held, _)| held.0 == seat) else { continue };
        match request {
            Request::Use { buyable, hold, refused } => {
                let Some(buyable) = ids.entity(buyable).filter(|_| !choosing) else { continue };
                uses.write(UseRequest { player, buyable, hold });
                if refused {
                    claims.write(RefusedByPlayer { player, buyable });
                }
            }
            Request::Held { slot } => {
                if let Ok(mut loadout) = loadouts.get_mut(player) {
                    loadout.set_held(usize::from(slot));
                }
            }
            Request::Revive { target } => {
                if !choosing {
                    mirror.holds.insert(seat, (target, REVIVE_HOLD_GRACE_SECS));
                }
            }
            Request::Paused(paused) => {
                if paused {
                    commands.entity(player).insert(Paused);
                } else {
                    commands.entity(player).remove::<Paused>();
                }
            }
            Request::Pick { card } => {
                picks.write(PickOffer { player, card: card.map(usize::from) });
            }
        }
    }
}

/// Keeps a client's Revive hold going between the frames it is heard in: each frame it still counts,
/// the Host's own `revive` is asked as if the reviver held Use. It lapses when the words stop.
fn hold_revives(
    mut mirror: ResMut<Mirror>,
    players: Query<(Entity, &PlayerSeat), With<Player>>,
    mut uses: MessageWriter<UseRequest>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    mirror.holds.retain(|&reviver, (target, left)| {
        let entity = |seat: Seat| players.iter().find(|(_, held)| held.0 == seat).map(|(entity, _)| entity);
        if let (Some(player), Some(buyable)) = (entity(reviver), entity(*target)) {
            uses.write(UseRequest { player, buyable, hold: true });
        }
        *left -= dt;
        *left > 0.0
    });
}

/// A remote Player's Weapon in hand, as the Host's copy of their Loadout has it.
fn show_remote_held(mut players: Query<(&Loadout, &mut HeldWeapon), With<Remote>>) {
    for (loadout, mut held) in &mut players {
        let now = HeldWeapon::of(loadout.held());
        if *held != now {
            *held = now;
        }
    }
}

/// Tells a remote Player how their Use went.
fn answer_requests(
    mut tell: Tell,
    mut bought: MessageReader<Purchased>,
    mut held: MessageReader<HoldPurchased>,
    mut cant_afford: MessageReader<CantAfford>,
    mut refused: MessageReader<BuyRefused>,
    remotes: Query<&Remote>,
    ids: Query<&NetId>,
) {
    let peer = |player| remotes.get(player).ok().map(|remote| remote.peer);
    let taps = bought.read().map(|bought| (bought.player, bought.buyable, false));
    let holds = held.read().map(|bought| (bought.player, bought.buyable, true));
    for (player, buyable, hold) in taps.chain(holds) {
        if let (Some(peer), Ok(id)) = (peer(player), ids.get(buyable)) {
            tell.to(peer, &WorldEvent::Bought { buyable: id.0, hold });
        }
    }
    for tried in cant_afford.read() {
        if let Some(peer) = peer(tried.player) {
            tell.to(peer, &WorldEvent::CantAfford);
        }
    }
    for tried in refused.read() {
        if let Some(peer) = peer(tried.player) {
            tell.to(peer, &WorldEvent::Refused);
        }
    }
}

fn tell_doors(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    doors: Query<(&NetId, &Door)>,
    switches: Query<(&NetId, &PowerSwitch)>,
    power: Res<Power>,
) {
    for (id, door) in &doors {
        if door.open && mirror.doors.insert(id.0) {
            tell.all(&WorldEvent::Door(id.0));
        }
    }
    for (id, switch) in &switches {
        if switch.started() && mirror.switches.insert(id.0) {
            tell.all(&WorldEvent::SwitchThrown(id.0));
        }
    }
    if power.is_on() && !mirror.power {
        mirror.power = true;
        tell.all(&WorldEvent::PowerOn);
    }
}

fn tell_planks(mut tell: Tell, mut mirror: ResMut<Mirror>, barriers: Res<Barriers>) {
    for barrier in 0..crate::map::WINDOWS.len() as u8 {
        let planks = barriers.planks(BarrierId(barrier));
        let told = mirror.planks.get(&barrier).copied().unwrap_or(BARRIER_PLANKS);
        if planks != told {
            mirror.planks.insert(barrier, planks);
            tell.all(&WorldEvent::Planks { barrier, planks });
        }
    }
}

fn tell_boxes(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    boxes: Query<(&NetId, &MysteryBox)>,
    seats: Query<&PlayerSeat>,
) {
    let seat = |player| seats.get(player).map_or(0, |seat| seat.0);
    for (id, mystery_box) in &boxes {
        let phase = match mystery_box.state {
            BoxState::Idle => BoxPhase::Idle,
            BoxState::Spinning { spinner, .. } => BoxPhase::Spinning { spinner: seat(spinner) },
            BoxState::Offering { spinner, prize, upgraded, .. } => {
                BoxPhase::Offering { spinner: seat(spinner), prize, upgraded }
            }
            BoxState::Rearming { .. } => BoxPhase::Rearming,
            BoxState::Leaving { .. } => BoxPhase::Leaving,
            BoxState::Away { to, .. } => BoxPhase::Away { to: to as u8 },
        };
        let now = (mystery_box.location as u8, phase);
        // A Box is told of from the start, standing idle where it began.
        let told = mirror.boxes.get(&id.0).copied().unwrap_or((0, BoxPhase::Idle));
        if now != told {
            mirror.boxes.insert(id.0, now);
            tell.all(&WorldEvent::Box { id: id.0, location: now.0, phase });
        }
    }
}

fn tell_machines(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    machines: Query<(&NetId, &PackAPunch)>,
    seats: Query<&PlayerSeat>,
) {
    for (id, machine) in &machines {
        let seat_of = |owner: Option<Entity>| owner.and_then(|owner| seats.get(owner).ok()).map(|seat| seat.0);
        let inside = machine.view().map(|(weapon, elapsed, owner)| (weapon, elapsed, seat_of(owner)));
        let key = inside.as_ref().map(|&(weapon, _, owner)| (weapon.id, owner));
        if mirror.machines.get(&id.0).copied().unwrap_or(None) == key {
            continue;
        }
        mirror.machines.insert(id.0, key);
        let inside = inside.map(|(weapon, elapsed, owner)| Inside { weapon: CopyWire::of(weapon), elapsed, owner });
        tell.all(&WorldEvent::Machine { id: id.0, inside });
    }
}

fn tell_traps(mut tell: Tell, mut mirror: ResMut<Mirror>, traps: Query<(&NetId, &Trap)>) {
    for (id, trap) in &traps {
        let phase = trap.phase();
        let kind = std::mem::discriminant(&phase);
        let told = mirror.traps.get(&id.0).copied().unwrap_or(std::mem::discriminant(&TrapPhase::Idle));
        if kind != told {
            mirror.traps.insert(id.0, kind);
            tell.all(&WorldEvent::Trap { id: id.0, phase });
        }
    }
}

fn tell_teleporter(mut tell: Tell, mut mirror: ResMut<Mirror>, teleporter: Res<crate::teleporter::Teleporter>) {
    let view = teleporter.view();
    // The Run starts with nothing Linked and nothing under way.
    let quiet = TeleporterView { linked: [false; 2], linking: None, trip: None, sabotaged: false };
    if mirror.teleporter.unwrap_or(quiet) != view {
        mirror.teleporter = Some(view);
        tell.all(&WorldEvent::Teleporter(view));
    }
}

/// Power-ups lying on the ground, and being taken.
fn tell_drops(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    drops: Query<(Entity, &Dropped)>,
    mut gone: RemovedComponents<Dropped>,
    mut taken: MessageReader<PowerUpTaken>,
    players: Query<(&PlayerSeat, &Modifiers), With<Player>>,
) {
    for (entity, dropped) in &drops {
        if mirror.drops.contains_key(&entity) {
            continue;
        }
        let id = mirror.next_drop;
        mirror.next_drop += 1;
        mirror.drops.insert(entity, id);
        tell.all(&WorldEvent::DropAppeared { id, power_up: dropped.power_up(), at: dropped.at().to_array() });
    }
    for entity in gone.read() {
        if let Some(id) = mirror.drops.remove(&entity) {
            tell.all(&WorldEvent::DropGone { id });
        }
    }
    for &PowerUpTaken(power_up, taker) in taken.read() {
        let taker = taker.and_then(|taker| players.get(taker).ok());
        let extra_secs = taker.map_or(0.0, |(_, modifiers)| resolve(Stat::PowerUpDuration, 0.0, &modifiers.0));
        tell.all(&WorldEvent::PowerUp { power_up, taker: taker.map(|(seat, _)| seat.0), extra_secs });
    }
}

fn tell_rounds(mut tell: Tell, mut mirror: ResMut<Mirror>, round: Option<Res<Round>>) {
    let Some(round) = round else { return };
    let now = (round.number, round.hellhound);
    if mirror.round != Some(now) {
        mirror.round = Some(now);
        mirror.intermission = false;
        tell.all(&WorldEvent::Round { number: now.0, hellhound: now.1 });
    }
    let resting = matches!(round.phase, RoundPhase::Intermission(_));
    if resting && !mirror.intermission {
        tell.all(&WorldEvent::RoundOver);
    }
    mirror.intermission = resting;
}

/// What the Host did to each remote Player's Weapons and Equipment since it last said.
fn tell_hands(
    mut tell: Tell,
    mut players: Query<(&mut Loadout, Option<&mut Equipment>, Option<&Knife>, Option<&Remote>), With<Player>>,
) {
    for (mut loadout, equipment, knife, remote) in &mut players {
        let ops = loadout.drain_host_ops();
        let gear = equipment.and_then(|mut equipment| {
            std::mem::take(&mut equipment.host_dirty).then(|| Gear {
                frags: equipment.frags,
                claymores_owned: equipment.claymores_owned,
                claymores: equipment.claymores,
                monkey_bombs_owned: equipment.monkey_bombs_owned,
                monkey_bombs: equipment.monkey_bombs,
                bowie: knife.is_some_and(|knife| knife.bowie),
            })
        });
        let Some(remote) = remote else { continue };
        if ops.is_empty() && gear.is_none() {
            continue;
        }
        let ops = ops
            .into_iter()
            .filter_map(|op| {
                Some(match op {
                    HostOp::Put(slot) => HandOp::Put { slot: slot as u8, copy: CopyWire::of(loadout.slot(slot)?) },
                    HostOp::Take => HandOp::Take,
                    HostOp::Refill(slot) => HandOp::Refill(slot as u8),
                    HostOp::MaxAmmo => HandOp::MaxAmmo,
                    HostOp::Slots(count) => HandOp::Slots(count as u8),
                    HostOp::Replace => HandOp::Replace {
                        slots: loadout.slots().iter().map(|slot| slot.as_ref().map(CopyWire::of)).collect(),
                        held: loadout.held_slot() as u8,
                    },
                })
            })
            .collect();
        tell.to(remote.peer, &WorldEvent::Hands { ops, held: loadout.held_slot() as u8, gear });
    }
}

fn tell_perks(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    players: Query<(&PlayerSeat, &Perks, Option<&Drinking>), With<Player>>,
) {
    for (seat, perks, drinking) in &players {
        let now = (drinking.map(|drinking| drinking.perk), perks.0.clone());
        let told = mirror.perks.get(&seat.0).cloned().unwrap_or((None, Vec::new()));
        if told != now {
            mirror.perks.insert(seat.0, now.clone());
            tell.all(&WorldEvent::Perks { seat: seat.0, drinking: now.0, held: now.1 });
        }
    }
}

/// Each Player's Weapon in hand, to everyone but the Player, whose game knows.
fn tell_held(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    players: Query<(&PlayerSeat, &HeldWeapon, Option<&Remote>), With<Player>>,
) {
    for (seat, held, remote) in &players {
        if mirror.held.get(&seat.0) == Some(held) {
            continue;
        }
        mirror.held.insert(seat.0, held.clone());
        tell.all_but(remote.map(|remote| remote.peer), &WorldEvent::Held { seat: seat.0, weapon: held.clone() });
    }
}

/// What each Player is going through, to everyone, when it changes.
fn tell_status(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    players: Query<
        (&PlayerSeat, Option<&LastStand>, Has<AwaitingRespawn>, Option<&Reviving>, Has<Paused>, Has<Choosing>),
        With<Player>,
    >,
    seats: Query<&PlayerSeat>,
) {
    for (seat, last_stand, awaiting, reviving, paused, choosing) in &players {
        let status = Status {
            down: last_stand.map(|stand| Down { left: stand.remaining_secs(), coop: stand.is_coop() }),
            awaiting,
            reviving: reviving.and_then(|reviving| {
                let (elapsed, needed) = reviving.timing();
                Some(Revive { target: seats.get(reviving.target).ok()?.0, elapsed, needed })
            }),
            paused,
            choosing,
        };
        if mirror.status.get(&seat.0).is_some_and(|told| told.same(&status))
            || (!mirror.status.contains_key(&seat.0) && status.same(&Status::default()))
        {
            continue;
        }
        mirror.status.insert(seat.0, status);
        tell.all(&WorldEvent::Status { seat: seat.0, status });
    }
}

/// Each remote Player's Health, to them: at once when it falls, and as it regenerates in steps.
fn tell_health(mut tell: Tell, mut mirror: ResMut<Mirror>, players: Query<(&PlayerSeat, &Health, &Remote), With<Player>>) {
    for (seat, health, remote) in &players {
        let told = mirror.health.get(&seat.0).copied().unwrap_or(PLAYER_MAX_HEALTH);
        let now = health.current;
        let full = now >= health.max && told < health.max;
        if now < told || (now - told).abs() >= HEALTH_TELL_STEP || full {
            mirror.health.insert(seat.0, now);
            tell.to(remote.peer, &WorldEvent::Health { current: now, max: health.max });
        }
    }
}

/// Each remote Player's own Offer, when it opens, and when it closes.
fn tell_offers(mut tell: Tell, mut mirror: ResMut<Mirror>, players: Query<(&PlayerSeat, Option<&Offer>, &Remote), With<Player>>) {
    for (seat, offer, remote) in &players {
        let now = offer.map(Offer::wire);
        let told = mirror.offers.get(&seat.0).cloned().unwrap_or(None);
        if told == now {
            continue;
        }
        mirror.offers.insert(seat.0, now.clone());
        match now {
            Some(wire) => tell.to(remote.peer, &WorldEvent::Offer(wire)),
            None => tell.to(remote.peer, &WorldEvent::OfferClosed),
        }
    }
}

/// Each remote Player's Boons, to them, and the Pacts held, to everyone, when they change.
fn tell_gauntlet(
    mut tell: Tell,
    mut mirror: ResMut<Mirror>,
    players: Query<(&PlayerSeat, &HeldBoons, &Remote), With<Player>>,
    pacts: Option<Res<HeldPacts>>,
) {
    for (seat, held, remote) in &players {
        let now = boons_wire(held);
        if mirror.boons.get(&seat.0).map_or(!now.is_empty(), |told| *told != now) {
            mirror.boons.insert(seat.0, now.clone());
            tell.to(remote.peer, &WorldEvent::Boons(now));
        }
    }
    if let Some(pacts) = pacts {
        let now = pacts_wire(&pacts);
        if mirror.pacts != now {
            mirror.pacts = now.clone();
            tell.all(&WorldEvent::Pacts(now));
        }
    }
}

/// A Player who left the Run, to everyone still in it.
fn tell_left(mut tell: Tell, mut left: MessageReader<PlayerLeft>) {
    for &PlayerLeft { seat } in left.read() {
        tell.all(&WorldEvent::Left { seat: seat.0 });
    }
}

// A client

/// Hands what the Host said to the systems that show it, once there is a world to show it in.
fn release_events(mut mirror: ResMut<Mirror>, ids: Res<NetIds>, mut heard: MessageWriter<Heard>) {
    if ids.next == 0 {
        return;
    }
    for event in std::mem::take(&mut mirror.events) {
        heard.write(Heard(event));
    }
}

/// Sends the Host a Use the `LocalPlayer` made, or a Revive they hold: that one names a Player.
fn forward_requests(
    mut requests: MessageReader<UseRequest>,
    local: Query<Entity, With<LocalPlayer>>,
    buyables: Query<(&NetId, Has<Refused>)>,
    teammates: Query<&PlayerSeat, With<Player>>,
    mut wire: ResMut<Wire>,
) {
    let Ok(local) = local.single() else { return };
    for request in requests.read() {
        if request.player != local {
            continue;
        }
        let message = match (buyables.get(request.buyable), teammates.get(request.buyable)) {
            (Ok((id, refused)), _) => Request::Use { buyable: id.0, hold: request.hold, refused },
            (_, Ok(seat)) if request.hold => Request::Revive { target: seat.0 },
            // Anything else is no Buyable here.
            _ => continue,
        };
        wire.0.send(HOST, encode(&ToHost::Request(message)));
    }
}

/// Sends the Host the card the `LocalPlayer` picked of their Offer.
fn forward_picks(mut picks: MessageReader<PickOffer>, local: Query<Entity, With<LocalPlayer>>, mut wire: ResMut<Wire>) {
    let Ok(local) = local.single() else { return };
    for pick in picks.read().filter(|pick| pick.player == local) {
        let card = pick.card.map(|card| card as u8);
        wire.0.send(HOST, encode(&ToHost::Request(Request::Pick { card })));
    }
}

/// Tells the Host when the `LocalPlayer`'s Pause menu opens or closes.
fn report_paused(mut mirror: ResMut<Mirror>, local: Query<Has<Paused>, With<LocalPlayer>>, mut wire: ResMut<Wire>) {
    let Ok(paused) = local.single() else { return };
    if mirror.reported_paused != paused {
        mirror.reported_paused = paused;
        wire.0.send(HOST, encode(&ToHost::Request(Request::Paused(paused))));
    }
}

/// Tells the Host when the `LocalPlayer` takes another Weapon in hand.
fn report_switches(
    mut mirror: ResMut<Mirror>,
    local: Query<&Loadout, With<LocalPlayer>>,
    mut wire: ResMut<Wire>,
) {
    let Ok(loadout) = local.single() else { return };
    let held = loadout.held_slot();
    if mirror.reported_held != Some(held) {
        // The first look is the starting Weapon, which the Host has.
        if mirror.reported_held.is_some() {
            wire.0.send(HOST, encode(&ToHost::Request(Request::Held { slot: held as u8 })));
        }
        mirror.reported_held = Some(held);
    }
}

/// Runs the clocks of what the Host has said, between its words, so that it plays out smoothly.
fn run_shown_clocks(
    time: Res<Time>,
    mut boxes: Query<&mut MysteryBox>,
    mut machines: Query<&mut PackAPunch>,
    mut traps: Query<&mut Trap>,
    mut teleporter: ResMut<crate::teleporter::Teleporter>,
    mut active: ResMut<ActivePowerUps>,
    mut points: Query<&mut Points, With<Player>>,
    mut round: Option<ResMut<Round>>,
    mut drinking: Query<&mut Drinking>,
    mut last_stands: Query<&mut LastStand>,
    mut revives: Query<&mut Reviving>,
) {
    let dt = time.delta_secs();
    for mut mystery_box in &mut boxes {
        mystery_box.tick_shown(dt);
    }
    for mut machine in &mut machines {
        machine.advance(dt);
    }
    for mut trap in &mut traps {
        trap.advance_shown(dt);
    }
    teleporter.advance(dt);
    active.tick(dt);
    let double = active.active(PowerUp::DoublePoints);
    for mut points in &mut points {
        if points.double != double {
            points.double = double;
        }
    }
    if let Some(round) = round.as_mut()
        && let RoundPhase::Intermission(left) = round.phase
    {
        round.phase = RoundPhase::Intermission((left - dt).max(0.001));
    }
    for mut drinking in &mut drinking {
        drinking.advance(dt);
    }
    for mut stand in &mut last_stands {
        stand.advance_shown(dt);
    }
    for mut reviving in &mut revives {
        reviving.advance_shown(dt);
    }
}

fn hear_answers(
    mut heard: MessageReader<Heard>,
    local: Query<Entity, With<LocalPlayer>>,
    ids: Res<NetIds>,
    mut bought: MessageWriter<Purchased>,
    mut held: MessageWriter<HoldPurchased>,
    mut cant_afford: MessageWriter<CantAfford>,
    mut refused: MessageWriter<BuyRefused>,
) {
    let Ok(player) = local.single() else { return };
    for Heard(event) in heard.read() {
        match event {
            WorldEvent::Bought { buyable, hold } => {
                let Some(buyable) = ids.entity(*buyable) else { continue };
                if *hold {
                    held.write(HoldPurchased { player, buyable });
                } else {
                    bought.write(Purchased { player, buyable });
                }
            }
            WorldEvent::CantAfford => {
                cant_afford.write(CantAfford { player });
            }
            WorldEvent::Refused => {
                refused.write(BuyRefused { player });
            }
            _ => {}
        }
    }
}

fn hear_doors(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    ids: Res<NetIds>,
    mut doors: Query<(&mut Door, &Transform)>,
    mut switches: Query<&mut PowerSwitch>,
    mut power: ResMut<Power>,
) {
    for Heard(event) in heard.read() {
        match event {
            WorldEvent::Door(id) => {
                let Some(entity) = ids.entity(*id) else { continue };
                if let Ok((mut door, transform)) = doors.get_mut(entity)
                    && !door.open
                {
                    open(&mut commands, entity, &mut door, transform);
                }
            }
            WorldEvent::SwitchThrown(id) => {
                let Some(entity) = ids.entity(*id) else { continue };
                if let Ok(mut switch) = switches.get_mut(entity) {
                    switch.start();
                    commands.entity(entity).remove::<Buyable>();
                }
            }
            WorldEvent::PowerOn => power.switch_on(),
            _ => {}
        }
    }
}

fn hear_planks(
    mut heard: MessageReader<Heard>,
    mut barriers: ResMut<Barriers>,
    mut torn: MessageWriter<PlankTorn>,
    mut rebuilt: MessageWriter<PlankRebuilt>,
) {
    for Heard(event) in heard.read() {
        let WorldEvent::Planks { barrier, planks } = *event else { continue };
        let barrier = BarrierId(barrier);
        let way = barriers.show(barrier, planks);
        let (tore, rebuilds) = plank_news(barrier, way, planks);
        if let Some(tore) = tore {
            torn.write(tore);
        }
        if let Some(rebuilds) = rebuilds {
            rebuilt.write(rebuilds);
        }
    }
}

fn hear_boxes(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    ids: Res<NetIds>,
    mut boxes: Query<(&mut MysteryBox, &mut Transform, &mut Visibility)>,
    seats: Query<(Entity, &PlayerSeat), With<Player>>,
) {
    let player = |seat: Seat| seats.iter().find(|(_, held)| held.0 == seat).map(|(entity, _)| entity);
    for Heard(event) in heard.read() {
        let WorldEvent::Box { id, location, phase } = *event else { continue };
        let Some(entity) = ids.entity(id) else { continue };
        let Ok((mut mystery_box, mut transform, mut visibility)) = boxes.get_mut(entity) else { continue };
        let teddy = mystery_box.teddy;
        let state = match phase {
            BoxPhase::Idle => BoxState::Idle,
            BoxPhase::Spinning { spinner } => match player(spinner) {
                Some(spinner) => BoxState::Spinning { spinner, elapsed: 0.0, paid: 0, teddy },
                None => BoxState::Idle,
            },
            BoxPhase::Offering { spinner, prize, upgraded } => match player(spinner) {
                Some(spinner) => {
                    BoxState::Offering { spinner, prize, left: BOX_PICKUP_SECS, counts: teddy, upgraded }
                }
                None => BoxState::Idle,
            },
            BoxPhase::Rearming => BoxState::Rearming { left: BOX_REARM_SECS },
            BoxPhase::Leaving => BoxState::Leaving { elapsed: 0.0 },
            BoxPhase::Away { to } => BoxState::Away { left: BOX_MOVE_SECS, to: usize::from(to) },
        };
        mystery_box.show(&mut commands, entity, &mut transform, &mut visibility, usize::from(location), state);
    }
}

fn hear_machines(
    mut heard: MessageReader<Heard>,
    ids: Res<NetIds>,
    mut machines: Query<&mut PackAPunch>,
    seats: Query<(Entity, &PlayerSeat), With<Player>>,
) {
    let player = |seat: Seat| seats.iter().find(|(_, held)| held.0 == seat).map(|(entity, _)| entity);
    for Heard(event) in heard.read() {
        let WorldEvent::Machine { id, inside } = event else { continue };
        let Some(entity) = ids.entity(*id) else { continue };
        if let Ok(mut machine) = machines.get_mut(entity) {
            machine.show(inside.as_ref().map(|inside| {
                (inside.weapon.build(), inside.elapsed, inside.owner.and_then(player))
            }));
        }
    }
}

fn hear_traps(mut heard: MessageReader<Heard>, ids: Res<NetIds>, mut traps: Query<&mut Trap>) {
    for Heard(event) in heard.read() {
        let WorldEvent::Trap { id, phase } = *event else { continue };
        if let Some(entity) = ids.entity(id)
            && let Ok(mut trap) = traps.get_mut(entity)
        {
            trap.show(phase);
        }
    }
}

fn hear_teleporter(
    mut heard: MessageReader<Heard>,
    mut teleporter: ResMut<crate::teleporter::Teleporter>,
    mut cues: MessageWriter<PlayCue>,
) {
    for Heard(event) in heard.read() {
        let WorldEvent::Teleporter(view) = *event else { continue };
        let before = teleporter.view();
        teleporter.show(view);
        let mut cue = |cue, at| {
            cues.write(PlayCue { cue, at: Some(at) });
        };
        if before.linking.is_some() && view.linking.is_none() && view.linked != before.linked {
            cue(Cue::MainframeLink, mainframe_use_point());
        }
        match (before.trip, view.trip) {
            (None, Some((pad, false))) => cue(Cue::TeleporterCharge, pad.at()),
            (Some((_, false)), Some((pad, true))) => cue(Cue::TeleporterFire, pad.at()),
            (Some(_), None) => cue(Cue::TeleporterArrive, mainframe_use_point()),
            _ => {}
        }
    }
}

fn hear_drops(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    mut mirror: ResMut<Mirror>,
    assets: Option<Res<PowerUpAssets>>,
    mut active: ResMut<ActivePowerUps>,
    mut taken: MessageWriter<PowerUpTaken>,
    players: Query<(Entity, &PlayerSeat), With<Player>>,
) {
    for Heard(event) in heard.read() {
        match *event {
            WorldEvent::DropAppeared { id, power_up, at } => {
                if let Some(assets) = &assets {
                    let entity = spawn_drop(&mut commands, assets, power_up, Vec3::from_array(at));
                    mirror.shown_drops.insert(id, entity);
                }
            }
            WorldEvent::DropGone { id } => {
                if let Some(entity) = mirror.shown_drops.remove(&id) {
                    commands.entity(entity).try_despawn();
                }
            }
            WorldEvent::PowerUp { power_up, taker, extra_secs } => {
                active.start(power_up, extra_secs);
                let taker = taker.and_then(|taker| players.iter().find(|(_, seat)| seat.0 == taker)).map(|(entity, _)| entity);
                taken.write(PowerUpTaken(power_up, taker));
            }
            _ => {}
        }
    }
}

fn hear_rounds(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    mut round: Option<ResMut<Round>>,
    mut started: MessageWriter<RoundStarted>,
    mut ended: MessageWriter<RoundEnded>,
) {
    for Heard(event) in heard.read() {
        match *event {
            WorldEvent::Round { number, hellhound } => {
                commands.insert_resource(Round::shown(number, hellhound));
                started.write(RoundStarted);
            }
            WorldEvent::RoundOver => {
                if let Some(round) = round.as_mut() {
                    round.phase = RoundPhase::Intermission(INTERMISSION_SECS);
                }
                ended.write(RoundEnded);
            }
            _ => {}
        }
    }
}

/// The Host's changes to the `LocalPlayer`'s Weapons and Equipment.
fn hear_hands(
    mut heard: MessageReader<Heard>,
    mut mirror: ResMut<Mirror>,
    mut local: Query<(&mut Loadout, &Modifiers, &mut Equipment, &mut Knife), With<LocalPlayer>>,
) {
    let Ok((mut loadout, modifiers, mut equipment, mut knife)) = local.single_mut() else { return };
    for Heard(event) in heard.read() {
        let WorldEvent::Hands { ops, gear, .. } = event else { continue };
        for op in ops {
            match op {
                HandOp::Put { slot, copy } => {
                    let slot = usize::from(*slot);
                    if slot < loadout.slot_count() {
                        loadout.put_copy(slot, copy.build());
                    }
                }
                HandOp::Take => {
                    loadout.take_held();
                }
                HandOp::Refill(slot) => {
                    let slot = usize::from(*slot);
                    if loadout.slot(slot).is_some() {
                        loadout.refill(slot, &modifiers.0);
                    }
                }
                HandOp::MaxAmmo => loadout.max_ammo(),
                HandOp::Slots(count) => loadout.set_slot_count(usize::from(*count)),
                HandOp::Replace { slots, held } => {
                    loadout.show_all(slots.iter().map(|slot| slot.as_ref().map(CopyWire::build)).collect(), usize::from(*held));
                }
            }
        }
        loadout.drain_host_ops();
        // What the Host just did is what it has, not something to report back.
        mirror.reported_held = Some(loadout.held_slot());
        if let Some(gear) = gear {
            equipment.frags = equipment.frags.max(gear.frags);
            equipment.claymores_owned |= gear.claymores_owned;
            equipment.claymores = equipment.claymores.max(gear.claymores);
            equipment.monkey_bombs_owned |= gear.monkey_bombs_owned;
            equipment.monkey_bombs = equipment.monkey_bombs.max(gear.monkey_bombs);
            knife.bowie |= gear.bowie;
            equipment.host_dirty = false;
        }
    }
}

fn hear_perks(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerSeat, &mut Perks, &mut Modifiers, Has<Drinking>)>,
) {
    for Heard(event) in heard.read() {
        let WorldEvent::Perks { seat, drinking, held } = event else { continue };
        let Some((entity, _, mut perks, mut modifiers, drinking_now)) =
            players.iter_mut().find(|(_, held, ..)| held.0 == *seat)
        else {
            continue;
        };
        perks.show(held, &mut modifiers);
        match (drinking, drinking_now) {
            (Some(perk), false) => {
                commands.entity(entity).insert(Drinking::new(*perk));
            }
            (None, true) => {
                commands.entity(entity).remove::<Drinking>();
            }
            _ => {}
        }
    }
}

fn hear_held(
    mut heard: MessageReader<Heard>,
    mut players: Query<(&PlayerSeat, &mut HeldWeapon), Without<LocalPlayer>>,
) {
    for Heard(event) in heard.read() {
        let WorldEvent::Held { seat, weapon } = event else { continue };
        if let Some((_, mut held)) = players.iter_mut().find(|(held, _)| held.0 == *seat) {
            *held = weapon.clone();
        }
    }
}

/// What the Host says each Player is going through. The `LocalPlayer`'s own Pause and Offer are its
/// own to know; everything else is the Host's word.
fn hear_status(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerSeat, Has<LocalPlayer>, Has<LastStand>, Has<AwaitingRespawn>, Option<&Reviving>), With<Player>>,
    mut downs: MessageWriter<WentDown>,
    mut revived: MessageWriter<Revived>,
    mut respawned: MessageWriter<Respawned>,
) {
    let entity = |seat: Seat| players.iter().find(|(_, held, ..)| held.0 == seat).map(|(entity, ..)| entity);
    let statuses: Vec<(Seat, Status)> = heard
        .read()
        .filter_map(|Heard(event)| match event {
            WorldEvent::Status { seat, status } => Some((*seat, *status)),
            _ => None,
        })
        .collect();
    // Who is Reviving whom, before any of this changes it: a Revive is done when the Player is up
    // again and not for want of the time.
    let revivers: Vec<(Entity, Entity)> =
        players.iter().filter_map(|(entity, .., reviving)| Some((entity, reviving?.target))).collect();
    for (seat, status) in statuses {
        let Some((player, _, local, last_stand, awaiting, reviving)) =
            players.iter().find(|(_, held, ..)| held.0 == seat)
        else {
            continue;
        };
        let mut entity_commands = commands.entity(player);
        match (status.down, last_stand) {
            (Some(down), false) => {
                entity_commands.insert(LastStand::shown(down.left, down.coop));
                downs.write(WentDown { player, outcome: DownOutcome::LastStand });
            }
            (None, true) => {
                entity_commands.remove::<LastStand>();
                if !status.awaiting
                    && let Some(&(reviver, _)) = revivers.iter().find(|(_, target)| *target == player)
                {
                    revived.write(Revived { player, reviver });
                }
            }
            _ => {}
        }
        match (status.awaiting, awaiting) {
            (true, false) => {
                entity_commands.insert((AwaitingRespawn, ColliderDisabled));
            }
            (false, true) => {
                entity_commands.remove::<(AwaitingRespawn, ColliderDisabled)>();
                respawned.write(Respawned { player });
            }
            _ => {}
        }
        match (status.reviving.and_then(|revive| entity(revive.target).map(|target| (target, revive))), reviving) {
            (Some((target, revive)), held) if held.is_none_or(|held| held.target != target) => {
                entity_commands.insert(Reviving::shown(target, revive.elapsed, revive.needed));
            }
            (None, Some(_)) => {
                entity_commands.remove::<Reviving>();
            }
            _ => {}
        }
        if !local {
            if status.paused {
                entity_commands.insert(Paused);
            } else {
                entity_commands.remove::<Paused>();
            }
            if status.choosing {
                entity_commands.insert(Choosing);
            } else {
                entity_commands.remove::<Choosing>();
            }
        }
    }
}

/// The Host's word of the `LocalPlayer`'s Health. A fall is a hit, for the screen and the sound.
fn hear_health(
    mut heard: MessageReader<Heard>,
    mut local: Query<(Entity, &mut Health), With<LocalPlayer>>,
    mut hits: MessageWriter<PlayerHit>,
) {
    let Ok((player, mut health)) = local.single_mut() else { return };
    for Heard(event) in heard.read() {
        let WorldEvent::Health { current, max } = *event else { continue };
        if current < health.current {
            hits.write(PlayerHit { player, damage: health.current - current });
            health.since_hit = 0.0;
        }
        health.max = max;
        health.current = current;
    }
}

/// The `LocalPlayer`'s own Offer opening and closing, by the Host's word.
fn hear_offers(
    mut heard: MessageReader<Heard>,
    mut commands: Commands,
    local: Query<Entity, With<LocalPlayer>>,
    mut cues: MessageWriter<PlayCue>,
) {
    let Ok(player) = local.single() else { return };
    for Heard(event) in heard.read() {
        match event {
            WorldEvent::Offer(wire) => {
                let Some(offer) = Offer::from_wire(wire) else { continue };
                let cue = if wire.1 { Cue::PactOffer } else { Cue::BoonOffer };
                cues.write(PlayCue { cue, at: None });
                commands.entity(player).insert((offer, Choosing));
            }
            WorldEvent::OfferClosed => {
                commands.entity(player).remove::<(Offer, Choosing)>();
            }
            _ => {}
        }
    }
}

/// The Boons the `LocalPlayer` holds, and the Pacts held, by the Host's word.
fn hear_gauntlet(
    mut heard: MessageReader<Heard>,
    mut local: Query<&mut HeldBoons, With<LocalPlayer>>,
    pacts: Option<ResMut<HeldPacts>>,
) {
    let mut pacts = pacts;
    for Heard(event) in heard.read() {
        match event {
            WorldEvent::Boons(told) => {
                if let Ok(mut held) = local.single_mut() {
                    show_boons(&mut held, told);
                }
            }
            WorldEvent::Pacts(told) => {
                if let Some(held) = pacts.as_mut() {
                    show_pacts(held, told);
                }
            }
            _ => {}
        }
    }
}

/// A Player who left the Run is gone from this game too.
fn hear_left(mut heard: MessageReader<Heard>, mut commands: Commands, players: Query<(Entity, &PlayerSeat, &PlayerName), With<Player>>) {
    for Heard(event) in heard.read() {
        let WorldEvent::Left { seat } = *event else { continue };
        for (player, _, name) in players.iter().filter(|(_, held, _)| held.0 == seat) {
            info!("{} left", name.0);
            commands.entity(player).despawn();
        }
    }
}

#[cfg(test)]
mod downs;
#[cfg(test)]
mod shared;
#[cfg(test)]
mod tests;
