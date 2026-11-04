//! The combat model's rules: what one bullet hits on its way through a line of Zombies, how
//! much damage each hit does, and where spread sends it.

use bevy::math::{Dir3, Vec3};
use bevy::prelude::{Entity, Message};

use crate::tuning::PENETRATION_DAMAGE_FACTOR;
use crate::zombie::{Attack, HitZone};

/// A Player's hitscan bullet or Knife landed on a Zombie. The shooter judges the hit; the handler
/// turns it into damage the Player is credited for.
#[derive(Message, Debug, Clone, Copy)]
pub struct HitReport {
    pub player: Entity,
    pub zombie: Entity,
    pub zone: HitZone,
    pub damage: f32,
    /// A bullet, or the Knife.
    pub attack: Attack,
}

/// Something a bullet's line crossed, nearest first: a Zombie's hit area, or the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Crossing<Z> {
    Zombie { zombie: Z, zone: HitZone, distance: f32 },
    World,
}

/// One Zombie a bullet hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulletHit<Z> {
    pub zombie: Z,
    pub zone: HitZone,
    pub distance: f32,
    /// Zombies the bullet had already passed through.
    pub passed: u32,
}

/// The Zombies a bullet hits, given everything its line crosses in order: the first Zombie and up
/// to `extra` more, stopping at the world. A Zombie counts once, in the first hit area crossed.
pub fn pierce<Z: PartialEq + Copy>(crossings: impl IntoIterator<Item = Crossing<Z>>, extra: u32) -> Vec<BulletHit<Z>> {
    let mut hits: Vec<BulletHit<Z>> = Vec::new();
    for crossing in crossings {
        let Crossing::Zombie { zombie, zone, distance } = crossing else {
            break;
        };
        if hits.iter().any(|hit| hit.zombie == zombie) {
            continue;
        }
        if hits.len() as u32 > extra {
            break;
        }
        hits.push(BulletHit {
            zombie,
            zone,
            distance,
            passed: hits.len() as u32,
        });
    }
    hits
}

/// What the sights add to a bullet: an aimed optic's Headshot factor, extra Zombies passed, and
/// the damage kept through each Zombie passed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sights {
    pub headshot: f32,
    pub extra_pierce: u32,
    pub pierce_damage: f32,
}

impl Sights {
    /// A bullet with no optic's help.
    pub const PLAIN: Sights = Sights {
        headshot: 1.0,
        extra_pierce: 0,
        pierce_damage: PENETRATION_DAMAGE_FACTOR,
    };
}

/// Damage for one bullet hit: the (already resolved) damage at its range, times the (already
/// resolved) multiplier for the hit zone (and the sights' on a Headshot), times the penetration factor.
pub fn hit_damage<Z>(zone_multiplier: f32, damage_at_range: f32, hit: &BulletHit<Z>, sights: Sights) -> f32 {
    let optic = if hit.zone == HitZone::Head { sights.headshot } else { 1.0 };
    damage_at_range * zone_multiplier * optic * sights.pierce_damage.powi(hit.passed as i32)
}

/// A bullet's direction within a cone of `spread_degrees` around `forward`. `u` and `v` are
/// uniform in [0, 1): `u` picks how far out (evenly over the cone's face), `v` which way round.
pub fn spread_direction(forward: Dir3, spread_degrees: f32, u: f32, v: f32) -> Dir3 {
    if spread_degrees <= 0.0 {
        return forward;
    }
    let (right, up) = forward.any_orthonormal_pair();
    let radius = spread_degrees.to_radians().tan() * u.sqrt();
    let angle = v * std::f32::consts::TAU;
    let direction: Vec3 = *forward + (right * angle.cos() + up * angle.sin()) * radius;
    Dir3::new(direction).unwrap_or(forward)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arsenal::{Falloff, Penetration, WeaponId};

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn zombie(id: u32, distance: f32) -> Crossing<u32> {
        Crossing::Zombie {
            zombie: id,
            zone: HitZone::Torso,
            distance,
        }
    }

    fn hit(zone: HitZone, passed: u32) -> BulletHit<()> {
        BulletHit {
            zombie: (),
            zone,
            distance: 0.0,
            passed,
        }
    }

    #[test]
    fn headshot_multipliers_by_class() {
        let head = |id: WeaponId| id.stats().zone_multiplier(HitZone::Head);
        assert_eq!(head(WeaponId::M1911), 3.5);
        assert_eq!(head(WeaponId::Python), 3.5);
        assert_eq!(head(WeaponId::Mp40), 4.0);
        assert_eq!(head(WeaponId::Commando), 4.0);
        assert_eq!(head(WeaponId::M14), 3.0);
        assert_eq!(head(WeaponId::Rpk), 3.0);
        assert_eq!(head(WeaponId::Olympia), 1.0);
        assert_eq!(head(WeaponId::TrenchGun), 1.0);
        assert_eq!(head(WeaponId::Dragunov), 10.0);
        assert_eq!(head(WeaponId::L96a1), 10.0);
        // Explosives have none.
        assert_eq!(head(WeaponId::M72Law), 1.0);
        assert_eq!(head(WeaponId::RayGun), 1.0);
    }

    #[test]
    fn only_snipers_reward_the_neck_and_torso() {
        let sniper = WeaponId::Dragunov.stats();
        assert_eq!(sniper.zone_multiplier(HitZone::Neck), 5.0);
        assert_eq!(sniper.zone_multiplier(HitZone::Torso), 1.5);
        assert_eq!(sniper.zone_multiplier(HitZone::Limb), 1.0);

        let rifle = WeaponId::Commando.stats();
        for zone in [HitZone::Neck, HitZone::Torso, HitZone::Limb] {
            assert_eq!(rifle.zone_multiplier(zone), 1.0);
        }
    }

    #[test]
    fn hit_damage_combines_zone_and_penetration() {
        let m1911 = WeaponId::M1911.stats().zone_multiplier(HitZone::Head);
        assert!(close(hit_damage(m1911, 20.0, &hit(HitZone::Head, 0), Sights::PLAIN), 70.0));
        assert!(close(hit_damage(m1911, 20.0, &hit(HitZone::Head, 1), Sights::PLAIN), 52.5));
        let l96 = WeaponId::L96a1.stats().zone_multiplier(HitZone::Torso);
        assert!(close(hit_damage(l96, 500.0, &hit(HitZone::Torso, 2), Sights::PLAIN), 500.0 * 1.5 * 0.5625));
    }

    #[test]
    fn penetration_tiers_pass_1_2_and_4_extra_zombies() {
        assert_eq!(Penetration::None.extra_zombies(), 0);
        assert_eq!(Penetration::Small.extra_zombies(), 1);
        assert_eq!(Penetration::Medium.extra_zombies(), 2);
        assert_eq!(Penetration::Large.extra_zombies(), 4);
        assert_eq!(WeaponId::M14.stats().penetration, Penetration::Large);
    }

    #[test]
    fn a_bullet_stops_after_its_extra_zombies() {
        let line: Vec<_> = (0..8).map(|i| zombie(i, i as f32)).collect();
        let hits = pierce(line.clone(), WeaponId::M14.stats().penetration.extra_zombies());
        assert_eq!(hits.iter().map(|hit| hit.zombie).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
        assert_eq!(hits.iter().map(|hit| hit.passed).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);

        assert_eq!(pierce(line.clone(), 1).len(), 2);
        assert_eq!(pierce(line, 0).len(), 1);
    }

    #[test]
    fn penetration_damage_falls_by_a_quarter_per_zombie_passed() {
        let m14 = WeaponId::M14.stats().zone_multiplier(HitZone::Torso);
        let line: Vec<_> = (0..5).map(|i| zombie(i, 1.0)).collect();
        let damage: Vec<_> = pierce(line, 4).iter().map(|hit| hit_damage(m14, 100.0, hit, Sights::PLAIN)).collect();
        let expected = [100.0, 75.0, 56.25, 42.1875, 31.640625];
        for (got, want) in damage.iter().zip(expected) {
            assert!(close(*got, want), "{got} != {want}");
        }
    }

    #[test]
    fn an_aimed_optic_adds_to_headshots_and_pierces_at_full_damage() {
        let mnesia = WeaponId::M14.upgraded();
        let red_dot = Sights {
            headshot: 1.5,
            ..Sights::PLAIN
        };
        // ×6 for Mnesia's Headshot, then ×1.5 on top.
        assert!(close(hit_damage(mnesia.zone_multiplier(HitZone::Head), 100.0, &hit(HitZone::Head, 0), red_dot), 900.0));
        assert!(close(hit_damage(mnesia.zone_multiplier(HitZone::Limb), 100.0, &hit(HitZone::Limb, 0), red_dot), 100.0));
        let acog = Sights {
            extra_pierce: 2,
            pierce_damage: 1.0,
            ..Sights::PLAIN
        };
        assert!(close(hit_damage(mnesia.zone_multiplier(HitZone::Limb), 100.0, &hit(HitZone::Limb, 6), acog), 100.0));
    }

    #[test]
    fn the_world_stops_a_bullet() {
        let crossings = [zombie(0, 1.0), Crossing::World, zombie(1, 3.0)];
        assert_eq!(pierce(crossings, 4).len(), 1);
        assert!(pierce([Crossing::World, zombie(0, 1.0)], 4).is_empty());
    }

    #[test]
    fn a_zombie_is_hit_once_in_the_first_area_crossed() {
        let crossings = [
            Crossing::Zombie {
                zombie: 0,
                zone: HitZone::Limb,
                distance: 1.0,
            },
            Crossing::Zombie {
                zombie: 0,
                zone: HitZone::Torso,
                distance: 1.2,
            },
            zombie(1, 2.0),
        ];
        let hits = pierce(crossings, 1);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].zone, HitZone::Limb);
        assert_eq!((hits[1].zombie, hits[1].passed), (1, 1));
    }

    #[test]
    fn damage_falls_off_from_max_to_min() {
        let falloff = Falloff { start: 10.0, end: 20.0 };
        assert_eq!(falloff.damage(100.0, 60.0, 5.0), 100.0);
        assert_eq!(falloff.damage(100.0, 60.0, 15.0), 80.0);
        assert_eq!(falloff.damage(100.0, 60.0, 50.0), 60.0);
        assert_eq!(Falloff::NONE.damage(300.0, 300.0, 1000.0), 300.0);

        // The Trench Gun: 160 a pellet up close, 15 past 800 in (20.3 m).
        let trench = WeaponId::TrenchGun.stats();
        assert_eq!(trench.damage_at(2.0), 160.0);
        assert_eq!(trench.damage_at(25.0), 15.0);
        // The M14 keeps its 105 out to 1400 in (35.6 m).
        assert_eq!(WeaponId::M14.stats().damage_at(30.0), 105.0);
    }

    #[test]
    fn shotguns_fire_their_pellets() {
        assert_eq!(WeaponId::Olympia.stats().pellets, 8);
        assert_eq!(WeaponId::Spas12.stats().pellets, 8);
        assert_eq!(WeaponId::M14.stats().pellets, 1);
    }

    #[test]
    fn splash_falls_off_to_its_edge() {
        let splash = WeaponId::RayGun.stats().splash.unwrap();
        assert_eq!(splash.damage(0.0), Some(1500.0));
        assert!(close(splash.damage(0.8).unwrap(), 900.0));
        assert_eq!(splash.damage(1.7), None);
    }

    #[test]
    fn spread_stays_inside_its_cone() {
        let forward = Dir3::NEG_Z;
        assert_eq!(spread_direction(forward, 0.0, 0.9, 0.3), forward);
        for (u, v) in [(0.0, 0.0), (0.999, 0.25), (0.5, 0.7), (0.999, 0.999)] {
            let direction = spread_direction(forward, 6.0, u, v);
            let degrees = forward.angle_between(*direction).to_degrees();
            assert!(degrees <= 6.0 + 1e-3, "{degrees}");
        }
        let edge = spread_direction(forward, 6.0, 0.99999, 0.1);
        assert!(close(forward.angle_between(*edge).to_degrees(), 6.0));
    }
}
