//! Pack-a-Punch: the machine in the Gazebo that, once the Power is on, takes the Weapon in hand
//! for `PAP_PRICE`, works on it for `PAP_PROCESS_SECS`, then holds its Upgraded form for
//! `PAP_PICKUP_SECS`. A Weapon not taken in time is lost, with no refund.
//!
//! An Upgraded Weapon in hand is Repacked instead, through the same flow: a tap adds a random Ammo
//! Mod it lacks, and holding the Use key adds a random Attachment in a free mount. Each costs more
//! with every Repack of that copy, of either kind.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::ammo_mod::{ammo_mod_slots, repack_price};
use crate::arsenal::WeaponId;
use crate::attachment::attachment_repack_price;
use crate::authority::HostSimulation;
use crate::gun_model::GunModels;
use crate::buy::{Buyable, HoldBuy, HoldPurchased, Purchased, Refused};
use crate::layers::GameLayer;
use crate::modifier::{Modifier, Modifiers, Stat, resolve_points};
use crate::perk::Drinking;
use crate::player::{LocalPlayer, Player};
use crate::power::Power;
use crate::sapper::Sabotaged;
use crate::state::{GameState, InRun, Pause};
use crate::tuning::*;
use crate::weapon::{Loadout, UPGRADED_PURPLE, WeaponCopy};

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
}

/// The Pack-a-Punch machine. It holds one Weapon at a time.
#[derive(Component, Default)]
pub struct PackAPunch {
    inside: Option<Inside>,
}

impl PackAPunch {
    /// The Weapon inside, if any, so the Box can count it as in play.
    pub fn inside(&self) -> Option<WeaponId> {
        self.inside.as_ref().map(|inside| inside.weapon.id)
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
        self.inside = Some(Inside { weapon, elapsed: 0.0 });
    }

    /// Advance the clock. A Weapon left past its pickup window is lost.
    pub fn advance(&mut self, dt: f32) {
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

/// The machine's spot at the back of the Gazebo, facing out of its Power Door (+Z).
pub(crate) const MACHINE_AT: Vec3 = Vec3::new(5.0, 0.0, -9.2);
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
            ((use_machine, run_machine).chain().in_set(HostSimulation), prompt_machine, show_machine)
                .chain()
                .run_if(in_state(Pause::Running)),
        );
    }
}

fn spawn_machine(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    guns: Res<GunModels>,
) {
    let body = Cuboid::from_size(MACHINE_SIZE);
    let centre = MACHINE_AT + Vec3::Y * body.half_size.y;
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
    commands.spawn((
        InRun,
        Name::new("Pack-a-Punch"),
        PackAPunch::default(),
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
                Visibility::Hidden,
            ),
        ],
    ));
}

/// A bought upgrade or Repack takes the Weapon in hand and brings up the other; a used Ready
/// machine hands the Upgraded Weapon back.
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
                    machine.insert(weapon, work);
                }
            }
            Some(Stage::Ready) if !hold => {
                if let Some(weapon) = machine.take() {
                    loadout.give_copy(weapon);
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
    )>,
    player: Single<(&Loadout, &Modifiers, Has<Drinking>), With<LocalPlayer>>,
    power: Res<Power>,
) {
    let (loadout, modifiers, drinking) = *player;
    let copy = loadout.held();
    let held = Held {
        upgraded: copy.upgraded,
        ammo_mod_room: copy.ammo_mods.has_room(ammo_mod_slots(&modifiers.0)),
        free_mount: copy.has_free_mount(),
        spare: loadout.has_spare(),
    };
    let tap_prompt = "Press F to Repack your Weapon".to_string();
    let hold_prompt = "Hold F to Repack an Attachment".to_string();
    let price = |base| machine_price(base, &modifiers.0);
    let upgrade_price = price(PAP_PRICE);
    let (tap_price, hold_price) = (price(repack_price(copy.repacks)), price(attachment_repack_price(copy.repacks)));
    for (entity, machine, transform, buyable, hold_buy, refused, sabotaged) in &mut machines {
        let want = if sabotaged {
            MachinePrompt::NoPrompt
        } else {
            machine_prompt(power.is_on(), machine.stage(), held, !drinking)
        };
        // The hold half, if this Prompt has one, and whether a tap buys alongside it.
        let mut hold = None;
        let (prompt, price, refuse) = match want {
            MachinePrompt::NoPrompt => {
                if buyable.is_some() {
                    commands.entity(entity).remove::<(Buyable, HoldBuy, Refused)>();
                }
                continue;
            }
            MachinePrompt::NeedsPower => ("You must turn on the Power first".to_string(), None, true),
            MachinePrompt::Upgrade => ("Press F to Pack-a-Punch your Weapon".to_string(), Some(upgrade_price), false),
            MachinePrompt::OnlyWeapon => ("Press F to Pack-a-Punch your Weapon".to_string(), Some(upgrade_price), true),
            MachinePrompt::Repack { tap, hold: holds } | MachinePrompt::RepackOnlyWeapon { tap, hold: holds } => {
                let refuse = matches!(want, MachinePrompt::RepackOnlyWeapon { .. });
                if holds {
                    hold = Some(tap);
                }
                // With only the hold on offer, the Prompt's own line is the hold's.
                if tap {
                    (tap_prompt.clone(), Some(tap_price), refuse)
                } else {
                    (hold_prompt.clone(), Some(hold_price), refuse)
                }
            }
            MachinePrompt::Take => {
                let name = machine.inside.as_ref().map_or("", |inside| inside.weapon.stats().name);
                (format!("Press F to take {name}"), None, false)
            }
        };
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
    power: Res<Power>,
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
                light.set_if_neq(shown(power.is_on()));
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
        assert_eq!(mustang.class, crate::arsenal::WeaponClass::DualWield);
        assert!(mustang.splash.is_some(), "Mustang & Sally fires grenades");
        assert!(WeaponId::Olympia.upgraded().burn.is_some(), "the Hades burns");
        assert_eq!(WeaponId::Olympia.upgraded().pellets, 12);
        assert_eq!(WeaponId::Cz75.upgraded().fire, crate::arsenal::FireMode::Auto);
        assert_eq!(WeaponId::FnFal.upgraded().fire, crate::arsenal::FireMode::Burst(3));
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
        assert_eq!(once.attachments, [crate::attachment::Attachment::Masterkey]);
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
