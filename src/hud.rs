use bevy::prelude::*;

use crate::buy::{Buyable, CantAfford, Focus, can_afford};
use crate::player::{Health, Player};
use crate::points::{Points, PointsAwarded};
use crate::round::{Round, RoundPhase};
use crate::state::{GameState, InRun};
use crate::tuning::CANT_AFFORD_FLASH_SECS;
use crate::weapon::Loadout;

pub const ROUND_RED: Color = Color::srgb(0.7, 0.05, 0.02);
const POPUP_SECS: f32 = 0.8;
const POINTS_GOLD: Color = Color::srgb(1.0, 0.9, 0.5);
const CANT_AFFORD_RED: Color = Color::srgb(1.0, 0.15, 0.1);

#[derive(Component)]
struct DamageTint;

#[derive(Component)]
struct RoundText;

#[derive(Component)]
struct PointsText;

#[derive(Component)]
struct AmmoText;

/// The Prompt under the crosshair. Its one child span holds the price.
#[derive(Component)]
struct PromptText;

#[derive(Component)]
struct PromptCost;

/// Seconds left of the red Points flash after a refused buy.
#[derive(Resource, Default)]
struct PointsFlash(f32);

#[derive(Component)]
struct PopupColumn;

#[derive(Component)]
struct Popup {
    age: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointsFlash>()
            .add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(OnEnter(GameState::GameOver), spawn_game_over_screen)
            .add_systems(
                Update,
                (
                    damage_tint,
                    round_text,
                    flash_points,
                    points_text,
                    ammo_text,
                    prompt_text,
                    spawn_popups,
                    animate_popups,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, restart.run_if(in_state(GameState::GameOver)));
    }
}

/// A whole number with comma thousands separators, as every Points figure is shown.
pub fn with_commas(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

pub fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

pub fn full_screen() -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        height: percent(100.0),
        ..default()
    }
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((InRun, DamageTint, full_screen(), BackgroundColor(Color::NONE)));

    // Crosshair: a small plus in the middle of the screen.
    commands
        .spawn((
            InRun,
            Node {
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..full_screen()
            },
        ))
        .with_children(|centre| {
            for (w, h) in [(14.0, 2.0), (2.0, 14.0)] {
                centre.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(w),
                        height: px(h),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.8)),
                ));
            }
        });

    // The Prompt, just under the crosshair.
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(56.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|row| {
            row.spawn((PromptText, text("", 22.0, Color::WHITE), Visibility::Hidden))
                .with_child((PromptCost, TextSpan::default(), TextColor(Color::WHITE)));
        });

    commands.spawn((
        InRun,
        RoundText,
        text("1", 96.0, ROUND_RED),
        Node {
            position_type: PositionType::Absolute,
            left: px(32.0),
            bottom: px(16.0),
            ..default()
        },
    ));

    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                right: px(32.0),
                bottom: px(24.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexEnd,
                row_gap: px(4.0),
                ..default()
            },
        ))
        .with_children(|column| {
            column.spawn((
                PopupColumn,
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexEnd,
                    height: px(120.0),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },
            ));
            column.spawn((PointsText, text("0", 36.0, POINTS_GOLD)));
            column.spawn((AmmoText, text("", 28.0, Color::WHITE)));
        });
}

fn damage_tint(
    health: Single<&Health, With<Player>>,
    mut tint: Single<&mut BackgroundColor, With<DamageTint>>,
) {
    let hurt = 1.0 - health.fraction();
    tint.0 = Color::srgba(0.6, 0.0, 0.0, hurt * 0.55);
}

fn round_text(
    round: Option<Res<Round>>,
    text: Single<(&mut Text, &mut TextColor), With<RoundText>>,
    time: Res<Time>,
) {
    let Some(round) = round else { return };
    let (mut text, mut color) = text.into_inner();
    text.0 = round.number.to_string();
    // Flash between Rounds.
    color.0 = match round.phase {
        RoundPhase::Intermission(_) if (time.elapsed_secs() * 3.0).fract() < 0.5 => Color::WHITE,
        _ => ROUND_RED,
    };
}

fn flash_points(mut flash: ResMut<PointsFlash>, mut refused: MessageReader<CantAfford>, time: Res<Time>) {
    flash.0 = (flash.0 - time.delta_secs()).max(0.0);
    if refused.read().count() > 0 {
        flash.0 = CANT_AFFORD_FLASH_SECS;
    }
}

fn points_text(
    points: Res<Points>,
    flash: Res<PointsFlash>,
    text: Single<(&mut Text, &mut TextColor), With<PointsText>>,
) {
    let (mut text, mut color) = text.into_inner();
    text.0 = with_commas(points.balance);
    color.0 = if flash.0 > 0.0 { CANT_AFFORD_RED } else { POINTS_GOLD };
}

/// The focused Buyable's Prompt, always with its price, in red when the Player can't afford it.
fn prompt_text(
    focus: Res<Focus>,
    buyables: Query<&Buyable>,
    points: Res<Points>,
    prompt: Single<(&mut Text, &mut Visibility), With<PromptText>>,
    cost: Single<(&mut TextSpan, &mut TextColor), With<PromptCost>>,
) {
    let (mut text, mut visibility) = prompt.into_inner();
    let Some(buyable) = focus.0.and_then(|e| buyables.get(e).ok()) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    if text.0 != buyable.prompt {
        text.0.clone_from(&buyable.prompt);
    }
    let (mut span, mut color) = cost.into_inner();
    let price = buyable.price.map_or(String::new(), |price| format!(" [Cost: {}]", with_commas(price)));
    if span.0 != price {
        span.0 = price;
    }
    color.0 = if can_afford(points.balance, buyable.price) { Color::WHITE } else { CANT_AFFORD_RED };
}

fn ammo_text(loadout: Single<&Loadout, With<Player>>, mut text: Single<&mut Text, With<AmmoText>>) {
    let weapon = loadout.held();
    let (name, reserve) = (weapon.stats().name, weapon.reserve);
    text.0 = if loadout.reload.is_some() {
        format!("{name}  RELOADING  {reserve}")
    } else {
        format!("{name}  {} / {reserve}", weapon.magazine)
    };
}

fn spawn_popups(
    mut commands: Commands,
    mut awarded: MessageReader<PointsAwarded>,
    column: Single<Entity, With<PopupColumn>>,
) {
    for PointsAwarded(amount) in awarded.read() {
        commands.entity(*column).with_child((
            Popup { age: 0.0 },
            text(format!("+{}", with_commas(*amount)), 24.0, Color::srgb(1.0, 0.85, 0.2)),
        ));
    }
}

fn animate_popups(
    mut commands: Commands,
    mut popups: Query<(Entity, &mut Popup, &mut TextColor)>,
    time: Res<Time>,
) {
    for (entity, mut popup, mut color) in &mut popups {
        popup.age += time.delta_secs();
        if popup.age >= POPUP_SECS {
            commands.entity(entity).despawn();
        } else {
            color.0.set_alpha(1.0 - popup.age / POPUP_SECS);
        }
    }
}

pub fn overlay(commands: &mut Commands, lines: &[(&str, f32, Color)]) -> Entity {
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(16.0),
                ..full_screen()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            GlobalZIndex(10),
        ))
        .with_children(|overlay| {
            for (line, size, color) in lines {
                overlay.spawn(text(*line, *size, *color));
            }
        })
        .id()
}

fn spawn_game_over_screen(mut commands: Commands, round: Res<Round>) {
    let survived = format!(
        "You survived {} round{}",
        round.number,
        if round.number == 1 { "" } else { "s" }
    );
    let screen = overlay(
        &mut commands,
        &[
            ("GAME OVER", 80.0, ROUND_RED),
            (&survived, 32.0, Color::WHITE),
            ("Press Enter to play again", 24.0, Color::srgb(0.7, 0.7, 0.7)),
        ],
    );
    commands.entity(screen).insert(DespawnOnExit(GameState::GameOver));
}

fn restart(input: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<GameState>>) {
    if input.just_pressed(KeyCode::Enter) {
        next.set(GameState::Playing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_numbers_have_no_separator() {
        assert_eq!(with_commas(0), "0");
        assert_eq!(with_commas(10), "10");
        assert_eq!(with_commas(999), "999");
    }

    #[test]
    fn thousands_are_separated_by_commas() {
        assert_eq!(with_commas(1_000), "1,000");
        assert_eq!(with_commas(25_500), "25,500");
        assert_eq!(with_commas(1_234_567), "1,234,567");
    }

    #[test]
    fn the_largest_points_total_still_formats() {
        assert_eq!(with_commas(u64::MAX), "18,446,744,073,709,551,615");
    }
}
