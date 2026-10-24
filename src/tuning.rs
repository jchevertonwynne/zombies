//! Every gameplay number lives here, so balancing never means hunting through systems.

// Player
pub const PLAYER_RADIUS: f32 = 0.35;
pub const PLAYER_CAPSULE_LENGTH: f32 = 1.1;
pub const PLAYER_EYE_HEIGHT: f32 = 0.7; // above the capsule centre
pub const PLAYER_WALK_SPEED: f32 = 5.0;
pub const PLAYER_SPRINT_SPEED: f32 = 7.5;
pub const PLAYER_ACCELERATION: f32 = 60.0;
pub const PLAYER_JUMP_SPEED: f32 = 5.5;
pub const GRAVITY: f32 = 18.0;
pub const MOUSE_SENSITIVITY: f32 = 0.0025;

// Health
pub const PLAYER_MAX_HEALTH: f32 = 100.0;
pub const HEALTH_REGEN_DELAY_SECS: f32 = 3.0;
pub const HEALTH_REGEN_PER_SEC: f32 = 60.0;

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
pub const ZOMBIE_WALK_SPEED: f32 = 1.5;
pub const ZOMBIE_JOG_SPEED: f32 = 3.5;
pub const ZOMBIE_SPRINT_SPEED: f32 = 5.5;
/// Centre-to-centre distance at which a Zombie starts (and lands) a swing.
pub const ZOMBIE_REACH: f32 = 1.2;
pub const ZOMBIE_WIND_UP_SECS: f32 = 0.5;
pub const ZOMBIE_ATTACK_COOLDOWN_SECS: f32 = 1.0;
pub const ZOMBIE_DAMAGE: f32 = 50.0;
pub const ZOMBIE_FALL_SECS: f32 = 0.4;
pub const ZOMBIE_CORPSE_SECS: f32 = 2.0;

// Rounds
pub const ALIVE_CAP: u32 = 24;
pub const SPAWN_INTERVAL_SECS: f32 = 1.5;
pub const INTERMISSION_SECS: f32 = 10.0;

// Points
pub const POINTS_PER_HIT: u32 = 10;
pub const POINTS_BODY_KILL: u32 = 60;
pub const POINTS_HEADSHOT_KILL: u32 = 100;
pub const POINTS_KNIFE_KILL: u32 = 130;

// Map
pub const ARENA_HALF_SIZE: f32 = 20.0;
pub const WALL_HEIGHT: f32 = 4.0;
