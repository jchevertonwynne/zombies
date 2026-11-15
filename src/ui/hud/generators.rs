//! A row of Generator marks along the top left, on a Map with Generators: each Unclaimed, Charging
//! with a meter, Held, or Lost.

use bevy::prelude::*;

use crate::enemies::target::half_height_above_feet;
use crate::maps::generator::{GeneratorState, Generators, on_plates};
use crate::players::player::{Locomotion, LocalPlayer};
use crate::maps::map::MapId;
use crate::state::{GameState, InRun};

/// What a Generator's mark shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    Unclaimed,
    /// With how full its meter is, and whether it's filling now.
    Charging { fill: f32, live: bool },
    Held,
    Lost,
}

/// A Generator's mark, from its state.
pub fn mark(state: GeneratorState) -> Mark {
    match state {
        GeneratorState::Unclaimed => Mark::Unclaimed,
        GeneratorState::Charging { live, .. } => Mark::Charging { fill: state.fraction(), live },
        GeneratorState::Held => Mark::Held,
        GeneratorState::Lost => Mark::Lost,
    }
}

/// Where the top-left corner of the HUD starts, in pixels in from the edges.
pub const CORNER_PX: f32 = 16.0;
/// How tall the Generator row is, fixed so what sits under it can start below it.
const ROW_HEIGHT_PX: f32 = 72.0;
const ROW_GAP_PX: f32 = 8.0;

/// How far down the top-left column the Gauntlet's HUD starts on `map`: under the Generator row if
/// the Map has one, at the corner if not.
pub fn corner_below_generators(map: MapId) -> f32 {
    if map.layout().generators.is_empty() { CORNER_PX } else { CORNER_PX + ROW_HEIGHT_PX + ROW_GAP_PX }
}

const LAMP_PX: f32 = 22.0;
const METER_PX: f32 = 64.0;
const UNCLAIMED_GREY: Color = Color::srgba(0.35, 0.35, 0.35, 0.8);
const CHARGING_AMBER: Color = Color::srgb(1.0, 0.65, 0.1);
const PAUSED_AMBER: Color = Color::srgb(0.55, 0.36, 0.08);
const HELD_GREEN: Color = Color::srgb(0.35, 0.95, 0.45);
const LOST_RED: Color = Color::srgb(0.9, 0.15, 0.12);

impl Mark {
    fn colour(self) -> Color {
        match self {
            Mark::Unclaimed => UNCLAIMED_GREY,
            Mark::Charging { live: true, .. } => CHARGING_AMBER,
            Mark::Charging { live: false, .. } => PAUSED_AMBER,
            Mark::Held => HELD_GREEN,
            Mark::Lost => LOST_RED,
        }
    }
}

/// The lamp of the Generator at this place in `Layout.generators`.
#[derive(Component)]
struct Lamp(usize);

/// The meter under a lamp, shown while its Generator Charges, and its fill.
#[derive(Component)]
struct Meter(usize);

#[derive(Component)]
struct Fill(usize);

/// The "ON PLATE" line under a lamp, shown while the local Player stands on that plate.
#[derive(Component)]
struct OnPlate(usize);

const ON_PLATE_AMBER: Color = Color::srgb(1.0, 0.8, 0.3);

pub struct GeneratorHudPlugin;

impl Plugin for GeneratorHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_row)
            .add_systems(Update, show_on_plate.run_if(in_state(GameState::Playing)).run_if(resource_exists::<Generators>))
            .add_systems(Update, show_marks.run_if(in_state(GameState::Playing)).run_if(resource_exists::<Generators>));
    }
}

fn spawn_row(mut commands: Commands, map: Res<MapId>) {
    let generators = map.layout().generators;
    if generators.is_empty() {
        return;
    }
    commands
        .spawn((
            InRun,
            Node {
                position_type: PositionType::Absolute,
                left: px(CORNER_PX),
                top: px(CORNER_PX),
                height: px(ROW_HEIGHT_PX),
                column_gap: px(14.0),
                ..default()
            },
        ))
        .with_children(|row| {
            for (index, generator) in generators.iter().enumerate() {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(4.0),
                    ..default()
                })
                .with_children(|mark| {
                    mark.spawn((
                        Lamp(index),
                        Node {
                            width: px(LAMP_PX),
                            height: px(LAMP_PX),
                            border: UiRect::all(px(2.0)),
                            ..default()
                        },
                        BackgroundColor(UNCLAIMED_GREY),
                        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                    ));
                    mark.spawn(super::text(generator.section.name().to_uppercase(), 11.0, Color::srgba(0.9, 0.9, 0.9, 0.8)));
                    mark.spawn((
                        OnPlate(index),
                        Node { display: Display::None, ..default() },
                        super::text("ON PLATE", 11.0, ON_PLATE_AMBER),
                    ));
                    mark.spawn((
                        Meter(index),
                        Node {
                            display: Display::None,
                            width: px(METER_PX),
                            height: px(5.0),
                            border: UiRect::all(px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
                        BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.8)),
                    ))
                    .with_child((
                        Fill(index),
                        Node {
                            width: percent(0.0),
                            height: percent(100.0),
                            ..default()
                        },
                        BackgroundColor(CHARGING_AMBER),
                    ));
                });
            }
        });
}

/// Shows "ON PLATE" under the lamps of the plate the local Player stands on, and hides it when
/// they step off.
fn show_on_plate(
    generators: Res<Generators>,
    player: Query<(&Transform, Option<&Locomotion>), With<LocalPlayer>>,
    mut lines: Query<(&OnPlate, &mut Node)>,
) {
    let feet: Vec<Vec3> =
        player.iter().map(|(transform, locomotion)| transform.translation - Vec3::Y * half_height_above_feet(locomotion)).collect();
    let on = on_plates(generators.layout(), &feet);
    for (&OnPlate(index), mut node) in &mut lines {
        let display = if on.get(index).copied().unwrap_or(false) { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

fn show_marks(
    generators: Res<Generators>,
    mut lamps: Query<(&Lamp, &mut BackgroundColor), (Without<Fill>, Without<Meter>)>,
    mut meters: Query<(&Meter, &mut Node), Without<Fill>>,
    mut fills: Query<(&Fill, &mut Node, &mut BackgroundColor), Without<Meter>>,
) {
    let mark_of = |index: usize| generators.states().get(index).map(|&state| mark(state));
    for (&Lamp(index), mut colour) in &mut lamps {
        if let Some(mark) = mark_of(index) {
            colour.set_if_neq(BackgroundColor(mark.colour()));
        }
    }
    for (&Meter(index), mut node) in &mut meters {
        let display = if matches!(mark_of(index), Some(Mark::Charging { .. })) { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    for (&Fill(index), mut node, mut colour) in &mut fills {
        if let Some(mark @ Mark::Charging { fill, .. }) = mark_of(index) {
            node.width = percent(fill * 100.0);
            colour.set_if_neq(BackgroundColor(mark.colour()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gauntlet_column_starts_below_the_generator_row_and_the_row_fits_its_height() {
        assert_eq!(corner_below_generators(MapId::Greybox), CORNER_PX);
        let below = corner_below_generators(MapId::Dam);
        assert!(below >= CORNER_PX + ROW_HEIGHT_PX + ROW_GAP_PX);
        // Lamp, its name and "ON PLATE" at 11 px (about 14 px a line), its meter, and the gaps between them.
        let content = LAMP_PX + 4.0 + 14.0 + 4.0 + 14.0 + 4.0 + 5.0 + 2.0;
        assert!(content <= ROW_HEIGHT_PX, "the row's contents are {content} px tall");
    }

    #[test]
    fn each_state_has_its_mark_and_a_charge_shows_how_far_it_has_come() {
        assert_eq!(mark(GeneratorState::Unclaimed), Mark::Unclaimed);
        assert_eq!(mark(GeneratorState::Held), Mark::Held);
        assert_eq!(mark(GeneratorState::Lost), Mark::Lost);
        assert_eq!(
            mark(GeneratorState::Charging { charged: 5.0, needed: 20.0, live: false }),
            Mark::Charging { fill: 0.25, live: false }
        );
        assert_eq!(
            mark(GeneratorState::Charging { charged: 15.0, needed: 20.0, live: true }),
            Mark::Charging { fill: 0.75, live: true }
        );
    }
}
