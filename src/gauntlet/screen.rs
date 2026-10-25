//! Gauntlet's screens: the Boon Offer's cards over the frozen world, and the Held column on the
//! HUD's left edge.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Button;

use super::Offer;
use super::boon::{HeldBoons, effect_line, stack_line};
use crate::hud::{full_screen, text};
use crate::state::{InRun, Pause};

const GREY: Color = Color::srgb(0.65, 0.65, 0.65);
const NEW_GREEN: Color = Color::srgb(0.45, 0.9, 0.45);
const CARD_IDLE: Color = Color::srgba(0.07, 0.07, 0.09, 0.95);
const CARD_HOVER: Color = Color::srgba(0.22, 0.22, 0.26, 0.97);
/// The Held column wraps into a second column this far above the bottom of the screen, where the
/// Perk icons sit over the Round counter.
const HELD_COLUMN_BOTTOM_PX: f32 = 200.0;

/// One of the Offer's cards: its place in the Offer.
#[derive(Component)]
pub struct OfferCard(pub usize);

/// The Held column on the HUD's left edge.
#[derive(Component)]
pub struct HeldColumn;

/// Shows the Offer's cards whenever the world freezes for it, including on coming back from Pause.
/// The input lockout starts again each time they appear.
pub fn spawn_offer(
    mut commands: Commands,
    offer: Option<ResMut<Offer>>,
    held: Res<HeldBoons>,
    real: Res<Time<Real>>,
) {
    let Some(mut offer) = offer else {
        return;
    };
    offer.shown_at = real.elapsed_secs();
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(24.0),
                ..full_screen()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
            GlobalZIndex(20),
            DespawnOnExit(Pause::Frozen),
        ))
        .with_children(|root| {
            root.spawn(text(format!("ROUND {} COMPLETE", offer.round), 22.0, GREY));
            root.spawn(text("CHOOSE A BOON", 52.0, Color::WHITE));
            root.spawn(Node {
                column_gap: px(28.0),
                ..default()
            })
            .with_children(|row| {
                for (index, boon) in offer.choices.iter().enumerate() {
                    let stacks = held.stacks(boon.id);
                    let accent = boon.rarity.colour();
                    row.spawn((
                        OfferCard(index),
                        Button,
                        Hovered::default(),
                        Node {
                            width: px(280.0),
                            height: px(340.0),
                            flex_direction: FlexDirection::Column,
                            border: UiRect::all(px(4.0)),
                            ..default()
                        },
                        BorderColor::all(accent),
                        BackgroundColor(CARD_IDLE),
                    ))
                    .with_children(|card| {
                        card.spawn((
                            Node {
                                padding: UiRect::axes(px(12.0), px(6.0)),
                                ..default()
                            },
                            BackgroundColor(accent),
                        ))
                        .with_child(text(
                            boon.rarity.label(),
                            18.0,
                            Color::BLACK,
                        ));
                        card.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(px(16.0)),
                            row_gap: px(12.0),
                            ..default()
                        })
                        .with_children(|body| {
                            let stack_colour = if stacks == 0 { NEW_GREEN } else { Color::WHITE };
                            body.spawn(text(boon.name, 30.0, accent));
                            body.spawn(text(stack_line(stacks), 16.0, stack_colour));
                            body.spawn(text("EFFECT", 14.0, GREY));
                            body.spawn(text(effect_line(boon, stacks), 22.0, Color::WHITE));
                            if let Some(requirement) = boon.requirement {
                                body.spawn(text(
                                    format!("Requires {}", requirement.label()),
                                    14.0,
                                    GREY,
                                ));
                            }
                        });
                    });
                }
            });
            root.spawn(text("Click a Boon to take it", 18.0, GREY));
        });
}

pub fn highlight_cards(mut cards: Query<(&Hovered, &mut BackgroundColor), With<OfferCard>>) {
    for (hovered, mut background) in &mut cards {
        let colour = if hovered.get() { CARD_HOVER } else { CARD_IDLE };
        background.set_if_neq(BackgroundColor(colour));
    }
}

pub fn spawn_held_column(mut commands: Commands) {
    commands.spawn((
        InRun,
        HeldColumn,
        Node {
            position_type: PositionType::Absolute,
            left: px(16.0),
            top: px(16.0),
            bottom: px(HELD_COLUMN_BOTTOM_PX),
            flex_direction: FlexDirection::Column,
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::FlexStart,
            row_gap: px(2.0),
            column_gap: px(24.0),
            ..default()
        },
    ));
}

/// "Name Stacks" for each held Boon, in its Rarity colour and in pick order.
pub fn refresh_held_column(
    mut commands: Commands,
    held: Res<HeldBoons>,
    column: Query<Entity, With<HeldColumn>>,
) {
    for column in &column {
        commands
            .entity(column)
            .despawn_children()
            .with_children(|column| {
                for &(boon, stacks) in &held.0 {
                    column.spawn(text(
                        format!("{} {stacks}", boon.name),
                        18.0,
                        boon.rarity.colour(),
                    ));
                }
            });
    }
}
