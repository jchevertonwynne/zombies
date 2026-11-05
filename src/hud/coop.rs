//! The Co-op HUD. Teammates' Points stack over the local Player's in the bottom-right in Seat
//! order, each with a status tag to its left. A Downed teammate shows a revive icon in the world,
//! seen through walls, and a Weapon waiting at the Box or Pack-a-Punch is labelled over it. The
//! local Revive bar sits under the crosshair, and "BLEEDING OUT" over it while the local Player is
//! Down. Solo shows none of it.
//!
//! `read_coop_view` turns the Players' state into a `CoopView`, and `draw_coop` puts it on screen.

use bevy::prelude::*;

use super::{column, full_screen, row, text, with_commas};
use crate::last_stand::LastStand;
use crate::mystery_box::{BoxState, MysteryBox};
use crate::pack_a_punch::PackAPunch;
use crate::player::{LocalPlayer, Player, PlayerCamera, PlayerName, Seat};
use crate::points::Points;
use crate::respawn::AwaitingRespawn;
use crate::revive::Reviving;
use crate::state::{Choosing, Paused, RunType};

const DOWN_RED: Color = Color::srgb(1.0, 0.2, 0.15);
const DIM: Color = Color::srgb(0.55, 0.55, 0.55);
const CHOOSING_PURPLE: Color = Color::srgb(0.8, 0.6, 1.0);
/// How much of a held Seat's row shows through.
const HELD_ROW_ALPHA: f32 = 0.35;
/// How far above a Downed Player's centre their revive icon floats, and above the Box and
/// Pack-a-Punch their Weapon's label does.
const REVIVE_ICON_LIFT: f32 = 0.5;
const BOX_LABEL_LIFT: f32 = 1.6;
const PAP_LABEL_LIFT: f32 = 2.2;

/// A Player whose dropped connection still holds their Seat. Every machine adds it with
/// `Disconnected`, and takes it away with it.
#[derive(Component)]
pub struct SeatHeld;

/// What a teammate is up to, as their row's status tag.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Down(u32),
    BeingRevived(String),
    Reviving(String),
    AwaitingRespawn,
    Paused,
    Choosing,
    Reconnecting,
}

impl Tag {
    pub fn text(&self) -> String {
        match self {
            Tag::Down(secs) => format!("DOWN {secs}s"),
            Tag::BeingRevived(by) => format!("being Revived by {by}"),
            Tag::Reviving(whom) => format!("Reviving {whom}"),
            Tag::AwaitingRespawn => "waiting to Respawn".to_string(),
            Tag::Paused => "PAUSED".to_string(),
            Tag::Choosing => "choosing a Boon".to_string(),
            Tag::Reconnecting => "Reconnecting… Seat held".to_string(),
        }
    }

    fn colour(&self) -> Color {
        match self {
            Tag::Down(_) => DOWN_RED,
            Tag::BeingRevived(_) | Tag::Reviving(_) => Color::WHITE,
            Tag::Choosing => CHOOSING_PURPLE,
            Tag::AwaitingRespawn | Tag::Paused | Tag::Reconnecting => DIM,
        }
    }
}

/// What a teammate is doing, for choosing their tag.
#[derive(Debug, Default)]
pub struct Standing<'a> {
    /// Their connection dropped and their Seat is held.
    pub held: bool,
    /// Seconds left of their Bleed Out, if they are Downed.
    pub down_left: Option<f32>,
    pub revived_by: Option<&'a str>,
    pub awaiting_respawn: bool,
    pub reviving: Option<&'a str>,
    pub paused: bool,
    pub choosing: bool,
}

/// Whole seconds to show for a time left: counting down to 1 until it's up.
pub fn whole_secs(left: f32) -> u32 {
    left.max(0.0).ceil() as u32
}

/// The one tag a teammate wears: a held Seat first, then being Downed (or Revived), waiting to
/// Respawn, Reviving, Paused, and an open Offer.
pub fn status_tag(standing: &Standing) -> Option<Tag> {
    if standing.held {
        Some(Tag::Reconnecting)
    } else if let Some(left) = standing.down_left {
        Some(match standing.revived_by {
            Some(by) => Tag::BeingRevived(by.to_string()),
            None => Tag::Down(whole_secs(left)),
        })
    } else if standing.awaiting_respawn {
        Some(Tag::AwaitingRespawn)
    } else if let Some(whom) = standing.reviving {
        Some(Tag::Reviving(whom.to_string()))
    } else if standing.paused {
        Some(Tag::Paused)
    } else if standing.choosing {
        Some(Tag::Choosing)
    } else {
        None
    }
}

/// A Weapon waiting at the Box or Pack-a-Punch, as its label reads.
pub fn waiting_label(owner: &str, weapon: &str, left: f32) -> String {
    format!("{owner}'s {weapon} {}", whole_secs(left))
}

/// The local Player's Revive bar caption.
pub fn revive_bar_label(reviving: bool, other: &str) -> String {
    if reviving { format!("Reviving {other}") } else { format!("Being Revived by {other}") }
}

/// The bleeding-out line over the crosshair.
pub fn bleeding_out_label(left: f32) -> String {
    format!("BLEEDING OUT {}", whole_secs(left))
}

/// One teammate's row in the corner stack.
#[derive(Debug, Clone, PartialEq)]
pub struct TeammateRow {
    pub seat: Seat,
    pub name: String,
    pub points: u64,
    pub tag: Option<Tag>,
    /// Their Seat is held while they reconnect, so the row is faded.
    pub held: bool,
}

/// A Downed teammate's revive icon.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviveIcon {
    pub at: Vec3,
    pub seat: Seat,
    pub left: f32,
    /// Who is Reviving them, and how far along.
    pub reviver: Option<(Seat, f32)>,
}

/// A teammate's Weapon waiting at the Box or Pack-a-Punch.
#[derive(Debug, Clone, PartialEq)]
pub struct WaitingWeapon {
    pub at: Vec3,
    pub seat: Seat,
    pub label: String,
}

/// The local Player's Revive, as reviver or as the one being Revived.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalRevive {
    pub label: String,
    pub progress: f32,
    /// The other Player's Seat, whose colour the bar fills in.
    pub other: Seat,
}

/// Everything the Co-op HUD shows, read off the Players.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct CoopView {
    pub rows: Vec<TeammateRow>,
    pub icons: Vec<ReviveIcon>,
    pub waiting: Vec<WaitingWeapon>,
    pub revive: Option<LocalRevive>,
    /// Seconds of the local Player's Bleed Out left, while they are Down.
    pub bleeding: Option<f32>,
}

type PlayerState = (
    Entity,
    &'static Seat,
    &'static PlayerName,
    &'static Points,
    &'static GlobalTransform,
    Option<&'static LastStand>,
    Option<&'static Reviving>,
    Has<AwaitingRespawn>,
    Has<Paused>,
    Has<Choosing>,
    Has<SeatHeld>,
);

pub fn read_coop_view(
    players: Query<PlayerState, With<Player>>,
    local: Single<Entity, With<LocalPlayer>>,
    boxes: Query<(&MysteryBox, &GlobalTransform)>,
    machines: Query<(&PackAPunch, &GlobalTransform)>,
    mut view: ResMut<CoopView>,
) {
    let local = *local;
    let seat_and_name = |player: Entity| players.get(player).ok().map(|(_, seat, name, ..)| (*seat, name.0.as_str()));
    let reviver_of = |target: Entity| {
        players.iter().find_map(|(entity, seat, name, _, _, _, reviving, ..)| {
            reviving.filter(|reviving| reviving.target == target).map(|reviving| (entity, *seat, name.0.as_str(), reviving.progress()))
        })
    };
    let mut next = CoopView::default();
    let mut ordered: Vec<_> = players.iter().collect();
    ordered.sort_by_key(|(_, seat, ..)| seat.0);
    for (entity, &seat, name, points, at, last_stand, reviving, awaiting_respawn, paused, choosing, held) in ordered {
        let coop_stand = last_stand.filter(|stand| stand.is_coop());
        if entity == local {
            next.bleeding = coop_stand.map(LastStand::remaining_secs);
            next.revive = match (reviving.and_then(|r| seat_and_name(r.target)), reviver_of(entity)) {
                (Some((other, whom)), _) => reviving.map(|r| (true, whom, other, r.progress())),
                (None, Some((_, other, by, progress))) => Some((false, by, other, progress)),
                (None, None) => None,
            }
            .map(|(reviving, other_name, other, progress)| LocalRevive {
                label: revive_bar_label(reviving, other_name),
                progress,
                other,
            });
            continue;
        }
        let reviver = reviver_of(entity);
        let standing = Standing {
            held,
            down_left: coop_stand.map(LastStand::remaining_secs),
            revived_by: reviver.map(|(_, _, by, _)| by),
            awaiting_respawn,
            reviving: reviving.and_then(|r| seat_and_name(r.target)).map(|(_, whom)| whom),
            paused,
            choosing,
        };
        next.rows.push(TeammateRow { seat, name: name.0.clone(), points: points.balance, tag: status_tag(&standing), held });
        if let Some(stand) = coop_stand {
            next.icons.push(ReviveIcon {
                at: at.translation() + Vec3::Y * REVIVE_ICON_LIFT,
                seat,
                left: stand.remaining_secs(),
                reviver: reviver.map(|(_, by, _, progress)| (by, progress)),
            });
        }
    }
    let mut wait = |owner: Entity, weapon: &str, left: f32, at: Vec3| {
        if let Some((seat, name)) = seat_and_name(owner).filter(|_| owner != local) {
            next.waiting.push(WaitingWeapon { at, seat, label: waiting_label(name, weapon, left) });
        }
    };
    for (mystery_box, at) in &boxes {
        if let BoxState::Offering { spinner, prize, left, upgraded, .. } = mystery_box.state {
            wait(spinner, prize.shown_name(upgraded), left, at.translation() + Vec3::Y * BOX_LABEL_LIFT);
        }
    }
    for (machine, at) in &machines {
        if let Some(waiting) = machine.waiting()
            && let Some(owner) = waiting.owner
        {
            wait(owner, waiting.weapon, waiting.left, at.translation() + Vec3::Y * PAP_LABEL_LIFT);
        }
    }
    if *view != next {
        *view = next;
    }
}

/// The corner stack of teammates' rows, the first child of the bottom-right column.
#[derive(Component)]
pub struct TeammateStack;

/// Everything the Co-op HUD floats over the world and the middle of the screen.
#[derive(Component)]
pub struct CoopLayer;

pub fn spawn_layer(commands: &mut Commands) {
    commands.spawn((CoopLayer, super::InRun, full_screen()));
}

pub fn stack_node() -> impl Bundle {
    (TeammateStack, Node { display: Display::None, ..column(2.0, AlignItems::FlexEnd) })
}

/// Whether the Run is Co-op, so the Co-op HUD has anything to show.
pub fn is_coop(run_type: Option<Res<RunType>>) -> bool {
    run_type.is_some_and(|run_type| run_type.is_coop())
}

fn bar(ui: &mut ChildSpawnerCommands, width: f32, height: f32, fill: f32, colour: Color) {
    ui.spawn((Node { width: px(width), height: px(height), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6))))
        .with_child((
            Node { width: percent(100.0 * fill.clamp(0.0, 1.0)), height: percent(100.0), ..default() },
            BackgroundColor(colour),
        ));
}

/// A spot on screen, anchored by its top-left corner, whose children stack centred.
fn at(pos: Vec2) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(pos.x),
        top: px(pos.y),
        ..column(0.0, AlignItems::Center)
    }
}

pub fn draw_coop(
    mut commands: Commands,
    view: Res<CoopView>,
    layer: Single<Entity, With<CoopLayer>>,
    mut stack: Single<(Entity, &mut Node), With<TeammateStack>>,
    camera: Single<(&Camera, &GlobalTransform), With<PlayerCamera>>,
) {
    let (camera, eye) = *camera;
    // On screen, or `None` when behind the camera.
    let project = |at: Vec3| {
        let ahead = (at - eye.translation()).dot(*eye.forward()) > 0.0;
        if ahead { camera.world_to_viewport(eye, at).ok() } else { None }
    };
    stack.1.display = if view.rows.is_empty() { Display::None } else { Display::Flex };
    commands.entity(stack.0).despawn_children().with_children(|stack| {
        for teammate in &view.rows {
            let fade = |colour: Color| if teammate.held { colour.with_alpha(HELD_ROW_ALPHA) } else { colour };
            let seat_colour = fade(teammate.seat.colour());
            stack.spawn(row(8.0)).with_children(|row| {
                if let Some(tag) = &teammate.tag {
                    row.spawn(text(tag.text(), 13.0, fade(tag.colour())));
                }
                row.spawn(text(teammate.name.clone(), 14.0, seat_colour));
                row.spawn(text(with_commas(teammate.points), 24.0, seat_colour));
            });
        }
    });
    commands.entity(*layer).despawn_children().with_children(|ui| {
        for icon in &view.icons {
            let Some(pos) = project(icon.at) else { continue };
            let square = if icon.reviver.is_some() { Color::WHITE } else { DOWN_RED };
            ui.spawn(at(pos - Vec2::new(30.0, 20.0))).with_children(|icon_ui| {
                icon_ui
                    .spawn((
                        Node {
                            width: px(22.0),
                            height: px(22.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(square),
                    ))
                    .with_child(text("+", 20.0, icon.seat.colour()));
                icon_ui.spawn(text(whole_secs(icon.left).to_string(), 14.0, square));
                if let Some((by, progress)) = icon.reviver {
                    bar(icon_ui, 40.0, 4.0, progress, by.colour());
                }
            });
        }
        for waiting in &view.waiting {
            let Some(pos) = project(waiting.at) else { continue };
            ui.spawn(at(pos - Vec2::new(70.0, 0.0))).with_child(text(waiting.label.clone(), 15.0, waiting.seat.colour()));
        }
        if let Some(revive) = &view.revive {
            ui.spawn(Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(62.0),
                ..column(4.0, AlignItems::Center)
            })
            .with_children(|under| {
                under.spawn(text(revive.label.clone(), 18.0, Color::WHITE));
                bar(under, 220.0, 8.0, revive.progress, revive.other.colour());
            });
        }
        if let Some(left) = view.bleeding {
            ui.spawn(Node {
                position_type: PositionType::Absolute,
                width: percent(100.0),
                top: percent(30.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .with_child(text(bleeding_out_label(left), 28.0, DOWN_RED));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_teammate_with_nothing_going_on_wears_no_tag() {
        assert_eq!(status_tag(&Standing::default()), None);
    }

    #[test]
    fn a_downed_teammate_counts_down_their_bleed_out() {
        let standing = Standing { down_left: Some(22.4), ..default() };
        assert_eq!(status_tag(&standing).unwrap().text(), "DOWN 23s");
    }

    #[test]
    fn a_revive_in_progress_names_the_reviver_and_the_revived() {
        let downed = Standing { down_left: Some(30.0), revived_by: Some("Nikolai"), ..default() };
        assert_eq!(status_tag(&downed).unwrap().text(), "being Revived by Nikolai");
        let reviver = Standing { reviving: Some("Takeo"), ..default() };
        assert_eq!(status_tag(&reviver).unwrap().text(), "Reviving Takeo");
    }

    #[test]
    fn paused_choosing_and_respawning_have_their_own_tags() {
        assert_eq!(status_tag(&Standing { paused: true, ..default() }).unwrap().text(), "PAUSED");
        assert_eq!(status_tag(&Standing { choosing: true, ..default() }).unwrap().text(), "choosing a Boon");
        assert_eq!(status_tag(&Standing { awaiting_respawn: true, ..default() }).unwrap().text(), "waiting to Respawn");
    }

    #[test]
    fn a_held_seat_reads_reconnecting_over_anything_else() {
        let standing = Standing { held: true, paused: true, down_left: Some(10.0), ..default() };
        assert_eq!(status_tag(&standing).unwrap().text(), "Reconnecting… Seat held");
    }

    #[test]
    fn being_downed_outranks_being_paused_or_choosing() {
        let standing = Standing { down_left: Some(5.0), paused: true, choosing: true, ..default() };
        assert_eq!(status_tag(&standing), Some(Tag::Down(5)));
    }

    #[test]
    fn times_round_up_and_stop_at_zero() {
        assert_eq!(whole_secs(0.2), 1);
        assert_eq!(whole_secs(9.0), 9);
        assert_eq!(whole_secs(-1.0), 0);
    }

    #[test]
    fn the_labels_read_as_the_design_says() {
        assert_eq!(waiting_label("Nikolai", "Ray Gun", 8.5), "Nikolai's Ray Gun 9");
        assert_eq!(revive_bar_label(true, "Takeo"), "Reviving Takeo");
        assert_eq!(revive_bar_label(false, "Nikolai"), "Being Revived by Nikolai");
        assert_eq!(bleeding_out_label(24.1), "BLEEDING OUT 25");
    }
}
