//! The Armory, reached from the Mode screen while Gauntlet is highlighted and never mid-Run: Scrip
//! is spent on Unlocks and Upgrade Track Ranks, and Tracks are switched on or off. Every purchase
//! or toggle changes `GauntletFile`, which is saved on change.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::boon::{BOONS, Rarity};
use super::pact::{CRIMSON, PACTS};
use super::progress::{Card, GauntletFile, Refusal, TRACK_RANKS, Track, rank_price};
use crate::hud::{ROUND_RED, text, with_commas};
use crate::menu::{
    HINT_GREY, MENU_BACKGROUND, MenuButton, MenuButtonLook, NEW_BEST_GOLD, button, colored_button,
    screen,
};
use crate::state::MainMenuScreen;

const BUTTON_IDLE: Color = Color::srgba(0.15, 0.15, 0.15, 0.9);
const UNLOCKED_GREEN: Color = Color::srgb(0.45, 0.9, 0.45);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum ArmoryTab {
    #[default]
    Unlocks,
    Upgrades,
}

/// The Armory's last refusal, shown under the Scrip balance until the next purchase.
#[derive(Resource, Default)]
pub struct ArmoryNote(String);

/// The part of the Armory rebuilt when the tab, the Scrip or a purchase changes.
#[derive(Component)]
pub struct ArmoryBoard;

/// What an Armory button does when clicked.
#[derive(Component, Clone, Copy)]
pub enum ArmoryButton {
    Tab(ArmoryTab),
    Unlock(&'static str),
    BuyRank(Track),
    Toggle(Track),
}

pub fn spawn_armory(
    mut commands: Commands,
    mut tab: ResMut<ArmoryTab>,
    mut note: ResMut<ArmoryNote>,
) {
    // Always written, so the board below is built on this first frame.
    *tab = ArmoryTab::Unlocks;
    note.0.clear();
    let screen = screen(&mut commands, MENU_BACKGROUND, |screen| {
        screen.spawn(text("ARMORY", 56.0, Color::WHITE));
        screen.spawn((
            ArmoryBoard,
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10.0),
                ..default()
            },
        ));
        button(screen, "Back", MenuButton::Back, 280.0);
    });
    commands
        .entity(screen)
        .insert(DespawnOnExit(MainMenuScreen::Armory));
}

/// A smaller menu button, so a whole tab fits on screen.
fn small_button(
    parent: &mut ChildSpawnerCommands,
    label: impl Into<String>,
    action: ArmoryButton,
    width: f32,
    color: Color,
) {
    parent
        .spawn((
            Button,
            Hovered::default(),
            MenuButtonLook,
            action,
            Node {
                width: px(width),
                height: px(34.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BUTTON_IDLE),
        ))
        .with_child(text(label, 20.0, color));
}

/// A fixed-width cell of text in a row.
fn cell(row: &mut ChildSpawnerCommands, value: impl Into<String>, width: f32, color: Color) {
    row.spawn((
        text(value, 20.0, color),
        Node {
            width: px(width),
            ..default()
        },
    ));
}

/// The tab, the note or the progress changed, so the board needs rebuilding.
pub fn armory_changed(tab: Res<ArmoryTab>, note: Res<ArmoryNote>, file: Res<GauntletFile>) -> bool {
    tab.is_changed() || note.is_changed() || file.is_changed()
}

pub fn refresh_armory(
    mut commands: Commands,
    board: Query<Entity, With<ArmoryBoard>>,
    tab: Res<ArmoryTab>,
    note: Res<ArmoryNote>,
    file: Res<GauntletFile>,
) {
    let Ok(board) = board.single() else {
        return;
    };
    let tab = *tab;
    commands
        .entity(board)
        .despawn_children()
        .with_children(|board| {
            board.spawn(text(
                format!(
                    "Scrip {}   (all-time {})",
                    with_commas(file.scrip),
                    with_commas(file.all_time_scrip)
                ),
                28.0,
                NEW_BEST_GOLD,
            ));
            board.spawn(text(note.0.clone(), 20.0, ROUND_RED));
            board
                .spawn(Node {
                    column_gap: px(12.0),
                    margin: UiRect::bottom(px(6.0)),
                    ..default()
                })
                .with_children(|tabs| {
                    for (label, each) in [
                        ("Unlocks", ArmoryTab::Unlocks),
                        ("Upgrades", ArmoryTab::Upgrades),
                    ] {
                        let color = if each == tab { ROUND_RED } else { Color::WHITE };
                        colored_button(tabs, label, ArmoryButton::Tab(each), 200.0, color);
                    }
                });
            match tab {
                ArmoryTab::Unlocks => unlocks_tab(board, &file),
                ArmoryTab::Upgrades => upgrades_tab(board, &file),
            }
        });
}

/// Three columns of the cards bought with Scrip, Rares, Legendaries and Pacts, each a button
/// showing its price or that it's Unlocked.
fn unlocks_tab(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    board.spawn(text(
        "Every Common, War Bonds, Barbed Wire and Shrapnel are Unlocked from the start.",
        18.0,
        HINT_GREY,
    ));
    let rares = BOONS
        .iter()
        .filter(|boon| boon.rarity == Rarity::Rare && !boon.starts_unlocked)
        .map(|boon| (Card::Boon(boon), boon.name, boon.rarity.colour()));
    let legendaries = BOONS
        .iter()
        .filter(|boon| boon.rarity == Rarity::Legendary && !boon.starts_unlocked)
        .map(|boon| (Card::Boon(boon), boon.name, boon.rarity.colour()));
    let pacts = PACTS
        .iter()
        .map(|pact| (Card::Pact(pact.id()), pact.name(), CRIMSON));
    let columns: [(&str, Vec<_>); 3] = [
        ("RARES", rares.collect()),
        ("LEGENDARIES", legendaries.collect()),
        ("PACTS", pacts.collect()),
    ];
    board
        .spawn(Node {
            column_gap: px(24.0),
            align_items: AlignItems::FlexStart,
            ..default()
        })
        .with_children(|row| {
            for (heading, cards) in columns {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(6.0),
                    ..default()
                })
                .with_children(|column| {
                    column.spawn(text(heading, 22.0, Color::WHITE));
                    for (card, name, colour) in cards {
                        let label = match card.unlock_price() {
                            _ if file.is_unlocked(card) => format!("{name}  Unlocked"),
                            Some(price) => format!("{name}  {price}"),
                            None => name.to_string(),
                        };
                        let colour = if file.is_unlocked(card) {
                            UNLOCKED_GREEN
                        } else if card.unlock_price().is_some_and(|price| price > file.scrip) {
                            HINT_GREY
                        } else {
                            colour
                        };
                        small_button(
                            column,
                            label,
                            ArmoryButton::Unlock(card.id()),
                            320.0,
                            colour,
                        );
                    }
                });
            }
        });
}

/// A row per Upgrade Track: its Ranks, its effect now and after the next Rank, the next Rank's
/// price, and its free on/off toggle.
fn upgrades_tab(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    board.spawn(text(
        format!(
            "Ranks switched on apply to every Gauntlet Run: {} of {} active.",
            file.active_ranks(),
            TRACK_RANKS * Track::ALL.len() as u32
        ),
        18.0,
        HINT_GREY,
    ));
    for track in Track::ALL {
        let ranks = file.ranks(track);
        let on = file.is_on(track);
        board
            .spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(12.0),
                ..default()
            })
            .with_children(|row| {
                let colour = if on { Color::WHITE } else { HINT_GREY };
                cell(row, track.name(), 220.0, colour);
                cell(row, format!("Rank {ranks}/{TRACK_RANKS}"), 110.0, colour);
                let effect = match rank_price(ranks) {
                    Some(_) => format!("{} -> {}", track.effect(ranks), track.effect(ranks + 1)),
                    None => track.effect(ranks),
                };
                cell(row, effect, 220.0, colour);
                match rank_price(ranks) {
                    Some(price) => {
                        let colour = if price > file.scrip {
                            HINT_GREY
                        } else {
                            NEW_BEST_GOLD
                        };
                        small_button(
                            row,
                            format!("Buy {price}"),
                            ArmoryButton::BuyRank(track),
                            150.0,
                            colour,
                        );
                    }
                    None => cell(row, "Maxed", 150.0, UNLOCKED_GREEN),
                }
                let (label, colour) = if on {
                    ("On", UNLOCKED_GREEN)
                } else {
                    ("Off", ROUND_RED)
                };
                small_button(row, label, ArmoryButton::Toggle(track), 90.0, colour);
            });
    }
}

/// The Armory's line for a refused purchase.
fn refusal_line(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::TooPoor => "Not enough Scrip.",
        Refusal::AlreadyUnlocked => "Already Unlocked.",
        Refusal::TrackFull => "Every Rank is bought.",
        Refusal::Unknown => "Can't be Unlocked.",
    }
}

/// A click on an Armory button. A refused purchase changes nothing but the note.
pub fn armory_click(
    activate: On<Activate>,
    buttons: Query<&ArmoryButton>,
    mut tab: ResMut<ArmoryTab>,
    mut note: ResMut<ArmoryNote>,
    mut file: ResMut<GauntletFile>,
) {
    let Ok(&action) = buttons.get(activate.entity) else {
        return;
    };
    let mut next = file.clone();
    let result = match action {
        ArmoryButton::Tab(picked) => {
            *tab = picked;
            note.0.clear();
            return;
        }
        ArmoryButton::Unlock(id) => next.unlock(id),
        ArmoryButton::BuyRank(track) => next.buy_rank(track),
        ArmoryButton::Toggle(track) => {
            next.toggle(track);
            Ok(())
        }
    };
    match result {
        Ok(()) => {
            *file = next;
            note.0.clear();
        }
        Err(refusal) => note.0 = refusal_line(refusal).to_string(),
    }
}
