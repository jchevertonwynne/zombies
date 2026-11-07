//! Hit Grace: once a Zombie-family melee hit lands on a Player, no other Zombie-family melee
//! hurts them for `HIT_GRACE_SECS`. Explosions, Traps and fire still do.

use super::headless::Sim;
use super::*;
use crate::map::PLAYER_START;
use crate::player::{Health, Player, take_hits};

/// The Sim, with hits taking Health, and its stand-in Player at full Health.
fn sim() -> (Sim, Entity) {
    let mut sim = Sim::new(60.0, |app| {
        app.add_systems(Update, take_hits.after(attack));
    });
    let world = sim.world_mut();
    let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    world.entity_mut(player).insert(Health {
        current: PLAYER_MAX_HEALTH,
        max: PLAYER_MAX_HEALTH,
        since_hit: 0.0,
    });
    (sim, player)
}

/// A Walker beside the Player, `side` metres along x, set to land its swing in `secs`.
fn swinging(sim: &mut Sim, side: f32, secs: f32) -> Entity {
    let zombie = sim.spawn(PLAYER_START + Vec3::X * side, None);
    sim.run(1);
    swing_in(sim, zombie, secs);
    zombie
}

fn swing_in(sim: &mut Sim, zombie: Entity, secs: f32) {
    let mut body = sim.world_mut().get_mut::<Zombie>(zombie).unwrap();
    body.wind_up = Some(secs);
    body.attack_cooldown = 0.0;
}

/// Runs `secs`, returning the frame and the hit of every `PlayerHit` sent.
fn sent_over(sim: &mut Sim, secs: f32) -> Vec<(usize, PlayerHit)> {
    let mut sent = Vec::new();
    for frame in 0..sim.frames(secs) {
        sim.run(1);
        sent.extend(sim.world_mut().resource_mut::<Messages<PlayerHit>>().drain().map(|hit| (frame, hit)));
    }
    sent
}

/// Runs `secs`, returning the damage of every `PlayerHit` sent.
fn hits_over(sim: &mut Sim, secs: f32) -> Vec<f32> {
    sent_over(sim, secs).into_iter().map(|(_, hit)| hit.damage).collect()
}

fn health(sim: &mut Sim, player: Entity) -> f32 {
    sim.world_mut().get::<Health>(player).unwrap().current
}

fn since_hit(sim: &mut Sim, player: Entity) -> f32 {
    sim.world_mut().get::<Health>(player).unwrap().since_hit
}

#[test]
fn two_swipes_in_the_same_tick_do_one_swipes_damage() {
    let (mut sim, player) = sim();
    let a = swinging(&mut sim, 0.9, 0.05);
    let b = swinging(&mut sim, -0.9, 0.05);
    swing_in(&mut sim, a, 0.05);
    let hits = hits_over(&mut sim, 0.1);
    assert_eq!(hits, [ZOMBIE_SWIPE_DAMAGE], "both swipes landed");
    assert_eq!(health(&mut sim, player), PLAYER_MAX_HEALTH - ZOMBIE_SWIPE_DAMAGE);
    // The blocked swipe is spent all the same.
    for zombie in [a, b] {
        assert!(sim.world_mut().get::<Zombie>(zombie).unwrap().attack_cooldown > 0.0, "a Zombie skipped its cooldown");
    }
}

#[test]
fn a_second_swipe_inside_hit_grace_does_nothing_and_one_after_it_lands() {
    let (mut sim, player) = sim();
    let a = swinging(&mut sim, 0.9, 0.05);
    assert_eq!(hits_over(&mut sim, 0.1), [ZOMBIE_SWIPE_DAMAGE]);
    // Just inside Hit Grace: no hit, no damage, and the regen delay runs on.
    let b = swinging(&mut sim, -0.9, HIT_GRACE_SECS - 0.25);
    swing_in(&mut sim, a, 10.0);
    // This Sim doesn't regenerate, so the regen delay holds wherever it's put.
    sim.world_mut().get_mut::<Health>(player).unwrap().since_hit = 1.0;
    assert_eq!(hits_over(&mut sim, HIT_GRACE_SECS - 0.2), [] as [f32; 0], "a swipe inside Hit Grace was sent");
    assert_eq!(health(&mut sim, player), PLAYER_MAX_HEALTH - ZOMBIE_SWIPE_DAMAGE);
    assert_eq!(since_hit(&mut sim, player), 1.0, "the blocked swipe reset the regen delay");
    assert!(sim.world_mut().get::<Zombie>(b).unwrap().attack_cooldown > 0.0, "the blocked swipe wasn't spent");
    // Past it, the next swipe lands as ever.
    swing_in(&mut sim, a, 0.25);
    assert_eq!(hits_over(&mut sim, 0.3), [ZOMBIE_SWIPE_DAMAGE], "a swipe after Hit Grace didn't land");
}

#[test]
fn a_lone_zombie_swipes_as_it_always_has() {
    let (mut sim, _player) = sim();
    swinging(&mut sim, 0.9, 0.05);
    // It winds up and swipes again after its cooldown, which outlasts Hit Grace.
    let hits = hits_over(&mut sim, 0.1 + ZOMBIE_HIT_COOLDOWN_SECS + ZOMBIE_WIND_UP_SECS + 0.1);
    assert_eq!(hits, [ZOMBIE_SWIPE_DAMAGE, ZOMBIE_SWIPE_DAMAGE]);
}

/// A Sapper beside the Player, facing them, set to slam in `secs`.
fn slamming(sim: &mut Sim, side: f32, secs: f32) -> Entity {
    let sapper = swinging(sim, side, secs);
    let world = sim.world_mut();
    let at = world.get::<Transform>(sapper).unwrap().translation;
    let mut body = Zombie::special(SpecialKind::Sapper, 1000.0, at);
    body.wind_up = Some(secs);
    world.entity_mut(sapper).insert(body);
    world.get_mut::<Transform>(sapper).unwrap().look_at(PLAYER_START.with_y(at.y), Vec3::Y);
    sapper
}

#[test]
fn a_sapper_slam_inside_hit_grace_does_nothing() {
    let (mut sim, player) = sim();
    swinging(&mut sim, 0.9, 0.05);
    slamming(&mut sim, -0.9, 0.2);
    assert_eq!(hits_over(&mut sim, 0.3), [ZOMBIE_SWIPE_DAMAGE], "the slam landed inside Hit Grace");
    assert_eq!(health(&mut sim, player), PLAYER_MAX_HEALTH - ZOMBIE_SWIPE_DAMAGE);
}

#[test]
fn a_sapper_slam_that_lands_opens_hit_grace() {
    let (mut sim, _player) = sim();
    slamming(&mut sim, -0.9, 0.05);
    swinging(&mut sim, 0.9, 0.2);
    assert_eq!(hits_over(&mut sim, 0.3), [SAPPER_SLAM_DAMAGE], "a swipe landed inside the slam's Hit Grace");
}

/// An explosion, a Trap or a fire pool: a `PlayerHit` sent straight to the Player.
fn environmental(sim: &mut Sim, player: Entity, damage: f32) {
    sim.send(PlayerHit { player, damage });
}

#[test]
fn environmental_harm_hurts_inside_hit_grace() {
    let (mut sim, player) = sim();
    swinging(&mut sim, 0.9, 0.05);
    hits_over(&mut sim, 0.1);
    environmental(&mut sim, player, 10.0);
    sim.run(1);
    assert_eq!(health(&mut sim, player), PLAYER_MAX_HEALTH - ZOMBIE_SWIPE_DAMAGE - 10.0);
}

#[test]
fn environmental_harm_opens_no_hit_grace() {
    let (mut sim, player) = sim();
    environmental(&mut sim, player, 10.0);
    sim.run(1);
    swinging(&mut sim, 0.9, 0.05);
    assert_eq!(hits_over(&mut sim, 0.1), [ZOMBIE_SWIPE_DAMAGE], "a swipe after a blast was blocked");
    assert_eq!(health(&mut sim, player), PLAYER_MAX_HEALTH - 10.0 - ZOMBIE_SWIPE_DAMAGE);
}

/// Three Zombies reaching through a Start Room window and a fourth beside the Player
/// inside it: however their swipes fall, no two hits land within Hit Grace of each other.
#[test]
fn reach_through_swipes_keep_to_hit_grace_too() {
    use crate::barrier::{Entering, PlankTorn, enter_through_windows};
    use crate::map::{BarrierId, window_spawn};

    let barrier = BarrierId(0);
    let mut sim = Sim::new(60.0, |app| {
        app.add_message::<PlankTorn>()
            .add_message::<crate::audio::PlayCue>()
            .insert_resource(crate::round::Round::shown(1, None))
            .add_systems(FixedUpdate, enter_through_windows);
    });
    // Just inside the window, which faces +Z from (-7, 26).
    let feet = Vec3::new(-7.0, 0.0, 25.3);
    let world = sim.world_mut();
    let player = world.query_filtered::<Entity, With<Player>>().single(world).unwrap();
    world.get_mut::<Transform>(player).unwrap().translation = feet + Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0);
    for along in [-0.55, 0.0, 0.55] {
        let zombie = sim.spawn(window_spawn(barrier) + Vec3::X * along, None);
        sim.world_mut().entity_mut(zombie).insert(Entering::new(barrier));
    }
    sim.spawn(feet + Vec3::new(0.9, 0.0, -0.3), None);

    let frame_secs = sim.frame_secs;
    let landed: Vec<f32> = sent_over(&mut sim, 20.0).into_iter().map(|(frame, _)| frame as f32 * frame_secs).collect();
    assert!(landed.len() >= 5, "only {} hits landed", landed.len());
    for pair in landed.windows(2) {
        assert!(pair[1] - pair[0] >= HIT_GRACE_SECS - 1e-3, "hits {:.3} s apart, at {:.3} and {:.3}", pair[1] - pair[0], pair[0], pair[1]);
    }
}

/// Each Player has their own Hit Grace: a swipe on one shields nobody else.
#[test]
fn hit_grace_is_per_player() {
    let (mut sim, player) = sim();
    let other_feet = PLAYER_START + Vec3::new(4.0, 0.0, 0.0);
    let other = sim
        .world_mut()
        .spawn((
            Player,
            crate::modifier::Modifiers::default(),
            crate::zone::PlayerZone::default(),
            Transform::from_translation(other_feet + Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0)),
        ))
        .id();
    let a = swinging(&mut sim, -0.9, 0.05);
    let b = sim.spawn(other_feet + Vec3::X * 0.9, None);
    sim.run(1);
    swing_in(&mut sim, a, 0.05);
    swing_in(&mut sim, b, 0.05);
    let mut hit: Vec<Entity> = sent_over(&mut sim, 0.1).into_iter().map(|(_, hit)| hit.player).collect();
    hit.sort();
    let mut both = vec![player, other];
    both.sort();
    assert_eq!(hit, both, "one Player's Hit Grace shielded the other");
}
