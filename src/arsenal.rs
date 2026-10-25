//! The Weapon table: every Weapon is one entry of plain data. The numbers live in the tuning
//! module (Appendix A1); this module holds their shape.

use crate::tuning::{WEAPON_HANDLING, WEAPONS};

/// Every base Weapon in Appendix A1. The Monkey Bomb is a Tactical, so it isn't here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponId {
    M1911,
    Olympia,
    M14,
    Mp40,
    TrenchGun,
    Ak74u,
    Cz75,
    Cz75DualWield,
    Python,
    Spectre,
    G11,
    Famas,
    Aug,
    Commando,
    Galil,
    FnFal,
    Hs10,
    Spas12,
    Dragunov,
    L96a1,
    Rpk,
    Hk21,
    M72Law,
    ChinaLake,
    RayGun,
    Thundergun,
    Crossbow,
    BallisticKnife,
}

impl WeaponId {
    pub fn stats(self) -> &'static WeaponStats {
        let row = &WEAPONS[self as usize];
        debug_assert_eq!(row.id, self, "the Weapon table is out of order");
        row
    }
}

/// The kind of Weapon, which decides how it handles (Appendix A2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponClass {
    Pistol,
    DualWield,
    Smg,
    Rifle,
    Shotgun,
    Sniper,
    Lmg,
    Launcher,
    RayGun,
    Thundergun,
    Crossbow,
    BallisticKnife,
}

impl WeaponClass {
    pub fn handling(self) -> &'static Handling {
        let row = &WEAPON_HANDLING[self as usize];
        debug_assert_eq!(row.class, self, "the handling table is out of order");
        row
    }
}

/// Per-class handling from Appendix A2.
#[derive(Debug)]
pub struct Handling {
    pub class: WeaponClass,
    /// Seconds to bring the Weapon up.
    pub raise_secs: f32,
    /// Seconds to put the Weapon away.
    pub drop_secs: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireMode {
    /// One shot per click. Pumps, bolts and break-actions are semi with a long fire interval.
    Semi,
    /// Fires while held.
    Auto,
    /// This many shots per click.
    Burst(u32),
}

/// How a Weapon is got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// The Player starts with it.
    Start,
    /// A Wall Weapon at this price.
    Wall(u64),
    /// A Box Weapon.
    Box,
    /// A Box Weapon of which at most one can be in play.
    LimitedBox,
}

/// How many extra Zombies a bullet passes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Penetration {
    None,
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReloadTime {
    /// Seconds with bullets still in the Magazine, and from empty.
    Magazine { tactical: f32, empty: f32 },
    /// Loaded a bullet (or shell) at a time: `start + per_bullet × loaded + end`.
    PerBullet {
        start: f32,
        per_bullet: f32,
        end: f32,
    },
    /// Python: bullets left are swapped one by one, an empty cylinder takes a speedloader.
    Revolver {
        start: f32,
        per_bullet: f32,
        empty: f32,
    },
}

impl ReloadTime {
    /// The unmodified seconds a Reload takes, given the bullets left and the bullets it will load.
    pub fn secs(self, magazine: u32, loading: u32) -> f32 {
        match self {
            ReloadTime::Magazine { tactical, empty } => {
                if magazine == 0 {
                    empty
                } else {
                    tactical
                }
            }
            ReloadTime::PerBullet {
                start,
                per_bullet,
                end,
            } => start + per_bullet * loading as f32 + end,
            ReloadTime::Revolver {
                start,
                per_bullet,
                empty,
            } => {
                if magazine == 0 {
                    empty
                } else {
                    start + per_bullet * loading as f32
                }
            }
        }
    }
}

/// Splash damage around an explosive's impact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Splash {
    pub max: f32,
    pub min: f32,
    pub radius: f32,
}

/// One row of the Weapon table.
#[derive(Debug)]
pub struct WeaponStats {
    pub id: WeaponId,
    pub name: &'static str,
    pub class: WeaponClass,
    pub fire: FireMode,
    #[allow(
        dead_code,
        reason = "Wall Weapons and the Mystery Box arrive in later slices"
    )]
    pub availability: Availability,
    /// Damage per bullet (pellet, or impact) up close, falling to `damage_min` at range.
    pub damage_max: f32,
    #[allow(dead_code, reason = "falloff arrives with the combat model")]
    pub damage_min: f32,
    /// Pellets per shot: 8 for shotguns, otherwise 1.
    #[allow(dead_code, reason = "pellets arrive with the combat model")]
    pub pellets: u32,
    #[allow(dead_code, reason = "explosives arrive with the Equipment slice")]
    pub splash: Option<Splash>,
    /// Bullets a full Magazine holds. Dual-wield counts both guns.
    pub magazine: u32,
    /// The most Reserve Ammo the Weapon carries.
    pub reserve_max: u32,
    /// Reserve Ammo when first got, if not full.
    pub reserve_start: Option<u32>,
    /// Seconds between shots (60 / RPM, or the rechamber time).
    pub fire_interval: f32,
    pub reload: ReloadTime,
    /// Headshot damage multiplier.
    pub headshot: f32,
    #[allow(dead_code, reason = "penetration arrives with the combat model")]
    pub penetration: Penetration,
}

impl WeaponStats {
    /// Reserve Ammo when first got.
    pub fn starting_reserve(&self) -> u32 {
        self.reserve_start.unwrap_or(self.reserve_max)
    }

    pub fn handling(&self) -> &'static Handling {
        self.class.handling()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_sits_at_its_own_id() {
        for (i, row) in WEAPONS.iter().enumerate() {
            assert_eq!(row.id as usize, i, "{} is out of place", row.name);
        }
        assert_eq!(WEAPONS.len(), WeaponId::BallisticKnife as usize + 1);
        for (i, row) in WEAPON_HANDLING.iter().enumerate() {
            assert_eq!(row.class as usize, i);
        }
        assert_eq!(
            WEAPON_HANDLING.len(),
            WeaponClass::BallisticKnife as usize + 1
        );
    }

    #[test]
    fn the_m1911_has_its_bo1_values() {
        let m1911 = WeaponId::M1911.stats();
        assert_eq!(m1911.damage_max, 20.0);
        assert_eq!(m1911.magazine, 8);
        assert_eq!(m1911.starting_reserve(), 32);
        assert_eq!(m1911.reserve_max, 80);
        assert_eq!(m1911.reload.secs(3, 5), 1.63);
        assert_eq!(m1911.reload.secs(0, 8), 1.8);
        assert_eq!(m1911.availability, Availability::Start);
    }

    #[test]
    fn every_weapon_carries_finite_reserve_ammo() {
        for row in &WEAPONS {
            assert!(
                row.reserve_max > 0 && row.starting_reserve() <= row.reserve_max,
                "{}",
                row.name
            );
        }
    }

    #[test]
    fn shells_load_one_at_a_time() {
        // Trench Gun: 0.9 + 0.6 a shell + 0.95.
        let reload = WeaponId::TrenchGun.stats().reload;
        assert!((reload.secs(4, 2) - (0.9 + 1.2 + 0.95)).abs() < 1e-5);
    }

    #[test]
    fn box_weapons_are_never_wall_weapons() {
        let walls = WEAPONS
            .iter()
            .filter(|row| matches!(row.availability, Availability::Wall(_)));
        let names: Vec<_> = walls.map(|row| row.name).collect();
        assert_eq!(names, ["Olympia", "M14", "MP40", "Trench Gun", "AK-74u"]);
    }
}
