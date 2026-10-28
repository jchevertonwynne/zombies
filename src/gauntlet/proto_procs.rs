//! PROTOTYPE, throwaway (wayfinder ticket #93): four Proc Boons from #92, to judge whether Proc
//! Boons make the power fantasy last. Never merge this to `main`.
//!
//! Press Insert during a Gauntlet Run to take a Stack of all four and let them turn up on later
//! Offers. Each Proc logs a line to the console when it fires, and a readout on the right shows
//! each one's Stacks and cooldown.

use bevy::prelude::*;

use super::boon::{BOONS, HeldBoons};
use crate::ammo_mod::{CastEffect, ProcElement};
use crate::player::{Player, PlayerHit};
use crate::weapon::Reloaded;
use crate::zombie::{Attack, Chilled, Dying, Zombie, ZombieKilled};

pub const ARC_LIGHT: &str = "proto_arc_light";
pub const HOT_BRASS: &str = "proto_hot_brass";
pub const ELBOW_ROOM: &str = "proto_elbow_room";
pub const FROSTBITE: &str = "proto_frostbite";
pub const IDS: [&str; 4] = [ARC_LIGHT, HOT_BRASS, ELBOW_ROOM, FROSTBITE];

const ARC_LIGHT_COOLDOWN_SECS: f32 = 2.0;
const HOT_BRASS_COOLDOWN_SECS: f32 = 8.0;
const ELBOW_ROOM_COOLDOWN_SECS: f32 = 15.0;
const FROSTBITE_RADIUS: f32 = 4.0;
const FROST_COLOUR: Color = Color::srgb(0.7, 0.9, 1.0);

// Stacks grow the Effect, never the chance or the cooldown.

pub fn arc_light_zombies(n: u32) -> u32 {
    3 + 2 * n.saturating_sub(1)
}

pub fn hot_brass_radius(n: u32) -> f32 {
    2.5 + 0.5 * n.saturating_sub(1) as f32
}

pub fn elbow_room_zombies(n: u32) -> u32 {
    6 + 3 * n.saturating_sub(1)
}

pub fn frostbite_secs(n: u32) -> f32 {
    3.0 + n.saturating_sub(1) as f32
}

/// Insert was pressed this Run: the prototype Proc Boons count as Unlocked on Offers.
#[derive(Resource, Default)]
pub struct ProtoProcs(pub bool);

/// Seconds left on each Proc Boon's cooldown: Arc Light, Hot Brass, Elbow Room.
#[derive(Resource, Default)]
pub struct ProcCooldowns([f32; 3]);

/// Frost bursts to draw: (where, radius, seconds left).
#[derive(Resource, Default)]
pub struct FrostFlashes(Vec<(Vec3, f32, f32)>);

pub fn reset(mut commands: Commands) {
    commands.insert_resource(ProtoProcs::default());
    commands.insert_resource(ProcCooldowns::default());
    commands.insert_resource(FrostFlashes::default());
}

pub fn grant(mut held: ResMut<HeldBoons>, mut proto: ResMut<ProtoProcs>) {
    proto.0 = true;
    for id in IDS {
        if let Some(boon) = BOONS.iter().find(|boon| boon.id == id) {
            held.add(boon);
        }
    }
    info!("PROTO: granted a Stack of each prototype Proc Boon");
}

fn feet(player: &Transform) -> Vec3 {
    player.translation - Vec3::Y * (crate::tuning::PLAYER_RADIUS + crate::tuning::PLAYER_CAPSULE_LENGTH / 2.0)
}

/// Every Trigger, read from the messages Classic sends.
pub fn fire_procs(
    mut kills: MessageReader<ZombieKilled>,
    mut reloads: MessageReader<Reloaded>,
    mut hits: MessageReader<PlayerHit>,
    held: Res<HeldBoons>,
    mut cooldowns: ResMut<ProcCooldowns>,
    mut frost: ResMut<FrostFlashes>,
    player: Single<&Transform, With<Player>>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut commands: Commands,
    mut casts: MessageWriter<CastEffect>,
    time: Res<Time>,
) {
    for left in &mut cooldowns.0 {
        *left = (*left - time.delta_secs()).max(0.0);
    }
    let at_player = feet(&player);
    for kill in kills.read() {
        // Proc damage never fires another Proc.
        if matches!(kill.attack, Attack::AmmoMod(_)) {
            continue;
        }
        let arc = held.stacks(ARC_LIGHT);
        if arc > 0 && kill.headshot && cooldowns.0[0] <= 0.0 {
            cooldowns.0[0] = ARC_LIGHT_COOLDOWN_SECS;
            casts.write(CastEffect {
                element: ProcElement::Shock,
                at: kill.at,
                size: arc_light_zombies(arc) as f32,
            });
            info!("PROTO proc: Arc Light (x{arc})");
        }
        let frostbite = held.stacks(FROSTBITE);
        if frostbite > 0 && matches!(kill.attack, Attack::Knife) {
            let secs = frostbite_secs(frostbite);
            let mut slowed = 0;
            for (zombie, at) in &zombies {
                if at.translation().with_y(0.0).distance(kill.at.with_y(0.0)) <= FROSTBITE_RADIUS {
                    commands.entity(zombie).insert(Chilled { left: secs });
                    slowed += 1;
                }
            }
            frost.0.push((kill.at, FROSTBITE_RADIUS, 0.4));
            info!("PROTO proc: Frostbite (x{frostbite}) slowed {slowed}");
        }
    }
    let brass = held.stacks(HOT_BRASS);
    for _ in reloads.read() {
        if brass > 0 && cooldowns.0[1] <= 0.0 {
            cooldowns.0[1] = HOT_BRASS_COOLDOWN_SECS;
            casts.write(CastEffect {
                element: ProcElement::Fire,
                at: at_player,
                size: hot_brass_radius(brass),
            });
            info!("PROTO proc: Hot Brass (x{brass})");
        }
    }
    let elbow = held.stacks(ELBOW_ROOM);
    for _ in hits.read() {
        if elbow > 0 && cooldowns.0[2] <= 0.0 {
            cooldowns.0[2] = ELBOW_ROOM_COOLDOWN_SECS;
            casts.write(CastEffect {
                element: ProcElement::Knockback,
                at: at_player,
                size: elbow_room_zombies(elbow) as f32,
            });
            info!("PROTO proc: Elbow Room (x{elbow})");
        }
    }
}

/// Frost wears off. A Frosted Zombie wears a pale blue ring at its feet and a halo.
pub fn tick_frost(
    mut commands: Commands,
    mut chilled: Query<(Entity, &mut Chilled, &GlobalTransform)>,
    mut frost: ResMut<FrostFlashes>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let flat = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    for (zombie, mut chill, at) in &mut chilled {
        chill.left -= dt;
        if chill.left <= 0.0 {
            commands.entity(zombie).remove::<Chilled>();
            continue;
        }
        let at = at.translation();
        gizmos.circle(Isometry3d::new(at - Vec3::Y * 0.85, flat), 0.45, FROST_COLOUR);
        gizmos.sphere(Isometry3d::from_translation(at + Vec3::Y * 0.9), 0.18, FROST_COLOUR);
    }
    frost.0.retain_mut(|(at, radius, left)| {
        *left -= dt;
        gizmos.circle(Isometry3d::new(*at + Vec3::Y * 0.05, flat), *radius, FROST_COLOUR);
        *left > 0.0
    });
}

/// Each Proc Boon's Stacks and cooldown, on the right of the screen.
#[derive(Component)]
pub struct ProcReadout;

pub fn spawn_readout(mut commands: Commands) {
    commands.spawn((
        crate::state::InRun,
        ProcReadout,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(FROST_COLOUR),
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(220.0),
            ..default()
        },
    ));
}

pub fn update_readout(
    held: Res<HeldBoons>,
    cooldowns: Res<ProcCooldowns>,
    mut readout: Single<&mut Text, With<ProcReadout>>,
) {
    let line = |name: &str, id: &str, cooldown: Option<f32>| {
        let n = held.stacks(id);
        match cooldown {
            _ if n == 0 => String::new(),
            Some(left) if left > 0.0 => format!("{name} x{n}  {left:.1}s\n"),
            _ => format!("{name} x{n}  READY\n"),
        }
    };
    let text = [
        line("Arc Light", ARC_LIGHT, Some(cooldowns.0[0])),
        line("Hot Brass", HOT_BRASS, Some(cooldowns.0[1])),
        line("Elbow Room", ELBOW_ROOM, Some(cooldowns.0[2])),
        line("Frostbite", FROSTBITE, None),
    ]
    .concat();
    if readout.0 != text {
        readout.0 = text;
    }
}
