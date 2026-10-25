//! Every gameplay number lives here, so balancing never means hunting through systems.

use bevy::math::Vec3;

use crate::arsenal::{
    Availability, FireMode, Handling, Penetration, ReloadTime, Splash, WeaponClass, WeaponId, WeaponStats,
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

// Weapon handling by class (Appendix A2, approx), in `WeaponClass` order.
pub const WEAPON_HANDLING: [Handling; 12] = [
    handling(WeaponClass::Pistol, 0.55, 0.45),
    handling(WeaponClass::DualWield, 0.3, 0.3),
    handling(WeaponClass::Smg, 0.75, 0.5),
    handling(WeaponClass::Rifle, 0.8, 0.6),
    handling(WeaponClass::Shotgun, 0.75, 0.6),
    handling(WeaponClass::Sniper, 1.2, 0.9),
    handling(WeaponClass::Lmg, 0.95, 0.65),
    handling(WeaponClass::Launcher, 1.2, 1.05),
    handling(WeaponClass::RayGun, 0.8, 0.8),
    handling(WeaponClass::Thundergun, 1.1, 0.83),
    handling(WeaponClass::Crossbow, 0.9, 0.75),
    handling(WeaponClass::BallisticKnife, 0.35, 0.35),
];

const fn handling(class: WeaponClass, raise_secs: f32, drop_secs: f32) -> Handling {
    Handling {
        class,
        raise_secs,
        drop_secs,
    }
}

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
    magazine: 0,
    reserve_max: 0,
    reserve_start: None,
    fire_interval: SEMI_CAP,
    reload: reload(0.0, 0.0),
    headshot: 1.0,
    penetration: Penetration::None,
};

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
