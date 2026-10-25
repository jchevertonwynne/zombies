//! PROTOTYPE — throwaway Boon Offer screens for "Boon offer screen".
//! Lives on the `prototype/boon-offer-screen` branch only; never merge.
//!
//! Three structurally different Offer screens over the live HUD:
//!   A Cards   three tall cards centred, world dimmed. Hover + click, or 1/2/3, picks at once.
//!             Shows the total after the pick; held Boons as chips along the bottom.
//!   B Ledger  a list down the left, world visible on the right. W/S or hover selects,
//!             F / Space / click confirms. Detail panel shows now -> after, plus the full held list.
//!   C Strip   a compact lower-third strip, world undimmed. Hold 1/2/3 to pick; no mouse.
//!             Shows the per-Stack effect; held Boons live on the HUD's left edge during play too.
//!
//!   F6  cycle variant         F7  +5 random held Boons
//!   F8  open a Boon Offer     F9  open a Pact Offer
//! An Offer also opens 2.0 s after each Round's last kill (a Pact Offer after every 5th Round).
//! Inputs are ignored for the first 0.5 s of an Offer so a trigger finger can't pick by accident.
//! The Boons and Pacts below are placeholders, not the roster.

use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::round::{Round, RoundPhase};
use crate::state::{GameState, InRun, Pause};
use crate::tuning::INTERMISSION_SECS;

const OFFER_DELAY_SECS: f32 = 2.0;
const INPUT_LOCKOUT_SECS: f32 = 0.5;
const HOLD_SECS: f32 = 0.6;
const STACK_WEIGHT_FACTOR: f32 = 0.98;
const PACT_EVERY_ROUNDS: u32 = 5;
const OFFER_SIZE: usize = 3;

const VARIANTS: [&str; 3] = ["A Cards", "B Ledger", "C Strip"];

const PACT_RED: Color = Color::srgb(0.8, 0.1, 0.15);
const DRAWBACK_RED: Color = Color::srgb(1.0, 0.4, 0.4);
const GREY: Color = Color::srgb(0.65, 0.65, 0.65);
const GOOD_GREEN: Color = Color::srgb(0.45, 0.9, 0.45);

#[derive(Clone, Copy, PartialEq)]
enum Rarity {
    Common,
    Rare,
    Legendary,
}

impl Rarity {
    fn colour(self) -> Color {
        match self {
            Rarity::Common => Color::srgb(0.8, 0.8, 0.8),
            Rarity::Rare => Color::srgb(0.3, 0.6, 1.0),
            Rarity::Legendary => Color::srgb(1.0, 0.75, 0.15),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Rarity::Common => "COMMON",
            Rarity::Rare => "RARE",
            Rarity::Legendary => "LEGENDARY",
        }
    }

    fn weight(self) -> f32 {
        match self {
            Rarity::Common => 60.0,
            Rarity::Rare => 30.0,
            Rarity::Legendary => 10.0,
        }
    }
}

struct BoonDef {
    name: &'static str,
    rarity: Rarity,
    requirement: Option<&'static str>,
    /// The whole effect at `n` Stacks.
    total: fn(u32) -> String,
    /// What one Stack adds.
    per_stack: &'static str,
}

struct PactDef {
    name: &'static str,
    upside: &'static str,
    drawback: &'static str,
}

fn chance(p: f32, n: u32) -> String {
    format!("{:.0}%", 100.0 * (1.0 - (1.0 - p).powi(n as i32)))
}

const BOONS: &[BoonDef] = &[
    BoonDef {
        name: "Deep Pockets",
        rarity: Rarity::Common,
        requirement: None,
        total: |n| format!("+{}% Points from kills", 10 * n),
        per_stack: "+10% Points from kills",
    },
    BoonDef {
        name: "Quick Hands",
        rarity: Rarity::Common,
        requirement: None,
        total: |n| format!("Reloads {}% faster", 8 * n),
        per_stack: "Reloads 8% faster",
    },
    BoonDef {
        name: "Thick Skin",
        rarity: Rarity::Common,
        requirement: None,
        total: |n| format!("+{} max Health", 15 * n),
        per_stack: "+15 max Health",
    },
    BoonDef {
        name: "Spare Change",
        rarity: Rarity::Common,
        requirement: None,
        total: |n| format!("+{} Points at the start of each Round", 100 * n),
        per_stack: "+100 Points each Round",
    },
    BoonDef {
        name: "Steady Hands",
        rarity: Rarity::Common,
        requirement: None,
        total: |n| format!("Hip spread -{}%", 6 * n),
        per_stack: "Hip spread -6%",
    },
    BoonDef {
        name: "Hollow Points",
        rarity: Rarity::Rare,
        requirement: None,
        total: |n| format!("Headshots deal +{}% damage", 15 * n),
        per_stack: "Headshots +15% damage",
    },
    BoonDef {
        name: "Handyman",
        rarity: Rarity::Rare,
        requirement: None,
        total: |n| format!("Planks rebuild {}% faster", 20 * n),
        per_stack: "Planks rebuild 20% faster",
    },
    BoonDef {
        name: "Lucky Mag",
        rarity: Rarity::Rare,
        requirement: None,
        total: |n| format!("{} chance a kill refills your magazine", chance(0.05, n)),
        per_stack: "5% chance a kill refills the magazine (hyperbolic)",
    },
    BoonDef {
        name: "Bottomless",
        rarity: Rarity::Legendary,
        requirement: None,
        total: |n| format!("+{}% magazine size", 25 * n),
        per_stack: "+25% magazine size",
    },
    BoonDef {
        name: "Overclocked",
        rarity: Rarity::Legendary,
        requirement: Some("a Pack-a-Punched Weapon"),
        total: |n| format!("Pack-a-Punched Weapons fire {}% faster", 20 * n),
        per_stack: "Pack-a-Punched Weapons fire 20% faster",
    },
];

const PACTS: &[PactDef] = &[
    PactDef {
        name: "Blood Money",
        upside: "Kills pay double Points",
        drawback: "Zombies have +25% Health",
    },
    PactDef {
        name: "Glass Cannon",
        upside: "Your Weapons deal +75% damage",
        drawback: "-40 max Health",
    },
    PactDef {
        name: "Hoarder",
        upside: "Carry a third Weapon",
        drawback: "Everything costs 50% more",
    },
    PactDef {
        name: "Fleet Foot",
        upside: "Sprint forever, +20% move speed",
        drawback: "Every Zombie is a Sprinter",
    },
    PactDef {
        name: "House Edge",
        upside: "The Mystery Box costs 10 Points",
        drawback: "The Teddy Bear chance is doubled",
    },
];

#[derive(Resource)]
struct Variant(usize);

/// Boons and Pacts held this Run, in pick order.
#[derive(Resource, Default)]
struct Held {
    boons: Vec<(usize, u32)>,
    pacts: Vec<usize>,
}

impl Held {
    fn stacks(&self, boon: usize) -> u32 {
        self.boons.iter().find(|(b, _)| *b == boon).map_or(0, |(_, n)| *n)
    }

    fn add_boon(&mut self, boon: usize) {
        match self.boons.iter_mut().find(|(b, _)| *b == boon) {
            Some((_, n)) => *n += 1,
            None => self.boons.push((boon, 1)),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum OfferKind {
    Boon,
    Pact,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum Choice {
    Pick(usize),
    Skip,
}

#[derive(Resource)]
struct Offer {
    kind: OfferKind,
    choices: Vec<usize>,
    selected: Option<Choice>,
    /// Variant C: the choice being held and for how long.
    hold: Option<(Choice, f32)>,
    opened: f32,
    confirmed: Option<Choice>,
    dirty: bool,
}

#[derive(Resource, Default)]
struct Pending {
    timer: Option<f32>,
    offered_round: u32,
}

#[derive(Component)]
struct OfferRoot;

#[derive(Component)]
struct HoldBar(Choice);

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct HeldColumn;

pub struct BoonOfferPrototypePlugin;

impl Plugin for BoonOfferPrototypePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Variant(0))
            .add_systems(OnEnter(GameState::Playing), reset)
            .add_systems(
                Update,
                (
                    schedule_offer,
                    force_offer(OfferKind::Boon).run_if(input_just_pressed(KeyCode::F8)),
                    force_offer(OfferKind::Pact).run_if(input_just_pressed(KeyCode::F9)),
                )
                    .run_if(in_state(Pause::Running)),
            )
            .add_systems(
                Update,
                (
                    next_variant.run_if(input_just_pressed(KeyCode::F6)),
                    add_held.run_if(input_just_pressed(KeyCode::F7)),
                    status_text,
                    held_column,
                )
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                Update,
                (hover, offer_input, commit, rebuild, hold_bars)
                    .chain()
                    .run_if(in_state(Pause::Offer)),
            );
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

fn reset(mut commands: Commands) {
    commands.insert_resource(Held::default());
    commands.insert_resource(Pending::default());
    commands.spawn((
        InRun,
        StatusText,
        text("", 15.0, Color::srgb(0.6, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            left: px(12.0),
            top: px(8.0),
            ..default()
        },
        GlobalZIndex(30),
    ));
    commands.spawn((
        InRun,
        HeldColumn,
        Node {
            position_type: PositionType::Absolute,
            left: px(16.0),
            top: px(64.0),
            flex_direction: FlexDirection::Column,
            row_gap: px(2.0),
            ..default()
        },
    ));
}

fn draw_boons(held: &Held) -> Vec<usize> {
    let mut pool: Vec<(usize, f32)> = BOONS
        .iter()
        .enumerate()
        .map(|(i, b)| (i, b.rarity.weight() * STACK_WEIGHT_FACTOR.powi(held.stacks(i) as i32)))
        .collect();
    let mut out = Vec::new();
    while out.len() < OFFER_SIZE && !pool.is_empty() {
        let total: f32 = pool.iter().map(|(_, w)| w).sum();
        let mut roll = rand::random_range(0.0..total);
        let k = pool
            .iter()
            .position(|(_, w)| {
                roll -= w;
                roll < 0.0
            })
            .unwrap_or(pool.len() - 1);
        out.push(pool.remove(k).0);
    }
    out
}

fn draw_pacts(held: &Held) -> Vec<usize> {
    let mut pool: Vec<usize> = (0..PACTS.len()).filter(|p| !held.pacts.contains(p)).collect();
    let mut out = Vec::new();
    while out.len() < OFFER_SIZE && !pool.is_empty() {
        out.push(pool.remove(rand::random_range(0..pool.len())));
    }
    out
}

fn open_offer(
    commands: &mut Commands,
    next: &mut NextState<Pause>,
    held: &Held,
    variant: &Variant,
    kind: OfferKind,
    now: f32,
) {
    let (kind, choices) = match kind {
        OfferKind::Pact if held.pacts.len() < PACTS.len() => (OfferKind::Pact, draw_pacts(held)),
        _ => (OfferKind::Boon, draw_boons(held)),
    };
    commands.insert_resource(Offer {
        kind,
        choices,
        selected: (variant.0 == 1).then_some(Choice::Pick(0)),
        hold: None,
        opened: now,
        confirmed: None,
        dirty: true,
    });
    next.set(Pause::Offer);
}

fn schedule_offer(
    mut commands: Commands,
    round: Option<Res<Round>>,
    mut pending: ResMut<Pending>,
    held: Res<Held>,
    variant: Res<Variant>,
    time: Res<Time>,
    real: Res<Time<Real>>,
    mut next: ResMut<NextState<Pause>>,
) {
    let Some(round) = round else { return };
    if !matches!(round.phase, RoundPhase::Intermission(_)) || pending.offered_round == round.number {
        return;
    }
    let left = pending.timer.get_or_insert(OFFER_DELAY_SECS);
    *left -= time.delta_secs();
    if *left > 0.0 {
        return;
    }
    pending.timer = None;
    pending.offered_round = round.number;
    let kind = if round.number % PACT_EVERY_ROUNDS == 0 {
        OfferKind::Pact
    } else {
        OfferKind::Boon
    };
    open_offer(&mut commands, &mut next, &held, &variant, kind, real.elapsed_secs());
}

fn force_offer(
    kind: OfferKind,
) -> impl FnMut(Commands, Res<Held>, Res<Variant>, Res<Time<Real>>, ResMut<NextState<Pause>>) {
    move |mut commands, held, variant, real, mut next| {
        open_offer(&mut commands, &mut next, &held, &variant, kind, real.elapsed_secs());
    }
}

fn next_variant(mut variant: ResMut<Variant>, offer: Option<ResMut<Offer>>) {
    variant.0 = (variant.0 + 1) % VARIANTS.len();
    if let Some(mut offer) = offer {
        offer.dirty = true;
        offer.hold = None;
        if variant.0 == 1 && offer.selected.is_none() {
            offer.selected = Some(Choice::Pick(0));
        }
    }
}

fn add_held(mut held: ResMut<Held>, offer: Option<ResMut<Offer>>) {
    for _ in 0..5 {
        held.add_boon(rand::random_range(0..BOONS.len()));
    }
    if let Some(mut offer) = offer {
        offer.dirty = true;
    }
}

fn status_text(
    variant: Res<Variant>,
    held: Res<Held>,
    round: Option<Res<Round>>,
    mut text: Single<&mut Text, With<StatusText>>,
) {
    let stacks: u32 = held.boons.iter().map(|(_, n)| n).sum();
    text.0 = format!(
        "PROTOTYPE Boon Offer screen | variant {} | F6 variant  F7 +5 held  F8 Boon Offer  F9 Pact Offer | Round {} | held {} Boons ({} Stacks), {} Pacts",
        VARIANTS[variant.0],
        round.map_or(0, |r| r.number),
        held.boons.len(),
        stacks,
        held.pacts.len(),
    );
}

/// Variant C keeps the held Boons on the HUD all the time.
fn held_column(
    mut commands: Commands,
    variant: Res<Variant>,
    held: Res<Held>,
    column: Single<Entity, With<HeldColumn>>,
) {
    if !variant.is_changed() && !held.is_changed() {
        return;
    }
    commands.entity(*column).despawn_children();
    if variant.0 != 2 {
        return;
    }
    commands.entity(*column).with_children(|column| {
        for &(boon, n) in &held.boons {
            let def = &BOONS[boon];
            column.spawn(text(format!("{} {}", def.name, n), 16.0, def.rarity.colour()));
        }
        for &pact in &held.pacts {
            let def = &PACTS[pact];
            column.spawn(text(def.name, 16.0, PACT_RED));
            column.spawn(text(format!("  ! {}", def.drawback), 14.0, DRAWBACK_RED));
        }
    });
}

fn hover(
    window: Single<&Window, With<PrimaryWindow>>,
    nodes: Query<(&Choice, &ComputedNode, &UiGlobalTransform)>,
    offer: Option<ResMut<Offer>>,
    variant: Res<Variant>,
    mut last_cursor: Local<Option<Vec2>>,
) {
    let Some(mut offer) = offer else { return };
    if variant.0 == 2 {
        return;
    }
    let Some(cursor) = window.physical_cursor_position() else { return };
    // Only a moving mouse takes the selection, so it doesn't fight W/S in the Ledger.
    if *last_cursor == Some(cursor) && !offer.dirty {
        return;
    }
    *last_cursor = Some(cursor);
    let over = nodes
        .iter()
        .find(|(_, node, transform)| node.contains_point(**transform, cursor))
        .map(|(choice, ..)| *choice);
    let selected = if variant.0 == 0 { over } else { over.or(offer.selected) };
    if selected != offer.selected {
        offer.selected = selected;
        offer.dirty = true;
    }
}

fn offer_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    real: Res<Time<Real>>,
    variant: Res<Variant>,
    window: Single<&Window, With<PrimaryWindow>>,
    nodes: Query<(&Choice, &ComputedNode, &UiGlobalTransform)>,
    offer: Option<ResMut<Offer>>,
) {
    let Some(mut offer) = offer else { return };
    if real.elapsed_secs() - offer.opened < INPUT_LOCKOUT_SECS {
        return;
    }
    let count = offer.choices.len();
    let pact = offer.kind == OfferKind::Pact;
    let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3];
    let under_cursor = || {
        let cursor = window.physical_cursor_position()?;
        nodes
            .iter()
            .find(|(_, node, transform)| node.contains_point(**transform, cursor))
            .map(|(choice, ..)| *choice)
    };

    match variant.0 {
        0 => {
            for (i, key) in digits.iter().enumerate().take(count) {
                if keys.just_pressed(*key) {
                    offer.confirmed = Some(Choice::Pick(i));
                }
            }
            if pact && keys.just_pressed(KeyCode::KeyX) {
                offer.confirmed = Some(Choice::Skip);
            }
            if mouse.just_pressed(MouseButton::Left)
                && let Some(choice) = under_cursor()
            {
                offer.confirmed = Some(choice);
            }
        }
        1 => {
            // Rows top to bottom: the choices, then Skip on a Pact Offer.
            let rows = count + pact as usize;
            let row = match offer.selected {
                Some(Choice::Pick(i)) => i,
                Some(Choice::Skip) => count,
                None => 0,
            };
            let up = keys.any_just_pressed([KeyCode::KeyW, KeyCode::ArrowUp]);
            let down = keys.any_just_pressed([KeyCode::KeyS, KeyCode::ArrowDown]);
            if up || down {
                let row = if up { (row + rows - 1) % rows } else { (row + 1) % rows };
                offer.selected = Some(if row == count { Choice::Skip } else { Choice::Pick(row) });
                offer.dirty = true;
            }
            if keys.any_just_pressed([KeyCode::KeyF, KeyCode::Space, KeyCode::Enter]) {
                offer.confirmed = offer.selected;
            }
            if mouse.just_pressed(MouseButton::Left)
                && let Some(choice) = under_cursor()
            {
                offer.confirmed = Some(choice);
            }
        }
        _ => {
            let held_key = digits
                .iter()
                .take(count)
                .position(|key| keys.pressed(*key))
                .map(Choice::Pick)
                .or((pact && keys.pressed(KeyCode::KeyX)).then_some(Choice::Skip));
            offer.hold = match (held_key, offer.hold) {
                (Some(choice), Some((held, secs))) if held == choice => {
                    Some((choice, secs + real.delta_secs()))
                }
                (Some(choice), _) => Some((choice, 0.0)),
                (None, _) => None,
            };
            if let Some((choice, secs)) = offer.hold
                && secs >= HOLD_SECS
            {
                offer.confirmed = Some(choice);
            }
        }
    }
}

fn commit(
    mut commands: Commands,
    offer: Option<Res<Offer>>,
    mut held: ResMut<Held>,
    round: Option<ResMut<Round>>,
    mut next: ResMut<NextState<Pause>>,
) {
    let Some(offer) = offer else { return };
    let Some(choice) = offer.confirmed else { return };
    if let Choice::Pick(k) = choice {
        let picked = offer.choices[k];
        match offer.kind {
            OfferKind::Boon => held.add_boon(picked),
            OfferKind::Pact => held.pacts.push(picked),
        }
    }
    // The Intermission starts after the pick.
    if let Some(mut round) = round
        && matches!(round.phase, RoundPhase::Intermission(_))
    {
        round.phase = RoundPhase::Intermission(INTERMISSION_SECS);
    }
    commands.remove_resource::<Offer>();
    next.set(Pause::Running);
}

fn rebuild(
    mut commands: Commands,
    offer: Option<ResMut<Offer>>,
    roots: Query<Entity, With<OfferRoot>>,
    variant: Res<Variant>,
    held: Res<Held>,
    round: Option<Res<Round>>,
) {
    let Some(mut offer) = offer else { return };
    if !offer.dirty {
        return;
    }
    offer.dirty = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    let round = round.map_or(0, |r| r.number);
    let root = match variant.0 {
        0 => cards(&mut commands, &offer, &held, round),
        1 => ledger(&mut commands, &offer, &held, round),
        _ => strip(&mut commands, &offer, round),
    };
    commands
        .entity(root)
        .insert((OfferRoot, DespawnOnExit(Pause::Offer), GlobalZIndex(20)));
}

fn hold_bars(offer: Option<Res<Offer>>, mut bars: Query<(&HoldBar, &mut Node)>) {
    let Some(offer) = offer else { return };
    for (bar, mut node) in &mut bars {
        let progress = match offer.hold {
            Some((choice, secs)) if choice == bar.0 => (secs / HOLD_SECS).min(1.0),
            _ => 0.0,
        };
        node.width = percent(progress * 100.0);
    }
}

fn title(kind: OfferKind) -> &'static str {
    match kind {
        OfferKind::Boon => "CHOOSE A BOON",
        OfferKind::Pact => "A PACT IS OFFERED",
    }
}

fn stack_line(stacks: u32) -> (String, Color) {
    if stacks == 0 {
        ("NEW".into(), GOOD_GREEN)
    } else {
        (format!("HELD x{stacks} -> x{}", stacks + 1), Color::WHITE)
    }
}

fn panel(selected: bool) -> Color {
    if selected {
        Color::srgba(0.22, 0.22, 0.26, 0.97)
    } else {
        Color::srgba(0.07, 0.07, 0.09, 0.95)
    }
}

// ---------------------------------------------------------------- A: Cards

fn cards(commands: &mut Commands, offer: &Offer, held: &Held, round: u32) -> Entity {
    let pact = offer.kind == OfferKind::Pact;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                height: percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(24.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
        ))
        .with_children(|root| {
            root.spawn(text(format!("ROUND {round} COMPLETE"), 22.0, GREY));
            root.spawn(text(title(offer.kind), 52.0, if pact { PACT_RED } else { Color::WHITE }));
            root.spawn(Node {
                column_gap: px(28.0),
                align_items: AlignItems::FlexEnd,
                ..default()
            })
            .with_children(|row| {
                for (k, &choice) in offer.choices.iter().enumerate() {
                    let selected = offer.selected == Some(Choice::Pick(k));
                    let accent = if pact { PACT_RED } else { BOONS[choice].rarity.colour() };
                    row.spawn((
                        Choice::Pick(k),
                        Node {
                            width: px(280.0),
                            height: px(370.0),
                            flex_direction: FlexDirection::Column,
                            border: UiRect::all(px(if selected { 6.0 } else { 3.0 })),
                            margin: UiRect::bottom(px(if selected { 16.0 } else { 0.0 })),
                            ..default()
                        },
                        BorderColor::all(accent),
                        BackgroundColor(panel(selected)),
                    ))
                    .with_children(|card| {
                        let header = if pact { "PACT" } else { BOONS[choice].rarity.label() };
                        card.spawn((
                            Node {
                                justify_content: JustifyContent::SpaceBetween,
                                padding: UiRect::axes(px(12.0), px(6.0)),
                                ..default()
                            },
                            BackgroundColor(accent),
                        ))
                        .with_children(|strip| {
                            strip.spawn(text(header, 18.0, Color::BLACK));
                            strip.spawn(text(format!("[{}]", k + 1), 18.0, Color::BLACK));
                        });
                        card.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(px(16.0)),
                            row_gap: px(12.0),
                            ..default()
                        })
                        .with_children(|body| {
                            if pact {
                                let def = &PACTS[choice];
                                body.spawn(text(def.name, 30.0, Color::WHITE));
                                body.spawn(text("UPSIDE", 14.0, GREY));
                                body.spawn(text(def.upside, 20.0, Rarity::Legendary.colour()));
                                body.spawn(text("DRAWBACK", 14.0, GREY));
                                body.spawn(text(def.drawback, 20.0, DRAWBACK_RED));
                                body.spawn(text("Both last the whole Run.", 14.0, GREY));
                            } else {
                                let def = &BOONS[choice];
                                let stacks = held.stacks(choice);
                                let (line, colour) = stack_line(stacks);
                                body.spawn(text(def.name, 30.0, Color::WHITE));
                                body.spawn(text(line, 16.0, colour));
                                body.spawn(text("AFTER THIS PICK", 14.0, GREY));
                                body.spawn(text((def.total)(stacks + 1), 22.0, Color::WHITE));
                                if let Some(req) = def.requirement {
                                    body.spawn(text(format!("Needs {req} (met)"), 14.0, GOOD_GREEN));
                                }
                            }
                        });
                    });
                }
            });
            if pact {
                let selected = offer.selected == Some(Choice::Skip);
                root.spawn((
                    Choice::Skip,
                    Node {
                        padding: UiRect::axes(px(24.0), px(10.0)),
                        border: UiRect::all(px(2.0)),
                        ..default()
                    },
                    BorderColor::all(GREY),
                    BackgroundColor(panel(selected)),
                ))
                .with_child(text("[X] Skip - take nothing", 20.0, Color::WHITE));
            }
            root.spawn(text(
                if pact { "Click or press 1-3 to take a Pact, X to skip" } else { "Click or press 1-3 to pick" },
                18.0,
                GREY,
            ));
            // Held Boons as chips along the bottom.
            root.spawn(Node {
                position_type: PositionType::Absolute,
                bottom: px(150.0),
                width: percent(70.0),
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                column_gap: px(8.0),
                row_gap: px(6.0),
                ..default()
            })
            .with_children(|chips| {
                let mut chip = |label: String, colour: Color| {
                    chips
                        .spawn((
                            Node {
                                padding: UiRect::axes(px(8.0), px(3.0)),
                                border: UiRect::all(px(2.0)),
                                ..default()
                            },
                            BorderColor::all(colour),
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                        ))
                        .with_child(text(label, 15.0, colour));
                };
                for &(boon, n) in &held.boons {
                    chip(format!("{} x{n}", BOONS[boon].name), BOONS[boon].rarity.colour());
                }
                for &p in &held.pacts {
                    chip(PACTS[p].name.to_string(), PACT_RED);
                }
            });
        })
        .id()
}

// ---------------------------------------------------------------- B: Ledger

fn ledger(commands: &mut Commands, offer: &Offer, held: &Held, round: u32) -> Entity {
    let pact = offer.kind == OfferKind::Pact;
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            ..default()
        })
        .with_children(|root| {
            // Left: the list.
            root.spawn((
                Node {
                    width: percent(42.0),
                    height: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    padding: UiRect::left(px(56.0)),
                    row_gap: px(10.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.82)),
            ))
            .with_children(|list| {
                list.spawn(text(format!("ROUND {round} COMPLETE"), 20.0, GREY));
                list.spawn((
                    text(title(offer.kind), 44.0, if pact { PACT_RED } else { Color::WHITE }),
                    Node {
                        margin: UiRect::bottom(px(16.0)),
                        ..default()
                    },
                ));
                for (k, &choice) in offer.choices.iter().enumerate() {
                    let selected = offer.selected == Some(Choice::Pick(k));
                    let accent = if pact { PACT_RED } else { BOONS[choice].rarity.colour() };
                    list.spawn((
                        Choice::Pick(k),
                        Node {
                            width: percent(92.0),
                            min_height: px(88.0),
                            border: UiRect::left(px(8.0)),
                            padding: UiRect::axes(px(16.0), px(10.0)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(4.0),
                            margin: UiRect::left(px(if selected { 24.0 } else { 0.0 })),
                            ..default()
                        },
                        BorderColor::all(accent),
                        BackgroundColor(panel(selected)),
                    ))
                    .with_children(|row| {
                        let marker = if selected { "> " } else { "" };
                        if pact {
                            let def = &PACTS[choice];
                            row.spawn(text(format!("{marker}{}", def.name), 28.0, Color::WHITE));
                            row.spawn(text(format!("+ {}", def.upside), 18.0, Rarity::Legendary.colour()));
                            row.spawn(text(format!("- {}", def.drawback), 18.0, DRAWBACK_RED));
                        } else {
                            let def = &BOONS[choice];
                            let stacks = held.stacks(choice);
                            row.spawn(Node {
                                column_gap: px(14.0),
                                align_items: AlignItems::Baseline,
                                ..default()
                            })
                            .with_children(|line| {
                                line.spawn(text(format!("{marker}{}", def.name), 28.0, Color::WHITE));
                                line.spawn(text(def.rarity.label(), 14.0, accent));
                                let (stack, colour) = stack_line(stacks);
                                line.spawn(text(stack, 14.0, colour));
                            });
                            row.spawn(text(def.per_stack, 17.0, GREY));
                        }
                    });
                }
                if pact {
                    let selected = offer.selected == Some(Choice::Skip);
                    list.spawn((
                        Choice::Skip,
                        Node {
                            width: percent(92.0),
                            border: UiRect::left(px(8.0)),
                            padding: UiRect::axes(px(16.0), px(10.0)),
                            margin: UiRect::left(px(if selected { 24.0 } else { 0.0 })),
                            ..default()
                        },
                        BorderColor::all(GREY),
                        BackgroundColor(panel(selected)),
                    ))
                    .with_child(text(
                        format!("{}Skip - take nothing", if selected { "> " } else { "" }),
                        22.0,
                        Color::WHITE,
                    ));
                }
                list.spawn((
                    text("W/S or mouse to choose - F, Space or click to take", 16.0, GREY),
                    Node {
                        margin: UiRect::top(px(16.0)),
                        ..default()
                    },
                ));
            });

            // Right: detail for the selection, then everything held.
            root.spawn((
                Node {
                    width: percent(28.0),
                    height: percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::axes(px(28.0), px(48.0)),
                    row_gap: px(8.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            ))
            .with_children(|side| {
                match offer.selected {
                    Some(Choice::Pick(k)) if pact => {
                        let def = &PACTS[offer.choices[k]];
                        side.spawn(text(def.name, 30.0, PACT_RED));
                        side.spawn(text(format!("Upside: {}", def.upside), 18.0, Color::WHITE));
                        side.spawn(text(format!("Drawback: {}", def.drawback), 18.0, DRAWBACK_RED));
                        side.spawn(text("Held once; the Drawback lasts the Run.", 15.0, GREY));
                    }
                    Some(Choice::Pick(k)) => {
                        let boon = offer.choices[k];
                        let def = &BOONS[boon];
                        let stacks = held.stacks(boon);
                        side.spawn(text(def.name, 30.0, def.rarity.colour()));
                        if let Some(req) = def.requirement {
                            side.spawn(text(format!("Needs {req} (met)"), 15.0, GOOD_GREEN));
                        }
                        let now = if stacks == 0 { "nothing".into() } else { (def.total)(stacks) };
                        side.spawn(text(format!("Now: {now}"), 18.0, GREY));
                        side.spawn(text(format!("After: {}", (def.total)(stacks + 1)), 18.0, Color::WHITE));
                    }
                    Some(Choice::Skip) => {
                        side.spawn(text("Skip", 30.0, Color::WHITE));
                        side.spawn(text("Nothing this Round.", 18.0, GREY));
                    }
                    None => {}
                }
                side.spawn((
                    text(format!("HELD BOONS ({})", held.boons.len()), 16.0, GREY),
                    Node {
                        margin: UiRect::top(px(28.0)),
                        ..default()
                    },
                ));
                for &(boon, n) in &held.boons {
                    let def = &BOONS[boon];
                    side.spawn(text(format!("{} x{n}  {}", def.name, (def.total)(n)), 15.0, def.rarity.colour()));
                }
                if !held.pacts.is_empty() {
                    side.spawn((
                        text("PACTS", 16.0, GREY),
                        Node {
                            margin: UiRect::top(px(16.0)),
                            ..default()
                        },
                    ));
                    for &p in &held.pacts {
                        side.spawn(text(PACTS[p].name, 15.0, PACT_RED));
                        side.spawn(text(format!("  - {}", PACTS[p].drawback), 14.0, DRAWBACK_RED));
                    }
                }
            });
        })
        .id()
}

// ---------------------------------------------------------------- C: Strip

fn strip(commands: &mut Commands, offer: &Offer, round: u32) -> Entity {
    let pact = offer.kind == OfferKind::Pact;
    let panel_box = |row: &mut ChildSpawnerCommands, choice: Choice, accent: Color, lines: Vec<(String, f32, Color)>| {
        row.spawn((
            choice,
            Node {
                width: px(300.0),
                min_height: px(118.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                border: UiRect::top(px(4.0)),
                ..default()
            },
            BorderColor::all(accent),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(10.0)),
                    row_gap: px(4.0),
                    ..default()
                })
                .with_children(|body| {
                    for (line, size, colour) in lines {
                        body.spawn(text(line, size, colour));
                    }
                });
            panel.spawn((
                HoldBar(choice),
                Node {
                    height: px(6.0),
                    width: percent(0.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ));
        });
    };
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            bottom: px(150.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8.0),
            ..default()
        })
        .with_children(|root| {
            let hint = if pact {
                format!("ROUND {round} - A PACT IS OFFERED - HOLD 1-3 TO TAKE, HOLD X TO SKIP")
            } else {
                format!("ROUND {round} - HOLD 1-3 TO TAKE A BOON")
            };
            root.spawn(text(hint, 20.0, if pact { PACT_RED } else { Color::WHITE }));
            root.spawn(Node {
                column_gap: px(12.0),
                ..default()
            })
            .with_children(|row| {
                for (k, &choice) in offer.choices.iter().enumerate() {
                    if pact {
                        let def = &PACTS[choice];
                        panel_box(
                            row,
                            Choice::Pick(k),
                            PACT_RED,
                            vec![
                                (format!("[{}] {}", k + 1, def.name), 22.0, Color::WHITE),
                                (format!("+ {}", def.upside), 16.0, Rarity::Legendary.colour()),
                                (format!("- {}", def.drawback), 16.0, DRAWBACK_RED),
                            ],
                        );
                    } else {
                        let def = &BOONS[choice];
                        panel_box(
                            row,
                            Choice::Pick(k),
                            def.rarity.colour(),
                            vec![
                                (format!("[{}] {}", k + 1, def.name), 22.0, Color::WHITE),
                                (def.rarity.label().into(), 13.0, def.rarity.colour()),
                                (format!("{} per Stack", def.per_stack), 16.0, GREY),
                            ],
                        );
                    }
                }
                if pact {
                    panel_box(row, Choice::Skip, GREY, vec![("[X] Skip".into(), 22.0, Color::WHITE)]);
                }
            });
        })
        .id()
}
