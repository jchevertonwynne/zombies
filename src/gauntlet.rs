//! Gauntlet: Classic with a Boon Offer after every Round. It's layered on top of Classic and
//! reaches it only through seams (ADR 0002): stat-modifier entries on the Player, messages Classic
//! already sends, Classic state it reads, and the `Freeze` and `IntermissionHold` it holds. Every
//! system here runs only when `Mode == Gauntlet`.

mod boon;
mod screen;

use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::last_stand::LastStand;
use crate::mode::Mode;
use crate::modifier::Modifiers;
use crate::mystery_box::TeddyBear;
use crate::player::Player;
use crate::points::BonusPoints;
use crate::power::Power;
use crate::records::{RecordExtra, RecordExtras};
use crate::round::{IntermissionHold, Round, RoundEnded};
use crate::state::{Freeze, GameState, Pause};
use crate::tuning::*;
use crate::zombie::ZombieKilled;
use boon::{
    BLOOD_MONEY, BOONS, BoonDef, HeldBoons, RunFacts, TEDDYS_TITHE, blood_money_points, draw_offer,
    is_boon_source, teddys_tithe_points,
};
use screen::OfferCard;

/// Seconds left before the Boon Offer opens, counted from the Round's last kill.
#[derive(Resource, Default)]
struct PendingOffer(Option<f32>);

/// The Boon Offer on screen. Quitting the Run loses it.
#[derive(Resource)]
struct Offer {
    choices: Vec<&'static BoonDef>,
    /// The Round just completed.
    round: u32,
    /// When the cards last appeared, in real seconds, for the input lockout.
    shown_at: f32,
}

pub struct GauntletPlugin;

impl Plugin for GauntletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldBoons>()
            .init_resource::<PendingOffer>()
            .add_systems(
                OnEnter(GameState::Playing),
                (reset, screen::spawn_held_column).chain().run_if(gauntlet),
            )
            .add_systems(
                Update,
                (
                    schedule_offer,
                    open_offer.run_if(in_state(Pause::Running)),
                    apply_boons,
                    blood_money,
                    teddys_tithe,
                    (record_boons, screen::refresh_held_column)
                        .run_if(resource_changed::<HeldBoons>),
                )
                    .run_if(in_state(GameState::Playing))
                    .run_if(gauntlet),
            )
            .add_systems(OnEnter(Pause::Frozen), screen::spawn_offer.run_if(gauntlet))
            .add_systems(
                Update,
                screen::highlight_cards
                    .run_if(in_state(Pause::Frozen))
                    .run_if(gauntlet),
            )
            .add_observer(pick);
    }
}

fn gauntlet(mode: Res<Mode>) -> bool {
    *mode == Mode::Gauntlet
}

fn reset(mut commands: Commands) {
    commands.insert_resource(HeldBoons::default());
    commands.insert_resource(PendingOffer::default());
    commands.remove_resource::<Offer>();
}

/// The Round ended: the Offer is due after its delay, and the Intermission waits for it.
fn schedule_offer(
    mut ended: MessageReader<RoundEnded>,
    mut pending: ResMut<PendingOffer>,
    mut hold: ResMut<IntermissionHold>,
) {
    if ended.read().count() > 0 {
        pending.0 = Some(BOON_OFFER_DELAY_SECS);
        hold.0 = true;
    }
}

/// Opens the Offer once its delay is up, freezing the world. A Round that ended in Last Stand
/// waits for the Self-Revive. With no eligible Boon there's no Offer, and the Intermission starts.
fn open_offer(
    mut commands: Commands,
    mut pending: ResMut<PendingOffer>,
    player: Query<Has<LastStand>, With<Player>>,
    held: Res<HeldBoons>,
    power: Option<Res<Power>>,
    round: Res<Round>,
    time: Res<Time>,
    mut hold: ResMut<IntermissionHold>,
    mut freeze: ResMut<Freeze>,
) {
    let Some(left) = pending.0.as_mut() else {
        return;
    };
    *left -= time.delta_secs();
    if *left > 0.0 || player.single().unwrap_or(false) {
        return;
    }
    pending.0 = None;
    let run = RunFacts {
        power_on: power.is_some_and(|power| power.is_on()),
        // No Pacts can be held until they arrive.
        held_pacts: Vec::new(),
    };
    let choices = draw_offer(BOONS, &held, &run, |total| rand::random_range(0.0..total));
    if choices.is_empty() {
        hold.0 = false;
        return;
    }
    commands.insert_resource(Offer {
        choices,
        round: round.number,
        shown_at: 0.0,
    });
    freeze.0 = true;
}

/// A click on a card, once the input lockout is over, takes that Boon. The world runs again and
/// the Intermission starts.
fn pick(
    activate: On<Activate>,
    mut commands: Commands,
    cards: Query<&OfferCard>,
    offer: Option<Res<Offer>>,
    real: Res<Time<Real>>,
    mut held: ResMut<HeldBoons>,
    mut freeze: ResMut<Freeze>,
    mut hold: ResMut<IntermissionHold>,
) {
    let (Ok(OfferCard(index)), Some(offer)) = (cards.get(activate.entity), offer) else {
        return;
    };
    if real.elapsed_secs() - offer.shown_at < BOON_OFFER_INPUT_LOCKOUT_SECS {
        return;
    }
    let Some(&boon) = offer.choices.get(*index) else {
        return;
    };
    held.add(boon);
    commands.remove_resource::<Offer>();
    freeze.0 = false;
    hold.0 = false;
}

/// Keeps the Player's Boon entries in step with the Boons held. A Down takes only Perk entries,
/// so these survive it.
fn apply_boons(held: Res<HeldBoons>, mut player: Query<&mut Modifiers, With<Player>>) {
    let Ok(mut modifiers) = player.single_mut() else {
        return;
    };
    let wanted = held.modifiers();
    let current: Vec<_> = modifiers
        .0
        .iter()
        .filter(|entry| is_boon_source(entry.source))
        .copied()
        .collect();
    if current != wanted {
        modifiers.0.retain(|entry| !is_boon_source(entry.source));
        modifiers.0.extend(wanted);
    }
}

fn blood_money(
    mut kills: MessageReader<ZombieKilled>,
    held: Res<HeldBoons>,
    mut bonus: MessageWriter<BonusPoints>,
) {
    let stacks = held.stacks(BLOOD_MONEY);
    for kill in kills.read() {
        let points = blood_money_points(kill.pay, stacks);
        if points > 0 {
            bonus.write(BonusPoints(points));
        }
    }
}

/// Teddy's Tithe: a Teddy Bear pays Points on top of its refund.
fn teddys_tithe(
    mut teddies: MessageReader<TeddyBear>,
    held: Res<HeldBoons>,
    mut bonus: MessageWriter<BonusPoints>,
) {
    let points = teddys_tithe_points(held.stacks(TEDDYS_TITHE));
    for _ in teddies.read() {
        if points > 0 {
            bonus.write(BonusPoints(points));
        }
    }
}

/// The held Boons go on the Run Record in pick order, with their Stacks.
fn record_boons(held: Res<HeldBoons>, mut extras: ResMut<RecordExtras>) {
    extras.0 = held
        .0
        .iter()
        .map(|&(boon, stacks)| RecordExtra {
            id: boon.id.to_string(),
            count: stacks,
        })
        .collect();
}
