//! The Weapon table: every Weapon is one entry of plain data. The numbers live in the tuning
//! module (Appendix A1); this module holds their shape.

use crate::tuning::{
    PENETRATION_DAMAGE_FACTOR, PENETRATION_LARGE, PENETRATION_MEDIUM, PENETRATION_SMALL, UPGRADED_WEAPONS,
    WEAPON_HANDLING, WEAPONS,
};
use crate::zombie::HitZone;

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

    /// Its hand-written Pack-a-Punch form (Appendix A3). Every Weapon here has one: Equipment, the
    /// Monkey Bomb, the Knife and the Bowie aren't Weapons in this table.
    pub fn upgraded(self) -> &'static WeaponStats {
        let row = &UPGRADED_WEAPONS[self as usize];
        debug_assert_eq!(row.id, self, "the upgraded Weapon table is out of order");
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
    /// Degrees of spread from the hip: fresh, and fully bloomed.
    pub hip_spread: (f32, f32),
    /// Degrees of spread fully aimed.
    pub aimed_spread: f32,
    /// The factor aiming puts on the FOV (1 aims without zooming), or `None` if it can't aim.
    pub ads_fov: Option<f32>,
    /// Seconds after a sprint before the Weapon can fire or aim.
    pub sprint_out_secs: f32,
    /// The factor carrying it puts on move speed.
    pub move_speed: f32,
    /// Where damage starts falling from max and where it reaches min.
    pub falloff: Falloff,
}

/// The distances, in metres, over which damage falls from max to min.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Falloff {
    pub start: f32,
    pub end: f32,
}

impl Falloff {
    /// No falloff: max damage at any range.
    pub const NONE: Falloff = Falloff {
        start: f32::INFINITY,
        end: f32::INFINITY,
    };

    /// Damage at a distance: `max` up to `start`, then straight down to `min` at `end` and beyond.
    pub fn damage(self, max: f32, min: f32, distance: f32) -> f32 {
        if distance <= self.start {
            max
        } else if distance >= self.end {
            min
        } else {
            let t = (distance - self.start) / (self.end - self.start);
            max + (min - max) * t
        }
    }
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

impl Penetration {
    /// How many Zombies past the first a bullet can hit.
    pub fn extra_zombies(self) -> u32 {
        match self {
            Penetration::None => 0,
            Penetration::Small => PENETRATION_SMALL,
            Penetration::Medium => PENETRATION_MEDIUM,
            Penetration::Large => PENETRATION_LARGE,
        }
    }
}

/// The damage factor on a bullet that has already passed through `passed` Zombies.
pub fn penetration_factor(passed: u32) -> f32 {
    PENETRATION_DAMAGE_FACTOR.powi(passed as i32)
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

impl Splash {
    /// Splash damage at a distance from the impact: `max` at the centre down to `min` at the edge,
    /// nothing beyond.
    pub fn damage(self, distance: f32) -> Option<f32> {
        (distance <= self.radius).then(|| self.max + (self.min - self.max) * distance / self.radius)
    }
}

/// Burn damage over time: `damage` every `tick_secs`, for `secs` after the last ignition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Burn {
    pub damage: f32,
    pub tick_secs: f32,
    pub secs: f32,
}

/// One row of the Weapon table. Upgraded rows keep their base Weapon's `id`.
#[derive(Debug, Clone, Copy)]
pub struct WeaponStats {
    pub id: WeaponId,
    pub name: &'static str,
    pub class: WeaponClass,
    pub fire: FireMode,
    pub availability: Availability,
    /// Damage per bullet (pellet, or impact) up close, falling to `damage_min` at range.
    pub damage_max: f32,
    pub damage_min: f32,
    /// Pellets per shot: 8 for shotguns, otherwise 1.
    pub pellets: u32,
    pub splash: Option<Splash>,
    /// Burn each hit sets on the Zombie, if any.
    pub burn: Option<Burn>,
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
    /// Neck and torso damage multipliers (only snipers have them).
    pub neck: f32,
    pub torso: f32,
    pub penetration: Penetration,
    /// This Weapon's own handling where it differs from its class.
    pub hip_spread: Option<(f32, f32)>,
    pub aimed_spread: Option<f32>,
    pub ads_fov: Option<f32>,
    pub falloff: Option<Falloff>,
}

impl WeaponStats {
    /// Reserve Ammo when first got.
    pub fn starting_reserve(&self) -> u32 {
        self.reserve_start.unwrap_or(self.reserve_max)
    }

    pub fn handling(&self) -> &'static Handling {
        self.class.handling()
    }

    /// The damage multiplier for a bullet landing in a hit zone.
    pub fn zone_multiplier(&self, zone: HitZone) -> f32 {
        match zone {
            HitZone::Head => self.headshot,
            HitZone::Neck => self.neck,
            HitZone::Torso => self.torso,
            HitZone::Limb => 1.0,
        }
    }

    /// Unmodified damage per bullet at a distance, before the hit zone and penetration.
    pub fn damage_at(&self, distance: f32) -> f32 {
        self.falloff
            .unwrap_or(self.handling().falloff)
            .damage(self.damage_max, self.damage_min, distance)
    }

    /// Hip spread in degrees, fresh and fully bloomed.
    pub fn hip_spread(&self) -> (f32, f32) {
        self.hip_spread.unwrap_or(self.handling().hip_spread)
    }

    pub fn aimed_spread(&self) -> f32 {
        self.aimed_spread.unwrap_or(self.handling().aimed_spread)
    }

    /// The factor aiming puts on the FOV, or `None` if this Weapon can't aim.
    pub fn ads_fov(&self) -> Option<f32> {
        self.handling().ads_fov.map(|fov| self.ads_fov.unwrap_or(fov))
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
