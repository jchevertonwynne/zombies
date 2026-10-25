//! Wall Weapons: chalk outlines that sell a Weapon, and then ammo for it at half price.

use bevy::prelude::*;

use crate::arsenal::{Availability, WeaponId};
use crate::buy::{Buyable, Purchased, Refused};
use crate::modifier::Modifiers;
use crate::player::Player;
use crate::state::Pause;
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

/// What a chalk sells, given the Weapon's wall price and, if the Player carries that Weapon,
/// whether its ammo is full.
pub fn offer(wall_price: u64, carried_full: Option<bool>) -> Offer {
    match carried_full {
        None => Offer::Weapon(wall_price),
        Some(full) => Offer::Ammo {
            price: ammo_price(wall_price),
            full,
        },
    }
}

pub struct WallWeaponPlugin;

impl Plugin for WallWeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (buy_wall_weapons, price_wall_weapons)
                .chain()
                .run_if(in_state(Pause::Running)),
        );
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
        let carried_full = loadout.slot_of(wall.0).map(|slot| loadout.ammo_full(slot, &modifiers.0));
        let offer = offer(price, carried_full);
        let name = wall.0.stats().name;
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
    mut purchases: MessageReader<Purchased>,
    walls: Query<&WallWeapon>,
    player: Single<(&mut Loadout, &Modifiers), With<Player>>,
) {
    let (mut loadout, modifiers) = player.into_inner();
    for Purchased(entity) in purchases.read() {
        let Ok(wall) = walls.get(*entity) else { continue };
        match loadout.slot_of(wall.0) {
            Some(slot) => loadout.refill(slot, &modifiers.0),
            // Into the empty slot, or in place of the Weapon in hand. Either way a Reload ends.
            None => loadout.give(wall.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_carried_weapon_becomes_an_ammo_buy() {
        let ammo = offer(1000, Some(false));
        assert_eq!(ammo, Offer::Ammo { price: 500, full: false });
        assert_eq!(ammo.price(), 500);
        assert!(!ammo.refused());
    }

    #[test]
    fn a_full_ammo_buy_is_refused() {
        let full = offer(1500, Some(true));
        assert_eq!(full.price(), 750, "the Prompt still shows the ammo price");
        assert!(full.refused());
    }
}
