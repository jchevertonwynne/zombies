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
// Faster than BO1 on purpose (playtest): walking just out-paces a Sprinter (5.5), and sprinting
// clearly escapes one while stamina lasts.
pub const PLAYER_WALK_SPEED: f32 = 6.5;
pub const PLAYER_SPRINT_SPEED: f32 = 9.0;
pub const PLAYER_ACCELERATION: f32 = 60.0;
pub const PLAYER_JUMP_SPEED: f32 = 5.5;
pub const GRAVITY: f32 = 18.0;
/// Ground steeper than this (the up-component of its normal below it) is a wall, not floor.
/// The West Hall ramp is 0.89.
pub const PLAYER_GROUND_NORMAL_Y: f32 = 0.7;
/// How far below the Player's feet a step looks for floor to stay on. Covers walking down the
/// ramp at a sprint (about 7 cm a step) and stops well short of any drop.
pub const PLAYER_GROUND_PROBE: f32 = 0.1;
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
/// Weapon slots the Player carries, before any stat entry adds more.
pub const WEAPON_SLOTS: u32 = 2;
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
        // Its 4 s fuse and Monkey Bomb pull are the bolt's (`AWFUL_LAWTON_FUSE_SECS`).
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
/// The Bowie Knife's melee damage, in place of the Knife's.
pub const BOWIE_KNIFE_DAMAGE: f32 = 1000.0;

// Equipment
/// Frags the Player starts a Run with.
pub const FRAG_START: u32 = 2;
/// Frags given at the start of each Round after the first, up to the most carried.
pub const FRAG_PER_ROUND: u32 = 2;
pub const FRAG_MAX: u32 = 4;
/// Seconds from pulling the pin to the blast; cooking spends it in the hand.
pub const FRAG_FUSE_SECS: f32 = 3.5;
pub const FRAG_SPLASH: Splash = Splash {
    max: 300.0, // approx
    min: 75.0,  // approx
    radius: inches(256.0), // approx
};
/// Claymores a bought Claymore wall gives, and refills to every Round.
pub const CLAYMORE_PER_ROUND: u32 = 2;
/// A Zombie this close (96 in), inside the front cone and in sight, sets a Claymore off.
pub const CLAYMORE_TRIGGER_RANGE: f32 = 2.4;
/// The whole width of a Claymore's front cone, in degrees.
pub const CLAYMORE_TRIGGER_CONE_DEGREES: f32 = 70.0;
/// Seconds from a Claymore being set off to its blast.
pub const CLAYMORE_DELAY_SECS: f32 = 0.4;
pub const CLAYMORE_SPLASH: Splash = Splash {
    max: 200.0,
    min: 50.0,
    radius: 5.0,
};
/// Taking the Monkey Bomb from the Mystery Box fills the Tactical slot to this. It never refills by Round.
pub const MONKEY_BOMB_CARRY: u32 = 3;
/// Seconds from a Monkey Bomb landing to its blast.
pub const MONKEY_BOMB_FUSE_SECS: f32 = 8.0;
/// The most Zombies one Monkey Bomb pulls, and how far away they can be.
pub const MONKEY_BOMB_LURE_MAX: usize = 96;
pub const MONKEY_BOMB_LURE_RANGE: f32 = 39.0;
pub const MONKEY_BOMB_SPLASH: Splash = Splash {
    max: 5000.0,
    min: 450.0,
    radius: 2.5,
};
/// A thrown Monkey Bomb that never lands (out of the Map) is cleaned up after this long.
pub const MONKEY_BOMB_LOST_SECS: f32 = 10.0;
/// How fast Equipment leaves the hand, and the upward lift added to the throw (approx).
pub const EQUIPMENT_THROW_SPEED: f32 = 14.0;
pub const EQUIPMENT_THROW_LIFT: f32 = 3.0;
/// Seconds a frag takes to come up into the hand while it cooks.
pub const COOK_RAISE_SECS: f32 = 0.25;
/// Seconds an overarm throw takes, and how far (0 to 1) through it the frag or Monkey Bomb leaves
/// the hand.
pub const THROW_SECS: f32 = 0.55;
pub const THROW_RELEASE_AT: f32 = 0.4;
/// Seconds planting a Claymore takes, and how far through it the hand lets go.
pub const PLANT_SECS: f32 = 0.8;
pub const PLANT_RELEASE_AT: f32 = 0.6;
/// How far ahead of the camera a throw is aimed at, so it flies to the crosshair from the hand.
pub const THROW_AIM_DISTANCE: f32 = 20.0;
/// The least fuse a frag has left once it leaves the hand, after the wind-up burned some.
pub const FRAG_MIN_FUSE_SECS: f32 = 0.1;
/// The radius of a thrown frag or Monkey Bomb's collider.
pub const THROWN_RADIUS: f32 = 0.08;
/// How much of its speed a thrown frag or Monkey Bomb keeps off a bounce (approx).
pub const THROWN_RESTITUTION: f32 = 0.3;
/// The Player's own explosive costs this flat amount while their Health is above it, and full
/// damage at or below it.
pub const SELF_EXPLOSIVE_DAMAGE: f32 = 75.0;

// Projectiles
/// The M72 LAW's rocket flies straight at this speed (approx).
pub const LAW_ROCKET_SPEED: f32 = 60.0;
/// The China Lake's grenade leaves at this speed and drops under gravity (approx).
pub const CHINA_LAKE_GRENADE_SPEED: f32 = 30.0;
/// A projectile that hits nothing bursts in the air after this long.
pub const PROJECTILE_LIFETIME_SECS: f32 = 5.0;
/// On a client, a thrown or launched thing the Host has said goes up flies for show until its
/// burst arrives, and is let go of after this long if none does.
pub const COSMETIC_FLIGHT_SECS: f32 = 12.0;
/// A burst the Host reports ends the nearest thing flying for show within this many metres.
pub const COSMETIC_BURST_MATCH_METRES: f32 = 6.0;
/// How fast a Wonder Weapon's shot is drawn flying on a machine that did not fire it (it is
/// instant on the Host).
pub const COSMETIC_WONDER_SPEED: f32 = 90.0;

// Special Box Weapons
/// The Thundergun's blast hits every Zombie in sight inside a cylinder this long and this wide
/// (radius) along the Player's aim.
pub const THUNDERGUN_RANGE: f32 = 30.0;
pub const THUNDERGUN_RADIUS: f32 = 4.6;
/// Under this distance the blast kills outright; beyond it, it knocks down for
/// `THUNDERGUN_KNOCKDOWN_DAMAGE`. It never makes Crawlers.
pub const THUNDERGUN_KILL_RANGE: f32 = 12.0;
pub const THUNDERGUN_KNOCKDOWN_DAMAGE: f32 = 30.0;
/// How long a knocked-down Zombie lies before it gets up (approx).
pub const THUNDERGUN_KNOCKDOWN_SECS: f32 = 1.5;
/// How fast the blast flings a Zombie it knocks down, slowing to a stop as it lies (approx).
pub const THUNDERGUN_FLING_SPEED: f32 = 8.0;
/// The Crossbow's bolt explodes this long after it sticks; the Awful Lawton's waits longer and
/// pulls Zombies like a Monkey Bomb meanwhile.
pub const CROSSBOW_FUSE_SECS: f32 = 1.5;
pub const AWFUL_LAWTON_FUSE_SECS: f32 = 4.0;
/// The Knife's melee damage while the Ballistic Knife (or the Krauss Refibrillator) is in hand,
/// without and with the Bowie Knife.
pub const BALLISTIC_KNIFE_MELEE_DAMAGE: f32 = 500.0;
pub const BALLISTIC_KNIFE_BOWIE_MELEE_DAMAGE: f32 = 1000.0;
pub const KRAUSS_MELEE_DAMAGE: f32 = 1000.0;
pub const KRAUSS_BOWIE_MELEE_DAMAGE: f32 = 1500.0;
/// How close the Player walks to a stuck blade to pick it back up (approx).
pub const BALLISTIC_KNIFE_PICKUP_RANGE: f32 = 1.5;

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

// Crawlers
/// An explosive hit a normal Zombie survives, of at least this share of its pre-hit health, has
/// a `CRAWLER_GIB_CHANCE` chance of blowing its legs off.
pub const CRAWLER_GIB_MIN_FRACTION: f32 = 0.1;
pub const CRAWLER_GIB_CHANCE: f64 = 0.5;
pub const CRAWLER_SPEED: f32 = 1.0;
/// A Crawler's body is a ball this big, so it's about 0.6 m tall and the Player can jump over it.
pub const CRAWLER_RADIUS: f32 = 0.3;
/// The length of a Crawler's lying body between its rounded ends.
pub const CRAWLER_BODY_LENGTH: f32 = 0.6;
/// A Crawler never tears Planks: it waits for an open Barrier, then climbs in over this long.
pub const CRAWLER_VAULT_SECS: f32 = 2.0;
/// Waiting this long at a planked Barrier counts as stuck.
pub const CRAWLER_PLANKED_STUCK_SECS: f32 = 10.0;

// Navigation and spawning
/// Seconds between a Zombie's re-plans, or `ZOMBIE_REPATH_FAR_SECS` beyond `ZOMBIE_REPATH_FAR_DIST` of the Player.
pub const ZOMBIE_REPATH_SECS: f32 = 0.25;
pub const ZOMBIE_REPATH_FAR_SECS: f32 = 1.0;
pub const ZOMBIE_REPATH_FAR_DIST: f32 = 30.0;
/// Seconds between a Zombie picking which Player to go after. Each pick plans a route to every
/// Player who is up, which costs more than the one re-plan a Zombie makes every
/// `ZOMBIE_REPATH_SECS`, so it's as slow as the slowest of those, `ZOMBIE_REPATH_FAR_SECS`: a
/// pick always lands on a re-plan, and a Train can still pass between Players within a second.
/// A Zombie whose Player goes down picks again at once.
pub const ZOMBIE_RETARGET_SECS: f32 = 1.0;
/// How close (horizontally) a Zombie gets to a path node before heading for the next one.
pub const ZOMBIE_WAYPOINT_REACH: f32 = 0.6;
/// The most the Player's feet can be above or below a Zombie's for it to steer straight at them.
/// More than the Yard cover boxes' 1.1 m, so a Player standing on one is swiped from beside it,
/// and far less than the Balcony's 4 m, which is a separate floor reached by the ramp.
pub const ZOMBIE_DIRECT_MAX_RISE: f32 = 1.2;
/// Separation steering: Zombies closer than this push apart, by up to this weight of their stride.
pub const ZOMBIE_SEPARATION_DIST: f32 = 1.0;
pub const ZOMBIE_SEPARATION_WEIGHT: f32 = 0.6;
/// Seconds a Riser takes to climb out of the ground; it can be shot but can't attack meanwhile.
pub const ZOMBIE_RISE_SECS: f32 = 1.5;
/// Stuck cleanup: a Zombie that makes no progress, or stays beyond `ZOMBIE_CLEANUP_DIST` of the
/// Player, for `ZOMBIE_CLEANUP_SECS` despawns and goes back into the Horde.
pub const ZOMBIE_CLEANUP_DIST: f32 = 40.0;
pub const ZOMBIE_CLEANUP_SECS: f32 = 10.0;
/// Moving this far counts as progress for the stuck rule.
pub const ZOMBIE_PROGRESS_DIST: f32 = 1.0;
/// Seconds a Zombie takes to climb in through a Barrier with no Planks left. It can be shot
/// meanwhile, and a vault that has started always completes.
pub const ZOMBIE_VAULT_SECS: f32 = 1.0;

// Barriers
/// Planks on every Barrier. They never repair themselves.
pub const BARRIER_PLANKS: u8 = 6;
/// Places outside each window a Zombie can tear at it from; extra Zombies wait outside.
pub const BARRIER_ATTACK_SPOTS: usize = 3;
/// Each attack spot tears one Plank this often, independently of the others.
pub const ZOMBIE_PLANK_TEAR_SECS: f32 = 1.5;
/// A reach-through swipe hits for `ZOMBIE_SWIPE_DAMAGE` with this chance per swing, planked or not.
pub const BARRIER_SWIPE_CHANCE: f64 = 0.5;
/// A Zombie at a window can swipe the Player only within this distance of the Zombie (90 in)…
pub const BARRIER_SWIPE_PLAYER_DIST: f32 = 2.3;
/// …and this distance of the Barrier (51 in).
pub const BARRIER_SWIPE_WINDOW_DIST: f32 = 1.3;
/// Holding Use puts one Plank back per `BARRIER_REBUILD_SECS`, paying `BARRIER_REBUILD_POINTS`.
pub const BARRIER_REBUILD_SECS: f32 = 1.0;
pub const BARRIER_REBUILD_POINTS: u64 = 10;
/// A rebuilt Plank slides into its slot from inside the room over `PLANK_SLIDE_SECS`, starting
/// `PLANK_SLIDE_FROM` metres back from the window.
pub const PLANK_SLIDE_SECS: f32 = 0.25;
pub const PLANK_SLIDE_FROM: f32 = 1.1;
/// How long the rebuild hand takes to come up when the Use key goes down at a Barrier, and to go
/// away again when it's let go.
pub const REBUILD_HAND_SECS: f32 = 0.2;
/// A bought Wall Weapon's copy lifts off the chalk toward the Player over `WALL_LIFT_SECS`, this
/// many metres of the way.
pub const WALL_LIFT_SECS: f32 = 0.4;
pub const WALL_LIFT_REACH: f32 = 0.7;
/// An ammo buy plays its pose on the held Weapon for this long: a quick tilt and a magazine slap.
pub const AMMO_BUY_SECS: f32 = 0.45;
/// The Round cap on rebuild Points: `min(BARRIER_REBUILD_CAP_PER_ROUND × round, BARRIER_REBUILD_CAP_MAX)`.
pub const BARRIER_REBUILD_CAP_PER_ROUND: u64 = 50;
pub const BARRIER_REBUILD_CAP_MAX: u64 = 500;

// Rounds
pub const ZOMBIE_HEALTH_START: u64 = 150;
/// Added each Round through Round 9.
pub const ZOMBIE_HEALTH_PER_ROUND: u64 = 100;
/// From Round 10, `health += floor(health × growth)` each Round.
pub const ZOMBIE_HEALTH_GROWTH: f64 = 0.1;
/// Solo Horde: `HORDE_BASE + int(HORDE_SOLO_BONUS × multiplier)`.
pub const HORDE_BASE: u32 = 24;
pub const HORDE_SOLO_BONUS: f64 = 3.0;
/// Each Player past the first adds this much to the Horde bonus's multiplier:
/// `HORDE_BASE + int((N − 1) × HORDE_COOP_PER_PLAYER × m)`.
pub const HORDE_COOP_PER_PLAYER: f64 = 6.0;
/// Horde scale for Rounds 1–5.
pub const HORDE_EARLY_SCALE: [f64; 5] = [0.25, 0.3, 0.5, 0.7, 0.9];
/// The Alive Cap through Round `ALIVE_CAP_GROWTH_FROM_ROUND`, and the floor Solo plays on.
pub const ALIVE_CAP: u32 = 24;
/// The floor a Co-op Run's Alive Cap holds up until the Round ramp passes it.
pub const ALIVE_CAP_COOP: u32 = 32;
/// The Alive Cap starts growing the Round after this one.
pub const ALIVE_CAP_GROWTH_FROM_ROUND: u32 = 10;
/// Added to the Alive Cap each Round past `ALIVE_CAP_GROWTH_FROM_ROUND`.
pub const ALIVE_CAP_PER_ROUND: u32 = 2;
/// The most the Round's Alive Cap grows to, from Round 22.
pub const ALIVE_CAP_MAX: u32 = 48;
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

// Hellhounds
/// The first Hellhound Round is drawn uniformly from these Rounds…
pub const HELLHOUND_FIRST_ROUNDS: std::ops::RangeInclusive<u32> = 5..=7;
/// …then each next one comes this many Rounds after the last, drawn uniformly.
pub const HELLHOUND_ROUND_GAPS: std::ops::RangeInclusive<u32> = 4..=5;
/// A Hellhound Round's Horde: `HELLHOUND_HORDE_EARLY` for the first `HELLHOUND_HORDE_EARLY_ROUNDS`
/// Hellhound Rounds, then `HELLHOUND_HORDE`.
pub const HELLHOUND_HORDE_EARLY: u32 = 6;
pub const HELLHOUND_HORDE_EARLY_ROUNDS: u32 = 2;
pub const HELLHOUND_HORDE: u32 = 8;
// (each is per Player: a Hellhound Round's Horde is N times these)
/// Hounds alive at once in a Hellhound Round, for each Player who is up (at least 1 in all).
pub const HELLHOUND_ALIVE_PER_PLAYER: u32 = 2;
/// The wait before a Hellhound Round's first hound.
pub const HELLHOUND_FIRST_SPAWN_SECS: f32 = 7.0;
/// Seconds between hounds in Hellhound Round #1, #2, #3 and #4 on.
pub const HELLHOUND_SPAWN_GAPS_SECS: [f32; 4] = [3.0, 2.5, 2.0, 1.5];
/// Hounds come in at Hellhound Spawn Points this far from the Player.
pub const HELLHOUND_SPAWN_MIN_DIST: f32 = 10.0;
pub const HELLHOUND_SPAWN_MAX_DIST: f32 = 25.0;
/// The lightning strike before a hound arrives; it can't be hurt, move or bite meanwhile.
pub const HELLHOUND_TELEGRAPH_SECS: f32 = 1.5;
/// Hound health in Hellhound Round #1, #2, #3 and #4 on.
pub const HELLHOUND_HEALTH: [f32; 4] = [400.0, 900.0, 1300.0, 1600.0];
pub const HELLHOUND_BITE_DAMAGE: f32 = 40.0;
/// Centre-to-centre distance at which a hound starts (and lands) a bite.
pub const HELLHOUND_REACH: f32 = 2.5;
/// Bites land this far apart while the Player stays in reach.
pub const HELLHOUND_BITE_SECS: f32 = 1.0;
/// The snap before a bite lands (invented); it's part of `HELLHOUND_BITE_SECS`.
pub const HELLHOUND_BITE_WIND_UP_SECS: f32 = 0.2;
/// Invented.
pub const HELLHOUND_SPEED: f32 = 7.0;
/// A hound's body is a ball this big, so it stands about 0.7 m tall.
pub const HELLHOUND_RADIUS: f32 = 0.35;
/// Mixed hounds: from each listed Round, this share of normal spawns are hounds instead.
pub const HELLHOUND_MIXED_CHANCE: [(u32, f64); 3] = [(16, 0.01), (21, 0.02), (31, 0.03)];
/// The most mixed hounds alive at once.
pub const HELLHOUND_MIXED_MAX_ALIVE: u32 = 2;
/// Hounds called in around a point (the Teleporter's) land between these distances from it (approx).
pub const HELLHOUND_AROUND_MIN_DIST: f32 = 2.0;
pub const HELLHOUND_AROUND_MAX_DIST: f32 = 5.0;

// Napalm and Shrieker, sharing one special slot
/// The first Napalm Round is drawn uniformly from these Rounds; after each Napalm's death the next
/// comes this many Rounds later.
pub const NAPALM_FIRST_ROUNDS: std::ops::RangeInclusive<u32> = 5..=7;
pub const NAPALM_ROUND_GAPS: std::ops::RangeInclusive<u32> = 1..=2;
/// Likewise for the Shrieker.
pub const SHRIEKER_FIRST_ROUNDS: std::ops::RangeInclusive<u32> = 4..=7;
pub const SHRIEKER_ROUND_GAPS: std::ops::RangeInclusive<u32> = 1..=3;
/// A special spawns once this share of the Horde has spawned, drawn uniformly each Round.
pub const SPECIAL_SPAWN_HORDE_FRACTION: std::ops::RangeInclusive<f64> = 0.25..=0.75;
/// A special never rises this close to the Player.
pub const SPECIAL_SPAWN_MIN_DIST: f32 = 5.0;
pub const NAPALM_RISE_SECS: f32 = 1.5;
pub const SHRIEKER_RISE_SECS: f32 = 1.0;
/// Health as a multiple of the Round's Zombie health.
pub const NAPALM_HEALTH_MULTIPLIER: f32 = 4.0;
pub const SHRIEKER_HEALTH_MULTIPLIER: f32 = 2.5;
pub const NAPALM_SWIPE_DAMAGE: f32 = 60.0;
/// A Napalm self-detonates with the Player within this far horizontally and vertically…
pub const NAPALM_DETONATE_HORIZONTAL: f32 = 2.3;
pub const NAPALM_DETONATE_VERTICAL: f32 = 1.3;
/// …at least this long after it spawned, after this wind-up.
pub const NAPALM_DETONATE_MIN_AGE_SECS: f32 = 2.0;
pub const NAPALM_WIND_UP_SECS: f32 = 0.75;
/// A Napalm's explosion hurts the Player in sight: `NAPALM_BLAST_CLOSE_DAMAGE` under
/// `NAPALM_BLAST_CLOSE_DIST`, `NAPALM_BLAST_MID_DAMAGE` out to `NAPALM_BLAST_MID_DIST`, then falling
/// to `NAPALM_BLAST_FAR_DAMAGE` at `NAPALM_BLAST_FAR_DIST`.
pub const NAPALM_BLAST_CLOSE_DIST: f32 = 2.3;
pub const NAPALM_BLAST_CLOSE_DAMAGE: f32 = 250.0;
pub const NAPALM_BLAST_MID_DIST: f32 = 3.8;
pub const NAPALM_BLAST_MID_DAMAGE: f32 = 100.0;
pub const NAPALM_BLAST_FAR_DIST: f32 = 10.2;
pub const NAPALM_BLAST_FAR_DAMAGE: f32 = 50.0;
/// Shellshock from a Napalm's blast: the longest at its centre, the shortest at its edge.
pub const NAPALM_SHELLSHOCK_SECS: std::ops::RangeInclusive<f32> = 1.5..=3.0;
/// The blast kills every Zombie in sight within this.
pub const NAPALM_KILL_RADIUS: f32 = 6.35;
/// The heat tint shows within this of a living Napalm.
pub const NAPALM_HEAT_DIST: f32 = 10.2;
/// Fire pools: on death, `NAPALM_DEATH_POOL_DAMAGE` to the Player every `NAPALM_POOL_TICK_SECS`
/// for `NAPALM_DEATH_POOL_SECS`; on spawn, `NAPALM_SPAWN_POOL_DAMAGE` for `NAPALM_SPAWN_POOL_SECS`.
pub const NAPALM_POOL_RADIUS: f32 = 2.0;
pub const NAPALM_POOL_TICK_SECS: f32 = 0.5;
pub const NAPALM_DEATH_POOL_SECS: f32 = 20.0;
pub const NAPALM_DEATH_POOL_DAMAGE: f32 = 40.0;
pub const NAPALM_SPAWN_POOL_SECS: f32 = 6.0;
pub const NAPALM_SPAWN_POOL_DAMAGE: f32 = 10.0;
/// Zombies walking into a fire pool burn and die, at most this many burning at once…
pub const NAPALM_POOL_MAX_BURNING: usize = 6;
/// …after burning this long (invented).
pub const NAPALM_POOL_BURN_SECS: f32 = 1.0;
/// A Player's kill of a Napalm pays this on top of its kill Points.
pub const NAPALM_KILL_BONUS: u64 = 300;
/// A Shrieker screams with the Player within this far horizontally and vertically, in front of
/// it (the dot of its facing with the way to the Player at least `SHRIEKER_SCREAM_MIN_DOT`)…
pub const SHRIEKER_SCREAM_HORIZONTAL: f32 = 7.6;
pub const SHRIEKER_SCREAM_VERTICAL: f32 = 1.8;
pub const SHRIEKER_SCREAM_MIN_DOT: f32 = 0.4;
/// …after this wind-up, then waits this long, drawn uniformly, before the next.
pub const SHRIEKER_WIND_UP_SECS: f32 = 0.5;
pub const SHRIEKER_COOLDOWN_SECS: std::ops::RangeInclusive<f32> = 6.0..=10.0;
/// A scream blurs the Player's view and shakes it.
pub const SHRIEKER_BLUR_SECS: f32 = 4.0;
pub const SHRIEKER_SHAKE_SECS: f32 = 3.0;
/// How far the view shakes at first, in metres (invented).
pub const SHRIEKER_SHAKE_AMPLITUDE: f32 = 0.05;
/// The death scream kills every Zombie in sight within this, after a delay drawn uniformly.
pub const SHRIEKER_DEATH_SCREAM_RADIUS: f32 = 12.2;
pub const SHRIEKER_DEATH_SCREAM_DELAY_SECS: std::ops::RangeInclusive<f32> = 0.0..=1.0;
/// A Shrieker pays hit Points on every this-many-th hit only.
pub const SHRIEKER_HIT_POINTS_EVERY: u32 = 2;
/// Seconds between a Shrieker's idle gasps, drawn uniformly.
pub const SHRIEKER_IDLE_SECS: std::ops::RangeInclusive<f32> = 2.0..=4.0;
// The Sapper
/// The first Sapper Round is drawn uniformly from these Rounds; each time one appears the next is
/// due this many Rounds later, drawn uniformly.
pub const SAPPER_FIRST_ROUNDS: std::ops::RangeInclusive<u32> = 8..=10;
pub const SAPPER_ROUND_GAPS: std::ops::RangeInclusive<u32> = 4..=6;
/// The announcer's warning plays this long before the Sapper comes through its window.
pub const SAPPER_WARNING_SECS: f32 = 2.0;
/// Health as a multiple of the Round's Zombie health, with no cap.
pub const SAPPER_HEALTH_MULTIPLIER: f32 = 8.0;
/// Its helmet comes off once it has lost this share of its max health; until then a Headshot
/// counts as a body hit.
pub const SAPPER_HELMET_FRACTION: f32 = 0.15;
pub const SAPPER_SPEED: f32 = 4.5;
/// The slam: `SAPPER_SLAM_DAMAGE` to a Player within `SAPPER_SLAM_REACH` in a
/// `SAPPER_SLAM_ARC_DEGREES` arc in front of it, after the wind-up, then the cooldown.
pub const SAPPER_SLAM_DAMAGE: f32 = 150.0;
pub const SAPPER_SLAM_REACH: f32 = 3.0;
pub const SAPPER_SLAM_ARC_DEGREES: f32 = 60.0;
pub const SAPPER_SLAM_WIND_UP_SECS: f32 = 0.8;
pub const SAPPER_SLAM_COOLDOWN_SECS: f32 = 2.5;
/// A Sabotage takes this long. Damage doesn't interrupt it; a stun starts it over.
pub const SAPPER_SABOTAGE_SECS: f32 = 3.0;
/// How close to a system it must stand to Sabotage it (invented).
pub const SAPPER_SABOTAGE_REACH: f32 = 3.0;
/// Holding the Use key on a Sabotaged system restores it for this.
pub const SAPPER_RESTORE_PRICE: u64 = 2000;
/// A Player's kill of the Sapper pays this on top of its kill Points.
pub const SAPPER_KILL_BONUS: u64 = 500;
/// The Thundergun stuns it for this long instead of killing or knocking it down.
pub const SAPPER_THUNDERGUN_STUN_SECS: f32 = 1.5;
/// An Ammo Mod proc that would kill it deals this share of its max health instead.
pub const SAPPER_AMMO_MOD_HEALTH_FRACTION: f32 = 0.05;
/// Dead Wire stuns it for this long as well.
pub const SAPPER_DEAD_WIRE_STUN_SECS: f32 = 1.0;
/// Stuck this long without progress toward its goal, it's moved to the path node nearest its
/// target instead of being cleaned up.
pub const SAPPER_STUCK_SECS: f32 = 10.0;
/// Seconds between its footsteps while it walks (invented).
pub const SAPPER_STEP_SECS: f32 = 0.55;
// Nova Crawlers
/// After Power, this share of normal spawns are Nova Crawlers instead, with at most
/// `NOVA_CRAWLER_MAX_ALIVE` alive and none on Hellhound Rounds.
pub const NOVA_CRAWLER_SPAWN_CHANCE: f64 = 0.10;
pub const NOVA_CRAWLER_MAX_ALIVE: u32 = 6;
/// A Nova Crawler's health, as a share of a normal Zombie's in the same Round.
pub const NOVA_CRAWLER_HEALTH_MULT: f32 = 0.75;
pub const NOVA_CRAWLER_SWIPE_DAMAGE: f32 = 45.0;
pub const NOVA_CRAWLER_SPEED: f32 = 4.0;
/// A Nova Crawler's body is a ball this big, so it's about 0.6 m tall.
pub const NOVA_CRAWLER_RADIUS: f32 = 0.3;
/// Seconds a Nova Crawler takes to drop from its Vent to the floor; it can be shot meanwhile.
pub const NOVA_VENT_DROP_SECS: f32 = 1.0;
/// How far above the floor a Vent opens.
pub const NOVA_VENT_HEIGHT: f32 = 3.5;
/// A bullet killing blow bursts a gas cloud this big, lasting this long, that blurs the Player
/// inside it.
pub const NOVA_GAS_RADIUS: f32 = 3.2;
pub const NOVA_GAS_SECS: f32 = 7.0;
/// How strongly the gas blurs the Player's view while they stand in it (invented).
pub const NOVA_GAS_BLUR_STRENGTH: f32 = 0.6;
/// The burst Shellshocks the Player within this distance, for `NOVA_SHELLSHOCK_SECS`, and kills
/// other Zombies within it (Unpaid).
pub const NOVA_BLAST_RADIUS: f32 = 2.4;
pub const NOVA_SHELLSHOCK_SECS: f32 = 2.5;

// Shellshock
/// The Player moves at this factor of their speed while Shellshocked.
pub const SHELLSHOCK_SPEED_MULT: f32 = 0.5;
/// Every other sound plays at this factor of its volume while Shellshocked: the muffled mix
/// (invented).
pub const SHELLSHOCK_MUFFLE_GAIN: f32 = 0.3;
/// How strongly the Shellshock overlay blurs the view (invented).
pub const SHELLSHOCK_OVERLAY_STRENGTH: f32 = 1.0;

// Audio (per-cue gains live beside the audio code)
/// At most this many idle Zombie vocals play at once, the nearest Zombies first.
pub const IDLE_VOCAL_CAP: usize = 6;
/// Seconds between one Zombie's idle vocals, by what it is (invented): a Walker moans slowly, a
/// Sprinter shrieks often.
pub const WALKER_IDLE_SECS: std::ops::RangeInclusive<f32> = 5.0..=9.0;
pub const JOGGER_IDLE_SECS: std::ops::RangeInclusive<f32> = 3.0..=6.0;
pub const SPRINTER_IDLE_SECS: std::ops::RangeInclusive<f32> = 1.5..=3.5;
pub const CRAWLER_IDLE_SECS: std::ops::RangeInclusive<f32> = 4.0..=8.0;
pub const NOVA_CRAWLER_IDLE_SECS: std::ops::RangeInclusive<f32> = 2.0..=4.0;
pub const HELLHOUND_IDLE_SECS: std::ops::RangeInclusive<f32> = 2.0..=4.0;
/// A Zombie due a vocal with no voice free tries again this soon (invented).
pub const IDLE_VOCAL_RETRY_SECS: f32 = 0.5;
/// A powered Perk machine plays its jingle this often, at random within the range.
pub const PERK_JINGLE_SECS: std::ops::RangeInclusive<f32> = 120.0..=240.0;
/// The heartbeat loops while the Player's Health is below this fraction of its max.
pub const LOW_HEALTH_FRACTION: f32 = 0.4;

// Fog
/// The Player's fog, linear from start to end.
pub const FOG_START: f32 = 10.0;
pub const FOG_END: f32 = 45.0;
/// The fog pulls in to this for a Hellhound Round (approx)…
pub const HELLHOUND_FOG_START: f32 = 3.0;
pub const HELLHOUND_FOG_END: f32 = 20.0;
/// …moving at this many metres a second (approx).
pub const FOG_PULL_SPEED: f32 = 8.0;

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
/// The south wing: into the Passage and the Tunnel from the rooms above, and through the
/// Storeroom between the Workshop and the Generator Room.
pub const DOOR_PRICE_DINING_PASSAGE: u64 = 1000;
pub const DOOR_PRICE_POWER_TUNNEL: u64 = 1000;
pub const DOOR_PRICE_WORKSHOP_STOREROOM: u64 = 1250;
pub const DOOR_PRICE_STOREROOM_GENERATOR: u64 = 1250;

// Buying
/// How far from a Buyable's use point the Player can buy it, height included.
pub const USE_RANGE: f32 = 2.0;
/// How far off the crosshair a Buyable's use point can be and still be bought.
pub const USE_ANGLE: f32 = 45.0_f32.to_radians();
/// How near a Barrier's inside face the Player must be to rebuild it, looking anywhere.
pub const BARRIER_REBUILD_RANGE: f32 = 2.5;
/// A Door's range override: its use point sits mid-Door, so this reaches the Door's full width.
pub const DOOR_USE_RANGE: f32 = USE_RANGE + 1.2;
/// An opening Door shakes for this long, then sinks into the floor over `DOOR_SINK_SECS`.
pub const DOOR_SHAKE_SECS: f32 = 0.25;
/// How long an opening Door takes to sink out of sight.
pub const DOOR_SINK_SECS: f32 = 0.6;
/// How far an opening Door sinks: a little more than its height, so it ends fully underground.
pub const DOOR_SINK_DEPTH: f32 = 3.2;
/// How far an opening Door shakes side to side, at most.
pub const DOOR_SHAKE_AMPLITUDE: f32 = 0.04;
pub const CANT_AFFORD_FLASH_SECS: f32 = 0.5;
/// An ammo buy costs half the Wall Weapon's price, rounded up to a multiple of this.
pub const AMMO_PRICE_STEP: u64 = 10;
/// The Dining Room's frag wall, which fills the Lethal slot to the most carried.
pub const FRAG_WALL_PRICE: u64 = 250;
/// The Power Room's Claymore wall and the Balcony's Bowie Knife are one-time buys.
pub const CLAYMORE_WALL_PRICE: u64 = 1000;
pub const BOWIE_KNIFE_WALL_PRICE: u64 = 3000;

/// An Upgraded Weapon's ammo buy, whatever its wall price.
pub const PAP_AMMO_PRICE: u64 = 4500;

// Pack-a-Punch
pub const PAP_PRICE: u64 = 5000;
/// How long the machine works on a Weapon before it can be taken.
pub const PAP_PROCESS_SECS: f32 = 3.0;
/// Seconds the Weapon takes to slide into the machine's slot, and again to slide back out.
pub const PAP_SLIDE_SECS: f32 = 0.6;
/// How long the Upgraded Weapon waits to be taken before it's lost, with no refund.
pub const PAP_PICKUP_SECS: f32 = 15.0;
/// Seconds for the glint band to sweep once along an Upgraded Weapon.
pub const GLINT_SWEEP_SECS: f32 = 1.2;

// Repack
/// A tap Repack: a random Ammo Mod the Upgraded Weapon lacks.
pub const REPACK_AMMO_MOD_PRICE: u64 = 2500;
/// Each earlier Repack of the same copy adds this to the price.
pub const REPACK_PRICE_STEP: u64 = 1000;
/// Ammo Mods one Weapon can hold.
pub const MAX_AMMO_MODS: usize = 2;
/// A hold Repack: a random Attachment in a free mount.
pub const REPACK_ATTACHMENT_PRICE: u64 = 2000;
/// How long the Use key is held to buy the hold half of a Buyable, like a Repack's Attachment
/// (approx). A shorter press is a tap.
pub const HOLD_BUY_SECS: f32 = 0.6;

// Attachments (Appendix A4). Each is its own modifier source on the Weapon copy.
/// Red Dot Sight: aimed Headshots, on top of the Headshot multiplier.
pub const RED_DOT_HEADSHOT_MULT: f32 = 1.5;
/// ACOG Sight: aimed shots pass this many more Zombies, keeping full damage through each.
pub const ACOG_EXTRA_PIERCE: f32 = 2.0;
pub const ACOG_PIERCE_DAMAGE_FACTOR: f32 = 1.0;
/// ACOG Sight: the factor on the aimed FOV.
pub const ACOG_ADS_FOV_MULT: f32 = 0.55;
pub const RAPID_FIRE_INTERVAL_MULT: f32 = 0.8;
pub const EXTENDED_MAG_MAGAZINE_MULT: f32 = 1.5;
pub const EXTENDED_MAG_RESERVE_MULT: f32 = 1.25;
/// Dual Mag: every second Reload takes this, flat; Speed Cola doesn't shorten it.
pub const DUAL_MAG_RELOAD_SECS: f32 = 0.4;
pub const GRIP_HIP_SPREAD_MULT: f32 = 0.6;
/// Seconds to bring the Underbarrel up, or back to the main gun.
pub const UNDERBARREL_TOGGLE_SECS: f32 = 0.5;
/// The Grenade Launcher's grenade speed (approx, as the China Lake's).
pub const GRENADE_LAUNCHER_SPEED: f32 = 30.0;
/// The Flamethrower's cone: its reach, and its half-angle in degrees (approx).
pub const FLAMETHROWER_RANGE: f32 = 6.0;
pub const FLAMETHROWER_CONE_DEGREES: f32 = 15.0;
/// Flamethrower ticks a second. Its tank and reserve are counted in ticks: 10 s and 30 s.
pub const FLAMETHROWER_TICKS_PER_SEC: f32 = 10.0;
/// Seconds of tank refilled from the reserve per second the Flamethrower isn't firing.
pub const FLAMETHROWER_RECHARGE_RATE: f32 = 1.0;
/// The Burn the Flamethrower leaves on every Zombie it touches (approx, as the Hades').
pub const FLAMETHROWER_BURN: Burn = HADES_BURN;

/// The Underbarrels as Weapon rows, so they fire, fall off and reload by the same rules. An
/// Underbarrel row's `id` is never read; each borrows the base Weapon nearest it.
pub const MASTERKEY: WeaponStats = WeaponStats {
    name: "Masterkey",
    damage_max: 240.0,
    damage_min: 240.0,
    magazine: 6,
    reserve_max: 30,
    // Two shells at a time, as Gut Shot.
    reload: shells(0.9, 0.3, 0.95),
    penetration: Penetration::Small,
    ..base(WeaponId::TrenchGun)
};
pub const GRENADE_LAUNCHER: WeaponStats = WeaponStats {
    name: "Grenade Launcher",
    damage_max: 1000.0,
    damage_min: 1000.0,
    splash: Some(Splash {
        max: 1000.0,
        min: 75.0,
        radius: 5.0,
    }),
    magazine: 1,
    reserve_max: 5,
    reserve_start: None,
    reload: reload(2.0, 2.0), // approx
    ..base(WeaponId::ChinaLake)
};
pub const FLAMETHROWER: WeaponStats = WeaponStats {
    name: "Flamethrower",
    fire: FireMode::Auto,
    damage_max: 100.0,
    damage_min: 100.0,
    pellets: 1,
    splash: None,
    burn: Some(FLAMETHROWER_BURN),
    magazine: (10.0 * FLAMETHROWER_TICKS_PER_SEC) as u32,
    reserve_max: (30.0 * FLAMETHROWER_TICKS_PER_SEC) as u32,
    reserve_start: None,
    fire_interval: 1.0 / FLAMETHROWER_TICKS_PER_SEC,
    falloff: Some(Falloff::NONE),
    ..base(WeaponId::ChinaLake)
};

// Ammo Mods: chance per damage event, and the cooldown a trigger starts.
pub const BLAST_FURNACE_CHANCE: f32 = 0.15;
pub const BLAST_FURNACE_COOLDOWN_SECS: f32 = 15.0;
pub const DEAD_WIRE_CHANCE: f32 = 0.20;
pub const DEAD_WIRE_COOLDOWN_SECS: f32 = 5.0;
pub const FIREWORKS_CHANCE: f32 = 0.10;
pub const FIREWORKS_COOLDOWN_SECS: f32 = 20.0;
pub const THUNDER_WALL_CHANCE: f32 = 0.25;
pub const THUNDER_WALL_COOLDOWN_SECS: f32 = 10.0;
pub const TURNED_CHANCE: f32 = 0.15;
pub const TURNED_COOLDOWN_SECS: f32 = 15.0;
/// Blast Furnace sets alight everything within this of the shot Zombie (120 in). Each burns for
/// `1 / BLAST_FURNACE_BURN_TICKS` of its health a tick and dies on the last.
pub const BLAST_FURNACE_RADIUS: f32 = 3.05;
pub const BLAST_FURNACE_BURN_TICKS: u32 = 6;
pub const BLAST_FURNACE_TICK_SECS: f32 = 0.5;
/// Dead Wire's jump radii by depth (100, 80, 60, 40, 20 in); the chain stops after the last.
pub const DEAD_WIRE_JUMP_RADII: [f32; 5] = [2.54, 2.03, 1.52, 1.02, 0.51];
/// Zombies one chain can hit, the shot one included.
pub const DEAD_WIRE_MAX_ZOMBIES: usize = 9;
/// Each Zombie hit takes its health plus this.
pub const DEAD_WIRE_EXTRA_DAMAGE: f32 = 666.0;
/// A Zombie deeper than the first jump is hit after a random delay in this range, times its depth.
pub const DEAD_WIRE_DELAY_MIN_SECS: f32 = 0.2;
pub const DEAD_WIRE_DELAY_MAX_SECS: f32 = 0.6;
/// The Fireworks phantom gun rises this far (56 in) over the rise, then fires its shots at
/// random Zombies within range, then sinks back over the rise.
pub const FIREWORKS_RISE: f32 = 1.42;
pub const FIREWORKS_RISE_SECS: f32 = 0.5;
pub const FIREWORKS_SHOTS: u32 = 10;
pub const FIREWORKS_SHOT_SECS: f32 = 0.1;
pub const FIREWORKS_RANGE: f32 = 15.2;
/// Candidates a shot checks for line of sight; past these it fires at the next one regardless.
pub const FIREWORKS_SIGHT_CHECKS: usize = 3;
/// Thunder Wall kills and flings up to this many, the shot Zombie included, within its radius
/// (180 in) and in front of the Player.
pub const THUNDER_WALL_RADIUS: f32 = 4.6;
pub const THUNDER_WALL_MAX_ZOMBIES: usize = 6;
/// How hard a Thunder Wall flings a body away, and its upward lift (approx).
pub const THUNDER_WALL_FLING_SPEED: f32 = 10.0;
pub const THUNDER_WALL_FLING_LIFT: f32 = 5.0;
/// The turn kills up to this many other Zombies within its radius (90 in), flinging them.
pub const TURNED_BLAST_RADIUS: f32 = 2.3;
pub const TURNED_BLAST_MAX_ZOMBIES: usize = 3;
pub const TURNED_FLING_SPEED: f32 = 6.0;
pub const TURNED_FLING_LIFT: f32 = 2.0;
/// A Turned ally dies after this long or this many kills, its last swing finishing first.
pub const TURNED_LIFETIME_SECS: f32 = 20.0;
pub const TURNED_MAX_KILLS: u32 = 12;
pub const TURNED_LAST_SWING_SECS: f32 = 0.5;

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
/// Quick Revive needs no Power.
pub const PERK_PRICE_QUICK_REVIVE: u64 = 500;
/// In Co-op, Quick Revive costs this and needs the Power.
pub const PERK_PRICE_QUICK_REVIVE_COOP: u64 = 1500;
/// Solo Quick Revive sells this many times, then its machine leaves.
pub const QUICK_REVIVE_SOLO_BUYS: u32 = 3;
/// After the last buy, the machine waits this long, then floats away over `QUICK_REVIVE_FLOAT_SECS`.
pub const QUICK_REVIVE_LEAVE_DELAY_SECS: f32 = 2.0;
pub const QUICK_REVIVE_FLOAT_SECS: f32 = 3.0;
/// How high the machine rises as it floats away (approx).
pub const QUICK_REVIVE_FLOAT_HEIGHT: f32 = 12.0;

// Traps
pub const TRAP_PRICE: u64 = 1000;
/// How long a Trap's switch takes to throw before the Trap starts running.
pub const TRAP_FLIP_SECS: f32 = 0.5;
pub const TRAP_RUN_SECS: f32 = 40.0;
/// After running, how long until the Trap can be bought again.
pub const TRAP_COOLDOWN_SECS: f32 = 60.0;
/// A Trap's volume across its doorway: width along the wall, height, depth through the doorway.
pub const TRAP_SIZE: Vec3 = Vec3::new(2.4, 3.0, 2.5);
/// A Zombie touching a running Trap dies after a random delay up to this.
pub const TRAP_KILL_DELAY_MAX_SECS: f32 = 1.25;
/// With Juggernog, each touch of a running Trap does this much and shocks the Player.
pub const TRAP_PLAYER_DAMAGE: f32 = 50.0;
/// How long a shock lasts. Still inside when it ends is another touch.
pub const TRAP_SHOCK_SECS: f32 = 2.5;
/// The shock's factor on move speed (approx).
pub const TRAP_SHOCK_SPEED_MULT: f32 = 0.5;
/// The Sapper takes this fraction of its max Health once per entry, plus a stun, instead of dying.
pub const TRAP_SAPPER_HEALTH_FRACTION: f32 = 0.2;
pub const TRAP_SAPPER_STUN_SECS: f32 = 2.0;

// Teleporter
/// After activating a Pad, the Player has this long to use the Mainframe and Link it.
pub const TELEPORTER_LINK_SECS: f32 = 30.0;
pub const TELEPORTER_PRICE: u64 = 1500;
/// Shared by both Pads, from the end of a trip.
pub const TELEPORTER_COOLDOWN_SECS: f32 = 5.0;
/// The Pad charges this long before the Player leaves.
pub const TELEPORTER_CHARGE_SECS: f32 = 2.0;
/// The charge kills up to `TELEPORTER_CHARGE_KILL_MAX` Zombies within this of the Pad (300 in).
pub const TELEPORTER_CHARGE_KILL_RADIUS: f32 = 7.6;
pub const TELEPORTER_CHARGE_KILL_MAX: usize = 20;
/// The Player must still be this close to the Pad when the charge ends (176 in), or the Points
/// are lost.
pub const TELEPORTER_STAY_RADIUS: f32 = 4.5;
/// How far apart, along the floor, a trip sets down the Players it carries in the holding space
/// and at the Mainframe.
pub const TELEPORTER_RIDER_SPACING: f32 = 1.2;
/// Seconds in the black holding space between the Pad and the Mainframe.
pub const TELEPORTER_HOLDING_SECS: f32 = 2.0;
/// On arrival, every Zombie within this of the Mainframe dies (300 in).
pub const TELEPORTER_ARRIVAL_KILL_RADIUS: f32 = 7.6;
/// The random aftereffect on arrival, fading out over its last `TELEPORTER_AFTEREFFECT_FADE_SECS`.
pub const TELEPORTER_AFTEREFFECT_SECS: f32 = 4.0;
pub const TELEPORTER_AFTEREFFECT_FADE_SECS: f32 = 1.0;
/// The FOV warp swings the FOV by up to this fraction, this many times a second (invented).
pub const TELEPORTER_FOV_WARP: f32 = 0.25;
pub const TELEPORTER_FOV_WARP_HZ: f32 = 1.5;
/// The arrival drop comes this long after arrival.
pub const TELEPORTER_DROP_DELAY_SECS: f32 = 2.0;
/// From this Round, 1 arrival in 6 spawns a Hellhound instead of a Power-up.
pub const TELEPORTER_HOUND_FROM_ROUND: u32 = 15;
pub const TELEPORTER_HOUND_CHANCE: f64 = 1.0 / 6.0;
/// From the Round after this one, each Round past it adds this chance that nothing drops.
pub const TELEPORTER_NOTHING_AFTER_ROUND: u32 = 15;
pub const TELEPORTER_NOTHING_CHANCE_PER_ROUND: f64 = 0.05;
/// Teleporting again within this long, above `TELEPORTER_RETELEPORT_ABOVE_ROUND`, with both Pads
/// Linked, spawns `TELEPORTER_RETELEPORT_HOUNDS` Hellhounds and gives no Power-up.
pub const TELEPORTER_RETELEPORT_SECS: f32 = 60.0;
pub const TELEPORTER_RETELEPORT_ABOVE_ROUND: u32 = 20;
pub const TELEPORTER_RETELEPORT_HOUNDS: u32 = 4;
/// A Pad's radius, and how far from its centre it can be used (approx).
pub const TELEPORTER_PAD_RADIUS: f32 = 1.2;
pub const TELEPORTER_PAD_USE_RANGE: f32 = 2.5;

// Last Stand
pub const LAST_STAND_SECS: f32 = 10.0;
/// A Co-op Last Stand lasts this long, then the Player Bleeds Out.
pub const LAST_STAND_COOP_SECS: f32 = 45.0;
/// A Co-op Down takes this percent of the Player's Points, and a Revive gives it to the reviver.
pub const LAST_STAND_COOP_POINTS_LOSS_PERCENT: u64 = 5;
/// A Bleed Out takes this fraction of every other Player's Points.
pub const BLEED_OUT_TEAM_LOSS_FRACTION: f32 = 0.10;
/// From this Round on, a Player who Respawns has at least `RESPAWN_POINTS_FLOOR` Points.
pub const RESPAWN_FLOOR_FROM_ROUND: u32 = 7;
pub const RESPAWN_POINTS_FLOOR: u64 = 1500;
/// A Respawned Player appears this far to the side of the teammate they come back beside, in metres.
pub const RESPAWN_BESIDE_DISTANCE: f32 = 1.5;
/// A Respawn spot needs floor within this far below the Player's feet, in metres.
pub const RESPAWN_FLOOR_REACH: f32 = 1.0;
/// A Player waiting to Respawn watches from this far behind, and this far above, the teammate they follow.
pub const SPECTATE_BEHIND_DISTANCE: f32 = 2.5;
pub const SPECTATE_LIFT: f32 = 0.6;
/// How close a reviver must stand to the Player in Last Stand, in metres (BO1's 75 units).
pub const REVIVE_REACH: f32 = 2.0;
/// How long the reviver holds Use to Revive, and with Quick Revive.
pub const REVIVE_SECS: f32 = 3.0;
pub const REVIVE_QUICK_SECS: f32 = 1.5;
/// The crawl's factor on walk speed.
pub const LAST_STAND_CRAWL_MULT: f32 = 0.15;
/// The Last Stand's pistol, Upgraded (Mustang & Sally), with a full Magazine and this many more
/// in Reserve Ammo.
pub const LAST_STAND_WEAPON: WeaponId = WeaponId::M1911;
pub const LAST_STAND_SPARE_MAGAZINES: u32 = 2;
/// Zombies walk away from a Player in Last Stand until they're about this far off.
pub const LAST_STAND_ZOMBIE_RETREAT_DISTANCE: f32 = 15.0;
/// How far the eye drops while crawling (approx).
pub const LAST_STAND_EYE_DROP: f32 = 0.9;
/// Health after a Self-Revive.
pub const SELF_REVIVE_HEALTH: f32 = 100.0;

// Mystery Box
pub const BOX_PRICE: u64 = 950;
/// A spin: 40 slowing steps through the pool before the Weapon shows.
pub const BOX_SPIN_SECS: f32 = 2.85;
pub const BOX_SPIN_STEPS: u32 = 40;
/// How long a spun Weapon waits to be taken before it's lost, with no refund.
pub const BOX_PICKUP_SECS: f32 = 12.0;
/// After a Weapon is taken or lost, how long until the Box can be spun again.
pub const BOX_REARM_SECS: f32 = 3.0;
/// The Teddy table, by completed pulls at this location: none before `FREE`, then the early
/// chance, then after `LATE` pulls certain at the first location, else the late chance, and the
/// latest chance from `LATEST` pulls.
pub const BOX_TEDDY_FREE_PULLS: u32 = 4;
pub const BOX_TEDDY_LATE_PULLS: u32 = 8;
pub const BOX_TEDDY_LATEST_PULLS: u32 = 13;
pub const BOX_TEDDY_CHANCE_EARLY: f64 = 0.15;
pub const BOX_TEDDY_CHANCE_FIRST_LOCATION: f64 = 1.0;
pub const BOX_TEDDY_CHANCE_LATE: f64 = 0.30;
pub const BOX_TEDDY_CHANCE_LATEST: f64 = 0.50;
/// The Teddy Bear sits this long, then flies up for `BOX_TEDDY_FLY_SECS`.
pub const BOX_TEDDY_RISE_SECS: f32 = 0.5;
pub const BOX_TEDDY_FLY_SECS: f32 = 4.0;
/// Then the Box shakes before it vanishes.
pub const BOX_SHAKE_SECS: f32 = 5.0;
/// How long after vanishing the Box reappears at its next location.
pub const BOX_MOVE_SECS: f32 = 12.0;

// Power-ups
/// A drop is armed once Points Earned passes the threshold: first `START`, then each gap is the
/// last ×`GROWTH`, counted from the Points Earned when it armed. A Co-op Run's first threshold is
/// `START + PER_PLAYER × N`, N the Players it starts with; Solo's is `START`.
pub const POWER_UP_THRESHOLD_START: f64 = 2000.0;
pub const POWER_UP_THRESHOLD_PER_PLAYER: f64 = 500.0;
pub const POWER_UP_THRESHOLD_GROWTH: f64 = 1.14;
/// The chance any eligible kill drops one anyway.
pub const POWER_UP_KILL_CHANCE: f64 = 0.03;
pub const POWER_UPS_PER_ROUND: u32 = 4;
/// A Power-up sits solid this long, then blinks for `POWER_UP_BLINK_DROP_SECS` and vanishes.
pub const POWER_UP_SOLID_SECS: f32 = 15.0;
pub const POWER_UP_BLINK_DROP_SECS: f32 = 11.5;
/// How long each blink shows, and hides, the Power-up.
pub const POWER_UP_BLINK_INTERVAL_SECS: f32 = 0.25;
/// How close the Player walks to take a Power-up (64 in, roughly), and how high it floats.
pub const POWER_UP_PICKUP_RADIUS: f32 = 1.6;
pub const POWER_UP_FLOAT_HEIGHT: f32 = 0.7;
/// Insta-Kill, Double Points and Fire Sale last this long. Picking one up again restarts it.
pub const POWER_UP_TIMED_SECS: f32 = 30.0;
/// Carpenter needs this many fully broken Barriers to come out of the bag.
pub const CARPENTER_MIN_BROKEN_BARRIERS: usize = 5;
pub const CARPENTER_POINTS: u64 = 200;
/// The Nuke kills each Zombie in play after a random delay in this range.
pub const NUKE_POINTS: u64 = 400;
pub const NUKE_DELAY_SECS: std::ops::RangeInclusive<f32> = 0.1..=0.7;
/// A Fire Sale Box's price.
pub const FIRE_SALE_BOX_PRICE: u64 = 10;

// HUD
/// The Round counter shows chalk tallies up to this Round, then numerals.
pub const ROUND_TALLY_MAX: u32 = 5;
/// How long a ±N Points popup lasts.
pub const HUD_POPUP_SECS: f32 = 1.2;
/// The Reload hint shows once the Magazine is down to this fraction of its size.
pub const LOW_MAGAZINE_FRACTION: f32 = 0.25;
/// A timed Power-up's bar blinks for its last this many seconds, at `POWER_UP_BLINK_INTERVAL_SECS`.
pub const POWER_UP_BLINK_SECS: f32 = 5.0;
/// An instant Power-up's name shows this long.
pub const INSTANT_POWER_UP_BANNER_SECS: f32 = 2.0;

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

// Camera sway (all approx, tuned by feel). Offsets are in metres, rates per second.
/// Ground covered by one full stride (two footsteps), which sets the bob's frequency.
pub const BOB_STRIDE_METRES: f32 = 4.4;
/// Side-to-side and downward head bob at walk speed, growing to the sprint values at sprint speed.
pub const BOB_WALK_LATERAL: f32 = 0.02;
pub const BOB_WALK_VERTICAL: f32 = 0.03;
pub const BOB_SPRINT_LATERAL: f32 = 0.04;
pub const BOB_SPRINT_VERTICAL: f32 = 0.055;
/// The Last Stand crawl's slow sway: one stride per this much ground, at full crawl speed.
pub const CRAWL_SWAY_STRIDE_METRES: f32 = 1.5;
pub const CRAWL_SWAY_LATERAL: f32 = 0.05;
pub const CRAWL_SWAY_VERTICAL: f32 = 0.02;
/// How quickly the bob eases in and out (starting, stopping, leaving the ground, aiming).
pub const SWAY_EASE_RATE: f32 = 10.0;
/// Landing dip: none below the minimum fall speed, then deeper per m/s, up to the max.
/// The Balcony drop lands at about 12 m/s.
pub const LANDING_DIP_MIN_SPEED: f32 = 3.0;
pub const LANDING_DIP_PER_SPEED: f32 = 0.012;
pub const LANDING_DIP_MAX: f32 = 0.12;
pub const LANDING_DIP_SECS: f32 = 0.35;
/// The view model's own bob against the camera, as a share of the head bob.
pub const WEAPON_BOB_SCALE: f32 = 0.5;
/// The view model trails mouse look: metres per radian per second of turn, up to the max, and a
/// matching tilt at the max.
pub const WEAPON_SWAY_LAG: f32 = 0.006;
pub const WEAPON_SWAY_LAG_MAX: f32 = 0.03;
pub const WEAPON_SWAY_TILT_MAX: f32 = 0.08;
pub const WEAPON_SWAY_EASE_RATE: f32 = 12.0;

// Records
/// Run Records kept per (Map, Mode).
pub const RECORDS_KEPT: usize = 10;

// Gauntlet
/// The Boon Offer opens this long after a Round's last kill.
pub const BOON_OFFER_DELAY_SECS: f32 = 2.0;
/// Distinct Boons on one Offer, or fewer if fewer are eligible.
pub const BOON_OFFER_SIZE: usize = 3;
/// Clicks are ignored for this long after the Offer appears, so a trigger finger can't pick.
pub const BOON_OFFER_INPUT_LOCKOUT_SECS: f32 = 0.5;
/// A Boon's Offer weight by Rarity: Common, Rare, Legendary.
pub const BOON_RARITY_WEIGHTS: [f64; 3] = [60.0, 30.0, 10.0];
/// Each Stack held multiplies a Boon's Offer weight by this.
pub const BOON_STACK_WEIGHT_FACTOR: f64 = 0.98;

// Boons (Appendix B), per Stack
pub const BOON_BLOOD_MONEY_POINTS_PER_KILL: u64 = 10;
pub const BOON_HOLLOW_POINTS_DAMAGE_INCREASE: f32 = 0.10;
pub const BOON_LIGHT_FEET_MOVE_SPEED_INCREASE: f32 = 0.05;
/// Added to the held Weapon's Headshot multiplier.
pub const BOON_MARKSMANS_EYE_HEADSHOT_MULTIPLIER: f32 = 0.5;
pub const BOON_FULL_BELT_MAGAZINE_INCREASE: f32 = 0.20;
pub const BOON_STRIPPER_CLIPS_BULLETS_PER_KILL: u32 = 1;
pub const BOON_SECOND_WIND_REGEN_DELAY_FACTOR: f32 = 0.85;
pub const BOON_TEDDYS_TITHE_POINTS: u64 = 500;
pub const BOON_BOTTOMLESS_CUP_PERK_PRICE_FACTOR: f32 = 0.9;
pub const BOON_LONG_FUSE_DURATION_SECS: f32 = 10.0;
pub const BOON_SCROUNGER_POWER_UPS_PER_ROUND: u32 = 1;
pub const BOON_SHORT_CIRCUIT_TRAP_COOLDOWN_FACTOR: f32 = 0.8;
pub const BOON_UNION_RATES_POINTS_PER_PLANK: u64 = 10;
/// Each Stack adds this many times the Classic Round cap on rebuild Points: (1 + n) × the cap.
pub const BOON_UNION_RATES_CAP_INCREASE: f32 = 1.0;
pub const BOON_TRENCH_RAIDER_KNIFE_DAMAGE_INCREASE: f32 = 0.50;
pub const BOON_BANDOLIER_LETHALS_CARRIED: u32 = 1;
pub const BOON_BANDOLIER_LETHALS_PER_ROUND: u32 = 1;
pub const BOON_WAR_BONDS_POINTS_PER_ROUND: u64 = 50;
pub const BOON_LOADED_DICE_FREE_SPIN_CHANCE: f32 = 0.15;
pub const BOON_FIELD_ENGINEER_PRICE_FACTOR: f32 = 0.9;
pub const BOON_LIVE_WIRE_COOLDOWN_FACTOR: f32 = 0.85;
pub const BOON_IRON_STOMACH_PERKS_KEPT: u32 = 1;
pub const BOON_LAST_RITES_QUICK_REVIVE_BUYS: u32 = 1;
pub const BOON_BOUNTY_POINTS_PER_TRAP_KILL: u64 = 50;
/// The fraction of its max Health a Zombie loses for each Plank it tears off.
pub const BOON_BARBED_WIRE_HEALTH_FRACTION: f32 = 0.10;
pub const BOON_BUTCHERS_BILL_DROP_CHANCE: f64 = 0.10;
pub const BOON_SHRAPNEL_DAMAGE_INCREASE: f32 = 0.25;
pub const BOON_SHRAPNEL_RADIUS_INCREASE: f32 = 0.10;
pub const BOON_THIRD_HAND_WEAPON_SLOTS: u32 = 1;
pub const BOON_THROUGH_AND_THROUGH_ZOMBIES: u32 = 1;
pub const BOON_BLACK_MARKET_UPGRADE_CHANCE: f32 = 0.20;
pub const BOON_DOUBLE_PUNCHED_AMMO_MOD_SLOTS: u32 = 1;
pub const BOON_LUCKY_STRIKE_CHANCE: f64 = 0.20;

// Pacts (Appendix B): fixed, held once, never scaled by Round
/// Every this many Rounds, a Pact Offer replaces the Boon Offer.
pub const PACT_OFFER_EVERY_ROUNDS: u32 = 5;
/// Distinct Pacts on one Pact Offer, or fewer if fewer are left.
pub const PACT_OFFER_SIZE: usize = 3;
pub const PACT_GLASS_JAW_DAMAGE_FACTOR: f32 = 2.0;
/// Taken off max Health: one swipe Downs without Juggernog.
pub const PACT_GLASS_JAW_MAX_HEALTH_LOSS: f32 = 40.0;
/// All Points ×2, added to Double Points' ×2 (×3 during it).
pub const PACT_GOLD_RUSH_POINTS_FACTOR: f32 = 2.0;
pub const PACT_GOLD_RUSH_PRICE_FACTOR: f32 = 1.25;
/// A killed Zombie's blast does this fraction of its max Health.
pub const PACT_SCORCHED_EARTH_HEALTH_FRACTION: f32 = 0.5;
pub const PACT_SCORCHED_EARTH_RADIUS_M: f32 = 2.0;
/// The blast goes off this high above the dead Zombie's feet, about its middle.
pub const PACT_SCORCHED_EARTH_BLAST_HEIGHT_M: f32 = 1.0;
pub const PACT_SCORCHED_EARTH_ZOMBIE_SPEED_FACTOR: f32 = 1.2;
pub const PACT_OPEN_BAR_HORDE_FACTOR: f32 = 1.25;
/// Self-Revives without Quick Revive, on top of it.
pub const PACT_UNDYING_SELF_REVIVES: u32 = 2;
pub const PACT_ARSENAL_BOX_PRICE_FACTOR: f32 = 2.0;
pub const PACT_ARSENAL_TEDDY_CHANCE_FACTOR: f32 = 2.0;
/// Pack-a-Punch and Repack prices.
pub const PACT_OVERCLOCKED_PRICE_FACTOR: f32 = 2.0;
pub const PACT_COLD_STEEL_RESERVE_FACTOR: f32 = 0.5;
pub const PACT_LIGHTNING_ROD_DROP_CHANCE_FACTOR: f32 = 0.5;
/// Damage beyond a kill carries to the nearest Zombie this close.
pub const PACT_OVERKILL_CARRY_RADIUS_M: f32 = 5.0;
/// Added to the Round's Alive Cap.
pub const PACT_OVERKILL_ALIVE_CAP_BONUS: u32 = 8;
pub const PACT_DEAD_RUN_MOVE_SPEED_FACTOR: f32 = 1.3;

// Scrip, Unlocks and Upgrade Tracks (Appendix B)
/// Unlock prices in Scrip. Commons and a few Rares are Unlocked from the first launch.
pub const UNLOCK_PRICE_RARE: u64 = 100;
pub const UNLOCK_PRICE_LEGENDARY: u64 = 250;
pub const UNLOCK_PRICE_PACT: u64 = 200;
/// Each Upgrade Track's Rank prices in Scrip, first Rank first. A Track has this many Ranks.
pub const UPGRADE_TRACK_RANK_PRICES: [u64; 5] = [50, 100, 150, 200, 250];
pub const TRACK_STARTING_POINTS_PER_RANK: u64 = 100;
pub const TRACK_WEAPON_DAMAGE_INCREASE_PER_RANK: f32 = 0.03;
pub const TRACK_MOVE_SPEED_INCREASE_PER_RANK: f32 = 0.02;
/// The Box spin price is multiplied by this once per Rank.
pub const TRACK_BOX_PRICE_FACTOR_PER_RANK: f32 = 0.95;
/// Extra seconds on timed Power-ups per Rank.
pub const TRACK_POWER_UP_DURATION_SECS_PER_RANK: f32 = 2.0;

// Muzzle flash, tracers and impact puffs
/// How long the muzzle flash and its light last after a shot, in seconds.
pub const MUZZLE_FLASH_SECS: f32 = 0.06;
/// The muzzle flash's size (edge of its cube, in metres) per class of Weapon.
pub const MUZZLE_FLASH_SIZE_SMALL: f32 = 0.05;
pub const MUZZLE_FLASH_SIZE_MEDIUM: f32 = 0.075;
pub const MUZZLE_FLASH_SIZE_LARGE: f32 = 0.115;
/// The muzzle light's brightness (lumens) at the instant of the shot, and how far it reaches.
pub const MUZZLE_LIGHT_INTENSITY: f32 = 120_000.0;
pub const MUZZLE_LIGHT_RANGE: f32 = 9.0;
/// How long an impact puff lasts, and the fastest it flies out, in metres per second.
pub const IMPACT_PUFF_SECS: f32 = 0.25;
pub const IMPACT_PUFF_SPEED: f32 = 2.5;
/// Puffs thrown by one impact on the world, on a Zombie, and on a Zombie's head.
pub const IMPACT_PUFFS_WORLD: u32 = 3;
pub const IMPACT_PUFFS_ZOMBIE: u32 = 4;
pub const IMPACT_PUFFS_HEADSHOT: u32 = 10;
/// Puff cube edge, in metres, for a normal impact and for a Headshot.
pub const IMPACT_PUFF_SIZE: f32 = 0.06;
pub const IMPACT_PUFF_SIZE_HEADSHOT: f32 = 0.09;
/// The most puffs alive at once, so a Train with an LMG stays cheap.
pub const IMPACT_PUFF_CAP: usize = 64;
/// Every this-many impacts draws a tracer from the muzzle, which lasts this long.
pub const TRACER_EVERY: u32 = 3;
pub const TRACER_SECS: f32 = 0.05;
pub const TRACER_WIDTH: f32 = 0.012;
/// How far a teammate's tracer reaches along their aim, in metres. Theirs has no impact to end at.
pub const TEAMMATE_TRACER_LENGTH: f32 = 14.0;

// Co-op: teammate footsteps
/// Seconds between a walking teammate's footsteps, and a sprinting one's.
pub const TEAMMATE_STEP_SECS: f32 = 0.5;
pub const TEAMMATE_SPRINT_STEP_SECS: f32 = 0.3;
/// A teammate slower than this, in metres per second, makes no footsteps.
pub const TEAMMATE_STEP_MIN_SPEED: f32 = 0.5;

// Co-op: the Room
/// The longest a Player's Name may be, in characters.
pub const PLAYER_NAME_MAX_CHARS: usize = 12;
/// The longest a Room's password may be, in characters.
pub const ROOM_PASSWORD_MAX_CHARS: usize = 16;
/// How many Players a Room holds: the Seats.
pub const ROOM_SEATS: usize = 4;
/// How many Players a Room needs before its Host can Start: a lone Host plays solo.
pub const ROOM_MIN_PLAYERS: usize = 2;
/// How often the Host tells the lobby the Room is still there, in seconds.
pub const LOBBY_BEAT_SECS: f32 = 10.0;
/// How long to wait before asking the lobby again after it couldn't be reached, in seconds.
pub const LOBBY_RETRY_SECS: f32 = 2.0;
/// How often the Join screen asks the lobby for the list afresh, in seconds.
pub const LOBBY_LIST_SECS: f32 = 5.0;

/// How far from a teammate a name tag is at full strength, and how far it has faded away to nothing.
pub const NAME_TAG_FADE_START: f32 = 12.0;
pub const NAME_TAG_FADE_END: f32 = 32.0;
/// How far above a teammate's head their name tag floats.
pub const NAME_TAG_LIFT: f32 = 0.3;
/// The size of a name tag's letters, in pixels.
pub const NAME_TAG_FONT_PX: f32 = 18.0;

// Co-op: movement over the network
/// How often the simulation steps: Bevy's default fixed rate, which the game never changes.
pub const FIXED_HZ: u32 = 64;
/// How often the Host tells each Player how things stand.
pub const SNAPSHOT_HZ: u32 = 32;
/// How many of a Player's last ticks of input each datagram carries, so one lost costs nothing.
pub const INPUT_RESEND: usize = 4;
/// How many snapshots behind the newest the other Players are shown, to always have two to
/// show between.
pub const INTERPOLATION_SNAPSHOTS: u32 = 2;
/// How many ticks of a remote Player's input the Host keeps in hand to start with, and how many
/// ticks it watches before keeping more or fewer.
pub const REMOTE_INPUT_DEPTH: u32 = 2;
pub const REMOTE_INPUT_WINDOW: u32 = 60;
/// A remote Player whose input has stopped coming for this many ticks stands still.
pub const REMOTE_STALL_TICKS: u32 = 32;
/// How many ticks of its own input a client keeps to replay, two seconds' worth.
pub const REPLAY_TICKS: usize = 128;
/// How far a position is told to, in metres: the 1 cm that three `i16`s give a 650 m box.
pub const WIRE_POSITION_STEP: f32 = 0.01;
/// How far a puppet may stray from where it should be, in ticks, before it eases back, and how far
/// before it jumps.
pub const PUPPET_SLACK_TICKS: f32 = 2.0;
pub const PUPPET_JUMP_TICKS: f32 = 8.0;
/// The most a snapshot may come to, in bytes: one QUIC datagram.
pub const SNAPSHOT_MAX_BYTES: usize = 1000;
/// The most Zombies a snapshot tells of (the Alive Cap's top end of 48 and Overkill's 8), and the
/// most Hellhounds (`HELLHOUND_ALIVE_PER_PLAYER` for each of 4 Players). Past that the dead and
/// then the highest ids are left out.
pub const SNAPSHOT_MAX_ZOMBIES: usize = 56;
pub const SNAPSHOT_MAX_HELLHOUNDS: usize = 8;
/// The most Special Zombies a snapshot names: the special slot's one, the Sapper, and the most Nova
/// Crawlers alive, with room to spare.
pub const SNAPSHOT_MAX_SPECIALS: usize = 12;
/// A Zombie that a snapshot has farther from where the one before had it than this has been
/// moved, not walked, and is shown there at once.
pub const PUPPET_SNAP_METRES: f32 = 6.0;
