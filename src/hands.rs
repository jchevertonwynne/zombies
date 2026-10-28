//! How the Player's hands move on the view model: Reloads and the Knife. Pure poses here; the
//! view model applies them (see `crate::weapon`).
//!
//! A magazine-fed Weapon is tipped up and over while the left hand pulls the magazine down out of
//! sight, brings a full one back up and seats it. Anything else (shells, a cylinder, a rocket,
//! the Wonder Weapons) is rolled the other way while the left hand feeds it, again and again. The
//! Knife is a right hand slashing across the screen while the Weapon drops out of the way; the
//! Ballistic Knife in hand lunges and stabs with its own blade instead. A Perk drink puts the
//! Weapon away, brings the Perk's bottle up in the right hand, pops its cap, tips it back and
//! tosses it away before the Weapon comes back up.
//!
//! Aiming and sprinting move the whole Weapon: aiming brings the sights up under the crosshair,
//! and sprinting lets the muzzle drop and roll in to the left, BO1-style.

use bevy::prelude::*;

/// How a Weapon is reloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadStyle {
    /// Swap the magazine.
    Magazine,
    /// The left hand feeds it by hand.
    Feed,
}

/// The view model mid-Reload: its whole turn and shift, and where the left hand and the magazine
/// have moved from their rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReloadPose {
    pub turn: Quat,
    pub shift: Vec3,
    pub hand: Vec3,
    pub magazine: Vec3,
}

impl ReloadPose {
    pub const REST: ReloadPose = ReloadPose {
        turn: Quat::IDENTITY,
        shift: Vec3::ZERO,
        hand: Vec3::ZERO,
        magazine: Vec3::ZERO,
    };
}

/// How far below its seat a magazine goes while it's swapped: well out of sight.
const MAGAZINE_DROP: f32 = 0.5;
/// Seconds per feed of the left hand.
const FEED_SECS: f32 = 0.42;

/// Where the view model sits at the hip, from the camera: right and down. (Its depth depends on
/// the model's length, so that's the view model's own.)
pub const HIP_SIDE: f32 = 0.22;
pub const HIP_DROP: f32 = 0.2;
/// How much closer to the camera the Weapon is pulled when it's aimed.
const AIM_CLOSER: f32 = 0.06;
/// How much of its kick is left when it's fully aimed.
const AIMED_KICK: f32 = 0.35;

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The pose `elapsed` seconds into a Reload of `secs`. `grab` is where the left hand takes the
/// magazine from (Magazine) or feeds the gun at (Feed), from its rest.
pub fn reload_pose(style: ReloadStyle, elapsed: f32, secs: f32, grab: Vec3) -> ReloadPose {
    let t = if secs > 0.0 { (elapsed / secs).clamp(0.0, 1.0) } else { 1.0 };
    // In and out of the tipped-over pose at either end.
    let tipped = smoothstep(t / 0.12) * smoothstep((1.0 - t) / 0.12);
    match style {
        ReloadStyle::Magazine => {
            let magazine = if t < 0.17 {
                0.0
            } else if t < 0.35 {
                -MAGAZINE_DROP * smoothstep((t - 0.17) / 0.18)
            } else if t < 0.45 {
                -MAGAZINE_DROP
            } else if t < 0.68 {
                (-MAGAZINE_DROP).lerp(-0.05, smoothstep((t - 0.45) / 0.23))
            } else {
                -0.05 * (1.0 - smoothstep((t - 0.68) / 0.06))
            };
            // Reaching for the magazine, and letting go of it once it's seated.
            let reach = smoothstep((t - 0.03) / 0.12) * (1.0 - smoothstep((t - 0.74) / 0.16));
            // A little knock as the new one seats.
            let seat = (1.0 - ((t - 0.74) / 0.04).abs()).max(0.0);
            ReloadPose {
                turn: Quat::from_rotation_z(0.45 * tipped) * Quat::from_rotation_x(0.25 * tipped),
                shift: Vec3::new(-0.06, 0.02 + 0.015 * seat, 0.02) * tipped,
                hand: grab * reach + Vec3::Y * magazine,
                magazine: Vec3::Y * magazine,
            }
        }
        ReloadStyle::Feed => {
            let reach = smoothstep((t - 0.02) / 0.1) * (1.0 - smoothstep((t - 0.86) / 0.12));
            // Down for the next round and back up into the gun, once per feed.
            let feeding = (elapsed - 0.12 * secs).max(0.0) / FEED_SECS;
            let away = if (0.12..0.86).contains(&t) {
                1.0 - (feeding * std::f32::consts::PI).sin().abs()
            } else {
                0.0
            };
            ReloadPose {
                turn: Quat::from_rotation_z(-0.35 * tipped) * Quat::from_rotation_x(0.15 * tipped),
                shift: Vec3::new(-0.04, 0.03, 0.0) * tipped,
                hand: grab * reach + Vec3::new(-0.04, -0.12, 0.06) * away * reach,
                magazine: Vec3::ZERO,
            }
        }
    }
}

/// How much of an aim or a carry is left while another pose (a switch, a Reload, the Knife, a
/// Perk) has taken over by `busy`, 0 (not at all) to 1 (completely). Never jumps.
fn left_alone(busy: f32) -> f32 {
    1.0 - smoothstep(busy)
}

/// How far the view model moves from the hip when `aim` (0 hip to 1 aimed) of the way up: over to
/// the middle, raised until the sights (`sight_height` above the model's origin) are at the screen
/// centre, and a little closer. Zero at the hip.
pub fn aim_shift(aim: f32, sight_height: f32, busy: f32) -> Vec3 {
    let amount = smoothstep(aim) * left_alone(busy);
    Vec3::new(-HIP_SIDE, HIP_DROP - sight_height, AIM_CLOSER) * amount
}

/// How much of its kick the Weapon keeps at `aim`: all of it at the hip, less when aimed.
pub fn aimed_kick(kick: f32, aim: f32) -> f32 {
    kick * (1.0 - (1.0 - AIMED_KICK) * smoothstep(aim))
}

/// The carry while sprinting, `carry` of the way (0 to 1) there: the Weapon sinks, its muzzle
/// drops and it rolls in to the left. Returns how far it moves, and how it turns.
pub fn carry_pose(carry: f32, busy: f32) -> (Vec3, Quat) {
    let amount = smoothstep(carry) * left_alone(busy);
    let turn = Quat::from_rotation_y(0.25 * amount)
        * Quat::from_rotation_x(-0.45 * amount)
        * Quat::from_rotation_z(0.5 * amount);
    (Vec3::new(-0.04, -0.08, 0.03) * amount, turn)
}

/// A Knife slash `t` of the way (0 to 1) through: how far the Weapon drops out of the way, and
/// where the knife hand is (camera space), if it's in sight.
pub fn knife_pose(t: f32) -> (f32, Option<Transform>) {
    let t = t.clamp(0.0, 1.0);
    let dip = smoothstep(t / 0.12) * (1.0 - smoothstep((t - 0.6) / 0.3));
    if t >= 0.85 {
        return (dip, None);
    }
    // In from the bottom right, cocked up by the right ear, slashed hard across, and away down to
    // the left.
    let off_right = Vec3::new(0.45, -0.5, -0.35);
    let cocked = Vec3::new(0.28, -0.04, -0.42);
    let slashed = Vec3::new(-0.22, -0.2, -0.5);
    let off_left = Vec3::new(-0.45, -0.6, -0.35);
    let (at, swing) = if t < 0.15 {
        (off_right.lerp(cocked, smoothstep(t / 0.15)), 0.0)
    } else if t < 0.4 {
        // Accelerating into the cut.
        let cut = ((t - 0.15) / 0.25).powi(2);
        (cocked.lerp(slashed, cut), cut)
    } else {
        (slashed.lerp(off_left, smoothstep((t - 0.4) / 0.45)), 1.0)
    };
    let rotation = Quat::from_rotation_y((-1.2f32).lerp(1.1, swing)) * Quat::from_rotation_x(0.2);
    (dip, Some(Transform::from_translation(at).with_rotation(rotation)))
}

/// The Ballistic Knife's stab, `t` of the way through: how far forward the view model lunges.
pub fn stab_lunge(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.2 {
        0.22 * smoothstep(t / 0.2)
    } else {
        0.22 * (1.0 - smoothstep((t - 0.2) / 0.5))
    }
}

/// A Perk drink `t` of the way (0 to 1) through: how far the Weapon is put away (1 for out of
/// sight), and where the bottle hand is (camera space), if it's in sight.
pub fn drink_pose(t: f32) -> (f32, Option<Transform>) {
    let t = t.clamp(0.0, 1.0);
    // Down at once, and back up once the bottle is gone, in time for the Perk to apply.
    let lowered = smoothstep(t / 0.1) * (1.0 - smoothstep((t - 0.84) / 0.16));
    if !(0.08..0.86).contains(&t) {
        return (lowered, None);
    }
    let below = Vec3::new(0.3, -0.55, -0.4);
    let held = Vec3::new(0.14, -0.19, -0.4);
    // Up under the nose, the neck pointing back at the mouth.
    let mouth = Vec3::new(0.04, -0.06, -0.27);
    let away = Vec3::new(0.45, -0.7, -0.3);
    // How far it's tipped back toward the mouth, leant in from the right, and spun as it's tossed.
    let (at, tip, lean, spin) = if t < 0.24 {
        (below.lerp(held, smoothstep((t - 0.08) / 0.16)), 0.0, 0.2, 0.0)
    } else if t < 0.32 {
        // A flick as the cap pops off.
        let flick = (1.0 - ((t - 0.28) / 0.04).abs()).max(0.0);
        (held + Vec3::Y * 0.02 * flick, -0.2 * flick, 0.2, 0.0)
    } else if t < 0.44 {
        let up = smoothstep((t - 0.32) / 0.12);
        (held.lerp(mouth, up), 1.9 * up, 0.2.lerp(0.35, up), 0.0)
    } else if t < 0.72 {
        // Three glugs, tipping it further as it empties.
        let k = (t - 0.44) / 0.28;
        let glug = (k * std::f32::consts::TAU * 3.0).sin() * 0.008;
        (mouth + Vec3::Y * glug, 1.9 + 0.4 * k, 0.35, 0.0)
    } else {
        let toss = smoothstep((t - 0.72) / 0.14);
        (mouth.lerp(away, toss), 2.3.lerp(0.6, toss), 0.35, -2.4 * toss)
    };
    let rotation = Quat::from_rotation_z(lean + spin) * Quat::from_rotation_x(tip);
    (lowered, Some(Transform::from_translation(at).with_rotation(rotation)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAB: Vec3 = Vec3::new(0.0, -0.05, 0.1);

    #[test]
    fn a_reload_starts_and_ends_at_rest() {
        for style in [ReloadStyle::Magazine, ReloadStyle::Feed] {
            for elapsed in [0.0, 2.0] {
                let pose = reload_pose(style, elapsed, 2.0, GRAB);
                assert!(pose.turn.angle_between(Quat::IDENTITY) < 1e-4, "{style:?} at {elapsed}");
                assert!(pose.hand.length() < 1e-4 && pose.magazine.length() < 1e-4, "{style:?} at {elapsed}");
            }
        }
    }

    #[test]
    fn the_magazine_goes_out_of_sight_with_the_hand_and_comes_back_seated() {
        let swapping = reload_pose(ReloadStyle::Magazine, 0.8, 2.0, GRAB);
        assert!(swapping.magazine.y <= -MAGAZINE_DROP + 1e-4);
        assert!((swapping.hand - (GRAB + swapping.magazine)).length() < 1e-4, "the hand holds it");
        let seated = reload_pose(ReloadStyle::Magazine, 1.6, 2.0, GRAB);
        assert_eq!(seated.magazine, Vec3::ZERO);
    }

    #[test]
    fn feeding_moves_the_hand_in_and_out() {
        let ys: Vec<f32> = (0..40).map(|i| reload_pose(ReloadStyle::Feed, 0.6 + i as f32 * 0.03, 3.0, GRAB).hand.y).collect();
        let (low, high) = ys.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &y| (lo.min(y), hi.max(y)));
        assert!(high - low > 0.08, "{low}..{high}");
    }

    #[test]
    fn the_knife_slashes_right_to_left_then_leaves() {
        let (_, cocked) = knife_pose(0.15);
        let (_, slashed) = knife_pose(0.4);
        assert!(cocked.unwrap().translation.x > 0.0);
        assert!(slashed.unwrap().translation.x < 0.0);
        let (dip, gone) = knife_pose(0.95);
        assert!(gone.is_none());
        assert!(dip < 0.2);
        assert!(knife_pose(0.3).0 > 0.9, "the Weapon is out of the way mid-slash");
    }

    #[test]
    fn a_drink_puts_the_weapon_away_and_brings_it_back() {
        assert_eq!(drink_pose(0.0).0, 0.0);
        assert!(drink_pose(0.5).0 > 0.99);
        assert!(drink_pose(1.0).0 < 1e-4);
        assert!(drink_pose(0.0).1.is_none() && drink_pose(0.95).1.is_none(), "no bottle before or after");
    }

    #[test]
    fn the_bottle_comes_up_to_the_mouth_tipped_back_then_goes() {
        let raised = drink_pose(0.2).1.unwrap();
        let drinking = drink_pose(0.6).1.unwrap();
        assert!(drinking.translation.y > raised.translation.y && drinking.translation.z > raised.translation.z);
        // Its neck (+Y) points back toward the camera (+Z) while it's drunk.
        assert!((drinking.rotation * Vec3::Y).z > 0.5);
        assert!((raised.rotation * Vec3::Y).y > 0.9, "upright before it's drunk");
        let tossed = drink_pose(0.85).1.unwrap();
        assert!(tossed.translation.y < -0.6, "on its way out of sight");
    }

    #[test]
    fn the_drink_pose_never_jumps() {
        let mut last = drink_pose(0.0);
        for i in 1..=1000 {
            let pose = drink_pose(i as f32 / 1000.0);
            assert!((pose.0 - last.0).abs() < 0.03);
            if let (Some(a), Some(b)) = (pose.1, last.1) {
                assert!(a.translation.distance(b.translation) < 0.02, "at {i}");
                assert!(a.rotation.angle_between(b.rotation) < 0.06, "at {i}");
            }
            last = pose;
        }
    }

    #[test]
    fn aiming_starts_and_ends_at_the_hip_and_never_jumps() {
        assert_eq!(aim_shift(0.0, 0.08, 0.0), Vec3::ZERO);
        let mut last = Vec3::ZERO;
        for i in 1..=100 {
            let shift = aim_shift(i as f32 / 100.0, 0.08, 0.0);
            assert!((shift - last).length() < 0.02, "jumped at {i}");
            last = shift;
        }
    }

    #[test]
    fn aimed_the_sights_are_centred_and_close() {
        let sights = 0.08;
        // The view model's origin, aimed: the hip plus the shift.
        let at = Vec3::new(HIP_SIDE, -HIP_DROP, 0.0) + aim_shift(1.0, sights, 0.0);
        assert!(at.x.abs() < 1e-5, "under the crosshair");
        assert!((at.y + sights).abs() < 1e-5, "so the sights are at the centre: {at}");
        assert!(aim_shift(1.0, sights, 0.0).z > 0.0, "closer");
    }

    #[test]
    fn aiming_gives_way_to_another_pose() {
        assert_eq!(aim_shift(1.0, 0.08, 1.0), Vec3::ZERO);
        assert!(aim_shift(1.0, 0.08, 0.5).length() < aim_shift(1.0, 0.08, 0.0).length());
    }

    #[test]
    fn kick_shrinks_when_aimed() {
        assert_eq!(aimed_kick(1.0, 0.0), 1.0);
        assert!((aimed_kick(1.0, 1.0) - AIMED_KICK).abs() < 1e-5);
        assert_eq!(aimed_kick(0.0, 1.0), 0.0);
    }

    #[test]
    fn the_carry_starts_at_rest_and_never_jumps() {
        let (shift, turn) = carry_pose(0.0, 0.0);
        assert_eq!(shift, Vec3::ZERO);
        assert!(turn.angle_between(Quat::IDENTITY) < 1e-6);
        let mut last = Quat::IDENTITY;
        for i in 1..=100 {
            let (_, turn) = carry_pose(i as f32 / 100.0, 0.0);
            assert!(turn.angle_between(last) < 0.05, "jumped at {i}");
            last = turn;
        }
    }

    #[test]
    fn carried_the_muzzle_drops_and_rolls_left() {
        let (shift, turn) = carry_pose(1.0, 0.0);
        assert!(shift.y < 0.0, "sunk");
        assert!((turn * Vec3::NEG_Z).y < -0.2, "muzzle down");
        assert!((turn * Vec3::Y).x < -0.2, "top rolled to the left");
    }

    #[test]
    fn the_carry_gives_way_to_another_pose() {
        let (shift, turn) = carry_pose(1.0, 1.0);
        assert_eq!(shift, Vec3::ZERO);
        assert!(turn.angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn the_stab_lunges_and_comes_back() {
        assert_eq!(stab_lunge(0.0), 0.0);
        assert!(stab_lunge(0.2) > 0.2);
        assert!(stab_lunge(1.0) < 1e-4);
    }
}
