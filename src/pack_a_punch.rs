//! Pack-a-Punch: the machine in the Gazebo that, once the Power is on, takes the Weapon in hand
//! for `PAP_PRICE`, works on it for `PAP_PROCESS_SECS`, then holds its Upgraded form for
//! `PAP_PICKUP_SECS`. A Weapon not taken in time is lost, with no refund.
//!
//! Repacks (Ammo Mods and Attachments on an Upgraded Weapon) arrive later: an Upgraded Weapon in
//! hand shows no Prompt until then.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::arsenal::WeaponId;
use crate::buy::{Buyable, Purchased, Refused};
use crate::layers::GameLayer;
use crate::perk::Drinking;
use crate::player::Player;
use crate::power::Power;
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

/// What the machine's Prompt shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachinePrompt {
    /// Nothing: a Weapon is being worked on, the Player's hands are busy, or the Weapon in hand
    /// is Upgraded already.
    NoPrompt,
    /// "You must turn on the Power first", and the Use key does nothing.
    NeedsPower,
    /// The upgrade, at `PAP_PRICE`.
    Upgrade,
    /// The upgrade's Prompt, refused with the buzzer: the Weapon in hand is the Player's only
    /// one, and the held slot is always filled.
    OnlyWeapon,
    /// Take the Upgraded Weapon, free.
    Take,
}

/// The Weapon in the Player's hand, as the machine sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub upgraded: bool,
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
        None if !hands_free || held.upgraded => MachinePrompt::NoPrompt,
        None if !held.spare => MachinePrompt::OnlyWeapon,
        None => MachinePrompt::Upgrade,
    }
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
    #[allow(dead_code)]
    pub fn inside(&self) -> Option<WeaponId> {
        self.inside.as_ref().map(|inside| inside.weapon.id)
    }

    /// The stage of the Weapon inside, if any.
    pub fn stage(&self) -> Option<Stage> {
        self.inside.as_ref().map(|inside| stage(inside.elapsed))
    }

    /// Take in a Weapon and start working on it. It comes out Upgraded, with full ammo.
    pub fn insert(&mut self, weapon: WeaponCopy) {
        debug_assert!(self.inside.is_none(), "one Weapon at a time");
        self.inside = Some(Inside {
            weapon: weapon.upgrade(),
            elapsed: 0.0,
        });
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
const MACHINE_AT: Vec3 = Vec3::new(5.0, 0.0, -9.2);
const MACHINE_SIZE: Vec3 = Vec3::new(1.6, 1.6, 0.9);

/// The Weapon on the machine's tray, shown while one is inside.
#[derive(Component)]
struct Tray;

/// The machine's lights, lit once the Power is on.
#[derive(Component)]
struct MachineLight;

pub struct PackAPunchPlugin;

impl Plugin for PackAPunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_machine).add_systems(
            Update,
            (use_machine, run_machine, prompt_machine, show_machine)
                .chain()
                .run_if(in_state(Pause::Running)),
        );
    }
}

fn spawn_machine(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let body = Cuboid::from_size(MACHINE_SIZE);
    let centre = MACHINE_AT + Vec3::Y * body.half_size.y;
    let glow = materials.add(StandardMaterial {
        base_color: UPGRADED_PURPLE,
        emissive: LinearRgba::from(UPGRADED_PURPLE) * 6.0,
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
                Tray,
                Mesh3d(meshes.add(Cuboid::new(0.6, 0.1, 0.12))),
                MeshMaterial3d(glow.clone()),
                Transform::from_xyz(0.0, body.half_size.y + 0.05, 0.1),
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

/// A bought upgrade takes the Weapon in hand and brings up the other; a used Ready machine hands
/// the Upgraded Weapon back.
fn use_machine(
    mut purchases: MessageReader<Purchased>,
    mut machines: Query<&mut PackAPunch>,
    mut loadout: Single<&mut Loadout, With<Player>>,
) {
    for Purchased(entity) in purchases.read() {
        let Ok(mut machine) = machines.get_mut(*entity) else { continue };
        match machine.stage() {
            None => {
                if !loadout.held().upgraded
                    && let Some(weapon) = loadout.take_held()
                {
                    machine.insert(weapon);
                }
            }
            Some(Stage::Ready) => {
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

/// The machine owns its Buyable, so it keeps the Prompt, price and refusal current.
fn prompt_machine(
    mut commands: Commands,
    mut machines: Query<(Entity, &PackAPunch, &Transform, Option<&mut Buyable>, Has<Refused>)>,
    player: Single<(&Loadout, Has<Drinking>), With<Player>>,
    power: Res<Power>,
) {
    let (loadout, drinking) = *player;
    let held = Held {
        upgraded: loadout.held().upgraded,
        spare: loadout.has_spare(),
    };
    for (entity, machine, transform, buyable, refused) in &mut machines {
        let want = machine_prompt(power.is_on(), machine.stage(), held, !drinking);
        let (prompt, price, refuse) = match want {
            MachinePrompt::NoPrompt => {
                if buyable.is_some() {
                    commands.entity(entity).remove::<(Buyable, Refused)>();
                }
                continue;
            }
            MachinePrompt::NeedsPower => ("You must turn on the Power first".to_string(), None, true),
            MachinePrompt::Upgrade => ("Press F to Pack-a-Punch your Weapon".to_string(), Some(PAP_PRICE), false),
            MachinePrompt::OnlyWeapon => ("Press F to Pack-a-Punch your Weapon".to_string(), Some(PAP_PRICE), true),
            MachinePrompt::Take => {
                let name = machine.inside.as_ref().map_or("", |inside| inside.weapon.stats().name);
                (format!("Press F to take {name}"), None, false)
            }
        };
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
                    use_point: transform.translation + Vec3::Z * (MACHINE_SIZE.z / 2.0 + 0.1),
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

/// The tray shows a Weapon while one is inside; the lights come on with the Power.
fn show_machine(
    machines: Query<(&PackAPunch, &Children)>,
    mut trays: Query<&mut Visibility, (With<Tray>, Without<MachineLight>)>,
    mut lights: Query<&mut Visibility, With<MachineLight>>,
    power: Res<Power>,
) {
    let shown = |on: bool| if on { Visibility::Inherited } else { Visibility::Hidden };
    for (machine, children) in &machines {
        for child in children {
            if let Ok(mut tray) = trays.get_mut(*child) {
                tray.set_if_neq(shown(machine.stage().is_some()));
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
        machine.insert(WeaponCopy::new(WeaponId::M14));
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
    fn an_upgraded_weapon_or_busy_hands_show_no_upgrade() {
        let upgraded = Held { upgraded: true, ..FRESH };
        assert_eq!(machine_prompt(true, None, upgraded, true), MachinePrompt::NoPrompt);
        assert_eq!(machine_prompt(true, None, FRESH, false), MachinePrompt::NoPrompt);
    }

    #[test]
    fn the_players_only_weapon_is_refused() {
        let only = Held { spare: false, ..FRESH };
        assert_eq!(machine_prompt(true, None, only, true), MachinePrompt::OnlyWeapon);
    }
}
