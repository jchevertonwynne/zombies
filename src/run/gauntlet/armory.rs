//! The Armory, reached from the Mode screen while Gauntlet is highlighted and never mid-Run: Scrip
//! is spent on Unlocks and Upgrade Track Ranks, and Tracks are switched on or off. A third tab lists
//! the Commendations, and the Unlocks tab ends in the Duo codex. Every purchase or toggle changes
//! `GauntletFile`, which is saved on change.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::boon::{BOONS, Rarity, find as find_boon};
use super::commendations::{COMMENDATIONS, Group};
use super::duos::DUOS;
use super::heat::{Condition, scrip_multiplier};
use super::pact::{CRIMSON, PACTS};
use super::progress::{Card, GauntletFile, Refusal, Track};
use crate::ui::hud::{ROUND_RED, text, with_commas};
use crate::ui::menu::{
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
    Commendations,
    Heat,
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
    Heat(Condition),
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
                        ("Commendations", ArmoryTab::Commendations),
                        ("Heat", ArmoryTab::Heat),
                    ] {
                        let color = if each == tab { ROUND_RED } else { Color::WHITE };
                        colored_button(tabs, label, ArmoryButton::Tab(each), 200.0, color);
                    }
                });
            match tab {
                ArmoryTab::Unlocks => unlocks_tab(board, &file),
                ArmoryTab::Upgrades => upgrades_tab(board, &file),
                ArmoryTab::Commendations => commendations_tab(board, &file),
                ArmoryTab::Heat => heat_tab(board, &file),
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
    duo_codex(board, file);
}

/// Every Duo Boon's recipe, from the start, with a recipe Boon not yet Unlocked greyed out.
fn duo_codex(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    board.spawn((
        text("DUO CODEX", 22.0, Color::WHITE),
        Node {
            margin: UiRect::top(px(6.0)),
            ..default()
        },
    ));
    board
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            column_gap: px(18.0),
            max_width: px(1500.0),
            ..default()
        })
        .with_children(|codex| {
            for duo in DUOS {
                let Some(recipe) = duo.duo else { continue };
                codex
                    .spawn(Node {
                        column_gap: px(6.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn(text(duo.name, 16.0, Rarity::Legendary.colour()));
                        for (at, id) in recipe.recipe.iter().enumerate() {
                            let Some(boon) = find_boon(id) else { continue };
                            let colour = if file.is_unlocked(Card::Boon(boon)) {
                                recipe.families[at].colour()
                            } else {
                                HINT_GREY
                            };
                            let joiner = if at == 0 { "=" } else { "+" };
                            row.spawn(text(format!("{joiner} {}", boon.name), 16.0, colour));
                        }
                    });
            }
        });
}

/// Each Commendation by group, its reward and whether it's earned, with the Kit a Family one earns.
fn commendations_tab(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    let earned = COMMENDATIONS
        .iter()
        .filter(|commendation| file.has_commendation(commendation.id))
        .count();
    board.spawn(text(
        format!(
            "{earned} of {} earned. Each pays its Scrip once, whatever the Heat. A Family one earns that Family's Kit.",
            COMMENDATIONS.len()
        ),
        18.0,
        HINT_GREY,
    ));
    board
        .spawn(Node {
            column_gap: px(28.0),
            align_items: AlignItems::FlexStart,
            ..default()
        })
        .with_children(|row| {
            for group in Group::ALL {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4.0),
                    ..default()
                })
                .with_children(|column| {
                    column.spawn(text(group.name(), 22.0, Color::WHITE));
                    for commendation in COMMENDATIONS.iter().filter(|c| c.group == group) {
                        let done = file.has_commendation(commendation.id);
                        let (mark, colour) = if done {
                            ("[x]", UNLOCKED_GREEN)
                        } else {
                            ("[ ]", HINT_GREY)
                        };
                        column.spawn(text(
                            format!("{mark} {}  +{}", commendation.name, commendation.scrip),
                            17.0,
                            colour,
                        ));
                    }
                });
            }
        });
}

/// "●●○" for `degree` of `max` Degrees.
fn pips(degree: u32, max: u32) -> String {
    (1..=max).map(|at| if at <= degree { '●' } else { '○' }).collect()
}

/// Heat, once a Run is Won: a row per Condition with its Degree pips and what the Degree does.
/// Clicking a row raises its Degree, wrapping back to 0. The header shows the Heat and the Scrip it
/// pays; before the first Win it just says how to unlock it.
fn heat_tab(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    if !file.has_won {
        board.spawn(text("Win a Run to unlock Heat", 24.0, HINT_GREY));
        return;
    }
    let run = file.run_heat();
    board.spawn(text(
        format!("Heat {} · Scrip ×{:.1}", run.total(), scrip_multiplier(run.total())),
        26.0,
        NEW_BEST_GOLD,
    ));
    board.spawn(text(
        "Each Degree is 1 Heat. Click a Condition to raise it; past its top it goes back to 0.",
        18.0,
        HINT_GREY,
    ));
    for condition in Condition::ALL {
        let degree = run.degree(condition);
        board
            .spawn(Node { align_items: AlignItems::Center, column_gap: px(12.0), ..default() })
            .with_children(|row| {
                let colour = if degree > 0 { ROUND_RED } else { Color::WHITE };
                small_button(row, condition.name(), ArmoryButton::Heat(condition), 220.0, colour);
                cell(row, pips(degree, condition.max_degree()), 90.0, colour);
                cell(row, condition.effect(degree), 560.0, if degree > 0 { Color::WHITE } else { HINT_GREY });
            });
    }
}

/// A row per Upgrade Track: its Ranks, its effect now and after the next Rank, the next Rank's
/// price, and its free on/off toggle.
fn upgrades_tab(board: &mut ChildSpawnerCommands, file: &GauntletFile) {
    board.spawn(text(
        format!(
            "Ranks switched on apply to every Gauntlet Run: {} of {} active.",
            file.active_ranks(),
            Track::total_ranks()
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
                cell(row, format!("Rank {ranks}/{}", track.max_ranks()), 110.0, colour);
                let effect = match track.rank_price(ranks) {
                    Some(_) => format!("{} -> {}", track.effect(ranks), track.effect(ranks + 1)),
                    None => track.effect(ranks),
                };
                cell(row, effect, 220.0, colour);
                match track.rank_price(ranks) {
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
        Refusal::HeatLocked => "Win a Run to unlock Heat.",
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
        ArmoryButton::Heat(condition) => next.cycle_heat(condition),
    };
    match result {
        Ok(()) => {
            *file = next;
            note.0.clear();
        }
        Err(refusal) => note.0 = refusal_line(refusal).to_string(),
    }
}
