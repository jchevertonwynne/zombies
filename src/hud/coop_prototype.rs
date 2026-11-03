//! PROTOTYPE, throw me away: three Co-op HUD variants over the real HUD, with three fake
//! teammates, for "Prototype: the Co-op HUD" (#135). Run `cargo run --features coop-hud-prototype`,
//! start a Run, and press ] for the next variant ([ for the one before).
//! `COOP_HUD_VARIANT=B` picks the variant to start on.
//!
//! A scripted story loops every 32 s through four Phases, so each variant shows every case: a
//! Downed teammate bleeding out, a teammate Reviving them, you Reviving, you being Revived, a
//! Weapon waiting in the Box and at Pack-a-Punch, a paused teammate, one with an open Boon Offer,
//! and one Reconnecting with their Seat held. The whole layer is rebuilt every frame.

use bevy::prelude::*;

use super::{full_screen, text, with_commas};
use crate::mystery_box::MysteryBox;
use crate::pack_a_punch::PackAPunch;
use crate::player::{Player, PlayerCamera};
use crate::points::Points;
use crate::state::{GameState, InRun};

const SEAT_COLOURS: [Color; 4] = [
    Color::srgb(0.95, 0.95, 0.95),
    Color::srgb(0.35, 0.6, 1.0),
    Color::srgb(1.0, 0.85, 0.25),
    Color::srgb(0.4, 0.9, 0.4),
];
const DOWN_RED: Color = Color::srgb(1.0, 0.2, 0.15);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const PHASE_SECS: f32 = 8.0;
const BLEED_OUT_SECS: f32 = 45.0;
const REVIVE_SECS: f32 = 3.0;
const VARIANTS: [(&str, &str); 3] = [
    ("A", "BO1 corner stack"),
    ("B", "Roster cards"),
    ("C", "All in the world"),
];

pub struct CoopHudPrototypePlugin;

impl Plugin for CoopHudPrototypePlugin {
    fn build(&self, app: &mut App) {
        let start = std::env::var("COOP_HUD_VARIANT")
            .ok()
            .and_then(|v| VARIANTS.iter().position(|(key, _)| key.eq_ignore_ascii_case(&v)))
            .unwrap_or(0);
        app.insert_resource(Variant(start))
            .init_resource::<Story>()
            .add_systems(OnEnter(GameState::Playing), spawn_root)
            .add_systems(Update, (switch, tell, draw).chain().run_if(in_state(GameState::Playing)));
    }
}

#[derive(Resource)]
struct Variant(usize);

#[derive(Resource, Default)]
struct Story {
    elapsed: f32,
    /// Where each fake teammate stands, fixed around the Player when the Run starts.
    spots: Option<[Vec3; 3]>,
}

#[derive(Component)]
struct Root;

fn spawn_root(mut commands: Commands, mut story: ResMut<Story>) {
    *story = Story::default();
    commands.spawn((Root, InRun, full_screen(), GlobalZIndex(50)));
}

fn switch(keys: Res<ButtonInput<KeyCode>>, mut variant: ResMut<Variant>) {
    let n = VARIANTS.len();
    if keys.just_pressed(KeyCode::BracketRight) {
        variant.0 = (variant.0 + 1) % n;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        variant.0 = (variant.0 + n - 1) % n;
    }
}

fn tell(time: Res<Time>, mut story: ResMut<Story>, player: Single<&GlobalTransform, With<Player>>) {
    story.elapsed += time.delta_secs();
    if story.spots.is_none() {
        let at = player.translation();
        let forward = player.forward().with_y(0.0).normalize_or(Vec3::NEG_Z);
        let right = forward.cross(Vec3::Y);
        story.spots = Some([at + forward * 9.0 + right * 3.0, at + forward * 5.0 - right * 4.0, at - forward * 6.0]);
    }
}

/// What one teammate is doing right now.
#[derive(Clone)]
enum Status {
    Up,
    /// Bleeding out, with seconds left, and who is Reviving them and how far along.
    Downed { left: f32, reviver: Option<(usize, f32)> },
    Reviving(usize),
    Paused,
    BoonOffer,
    Reconnecting,
}

struct Mate {
    seat: usize,
    name: &'static str,
    points: u64,
    status: Status,
    at: Vec3,
}

/// A Weapon waiting for its taker: in the Box or Pack-a-Punch.
struct Waiting {
    seat: usize,
    weapon: &'static str,
    machine: &'static str,
    left: f32,
    at: Option<Vec3>,
}

/// The local Player's Revive bar, as reviver or as the one being Revived.
enum MyRevive {
    Reviving { whom: usize, progress: f32 },
    BeingRevived { by: usize, progress: f32 },
}

struct Scene {
    phase: &'static str,
    mates: Vec<Mate>,
    waiting: Vec<Waiting>,
    mine: Option<MyRevive>,
    my_bleed: Option<f32>,
}

const NAMES: [&str; 4] = ["You", "Takeo", "Nikolai", "Richtofen"];

fn scene(story: &Story, box_at: Option<Vec3>, pap_at: Option<Vec3>) -> Scene {
    let spots = story.spots.unwrap_or_default();
    let t = story.elapsed % (4.0 * PHASE_SECS);
    let phase = (t / PHASE_SECS) as usize;
    let into = t % PHASE_SECS;
    let progress = (into / REVIVE_SECS).min(1.0);
    let bleed = BLEED_OUT_SECS - 12.0 - into;
    let mate = |seat: usize, points: u64, status: Status| Mate { seat, name: NAMES[seat], points, status, at: spots[seat - 1] };
    match phase {
        0 => Scene {
            phase: "1/4 Takeo is Down and bleeding out; Nikolai's Ray Gun waits in the Box; Richtofen is paused",
            mates: vec![
                mate(1, 1_240, Status::Downed { left: bleed + 8.0, reviver: None }),
                mate(2, 6_810, Status::Up),
                mate(3, 3_050, Status::Paused),
            ],
            waiting: vec![Waiting { seat: 2, weapon: "Ray Gun", machine: "Box", left: 12.0 - into, at: box_at }],
            mine: None,
            my_bleed: None,
        },
        1 => Scene {
            phase: "2/4 Nikolai is Reviving Takeo; Richtofen has a Boon Offer open",
            mates: vec![
                mate(1, 1_240, Status::Downed { left: bleed, reviver: Some((2, progress)) }),
                mate(2, 6_810, Status::Reviving(1)),
                mate(3, 3_050, Status::BoonOffer),
            ],
            waiting: vec![],
            mine: None,
            my_bleed: None,
        },
        2 => Scene {
            phase: "3/4 You are Reviving Takeo; Richtofen dropped and his Seat is held",
            mates: vec![
                mate(1, 1_240, Status::Downed { left: bleed, reviver: Some((0, progress)) }),
                mate(2, 7_420, Status::Up),
                mate(3, 3_050, Status::Reconnecting),
            ],
            waiting: vec![],
            mine: Some(MyRevive::Reviving { whom: 1, progress }),
            my_bleed: None,
        },
        _ => Scene {
            phase: "4/4 You are Down and Nikolai is Reviving you; Nikolai's Weapon waits at Pack-a-Punch",
            mates: vec![
                mate(1, 1_490, Status::Up),
                mate(2, 7_420, Status::Reviving(0)),
                mate(3, 3_050, Status::Up),
            ],
            waiting: vec![Waiting { seat: 2, weapon: "Porter's X2 Ray Gun", machine: "Pack-a-Punch", left: 15.0 - into, at: pap_at }],
            mine: Some(MyRevive::BeingRevived { by: 2, progress }),
            my_bleed: Some(BLEED_OUT_SECS - 20.0 - into),
        },
    }
}

fn status_line(status: &Status) -> Option<(String, Color)> {
    match status {
        Status::Up => None,
        Status::Downed { left, reviver: None } => Some((format!("DOWN  {:.0}s", left.max(0.0)), DOWN_RED)),
        Status::Downed { reviver: Some((by, _)), .. } => Some((format!("being Revived by {}", NAMES[*by]), Color::WHITE)),
        Status::Reviving(whom) => Some((format!("Reviving {}", NAMES[*whom]), Color::WHITE)),
        Status::Paused => Some(("PAUSED".into(), DIM)),
        Status::BoonOffer => Some(("choosing a Boon".into(), Color::srgb(0.8, 0.6, 1.0))),
        Status::Reconnecting => Some(("Reconnecting…  Seat held".into(), DIM)),
    }
}

fn faded(color: Color, status: &Status) -> Color {
    if matches!(status, Status::Reconnecting) { color.with_alpha(0.35) } else { color }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    root: Single<Entity, With<Root>>,
    variant: Res<Variant>,
    story: Res<Story>,
    points: Res<Points>,
    camera: Single<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    window: Single<&Window>,
    boxes: Query<(&MysteryBox, &GlobalTransform)>,
    machines: Query<&GlobalTransform, With<PackAPunch>>,
) {
    let box_at = boxes.iter().find(|(b, _)| b.present()).map(|(_, at)| at.translation() + Vec3::Y * 1.6);
    let pap_at = machines.iter().next().map(|at| at.translation() + Vec3::Y * 2.2);
    let scene = scene(&story, box_at, pap_at);
    let (camera, eye) = *camera;
    let size = window.size();
    // On screen, or `None` when behind the camera.
    let project = |at: Vec3| -> Option<Vec2> {
        let ahead = (at - eye.translation()).dot(*eye.forward()) > 0.0;
        if ahead { camera.world_to_viewport(eye, at).ok() } else { None }
    };
    let me = points.balance;
    let (key, name) = VARIANTS[variant.0];
    commands.entity(*root).despawn_children().with_children(|ui| {
        match variant.0 {
            0 => corner_stack(ui, &scene, me, &project),
            1 => roster_cards(ui, &scene, &project),
            _ => in_world(ui, &scene, me, &project, size),
        }
        switcher(ui, key, name, scene.phase);
    });
}

type Project<'a> = dyn Fn(Vec3) -> Option<Vec2> + 'a;

fn at(pos: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(pos.x),
        top: px(pos.y),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn bar(ui: &mut ChildSpawnerCommands, width: f32, height: f32, fill: f32, color: Color) {
    ui.spawn((
        Node { width: px(width), height: px(height), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
    ))
    .with_child((
        Node { width: percent(100.0 * fill.clamp(0.0, 1.0)), height: percent(100.0), ..default() },
        BackgroundColor(color),
    ));
}

/// The local Revive bar under the crosshair, shared by A and B at different places.
fn my_revive_bar(ui: &mut ChildSpawnerCommands, scene: &Scene, top: Val, bottom: Val, width: f32) {
    let Some(mine) = &scene.mine else { return };
    let (label, progress, color) = match mine {
        MyRevive::Reviving { whom, progress } => (format!("Reviving {}", NAMES[*whom]), *progress, SEAT_COLOURS[*whom]),
        MyRevive::BeingRevived { by, progress } => (format!("Being Revived by {}", NAMES[*by]), *progress, SEAT_COLOURS[*by]),
    };
    ui.spawn(Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        top,
        bottom,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: px(4.0),
        ..default()
    })
    .with_children(|c| {
        c.spawn(text(label, 18.0, Color::WHITE));
        bar(c, width, 8.0, progress, color);
    });
}

fn my_bleed_tint(ui: &mut ChildSpawnerCommands, scene: &Scene) {
    if let Some(left) = scene.my_bleed {
        ui.spawn((full_screen(), BackgroundColor(Color::srgba(0.5, 0.0, 0.0, 0.25))));
        ui.spawn(Node { position_type: PositionType::Absolute, width: percent(100.0), top: percent(30.0), justify_content: JustifyContent::Center, ..default() })
            .with_child(text(format!("BLEEDING OUT  {:.0}", left.max(0.0)), 28.0, DOWN_RED));
    }
}

/// A: BO1. Teammates' Points stacked over your own in the bottom-right, in Seat colours, with a
/// short status tag. A Downed teammate is a revive icon seen through walls, with their timer
/// under it; a waiting Weapon is labelled over its machine.
fn corner_stack(ui: &mut ChildSpawnerCommands, scene: &Scene, _me: u64, project: &Project) {
    ui.spawn(Node {
        position_type: PositionType::Absolute,
        right: px(32.0),
        bottom: px(24.0 + 36.0 + 20.0 + 15.0 + 24.0 + 32.0 + 140.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexEnd,
        row_gap: px(2.0),
        ..default()
    })
    .with_children(|stack| {
        for mate in &scene.mates {
            stack
                .spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(8.0), ..default() })
                .with_children(|row| {
                    if let Some((line, color)) = status_line(&mate.status) {
                        row.spawn(text(line, 13.0, color));
                    }
                    row.spawn(text(mate.name, 14.0, faded(SEAT_COLOURS[mate.seat], &mate.status)));
                    row.spawn(text(with_commas(mate.points), 24.0, faded(SEAT_COLOURS[mate.seat], &mate.status)));
                });
        }
    });
    for mate in &scene.mates {
        let Status::Downed { left, reviver } = &mate.status else { continue };
        let Some(pos) = project(mate.at + Vec3::Y * 0.6) else { continue };
        let icon = if reviver.is_some() { Color::WHITE } else { DOWN_RED };
        ui.spawn(at(pos - Vec2::new(30.0, 20.0))).with_children(|m| {
            m.spawn((Node { width: px(22.0), height: px(22.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(icon)))
                .with_child(text("+", 20.0, SEAT_COLOURS[mate.seat]));
            m.spawn(text(format!("{:.0}", left.max(0.0)), 14.0, icon));
            if let Some((by, progress)) = reviver {
                bar(m, 40.0, 4.0, *progress, SEAT_COLOURS[*by]);
            }
        });
    }
    for waiting in &scene.waiting {
        let Some(pos) = waiting.at.and_then(project) else { continue };
        ui.spawn(at(pos - Vec2::new(70.0, 0.0)))
            .with_child(text(format!("{}'s {}  {:.0}", NAMES[waiting.seat], waiting.weapon, waiting.left.max(0.0)), 15.0, SEAT_COLOURS[waiting.seat]));
    }
    my_revive_bar(ui, scene, percent(62.0), Val::Auto, 220.0);
    my_bleed_tint(ui, scene);
}

/// B: a roster down the left edge, one card per teammate, holding everything about them: Points,
/// status with its own timer bar, and any Weapon waiting for them. The world only gets a small
/// diamond over a Downed teammate.
fn roster_cards(ui: &mut ChildSpawnerCommands, scene: &Scene, project: &Project) {
    ui.spawn(Node {
        position_type: PositionType::Absolute,
        left: px(24.0),
        top: percent(28.0),
        flex_direction: FlexDirection::Column,
        row_gap: px(8.0),
        ..default()
    })
    .with_children(|roster| {
        for mate in &scene.mates {
            let colour = faded(SEAT_COLOURS[mate.seat], &mate.status);
            roster
                .spawn((
                    Node { flex_direction: FlexDirection::Row, width: px(230.0), ..default() },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                ))
                .with_children(|card| {
                    card.spawn((Node { width: px(6.0), ..default() }, BackgroundColor(colour)));
                    card.spawn(Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(6.0)), row_gap: px(3.0), flex_grow: 1.0, ..default() })
                        .with_children(|body| {
                            body.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|top| {
                                top.spawn(text(mate.name, 16.0, colour));
                                top.spawn(text(with_commas(mate.points), 16.0, colour));
                            });
                            if let Some((line, color)) = status_line(&mate.status) {
                                body.spawn(text(line, 13.0, color));
                            }
                            match &mate.status {
                                Status::Downed { left, reviver: None } => bar(body, 200.0, 4.0, left / BLEED_OUT_SECS, DOWN_RED),
                                Status::Downed { reviver: Some((by, progress)), .. } => bar(body, 200.0, 4.0, *progress, SEAT_COLOURS[*by]),
                                _ => {}
                            }
                            for waiting in scene.waiting.iter().filter(|w| w.seat == mate.seat) {
                                body.spawn(text(format!("{} waiting at {}  {:.0}", waiting.weapon, waiting.machine, waiting.left.max(0.0)), 12.0, DIM));
                            }
                        });
                });
        }
    });
    for mate in &scene.mates {
        if !matches!(mate.status, Status::Downed { .. }) {
            continue;
        }
        let Some(pos) = project(mate.at + Vec3::Y * 0.6) else { continue };
        ui.spawn((
            Node { position_type: PositionType::Absolute, left: px(pos.x - 7.0), top: px(pos.y - 7.0), width: px(14.0), height: px(14.0), ..default() },
            UiTransform::from_rotation(Rot2::degrees(45.0)),
            BackgroundColor(SEAT_COLOURS[mate.seat]),
            Outline::new(px(2.0), px(0.0), DOWN_RED),
        ));
    }
    // B puts the Revive bar low and wide, over the Power-up bars.
    my_revive_bar(ui, scene, Val::Auto, px(170.0), 420.0);
    my_bleed_tint(ui, scene);
}

/// C: next to nothing in the corners, just a top strip of Seat chips with Points. Everything else
/// floats in the world over each teammate: name and status over every head, a Downed teammate
/// pinned to the screen edge with an arrow when out of view, the Revive bar wrapped around the
/// crosshair.
fn in_world(ui: &mut ChildSpawnerCommands, scene: &Scene, me: u64, project: &Project, size: Vec2) {
    ui.spawn(Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        top: px(12.0),
        justify_content: JustifyContent::Center,
        column_gap: px(6.0),
        ..default()
    })
    .with_children(|strip| {
        let chip = |strip: &mut ChildSpawnerCommands, seat: usize, points: u64, status: Option<&Status>| {
            let down = matches!(status, Some(Status::Downed { .. }));
            let colour = status.map_or(SEAT_COLOURS[seat], |s| faded(SEAT_COLOURS[seat], s));
            strip
                .spawn((
                    Node { padding: UiRect::axes(px(8.0), px(3.0)), column_gap: px(6.0), ..default() },
                    BackgroundColor(if down { DOWN_RED.with_alpha(0.7) } else { colour.with_alpha(0.25) }),
                    Outline::new(px(1.0), px(0.0), colour),
                ))
                .with_children(|c| {
                    c.spawn(text(NAMES[seat], 13.0, colour));
                    c.spawn(text(with_commas(points), 13.0, Color::WHITE));
                });
        };
        chip(strip, 0, me, None);
        for mate in &scene.mates {
            chip(strip, mate.seat, mate.points, Some(&mate.status));
        }
    });
    let centre = size / 2.0;
    for mate in &scene.mates {
        let downed = matches!(mate.status, Status::Downed { .. });
        let head = mate.at + Vec3::Y * if downed { 0.6 } else { 2.0 };
        let colour = faded(SEAT_COLOURS[mate.seat], &mate.status);
        let on_screen = project(head).filter(|p| p.x > 0.0 && p.y > 0.0 && p.x < size.x && p.y < size.y);
        match on_screen {
            Some(pos) => {
                ui.spawn(at(pos - Vec2::new(60.0, 30.0))).with_children(|tag| {
                    tag.spawn(text(mate.name, 15.0, colour));
                    if let Some((line, color)) = status_line(&mate.status) {
                        tag.spawn(text(line, 12.0, color));
                    }
                    if let Status::Downed { reviver: Some((by, progress)), .. } = &mate.status {
                        bar(tag, 60.0, 4.0, *progress, SEAT_COLOURS[*by]);
                    }
                });
            }
            None if downed => {
                // Pinned to the screen edge in the teammate's direction.
                let to = project(head).map_or(-(centre - Vec2::new(size.x / 2.0, 0.0)), |p| p - centre);
                let dir = if to.length_squared() < 1.0 { Vec2::Y } else { to.normalize() };
                let edge = centre + dir * (centre - Vec2::splat(60.0));
                let arrow = if dir.x.abs() > dir.y.abs() { if dir.x > 0.0 { "▶" } else { "◀" } } else if dir.y > 0.0 { "▼" } else { "▲" };
                ui.spawn(at(edge - Vec2::new(40.0, 20.0))).with_children(|pin| {
                    pin.spawn(text(format!("{arrow} {}", mate.name), 15.0, DOWN_RED));
                    if let Some((line, color)) = status_line(&mate.status) {
                        pin.spawn(text(line, 12.0, color));
                    }
                });
            }
            None => {}
        }
    }
    for waiting in &scene.waiting {
        let Some(pos) = waiting.at.and_then(project) else { continue };
        ui.spawn(at(pos - Vec2::new(50.0, 0.0))).with_children(|l| {
            l.spawn(text(format!("{}'s", NAMES[waiting.seat]), 13.0, SEAT_COLOURS[waiting.seat]));
            l.spawn(text(waiting.weapon, 16.0, Color::WHITE));
            bar(l, 80.0, 3.0, waiting.left / 15.0, SEAT_COLOURS[waiting.seat]);
        });
    }
    if let Some(mine) = &scene.mine {
        let (progress, color, label) = match mine {
            MyRevive::Reviving { whom, progress } => (*progress, SEAT_COLOURS[*whom], NAMES[*whom].to_string()),
            MyRevive::BeingRevived { by, progress } => (*progress, SEAT_COLOURS[*by], format!("by {}", NAMES[*by])),
        };
        // Four short bars boxing the crosshair, each filling with the Revive.
        for (dx, dy, w, h) in [(-30.0, -32.0, 60.0, 4.0), (-30.0, 28.0, 60.0, 4.0)] {
            ui.spawn(Node { position_type: PositionType::Absolute, left: px(centre.x + dx), top: px(centre.y + dy), ..default() })
                .with_children(|b| bar(b, w, h, progress, color));
        }
        ui.spawn(at(centre + Vec2::new(-40.0, 36.0))).with_child(text(label, 13.0, color));
    }
    my_bleed_tint(ui, scene);
}

/// The prototype's own switcher, bottom-centre over everything: plainly not part of the HUD.
fn switcher(ui: &mut ChildSpawnerCommands, key: &str, name: &str, phase: &str) {
    ui.spawn(Node {
        position_type: PositionType::Absolute,
        width: percent(100.0),
        bottom: px(110.0),
        justify_content: JustifyContent::Center,
        ..default()
    })
    .with_children(|c| {
        c.spawn((
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, padding: UiRect::axes(px(14.0), px(6.0)), border_radius: BorderRadius::all(px(12.0)), ..default() },
            BackgroundColor(Color::srgb(1.0, 0.0, 0.85)),
        ))
        .with_children(|pill| {
            pill.spawn(text(format!("◀ [   {key} ({name})   ] ▶"), 16.0, Color::BLACK));
            pill.spawn(text(phase, 12.0, Color::BLACK));
        });
    });
}
