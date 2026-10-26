//! The BO1 HUD. Bottom-left: Perk icons over the Round counter. Bottom-right: Points with ±N
//! popups, the Weapon in hand with its Ammo Mods, Equipment and `magazine | reserve`. Under the
//! crosshair: the Prompt and a Reload hint. Bottom-centre: timer bars for timed Power-ups, with a
//! banner for instant ones. Under it all, the effect overlay layer. No Box, Power or Pack-a-Punch
//! state shows.

pub mod overlay;

use bevy::prelude::*;

use crate::controls::with_use_key;
use crate::settings::Settings;
use crate::ammo_mod::AmmoMod;
use crate::attachment::Underbarrel;
use crate::buy::{Buyable, CantAfford, Focus, HoldBuy, can_afford, charged};
use crate::equipment::Equipment;
use crate::last_stand::LastStand;
use crate::modifier::Modifiers;
use crate::perk::{Perk, Perks};
use crate::player::{Health, Player};
use crate::points::{Points, PointsAwarded};
use crate::power_up::{ActivePowerUps, PowerUp, PowerUpTaken};
use crate::round::{Round, RoundPhase};
use crate::state::{GameState, InRun};
use crate::tuning::*;
use crate::weapon::{Knife, Loadout, UPGRADED_PURPLE};
use overlay::{Overlay, OverlayPlugin, Overlays};

pub const ROUND_RED: Color = Color::srgb(0.7, 0.05, 0.02);
const POINTS_GOLD: Color = Color::srgb(1.0, 0.9, 0.5);
const EARN_GOLD: Color = Color::srgb(1.0, 0.85, 0.2);
const SPEND_RED: Color = Color::srgb(1.0, 0.3, 0.25);
const CANT_AFFORD_RED: Color = Color::srgb(1.0, 0.15, 0.1);
const DETAIL_GREY: Color = Color::srgb(0.7, 0.7, 0.7);
const HINT_YELLOW: Color = Color::srgb(1.0, 0.85, 0.3);
const FRAG_GREEN: Color = Color::srgb(0.5, 0.6, 0.4);

/// How far up from the bottom of the screen the bottom-left corner (Perks over the Round counter)
/// reaches. Anything else down the left edge stays above it.
pub const BOTTOM_LEFT_HEIGHT_PX: f32 = 200.0;
const ROUND_PX: f32 = 96.0;
const PERK_ICON_PX: f32 = 44.0;
const POWER_UP_BAR_PX: f32 = 120.0;

/// What the Round counter shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundCounter {
    /// Chalk tallies, the fifth striking through the other four.
    Tallies(u32),
    Numeral(u32),
}

/// Chalk tallies up to `ROUND_TALLY_MAX`, then numerals.
pub fn round_counter(round: u32) -> RoundCounter {
    if round <= ROUND_TALLY_MAX {
        RoundCounter::Tallies(round)
    } else {
        RoundCounter::Numeral(round)
    }
}

/// Whether the Magazine is down to `LOW_MAGAZINE_FRACTION` of its size.
pub fn low_magazine(magazine: u32, magazine_size: u32) -> bool {
    magazine_size > 0 && magazine as f32 <= magazine_size as f32 * LOW_MAGAZINE_FRACTION
}

/// The hint under the crosshair for the held Weapon's ammo: Reload once the Magazine runs low and
/// there's Reserve Ammo to load, No Ammo once both are empty, and nothing during a Reload.
pub fn reload_hint(magazine: u32, magazine_size: u32, reserve: u32, reloading: bool) -> Option<&'static str> {
    if reloading || !low_magazine(magazine, magazine_size) {
        None
    } else if reserve > 0 {
        Some("Reload")
    } else if magazine == 0 {
        Some("No Ammo")
    } else {
        None
    }
}

/// The ammo line: `magazine | reserve`.
pub fn ammo_line(magazine: impl std::fmt::Display, reserve: impl std::fmt::Display) -> String {
    format!("{magazine} | {reserve}")
}

/// The line under the Weapon's name: what's on it, in order.
pub fn detail_line<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    names.into_iter().collect::<Vec<_>>().join("  ·  ")
}

/// A Perk icon's label: its initials.
pub fn perk_initials(perk: Perk) -> String {
    perk.name().split(' ').filter_map(|word| word.chars().next()).collect()
}

/// How full a timed Power-up's bar is, from its seconds left.
pub fn bar_fill(left: f32) -> f32 {
    (left / POWER_UP_TIMED_SECS).clamp(0.0, 1.0)
}

/// Whether a timed Power-up's bar is lit: always, until its last `POWER_UP_BLINK_SECS`, then
/// blinking every `POWER_UP_BLINK_INTERVAL_SECS`.
pub fn bar_lit(left: f32, elapsed: f32) -> bool {
    left > POWER_UP_BLINK_SECS || ((elapsed / POWER_UP_BLINK_INTERVAL_SECS) as u64).is_multiple_of(2)
}

/// The change in the Points balance that the frame's awards don't account for: negative for a
/// purchase, positive for a refund.
pub fn unexplained_change(before: u64, after: u64, awarded: u64) -> i64 {
    after as i64 - before as i64 - awarded as i64
}

/// The instant Power-up whose name is showing, and for how much longer.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct Banner(Option<(PowerUp, f32)>);

impl Banner {
    /// Shows an instant Power-up's name for `INSTANT_POWER_UP_BANNER_SECS`. Timed ones have bars.
    pub fn taken(&mut self, power_up: PowerUp) {
        if !ActivePowerUps::TIMED.contains(&power_up) {
            self.0 = Some((power_up, INSTANT_POWER_UP_BANNER_SECS));
        }
    }

    pub fn tick(&mut self, dt: f32) {
        if let Some((_, left)) = &mut self.0 {
            *left -= dt;
            if *left <= 0.0 {
                self.0 = None;
            }
        }
    }

    pub fn showing(&self) -> Option<PowerUp> {
        self.0.map(|(power_up, _)| power_up)
    }
}

/// The Round counter's numeral.
#[derive(Component)]
struct RoundNumeral;

/// The box holding the chalk tallies.
#[derive(Component)]
struct TallyBox;

/// One chalk stroke: the first four stand, the fifth strikes through.
#[derive(Component)]
struct Tally(u32);

#[derive(Component)]
struct PerkRow;

#[derive(Component)]
struct PointsText;

#[derive(Component)]
struct WeaponName;

/// The Weapon's Attachments and Ammo Mods.
#[derive(Component)]
struct WeaponDetails;

/// One pip per frag carried.
#[derive(Component)]
struct LethalPips;

/// The Tactical, Claymores and Bowie Knife, as held.
#[derive(Component)]
struct EquipmentText;

#[derive(Component)]
struct AmmoText;

/// The Prompt under the crosshair. Its child spans hold the price, then a Buyable's hold half
/// and its price on a second line.
#[derive(Component)]
struct PromptText;

#[derive(Component)]
struct PromptCost;

#[derive(Component)]
struct PromptHold;

#[derive(Component)]
struct PromptHoldCost;

#[derive(Component)]
struct ReloadHint;

/// A timed Power-up's bar, bottom-centre.
#[derive(Component)]
struct PowerUpBar(PowerUp);

/// The draining part of a timed Power-up's bar.
#[derive(Component)]
struct PowerUpFill(PowerUp);

#[derive(Component)]
struct BannerText;

/// Seconds left of the red Points flash after a refused buy.
#[derive(Resource, Default)]
struct PointsFlash(f32);

/// The balance the popups last saw; none yet at the start of a Run.
#[derive(Resource, Default)]
struct LastBalance(Option<u64>);

#[derive(Component)]
struct PopupColumn;

#[derive(Component)]
struct Popup {
    age: f32,
}

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(OverlayPlugin)
            .init_resource::<PointsFlash>()
            .init_resource::<LastBalance>()
            .init_resource::<Banner>()
            .add_systems(OnEnter(GameState::Playing), (reset, spawn_hud))
            .add_systems(
                Update,
                (
                    effect_overlays,
                    (round_counter_text, perk_row),
                    (flash_points, points_text, spawn_popups, animate_popups).chain(),
                    (weapon_text, equipment_text),
                    (prompt_text, reload_hint_text),
                    (power_up_bars, banner_text),
                )
                    .run_if(in_state(GameState::Playing)),
            );
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

fn column(gap: f32, align: AlignItems) -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        align_items: align,
        row_gap: px(gap),
        ..default()
    }
}

fn row(gap: f32) -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(gap),
        ..default()
    }
}

fn reset(mut banner: ResMut<Banner>, mut balance: ResMut<LastBalance>, mut flash: ResMut<PointsFlash>) {
    *banner = Banner::default();
    balance.0 = None;
    flash.0 = 0.0;
}

fn spawn_hud(mut commands: Commands) {
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

    // The Prompt, just under the crosshair, and the Reload hint under that.
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(56.0),
                ..column(4.0, AlignItems::Center)
            },
        ))
        .with_children(|below| {
            below
                .spawn((PromptText, text("", 22.0, Color::WHITE), Visibility::Hidden))
                .with_child((PromptCost, TextSpan::default(), TextColor(Color::WHITE)))
                .with_child((PromptHold, TextSpan::default(), TextColor(Color::WHITE)))
                .with_child((PromptHoldCost, TextSpan::default(), TextColor(Color::WHITE)));
            below.spawn((ReloadHint, text("", 20.0, HINT_YELLOW), Visibility::Hidden));
        });

    // Bottom-left: Perks over the Round counter.
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                left: px(32.0),
                bottom: px(16.0),
                ..column(8.0, AlignItems::FlexStart)
            },
        ))
        .with_children(|left| {
            left.spawn((PerkRow, row(6.0)));
            left.spawn(Node {
                height: px(ROUND_PX),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|counter| {
                counter.spawn((RoundNumeral, text("", ROUND_PX, ROUND_RED)));
                spawn_tallies(counter);
            });
        });

    // Bottom-right: Points, the Weapon, Equipment and ammo.
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                right: px(32.0),
                bottom: px(24.0),
                ..column(4.0, AlignItems::FlexEnd)
            },
        ))
        .with_children(|right| {
            right.spawn((
                PopupColumn,
                Node {
                    height: px(120.0),
                    justify_content: JustifyContent::FlexEnd,
                    ..column(0.0, AlignItems::FlexEnd)
                },
            ));
            right.spawn((PointsText, text("0", 36.0, POINTS_GOLD)));
            right.spawn((WeaponName, text("", 20.0, Color::WHITE)));
            right.spawn((WeaponDetails, text("", 15.0, DETAIL_GREY)));
            right.spawn(row(14.0)).with_children(|equipment| {
                equipment.spawn((EquipmentText, text("", 16.0, DETAIL_GREY)));
                equipment.spawn((LethalPips, row(4.0)));
            });
            right.spawn((AmmoText, text("", 32.0, Color::WHITE)));
        });

    // Bottom-centre: a bar for each timed Power-up.
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                bottom: px(24.0),
                justify_content: JustifyContent::Center,
                column_gap: px(16.0),
                ..default()
            },
        ))
        .with_children(|centre| {
            for power_up in ActivePowerUps::TIMED {
                centre
                    .spawn((
                        PowerUpBar(power_up),
                        Node {
                            display: Display::None,
                            ..column(3.0, AlignItems::Center)
                        },
                    ))
                    .with_children(|bar| {
                        bar.spawn(text(power_up.name().to_uppercase(), 14.0, power_up.colour()));
                        bar.spawn((
                            Node {
                                width: px(POWER_UP_BAR_PX),
                                height: px(8.0),
                                border: UiRect::all(px(1.0)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                            BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                        ))
                        .with_child((
                            PowerUpFill(power_up),
                            Node {
                                width: percent(100.0),
                                height: percent(100.0),
                                ..default()
                            },
                            BackgroundColor(power_up.colour()),
                        ));
                    });
            }
        });

    // An instant Power-up's name, high in the middle.
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(25.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child((BannerText, text("", 40.0, Color::WHITE), Visibility::Hidden));
}

/// Four standing strokes and a fifth across them, sized to the Round counter.
fn spawn_tallies(counter: &mut ChildSpawnerCommands) {
    let stroke = ROUND_PX * 0.08;
    counter
        .spawn((
            TallyBox,
            Node {
                height: px(ROUND_PX),
                align_items: AlignItems::Center,
                column_gap: px(stroke * 2.0),
                padding: UiRect::horizontal(px(stroke)),
                ..default()
            },
        ))
        .with_children(|tallies| {
            for i in 1..ROUND_TALLY_MAX {
                tallies.spawn((
                    Tally(i),
                    Node {
                        width: px(stroke),
                        height: px(ROUND_PX * 0.75),
                        ..default()
                    },
                    BackgroundColor(ROUND_RED),
                ));
            }
            tallies.spawn((
                Tally(ROUND_TALLY_MAX),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    width: px(stroke * 9.0),
                    height: px(stroke),
                    ..default()
                },
                UiTransform::from_rotation(Rot2::degrees(-35.0)),
                BackgroundColor(ROUND_RED),
            ));
        });
}

/// Low-Health redness and the Last Stand, through the overlay layer.
fn effect_overlays(player: Single<(&Health, Has<LastStand>), With<Player>>, mut overlays: ResMut<Overlays>) {
    let (health, last_stand) = *player;
    overlays.hold(Overlay::LowHealth, 1.0 - health.fraction());
    if last_stand {
        overlays.hold(Overlay::LastStand, 1.0);
    }
}

/// The Round as tallies or a numeral, flashing white in the Intermission.
fn round_counter_text(
    round: Option<Res<Round>>,
    numeral: Single<(&mut Text, &mut TextColor, &mut Node), (With<RoundNumeral>, Without<TallyBox>)>,
    mut tally_box: Single<&mut Node, (With<TallyBox>, Without<Tally>)>,
    mut tallies: Query<(&Tally, &mut Node, &mut BackgroundColor), Without<RoundNumeral>>,
    time: Res<Time>,
) {
    let Some(round) = round else { return };
    let colour = match round.phase {
        RoundPhase::Intermission(_) if (time.elapsed_secs() * 3.0).fract() < 0.5 => Color::WHITE,
        _ => ROUND_RED,
    };
    let (mut text, mut text_colour, mut numeral_node) = numeral.into_inner();
    text_colour.set_if_neq(TextColor(colour));
    let (shown, numeral) = match round_counter(round.number) {
        RoundCounter::Tallies(n) => (n, false),
        RoundCounter::Numeral(_) => (0, true),
    };
    numeral_node.display = if numeral { Display::Flex } else { Display::None };
    tally_box.display = if numeral { Display::None } else { Display::Flex };
    let number = round.number.to_string();
    if text.0 != number {
        text.0 = number;
    }
    for (Tally(i), mut node, mut background) in &mut tallies {
        node.display = if *i <= shown { Display::Flex } else { Display::None };
        background.set_if_neq(BackgroundColor(colour));
    }
}

/// An icon for each Perk held, in the order bought.
fn perk_row(mut commands: Commands, perks: Single<Ref<Perks>, With<Player>>, row: Single<Entity, With<PerkRow>>) {
    if !perks.is_changed() {
        return;
    }
    commands.entity(*row).despawn_children().with_children(|row| {
        for &perk in &perks.0 {
            row.spawn((
                Node {
                    width: px(PERK_ICON_PX),
                    height: px(PERK_ICON_PX),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BackgroundColor(perk.colour().with_alpha(0.8)),
                BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)),
            ))
            .with_child(text(perk_initials(perk), PERK_ICON_PX * 0.4, Color::WHITE));
        }
    });
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
    let balance = with_commas(points.balance);
    if text.0 != balance {
        text.0 = balance;
    }
    color.set_if_neq(TextColor(if flash.0 > 0.0 { CANT_AFFORD_RED } else { POINTS_GOLD }));
}

/// A gold +N for each award, and a red -N for whatever left the balance otherwise.
fn spawn_popups(
    mut commands: Commands,
    mut awarded: MessageReader<PointsAwarded>,
    points: Res<Points>,
    mut last: ResMut<LastBalance>,
    column: Single<Entity, With<PopupColumn>>,
) {
    let mut popups = Vec::new();
    let mut total = 0;
    for PointsAwarded(amount) in awarded.read() {
        total += amount;
        popups.push((format!("+{}", with_commas(*amount)), EARN_GOLD));
    }
    let before = last.0.unwrap_or(points.balance);
    let change = unexplained_change(before, points.balance, total);
    if change < 0 {
        popups.push((format!("-{}", with_commas(change.unsigned_abs())), SPEND_RED));
    } else if change > 0 {
        popups.push((format!("+{}", with_commas(change as u64)), EARN_GOLD));
    }
    last.0 = Some(points.balance);
    for (label, colour) in popups {
        commands
            .entity(*column)
            .with_child((Popup { age: 0.0 }, text(label, 24.0, colour)));
    }
}

fn animate_popups(
    mut commands: Commands,
    mut popups: Query<(Entity, &mut Popup, &mut TextColor)>,
    time: Res<Time>,
) {
    for (entity, mut popup, mut color) in &mut popups {
        popup.age += time.delta_secs();
        if popup.age >= HUD_POPUP_SECS {
            commands.entity(entity).despawn();
        } else {
            color.0.set_alpha(1.0 - popup.age / HUD_POPUP_SECS);
        }
    }
}

/// The Weapon in hand, purple if Upgraded, with what's on it and its ammo. With the Underbarrel
/// up, its name and ammo show beside the Weapon's.
fn weapon_text(
    loadout: Single<&Loadout, With<Player>>,
    name: Single<(&mut Text, &mut TextColor), (With<WeaponName>, Without<WeaponDetails>, Without<AmmoText>)>,
    details: Single<(&mut Text, &mut Visibility), (With<WeaponDetails>, Without<AmmoText>)>,
    mut ammo: Single<&mut Text, (With<AmmoText>, Without<WeaponName>, Without<WeaponDetails>)>,
) {
    let weapon = loadout.held();
    let (mut name_text, mut colour) = name.into_inner();
    colour.set_if_neq(TextColor(if weapon.upgraded { UPGRADED_PURPLE } else { Color::WHITE }));
    let name = match loadout.underbarrel() {
        Some(underbarrel) => format!("{} | {}", weapon.stats().name, underbarrel.kind.stats().name),
        None => weapon.stats().name.to_string(),
    };
    if name_text.0 != name {
        name_text.0 = name;
    }
    // Attachments go ahead of the Ammo Mods.
    let attachments = weapon.attachments.iter().map(|attachment| attachment.name());
    let line = detail_line(attachments.chain(weapon.ammo_mods.iter().map(AmmoMod::name)));
    let (mut details, mut visibility) = details.into_inner();
    visibility.set_if_neq(if line.is_empty() { Visibility::Hidden } else { Visibility::Inherited });
    if details.0 != line {
        details.0 = line;
    }
    let line = match loadout.underbarrel() {
        // The Flamethrower's fuel shows in seconds.
        Some(underbarrel) if underbarrel.kind == Underbarrel::Flamethrower => ammo_line(
            format!("{:.1}s", underbarrel.magazine as f32 / FLAMETHROWER_TICKS_PER_SEC),
            format!("{:.0}s", underbarrel.reserve as f32 / FLAMETHROWER_TICKS_PER_SEC),
        ),
        _ => {
            let (magazine, reserve) = loadout.gun_ammo();
            ammo_line(magazine, reserve)
        }
    };
    if ammo.0 != line {
        ammo.0 = line;
    }
}

/// A pip per frag (the Lethal), then the Tactical, Claymores and Bowie Knife once held.
fn equipment_text(
    mut commands: Commands,
    player: Single<(&Equipment, &Knife), With<Player>>,
    mut text: Single<&mut Text, With<EquipmentText>>,
    pips: Single<(Entity, Option<&Children>), With<LethalPips>>,
) {
    let (equipment, knife) = *player;
    let mut parts = Vec::new();
    if knife.bowie {
        parts.push("Bowie Knife".to_string());
    }
    if equipment.claymores_owned {
        parts.push(format!("Claymores {}", equipment.claymores));
    }
    if equipment.monkey_bombs_owned {
        parts.push(format!("Monkey Bombs {}", equipment.monkey_bombs));
    }
    let line = parts.join("   ");
    if text.0 != line {
        text.0 = line;
    }
    let (pips, shown) = *pips;
    if shown.map_or(0, |children| children.len()) != equipment.frags as usize {
        commands.entity(pips).despawn_children().with_children(|pips| {
            for _ in 0..equipment.frags {
                pips.spawn((
                    Node {
                        width: px(10.0),
                        height: px(14.0),
                        ..default()
                    },
                    BackgroundColor(FRAG_GREEN),
                ));
            }
        });
    }
}

/// The focused Buyable's Prompt, always with its price, in red when the Player can't afford it. A
/// hold half offered beside the tap gets a line of its own.
fn prompt_text(
    focus: Res<Focus>,
    buyables: Query<(&Buyable, Option<&HoldBuy>)>,
    player: Single<&Modifiers, With<Player>>,
    points: Res<Points>,
    settings: Res<Settings>,
    prompt: Single<(&mut Text, &mut Visibility), With<PromptText>>,
    cost: Single<(&mut TextSpan, &mut TextColor), (With<PromptCost>, Without<PromptHoldCost>)>,
    mut hold_line: Single<&mut TextSpan, (With<PromptHold>, Without<PromptCost>, Without<PromptHoldCost>)>,
    hold_cost: Single<(&mut TextSpan, &mut TextColor), (With<PromptHoldCost>, Without<PromptCost>)>,
) {
    let (mut text, mut visibility) = prompt.into_inner();
    let Some((buyable, hold)) = focus.0.and_then(|e| buyables.get(e).ok()) else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let shown = with_use_key(&buyable.prompt, &settings.bindings);
    if text.0 != shown {
        text.0 = shown;
    }
    let (mut span, mut color) = cost.into_inner();
    set_price(&mut span, &mut color, charged(buyable.price, &player.0), points.balance);
    let (mut hold_span, mut hold_color) = hold_cost.into_inner();
    match hold.filter(|hold| hold.tap) {
        Some(hold) => {
            let line = format!("\n{}", with_use_key(&hold.prompt, &settings.bindings));
            if hold_line.0 != line {
                hold_line.0 = line;
            }
            set_price(&mut hold_span, &mut hold_color, charged(hold.price, &player.0), points.balance);
        }
        None => {
            hold_line.0.clear();
            hold_span.0.clear();
        }
    }
}

/// A Prompt's price, in red when the Player can't afford it.
fn set_price(span: &mut TextSpan, color: &mut TextColor, price: Option<u64>, balance: u64) {
    let text = price.map_or(String::new(), |price| format!(" [Cost: {}]", with_commas(price)));
    if span.0 != text {
        span.0 = text;
    }
    color.0 = if can_afford(balance, price) { Color::WHITE } else { CANT_AFFORD_RED };
}

fn reload_hint_text(
    player: Single<(&Loadout, &Modifiers), With<Player>>,
    hint: Single<(&mut Text, &mut Visibility), (With<ReloadHint>, Without<PromptText>)>,
) {
    let (loadout, modifiers) = *player;
    let (magazine, reserve) = loadout.gun_ammo();
    let (mut text, mut visibility) = hint.into_inner();
    let size = loadout.magazine_size(&modifiers.0);
    match reload_hint(magazine, size, reserve, loadout.reload.is_some()) {
        Some(hint) => {
            visibility.set_if_neq(Visibility::Inherited);
            if text.0 != hint {
                text.0 = hint.to_string();
            }
        }
        None => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Each timed Power-up's bar drains with its timer and blinks near the end.
fn power_up_bars(
    active: Res<ActivePowerUps>,
    mut bars: Query<(&PowerUpBar, &mut Node, &mut Visibility)>,
    mut fills: Query<(&PowerUpFill, &mut Node), Without<PowerUpBar>>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_secs();
    for (PowerUpBar(power_up), mut node, mut visibility) in &mut bars {
        let left = active.left(*power_up);
        let display = if left > 0.0 { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        visibility.set_if_neq(if bar_lit(left, elapsed) { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (PowerUpFill(power_up), mut node) in &mut fills {
        node.width = percent(bar_fill(active.left(*power_up)) * 100.0);
    }
}

fn banner_text(
    mut banner: ResMut<Banner>,
    mut taken: MessageReader<PowerUpTaken>,
    text: Single<(&mut Text, &mut TextColor, &mut Visibility), With<BannerText>>,
    time: Res<Time>,
) {
    banner.tick(time.delta_secs());
    for PowerUpTaken(power_up) in taken.read() {
        banner.taken(*power_up);
    }
    let (mut text, mut colour, mut visibility) = text.into_inner();
    let Some(power_up) = banner.showing() else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let name = power_up.name().to_uppercase();
    if text.0 != name {
        text.0 = name;
    }
    colour.set_if_neq(TextColor(power_up.colour()));
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

    #[test]
    fn rounds_one_to_five_are_tallies_then_numerals() {
        for round in 1..=5 {
            assert_eq!(round_counter(round), RoundCounter::Tallies(round));
        }
        assert_eq!(round_counter(6), RoundCounter::Numeral(6));
        assert_eq!(round_counter(42), RoundCounter::Numeral(42));
    }

    #[test]
    fn a_magazine_is_low_at_a_quarter_or_less() {
        assert!(!low_magazine(9, 32));
        assert!(low_magazine(8, 32));
        assert!(low_magazine(0, 32));
        assert!(!low_magazine(2, 6));
        assert!(low_magazine(1, 6));
        // A single-shot Weapon is only low once it's empty.
        assert!(!low_magazine(1, 1));
        assert!(low_magazine(0, 1));
    }

    #[test]
    fn the_reload_hint_needs_a_low_magazine_and_reserve_ammo() {
        assert_eq!(reload_hint(20, 32, 100, false), None);
        assert_eq!(reload_hint(8, 32, 100, false), Some("Reload"));
        assert_eq!(reload_hint(0, 32, 100, false), Some("Reload"));
        // Nothing to load.
        assert_eq!(reload_hint(5, 32, 0, false), None);
        assert_eq!(reload_hint(0, 32, 0, false), Some("No Ammo"));
    }

    #[test]
    fn no_reload_hint_during_a_reload() {
        assert_eq!(reload_hint(0, 32, 100, true), None);
    }

    #[test]
    fn ammo_shows_magazine_then_reserve() {
        assert_eq!(ammo_line(8, 80), "8 | 80");
    }

    #[test]
    fn the_detail_line_lists_what_is_on_the_weapon() {
        assert_eq!(detail_line([]), "");
        assert_eq!(detail_line(["Dead Wire"]), "Dead Wire");
        assert_eq!(detail_line(["Red Dot", "Dead Wire"]), "Red Dot  ·  Dead Wire");
    }

    #[test]
    fn perk_icons_show_initials() {
        assert_eq!(perk_initials(Perk::Juggernog), "J");
        assert_eq!(perk_initials(Perk::SpeedCola), "SC");
        assert_eq!(perk_initials(Perk::QuickRevive), "QR");
    }

    #[test]
    fn a_bar_drains_with_its_timer() {
        assert_eq!(bar_fill(POWER_UP_TIMED_SECS), 1.0);
        assert_eq!(bar_fill(POWER_UP_TIMED_SECS / 2.0), 0.5);
        assert_eq!(bar_fill(0.0), 0.0);
    }

    #[test]
    fn a_bar_blinks_only_in_its_last_five_seconds() {
        let step = POWER_UP_BLINK_INTERVAL_SECS;
        for i in 0..8 {
            assert!(bar_lit(POWER_UP_BLINK_SECS + 0.1, i as f32 * step + step / 2.0));
        }
        assert!(bar_lit(4.0, step / 2.0));
        assert!(!bar_lit(4.0, step * 1.5));
        assert!(bar_lit(4.0, step * 2.5));
    }

    #[test]
    fn spending_shows_as_a_drop_the_awards_do_not_explain() {
        assert_eq!(unexplained_change(1000, 1100, 100), 0);
        assert_eq!(unexplained_change(1000, 250, 0), -750);
        // A buy and a hit in the same frame.
        assert_eq!(unexplained_change(1000, 260, 10), -750);
        // A Teddy Bear's refund.
        assert_eq!(unexplained_change(0, 950, 0), 950);
    }

    #[test]
    fn instant_power_ups_get_a_two_second_banner() {
        let mut banner = Banner::default();
        banner.taken(PowerUp::DoublePoints);
        assert_eq!(banner.showing(), None);
        banner.taken(PowerUp::Nuke);
        assert_eq!(banner.showing(), Some(PowerUp::Nuke));
        banner.tick(1.9);
        assert_eq!(banner.showing(), Some(PowerUp::Nuke));
        banner.tick(0.2);
        assert_eq!(banner.showing(), None);
    }

    #[test]
    fn a_new_instant_power_up_replaces_the_banner() {
        let mut banner = Banner::default();
        banner.taken(PowerUp::MaxAmmo);
        banner.tick(1.5);
        banner.taken(PowerUp::Carpenter);
        banner.tick(1.5);
        assert_eq!(banner.showing(), Some(PowerUp::Carpenter));
    }
}
