//! Every gameplay number lives here, so balancing never means hunting through systems.

use bevy::math::Vec3;

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

// Weapon (M1911)
pub const PISTOL_MAGAZINE_SIZE: u32 = 8;
pub const PISTOL_DAMAGE: f32 = 40.0;
pub const PISTOL_FIRE_COOLDOWN_SECS: f32 = 0.15;
pub const PISTOL_RELOAD_SECS: f32 = 1.6;
pub const PISTOL_RANGE: f32 = 100.0;
pub const HEADSHOT_MULTIPLIER: f32 = 3.5;

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

// Map
pub const ARENA_HALF_SIZE: f32 = 20.0;
pub const WALL_HEIGHT: f32 = 4.0;

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
