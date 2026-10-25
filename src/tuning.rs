//! Every gameplay number lives here, so balancing never means hunting through systems.

use bevy::math::Vec3;

use crate::arsenal::{
    Availability, Burn, Falloff, FireMode, Handling, Penetration, ReloadTime, Splash, WeaponClass, WeaponId,
    WeaponStats,
};

// Player
pub const PLAYER_RADIUS: f32 = 0.35;
pub const PLAYER_CAPSULE_LENGTH: f32 = 1.1;
pub const PLAYER_EYE_HEIGHT: f32 = 0.7; // above the capsule centre
pub const PLAYER_WALK_SPEED: f32 = 5.0;
pub const PLAYER_SPRINT_SPEED: f32 = 7.5;
pub const PLAYER_ACCELERATION: f32 = 60.0;
pub const PLAYER_JUMP_SPEED: f32 = 5.5;
pub const GRAVITY: f32 = 18.0;
/// Radians of look per pixel of mouse motion at the default sensitivity.
pub const MOUSE_SENSITIVITY: f32 = 0.0025;

// Health
pub const PLAYER_MAX_HEALTH: f32 = 100.0;
pub const HEALTH_REGEN_DELAY_SECS: f32 = 3.0;
/// Seconds a regen takes from empty to full, whatever the max Health, so the rate scales with it.
pub const HEALTH_REGEN_FULL_SECS: f32 = 1.67;

// Sprint
pub const SPRINT_DURATION_SECS: f32 = 4.0;
/// Seconds to recharge a fully spent sprint (approx).
pub const SPRINT_RECHARGE_SECS: f32 = 2.0;

// Weapons
pub const STARTING_WEAPON: WeaponId = WeaponId::M1911;
/// How far a bullet reaches.
pub const HITSCAN_RANGE: f32 = 100.0;

/// Aiming narrows the FOV ×0.77 down iron sights and ×0.46 through a scope.
pub const ADS_FOV_IRON: f32 = 0.77;
pub const ADS_FOV_SCOPE: f32 = 0.46;
/// Seconds to bring the sights fully up, or down (approx).
pub const ADS_SECS: f32 = 0.2;
/// The share of the hip spread range (fresh to fully bloomed) each shot adds (approx).
pub const SPREAD_BLOOM_PER_SHOT: f32 = 0.25;
/// Seconds for a full bloom to settle back to fresh (approx).
pub const SPREAD_RECOVERY_SECS: f32 = 0.6;
pub const SPRINT_OUT_SECS: f32 = 0.3;
/// Sprint-out for LMGs and dual-wield.
pub const SPRINT_OUT_HEAVY_SECS: f32 = 0.45;
/// Extra Zombies a bullet passes through, by penetration tier.
pub const PENETRATION_SMALL: u32 = 1;
pub const PENETRATION_MEDIUM: u32 = 2;
pub const PENETRATION_LARGE: u32 = 4;
/// Damage kept for each Zombie a bullet has already passed through (approx).
pub const PENETRATION_DAMAGE_FACTOR: f32 = 0.75;

const fn inches(inches: f32) -> f32 {
    inches * 0.0254
}

const fn falloff(start_inches: f32, end_inches: f32) -> Falloff {
    Falloff {
        start: inches(start_inches),
        end: inches(end_inches),
    }
}

/// SMGs and pistols start falling off at 700–800 in; this takes the middle.
const FALLOFF_SHORT: Falloff = falloff(750.0, 1800.0);

/// The defaults every handling row starts from.
const HANDLING: Handling = Handling {
    class: WeaponClass::Pistol,
    raise_secs: 0.0,
    drop_secs: 0.0,
    hip_spread: (0.0, 0.0),
    aimed_spread: 0.0,
    ads_fov: Some(1.0),
    sprint_out_secs: SPRINT_OUT_SECS,
    move_speed: 1.0,
    falloff: Falloff::NONE,
};

// Weapon handling by class (Appendix A2, approx), in `WeaponClass` order. Spreads are in degrees.
pub const WEAPON_HANDLING: [Handling; 12] = [
    Handling {
        class: WeaponClass::Pistol,
        raise_secs: 0.55,
        drop_secs: 0.45,
        hip_spread: (3.0, 6.0),
        falloff: FALLOFF_SHORT,
        ..HANDLING
    },
    Handling {
        class: WeaponClass::DualWield,
        raise_secs: 0.3,
        drop_secs: 0.3,
        hip_spread: (4.0, 7.0),
        ads_fov: None,
        sprint_out_secs: SPRINT_OUT_HEAVY_SECS,
        falloff: FALLOFF_SHORT,
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Smg,
        raise_secs: 0.75,
        drop_secs: 0.5,
        hip_spread: (2.0, 5.0),
        ads_fov: Some(ADS_FOV_IRON),
        falloff: FALLOFF_SHORT,
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Rifle,
        raise_secs: 0.8,
        drop_secs: 0.6,
        hip_spread: (3.0, 7.0),
        ads_fov: Some(ADS_FOV_IRON),
        move_speed: 0.95,
        falloff: falloff(1600.0, 3000.0),
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Shotgun,
        raise_secs: 0.75,
        drop_secs: 0.6,
        // A2 gives 4–5 → 7–8 across the shotguns; this takes the middle.
        hip_spread: (4.5, 7.5),
        aimed_spread: 4.25,
        ads_fov: Some(ADS_FOV_IRON),
        falloff: falloff(256.0, 800.0),
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Sniper,
        raise_secs: 1.2,
        drop_secs: 0.9,
        hip_spread: (10.0, 15.0),
        ads_fov: Some(ADS_FOV_SCOPE),
        move_speed: 0.95,
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Lmg,
        raise_secs: 0.95,
        drop_secs: 0.65,
        hip_spread: (2.5, 7.0),
        ads_fov: Some(ADS_FOV_IRON),
        sprint_out_secs: SPRINT_OUT_HEAVY_SECS,
        move_speed: 0.85,
        falloff: falloff(1024.0, 2400.0),
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Launcher,
        raise_secs: 1.2,
        drop_secs: 1.05,
        hip_spread: (5.0, 6.0),
        aimed_spread: 1.7,
        ads_fov: Some(0.85),
        move_speed: 0.8,
        ..HANDLING
    },
    Handling {
        class: WeaponClass::RayGun,
        raise_secs: 0.8,
        drop_secs: 0.8,
        hip_spread: (1.0, 2.0),
        sprint_out_secs: 0.2,
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Thundergun,
        raise_secs: 1.1,
        drop_secs: 0.83,
        hip_spread: (2.0, 4.0),
        ..HANDLING
    },
    Handling {
        class: WeaponClass::Crossbow,
        raise_secs: 0.9,
        drop_secs: 0.75,
        hip_spread: (2.0, 4.0),
        ads_fov: Some(0.85),
        ..HANDLING
    },
    Handling {
        class: WeaponClass::BallisticKnife,
        raise_secs: 0.35,
        drop_secs: 0.35,
        ads_fov: Some(0.85),
        ..HANDLING
    },
];

/// Seconds between shots at a rate of fire.
const fn rpm(rounds_per_minute: f32) -> f32 {
    60.0 / rounds_per_minute
}

const fn reload(tactical: f32, empty: f32) -> ReloadTime {
    ReloadTime::Magazine { tactical, empty }
}

const fn shells(start: f32, per_bullet: f32, end: f32) -> ReloadTime {
    ReloadTime::PerBullet { start, per_bullet, end }
}

/// Semi-auto pistols and rifles are capped at 625 RPM.
const SEMI_CAP: f32 = rpm(625.0);
const SHOTGUN_PELLETS: u32 = 8;

/// The defaults every row starts from.
const ROW: WeaponStats = WeaponStats {
    id: WeaponId::M1911,
    name: "",
    class: WeaponClass::Pistol,
    fire: FireMode::Semi,
    availability: Availability::Box,
    damage_max: 0.0,
    damage_min: 0.0,
    pellets: 1,
    splash: None,
    burn: None,
    magazine: 0,
    reserve_max: 0,
    reserve_start: None,
    fire_interval: SEMI_CAP,
    reload: reload(0.0, 0.0),
    headshot: 1.0,
    neck: 1.0,
    torso: 1.0,
    penetration: Penetration::None,
    hip_spread: None,
    aimed_spread: None,
    ads_fov: None,
    falloff: None,
};

/// Snipers' multipliers beyond the head.
const SNIPER_NECK: f32 = 5.0;
const SNIPER_TORSO: f32 = 1.5;

/// Every base Weapon (Appendix A1), in `WeaponId` order. Splash radii are in metres.
/// Single-shot Weapons with no rate of fire listed can fire again as soon as their Reload ends.
pub const WEAPONS: [WeaponStats; 28] = [
    WeaponStats {
        id: WeaponId::M1911,
        name: "M1911",
        availability: Availability::Start,
        damage_max: 20.0,
        damage_min: 20.0,
        magazine: 8,
        reserve_max: 80,
        reserve_start: Some(32),
        reload: reload(1.63, 1.8),
        headshot: 3.5,
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Olympia,
        name: "Olympia",
        class: WeaponClass::Shotgun,
        availability: Availability::Wall(500),
        damage_max: 80.0,
        damage_min: 80.0,
        pellets: SHOTGUN_PELLETS,
        magazine: 2,
        reserve_max: 38,
        fire_interval: rpm(212.0),
        reload: reload(3.3, 3.9),
        penetration: Penetration::Small,
        aimed_spread: Some(5.5),
        falloff: Some(falloff(300.0, 900.0)),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::M14,
        name: "M14",
        class: WeaponClass::Rifle,
        availability: Availability::Wall(500),
        damage_max: 105.0,
        damage_min: 80.0,
        magazine: 8,
        reserve_max: 96,
        reload: reload(2.4, 3.45),
        headshot: 3.0,
        penetration: Penetration::Large,
        falloff: Some(falloff(1400.0, 2500.0)),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Mp40,
        name: "MP40",
        class: WeaponClass::Smg,
        fire: FireMode::Auto,
        availability: Availability::Wall(1000),
        damage_max: 100.0,
        damage_min: 60.0,
        magazine: 32,
        reserve_max: 192,
        fire_interval: rpm(536.0),
        reload: reload(2.3, 2.9),
        headshot: 4.0,
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::TrenchGun,
        name: "Trench Gun",
        class: WeaponClass::Shotgun,
        availability: Availability::Wall(1500),
        damage_max: 160.0,
        damage_min: 15.0,
        pellets: SHOTGUN_PELLETS,
        magazine: 6,
        reserve_max: 60,
        fire_interval: rpm(212.0),
        reload: shells(0.9, 0.6, 0.95),
        penetration: Penetration::Medium,
        aimed_spread: Some(5.5),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Ak74u,
        name: "AK-74u",
        class: WeaponClass::Smg,
        fire: FireMode::Auto,
        availability: Availability::Wall(1200),
        damage_max: 120.0,
        damage_min: 80.0,
        magazine: 20,
        reserve_max: 160,
        fire_interval: rpm(750.0),
        reload: reload(2.1, 2.8),
        headshot: 4.0,
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Cz75,
        name: "CZ75",
        damage_max: 150.0,
        damage_min: 100.0,
        magazine: 15,
        reserve_max: 135,
        reload: reload(1.63, 2.01),
        headshot: 3.5,
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Cz75DualWield,
        name: "CZ75 Dual Wield",
        class: WeaponClass::DualWield,
        damage_max: 150.0,
        damage_min: 100.0,
        magazine: 24, // 12 + 12
        reserve_max: 228,
        reload: reload(1.9, 2.2),
        headshot: 3.5,
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Python,
        name: "Python",
        damage_max: 1000.0,
        damage_min: 300.0,
        magazine: 6,
        reserve_max: 84,
        reload: ReloadTime::Revolver {
            start: 1.6,
            per_bullet: 0.65,
            empty: 1.0,
        },
        headshot: 3.5,
        penetration: Penetration::Medium,
        hip_spread: Some((2.0, 4.0)),
        falloff: Some(falloff(384.0, 1200.0)),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Spectre,
        name: "Spectre",
        class: WeaponClass::Smg,
        fire: FireMode::Auto,
        damage_max: 90.0,
        damage_min: 50.0,
        magazine: 30,
        reserve_max: 120,
        fire_interval: rpm(937.0),
        reload: reload(2.3, 3.0),
        headshot: 4.0,
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::G11,
        name: "G11",
        class: WeaponClass::Rifle,
        fire: FireMode::Burst(3),
        damage_max: 100.0,
        damage_min: 70.0,
        magazine: 48,
        reserve_max: 144,
        fire_interval: rpm(1250.0), // within a burst
        reload: reload(2.7, 3.85),
        headshot: 4.0,
        penetration: Penetration::Medium,
        ads_fov: Some(ADS_FOV_SCOPE),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Famas,
        name: "FAMAS",
        class: WeaponClass::Rifle,
        fire: FireMode::Auto,
        damage_max: 100.0,
        damage_min: 70.0,
        magazine: 30,
        reserve_max: 150,
        fire_interval: rpm(937.0),
        reload: reload(2.5, 3.3),
        headshot: 4.0,
        penetration: Penetration::Medium,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Aug,
        name: "AUG",
        class: WeaponClass::Rifle,
        fire: FireMode::Auto,
        damage_max: 140.0,
        damage_min: 90.0,
        magazine: 30,
        reserve_max: 270,
        fire_interval: rpm(937.0),
        reload: reload(2.6, 3.05),
        headshot: 4.0,
        penetration: Penetration::Medium,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Commando,
        name: "Commando",
        class: WeaponClass::Rifle,
        fire: FireMode::Auto,
        damage_max: 150.0,
        damage_min: 100.0,
        magazine: 30,
        reserve_max: 270,
        fire_interval: rpm(750.0),
        reload: reload(2.05, 2.55),
        headshot: 4.0,
        penetration: Penetration::Medium,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Galil,
        name: "Galil",
        class: WeaponClass::Rifle,
        fire: FireMode::Auto,
        damage_max: 150.0,
        damage_min: 100.0,
        magazine: 35,
        reserve_max: 315,
        fire_interval: rpm(750.0),
        reload: reload(2.8, 3.8),
        headshot: 4.0,
        penetration: Penetration::Medium,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::FnFal,
        name: "FN FAL",
        class: WeaponClass::Rifle,
        damage_max: 160.0, // approx
        damage_min: 130.0, // approx
        magazine: 20,
        reserve_max: 180,
        reload: reload(2.5, 3.1),
        headshot: 4.0,
        penetration: Penetration::Medium,
        hip_spread: Some((5.0, 9.0)),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Hs10,
        name: "HS-10",
        class: WeaponClass::Shotgun,
        damage_max: 160.0,
        damage_min: 15.0,
        pellets: SHOTGUN_PELLETS,
        magazine: 6,
        reserve_max: 36,
        fire_interval: rpm(341.0),
        reload: shells(1.35, 0.65, 1.0),
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Spas12,
        name: "SPAS-12",
        class: WeaponClass::Shotgun,
        damage_max: 160.0,
        damage_min: 15.0,
        pellets: SHOTGUN_PELLETS,
        magazine: 8,
        reserve_max: 32,
        fire_interval: rpm(312.0),
        reload: shells(1.5, 0.567, 1.0),
        penetration: Penetration::Small,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Dragunov,
        name: "Dragunov",
        class: WeaponClass::Sniper,
        damage_max: 300.0,
        damage_min: 300.0,
        magazine: 10,
        reserve_max: 40,
        fire_interval: rpm(234.0),
        reload: reload(2.95, 3.75),
        headshot: 10.0,
        neck: SNIPER_NECK,
        torso: SNIPER_TORSO,
        penetration: Penetration::Large,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::L96a1,
        name: "L96A1",
        class: WeaponClass::Sniper,
        damage_max: 500.0, // approx
        damage_min: 500.0, // approx
        magazine: 5,
        reserve_max: 45,
        fire_interval: 1.0, // bolt rechamber
        reload: reload(3.3, 3.5),
        headshot: 10.0,
        neck: SNIPER_NECK,
        torso: SNIPER_TORSO,
        penetration: Penetration::Large,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Rpk,
        name: "RPK",
        class: WeaponClass::Lmg,
        fire: FireMode::Auto,
        damage_max: 130.0,
        damage_min: 90.0,
        magazine: 100,
        reserve_max: 400,
        fire_interval: rpm(750.0),
        reload: reload(4.0, 5.5),
        headshot: 3.0,
        penetration: Penetration::Large,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Hk21,
        name: "HK21",
        class: WeaponClass::Lmg,
        fire: FireMode::Auto,
        damage_max: 150.0,
        damage_min: 110.0,
        magazine: 125,
        reserve_max: 500,
        fire_interval: rpm(536.0),
        reload: reload(3.75, 4.75),
        headshot: 3.0,
        penetration: Penetration::Large,
        ..ROW
    },
    WeaponStats {
        id: WeaponId::M72Law,
        name: "M72 LAW",
        class: WeaponClass::Launcher,
        damage_max: 2000.0, // approx
        damage_min: 2000.0, // approx
        splash: Some(Splash {
            max: 320.0, // approx
            min: 100.0, // approx
            radius: 5.7,
        }),
        magazine: 1,
        reserve_max: 20,
        fire_interval: 0.0,
        reload: reload(3.0, 3.0),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::ChinaLake,
        name: "China Lake",
        class: WeaponClass::Launcher,
        damage_max: 600.0,
        damage_min: 600.0,
        splash: Some(Splash {
            max: 600.0,
            min: 75.0,
            radius: 5.0,
        }),
        magazine: 2,
        reserve_max: 20,
        fire_interval: 2.05, // pump rechamber
        reload: shells(2.0, 1.0, 0.0),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::RayGun,
        name: "Ray Gun",
        class: WeaponClass::RayGun,
        damage_max: 1000.0,
        damage_min: 1000.0,
        splash: Some(Splash {
            max: 1500.0,
            min: 300.0,
            radius: 1.6,
        }),
        magazine: 20,
        reserve_max: 160,
        fire_interval: rpm(181.0),
        reload: reload(3.0, 3.0),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Thundergun,
        name: "Thundergun",
        class: WeaponClass::Thundergun,
        availability: Availability::LimitedBox,
        // Its damage is scripted by distance, not a number per bullet.
        magazine: 2,
        reserve_max: 12,
        fire_interval: 0.95,
        reload: reload(2.1, 2.0),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::Crossbow,
        name: "Crossbow",
        class: WeaponClass::Crossbow,
        availability: Availability::LimitedBox,
        damage_max: 750.0,
        damage_min: 750.0,
        // The bolt explodes 1.5 s after it sticks.
        splash: Some(Splash {
            max: 400.0,
            min: 75.0,
            radius: 5.0,
        }),
        magazine: 1,
        reserve_max: 12,
        fire_interval: 0.0,
        reload: reload(2.75, 2.75),
        ..ROW
    },
    WeaponStats {
        id: WeaponId::BallisticKnife,
        name: "Ballistic Knife",
        class: WeaponClass::BallisticKnife,
        availability: Availability::LimitedBox,
        damage_max: 500.0,
        damage_min: 500.0,
        magazine: 1,
        reserve_max: 4,
        fire_interval: 0.0,
        reload: reload(1.5, 1.5),
        headshot: 1.5,
        ..ROW
    },
];

/// A base row, for an upgraded row to start from: a dash in Appendix A3 means unchanged.
const fn base(id: WeaponId) -> WeaponStats {
    WEAPONS[id as usize]
}

/// The Hades' incendiary pellets (approx: A3 only says "burn damage").
pub const HADES_BURN: Burn = Burn {
    damage: 50.0,
    tick_secs: 0.5,
    secs: 3.0,
};

/// Every Weapon's hand-written Pack-a-Punch form (Appendix A3), in `WeaponId` order. Each row
/// starts from its base row and writes out what A3 changes; nothing is a multiplier.
pub const UPGRADED_WEAPONS: [WeaponStats; 28] = [
    WeaponStats {
        name: "Mustang & Sally",
        class: WeaponClass::DualWield,
        damage_max: 1000.0,
        damage_min: 1000.0,
        // Fires grenades. The splash radius is unverified.
        splash: Some(Splash {
            max: 1200.0, // approx
            min: 100.0,  // approx
            radius: 3.0, // approx
        }),
        magazine: 12, // 6 + 6
        reserve_max: 50,
        reserve_start: None,
        fire_interval: rpm(300.0),
        reload: reload(1.6, 1.6),
        penetration: Penetration::None,
        ..base(WeaponId::M1911)
    },
    WeaponStats {
        name: "Hades",
        damage_max: 200.0,
        damage_min: 200.0,
        pellets: 12,
        burn: Some(HADES_BURN),
        magazine: 2,
        reserve_max: 60,
        reload: reload(2.0, 2.5),
        ..base(WeaponId::Olympia)
    },
    WeaponStats {
        name: "Mnesia",
        damage_max: 200.0,
        damage_min: 150.0,
        headshot: 6.0,
        torso: 2.0,
        magazine: 16,
        reserve_max: 192,
        ..base(WeaponId::M14)
    },
    WeaponStats {
        name: "The Afterburner",
        damage_max: 200.0,
        damage_min: 120.0,
        headshot: 5.0,
        magazine: 64,
        reserve_max: 192,
        fire_interval: rpm(600.0),
        ..base(WeaponId::Mp40)
    },
    WeaponStats {
        name: "Gut Shot",
        damage_max: 300.0,
        damage_min: 300.0,
        magazine: 10,
        reserve_max: 60,
        // Two shells at a time: half the time per shell.
        reload: shells(0.9, 0.3, 0.95),
        ..base(WeaponId::TrenchGun)
    },
    WeaponStats {
        name: "AK-74fu2",
        damage_max: 190.0,
        damage_min: 140.0,
        headshot: 5.0,
        magazine: 40,
        reserve_max: 280,
        ..base(WeaponId::Ak74u)
    },
    WeaponStats {
        name: "Calamity",
        fire: FireMode::Auto,
        damage_max: 300.0,
        damage_min: 200.0,
        magazine: 20,
        reserve_max: 240,
        fire_interval: rpm(535.0),
        ..base(WeaponId::Cz75)
    },
    WeaponStats {
        name: "Calamity & Jane",
        fire: FireMode::Auto,
        damage_max: 300.0,
        damage_min: 200.0,
        magazine: 40, // 20 + 20
        reserve_max: 320,
        fire_interval: rpm(535.0),
        ..base(WeaponId::Cz75DualWield)
    },
    WeaponStats {
        name: "Cobra",
        damage_max: 1000.0,
        damage_min: 600.0,
        headshot: 3.0,
        magazine: 12,
        reserve_max: 96,
        // The whole cylinder at once.
        reload: reload(2.6, 2.6),
        ..base(WeaponId::Python)
    },
    WeaponStats {
        name: "Phantom",
        damage_max: 130.0,
        damage_min: 80.0,
        headshot: 5.0,
        magazine: 45,
        reserve_max: 225,
        ..base(WeaponId::Spectre)
    },
    WeaponStats {
        name: "G115 Generator",
        fire: FireMode::Auto,
        damage_max: 150.0,
        damage_min: 100.0,
        headshot: 5.0,
        magazine: 48,
        reserve_max: 288,
        ..base(WeaponId::G11)
    },
    WeaponStats {
        name: "G16-GL35",
        damage_max: 150.0,
        damage_min: 100.0,
        headshot: 5.0,
        magazine: 45,
        reserve_max: 225,
        ..base(WeaponId::Famas)
    },
    WeaponStats {
        // Its built-in Masterkey arrives with Underbarrels.
        name: "AUG-50M3",
        damage_max: 200.0,
        damage_min: 140.0,
        headshot: 5.0,
        magazine: 30,
        reserve_max: 390,
        ..base(WeaponId::Aug)
    },
    WeaponStats {
        name: "Predator",
        damage_max: 210.0,
        damage_min: 150.0,
        headshot: 4.0,
        magazine: 40,
        reserve_max: 360,
        reload: reload(1.7, 2.1),
        ..base(WeaponId::Commando)
    },
    WeaponStats {
        name: "Lamentation",
        damage_max: 220.0,
        damage_min: 150.0,
        headshot: 5.0,
        magazine: 35,
        reserve_max: 490,
        ..base(WeaponId::Galil)
    },
    WeaponStats {
        name: "EPC WN",
        fire: FireMode::Burst(3),
        damage_max: 240.0,
        damage_min: 180.0,
        headshot: 5.0,
        magazine: 30,
        reserve_max: 360,
        ..base(WeaponId::FnFal)
    },
    WeaponStats {
        name: "Typhoid & Mary",
        class: WeaponClass::DualWield,
        damage_max: 300.0,
        damage_min: 300.0,
        magazine: 16, // 8 + 8
        reserve_max: 80,
        fire_interval: rpm(340.0),
        // The whole Magazine at once.
        reload: reload(3.0, 3.4),
        // Still a shotgun at range.
        falloff: Some(WEAPON_HANDLING[WeaponClass::Shotgun as usize].falloff),
        ..base(WeaponId::Hs10)
    },
    WeaponStats {
        name: "SPAZ-24",
        fire: FireMode::Auto,
        damage_max: 300.0,
        damage_min: 300.0,
        magazine: 24,
        reserve_max: 72,
        // The whole Magazine at once (~1 s).
        reload: reload(1.0, 1.0),
        ..base(WeaponId::Spas12)
    },
    WeaponStats {
        name: "D115 Disassembler",
        damage_max: 1000.0,
        damage_min: 1000.0,
        torso: 3.0,
        magazine: 10,
        reserve_max: 80,
        ..base(WeaponId::Dragunov)
    },
    WeaponStats {
        // Its body multipliers are unverified, so it keeps the L96A1's.
        name: "L115 Isolator",
        damage_max: 1000.0,
        damage_min: 1000.0,
        magazine: 8,
        reserve_max: 72,
        ..base(WeaponId::L96a1)
    },
    WeaponStats {
        name: "R115 Resonator",
        damage_max: 180.0,
        damage_min: 130.0,
        magazine: 125,
        reserve_max: 500,
        ..base(WeaponId::Rpk)
    },
    WeaponStats {
        name: "H115 Oscillator",
        damage_max: 210.0,
        damage_min: 150.0,
        magazine: 150,
        reserve_max: 750,
        ..base(WeaponId::Hk21)
    },
    WeaponStats {
        name: "M72 Anarchy",
        fire: FireMode::Auto,
        damage_max: 3000.0, // approx
        damage_min: 3000.0, // approx
        splash: Some(Splash {
            max: 600.0, // approx
            min: 100.0, // approx
            radius: 5.7,
        }),
        magazine: 10,
        reserve_max: 40,
        fire_interval: rpm(56.0),
        reload: reload(3.0, 3.0),
        ..base(WeaponId::M72Law)
    },
    WeaponStats {
        name: "China Beach",
        damage_max: 1000.0, // approx
        damage_min: 1000.0, // approx
        splash: Some(Splash {
            max: 1000.0, // approx
            min: 75.0,   // approx
            radius: 5.0,
        }),
        magazine: 5,
        reserve_max: 40,
        // One Reload loads every grenade.
        reload: reload(1.0, 1.0),
        ..base(WeaponId::ChinaLake)
    },
    WeaponStats {
        name: "Porter's X2 Ray Gun",
        splash: Some(Splash {
            max: 2000.0,
            min: 300.0,
            radius: 1.6,
        }),
        magazine: 40,
        reserve_max: 200,
        ..base(WeaponId::RayGun)
    },
    WeaponStats {
        // Ammo only.
        name: "ZeusCannon",
        magazine: 4,
        reserve_max: 24,
        ..base(WeaponId::Thundergun)
    },
    WeaponStats {
        // The 4 s fuse and the Monkey Bomb pull arrive with projectiles.
        name: "Awful Lawton",
        splash: Some(Splash {
            max: 2500.0,
            min: 225.0,
            radius: 5.0,
        }),
        ..base(WeaponId::Crossbow)
    },
    WeaponStats {
        name: "Krauss Refibrillator",
        damage_max: 1000.0,
        damage_min: 1000.0,
        headshot: 3.0,
        reserve_max: 9,
        reload: reload(0.75, 0.75),
        ..base(WeaponId::BallisticKnife)
    },
];

// Knife
pub const KNIFE_DAMAGE: f32 = 150.0;
pub const KNIFE_RANGE: f32 = 1.6;
pub const KNIFE_RADIUS: f32 = 0.3;
pub const KNIFE_COOLDOWN_SECS: f32 = 0.6;

// Zombie
pub const ZOMBIE_RADIUS: f32 = 0.35;
pub const ZOMBIE_CAPSULE_LENGTH: f32 = 0.8;
pub const ZOMBIE_HEAD_RADIUS: f32 = 0.2;
// Zombie hit areas (box sizes, width × height × depth). They decide the hit zone, so damage and kill Points.
// Torso and legs span the body capsule's full diameter both ways, so no edge shot slips through.
pub const ZOMBIE_ARM_SIZE: Vec3 = Vec3::new(0.12, 0.12, 0.6);
pub const ZOMBIE_LEG_SIZE: Vec3 = Vec3::new(0.35, 0.55, 0.7);
/// Each leg's sideways offset from the Zombie's centre.
pub const ZOMBIE_LEG_OFFSET: f32 = 0.175;
/// The torso fills the gap between the legs and the neck, so only its width and depth are set.
pub const ZOMBIE_TORSO_WIDTH: f32 = 0.7;
pub const ZOMBIE_TORSO_DEPTH: f32 = 0.7;
pub const ZOMBIE_NECK_SIZE: Vec3 = Vec3::new(0.22, 0.12, 0.22);
pub const ZOMBIE_WALK_SPEED: f32 = 1.5;
pub const ZOMBIE_JOG_SPEED: f32 = 3.5;
pub const ZOMBIE_SPRINT_SPEED: f32 = 5.5;
/// Centre-to-centre distance at which a Zombie starts (and lands) a swing.
pub const ZOMBIE_REACH: f32 = 1.2;
pub const ZOMBIE_WIND_UP_SECS: f32 = 0.5;
/// After a Zombie's swipe lands, that Zombie can't hit the Player again for this long.
pub const ZOMBIE_HIT_COOLDOWN_SECS: f32 = 0.4;
/// Two swipes Down a Player without Juggernog.
pub const ZOMBIE_SWIPE_DAMAGE: f32 = 60.0;
pub const ZOMBIE_FALL_SECS: f32 = 0.4;
pub const ZOMBIE_CORPSE_SECS: f32 = 2.0;

// Rounds
pub const ZOMBIE_HEALTH_START: u64 = 150;
/// Added each Round through Round 9.
pub const ZOMBIE_HEALTH_PER_ROUND: u64 = 100;
/// From Round 10, `health += floor(health × growth)` each Round.
pub const ZOMBIE_HEALTH_GROWTH: f64 = 0.1;
/// Solo Horde: `HORDE_BASE + int(HORDE_SOLO_BONUS × multiplier)`.
pub const HORDE_BASE: u32 = 24;
pub const HORDE_SOLO_BONUS: f64 = 3.0;
/// Horde scale for Rounds 1–5.
pub const HORDE_EARLY_SCALE: [f64; 5] = [0.25, 0.3, 0.5, 0.7, 0.9];
pub const ALIVE_CAP: u32 = 24;
/// Speed roll: `rand[base, base + SPREAD]`, base 1 in Round 1, else `(round − 1) × PER_ROUND`.
pub const SPEED_ROLL_PER_ROUND: u32 = 8;
pub const SPEED_ROLL_SPREAD: u32 = 34;
/// Rolls up to this are Walkers; up to `SPEED_ROLL_JOGGER_MAX` Joggers; above that Sprinters.
pub const SPEED_ROLL_WALKER_MAX: u32 = 35;
pub const SPEED_ROLL_JOGGER_MAX: u32 = 70;
/// Rounds in which the Horde's last Zombie sprints.
pub const LAST_ZOMBIE_SPRINT_ROUNDS: std::ops::RangeInclusive<u32> = 4..=9;
pub const SPAWN_DELAY_START_SECS: f32 = 2.0;
pub const SPAWN_DELAY_FACTOR: f32 = 0.95;
pub const SPAWN_DELAY_MIN_SECS: f32 = 0.08;
pub const INTERMISSION_SECS: f32 = 10.0;

// Points
pub const STARTING_POINTS: u64 = 500;
/// Paid for each hit a Zombie survives. The killing hit pays kill Points only.
pub const POINTS_PER_HIT: u64 = 10;
pub const POINTS_LIMB_KILL: u64 = 50;
pub const POINTS_TORSO_KILL: u64 = 60;
pub const POINTS_NECK_KILL: u64 = 70;
pub const POINTS_HEADSHOT_KILL: u64 = 100;
pub const POINTS_KNIFE_KILL: u64 = 130;

// Doors
pub const DOOR_PRICE_START_WEST: u64 = 750;
pub const DOOR_PRICE_START_EAST: u64 = 750;
pub const DOOR_PRICE_WEST_DINING: u64 = 1000;
pub const DOOR_PRICE_EAST_POWER: u64 = 1000;
pub const DOOR_PRICE_DINING_POWER: u64 = 1250;
pub const DOOR_PRICE_START_YARD: u64 = 1000;
pub const DOOR_PRICE_YARD_DINING: u64 = 1250;

// Buying
/// How far from a Buyable's use point the Player can buy it, height included.
pub const USE_RANGE: f32 = 2.0;
/// How far off the crosshair a Buyable's use point can be and still be bought.
pub const USE_ANGLE: f32 = 45.0_f32.to_radians();
/// A Door's range override: its use point sits mid-Door, so this reaches the Door's full width.
pub const DOOR_USE_RANGE: f32 = USE_RANGE + 1.2;
pub const CANT_AFFORD_FLASH_SECS: f32 = 0.5;
/// An ammo buy costs half the Wall Weapon's price, rounded up to a multiple of this.
pub const AMMO_PRICE_STEP: u64 = 10;
/// An Upgraded Weapon's ammo buy, whatever its wall price.
pub const PAP_AMMO_PRICE: u64 = 4500;

// Pack-a-Punch
pub const PAP_PRICE: u64 = 5000;
/// How long the machine works on a Weapon before it can be taken.
pub const PAP_PROCESS_SECS: f32 = 3.0;
/// How long the Upgraded Weapon waits to be taken before it's lost, with no refund.
pub const PAP_PICKUP_SECS: f32 = 15.0;
/// Seconds for the glint band to sweep once along an Upgraded Weapon.
pub const GLINT_SWEEP_SECS: f32 = 1.2;

// Power and Perks
/// How long the Power Switch's lever takes to throw.
pub const POWER_SWITCH_FLIP_SECS: f32 = 0.3;
/// How long drinking a Perk takes. The Perk applies at the end.
pub const PERK_DRINK_SECS: f32 = 2.8;
pub const PERK_PRICE_JUGGERNOG: u64 = 2500;
pub const PERK_PRICE_SPEED_COLA: u64 = 3000;
pub const PERK_PRICE_DOUBLE_TAP: u64 = 2000;
/// Max Health with Juggernog, up from `PLAYER_MAX_HEALTH`.
pub const JUGGERNOG_MAX_HEALTH: f32 = 160.0;
pub const SPEED_COLA_RELOAD_MULT: f32 = 0.5;
pub const DOUBLE_TAP_FIRE_INTERVAL_MULT: f32 = 0.75;


// Settings
/// Sensitivity runs 1–10; the default plays exactly like `MOUSE_SENSITIVITY` and the rest scale it.
pub const SENSITIVITY_MIN: u8 = 1;
pub const SENSITIVITY_MAX: u8 = 10;
pub const SENSITIVITY_DEFAULT: u8 = 5;
/// Master volume in percent.
pub const MASTER_VOLUME_DEFAULT: u8 = 80;
pub const MASTER_VOLUME_STEP: u8 = 5;
/// Vertical field of view in degrees.
pub const FOV_MIN: u8 = 65;
pub const FOV_MAX: u8 = 110;
pub const FOV_DEFAULT: u8 = 80;
pub const FOV_STEP: u8 = 5;

// Records
/// Run Records kept per (Map, Mode).
pub const RECORDS_KEPT: usize = 10;
