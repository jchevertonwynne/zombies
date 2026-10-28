//! Wall Weapons: chalk outlines that sell a Weapon, and then ammo for it at half price, or at
//! `PAP_AMMO_PRICE` for an Upgraded copy.

use bevy::prelude::*;

use crate::arsenal::{Availability, WeaponId};
use crate::buy::{Buyable, Purchased, Refused};
use crate::gun_model::GunModels;
use crate::hands::ammo_buy_pose;
use crate::modifier::Modifiers;
use crate::player::{Player, PlayerCamera};
use crate::state::{InRun, Pause};
use crate::tuning::*;
use crate::weapon::Loadout;

/// A chalk outline selling this Weapon.
#[derive(Component)]
pub struct WallWeapon(pub WeaponId);

/// What a chalk sells right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    /// The Weapon itself, at its wall price.
    Weapon(u64),
    /// Ammo for the Weapon the Player already carries. Refused when its ammo is full.
    Ammo { price: u64, full: bool },
}

impl Offer {
    pub fn price(self) -> u64 {
        match self {
            Offer::Weapon(price) | Offer::Ammo { price, .. } => price,
        }
    }

    pub fn refused(self) -> bool {
        matches!(self, Offer::Ammo { full: true, .. })
    }
}

/// A Weapon's price on the wall, if it is a Wall Weapon.
pub fn wall_price(id: WeaponId) -> Option<u64> {
    match id.stats().availability {
        Availability::Wall(price) => Some(price),
        _ => None,
    }
}

/// Ammo costs half the Weapon's wall price, rounded up to the next `AMMO_PRICE_STEP`.
pub fn ammo_price(wall_price: u64) -> u64 {
    wall_price.div_ceil(2).div_ceil(AMMO_PRICE_STEP) * AMMO_PRICE_STEP
}

/// The copy of a chalk's Weapon the Player carries, as far as its ammo buy cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carried {
    pub upgraded: bool,
    /// Whether its Magazine and Reserve Ammo are both full.
    pub full: bool,
}

/// What a chalk sells, given the Weapon's wall price and the copy the Player carries, if any.
pub fn offer(wall_price: u64, carried: Option<Carried>) -> Offer {
    match carried {
        None => Offer::Weapon(wall_price),
        Some(Carried { upgraded, full }) => Offer::Ammo {
            price: if upgraded { PAP_AMMO_PRICE } else { ammo_price(wall_price) },
            full,
        },
    }
}

/// How far along (0 to 1) a bought Wall Weapon's copy is `t` of the way (0 to 1) through its
/// lift-off, and how big it still is (1 whole to 0 gone). It eases away from the chalk and shrinks
/// out over the last of it, so it never jumps.
pub fn lift_off(t: f32) -> (f32, f32) {
    let ease = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let t = t.clamp(0.0, 1.0);
    (ease(t), 1.0 - ease((t - 0.7) / 0.3))
}

/// A copy of a bought Wall Weapon's world model, lifting off its chalk toward the Player. Purely
/// a world-space effect; the view model does its own switch.
#[derive(Component)]
struct LiftingOff {
    elapsed: f32,
    from: Vec3,
    to: Vec3,
}

/// Plays on the held Weapon while an ammo buy's pose goes on: seconds in, if one is.
#[derive(Resource, Default)]
pub struct AmmoBuy(Option<f32>);

impl AmmoBuy {
    /// The held Weapon's pose now: how far it tips out of the way, and where the magazine is, from
    /// its seat. At rest when no ammo buy is going on.
    pub fn pose(&self) -> (f32, f32) {
        self.0.map_or((0.0, 0.0), |elapsed| ammo_buy_pose(elapsed / AMMO_BUY_SECS))
    }
}

pub struct WallWeaponPlugin;

impl Plugin for WallWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AmmoBuy>().add_systems(
            Update,
            (buy_wall_weapons, price_wall_weapons, lift_off_wall_weapons)
                .chain()
                .run_if(in_state(Pause::Running)),
        );
    }
}

/// Moves each lifting copy toward the Player and removes it when it has gone, and runs out the
/// ammo buy's pose.
fn lift_off_wall_weapons(
    mut commands: Commands,
    mut lifting: Query<(Entity, &mut LiftingOff, &mut Transform)>,
    mut ammo_buy: ResMut<AmmoBuy>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut lift, mut transform) in &mut lifting {
        lift.elapsed += dt;
        if lift.elapsed >= WALL_LIFT_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        let (along, size) = lift_off(lift.elapsed / WALL_LIFT_SECS);
        transform.translation = lift.from.lerp(lift.to, along);
        transform.scale = Vec3::splat(size);
    }
    if let Some(elapsed) = ammo_buy.0 {
        ammo_buy.0 = (elapsed + dt < AMMO_BUY_SECS).then_some(elapsed + dt);
    }
}

/// Each chalk owns its Buyable, so it keeps the price, Prompt and refusal current.
fn price_wall_weapons(
    mut commands: Commands,
    player: Single<(&Loadout, &Modifiers), With<Player>>,
    mut walls: Query<(Entity, &WallWeapon, &mut Buyable, Has<Refused>)>,
) {
    let (loadout, modifiers) = *player;
    for (entity, wall, mut buyable, refused) in &mut walls {
        let Some(price) = wall_price(wall.0) else { continue };
        let slot = loadout.slot_of(wall.0);
        let carried = slot.map(|slot| Carried {
            upgraded: loadout.slot(slot).is_some_and(|weapon| weapon.upgraded),
            full: loadout.ammo_full(slot, &modifiers.0),
        });
        let offer = offer(price, carried);
        // An Upgraded copy's ammo goes by its upgraded name.
        let name = slot.and_then(|slot| loadout.slot(slot)).map_or(wall.0.stats().name, |weapon| weapon.stats().name);
        let prompt = match offer {
            Offer::Weapon(_) => format!("Press F to buy {name}"),
            Offer::Ammo { .. } => format!("Press F to buy ammo for {name}"),
        };
        if buyable.prompt != prompt {
            buyable.prompt = prompt;
        }
        if buyable.price != Some(offer.price()) {
            buyable.price = Some(offer.price());
        }
        match (offer.refused(), refused) {
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

/// A bought chalk gives its Weapon, or refills it if the Player already carries it.
fn buy_wall_weapons(
    mut commands: Commands,
    mut purchases: MessageReader<Purchased>,
    walls: Query<(&WallWeapon, &Transform, &Buyable)>,
    player: Single<(&mut Loadout, &Modifiers), With<Player>>,
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    guns: Res<GunModels>,
    mut ammo_buy: ResMut<AmmoBuy>,
) {
    let (mut loadout, modifiers) = player.into_inner();
    for Purchased(entity) in purchases.read() {
        let Ok((wall, chalk, buyable)) = walls.get(*entity) else { continue };
        match loadout.slot_of(wall.0) {
            Some(slot) => {
                loadout.refill(slot, &modifiers.0);
                ammo_buy.0 = Some(0.0);
            }
            // Into the empty slot, or in place of the Weapon in hand. Either way a Reload ends.
            None => {
                loadout.give(wall.0);
                // A copy of it lifts off the chalk toward the Player, lying along the wall.
                let from = chalk.translation + (buyable.use_point - chalk.translation) * 0.5;
                let toward = (camera.translation() - from).normalize_or_zero();
                commands.spawn((
                    InRun,
                    LiftingOff { elapsed: 0.0, from, to: from + toward * WALL_LIFT_REACH },
                    Mesh3d(guns.world(wall.0)),
                    MeshMaterial3d(guns.material.clone()),
                    Transform::from_translation(from).with_rotation(chalk.rotation),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lift_off_starts_whole_at_the_chalk_and_ends_gone_without_jumping() {
        assert_eq!(lift_off(0.0), (0.0, 1.0));
        assert_eq!(lift_off(1.0), (1.0, 0.0));
        let mut last = lift_off(0.0);
        for i in 1..=200 {
            let now = lift_off(i as f32 / 200.0);
            assert!(now.0 >= last.0 && now.1 <= last.1, "only moves on and shrinks, at step {i}");
            assert!((now.0 - last.0).abs() < 0.03 && (now.1 - last.1).abs() < 0.03, "jump at step {i}");
            last = now;
        }
    }

    #[test]
    fn an_ammo_buy_pose_is_at_rest_when_none_is_playing() {
        assert_eq!(AmmoBuy::default().pose(), (0.0, 0.0));
        let mid = AmmoBuy(Some(AMMO_BUY_SECS * 0.3)).pose();
        assert!(mid.0 > 0.0 && mid.1 < 0.0);
    }

    #[test]
    fn ammo_costs_half_the_wall_price() {
        assert_eq!(ammo_price(500), 250);
        assert_eq!(ammo_price(1000), 500);
        assert_eq!(ammo_price(1500), 750);
        assert_eq!(ammo_price(1200), 600);
    }

    #[test]
    fn ammo_price_rounds_up_to_ten() {
        assert_eq!(ammo_price(750), 380);
        assert_eq!(ammo_price(1010), 510);
        assert_eq!(ammo_price(1), 10);
        assert_eq!(ammo_price(20), 10);
    }

    #[test]
    fn the_five_wall_weapons_sell_at_the_spec_prices() {
        let prices = [
            (WeaponId::M14, 500, 250),
            (WeaponId::Olympia, 500, 250),
            (WeaponId::Mp40, 1000, 500),
            (WeaponId::TrenchGun, 1500, 750),
            (WeaponId::Ak74u, 1200, 600),
        ];
        for (id, price, ammo) in prices {
            assert_eq!(wall_price(id), Some(price), "{id:?}");
            assert_eq!(ammo_price(price), ammo, "{id:?}");
        }
        assert_eq!(wall_price(WeaponId::M1911), None);
        assert_eq!(wall_price(WeaponId::RayGun), None);
    }

    #[test]
    fn a_weapon_not_carried_is_sold_at_its_wall_price() {
        assert_eq!(offer(1000, None), Offer::Weapon(1000));
        assert!(!offer(1000, None).refused());
    }

    const BASE: Carried = Carried {
        upgraded: false,
        full: false,
    };

    #[test]
    fn a_carried_weapon_becomes_an_ammo_buy() {
        let ammo = offer(1000, Some(BASE));
        assert_eq!(ammo, Offer::Ammo { price: 500, full: false });
        assert_eq!(ammo.price(), 500);
        assert!(!ammo.refused());
    }

    #[test]
    fn a_full_ammo_buy_is_refused() {
        let full = offer(1500, Some(Carried { full: true, ..BASE }));
        assert_eq!(full.price(), 750, "the Prompt still shows the ammo price");
        assert!(full.refused());
    }

    #[test]
    fn an_upgraded_copys_ammo_costs_4500_at_any_chalk() {
        let upgraded = Carried { upgraded: true, ..BASE };
        for price in [500, 1000, 1200, 1500] {
            assert_eq!(offer(price, Some(upgraded)), Offer::Ammo { price: 4500, full: false });
        }
        let full = offer(500, Some(Carried { full: true, ..upgraded }));
        assert_eq!(full.price(), 4500);
        assert!(full.refused());
    }
}
