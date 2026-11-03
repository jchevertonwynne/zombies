//! Gauntlet's screens: the Boon or Pact Offer's cards over the frozen world, and the Held column
//! on the HUD's left edge.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::Button;

use super::boon::{BoonDef, HeldBoons, effect_line, requirement_line, stack_line};
use super::pact::{CRIMSON, HeldPacts, PactDef};
use super::{Choices, Offer};
use crate::hud::{BOTTOM_LEFT_HEIGHT_PX, full_screen, text};
use crate::round::Round;
use crate::state::{InRun, Pause};

const GREY: Color = Color::srgb(0.65, 0.65, 0.65);
const NEW_GREEN: Color = Color::srgb(0.45, 0.9, 0.45);
const CARD_IDLE: Color = Color::srgba(0.07, 0.07, 0.09, 0.95);
const CARD_HOVER: Color = Color::srgba(0.22, 0.22, 0.26, 0.97);
/// A held Pact's Drawback line, a dimmer crimson under its name.
const DRAWBACK_CRIMSON: Color = Color::srgb(0.7, 0.35, 0.4);

/// One of the Offer's cards: its place in the Offer.
#[derive(Component)]
pub struct OfferCard(pub usize);

/// A Pact Offer's Skip button.
#[derive(Component)]
pub struct SkipButton;

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
    let (title, footer) = match offer.choices {
        Choices::Boons(_) => ("CHOOSE A BOON", "Click a Boon to take it"),
        Choices::Pacts(_) => ("A PACT IS OFFERED", "Click a Pact to take it, or Skip"),
    };
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
            let title_colour = match offer.choices {
                Choices::Boons(_) => Color::WHITE,
                Choices::Pacts(_) => CRIMSON,
            };
            root.spawn(text(title, 52.0, title_colour));
            root.spawn(Node {
                column_gap: px(28.0),
                ..default()
            })
            .with_children(|row| match &offer.choices {
                Choices::Boons(boons) => {
                    for (index, boon) in boons.iter().enumerate() {
                        boon_card(row, index, boon, held.stacks(boon.id));
                    }
                }
                Choices::Pacts(pacts) => {
                    for (index, pact) in pacts.iter().enumerate() {
                        pact_card(row, index, pact, offer.round + 1);
                    }
                }
            });
            if matches!(offer.choices, Choices::Pacts(_)) {
                root.spawn((
                    SkipButton,
                    Button,
                    Hovered::default(),
                    Node {
                        padding: UiRect::axes(px(36.0), px(10.0)),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                    BorderColor::all(GREY),
                    BackgroundColor(CARD_IDLE),
                ))
                .with_child(text("SKIP", 24.0, Color::WHITE));
            }
            root.spawn(text(footer, 18.0, GREY));
        });
}

/// An empty card with its accent border and a label strip along the top; the body goes in the
/// returned closure's node.
fn card(
    row: &mut ChildSpawnerCommands,
    index: usize,
    accent: Color,
    label: &str,
    body: impl FnOnce(&mut ChildSpawnerCommands),
) {
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
        .with_child(text(label, 18.0, Color::BLACK));
        card.spawn(Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(16.0)),
            row_gap: px(12.0),
            ..default()
        })
        .with_children(body);
    });
}

/// A Boon's card in its Rarity colour: NEW or its Stacks, and its effect now and after.
fn boon_card(row: &mut ChildSpawnerCommands, index: usize, boon: &BoonDef, stacks: u32) {
    let accent = boon.rarity.colour();
    card(row, index, accent, boon.rarity.label(), |body| {
        let stack_colour = if stacks == 0 { NEW_GREEN } else { Color::WHITE };
        body.spawn(text(boon.name, 30.0, accent));
        body.spawn(text(stack_line(stacks), 16.0, stack_colour));
        body.spawn(text("EFFECT", 14.0, GREY));
        body.spawn(text(effect_line(boon, stacks), 22.0, Color::WHITE));
        if let Some(requirement) = requirement_line(boon.requirements) {
            body.spawn(text(requirement, 14.0, GREY));
        }
    });
}

/// A Pact's crimson card: its Upside and its Drawback.
fn pact_card(row: &mut ChildSpawnerCommands, index: usize, pact: &PactDef, round: u32) {
    card(row, index, CRIMSON, "PACT", |body| {
        body.spawn(text(pact.name(), 30.0, CRIMSON));
        body.spawn(text("UPSIDE", 14.0, GREY));
        body.spawn(text((pact.upside)(), 18.0, Color::WHITE));
        body.spawn(text("DRAWBACK", 14.0, GREY));
        body.spawn(text((pact.drawback)(round), 18.0, CRIMSON));
        if let Some(requirement) = requirement_line(pact.requirements) {
            body.spawn(text(requirement, 14.0, GREY));
        }
    });
}

pub fn highlight_cards(
    mut cards: Query<(&Hovered, &mut BackgroundColor), Or<(With<OfferCard>, With<SkipButton>)>>,
) {
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
            // It wraps into a second column above the HUD's bottom-left corner, where the Perk icons
            // sit over the Round counter.
            bottom: px(BOTTOM_LEFT_HEIGHT_PX),
            flex_direction: FlexDirection::Column,
            flex_wrap: FlexWrap::Wrap,
            align_content: AlignContent::FlexStart,
            row_gap: px(2.0),
            column_gap: px(24.0),
            ..default()
        },
    ));
}

/// "Name Stacks" for each held Boon, in its Rarity colour and in pick order; then each held Pact
/// in crimson, with its Drawback beneath.
pub fn refresh_held_column(
    mut commands: Commands,
    held: Res<HeldBoons>,
    pacts: Res<HeldPacts>,
    round: Option<Res<Round>>,
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
                let round = round.as_ref().map_or(1, |round| round.number);
                for pact in &pacts.0 {
                    // Name and Drawback together, so a wrap never parts them.
                    column
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            max_width: px(260.0),
                            ..default()
                        })
                        .with_children(|entry| {
                            entry.spawn(text(pact.name(), 18.0, CRIMSON));
                            entry.spawn(text((pact.drawback)(round), 13.0, DRAWBACK_CRIMSON));
                        });
                }
            });
    }
}
