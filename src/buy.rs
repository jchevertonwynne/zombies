//! The Buyable seam: one shared purchase flow for everything bought or used with the Use key.
//!
//! Each Buyable's owner rewrites its `prompt` and `price` every frame. This module picks the one
//! Buyable the Player is aiming at (or, failing that, the nearest one used from nearby, like a
//! Barrier), checks the Points, spends them, and tells the owner with
//! `Purchased`. The owner does the rest.

use bevy::prelude::*;

use crate::controls::{Action, Controls};
use crate::last_stand::LastStand;
use crate::modifier::{Modifier, Modifiers, Stat, resolve_points};
use crate::player::{Player, PlayerCamera};
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

/// The Player bought (or used) this Buyable. Its owner acts on it.
#[derive(Message)]
pub struct Purchased(pub Entity);

/// The Player tried to buy something they can't afford.
#[derive(Message)]
pub struct CantAfford;

/// Its owner refuses this Buyable for now, like an ammo buy with full ammo or a Perk machine
/// before Power. The Prompt still shows, but the Use key charges nothing and plays the buzzer.
#[derive(Component)]
pub struct Refused;

/// A Buyable used by holding the Use key, like a Barrier's Planks. Pressing Use on it isn't a
/// purchase, so the purchase flow leaves it alone; its owner reads `Focus` and the held key.
#[derive(Component)]
pub struct Held;

/// A Buyable used from anywhere near it, wherever the Player looks, like a Barrier's Planks. It
/// gets the Prompt only when nothing the Player is aiming at is in reach, so it never steals a buy
/// next to it.
#[derive(Component)]
pub struct Nearby;

/// The hold half of a Buyable, like a Repack's Attachment: holding the Use key for
/// `HOLD_BUY_SECS` buys this instead of the Buyable, and its owner hears `HoldPurchased`. While a
/// Buyable has one, a tap buys on release rather than on press, so a hold never also taps. Its
/// owner rewrites it every frame, like the Buyable, and `Refused` refuses both halves.
#[derive(Component)]
pub struct HoldBuy {
    pub prompt: String,
    pub price: Option<u64>,
    /// Whether a tap still buys the Buyable itself. Without it only the hold is offered.
    pub tap: bool,
}

/// The Player held the Use key on this Buyable's hold half and bought it.
#[derive(Message)]
pub struct HoldPurchased(pub Entity);

/// What the Use key did on a Buyable with a hold half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press<T> {
    /// Nothing yet: not pressed, still being held, or already spent.
    Nothing,
    /// Let go before `HOLD_BUY_SECS`.
    Tap(T),
    /// Held for `HOLD_BUY_SECS`.
    Hold(T),
}

/// The Use key held on a Buyable with a hold half.
#[derive(Resource, Debug)]
pub struct HoldTimer<T> {
    /// The Buyable it's held on, and for how long.
    holding: Option<(T, f32)>,
    /// Whether this hold has already bought, so it waits for the key to come up.
    spent: bool,
}

impl<T> Default for HoldTimer<T> {
    fn default() -> Self {
        Self {
            holding: None,
            spent: false,
        }
    }
}

impl<T: PartialEq + Copy> HoldTimer<T> {
    /// Advances by `dt`, with the Use key `pressed` while the Prompt belongs to `target` (`None`
    /// for no Buyable with a hold half). Letting go early is a tap; holding for `HOLD_BUY_SECS` is
    /// a hold, once. Looking away throws the press away.
    pub fn advance(&mut self, target: Option<T>, pressed: bool, dt: f32) -> Press<T> {
        let Some(target) = target else {
            *self = Self::default();
            return Press::Nothing;
        };
        if !pressed {
            let tapped = matches!(self.holding, Some((held, _)) if held == target) && !self.spent;
            *self = Self::default();
            return if tapped { Press::Tap(target) } else { Press::Nothing };
        }
        let secs = match self.holding {
            Some((held, secs)) if held == target => secs + dt,
            _ => {
                self.spent = false;
                0.0
            }
        };
        self.holding = Some((target, secs));
        if secs >= HOLD_BUY_SECS && !self.spent {
            self.spent = true;
            Press::Hold(target)
        } else {
            Press::Nothing
        }
    }
}

/// The Player tried to buy something its owner refuses.
#[derive(Message)]
pub struct BuyRefused;

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
    /// Nothing is charged; the buzzer plays.
    Refused,
}

impl Decision {
    /// A buy the owner refuses becomes a refusal. Not affording it still flashes the Points.
    pub fn refused_if(self, refused: bool) -> Self {
        if refused && self == Decision::Buy { Decision::Refused } else { self }
    }
}

/// Whether the Player can pay `price`. Priceless uses are always affordable.
pub fn can_afford(points: u64, price: Option<u64>) -> bool {
    price.is_none_or(|price| points >= price)
}

/// What the Player is really charged for an owner's `price`: every price goes through the
/// Player's `Stat::Price` entries, so a rule can make everything dearer without touching owners.
pub fn charged(price: Option<u64>, player: &[Modifier]) -> Option<u64> {
    price.map(|price| resolve_points(Stat::Price, price, player))
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

/// The nearest of the Nearby Buyables within their range of the eye, whichever way it looks.
pub fn pick_nearby<T>(eye: Vec3, candidates: impl IntoIterator<Item = (T, Vec3, f32)>) -> Option<T> {
    candidates
        .into_iter()
        .map(|(item, use_point, range)| (item, eye.distance(use_point), range))
        .filter(|&(_, distance, range)| distance <= range)
        .min_by(|(_, a, _), (_, b, _)| a.total_cmp(b))
        .map(|(item, _, _)| item)
}

pub struct BuyPlugin;

impl Plugin for BuyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Focus>()
            .init_resource::<HoldTimer<Entity>>()
            .add_message::<Purchased>()
            .add_message::<HoldPurchased>()
            .add_message::<CantAfford>()
            .add_message::<BuyRefused>()
            .add_systems(OnEnter(GameState::GameOver), |mut focus: ResMut<Focus>| focus.0 = None)
            .add_systems(
                Update,
                (focus, (buy, hold_buy).run_if(in_state(Pause::Running)))
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn buyer(points: &Points, down: bool, pause: &State<Pause>) -> Buyer {
    Buyer {
        points: points.balance,
        down,
        // Frozen counts: nothing is bought while the world stands still.
        paused: *pause.get() != Pause::Running,
    }
}

fn focus(
    camera: Single<&GlobalTransform, With<PlayerCamera>>,
    buyables: Query<(Entity, &Buyable, Has<Nearby>)>,
    down: Single<Has<LastStand>, With<Player>>,
    points: Res<Points>,
    pause: Res<State<Pause>>,
    mut focus: ResMut<Focus>,
) {
    let picked = if buyer(&points, *down, &pause).blocked() {
        None
    } else {
        let eye = camera.translation();
        let aimed = buyables.iter().filter(|(_, _, nearby)| !nearby).map(|(entity, b, _)| (entity, b.use_point, b.range));
        let nearby = buyables.iter().filter(|(_, _, nearby)| *nearby).map(|(entity, b, _)| (entity, b.use_point, b.range));
        pick(eye, camera.forward().as_vec3(), aimed).or_else(|| pick_nearby(eye, nearby))
    };
    if focus.0 != picked {
        focus.0 = picked;
    }
}

pub fn buy(
    controls: Controls,
    focus: Res<Focus>,
    // A Buyable with a hold half buys through `hold_buy`.
    buyables: Query<(&Buyable, Has<Refused>), (Without<Held>, Without<HoldBuy>)>,
    player: Single<(Has<LastStand>, &Modifiers), With<Player>>,
    pause: Res<State<Pause>>,
    mut points: ResMut<Points>,
    mut purchased: MessageWriter<Purchased>,
    mut cant_afford: MessageWriter<CantAfford>,
    mut refusals: MessageWriter<BuyRefused>,
) {
    if !controls.just_pressed(Action::Use) {
        return;
    }
    let Some(entity) = focus.0 else { return };
    let Ok((buyable, refused)) = buyables.get(entity) else { return };
    let (down, modifiers) = *player;
    let price = charged(buyable.price, &modifiers.0);
    // Focus already holds only Buyables in reach.
    match decide(buyer(&points, down, &pause), price, true).refused_if(refused) {
        Decision::Buy => {
            if let Some(price) = price {
                points.spend(price);
            }
            purchased.write(Purchased(entity));
        }
        Decision::CantAfford => {
            cant_afford.write(CantAfford);
        }
        Decision::Refused => {
            refusals.write(BuyRefused);
        }
        Decision::NoPrompt => {}
    }
}

/// The Use key on a Buyable with a hold half: a tap buys the Buyable on release, a hold buys the
/// hold half. Both go through the same affordability check, spending and refusals as `buy`.
fn hold_buy(
    controls: Controls,
    focus: Res<Focus>,
    buyables: Query<(&Buyable, &HoldBuy, Has<Refused>)>,
    player: Single<(Has<LastStand>, &Modifiers), With<Player>>,
    pause: Res<State<Pause>>,
    time: Res<Time>,
    mut timer: ResMut<HoldTimer<Entity>>,
    mut points: ResMut<Points>,
    mut purchased: MessageWriter<Purchased>,
    mut held: MessageWriter<HoldPurchased>,
    mut cant_afford: MessageWriter<CantAfford>,
    mut refusals: MessageWriter<BuyRefused>,
) {
    let target = focus.0.filter(|entity| buyables.contains(*entity));
    let (entity, hold) = match timer.advance(target, controls.pressed(Action::Use), time.delta_secs()) {
        Press::Nothing => return,
        Press::Tap(entity) => (entity, false),
        Press::Hold(entity) => (entity, true),
    };
    let Ok((buyable, hold_buy, refused)) = buyables.get(entity) else { return };
    if !hold && !hold_buy.tap {
        return;
    }
    let (down, modifiers) = *player;
    let price = charged(if hold { hold_buy.price } else { buyable.price }, &modifiers.0);
    match decide(buyer(&points, down, &pause), price, true).refused_if(refused) {
        Decision::Buy => {
            if let Some(price) = price {
                points.spend(price);
            }
            if hold {
                held.write(HoldPurchased(entity));
            } else {
                purchased.write(Purchased(entity));
            }
        }
        Decision::CantAfford => {
            cant_afford.write(CantAfford);
        }
        Decision::Refused => {
            refusals.write(BuyRefused);
        }
        Decision::NoPrompt => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letting_go_early_is_a_tap() {
        let mut timer = HoldTimer::default();
        assert_eq!(timer.advance(Some(1), true, 0.0), Press::Nothing);
        assert_eq!(timer.advance(Some(1), true, HOLD_BUY_SECS / 2.0), Press::Nothing);
        assert_eq!(timer.advance(Some(1), false, 0.016), Press::Tap(1));
        assert_eq!(timer.advance(Some(1), false, 0.016), Press::Nothing, "once");
    }

    #[test]
    fn holding_buys_the_hold_half_once_and_never_taps() {
        let mut timer = HoldTimer::default();
        timer.advance(Some(1), true, 0.0);
        assert_eq!(timer.advance(Some(1), true, HOLD_BUY_SECS), Press::Hold(1));
        assert_eq!(timer.advance(Some(1), true, 5.0), Press::Nothing, "still held, already bought");
        assert_eq!(timer.advance(Some(1), false, 0.016), Press::Nothing, "letting go isn't a tap");
        // A fresh press starts over.
        timer.advance(Some(1), true, 0.0);
        assert_eq!(timer.advance(Some(1), true, HOLD_BUY_SECS), Press::Hold(1));
    }

    #[test]
    fn looking_away_throws_the_press_away() {
        let mut timer = HoldTimer::default();
        timer.advance(Some(1), true, HOLD_BUY_SECS * 0.9);
        assert_eq!(timer.advance(None, true, 0.1), Press::Nothing);
        assert_eq!(timer.advance(Some(1), true, 0.1), Press::Nothing, "the hold starts over");
        assert_eq!(timer.advance(Some(2), false, 0.0), Press::Nothing, "a tap belongs to one Buyable");
    }

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
    fn a_refused_buy_charges_nothing_and_buzzes() {
        assert_eq!(decide(buyer(5000), Some(250), true).refused_if(true), Decision::Refused);
        assert_eq!(decide(buyer(5000), Some(250), true).refused_if(false), Decision::Buy);
    }

    #[test]
    fn a_refusal_never_shows_a_prompt_that_would_be_hidden_anyway() {
        assert_eq!(decide(buyer(5000), Some(250), false).refused_if(true), Decision::NoPrompt);
        let down = Buyer { down: true, ..buyer(5000) };
        assert_eq!(decide(down, Some(250), true).refused_if(true), Decision::NoPrompt);
        // Too few Points still flashes them red.
        assert_eq!(decide(buyer(0), Some(250), true).refused_if(true), Decision::CantAfford);
    }

    #[test]
    fn price_entries_make_every_price_dearer_but_leave_free_uses_free() {
        use crate::modifier::{Effect, Source};
        let dearer = [Modifier { stat: Stat::Price, source: Source("A"), effect: Effect::Mult(1.25) }];
        assert_eq!(charged(Some(1000), &[]), Some(1000));
        assert_eq!(charged(Some(1000), &dearer), Some(1250));
        assert_eq!(charged(Some(950), &dearer), Some(1188));
        assert_eq!(charged(Some(0), &dearer), Some(0));
        assert_eq!(charged(None, &dearer), None);
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
    fn a_nearby_buyable_is_reached_whichever_way_the_player_looks() {
        let behind = EYE + Vec3::new(0.0, -0.4, 1.5);
        assert_eq!(pick_nearby(EYE, [("behind", behind, BARRIER_REBUILD_RANGE)]), Some("behind"));
        assert!(reach(EYE, AHEAD, behind, BARRIER_REBUILD_RANGE).is_none(), "an aimed Buyable there would not be");
    }

    #[test]
    fn the_nearest_nearby_buyable_wins_and_far_ones_never_do() {
        let candidates = [
            ("far", EYE + Vec3::new(0.0, 0.0, -1.8), BARRIER_REBUILD_RANGE),
            ("near", EYE + Vec3::new(1.0, 0.0, 0.5), BARRIER_REBUILD_RANGE),
            ("out of range", EYE + Vec3::new(0.0, 0.0, 3.0), BARRIER_REBUILD_RANGE),
        ];
        assert_eq!(pick_nearby(EYE, candidates), Some("near"));
        assert_eq!(pick_nearby(EYE, [((), EYE + Vec3::X * 3.0, BARRIER_REBUILD_RANGE)]), None);
    }

    #[test]
    fn no_buyables_means_no_prompt() {
        assert_eq!(pick::<()>(EYE, AHEAD, []), None);
    }
}
