//! PROTOTYPE — throwaway HUD layouts for "HUD layout and feedback".
//! Lives on the `prototype/hud-layout` branch only; never merge.
//!
//! Three structurally different HUDs over the live game (the real HUD keeps only the
//! crosshair, damage tint and the pause / Game Over screens on this branch):
//!   A Faithful  BO1 corners: Round tallies bottom-left with Perks above, Points and ammo
//!               bottom-right, Power-up icons bottom-centre, prompt under the crosshair.
//!               No timers, no Box / Power state. Held Boons as the full left-edge column.
//!   B Panel     Everything always visible: a top-left status panel (Round, Points, Power,
//!               Box, PaP), Power-up timer bars top-centre, a weapon card on the right,
//!               named Perks bottom-centre. Held Boons condensed to Rarity counts; Tab expands.
//!   C Minimal   Almost nothing persistent: a small Round number. Points, ammo and Perks
//!               show only on change, when low, or while Tab is held. Power-ups as a
//!               countdown under the crosshair. Held Boons on Tab only.
//!
//!   F6  cycle variant       F5  Round +1 (Shift: +10)    F7  buy the next Perk
//!   F8  next Power-up       F9  next Weapon              F10 next interaction prompt
//!   F11 +5 random Boons (every 2nd press also a Pact)    F3  toggle Power   F2 move the Box
//!   Tab hold for detail (B, C)  G   throw a grenade (count only)
//! Round, Points, Points earned and ammo counts are live; everything else is mock state.
//! The whole HUD is rebuilt every frame: prototype only.

use bevy::prelude::*;

use crate::points::{Points, PointsEarned};
use crate::round::{Round, RoundPhase};
use crate::state::{GameState, InRun};
use crate::weapon::{ShotFired, Weapon};

const VARIANTS: [&str; 3] = ["A Faithful", "B Panel", "C Minimal"];

const ROUND_RED: Color = Color::srgb(0.7, 0.05, 0.02);
const POINTS_GOLD: Color = Color::srgb(1.0, 0.9, 0.5);
const EARN_GOLD: Color = Color::srgb(1.0, 0.85, 0.2);
const SPEND_RED: Color = Color::srgb(1.0, 0.3, 0.25);
const GREY: Color = Color::srgb(0.65, 0.65, 0.65);
const PAP_PURPLE: Color = Color::srgb(0.75, 0.45, 1.0);
const PACT_RED: Color = Color::srgb(0.8, 0.1, 0.15);
const DRAWBACK_RED: Color = Color::srgb(1.0, 0.4, 0.4);
const PANEL: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);

const POWER_UP_SECS: f32 = 30.0;
const POWER_UP_BLINK_SECS: f32 = 5.0;
const POPUP_SECS: f32 = 1.2;
const SHOW_ON_CHANGE_SECS: f32 = 3.0;
const MAX_PERKS: usize = 4;

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
}

const BOONS: &[(&str, Rarity)] = &[
    ("Blood Money", Rarity::Common),
    ("Hollow Points", Rarity::Common),
    ("Marksman's Eye", Rarity::Common),
    ("Full Belt", Rarity::Common),
    ("Light Feet", Rarity::Common),
    ("Second Wind", Rarity::Common),
    ("Long Fuse", Rarity::Common),
    ("Union Rates", Rarity::Common),
    ("Trench Raider", Rarity::Common),
    ("Bandolier", Rarity::Common),
    ("War Bonds", Rarity::Rare),
    ("Loaded Dice", Rarity::Rare),
    ("Barbed Wire", Rarity::Rare),
    ("Shrapnel", Rarity::Rare),
    ("Iron Stomach", Rarity::Rare),
    ("Third Hand", Rarity::Legendary),
    ("Lucky Strike", Rarity::Legendary),
    ("Black Market", Rarity::Legendary),
];

const PACTS: &[(&str, &str)] = &[
    ("Glass Jaw", "max Health -40"),
    ("Gold Rush", "every price x1.25"),
    ("Scorched Earth", "Zombies move +20% faster"),
    ("Cold Steel", "Reserve Ammo -50%"),
];

struct Perk {
    name: &'static str,
    short: &'static str,
    cost: u32,
    colour: Color,
}

const PERKS: &[Perk] = &[
    Perk { name: "Juggernog", short: "JG", cost: 2500, colour: Color::srgb(0.85, 0.1, 0.1) },
    Perk { name: "Speed Cola", short: "SC", cost: 3000, colour: Color::srgb(0.1, 0.75, 0.2) },
    Perk { name: "Double Tap", short: "DT", cost: 2000, colour: Color::srgb(0.95, 0.65, 0.1) },
    Perk { name: "Quick Revive", short: "QR", cost: 500, colour: Color::srgb(0.2, 0.7, 0.95) },
];

#[derive(Clone, Copy, PartialEq)]
enum PowerUp {
    InstaKill,
    DoublePoints,
    FireSale,
    MaxAmmo,
    Nuke,
    Carpenter,
}

impl PowerUp {
    const CYCLE: [PowerUp; 6] = [
        PowerUp::InstaKill,
        PowerUp::DoublePoints,
        PowerUp::MaxAmmo,
        PowerUp::FireSale,
        PowerUp::Nuke,
        PowerUp::Carpenter,
    ];

    fn name(self) -> &'static str {
        match self {
            PowerUp::InstaKill => "Insta-Kill",
            PowerUp::DoublePoints => "Double Points",
            PowerUp::FireSale => "Fire Sale",
            PowerUp::MaxAmmo => "Max Ammo",
            PowerUp::Nuke => "Kaboom!",
            PowerUp::Carpenter => "Carpenter",
        }
    }

    fn short(self) -> &'static str {
        match self {
            PowerUp::InstaKill => "IK",
            PowerUp::DoublePoints => "x2",
            PowerUp::FireSale => "$",
            _ => "",
        }
    }

    fn timed(self) -> bool {
        matches!(self, PowerUp::InstaKill | PowerUp::DoublePoints | PowerUp::FireSale)
    }

    fn colour(self) -> Color {
        match self {
            PowerUp::InstaKill => Color::srgb(0.9, 0.9, 0.9),
            PowerUp::DoublePoints => Color::srgb(0.3, 0.85, 0.3),
            PowerUp::FireSale => Color::srgb(1.0, 0.5, 0.1),
            _ => Color::WHITE,
        }
    }
}

struct MockWeapon {
    name: &'static str,
    upgraded: bool,
    attachments: &'static [&'static str],
    ammo_mod: Option<&'static str>,
}

const WEAPONS: &[MockWeapon] = &[
    MockWeapon { name: "M1911", upgraded: false, attachments: &[], ammo_mod: None },
    MockWeapon { name: "Commando", upgraded: false, attachments: &["Red Dot", "Extended Mag"], ammo_mod: None },
    MockWeapon { name: "Spectre", upgraded: false, attachments: &["Rapid Fire"], ammo_mod: None },
    MockWeapon { name: "Mustang & Sally", upgraded: true, attachments: &[], ammo_mod: Some("Blast Furnace") },
    MockWeapon { name: "Predator", upgraded: true, attachments: &["Red Dot", "Extended Mag"], ammo_mod: Some("Dead Wire") },
    MockWeapon { name: "Ray Gun", upgraded: false, attachments: &[], ammo_mod: None },
    MockWeapon { name: "Porter's X2 Ray Gun", upgraded: true, attachments: &[], ammo_mod: Some("Thunder Wall") },
];

struct Prompt {
    /// What the player is offered, e.g. "buy Door".
    action: &'static str,
    cost: Option<u32>,
    /// A second purchase on the same prompt (Wall Weapon ammo).
    extra: Option<(&'static str, u32)>,
    /// Shown instead of an action when it can't be used yet.
    blocked: Option<&'static str>,
}

const PROMPTS: &[Prompt] = &[
    Prompt { action: "open Door", cost: Some(750), extra: None, blocked: None },
    Prompt { action: "buy MP40", cost: Some(1000), extra: Some(("Ammo", 500)), blocked: None },
    Prompt { action: "buy Random Weapon", cost: Some(950), extra: None, blocked: None },
    Prompt { action: "buy Juggernog", cost: Some(2500), extra: None, blocked: None },
    Prompt { action: "buy Juggernog", cost: None, extra: None, blocked: Some("You must turn on the Power first!") },
    Prompt { action: "Pack-a-Punch", cost: Some(5000), extra: None, blocked: None },
    Prompt { action: "activate Trap", cost: Some(1000), extra: None, blocked: None },
    Prompt { action: "use the Teleporter", cost: Some(1500), extra: None, blocked: None },
    Prompt { action: "rebuild Barrier", cost: None, extra: None, blocked: None },
];

const BOX_SPOTS: &[&str] = &["Start Room", "West Hall", "Dining", "Yard", "Power Room"];

struct Popup {
    text: String,
    colour: Color,
    age: f32,
}

#[derive(Resource, Default)]
struct Mock {
    variant: usize,
    round_offset: u32,
    spent: u32,
    perks: Vec<usize>,
    next_perk: usize,
    /// Timed Power-ups and their seconds left.
    power_ups: Vec<(PowerUp, f32)>,
    next_power_up: usize,
    /// An instant Power-up's name, shown briefly.
    announce: Option<(PowerUp, f32)>,
    weapon: usize,
    /// `None` means no prompt; otherwise an index into PROMPTS.
    prompt: Option<usize>,
    boons: Vec<(usize, u32)>,
    pacts: Vec<usize>,
    boon_presses: u32,
    power_on: bool,
    box_spot: usize,
    popups: Vec<Popup>,
    last_points: u32,
    since_points_change: f32,
    since_shot: f32,
    since_perk_bought: f32,
    lethals: u32,
}

#[derive(Component)]
struct ProtoRoot;

pub struct HudPrototypePlugin;

impl Plugin for HudPrototypePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Mock {
            lethals: 2,
            since_points_change: 99.0,
            since_shot: 99.0,
            since_perk_bought: 99.0,
            ..default()
        })
        .add_systems(OnEnter(GameState::Playing), spawn_root)
        .add_systems(
            Update,
            (controls, tick, rebuild).chain().run_if(in_state(GameState::Playing)),
        );
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        InRun,
        ProtoRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            ..default()
        },
        GlobalZIndex(5),
    ));
}

fn controls(
    input: Res<ButtonInput<KeyCode>>,
    mut mock: ResMut<Mock>,
    points: Res<Points>,
) {
    let shift = input.pressed(KeyCode::ShiftLeft);
    if input.just_pressed(KeyCode::F6) {
        mock.variant = (mock.variant + 1) % VARIANTS.len();
        info!("HUD prototype variant: {}", VARIANTS[mock.variant]);
    }
    if input.just_pressed(KeyCode::F5) {
        mock.round_offset += if shift { 10 } else { 1 };
    }
    if input.just_pressed(KeyCode::F7) {
        if mock.perks.len() >= MAX_PERKS {
            mock.perks.clear();
            mock.next_perk = 0;
        } else {
            let perk = mock.next_perk % PERKS.len();
            mock.perks.push(perk);
            mock.next_perk += 1;
            spend(&mut mock, PERKS[perk].cost, points.0);
            mock.since_perk_bought = 0.0;
        }
    }
    if input.just_pressed(KeyCode::F8) {
        let power_up = PowerUp::CYCLE[mock.next_power_up % PowerUp::CYCLE.len()];
        mock.next_power_up += 1;
        if power_up.timed() {
            // Re-pickup restarts the timer.
            mock.power_ups.retain(|(p, _)| *p != power_up);
            mock.power_ups.push((power_up, POWER_UP_SECS));
        } else {
            if power_up == PowerUp::MaxAmmo {
                mock.lethals = 4;
            }
            mock.announce = Some((power_up, 2.0));
        }
    }
    if input.just_pressed(KeyCode::F9) {
        mock.weapon = (mock.weapon + 1) % WEAPONS.len();
    }
    if input.just_pressed(KeyCode::F10) {
        mock.prompt = match mock.prompt {
            None => Some(0),
            Some(i) if i + 1 < PROMPTS.len() => Some(i + 1),
            Some(_) => None,
        };
    }
    if input.just_pressed(KeyCode::F11) {
        mock.boon_presses += 1;
        for _ in 0..5 {
            let pick = rand::random_range(0..BOONS.len());
            match mock.boons.iter_mut().find(|(b, _)| *b == pick) {
                Some((_, stacks)) => *stacks += 1,
                None => mock.boons.push((pick, 1)),
            }
        }
        if mock.boon_presses % 2 == 0 && mock.pacts.len() < PACTS.len() {
            let next = mock.pacts.len();
            mock.pacts.push(next);
        }
    }
    if input.just_pressed(KeyCode::F3) {
        mock.power_on = !mock.power_on;
    }
    if input.just_pressed(KeyCode::F2) {
        mock.box_spot = (mock.box_spot + 1) % BOX_SPOTS.len();
    }
    if input.just_pressed(KeyCode::KeyG) && mock.lethals > 0 {
        mock.lethals -= 1;
    }
}

fn spend(mock: &mut Mock, cost: u32, live_points: u32) {
    mock.spent = (mock.spent + cost).min(live_points);
    mock.popups.push(Popup {
        text: format!("-{cost}"),
        colour: SPEND_RED,
        age: 0.0,
    });
}

fn tick(
    mut mock: ResMut<Mock>,
    mut earned: MessageReader<PointsEarned>,
    mut shots: MessageReader<ShotFired>,
    points: Res<Points>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    let double = mock.power_ups.iter().any(|(p, _)| *p == PowerUp::DoublePoints);
    for PointsEarned(amount) in earned.read() {
        mock.popups.push(Popup {
            text: format!("+{}", if double { amount * 2 } else { *amount }),
            colour: EARN_GOLD,
            age: 0.0,
        });
    }
    if shots.read().count() > 0 {
        mock.since_shot = 0.0;
    }
    for popup in &mut mock.popups {
        popup.age += dt;
    }
    mock.popups.retain(|p| p.age < POPUP_SECS);
    for (_, left) in &mut mock.power_ups {
        *left -= dt;
    }
    mock.power_ups.retain(|(_, left)| *left > 0.0);
    if let Some((_, left)) = &mut mock.announce {
        *left -= dt;
        if *left <= 0.0 {
            mock.announce = None;
        }
    }
    let shown = points.0.saturating_sub(mock.spent);
    if shown != mock.last_points {
        mock.last_points = shown;
        mock.since_points_change = 0.0;
    }
    mock.since_points_change += dt;
    mock.since_shot += dt;
    mock.since_perk_bought += dt;
}

/// Everything a variant draws from, gathered once per frame.
struct View<'a> {
    mock: &'a Mock,
    round: u32,
    between_rounds: bool,
    points: u32,
    magazine: u32,
    magazine_size: u32,
    reserve: Option<u32>,
    reloading: bool,
    tab: bool,
    blink_on: bool,
}

impl View<'_> {
    fn weapon(&self) -> &'static MockWeapon {
        &WEAPONS[self.mock.weapon]
    }

    fn reserve_text(&self) -> String {
        self.reserve.map_or("--".to_string(), |r| r.to_string())
    }

    fn low_magazine(&self) -> bool {
        self.magazine * 4 <= self.magazine_size
    }

    fn prompt(&self) -> Option<&'static Prompt> {
        self.mock.prompt.map(|i| &PROMPTS[i])
    }

    fn round_colour(&self) -> Color {
        if self.between_rounds && self.blink_on {
            Color::WHITE
        } else {
            ROUND_RED
        }
    }

    /// Timed Power-ups blink faster in their last seconds.
    fn power_up_visible(&self, left: f32, elapsed: f32) -> bool {
        left > POWER_UP_BLINK_SECS || (elapsed * 6.0).fract() < 0.5
    }
}

fn rebuild(
    mut commands: Commands,
    root: Single<Entity, With<ProtoRoot>>,
    mock: Res<Mock>,
    round: Option<Res<Round>>,
    points: Res<Points>,
    weapon: Single<&Weapon>,
    input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
) {
    let Some(round) = round else { return };
    let elapsed = time.elapsed_secs();
    let view = View {
        mock: &mock,
        round: round.number + mock.round_offset,
        between_rounds: matches!(round.phase, RoundPhase::Intermission(_)),
        points: points.0.saturating_sub(mock.spent),
        magazine: weapon.magazine,
        magazine_size: weapon.magazine_size,
        reserve: weapon.reserve,
        reloading: weapon.reloading.is_some(),
        tab: input.pressed(KeyCode::Tab),
        blink_on: (elapsed * 3.0).fract() < 0.5,
    };
    let mut root = commands.entity(*root);
    root.despawn_related::<Children>();
    root.with_children(|ui| {
        match mock.variant {
            0 => variant_a(ui, &view, elapsed),
            1 => variant_b(ui, &view, elapsed),
            _ => variant_c(ui, &view, elapsed),
        }
        label(ui, &view);
    });
}

// ---------- shared bits ----------

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

/// Absolutely positions `base`.
fn at(base: Node, left: Option<f32>, right: Option<f32>, top: Option<f32>, bottom: Option<f32>) -> Node {
    let val = |v: Option<f32>| v.map_or(Val::Auto, px);
    Node {
        position_type: PositionType::Absolute,
        left: val(left),
        right: val(right),
        top: val(top),
        bottom: val(bottom),
        ..base
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

/// A coloured square with a short label, standing in for a Perk or Power-up icon.
fn icon(ui: &mut ChildSpawnerCommands, short: &str, colour: Color, size: f32) {
    ui.spawn((
        Node {
            width: px(size),
            height: px(size),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(colour.with_alpha(0.75)),
        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)),
    ))
    .with_child(text(short, size * 0.4, Color::WHITE));
}

/// WaW/BO1 chalk tallies for Rounds 1-5, numerals after.
fn round_counter(ui: &mut ChildSpawnerCommands, view: &View, size: f32) {
    let colour = view.round_colour();
    if view.round > 5 {
        ui.spawn(text(view.round.to_string(), size, colour));
        return;
    }
    let bar_h = size * 0.75;
    let bar_w = (size * 0.08).max(3.0);
    ui.spawn((
        Node {
            height: px(size),
            align_items: AlignItems::Center,
            column_gap: px(bar_w * 2.0),
            padding: UiRect::horizontal(px(bar_w)),
            ..default()
        },
    ))
    .with_children(|tallies| {
        for _ in 0..view.round.min(4) {
            tallies.spawn((
                Node {
                    width: px(bar_w),
                    height: px(bar_h),
                    ..default()
                },
                BackgroundColor(colour),
            ));
        }
        if view.round == 5 {
            // The fifth stroke crosses the other four.
            tallies.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0.0),
                    width: px(bar_w * 9.0),
                    height: px(bar_w),
                    ..default()
                },
                UiTransform::from_rotation(Rot2::degrees(-35.0)),
                BackgroundColor(colour),
            ));
        }
    });
}

fn prompt_line(prompt: &Prompt, view: &View) -> String {
    if let Some(blocked) = prompt.blocked {
        return blocked.to_string();
    }
    let mut line = format!("Press & hold F to {}", prompt.action);
    if let Some(cost) = prompt.cost {
        let cost = fire_sale(prompt, cost, view);
        line += &format!(" [Cost: {cost}]");
    }
    if let Some((what, cost)) = prompt.extra {
        line += &format!(", {what} [Cost: {cost}]");
    }
    line
}

fn fire_sale(prompt: &Prompt, cost: u32, view: &View) -> u32 {
    let sale = view.mock.power_ups.iter().any(|(p, _)| *p == PowerUp::FireSale);
    if sale && prompt.action == "buy Random Weapon" { 10 } else { cost }
}

fn popups(ui: &mut ChildSpawnerCommands, view: &View, drift_left: bool) {
    for (i, popup) in view.mock.popups.iter().enumerate() {
        let t = popup.age / POPUP_SECS;
        let x = 20.0 + t * 80.0;
        let y = (i % 4) as f32 * 18.0 + t * 20.0;
        let mut node = at(Node::default(), None, None, None, Some(y));
        if drift_left {
            node.right = px(x);
        } else {
            node.left = px(x);
        }
        ui.spawn((node, text(&popup.text, 24.0, popup.colour.with_alpha(1.0 - t))));
    }
}

fn boon_full_list(ui: &mut ChildSpawnerCommands, view: &View, size: f32) {
    for (boon, stacks) in &view.mock.boons {
        let (name, rarity) = BOONS[*boon];
        ui.spawn(text(format!("{name} {stacks}"), size, rarity.colour()));
    }
    for pact in &view.mock.pacts {
        let (name, drawback) = PACTS[*pact];
        ui.spawn(text(name, size, PACT_RED));
        ui.spawn(text(format!("  {drawback}"), size * 0.85, DRAWBACK_RED));
    }
}

fn announce(ui: &mut ChildSpawnerCommands, view: &View, top: f32) {
    if let Some((power_up, _)) = view.mock.announce {
        ui.spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(top),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_child(text(power_up.name().to_uppercase(), 40.0, Color::WHITE));
    }
}

fn centred_below_crosshair(ui: &mut ChildSpawnerCommands, offset: f32, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    ui.spawn(Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        top: Val::Percent(50.0),
        margin: UiRect::top(px(offset)),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(4.0),
        ..default()
    })
    .with_children(f);
}

fn label(ui: &mut ChildSpawnerCommands, view: &View) {
    ui.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(6.0),
            right: px(8.0),
            padding: UiRect::axes(px(8.0), px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 0.0, 1.0, 0.6)),
    ))
    .with_child(text(
        format!("PROTOTYPE {}  (F6)", VARIANTS[view.mock.variant]),
        14.0,
        Color::WHITE,
    ));
}

// ---------- A: Faithful BO1 corners ----------

fn variant_a(ui: &mut ChildSpawnerCommands, view: &View, elapsed: f32) {
    // Bottom-left: Perks above the Round.
    ui.spawn(at(column(8.0, AlignItems::FlexStart), Some(32.0), None, None, Some(16.0)))
        .with_children(|left| {
            left.spawn(row(6.0)).with_children(|perks| {
                for perk in &view.mock.perks {
                    let perk = &PERKS[*perk];
                    icon(perks, perk.short, perk.colour, 44.0);
                }
            });
            round_counter(left, view, 96.0);
        });

    // Bottom-right: Points with popups drifting left, then weapon and ammo.
    ui.spawn(at(column(4.0, AlignItems::FlexEnd), None, Some(32.0), None, Some(24.0)))
        .with_children(|right| {
            right
                .spawn(Node {
                    width: px(220.0),
                    height: px(40.0),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                })
                .with_children(|points| {
                    popups(points, view, true);
                    points.spawn(text(view.points.to_string(), 36.0, POINTS_GOLD));
                });
            let weapon = view.weapon();
            let name_colour = if weapon.upgraded { PAP_PURPLE } else { GREY };
            right.spawn(text(weapon.name, 18.0, name_colour));
            right.spawn(row(14.0)).with_children(|ammo| {
                for _ in 0..view.mock.lethals {
                    ammo.spawn((
                        Node {
                            width: px(10.0),
                            height: px(14.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.5, 0.6, 0.4)),
                    ));
                }
                ammo.spawn(text(
                    format!("{} | {}", view.magazine, view.reserve_text()),
                    28.0,
                    Color::WHITE,
                ));
            });
        });

    // Bottom-centre: timed Power-up icons, no numbers.
    ui.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            bottom: px(40.0),
            justify_content: JustifyContent::Center,
            column_gap: px(10.0),
            ..default()
        },
    ))
    .with_children(|centre| {
        for (power_up, left) in &view.mock.power_ups {
            if view.power_up_visible(*left, elapsed) {
                icon(centre, power_up.short(), power_up.colour(), 56.0);
            }
        }
    });

    centred_below_crosshair(ui, 40.0, |below| {
        if let Some(prompt) = view.prompt() {
            below.spawn(text(prompt_line(prompt, view), 20.0, Color::WHITE));
        }
        if view.reloading {
            // BO1 shows nothing extra while reloading.
        } else if view.magazine == 0 || view.low_magazine() {
            below.spawn(text("Reload", 20.0, Color::srgb(1.0, 0.85, 0.3)));
        }
    });

    announce(ui, view, 25.0);

    // Gauntlet: the full left-edge held column from "Boon offer screen".
    if !view.mock.boons.is_empty() || !view.mock.pacts.is_empty() {
        ui.spawn(at(column(1.0, AlignItems::FlexStart), Some(12.0), None, Some(120.0), None))
            .with_children(|col| boon_full_list(col, view, 15.0));
    }
}

// ---------- B: Status panel ----------

fn variant_b(ui: &mut ChildSpawnerCommands, view: &View, elapsed: f32) {
    let _ = elapsed;
    // Top-left status panel.
    ui.spawn((
        at(
            Node {
            padding: UiRect::all(px(10.0)),
            ..column(4.0, AlignItems::FlexStart)
            },
            Some(16.0), None, Some(16.0), None,
        ),
        BackgroundColor(PANEL),
    ))
    .with_children(|panel| {
        panel.spawn(row(10.0)).with_children(|r| {
            r.spawn(text("ROUND", 18.0, GREY));
            r.spawn(text(view.round.to_string(), 48.0, view.round_colour()));
        });
        panel
            .spawn(Node {
                width: px(260.0),
                height: px(34.0),
                ..default()
            })
            .with_children(|points| {
                points.spawn(text(format!("{} pts", view.points), 30.0, POINTS_GOLD));
                popups(points, view, false);
            });
        let on_off = |on: bool| if on { ("ON", Color::srgb(0.4, 0.9, 0.4)) } else { ("OFF", SPEND_RED) };
        let (power, power_colour) = on_off(view.mock.power_on);
        panel.spawn(row(6.0)).with_children(|r| {
            r.spawn(text("Power", 16.0, GREY));
            r.spawn(text(power, 16.0, power_colour));
        });
        let sale = view.mock.power_ups.iter().any(|(p, _)| *p == PowerUp::FireSale);
        let box_at = if sale { "every spot (Fire Sale)" } else { BOX_SPOTS[view.mock.box_spot] };
        panel.spawn(row(6.0)).with_children(|r| {
            r.spawn(text("Box", 16.0, GREY));
            r.spawn(text(box_at, 16.0, Color::WHITE));
        });
        panel.spawn(row(6.0)).with_children(|r| {
            r.spawn(text("Pack-a-Punch", 16.0, GREY));
            let (pap, colour) = if view.mock.power_on {
                ("ready", Color::srgb(0.4, 0.9, 0.4))
            } else {
                ("needs Power", SPEND_RED)
            };
            r.spawn(text(pap, 16.0, colour));
        });

        // Gauntlet: condensed to Rarity counts; Pacts always in full.
        if !view.mock.boons.is_empty() || !view.mock.pacts.is_empty() {
            panel.spawn(Node {
                height: px(6.0),
                ..default()
            });
            if view.tab {
                boon_full_list(panel, view, 14.0);
            } else {
                panel.spawn(row(10.0)).with_children(|r| {
                    for rarity in [Rarity::Common, Rarity::Rare, Rarity::Legendary] {
                        let stacks: u32 = view
                            .mock
                            .boons
                            .iter()
                            .filter(|(b, _)| BOONS[*b].1 == rarity)
                            .map(|(_, s)| s)
                            .sum();
                        r.spawn(text(format!("■ {stacks}"), 16.0, rarity.colour()));
                    }
                    r.spawn(text("Boons (Tab)", 13.0, GREY));
                });
                for pact in &view.mock.pacts {
                    let (name, drawback) = PACTS[*pact];
                    panel.spawn(row(6.0)).with_children(|r| {
                        r.spawn(text(name, 14.0, PACT_RED));
                        r.spawn(text(drawback, 13.0, DRAWBACK_RED));
                    });
                }
            }
        }
    });

    // Top-centre: Power-up timer bars.
    ui.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            top: px(16.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4.0),
            ..default()
        },
    ))
    .with_children(|top| {
        for (power_up, left) in &view.mock.power_ups {
            top.spawn(row(8.0)).with_children(|r| {
                r.spawn((
                    Node {
                        width: px(130.0),
                        justify_content: JustifyContent::FlexEnd,
                        ..default()
                    },
                ))
                .with_child(text(power_up.name(), 16.0, power_up.colour()));
                r.spawn((
                    Node {
                        width: px(160.0),
                        height: px(10.0),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                ))
                .with_child((
                    Node {
                        width: percent(100.0 * left / POWER_UP_SECS),
                        height: percent(100.0),
                        ..default()
                    },
                    BackgroundColor(power_up.colour()),
                ));
                r.spawn(text(format!("{:.0}", left.ceil()), 16.0, Color::WHITE));
            });
        }
    });

    // Right: weapon card.
    ui.spawn((
        at(
            Node {
            padding: UiRect::all(px(10.0)),
            min_width: px(220.0),
            ..column(2.0, AlignItems::FlexEnd)
            },
            None, Some(16.0), None, Some(16.0),
        ),
        BackgroundColor(PANEL),
    ))
    .with_children(|card| {
        let weapon = view.weapon();
        card.spawn(text(
            weapon.name,
            22.0,
            if weapon.upgraded { PAP_PURPLE } else { Color::WHITE },
        ));
        for attachment in weapon.attachments {
            card.spawn(text(*attachment, 14.0, GREY));
        }
        if let Some(ammo_mod) = weapon.ammo_mod {
            card.spawn(text(ammo_mod, 14.0, PAP_PURPLE));
        }
        let ammo_colour = if view.low_magazine() { SPEND_RED } else { Color::WHITE };
        card.spawn(row(6.0)).with_children(|r| {
            if view.reloading {
                r.spawn(text("RELOADING", 30.0, GREY));
            } else {
                r.spawn(text(view.magazine.to_string(), 40.0, ammo_colour));
            }
            r.spawn(text(format!("/ {}", view.reserve_text()), 20.0, GREY));
        });
        card.spawn(text(format!("Grenades {}", view.mock.lethals), 14.0, GREY));
    });

    // Bottom-centre: named Perks, prompt above them.
    ui.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            bottom: px(16.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(10.0),
            ..default()
        },
    ))
    .with_children(|bottom| {
        if let Some(prompt) = view.prompt() {
            bottom
                .spawn((
                    Node {
                        padding: UiRect::axes(px(12.0), px(6.0)),
                        column_gap: px(10.0),
                        ..row(10.0)
                    },
                    BackgroundColor(PANEL),
                ))
                .with_children(|p| {
                    if let Some(blocked) = prompt.blocked {
                        p.spawn(text(blocked, 18.0, SPEND_RED));
                        return;
                    }
                    p.spawn(text("[F]", 18.0, POINTS_GOLD));
                    p.spawn(text(prompt.action, 18.0, Color::WHITE));
                    if let Some(cost) = prompt.cost {
                        let cost = fire_sale(prompt, cost, view);
                        let colour = if view.points >= cost { POINTS_GOLD } else { SPEND_RED };
                        p.spawn(text(cost.to_string(), 18.0, colour));
                    }
                    if let Some((what, cost)) = prompt.extra {
                        p.spawn(text(format!("· {what}"), 18.0, Color::WHITE));
                        p.spawn(text(cost.to_string(), 18.0, POINTS_GOLD));
                    }
                });
        }
        bottom.spawn(row(12.0)).with_children(|perks| {
            for perk in &view.mock.perks {
                let perk = &PERKS[*perk];
                perks
                    .spawn(column(2.0, AlignItems::Center))
                    .with_children(|p| {
                        icon(p, perk.short, perk.colour, 36.0);
                        p.spawn(text(perk.name, 12.0, GREY));
                    });
            }
        });
    });

    announce(ui, view, 22.0);
}

// ---------- C: Minimal, show on change ----------

fn variant_c(ui: &mut ChildSpawnerCommands, view: &View, elapsed: f32) {
    let mock = view.mock;
    // Top-centre: a small Round number, tallies early on.
    ui.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            top: px(12.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
    ))
    .with_children(|top| round_counter(top, view, 40.0));

    // Points bottom-right only for a few seconds after they change, or on Tab.
    let points_alpha = fade(mock.since_points_change, view.tab);
    if points_alpha > 0.0 {
        ui.spawn((
            at(
                Node {
                width: px(240.0),
                height: px(40.0),
                justify_content: JustifyContent::FlexEnd,
                ..default()
                },
                None, Some(32.0), None, Some(32.0),
            ),
        ))
        .with_children(|points| {
            popups(points, view, true);
            points.spawn(text(view.points.to_string(), 32.0, POINTS_GOLD.with_alpha(points_alpha)));
        });
    }

    // Perks bottom-left after a buy, or on Tab.
    let perk_alpha = fade(mock.since_perk_bought, view.tab);
    if perk_alpha > 0.0 {
        ui.spawn(at(row(6.0), Some(32.0), None, None, Some(32.0)))
            .with_children(|perks| {
                for perk in &mock.perks {
                    let perk = &PERKS[*perk];
                    icon(perks, perk.short, perk.colour.with_alpha(perk_alpha), 36.0);
                }
            });
    }

    // Ammo hugs the crosshair: only while firing, reloading, low, or on Tab.
    let show_ammo = view.tab || view.reloading || view.low_magazine() || mock.since_shot < 2.0;
    if show_ammo {
        ui.spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            margin: UiRect {
                left: px(28.0),
                top: px(-10.0),
                ..default()
            },
            ..column(0.0, AlignItems::FlexStart)
        })
        .with_children(|ammo| {
            let colour = if view.low_magazine() { SPEND_RED } else { Color::WHITE };
            let shown = if view.reloading {
                "...".to_string()
            } else {
                format!("{} / {}", view.magazine, view.reserve_text())
            };
            ammo.spawn(text(shown, 16.0, colour));
            if view.tab || mock.since_shot > 1.0 {
                let weapon = view.weapon();
                ammo.spawn(text(weapon.name, 12.0, if weapon.upgraded { PAP_PURPLE } else { GREY }));
            }
        });
    }

    centred_below_crosshair(ui, 30.0, |below| {
        // Power-ups as a small countdown line.
        if !mock.power_ups.is_empty() {
            below.spawn(row(14.0)).with_children(|r| {
                for (power_up, left) in &mock.power_ups {
                    if view.power_up_visible(*left, elapsed) {
                        r.spawn(text(
                            format!("{} {:.0}", power_up.short(), left.ceil()),
                            14.0,
                            power_up.colour(),
                        ));
                    }
                }
            });
        }
        if let Some(prompt) = view.prompt() {
            let line = match (prompt.blocked, prompt.cost) {
                (Some(blocked), _) => blocked.to_string(),
                (None, Some(cost)) => format!("F  {}  {}", prompt.action, fire_sale(prompt, cost, view)),
                (None, None) => format!("F  {}", prompt.action),
            };
            below.spawn(text(line, 16.0, Color::srgba(1.0, 1.0, 1.0, 0.85)));
        }
    });

    announce(ui, view, 30.0);

    // Gauntlet: held Boons on Tab only, as a right-hand sheet.
    if view.tab && (!mock.boons.is_empty() || !mock.pacts.is_empty()) {
        ui.spawn((
            at(
                Node {
                height: percent(100.0),
                width: px(280.0),
                padding: UiRect::all(px(16.0)),
                flex_wrap: FlexWrap::Wrap,
                ..column(2.0, AlignItems::FlexStart)
                },
                None, Some(0.0), Some(0.0), None,
            ),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|sheet| {
            sheet.spawn(text("HELD", 16.0, GREY));
            boon_full_list(sheet, view, 15.0);
        });
    }
}

/// Fully visible while `held`, otherwise fades out after SHOW_ON_CHANGE_SECS.
fn fade(since: f32, held: bool) -> f32 {
    if held {
        1.0
    } else {
        (1.0 - (since - SHOW_ON_CHANGE_SECS)).clamp(0.0, 1.0)
    }
}
