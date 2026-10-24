use bevy::prelude::*;

use crate::player::{Health, Player};
use crate::points::{Points, PointsEarned};
use crate::round::{Round, RoundPhase};
use crate::state::{GameState, InRun, Pause};
use crate::weapon::Weapon;

const ROUND_RED: Color = Color::srgb(0.7, 0.05, 0.02);
const POPUP_SECS: f32 = 0.8;

#[derive(Component)]
struct DamageTint;

#[derive(Component)]
struct RoundText;

#[derive(Component)]
struct PointsText;

#[derive(Component)]
struct AmmoText;

#[derive(Component)]
struct PopupColumn;

#[derive(Component)]
struct Popup {
    age: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(OnEnter(Pause::Paused), spawn_pause_screen)
            .add_systems(OnEnter(GameState::GameOver), spawn_game_over_screen)
            .add_systems(
                Update,
                (damage_tint, round_text, points_text, ammo_text, spawn_popups, animate_popups)
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(Update, restart.run_if(in_state(GameState::GameOver)));
    }
}

fn text(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn full_screen() -> Node {
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
            column.spawn((PointsText, text("0", 36.0, Color::srgb(1.0, 0.9, 0.5))));
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

fn points_text(points: Res<Points>, mut text: Single<&mut Text, With<PointsText>>) {
    text.0 = points.0.to_string();
}

fn ammo_text(weapon: Single<&Weapon, With<Player>>, mut text: Single<&mut Text, With<AmmoText>>) {
    let reserve = weapon.reserve.map_or("--".to_string(), |r| r.to_string());
    text.0 = if weapon.reloading.is_some() {
        format!("RELOADING  {reserve}")
    } else {
        format!("{} / {reserve}", weapon.magazine)
    };
}

fn spawn_popups(
    mut commands: Commands,
    mut earned: MessageReader<PointsEarned>,
    column: Single<Entity, With<PopupColumn>>,
) {
    for PointsEarned(amount) in earned.read() {
        commands.entity(*column).with_child((
            Popup { age: 0.0 },
            text(format!("+{amount}"), 24.0, Color::srgb(1.0, 0.85, 0.2)),
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

fn overlay(commands: &mut Commands, lines: &[(&str, f32, Color)]) -> Entity {
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

fn spawn_pause_screen(mut commands: Commands) {
    let screen = overlay(
        &mut commands,
        &[
            ("PAUSED", 64.0, Color::WHITE),
            ("Esc to resume", 24.0, Color::srgb(0.7, 0.7, 0.7)),
        ],
    );
    commands.entity(screen).insert(DespawnOnExit(Pause::Paused));
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
