//! Pack-a-Punch: the machine that, once the Power is on, takes the Weapon in hand for
//! `PAP_PRICE`, works on it for `PAP_PROCESS_SECS`, then holds its Upgraded form for
//! `PAP_PICKUP_SECS`. A Weapon not taken in time is lost, with no refund.
//!
//! An Upgraded Weapon in hand is Repacked instead, through the same flow: a tap adds a random Ammo
//! Mod it lacks, and holding the Use key adds a random Attachment in a free mount. Each costs more
//! with every Repack of that copy, of either kind.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::maps::map::{MapId, STOREY};

use crate::weapons::ammo_mod::{ammo_mod_slots, repack_price};
use crate::weapons::arsenal::WeaponId;
use crate::weapons::attachment::attachment_repack_price;
use crate::authority::HostSimulation;
use crate::weapons::gun_model::GunModels;
use crate::economy::buy::{Buyable, HoldBuy, HoldPurchased, NoLocalPrompt, Offer, Offers, PricedFor, Purchased, Refused};
use crate::layers::GameLayer;
use crate::modifier::{Modifier, Modifiers, Stat, resolve_points};
use crate::economy::perk::Drinking;
use crate::players::player::{LocalPlayer, Player};
use crate::maps::power::Supply;
use crate::enemies::sapper::Sabotaged;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;
use crate::weapons::weapon::{Loadout, UPGRADED_PURPLE, WeaponCopy};

/// Where a Weapon inside the machine is up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Being worked on; it can't be taken yet.
    Processing,
    /// Waiting to be taken.
    Ready,
    /// Not taken in time, and gone.
    Lost,
}

/// The stage of a Weapon that has been inside the machine for `elapsed` seconds.
pub fn stage(elapsed: f32) -> Stage {
    if elapsed < PAP_PROCESS_SECS {
        Stage::Processing
    } else if elapsed < PAP_PROCESS_SECS + PAP_PICKUP_SECS {
        Stage::Ready
    } else {
        Stage::Lost
    }
}

/// How far out of the machine's slot a Weapon that has been inside for `elapsed` seconds is shown:
/// 1 out in the open, 0 fully inside and out of sight. It slides in as it's taken, stays inside
/// while it's worked on, then slides back out and stays out until it's taken or lost.
pub fn slot_slide(elapsed: f32) -> f32 {
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    match stage(elapsed) {
        Stage::Processing => 1.0 - smooth(elapsed / PAP_SLIDE_SECS),
        Stage::Ready => smooth((elapsed - PAP_PROCESS_SECS) / PAP_SLIDE_SECS),
        Stage::Lost => 0.0,
    }
}

/// What the machine's Prompt shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachinePrompt {
    /// Nothing: a Weapon is being worked on, the Player's hands are busy, or the Weapon in hand
    /// is Upgraded with no room for another Ammo Mod or Attachment.
    NoPrompt,
    /// "You must turn on the Power first", and the Use key does nothing.
    NeedsPower,
    /// The upgrade, at `PAP_PRICE`.
    Upgrade,
    /// The upgrade's Prompt, refused with the buzzer: the Weapon in hand is the Player's only
    /// one, and the held slot is always filled.
    OnlyWeapon,
    /// A Repack at its rising prices: the tap for an Ammo Mod, the hold for an Attachment, or
    /// both, whichever the Weapon has room for.
    Repack { tap: bool, hold: bool },
    /// The Repack's Prompt, refused with the buzzer, as for `OnlyWeapon`.
    RepackOnlyWeapon { tap: bool, hold: bool },
    /// Take the Upgraded Weapon, free.
    Take,
}

/// The Weapon in the Player's hand, as the machine sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub upgraded: bool,
    /// Whether it has room for another Ammo Mod.
    pub ammo_mod_room: bool,
    /// Whether it has a free mount for an Attachment.
    pub free_mount: bool,
    /// Whether the Player carries another Weapon to bring up while this one is inside.
    pub spare: bool,
}

/// The machine's Prompt, given the Power, the Weapon inside (if any), the Weapon in hand and
/// whether the Player's hands are free (not drinking a Perk).
pub fn machine_prompt(power_on: bool, inside: Option<Stage>, held: Held, hands_free: bool) -> MachinePrompt {
    match inside {
        Some(Stage::Ready) if hands_free => MachinePrompt::Take,
        Some(_) => MachinePrompt::NoPrompt,
        None if !power_on => MachinePrompt::NeedsPower,
        None if !hands_free => MachinePrompt::NoPrompt,
        None if held.upgraded => {
            let (tap, hold) = (held.ammo_mod_room, held.free_mount);
            if !tap && !hold {
                MachinePrompt::NoPrompt
            } else if held.spare {
                MachinePrompt::Repack { tap, hold }
            } else {
                MachinePrompt::RepackOnlyWeapon { tap, hold }
            }
        }
        None if !held.spare => MachinePrompt::OnlyWeapon,
        None => MachinePrompt::Upgrade,
    }
}

/// What the machine charges for a Pack-a-Punch or a Repack whose Classic price is `base`, after
/// the Player's stat entries.
pub fn machine_price(base: u64, player: &[Modifier]) -> u64 {
    resolve_points(Stat::PackAPunchPrice, base, player)
}

/// What the machine charges `loadout`'s Weapon in hand: the upgrade, or the Repack whose price
/// rose with that Weapon's own Repacks (the tap's, or the hold's), after the Player's entries.
pub fn price_for(loadout: &Loadout, player: &[Modifier], hold: bool) -> Option<u64> {
    let copy = loadout.held();
    let base = match (copy.upgraded, hold) {
        (false, _) => PAP_PRICE,
        (true, false) => repack_price(copy.repacks),
        (true, true) => attachment_repack_price(copy.repacks),
    };
    Some(machine_price(base, player))
}

/// What the machine does to a Weapon put inside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Work {
    Upgrade,
    /// A tap Repack, its Ammo Mod picked by the roll in [0, 1), room allowing on a Weapon that
    /// holds the given number of Ammo Mods.
    AmmoMod(f32, usize),
    /// A hold Repack, its mount and Attachment picked by the rolls in [0, 1).
    Attachment(f32, f32),
}

/// A Weapon inside the machine, already in its Upgraded form.
struct Inside {
    weapon: WeaponCopy,
    elapsed: f32,
    /// The Player who put it in, the only one who can take it out. `None` for a Weapon nobody
    /// owns, which anyone takes.
    owner: Option<Entity>,
}

/// A Weapon out of the machine and waiting to be taken.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waiting {
    pub weapon: &'static str,
    /// Who alone can take it; `None` if anyone can.
    pub owner: Option<Entity>,
    /// Seconds left before it's lost.
    pub left: f32,
}

/// The Pack-a-Punch machine. It holds one Weapon at a time.
#[derive(Component, Default)]
pub struct PackAPunch {
    inside: Option<Inside>,
    /// Players who just took a Weapon out, with the seconds left before the machine answers their
    /// Use again (see `PAP_REUSE_SECS`).
    cooling: Vec<(Entity, f32)>,
}

impl PackAPunch {
    /// The Weapon inside, if any, so the Box can count it as in play.
    pub fn inside(&self) -> Option<WeaponId> {
        self.inside.as_ref().map(|inside| inside.weapon.id)
    }

    /// The Weapon inside, how long it has been there and who put it in, for the Host to tell the
    /// other games.
    pub fn view(&self) -> Option<(&WeaponCopy, f32, Option<Entity>)> {
        self.inside.as_ref().map(|inside| (&inside.weapon, inside.elapsed, inside.owner))
    }

    /// Shows what the Host says is inside: a Weapon already worked on, `elapsed` seconds in.
    pub fn show(&mut self, inside: Option<(WeaponCopy, f32, Option<Entity>)>) {
        // A Weapon that vanishes while still Ready was taken by its owner, so their game starts the
        // same window the Host did, to keep the Prompt in step with the Host's judgement.
        let taken_by = self
            .inside
            .as_ref()
            .filter(|was| inside.is_none() && stage(was.elapsed) == Stage::Ready && was.elapsed < PAP_PROCESS_SECS + PAP_PICKUP_SECS - 0.25)
            .and_then(|was| was.owner);
        if let Some(owner) = taken_by {
            self.cool(owner);
        }
        self.inside = inside.map(|(weapon, elapsed, owner)| Inside { weapon, elapsed, owner });
    }

    /// Starts `player`'s window after taking a Weapon out: the machine ignores their Use meanwhile.
    pub fn cool(&mut self, player: Entity) {
        match self.cooling.iter_mut().find(|(held, _)| *held == player) {
            Some((_, left)) => *left = PAP_REUSE_SECS,
            None => self.cooling.push((player, PAP_REUSE_SECS)),
        }
    }

    /// Whether `player` took a Weapon out a moment ago, so the machine ignores their Use.
    pub fn is_cooling(&self, player: Entity) -> bool {
        self.cooling.iter().any(|&(held, _)| held == player)
    }

    /// Whether `player` may take the Weapon inside out.
    pub fn is_owned_by(&self, player: Entity) -> bool {
        self.inside.as_ref().is_some_and(|inside| inside.owner.is_none_or(|owner| owner == player))
    }

    /// The Weapon waiting to be taken, if it's out of the machine and not yet lost.
    pub fn waiting(&self) -> Option<Waiting> {
        let inside = self.inside.as_ref()?;
        (stage(inside.elapsed) == Stage::Ready).then(|| Waiting {
            weapon: inside.weapon.stats().name,
            owner: inside.owner,
            left: PAP_PROCESS_SECS + PAP_PICKUP_SECS - inside.elapsed,
        })
    }

    /// The stage of the Weapon inside, if any.
    pub fn stage(&self) -> Option<Stage> {
        self.inside.as_ref().map(|inside| stage(inside.elapsed))
    }

    /// The Weapon to show on the tray, how far out of the slot it is (see `slot_slide`) and its
    /// stage, if one is inside.
    fn on_show(&self) -> Option<(WeaponId, f32, Stage)> {
        let inside = self.inside.as_ref()?;
        Some((inside.weapon.id, slot_slide(inside.elapsed), stage(inside.elapsed)))
    }

    /// Take in a Weapon and start working on it. It comes out Upgraded, or Repacked with an Ammo
    /// Mod or an Attachment; either way with full ammo.
    pub fn insert(&mut self, weapon: WeaponCopy, work: Work) {
        debug_assert!(self.inside.is_none(), "one Weapon at a time");
        let weapon = match work {
            Work::Upgrade => weapon.upgrade(),
            Work::AmmoMod(roll, slots) => weapon.repack(roll, slots),
            Work::Attachment(mount_roll, attachment_roll) => weapon.repack_attachment(mount_roll, attachment_roll),
        };
        self.inside = Some(Inside { weapon, elapsed: 0.0, owner: None });
    }

    /// Take in a Weapon from `owner` (see `insert`), who alone takes it back out.
    pub fn insert_for(&mut self, owner: Entity, weapon: WeaponCopy, work: Work) {
        self.insert(weapon, work);
        if let Some(inside) = &mut self.inside {
            inside.owner = Some(owner);
        }
    }

    /// Advance the clock. A Weapon left past its pickup window is lost.
    pub fn advance(&mut self, dt: f32) {
        self.cooling.retain_mut(|(_, left)| {
            *left -= dt;
            *left > 0.0
        });
        if let Some(inside) = &mut self.inside {
            inside.elapsed += dt;
            if stage(inside.elapsed) == Stage::Lost {
                self.inside = None;
            }
        }
    }

    /// Hand over the Upgraded Weapon, once it's ready.
    pub fn take(&mut self) -> Option<WeaponCopy> {
        if self.stage() != Some(Stage::Ready) {
            return None;
        }
        self.inside.take().map(|inside| inside.weapon)
    }
}

/// The machine's spot on `map`, on the floor. It always faces +Z: on the Greybox, out of the
/// Gazebo's Door; on the Quarry, along the Headframe's deck from just behind its front rail;
/// on the Village, along the Church's chancel from its south end; on the Dam, up on the Control
/// Room's dais from its south end; on the Cove, on the Sea Cave's rock shelf from its south end; on the Polder, on the Sluice House's
/// winch platform from its south end; on the Sanatorium, on the Operating Theatre's floor from its
/// south end, where the operating table stood; on the Archive, on the Strong Room's reading plinth against its south wall, west of the vault door.
pub(crate) fn machine_at(map: MapId) -> Vec3 {
    match map {
        MapId::Greybox => Vec3::new(5.0, 0.0, -9.2),
        MapId::Quarry => Vec3::new(2.0, 3.0 * STOREY, 35.2),
        MapId::Village => Vec3::new(69.0, 0.45, 9.0),
        MapId::Dam => Vec3::new(0.0, STOREY + 0.45, -0.8),
        MapId::Cove => Vec3::new(-50.25, 0.6, -10.3),
        MapId::Polder => Vec3::new(0.0, 0.6, 7.8),
        MapId::Sanatorium => Vec3::new(22.0, STOREY, 46.0),
        MapId::Archive => Vec3::new(4.0, 0.0, 21.0),
    }
}
const MACHINE_SIZE: Vec3 = Vec3::new(1.6, 1.6, 0.9);

/// Where the Weapon on the machine's tray rests when it's out, and where it is when fully inside,
/// from the machine's centre. The slot is in the top, so the body hides it going in.
const TRAY_OUT: Vec3 = Vec3::new(0.0, MACHINE_SIZE.y / 2.0 + 0.25, 0.1);
const TRAY_IN: Vec3 = Vec3::new(0.0, MACHINE_SIZE.y / 2.0 - 0.3, 0.1);

/// The Weapon on the machine's tray, shown while one is sliding in or out or waiting. It is
/// plain while it's worked on and has the Upgraded glint colour once it's Ready.
#[derive(Component)]
struct Tray {
    upgraded: Handle<StandardMaterial>,
}

/// The machine's lights, lit once the Power is on.
#[derive(Component)]
struct MachineLight;

pub struct PackAPunchPlugin;

impl Plugin for PackAPunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_machine).add_systems(
            Update,
            ((offer_machines, use_machine).chain().in_set(HostSimulation), run_machine, prompt_machine, show_machine)
                .chain()
                .run_if(world_runs),
        );
    }
}

fn spawn_machine(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    guns: Res<GunModels>,
    map: Res<MapId>,
) {
    let body = Cuboid::from_size(MACHINE_SIZE);
    let centre = machine_at(*map) + Vec3::Y * body.half_size.y;
    let glow = materials.add(StandardMaterial {
        base_color: UPGRADED_PURPLE,
        emissive: LinearRgba::from(UPGRADED_PURPLE) * 6.0,
        ..default()
    });
    let upgraded = materials.add(StandardMaterial {
        base_color: UPGRADED_PURPLE,
        emissive: LinearRgba::from(UPGRADED_PURPLE) * 2.0,
        perceptual_roughness: 0.4,
        ..default()
    });
    let machine = commands.spawn((
        InRun,
        Name::new("Pack-a-Punch"),
        PackAPunch::default(),
        PricedFor(Box::new(price_for)),
        Offers::default(),
        Mesh3d(meshes.add(body)),
        MeshMaterial3d(materials.add(Color::srgb(0.25, 0.18, 0.3))),
        Transform::from_translation(centre),
        RigidBody::Static,
        Collider::from(body),
        CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        children![
            (
                // The slot the Weapon slides into.
                Mesh3d(meshes.add(Cuboid::new(1.4, 0.02, 0.2))),
                MeshMaterial3d(materials.add(Color::srgb(0.05, 0.03, 0.08))),
                Transform::from_xyz(0.0, body.half_size.y + 0.005, 0.1),
            ),
            (
                // Its mesh and material are set from the Weapon inside as it shows.
                Tray { upgraded },
                Mesh3d(guns.world(WeaponId::M1911)),
                MeshMaterial3d(guns.material.clone()),
                Transform::from_translation(TRAY_IN),
                Visibility::Hidden,
            ),
            (
                MachineLight,
                Mesh3d(meshes.add(Cuboid::new(1.2, 0.1, 0.05))),
                MeshMaterial3d(glow),
                Transform::from_xyz(0.0, 0.45, body.half_size.z + 0.03),
                // One small unshadowed light in its purple, lit with the glow strip.
                PointLight {
                    color: UPGRADED_PURPLE,
                    intensity: MACHINE_LIGHT_INTENSITY,
                    range: MACHINE_LIGHT_RANGE,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Visibility::Hidden,
            ),
        ],
    )).id();
    crate::maps::machine_look::dress(
        &mut commands,
        &mut meshes,
        &mut materials,
        machine,
        &crate::maps::machine_look::pack_a_punch(),
        Color::srgb(0.55, 0.25, 0.75),
        LinearRgba::BLACK,
    );
}

/// A bought upgrade or Repack takes the Weapon in hand and brings up the other; a used Ready
/// machine hands the Upgraded Weapon back to the Player who put it in. Busy for everyone else
/// meanwhile.
fn use_machine(
    mut purchases: MessageReader<Purchased>,
    mut holds: MessageReader<HoldPurchased>,
    mut machines: Query<&mut PackAPunch>,
    mut players: Query<(&mut Loadout, &Modifiers), With<Player>>,
) {
    let taps = purchases.read().map(|&Purchased { player, buyable }| (player, buyable, false));
    let held = holds.read().map(|&HoldPurchased { player, buyable }| (player, buyable, true));
    for (player, entity, hold) in taps.chain(held).collect::<Vec<_>>() {
        let Ok(mut machine) = machines.get_mut(entity) else { continue };
        let Ok((mut loadout, modifiers)) = players.get_mut(player) else { continue };
        // The Use that took a Weapon out can't also buy the next one.
        if machine.is_cooling(player) {
            continue;
        }
        let slots = ammo_mod_slots(&modifiers.0);
        match machine.stage() {
            None => {
                let copy = loadout.held();
                let work = match (copy.upgraded, hold) {
                    (false, false) => Work::Upgrade,
                    (true, false) if copy.ammo_mods.has_room(slots) => Work::AmmoMod(rand::random(), slots),
                    (true, true) if copy.has_free_mount() => Work::Attachment(rand::random(), rand::random()),
                    _ => continue,
                };
                if let Some(weapon) = loadout.take_held() {
                    machine.insert_for(player, weapon, work);
                }
            }
            // Only the Player who put the Weapon in takes it out.
            Some(Stage::Ready) if !hold && machine.is_owned_by(player) => {
                if let Some(weapon) = machine.take() {
                    loadout.give_copy(weapon);
                    machine.cool(player);
                }
            }
            Some(_) => {}
        }
    }
}

fn run_machine(mut machines: Query<&mut PackAPunch>, time: Res<Time>) {
    for mut machine in &mut machines {
        machine.advance(time.delta_secs());
    }
}

/// Where the machine standing at `at` is used from, in front of it.
pub fn use_point(at: Vec3) -> Vec3 {
    at + Vec3::Z * (MACHINE_SIZE.z / 2.0 + 0.1)
}

/// The machine owns its Buyable, so it keeps the Prompt, price and refusal current, and the hold
/// half of a Repack with them. A Sabotaged machine shows no Prompt.
/// What the machine's Prompt reads for one Player.
struct Shown {
    prompt: String,
    price: Option<u64>,
    /// Whether the buy is refused with the buzzer.
    refuse: bool,
    /// Whether it has a hold half, and if so whether a tap still buys alongside it.
    hold: Option<bool>,
}

/// The Player's side of the machine's Prompt: the Weapon in hand and what it costs to work on it.
struct Hands<'a> {
    player: Entity,
    held: Held,
    drinking: bool,
    copy: &'a WeaponCopy,
    modifiers: &'a [Modifier],
}

impl Hands<'_> {
    fn new<'a>(
        player: Entity,
        loadout: &'a Loadout,
        modifiers: &'a Modifiers,
        drinking: bool,
    ) -> Hands<'a> {
        let copy = loadout.held();
        let held = Held {
            upgraded: copy.upgraded,
            ammo_mod_room: copy.ammo_mods.has_room(ammo_mod_slots(&modifiers.0)),
            free_mount: copy.has_free_mount(),
            spare: loadout.has_spare(),
        };
        Hands { player, held, drinking, copy, modifiers: &modifiers.0 }
    }

    fn price(&self, base: u64) -> u64 {
        machine_price(base, self.modifiers)
    }

    fn upgrade_price(&self) -> u64 {
        self.price(PAP_PRICE)
    }

    fn tap_price(&self) -> u64 {
        self.price(repack_price(self.copy.repacks))
    }

    fn hold_price(&self) -> u64 {
        self.price(attachment_repack_price(self.copy.repacks))
    }
}

/// The refusal the machine shows while it waits for Power, or `None` once it's Powered: the Power,
/// or every Generator Held.
fn unpowered(supply: &Supply) -> Option<String> {
    (!supply.powers_pack_a_punch()).then(|| supply.refusal(None))
}

/// What the machine shows `hands`, if anything. `unpowered` is the refusal it shows while it waits
/// for Power, `None` once it's Powered.
fn shown(machine: &PackAPunch, hands: &Hands, sabotaged: bool, unpowered: Option<&str>) -> Option<Shown> {
    let want = if sabotaged || machine.is_cooling(hands.player) {
        MachinePrompt::NoPrompt
    } else {
        machine_prompt(unpowered.is_none(), machine.stage(), hands.held, !hands.drinking)
    };
    // The hold half, if this Prompt has one, and whether a tap buys alongside it.
    let mut hold = None;
    let (prompt, price, refuse) = match want {
        MachinePrompt::NoPrompt => return None,
        MachinePrompt::NeedsPower => (unpowered.unwrap_or_default().to_string(), None, true),
        MachinePrompt::Upgrade => ("Press F to Pack-a-Punch your Weapon".to_string(), Some(hands.upgrade_price()), false),
        MachinePrompt::OnlyWeapon => ("Press F to Pack-a-Punch your Weapon".to_string(), Some(hands.upgrade_price()), true),
        MachinePrompt::Repack { tap, hold: holds } | MachinePrompt::RepackOnlyWeapon { tap, hold: holds } => {
            let refuse = matches!(want, MachinePrompt::RepackOnlyWeapon { .. });
            if holds {
                hold = Some(tap);
            }
            // With only the hold on offer, the Prompt's own line is the hold's.
            if tap {
                ("Press F to Repack your Weapon".to_string(), Some(hands.tap_price()), refuse)
            } else {
                ("Hold F to Repack an Attachment".to_string(), Some(hands.hold_price()), refuse)
            }
        }
        MachinePrompt::Take if !machine.is_owned_by(hands.player) => ("Pack-a-Punch is busy".to_string(), None, false),
        MachinePrompt::Take => {
            let name = machine.inside.as_ref().map_or("", |inside| inside.weapon.stats().name);
            (format!("Press F to take {name}"), None, false)
        }
    };
    Some(Shown { prompt, price, refuse, hold })
}

/// Judges what the machine offers each Player but the `LocalPlayer`, from their own Weapons and
/// Perks, for the Host to hold a request against (see `Offers`).
fn offer_machines(
    mut machines: Query<(&PackAPunch, &mut Offers, Has<Sabotaged>)>,
    players: Query<(Entity, &Loadout, &Modifiers, Has<Drinking>), (With<Player>, Without<LocalPlayer>)>,
    supply: Supply,
) {
    let unpowered = unpowered(&supply);
    for (machine, mut offers, sabotaged) in &mut machines {
        for (player, loadout, modifiers, drinking) in &players {
            let hands = Hands::new(player, loadout, modifiers, drinking);
            let offer = shown(machine, &hands, sabotaged, unpowered.as_deref()).map(|shown| Offer {
                price: shown.price,
                tap: shown.hold.unwrap_or(true),
                hold: shown.hold.map(|_| hands.hold_price()),
                refused: shown.refuse,
            });
            if offers.get(player) != Some(offer) {
                offers.set(player, offer);
            }
        }
    }
}

fn prompt_machine(
    mut commands: Commands,
    mut machines: Query<(
        Entity,
        &PackAPunch,
        &Transform,
        Option<&mut Buyable>,
        Option<&mut HoldBuy>,
        Has<Refused>,
        Has<Sabotaged>,
        Has<NoLocalPrompt>,
    )>,
    player: Single<(Entity, &Loadout, &Modifiers, Has<Drinking>), With<LocalPlayer>>,
    supply: Supply,
) {
    let unpowered = unpowered(&supply);
    let (local, loadout, modifiers, drinking) = *player;
    let hands = Hands::new(local, loadout, modifiers, drinking);
    let hold_price = hands.hold_price();
    let hold_prompt = "Hold F to Repack an Attachment".to_string();
    for (entity, machine, transform, buyable, hold_buy, refused, sabotaged, quiet) in &mut machines {
        let Some(Shown { prompt, price, refuse, hold }) = shown(machine, &hands, sabotaged, unpowered.as_deref()) else {
            if machine.is_cooling(local) && !sabotaged {
                // The local Player's window: no Prompt for them, but the Buyable stays for a
                // teammate to use, who is judged by the Host's Offers.
                if buyable.is_none() {
                    commands.entity(entity).insert(Buyable {
                        use_point: use_point(transform.translation),
                        prompt: String::new(),
                        price: None,
                        range: USE_RANGE,
                    });
                }
                if !quiet {
                    commands.entity(entity).insert(NoLocalPrompt).remove::<(HoldBuy, Refused)>();
                }
            } else if buyable.is_some() {
                commands.entity(entity).remove::<(Buyable, HoldBuy, Refused, NoLocalPrompt)>();
            }
            continue;
        };
        if quiet {
            commands.entity(entity).remove::<NoLocalPrompt>();
        }
        match (hold, hold_buy) {
            (Some(tap), Some(mut hold_buy)) => {
                if hold_buy.tap != tap {
                    hold_buy.tap = tap;
                }
                if hold_buy.price != Some(hold_price) {
                    hold_buy.price = Some(hold_price);
                }
            }
            (Some(tap), None) => {
                commands.entity(entity).insert(HoldBuy {
                    prompt: hold_prompt.clone(),
                    price: Some(hold_price),
                    tap,
                });
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<HoldBuy>();
            }
            (None, None) => {}
        }
        match buyable {
            Some(mut buyable) => {
                if buyable.prompt != prompt {
                    buyable.prompt = prompt;
                }
                if buyable.price != price {
                    buyable.price = price;
                }
            }
            None => {
                commands.entity(entity).insert(Buyable {
                    use_point: use_point(transform.translation),
                    prompt,
                    price,
                    range: USE_RANGE,
                });
            }
        }
        match (refuse, refused) {
            (true, false) => {
                commands.entity(entity).insert(Refused);
            }
            (false, true) => {
                commands.entity(entity).remove::<Refused>();
            }
            _ => {}
        }
    }
}

/// The tray shows the Weapon inside sliding into the slot and out again, like the Mystery Box's
/// Prize; the lights come on with the Power.
fn show_machine(
    machines: Query<(&PackAPunch, &Children)>,
    mut trays: Query<
        (&Tray, &mut Transform, &mut Visibility, &mut Mesh3d, &mut MeshMaterial3d<StandardMaterial>),
        Without<MachineLight>,
    >,
    mut lights: Query<&mut Visibility, (With<MachineLight>, Without<Tray>)>,
    guns: Res<GunModels>,
    supply: Supply,
) {
    let shown = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for (machine, children) in &machines {
        let on_show = machine.on_show().filter(|&(_, slide, _)| slide > 0.0);
        for child in children {
            if let Ok((tray, mut transform, mut visibility, mut mesh, mut material)) = trays.get_mut(*child) {
                visibility.set_if_neq(shown(on_show.is_some()));
                let Some((id, slide, stage)) = on_show else { continue };
                transform.translation = TRAY_IN.lerp(TRAY_OUT, slide);
                let (want_mesh, want_material) = match stage {
                    // With the atlas worn the Upgraded Weapon shows its camo; without it, flat purple.
                    Stage::Ready if guns.textured => (guns.world(id), guns.upgraded_material.clone()),
                    Stage::Ready => (guns.world(id), tray.upgraded.clone()),
                    _ => (guns.world(id), guns.material.clone()),
                };
                if mesh.0 != want_mesh {
                    mesh.0 = want_mesh;
                }
                if material.0 != want_material {
                    material.0 = want_material;
                }
            }
            if let Ok(mut light) = lights.get_mut(*child) {
                light.set_if_neq(shown(supply.powers_pack_a_punch()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuning::UPGRADED_WEAPONS;

    #[test]
    fn a_weapon_slides_in_stays_inside_then_slides_out_and_waits() {
        assert_eq!(slot_slide(0.0), 1.0);
        let mid = slot_slide(PAP_SLIDE_SECS / 2.0);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(slot_slide(PAP_SLIDE_SECS), 0.0);
        assert_eq!(slot_slide(PAP_PROCESS_SECS - 0.01), 0.0);
        let leaving = slot_slide(PAP_PROCESS_SECS + PAP_SLIDE_SECS / 2.0);
        assert!(leaving > 0.0 && leaving < 1.0);
        assert_eq!(slot_slide(PAP_PROCESS_SECS + PAP_SLIDE_SECS), 1.0);
        assert_eq!(slot_slide(PAP_PROCESS_SECS + PAP_PICKUP_SECS - 0.01), 1.0);
        // Lost: gone from sight at once.
        assert_eq!(slot_slide(PAP_PROCESS_SECS + PAP_PICKUP_SECS), 0.0);
    }

    // The slide in and the slide out both fit inside the processing time.
    const _: () = assert!(PAP_SLIDE_SECS * 2.0 < PAP_PROCESS_SECS);

    #[test]
    fn every_weapon_has_its_a3_upgraded_entry() {
        let names = [
            (WeaponId::M1911, "Mustang & Sally"),
            (WeaponId::Olympia, "Hades"),
            (WeaponId::M14, "Mnesia"),
            (WeaponId::Mp40, "The Afterburner"),
            (WeaponId::TrenchGun, "Gut Shot"),
            (WeaponId::Ak74u, "AK-74fu2"),
            (WeaponId::Cz75, "Calamity"),
            (WeaponId::Cz75DualWield, "Calamity & Jane"),
            (WeaponId::Python, "Cobra"),
            (WeaponId::Spectre, "Phantom"),
            (WeaponId::G11, "G115 Generator"),
            (WeaponId::Famas, "G16-GL35"),
            (WeaponId::Aug, "AUG-50M3"),
            (WeaponId::Commando, "Predator"),
            (WeaponId::Galil, "Lamentation"),
            (WeaponId::FnFal, "EPC WN"),
            (WeaponId::Hs10, "Typhoid & Mary"),
            (WeaponId::Spas12, "SPAZ-24"),
            (WeaponId::Dragunov, "D115 Disassembler"),
            (WeaponId::L96a1, "L115 Isolator"),
            (WeaponId::Rpk, "R115 Resonator"),
            (WeaponId::Hk21, "H115 Oscillator"),
            (WeaponId::M72Law, "M72 Anarchy"),
            (WeaponId::ChinaLake, "China Beach"),
            (WeaponId::RayGun, "Porter's X2 Ray Gun"),
            (WeaponId::Thundergun, "ZeusCannon"),
            (WeaponId::Crossbow, "Awful Lawton"),
            (WeaponId::BallisticKnife, "Krauss Refibrillator"),
        ];
        assert_eq!(names.len(), UPGRADED_WEAPONS.len());
        for (i, (id, name)) in names.into_iter().enumerate() {
            assert_eq!(UPGRADED_WEAPONS[i].id, id, "{name} is out of place");
            assert_eq!(id.upgraded().name, name);
        }
    }

    #[test]
    fn the_m14_upgrades_to_mnesia() {
        let mnesia = WeaponId::M14.upgraded();
        assert_eq!((mnesia.damage_max, mnesia.damage_min), (200.0, 150.0));
        assert_eq!((mnesia.headshot, mnesia.torso), (6.0, 2.0));
        assert_eq!((mnesia.magazine, mnesia.reserve_max), (16, 192));
        // Unchanged from the M14.
        assert_eq!(mnesia.class, WeaponId::M14.stats().class);
        assert_eq!(mnesia.reload, WeaponId::M14.stats().reload);
    }

    #[test]
    fn some_upgrades_change_how_the_weapon_works() {
        let mustang = WeaponId::M1911.upgraded();
        assert_eq!(mustang.class, crate::weapons::arsenal::WeaponClass::DualWield);
        assert!(mustang.splash.is_some(), "Mustang & Sally fires grenades");
        assert!(WeaponId::Olympia.upgraded().burn.is_some(), "the Hades burns");
        assert_eq!(WeaponId::Olympia.upgraded().pellets, 12);
        assert_eq!(WeaponId::Cz75.upgraded().fire, crate::weapons::arsenal::FireMode::Auto);
        assert_eq!(WeaponId::FnFal.upgraded().fire, crate::weapons::arsenal::FireMode::Burst(3));
        assert!(WeaponId::Olympia.stats().burn.is_none(), "base Weapons don't burn");
    }

    #[test]
    fn an_upgraded_weapon_comes_out_with_full_ammo() {
        let mut m14 = WeaponCopy::new(WeaponId::M14);
        m14.magazine = 1;
        m14.reserve = 0;
        let mnesia = m14.upgrade();
        assert!(mnesia.upgraded);
        assert_eq!(mnesia.stats().name, "Mnesia");
        assert_eq!((mnesia.magazine, mnesia.reserve), (16, 192));

        // The M1911 starts short of its Reserve; Mustang & Sally doesn't.
        let mustang = WeaponCopy::new(WeaponId::M1911).upgrade();
        assert_eq!((mustang.magazine, mustang.reserve), (12, 50));
    }

    fn machine_with_m14() -> PackAPunch {
        let mut machine = PackAPunch::default();
        machine.insert(WeaponCopy::new(WeaponId::M14), Work::Upgrade);
        machine
    }

    #[test]
    fn the_weapon_is_processed_for_3_s_then_can_be_taken() {
        let mut machine = machine_with_m14();
        assert_eq!(machine.inside(), Some(WeaponId::M14));
        machine.advance(PAP_PROCESS_SECS - 0.1);
        assert_eq!(machine.stage(), Some(Stage::Processing));
        assert!(machine.take().is_none(), "not while it's being worked on");
        machine.advance(0.2);
        assert_eq!(machine.stage(), Some(Stage::Ready));
        let taken = machine.take().expect("ready to take");
        assert!(taken.upgraded);
        assert_eq!(machine.stage(), None, "the machine is free again");
    }

    #[test]
    fn a_weapon_not_taken_within_15_s_is_lost() {
        let mut machine = machine_with_m14();
        machine.advance(PAP_PROCESS_SECS);
        machine.advance(PAP_PICKUP_SECS - 0.1);
        assert_eq!(machine.stage(), Some(Stage::Ready), "still waiting at 14.9 s");
        machine.advance(0.2);
        assert_eq!(machine.stage(), None);
        assert!(machine.take().is_none(), "gone for good");
    }

    #[test]
    fn stages_follow_the_clock() {
        assert_eq!(stage(0.0), Stage::Processing);
        assert_eq!(stage(PAP_PROCESS_SECS), Stage::Ready);
        assert_eq!(stage(PAP_PROCESS_SECS + PAP_PICKUP_SECS), Stage::Lost);
    }

    const FRESH: Held = Held {
        upgraded: false,
        ammo_mod_room: true,
        free_mount: true,
        spare: true,
    };

    #[test]
    fn the_machine_waits_for_the_power() {
        assert_eq!(machine_prompt(false, None, FRESH, true), MachinePrompt::NeedsPower);
        assert_eq!(machine_prompt(true, None, FRESH, true), MachinePrompt::Upgrade);
    }

    #[test]
    fn one_weapon_at_a_time() {
        assert_eq!(machine_prompt(true, Some(Stage::Processing), FRESH, true), MachinePrompt::NoPrompt);
        assert_eq!(machine_prompt(true, Some(Stage::Ready), FRESH, true), MachinePrompt::Take);
        assert_eq!(machine_prompt(true, Some(Stage::Ready), FRESH, false), MachinePrompt::NoPrompt);
    }

    #[test]
    fn a_full_upgraded_weapon_or_busy_hands_show_nothing() {
        let full = Held {
            upgraded: true,
            ammo_mod_room: false,
            free_mount: false,
            ..FRESH
        };
        assert_eq!(machine_prompt(true, None, full, true), MachinePrompt::NoPrompt);
        assert_eq!(machine_prompt(true, None, FRESH, false), MachinePrompt::NoPrompt);
    }

    #[test]
    fn an_upgraded_weapon_with_room_is_offered_a_repack() {
        let upgraded = Held { upgraded: true, ..FRESH };
        let both = MachinePrompt::Repack { tap: true, hold: true };
        assert_eq!(machine_prompt(true, None, upgraded, true), both);
        assert_eq!(machine_prompt(false, None, upgraded, true), MachinePrompt::NeedsPower);
        let only = Held { spare: false, ..upgraded };
        assert_eq!(
            machine_prompt(true, None, only, true),
            MachinePrompt::RepackOnlyWeapon { tap: true, hold: true }
        );
    }

    #[test]
    fn each_half_of_the_repack_shows_while_the_weapon_has_room_for_it() {
        let upgraded = Held { upgraded: true, ..FRESH };
        let no_mods = Held {
            ammo_mod_room: false,
            ..upgraded
        };
        assert_eq!(machine_prompt(true, None, no_mods, true), MachinePrompt::Repack { tap: false, hold: true });
        // Mustang & Sally, launchers and the wonder Weapons never have a free mount.
        let no_mounts = Held {
            free_mount: false,
            ..upgraded
        };
        assert_eq!(machine_prompt(true, None, no_mounts, true), MachinePrompt::Repack { tap: true, hold: false });
    }

    #[test]
    fn a_hold_repack_adds_an_attachment_and_both_kinds_raise_the_price() {
        let mut machine = PackAPunch::default();
        let mut mnesia = WeaponCopy::new(WeaponId::M14).upgrade();
        assert_eq!(attachment_repack_price(mnesia.repacks), 2000);
        mnesia.magazine = 0;
        // The Underbarrel mount, then its first Attachment: the Masterkey.
        machine.insert(mnesia, Work::Attachment(0.7, 0.0));
        machine.advance(PAP_PROCESS_SECS);
        let once = machine.take().expect("ready to take");
        assert_eq!(once.attachments, [crate::weapons::attachment::Attachment::Masterkey]);
        assert_eq!((once.repacks, once.magazine), (1, 16), "full ammo, as out of the upgrade");
        assert_eq!(once.underbarrel.map(|underbarrel| underbarrel.magazine), Some(6));
        assert_eq!(attachment_repack_price(once.repacks), 3000);

        // A tap Repack counts too.
        let twice = once.repack(0.0, MAX_AMMO_MODS);
        assert_eq!(attachment_repack_price(twice.repacks), 4000);
        assert_eq!(repack_price(twice.repacks), 4500);
    }

    #[test]
    fn a_repack_adds_an_ammo_mod_and_counts_toward_the_next_price() {
        let mut machine = PackAPunch::default();
        let mut mnesia = WeaponCopy::new(WeaponId::M14).upgrade();
        mnesia.magazine = 0;
        machine.insert(mnesia, Work::AmmoMod(0.0, MAX_AMMO_MODS));
        assert_eq!(machine.stage(), Some(Stage::Processing), "the same flow as the upgrade");
        machine.advance(PAP_PROCESS_SECS);
        let once = machine.take().expect("ready to take");
        assert_eq!(once.ammo_mods.len(), 1);
        assert_eq!((once.repacks, once.magazine), (1, 16));
        assert_eq!(repack_price(once.repacks), 3500);

        let twice = once.repack(0.99, MAX_AMMO_MODS);
        assert_eq!(twice.ammo_mods.len(), 2);
        assert!(!twice.ammo_mods.has_room(MAX_AMMO_MODS), "no third Ammo Mod");
        let unchanged = twice.repack(0.5, MAX_AMMO_MODS);
        assert_eq!((unchanged.ammo_mods.len(), unchanged.repacks), (2, 2));

        // With a third slot, there's room for one more.
        assert!(unchanged.ammo_mods.has_room(3));
        let thrice = unchanged.repack(0.5, 3);
        assert_eq!((thrice.ammo_mods.len(), thrice.repacks), (3, 3));
    }

    #[test]
    fn price_entries_rewrite_the_upgrade_and_repack_prices() {
        use crate::modifier::{Effect, Source};
        assert_eq!(machine_price(PAP_PRICE, &[]), 5000);
        let cheaper = [Modifier {
            stat: Stat::PackAPunchPrice,
            source: Source("test"),
            effect: Effect::Reduction(0.9, 2),
        }];
        assert_eq!(machine_price(PAP_PRICE, &cheaper), 4050);
        assert_eq!(machine_price(repack_price(1), &cheaper), 2835);
    }

    #[test]
    fn ammo_mods_stay_with_their_copy() {
        let mut loadout = Loadout::new(WeaponId::M14);
        loadout.give_copy(WeaponCopy::new(WeaponId::Mp40).upgrade().repack(0.0, MAX_AMMO_MODS));
        assert_eq!(loadout.held().ammo_mods.len(), 1);
        // Replacing the copy with a fresh one loses them.
        loadout.give(WeaponId::Mp40);
        assert!(loadout.held().ammo_mods.is_empty());
    }

    #[test]
    fn the_players_only_weapon_is_refused() {
        let only = Held { spare: false, ..FRESH };
        assert_eq!(machine_prompt(true, None, only, true), MachinePrompt::OnlyWeapon);
    }
}

/// Pack-a-Punch with two Players: busy for everyone while it works, its Weapon only back to the
/// Player who put it in, and a Repack priced by the Weapon of the Player paying.
#[cfg(test)]
mod two_player_tests {
    use super::*;
    use crate::economy::buy::UseRequest;
    use crate::players::player::two_players::{balance, two_players_with};
    use crate::economy::points::Points;
    use crate::enemies::zombie::headless::Sim;

    fn setup() -> (Sim, Entity, Entity, Entity) {
        let (mut sim, local, teammate) = two_players_with(|app| {
            app.add_systems(Update, (use_machine, run_machine).chain());
        });
        let world = sim.world_mut();
        for player in [local, teammate] {
            world.get_mut::<Points>(player).unwrap().balance = 20_000;
        }
        let machine = world
            .spawn((
                PackAPunch::default(),
                PricedFor(Box::new(price_for)),
                Buyable { use_point: Vec3::ZERO, prompt: String::new(), price: Some(PAP_PRICE), range: USE_RANGE },
            ))
            .id();
        (sim, local, teammate, machine)
    }

    fn has_upgraded(sim: &mut Sim, player: Entity, id: WeaponId) -> bool {
        sim.world_mut().get::<Loadout>(player).unwrap().weapons().any(|weapon| weapon.id == id && weapon.upgraded)
    }

    #[test]
    fn the_machine_is_busy_and_gives_the_weapon_only_to_its_owner() {
        let (mut sim, local, teammate, machine) = setup();
        sim.world_mut().get_mut::<Loadout>(teammate).unwrap().give(WeaponId::M14);
        sim.world_mut().get_mut::<Loadout>(local).unwrap().give(WeaponId::Olympia);
        let teammate_before = balance(&mut sim, teammate);
        sim.send(UseRequest { player: teammate, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), Some(WeaponId::M14));
        assert_eq!(balance(&mut sim, teammate), teammate_before - PAP_PRICE);

        // Busy: the other Player's use leaves what's inside alone.
        sim.send(UseRequest { player: local, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), Some(WeaponId::M14));

        // Ready, but not theirs to take. (A Ready machine's Buyable is free.)
        sim.run(sim.frames(PAP_PROCESS_SECS + 0.5));
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().stage(), Some(Stage::Ready));
        sim.world_mut().get_mut::<Buyable>(machine).unwrap().price = None;
        sim.send(UseRequest { player: local, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().stage(), Some(Stage::Ready));
        assert!(!has_upgraded(&mut sim, local, WeaponId::M14));

        sim.send(UseRequest { player: teammate, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), None);
        assert!(has_upgraded(&mut sim, teammate, WeaponId::M14));
        assert!(!has_upgraded(&mut sim, local, WeaponId::M14));
    }

    /// #588: a Weapon taken out of the machine must fire straight away, whatever the teammate is
    /// doing with their own hands. A teammate drinking a Perk (often bought on the same trip) or
    /// throwing Equipment used to hold the whole Weapon chain for every Player: no shot, and the
    /// raise out of the machine frozen part-way, until their drink or throw ended.
    #[test]
    fn a_teammates_drink_or_throw_doesnt_stop_the_packed_weapon_firing() {
        use bevy::ecs::system::RunSystemOnce;
        use crate::economy::perk::{Perk, hands_free};
        use crate::weapons::equipment::{Handled, Handling, arms_free};

        let (mut sim, local, teammate, machine) = setup();
        sim.world_mut().get_mut::<Loadout>(local).unwrap().give(WeaponId::M14);
        sim.send(UseRequest { player: local, buyable: machine, hold: false });
        sim.run(2);
        sim.run(sim.frames(PAP_PROCESS_SECS + 0.5));
        sim.world_mut().get_mut::<Buyable>(machine).unwrap().price = None;
        sim.send(UseRequest { player: local, buyable: machine, hold: false });
        sim.run(2);
        assert!(has_upgraded(&mut sim, local, WeaponId::M14), "the Upgraded Weapon wasn't taken");

        // The teammate drinks a Perk and throws a Monkey Bomb meanwhile.
        sim.world_mut().entity_mut(teammate).insert((Drinking::new(Perk::Juggernog), Handling::new(Handled::MonkeyBomb, 0.0)));
        assert!(sim.world_mut().run_system_once(hands_free).unwrap(), "the teammate's drink holds the local Weapon");
        assert!(sim.world_mut().run_system_once(arms_free).unwrap(), "the teammate's throw holds the local Weapon");

        // The local Player's own drink and throw still do.
        sim.world_mut().entity_mut(teammate).remove::<(Drinking, Handling)>();
        sim.world_mut().entity_mut(local).insert(Drinking::new(Perk::Juggernog));
        assert!(!sim.world_mut().run_system_once(hands_free).unwrap());
        sim.world_mut().entity_mut(local).remove::<Drinking>().insert(Handling::new(Handled::MonkeyBomb, 0.0));
        assert!(!sim.world_mut().run_system_once(arms_free).unwrap());
    }

    #[test]
    fn a_repack_costs_what_its_owners_weapon_costs() {
        let (mut sim, local, teammate, machine) = setup();
        // The teammate holds an Upgraded Weapon already Repacked twice; the local Player's is fresh.
        let mut worn = WeaponCopy::new(WeaponId::M14).upgrade();
        worn.repacks = 2;
        sim.world_mut().get_mut::<Loadout>(teammate).unwrap().give_copy(worn);
        sim.world_mut().get_mut::<Loadout>(local).unwrap().give(WeaponId::Olympia);
        let (local_before, teammate_before) = (balance(&mut sim, local), balance(&mut sim, teammate));
        // The Prompt's price is the upgrade's, whatever the teammate holds.
        sim.send(UseRequest { player: teammate, buyable: machine, hold: false });
        sim.run(2);
        assert_ne!(repack_price(2), PAP_PRICE);
        assert_eq!(balance(&mut sim, teammate), teammate_before - repack_price(2));
        assert_eq!(balance(&mut sim, local), local_before, "the local Player paid for the teammate's Repack");
    }
}

/// #597: the Use that takes a Weapon out of the machine must not also buy the next Pack-a-Punch,
/// for the Player who took it, whether it's the tap or the hold, locally or from a remote Player
/// (judged by the Host's Offers). A teammate is never held up by someone else's window.
#[cfg(test)]
mod reuse_window_tests {
    use super::*;
    use crate::economy::buy::UseRequest;
    use crate::economy::points::Points;
    use crate::enemies::zombie::headless::Sim;
    use crate::maps::power::Power;
    use crate::players::player::two_players::{balance, two_players_with};

    /// A Powered machine wired the way the game wires it: Prompt for the local Player, Offers for
    /// the teammate. Both Players hold an M14.
    fn setup() -> (Sim, Entity, Entity, Entity) {
        let (mut sim, local, teammate) = two_players_with(|app| {
            let mut power = Power::default();
            power.switch_on();
            app.insert_resource(power);
            app.add_systems(Update, (offer_machines, use_machine, run_machine, prompt_machine).chain());
        });
        let world = sim.world_mut();
        for player in [local, teammate] {
            world.get_mut::<Points>(player).unwrap().balance = 20_000;
            world.get_mut::<Loadout>(player).unwrap().give(WeaponId::M14);
        }
        let machine = world
            .spawn((PackAPunch::default(), PricedFor(Box::new(price_for)), Offers::default(), Transform::default()))
            .id();
        sim.run(2);
        (sim, local, teammate, machine)
    }

    /// `player` puts the M14 in, waits for it, and takes it out. Returns right after the take.
    fn pack_and_take(sim: &mut Sim, player: Entity, machine: Entity) {
        sim.send(UseRequest { player, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), Some(WeaponId::M14));
        sim.run(sim.frames(PAP_PROCESS_SECS + 0.5));
        sim.send(UseRequest { player, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), None, "the Weapon wasn't taken");
    }

    fn assert_nothing_bought(sim: &mut Sim, player: Entity, machine: Entity, before: u64) {
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), None);
        assert_eq!(balance(sim, player), before, "points were spent inside the window");
    }

    #[test]
    fn a_tap_right_after_taking_a_weapon_out_buys_nothing_for_either_player() {
        for remote in [false, true] {
            let (mut sim, local, teammate, machine) = setup();
            let player = if remote { teammate } else { local };
            pack_and_take(&mut sim, player, machine);
            let before = balance(&mut sim, player);
            sim.send(UseRequest { player, buyable: machine, hold: false });
            sim.run(2);
            assert_nothing_bought(&mut sim, player, machine, before);
        }
    }

    #[test]
    fn a_hold_right_after_taking_a_weapon_out_buys_nothing_for_either_player() {
        for remote in [false, true] {
            let (mut sim, local, teammate, machine) = setup();
            let player = if remote { teammate } else { local };
            pack_and_take(&mut sim, player, machine);
            let before = balance(&mut sim, player);
            sim.send(UseRequest { player, buyable: machine, hold: true });
            sim.run(2);
            assert_nothing_bought(&mut sim, player, machine, before);
        }
    }

    #[test]
    fn the_machine_answers_again_once_the_window_has_passed() {
        for remote in [false, true] {
            let (mut sim, local, teammate, machine) = setup();
            let player = if remote { teammate } else { local };
            pack_and_take(&mut sim, player, machine);
            sim.run(sim.frames(PAP_REUSE_SECS + 0.2));
            let before = balance(&mut sim, player);
            sim.send(UseRequest { player, buyable: machine, hold: false });
            sim.run(2);
            assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), Some(WeaponId::M14));
            assert_eq!(balance(&mut sim, player), before - repack_price(0));
        }
    }

    #[test]
    fn a_teammate_isnt_held_up_by_your_window() {
        let (mut sim, local, teammate, machine) = setup();
        pack_and_take(&mut sim, local, machine);
        let before = balance(&mut sim, teammate);
        sim.send(UseRequest { player: teammate, buyable: machine, hold: false });
        sim.run(2);
        assert_eq!(sim.world_mut().get::<PackAPunch>(machine).unwrap().inside(), Some(WeaponId::M14));
        assert_eq!(balance(&mut sim, teammate), before - PAP_PRICE);
    }

    #[test]
    fn a_clients_game_starts_the_same_window_when_it_sees_its_weapon_taken() {
        let player = Entity::from_raw_u32(7).unwrap();
        let mut machine = PackAPunch::default();
        machine.insert_for(player, WeaponCopy::new(WeaponId::M14), Work::Upgrade);
        machine.advance(PAP_PROCESS_SECS + 1.0);
        let view = machine.view().map(|(weapon, elapsed, owner)| (weapon.clone(), elapsed, owner));
        machine.show(None);
        assert!(machine.is_cooling(player), "the Host's take should start the window");
        machine.advance(PAP_REUSE_SECS + 0.1);
        assert!(!machine.is_cooling(player));
        // A Weapon that was lost, not taken, starts no window.
        let mut lost = PackAPunch::default();
        lost.show(view);
        lost.advance(PAP_PROCESS_SECS + PAP_PICKUP_SECS);
        assert!(!lost.is_cooling(player));
    }
}
