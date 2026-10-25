//! Attachments: what a hold Repack adds to an Upgraded Weapon. It picks a free mount uniformly,
//! then an Attachment in that mount uniformly. A Weapon holds one per mount, and built-in parts (a
//! sniper's scope, the AUG-50M3's Masterkey) fill theirs.
//!
//! Every Attachment is its own modifier source on the Weapon copy, so it multiplies with Perks
//! and anything else through `resolve`. Dual Mag's flat Reload is the one exception. Optics only
//! count while aiming down sights.
//!
//! Underbarrels are second guns under the barrel with their own ammo: X toggles to one, and a
//! Weapon switch puts the main gun back up.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::arsenal::{Burn, WeaponClass, WeaponId, WeaponStats};
use crate::burn::Ignite;
use crate::explosive::{Flight, in_sight};
use crate::modifier::{Effect, Modifier, Source, Stat};
use crate::perk::hands_free;
use crate::player::Player;
use crate::state::Pause;
use crate::tuning::*;
use crate::weapon::{Loadout, ViewModel};
use crate::zombie::{Attack, DamageZombie, Dying, HitZone, Zombie};

/// Where on a Weapon an Attachment fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mount {
    Optic,
    Barrel,
    Magazine,
    Underbarrel,
    Grip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attachment {
    RedDotSight,
    AcogSight,
    LongBarrel,
    RapidFire,
    ExtendedMag,
    DualMag,
    Masterkey,
    GrenadeLauncher,
    Flamethrower,
    Grip,
}

impl Attachment {
    pub const ALL: [Attachment; 10] = [
        Attachment::RedDotSight,
        Attachment::AcogSight,
        Attachment::LongBarrel,
        Attachment::RapidFire,
        Attachment::ExtendedMag,
        Attachment::DualMag,
        Attachment::Masterkey,
        Attachment::GrenadeLauncher,
        Attachment::Flamethrower,
        Attachment::Grip,
    ];

    pub fn mount(self) -> Mount {
        match self {
            Attachment::RedDotSight | Attachment::AcogSight => Mount::Optic,
            Attachment::LongBarrel | Attachment::RapidFire => Mount::Barrel,
            Attachment::ExtendedMag | Attachment::DualMag => Mount::Magazine,
            Attachment::Masterkey | Attachment::GrenadeLauncher | Attachment::Flamethrower => Mount::Underbarrel,
            Attachment::Grip => Mount::Grip,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Attachment::RedDotSight => "Red Dot Sight",
            Attachment::AcogSight => "ACOG Sight",
            Attachment::LongBarrel => "Long Barrel",
            Attachment::RapidFire => "Rapid Fire",
            Attachment::ExtendedMag => "Extended Mag",
            Attachment::DualMag => "Dual Mag",
            Attachment::Masterkey => "Masterkey",
            Attachment::GrenadeLauncher => "Grenade Launcher",
            Attachment::Flamethrower => "Flamethrower",
            Attachment::Grip => "Grip",
        }
    }

    /// The Underbarrel it is, if it's one.
    pub fn underbarrel(self) -> Option<Underbarrel> {
        match self {
            Attachment::Masterkey => Some(Underbarrel::Masterkey),
            Attachment::GrenadeLauncher => Some(Underbarrel::GrenadeLauncher),
            Attachment::Flamethrower => Some(Underbarrel::Flamethrower),
            _ => None,
        }
    }

    /// Its entries on the Weapon copy, all from its own source. Dual Mag and the Underbarrels
    /// have none: they change how the Weapon works, not its numbers.
    pub fn modifiers(self) -> Vec<Modifier> {
        let entry = |stat, effect| Modifier {
            stat,
            source: Source(self.name()),
            effect,
        };
        match self {
            Attachment::RedDotSight => vec![entry(Stat::AimedHeadshot, Effect::Mult(RED_DOT_HEADSHOT_MULT))],
            Attachment::AcogSight => vec![
                entry(Stat::AimedPierce, Effect::Flat(ACOG_EXTRA_PIERCE)),
                entry(
                    Stat::AimedPierceDamage,
                    Effect::Flat(ACOG_PIERCE_DAMAGE_FACTOR - PENETRATION_DAMAGE_FACTOR),
                ),
                entry(Stat::AimedFov, Effect::Mult(ACOG_ADS_FOV_MULT)),
            ],
            // Falloff scaled to nothing: max damage at any range.
            Attachment::LongBarrel => vec![entry(Stat::DamageFalloff, Effect::Mult(0.0))],
            Attachment::RapidFire => vec![entry(Stat::FireInterval, Effect::Mult(RAPID_FIRE_INTERVAL_MULT))],
            Attachment::ExtendedMag => vec![
                entry(Stat::MagazineSize, Effect::Mult(EXTENDED_MAG_MAGAZINE_MULT)),
                entry(Stat::ReserveSize, Effect::Mult(EXTENDED_MAG_RESERVE_MULT)),
            ],
            Attachment::Grip => vec![entry(Stat::HipSpread, Effect::Mult(GRIP_HIP_SPREAD_MULT))],
            Attachment::DualMag | Attachment::Masterkey | Attachment::GrenadeLauncher | Attachment::Flamethrower => {
                Vec::new()
            }
        }
    }
}

/// The price of a hold Repack of a copy already Repacked `repacks` times (of either kind).
pub fn attachment_repack_price(repacks: u32) -> u64 {
    REPACK_ATTACHMENT_PRICE.saturating_add(REPACK_PRICE_STEP.saturating_mul(repacks as u64))
}

/// The mounts an Upgraded Weapon has, by its class (Appendix A5). Mustang & Sally, launchers and
/// the wonder Weapons have none, so the hold half of the Repack never shows for them.
pub fn mounts(stats: &WeaponStats) -> &'static [Mount] {
    use Mount::*;
    if stats.id == WeaponId::M1911 {
        // Mustang & Sally is dual-wield on paper, but fires grenades.
        return &[];
    }
    match stats.class {
        WeaponClass::Smg | WeaponClass::Rifle | WeaponClass::Lmg => &[Optic, Barrel, Magazine, Underbarrel, Grip],
        WeaponClass::Pistol => &[Optic, Barrel, Magazine],
        WeaponClass::DualWield => &[Barrel, Magazine],
        WeaponClass::Shotgun => &[Optic, Barrel, Magazine, Grip],
        // The scope fills the optic; see `built_in`.
        WeaponClass::Sniper => &[Optic, Barrel, Magazine, Grip],
        WeaponClass::Launcher
        | WeaponClass::RayGun
        | WeaponClass::Thundergun
        | WeaponClass::Crossbow
        | WeaponClass::BallisticKnife => &[],
    }
}

/// The mount a built-in part fills on an Upgraded Weapon: a sniper's scope, or the AUG-50M3's
/// Masterkey.
pub fn built_in(stats: &WeaponStats) -> Option<Mount> {
    if stats.class == WeaponClass::Sniper {
        Some(Mount::Optic)
    } else {
        built_in_underbarrel(stats.id).map(|_| Mount::Underbarrel)
    }
}

/// The Underbarrel an Upgraded Weapon comes out of Pack-a-Punch with.
pub fn built_in_underbarrel(id: WeaponId) -> Option<Underbarrel> {
    (id == WeaponId::Aug).then_some(Underbarrel::Masterkey)
}

/// The mounts of an Upgraded Weapon still free for an Attachment, in A5 order.
pub fn free_mounts(stats: &WeaponStats, attachments: &[Attachment]) -> Vec<Mount> {
    mounts(stats)
        .iter()
        .copied()
        .filter(|mount| Some(*mount) != built_in(stats))
        .filter(|mount| attachments.iter().all(|attachment| attachment.mount() != *mount))
        .collect()
}

/// The Attachment a hold Repack adds: a free mount picked uniformly by `mount_roll`, then an
/// Attachment in it uniformly by `attachment_roll`, both in [0, 1). `None` with no mount free.
pub fn pick_attachment(free: &[Mount], mount_roll: f32, attachment_roll: f32) -> Option<Attachment> {
    let mount = *pick(free, mount_roll)?;
    let fitting: Vec<_> = Attachment::ALL.into_iter().filter(|attachment| attachment.mount() == mount).collect();
    pick(&fitting, attachment_roll).copied()
}

/// One of `items`, picked uniformly by `roll` in [0, 1).
fn pick<T>(items: &[T], roll: f32) -> Option<&T> {
    let last = items.len().checked_sub(1)?;
    items.get(((roll * items.len() as f32) as usize).min(last))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underbarrel {
    Masterkey,
    GrenadeLauncher,
    Flamethrower,
}

impl Underbarrel {
    /// Its row: what it fires, its ammo and its Reload.
    pub fn stats(self) -> &'static WeaponStats {
        match self {
            Underbarrel::Masterkey => &MASTERKEY,
            Underbarrel::GrenadeLauncher => &GRENADE_LAUNCHER,
            Underbarrel::Flamethrower => &FLAMETHROWER,
        }
    }

    /// The grenade it launches, if it fires a projectile.
    pub fn flight(self) -> Option<Flight> {
        (self == Underbarrel::GrenadeLauncher).then_some(Flight {
            speed: GRENADE_LAUNCHER_SPEED,
            gravity: GRAVITY,
        })
    }
}

/// An Underbarrel on a Weapon copy, with its own ammo. The Flamethrower counts its tank and
/// reserve in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnderbarrelAmmo {
    pub kind: Underbarrel,
    pub magazine: u32,
    pub reserve: u32,
    /// Ticks of Flamethrower recharge built up toward the next whole one.
    recharge: f32,
}

impl UnderbarrelAmmo {
    /// A freshly fitted Underbarrel, fully loaded.
    pub fn full(kind: Underbarrel) -> Self {
        let stats = kind.stats();
        Self {
            kind,
            magazine: stats.magazine,
            reserve: stats.reserve_max,
            recharge: 0.0,
        }
    }

    pub fn is_full(&self) -> bool {
        *self == Self::full(self.kind)
    }

    /// The Flamethrower's tank refills from its reserve while it isn't firing.
    pub fn recharge(&mut self, dt: f32) {
        if self.kind != Underbarrel::Flamethrower {
            return;
        }
        let room = self.kind.stats().magazine.saturating_sub(self.magazine).min(self.reserve);
        if room == 0 {
            self.recharge = 0.0;
            return;
        }
        self.recharge += dt * FLAMETHROWER_RECHARGE_RATE * FLAMETHROWER_TICKS_PER_SEC;
        let ticks = (self.recharge as u32).min(room);
        self.recharge -= ticks as f32;
        self.magazine += ticks;
        self.reserve -= ticks;
    }
}

/// Whether a point is inside the Flamethrower's cone from `origin` along `forward`.
pub fn in_flame(origin: Vec3, forward: Vec3, target: Vec3) -> bool {
    let to_target = target - origin;
    let distance = to_target.length();
    distance <= FLAMETHROWER_RANGE
        && (distance < 1e-4 || forward.angle_between(to_target) <= FLAMETHROWER_CONE_DEGREES.to_radians())
}

/// The Flamethrower went off for one tick: everything in its cone burns.
#[derive(Message, Clone, Copy)]
pub struct FlameFired {
    pub origin: Vec3,
    pub forward: Dir3,
    /// Damage per tick and the Burn it leaves, already resolved through any modifiers.
    pub damage: f32,
    pub burn: Burn,
}

/// The greybox part on the view model for one Attachment.
#[derive(Component)]
struct AttachmentModel(Attachment);

pub struct AttachmentPlugin;

impl Plugin for AttachmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<FlameFired>()
            .add_systems(Update, spawn_attachment_models)
            .add_systems(
                Update,
                (toggle_underbarrel.run_if(hands_free), burn_in_flames, show_attachments)
                    .run_if(in_state(Pause::Running)),
            );
    }
}

/// X toggles between the main gun and its Underbarrel.
fn toggle_underbarrel(mut loadout: Single<&mut Loadout, With<Player>>, input: Res<ButtonInput<KeyCode>>) {
    if input.just_pressed(KeyCode::KeyX) {
        loadout.toggle_underbarrel();
    }
}

/// Every Zombie in sight in the cone takes the tick's damage and is set alight. Flamethrower
/// damage is Paid, never a Headshot, and never hurts the Player.
fn burn_in_flames(
    mut flames: MessageReader<FlameFired>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    spatial: SpatialQuery,
    mut damage: MessageWriter<DamageZombie>,
    mut ignite: MessageWriter<Ignite>,
) {
    for flame in flames.read() {
        for (zombie, transform) in &zombies {
            let at = transform.translation();
            if !in_flame(flame.origin, *flame.forward, at) || !in_sight(&spatial, flame.origin, at) {
                continue;
            }
            damage.write(DamageZombie {
                zombie,
                damage: flame.damage,
                zone: HitZone::Torso,
                attack: Attack::Burn,
            });
            ignite.write(Ignite { zombie, burn: flame.burn });
        }
    }
}

fn spawn_attachment_models(
    mut commands: Commands,
    view_models: Query<Entity, Added<ViewModel>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for view_model in &view_models {
        let dark = materials.add(Color::srgb(0.1, 0.1, 0.11));
        let red = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.1, 0.05),
            emissive: LinearRgba::rgb(12.0, 0.5, 0.2),
            unlit: true,
            ..default()
        });
        let tank = materials.add(Color::srgb(0.6, 0.15, 0.05));
        // The view model is 0.3 long before its class stretches it; its muzzle is at −Z.
        let parts: [(Attachment, Mesh, Handle<StandardMaterial>, Vec3); 10] = [
            (Attachment::RedDotSight, Cuboid::new(0.03, 0.03, 0.04).into(), red, Vec3::new(0.0, 0.07, 0.03)),
            (Attachment::AcogSight, cylinder(0.022, 0.1), dark.clone(), Vec3::new(0.0, 0.075, 0.02)),
            (Attachment::LongBarrel, cylinder(0.012, 0.12), dark.clone(), Vec3::new(0.0, 0.02, -0.2)),
            (Attachment::RapidFire, cylinder(0.02, 0.05), dark.clone(), Vec3::new(0.0, 0.02, -0.15)),
            (Attachment::ExtendedMag, Cuboid::new(0.03, 0.16, 0.05).into(), dark.clone(), Vec3::new(0.0, -0.11, 0.0)),
            (Attachment::DualMag, Cuboid::new(0.06, 0.1, 0.05).into(), dark.clone(), Vec3::new(0.0, -0.09, 0.0)),
            (Attachment::Masterkey, cylinder(0.018, 0.2), dark.clone(), Vec3::new(0.0, -0.065, -0.08)),
            (Attachment::GrenadeLauncher, cylinder(0.025, 0.14), dark.clone(), Vec3::new(0.0, -0.07, -0.08)),
            (Attachment::Flamethrower, cylinder(0.018, 0.2), tank, Vec3::new(0.0, -0.065, -0.08)),
            (Attachment::Grip, Cuboid::new(0.025, 0.07, 0.025).into(), dark, Vec3::new(0.0, -0.08, -0.1)),
        ];
        commands.entity(view_model).with_children(|model| {
            for (attachment, mesh, material, at) in parts {
                let mut transform = Transform::from_translation(at);
                if matches!(mesh_shape(attachment), Shape::Tube) {
                    // Cylinders stand up the Y axis; lay them along the barrel.
                    transform.rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                }
                model.spawn((
                    AttachmentModel(attachment),
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    transform,
                    Visibility::Hidden,
                ));
            }
        });
    }
}

enum Shape {
    Box,
    Tube,
}

fn mesh_shape(attachment: Attachment) -> Shape {
    match attachment {
        Attachment::AcogSight
        | Attachment::LongBarrel
        | Attachment::RapidFire
        | Attachment::Masterkey
        | Attachment::GrenadeLauncher
        | Attachment::Flamethrower => Shape::Tube,
        _ => Shape::Box,
    }
}

fn cylinder(radius: f32, length: f32) -> Mesh {
    Cylinder::new(radius, length).into()
}

/// The parts on the view model are the held Weapon's Attachments, hidden while it's put away.
fn show_attachments(
    player: Single<&Loadout, With<Player>>,
    mut models: Query<(&AttachmentModel, &mut Visibility)>,
) {
    let held = player.held();
    let putting_away = player.swap.as_ref().is_some_and(|swap| swap.elapsed < swap.drop_secs);
    for (model, mut visibility) in &mut models {
        let shown = !putting_away && held.attachments.contains(&model.0);
        visibility.set_if_neq(if shown { Visibility::Inherited } else { Visibility::Hidden });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modifier::{resolve, resolve_count};

    fn upgraded(id: WeaponId) -> &'static WeaponStats {
        id.upgraded()
    }

    #[test]
    fn each_repack_of_a_copy_adds_1000_to_the_attachment_price() {
        assert_eq!(attachment_repack_price(0), 2000);
        assert_eq!(attachment_repack_price(1), 3000);
        assert_eq!(attachment_repack_price(4), 6000);
    }

    #[test]
    fn mounts_follow_the_upgraded_weapons_class() {
        use Mount::*;
        assert_eq!(mounts(upgraded(WeaponId::Mp40)), [Optic, Barrel, Magazine, Underbarrel, Grip]);
        assert_eq!(mounts(upgraded(WeaponId::M14)), [Optic, Barrel, Magazine, Underbarrel, Grip]);
        assert_eq!(mounts(upgraded(WeaponId::Rpk)), [Optic, Barrel, Magazine, Underbarrel, Grip]);
        assert_eq!(mounts(upgraded(WeaponId::Python)), [Optic, Barrel, Magazine]);
        assert_eq!(mounts(upgraded(WeaponId::Cz75DualWield)), [Barrel, Magazine]);
        assert_eq!(mounts(upgraded(WeaponId::Hs10)), [Barrel, Magazine]);
        assert_eq!(mounts(upgraded(WeaponId::Olympia)), [Optic, Barrel, Magazine, Grip]);
        for none in [
            WeaponId::M1911,
            WeaponId::M72Law,
            WeaponId::ChinaLake,
            WeaponId::RayGun,
            WeaponId::Thundergun,
            WeaponId::Crossbow,
            WeaponId::BallisticKnife,
        ] {
            assert!(mounts(upgraded(none)).is_empty(), "{}", upgraded(none).name);
            assert!(free_mounts(upgraded(none), &[]).is_empty());
        }
    }

    #[test]
    fn built_in_parts_fill_their_mounts() {
        use Mount::*;
        assert_eq!(free_mounts(upgraded(WeaponId::L96a1), &[]), [Barrel, Magazine, Grip], "the scope fills the optic");
        assert_eq!(free_mounts(upgraded(WeaponId::Dragunov), &[]), [Barrel, Magazine, Grip]);
        assert_eq!(free_mounts(upgraded(WeaponId::Aug), &[]), [Optic, Barrel, Magazine, Grip], "the Masterkey");
        assert_eq!(built_in_underbarrel(WeaponId::Aug), Some(super::Underbarrel::Masterkey));
        assert_eq!(built_in_underbarrel(WeaponId::Famas), None);
    }

    #[test]
    fn one_attachment_per_mount() {
        use Mount::*;
        let held = [Attachment::RapidFire, Attachment::Flamethrower];
        assert_eq!(free_mounts(upgraded(WeaponId::Mp40), &held), [Optic, Magazine, Grip]);
        let all = [
            Attachment::AcogSight,
            Attachment::LongBarrel,
            Attachment::DualMag,
            Attachment::GrenadeLauncher,
            Attachment::Grip,
        ];
        assert!(free_mounts(upgraded(WeaponId::Mp40), &all).is_empty());
        assert_eq!(pick_attachment(&[], 0.5, 0.5), None);
    }

    #[test]
    fn the_mount_is_picked_uniformly_then_the_attachment_in_it() {
        use Mount::*;
        let free = [Optic, Underbarrel, Grip];
        // A third of the mount roll each.
        assert_eq!(pick_attachment(&free, 0.0, 0.0), Some(Attachment::RedDotSight));
        assert_eq!(pick_attachment(&free, 0.32, 0.99), Some(Attachment::AcogSight));
        assert_eq!(pick_attachment(&free, 0.34, 0.0), Some(Attachment::Masterkey));
        assert_eq!(pick_attachment(&free, 0.5, 0.5), Some(Attachment::GrenadeLauncher));
        assert_eq!(pick_attachment(&free, 0.66, 0.99), Some(Attachment::Flamethrower));
        assert_eq!(pick_attachment(&free, 0.99, 0.5), Some(Attachment::Grip));
        // A roll of exactly 1 still lands on the last.
        assert_eq!(pick_attachment(&free, 1.0, 1.0), Some(Attachment::Grip));
    }

    #[test]
    fn a_mount_with_more_attachments_doesnt_win_more_often() {
        use Mount::*;
        let free = [Underbarrel, Grip];
        let rolls = 1000;
        let grips = (0..rolls)
            .filter(|i| pick_attachment(&free, *i as f32 / rolls as f32, 0.5) == Some(Attachment::Grip))
            .count();
        assert_eq!(grips, rolls / 2, "half the mount rolls, though the Underbarrel has three Attachments");
    }

    #[test]
    fn every_attachment_sits_in_one_mount_of_the_table() {
        let count = |mount| Attachment::ALL.iter().filter(|a| a.mount() == mount).count();
        assert_eq!(count(Mount::Optic), 2);
        assert_eq!(count(Mount::Barrel), 2);
        assert_eq!(count(Mount::Magazine), 2);
        assert_eq!(count(Mount::Underbarrel), 3);
        assert_eq!(count(Mount::Grip), 1);
        for attachment in Attachment::ALL {
            assert_eq!(attachment.underbarrel().is_some(), attachment.mount() == Mount::Underbarrel);
        }
    }

    fn perk(stat: Stat, mult: f32) -> Modifier {
        Modifier {
            stat,
            source: Source("Perk"),
            effect: Effect::Mult(mult),
        }
    }

    #[test]
    fn rapid_fire_multiplies_with_double_tap() {
        let mut entries = Attachment::RapidFire.modifiers();
        entries.push(perk(Stat::FireInterval, DOUBLE_TAP_FIRE_INTERVAL_MULT));
        assert!((resolve(Stat::FireInterval, 1.0, &entries) - 0.6).abs() < 1e-5);
    }

    #[test]
    fn extended_mag_grows_the_magazine_and_reserve_rounding_up() {
        let entries = Attachment::ExtendedMag.modifiers();
        // Mnesia: 16 + 192.
        assert_eq!(resolve_count(Stat::MagazineSize, 16, &entries), 24);
        assert_eq!(resolve_count(Stat::ReserveSize, 192, &entries), 240);
        // The Afterburner's 64 + 192, and a Reserve that doesn't divide evenly.
        assert_eq!(resolve_count(Stat::MagazineSize, 45, &entries), 68);
        assert_eq!(resolve_count(Stat::ReserveSize, 225, &entries), 282);
    }

    #[test]
    fn the_optics_change_aimed_shots() {
        let red_dot = Attachment::RedDotSight.modifiers();
        assert_eq!(resolve(Stat::AimedHeadshot, 1.0, &red_dot), 1.5);
        let acog = Attachment::AcogSight.modifiers();
        assert_eq!(resolve(Stat::AimedPierce, 0.0, &acog), 2.0);
        assert_eq!(resolve(Stat::AimedPierceDamage, PENETRATION_DAMAGE_FACTOR, &acog), 1.0);
        assert!((resolve(Stat::AimedFov, ADS_FOV_IRON, &acog) - 0.77 * 0.55).abs() < 1e-5);
        // Neither touches anything else.
        assert_eq!(resolve(Stat::Damage, 100.0, &red_dot), 100.0);
        assert_eq!(resolve(Stat::AimedHeadshot, 1.0, &acog), 1.0);
    }

    #[test]
    fn long_barrel_removes_falloff_and_grip_tightens_the_hip() {
        assert_eq!(resolve(Stat::DamageFalloff, 1.0, &Attachment::LongBarrel.modifiers()), 0.0);
        let grip = Attachment::Grip.modifiers();
        assert!((resolve(Stat::HipSpread, 5.0, &grip) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn each_attachment_is_its_own_source() {
        // Rapid Fire and a second ×0.8 from elsewhere multiply rather than add.
        let mut entries = Attachment::RapidFire.modifiers();
        entries.push(perk(Stat::FireInterval, 0.8));
        assert!((resolve(Stat::FireInterval, 1.0, &entries) - 0.64).abs() < 1e-5);
        assert!(Attachment::DualMag.modifiers().is_empty(), "Dual Mag's Reload is the one exception");
    }

    #[test]
    fn underbarrels_have_their_own_ammo() {
        let masterkey = UnderbarrelAmmo::full(Underbarrel::Masterkey);
        assert_eq!((masterkey.magazine, masterkey.reserve), (6, 30));
        assert_eq!(MASTERKEY.pellets, 8);
        assert_eq!(MASTERKEY.damage_at(0.0), 240.0);
        assert_eq!(MASTERKEY.headshot, 1.0, "shotgun rules");
        let launcher = UnderbarrelAmmo::full(Underbarrel::GrenadeLauncher);
        assert_eq!((launcher.magazine, launcher.reserve), (1, 5));
        let flame = UnderbarrelAmmo::full(Underbarrel::Flamethrower);
        assert_eq!((flame.magazine, flame.reserve), (100, 300), "10 s + 30 s of ticks");
    }

    #[test]
    fn the_flamethrower_recharges_one_second_per_second_idle() {
        let mut flame = UnderbarrelAmmo::full(Underbarrel::Flamethrower);
        flame.magazine = 40;
        flame.recharge(2.05);
        assert_eq!((flame.magazine, flame.reserve), (60, 280));
        flame.recharge(10.0);
        assert_eq!((flame.magazine, flame.reserve), (100, 240), "never past a full tank");
        flame.magazine = 0;
        flame.reserve = 3;
        flame.recharge(10.0);
        assert_eq!((flame.magazine, flame.reserve), (3, 0), "nothing left to recharge from");

        let mut shells = UnderbarrelAmmo::full(Underbarrel::Masterkey);
        shells.magazine = 0;
        shells.recharge(10.0);
        assert_eq!(shells.magazine, 0, "only the Flamethrower recharges");
    }

    #[test]
    fn the_flame_reaches_6_m_in_a_cone() {
        let origin = Vec3::ZERO;
        let forward = Vec3::NEG_Z;
        assert!(in_flame(origin, forward, Vec3::new(0.0, 0.0, -5.9)));
        assert!(!in_flame(origin, forward, Vec3::new(0.0, 0.0, -6.1)));
        assert!(in_flame(origin, forward, Vec3::new(0.5, 0.0, -3.0)));
        assert!(!in_flame(origin, forward, Vec3::new(2.0, 0.0, -3.0)), "outside the cone");
        assert!(!in_flame(origin, forward, Vec3::new(0.0, 0.0, 3.0)), "behind");
    }
}
