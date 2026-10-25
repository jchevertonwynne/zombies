//! The Buyable seam: one shared purchase flow for everything bought or used with the Use key.
//!
//! Each Buyable's owner rewrites its `prompt` and `price` every frame. This module picks the one
//! Buyable the Player is aiming at, checks the Points, spends them, and tells the owner with
//! `Purchased`. The owner does the rest.

use bevy::prelude::*;

use crate::player::PlayerCamera;
use crate::points::Points;
use crate::state::{GameState, Pause};
use crate::tuning::*;

/// Something in the Map the Use key buys or uses.
#[derive(Component)]
pub struct Buyable {
    /// Where the Player must be near and aiming at, in world space.
    pub use_point: Vec3,
    /// What the Use key does here, in BO1 wording, without the price.
    pub prompt: String,
    /// `None` for something used but not bought, like the Power Switch.
    pub price: Option<u64>,
    /// How far from the use point it can be bought. Usually `USE_RANGE`.
    pub range: f32,
}

/// A Buyable showing a refusal, such as "You must turn on the Power first": its Prompt shows, but
/// the Use key does nothing there.
#[derive(Component)]
pub struct Refused;

/// The Player bought (or used) this Buyable. Its owner acts on it.
#[derive(Message)]
pub struct Purchased(pub Entity);

/// The Player tried to buy something they can't afford.
#[derive(Message)]
pub struct CantAfford;

/// The Buyable whose Prompt is showing, if any.
#[derive(Resource, Default)]
pub struct Focus(pub Option<Entity>);

/// What the Player brings to a purchase.
#[derive(Debug, Clone, Copy)]
pub struct Buyer {
    pub points: u64,
    pub down: bool,
    pub paused: bool,
}

impl Buyer {
    /// Nothing can be bought while Down, paused or on Game Over (when no Buyer exists at all).
    pub fn blocked(&self) -> bool {
        self.down || self.paused
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Nothing happens and no Prompt shows.
    NoPrompt,
    /// Spend the price (if any) and send `Purchased`.
    Buy,
    /// Nothing is charged; the buzzer plays and the Points flash red.
    CantAfford,
}

/// Whether the Player can pay `price`. Priceless uses are always affordable.
pub fn can_afford(points: u64, price: Option<u64>) -> bool {
    price.is_none_or(|price| points >= price)
}

/// What pressing Use on a Buyable does.
pub fn decide(buyer: Buyer, price: Option<u64>, in_reach: bool) -> Decision {
    if buyer.blocked() || !in_reach {
        Decision::NoPrompt
    } else if can_afford(buyer.points, price) {
        Decision::Buy
    } else {
        Decision::CantAfford
    }
}

/// How far off the crosshair, and how far away, a use point is, if the Player can reach it:
/// within `range` and `USE_ANGLE`, height included.
pub fn reach(eye: Vec3, forward: Vec3, use_point: Vec3, range: f32) -> Option<(f32, f32)> {
    let to_point = use_point - eye;
    let distance = to_point.length();
    if distance > range {
        return None;
    }
    // Standing right on the use point counts as aiming straight at it.
    let angle = if distance < 1e-4 { 0.0 } else { forward.angle_between(to_point) };
    (angle <= USE_ANGLE).then_some((angle, distance))
}

/// The Buyable whose Prompt shows: of those in reach, the one closest to the crosshair by angle,
/// with distance breaking ties.
pub fn pick<T>(eye: Vec3, forward: Vec3, candidates: impl IntoIterator<Item = (T, Vec3, f32)>) -> Option<T> {
    candidates
        .into_iter()
        .filter_map(|(item, use_point, range)| reach(eye, forward, use_point, range).map(|r| (item, r)))
        .min_by(|(_, (a_angle, a_dist)), (_, (b_angle, b_dist))| {
            a_angle.total_cmp(b_angle).then(a_dist.total_cmp(b_dist))
        })
        .map(|(item, _)| item)
}

pub struct BuyPlugin;

impl Plugin for BuyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Focus>()
            .add_message::<Purchased>()
            .add_message::<CantAfford>()
            .add_systems(OnEnter(GameState::GameOver), |mut focus: ResMut<Focus>| focus.0 = None)
            .add_systems(
                Update,
                (focus, buy.run_if(in_state(Pause::Running)))
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn buyer(points: &Points, pause: &State<Pause>) -> Buyer {
    Buyer {
        points: points.balance,
        // The Last Stand arrives with Quick Revive; until then a Down is Game Over.
        down: false,
        paused: *pause.get() == Pause::Paused,
    }
}

fn focus(
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    buyables: Query<(Entity, &Buyable)>,
    points: Res<Points>,
    pause: Res<State<Pause>>,
    mut focus: ResMut<Focus>,
) {
    let picked = if buyer(&points, &pause).blocked() {
        None
    } else {
        let candidates = buyables.iter().map(|(entity, b)| (entity, b.use_point, b.range));
        pick(camera.translation(), camera.forward().as_vec3(), candidates)
    };
    if focus.0 != picked {
        focus.0 = picked;
    }
}

fn buy(
    input: Res<ButtonInput<KeyCode>>,
    focus: Res<Focus>,
    buyables: Query<&Buyable, Without<Refused>>,
    pause: Res<State<Pause>>,
    mut points: ResMut<Points>,
    mut purchased: MessageWriter<Purchased>,
    mut cant_afford: MessageWriter<CantAfford>,
) {
    if !input.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(entity) = focus.0 else { return };
    let Ok(buyable) = buyables.get(entity) else { return };
    // Focus already holds only Buyables in reach.
    match decide(buyer(&points, &pause), buyable.price, true) {
        Decision::Buy => {
            if let Some(price) = buyable.price {
                points.spend(price);
            }
            purchased.write(Purchased(entity));
        }
        Decision::CantAfford => {
            cant_afford.write(CantAfford);
        }
        Decision::NoPrompt => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buyer(points: u64) -> Buyer {
        Buyer {
            points,
            down: false,
            paused: false,
        }
    }

    #[test]
    fn enough_points_buys() {
        assert_eq!(decide(buyer(750), Some(750), true), Decision::Buy);
        assert_eq!(decide(buyer(5000), Some(750), true), Decision::Buy);
    }

    #[test]
    fn too_few_points_refuses_with_the_buzzer() {
        assert_eq!(decide(buyer(749), Some(750), true), Decision::CantAfford);
        assert_eq!(decide(buyer(0), Some(1), true), Decision::CantAfford);
    }

    #[test]
    fn priceless_uses_need_no_points() {
        assert_eq!(decide(buyer(0), None, true), Decision::Buy);
    }

    #[test]
    fn nothing_happens_out_of_reach() {
        assert_eq!(decide(buyer(5000), Some(750), false), Decision::NoPrompt);
        assert_eq!(decide(buyer(0), Some(750), false), Decision::NoPrompt);
    }

    #[test]
    fn nothing_can_be_bought_while_down_or_paused() {
        let down = Buyer { down: true, ..buyer(5000) };
        let paused = Buyer { paused: true, ..buyer(5000) };
        for blocked in [down, paused] {
            assert_eq!(decide(blocked, Some(750), true), Decision::NoPrompt);
            assert_eq!(decide(blocked, None, true), Decision::NoPrompt);
        }
        // Not even a buzzer when broke.
        assert_eq!(decide(Buyer { down: true, ..buyer(0) }, Some(750), true), Decision::NoPrompt);
    }

    #[test]
    fn affordability_colours_the_prompt() {
        assert!(can_afford(1000, Some(1000)));
        assert!(!can_afford(999, Some(1000)));
        assert!(can_afford(0, None));
    }

    const EYE: Vec3 = Vec3::new(0.0, 1.6, 0.0);
    const AHEAD: Vec3 = Vec3::NEG_Z;

    #[test]
    fn a_use_point_in_front_and_in_range_can_be_reached() {
        assert!(reach(EYE, AHEAD, Vec3::new(0.0, 1.6, -1.9), USE_RANGE).is_some());
    }

    #[test]
    fn a_use_point_beyond_range_cannot_be_reached() {
        assert!(reach(EYE, AHEAD, Vec3::new(0.0, 1.6, -2.1), USE_RANGE).is_none());
    }

    #[test]
    fn height_counts_toward_range() {
        // 1.5 m ahead and 1.5 m up is 2.12 m away.
        assert!(reach(EYE, AHEAD, Vec3::new(0.0, 3.1, -1.5), USE_RANGE).is_none());
    }

    #[test]
    fn a_range_override_reaches_further() {
        let far = Vec3::new(0.0, 1.6, -3.0);
        assert!(reach(EYE, AHEAD, far, USE_RANGE).is_none());
        assert!(reach(EYE, AHEAD, far, DOOR_USE_RANGE).is_some());
    }

    #[test]
    fn a_use_point_more_than_45_degrees_off_the_crosshair_cannot_be_reached() {
        assert!(reach(EYE, AHEAD, EYE + Vec3::new(1.0, 0.0, -1.1), USE_RANGE).is_some());
        assert!(reach(EYE, AHEAD, EYE + Vec3::new(1.1, 0.0, -1.0), USE_RANGE).is_none());
        // Behind the Player.
        assert!(reach(EYE, AHEAD, EYE + Vec3::new(0.0, 0.0, 1.0), USE_RANGE).is_none());
    }

    #[test]
    fn looking_up_or_down_counts_toward_the_angle() {
        assert!(reach(EYE, AHEAD, EYE + Vec3::new(0.0, -1.1, -1.0), USE_RANGE).is_none());
    }

    #[test]
    fn the_buyable_closest_to_the_crosshair_wins() {
        let candidates = [
            ("near but off to the side", EYE + Vec3::new(0.6, 0.0, -0.8), USE_RANGE),
            ("further but dead ahead", EYE + Vec3::new(0.0, 0.0, -1.8), USE_RANGE),
        ];
        assert_eq!(pick(EYE, AHEAD, candidates), Some("further but dead ahead"));
    }

    #[test]
    fn distance_breaks_ties_in_angle() {
        let candidates = [
            ("far", EYE + Vec3::new(0.0, 0.0, -1.8), USE_RANGE),
            ("near", EYE + Vec3::new(0.0, 0.0, -0.9), USE_RANGE),
        ];
        assert_eq!(pick(EYE, AHEAD, candidates), Some("near"));
    }

    #[test]
    fn buyables_out_of_reach_never_win() {
        let candidates = [
            ("too far", EYE + Vec3::new(0.0, 0.0, -2.5), USE_RANGE),
            ("too wide", EYE + Vec3::new(1.0, 0.0, -0.2), USE_RANGE),
        ];
        assert_eq!(pick(EYE, AHEAD, candidates), None);
    }

    #[test]
    fn no_buyables_means_no_prompt() {
        assert_eq!(pick::<()>(EYE, AHEAD, []), None);
    }
}
