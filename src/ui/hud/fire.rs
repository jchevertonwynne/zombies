//! The Fire's warning text (#572, from #507's "Warning cues"): "Gas! Get clear of the Rack Line" to a Player whose feet are in that line's
//! Blaze while it Flares up, and to no one else. There is no timer; like the Quarantine's text it stays diegetic. It reads the `FuelLines`
//! every machine shows, so a client and a late joiner see it too.

use bevy::prelude::*;

use crate::enemies::target::half_height_above_feet;
use crate::maps::fire::{FuelLines, LinePhase, LitMask, blaze_at};
use crate::maps::map::{Layout, MapId};
use crate::players::player::{LocalPlayer, Locomotion};
use crate::state::{GameState, InRun};

const WARNING_AMBER: Color = Color::srgb(1.0, 0.7, 0.12);

/// What the Fire says to a Player with their feet at `feet`, if anything: only to one inside the Blaze of a line that is Flaring up.
pub fn gas_text(layout: &Layout, lines: &FuelLines, feet: Vec3) -> Option<String> {
    let flaring = (0..lines.count() as u8)
        .map(crate::maps::fire::FuelLineId)
        .filter(|&line| matches!(lines.phase(line), LinePhase::FlareUp { .. }))
        .fold(LitMask::NONE, LitMask::with);
    let line = blaze_at(layout, flaring, feet)?;
    Some(format!("Gas! Get clear of the {}", layout.fuel_lines.get(usize::from(line.0))?.name))
}

#[derive(Component)]
struct GasText;

fn spawn_text(mut commands: Commands) {
    commands
        .spawn((InRun, Node { position_type: PositionType::Absolute, width: percent(100.0), top: percent(14.0), justify_content: JustifyContent::Center, ..default() }))
        .with_child((GasText, super::text("", 34.0, WARNING_AMBER), Visibility::Hidden));
}

fn show_text(
    lines: Res<FuelLines>,
    map: Res<MapId>,
    player: Query<(&Transform, Option<&Locomotion>), With<LocalPlayer>>,
    mut line: Single<(&mut Text, &mut Visibility), With<GasText>>,
) {
    let feet = player.single().ok().map(|(transform, locomotion)| transform.translation - Vec3::Y * half_height_above_feet(locomotion));
    let (text, visibility) = &mut *line;
    match feet.and_then(|feet| gas_text(map.layout(), &lines, feet)) {
        Some(words) => {
            if text.0 != words {
                text.0 = words;
            }
            visibility.set_if_neq(Visibility::Inherited);
        }
        None => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

pub struct FireHudPlugin;

impl Plugin for FireHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_text)
            .add_systems(Update, show_text.run_if(in_state(GameState::Playing)).run_if(resource_exists::<FuelLines>).run_if(resource_exists::<MapId>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::fire::FuelLineId;

    fn flaring(line: u8) -> FuelLines {
        let mut lines = FuelLines::start(6);
        lines.show(FuelLineId(line), LinePhase::FlareUp { elapsed: 1.0 });
        lines
    }

    #[test]
    fn gas_text_shows_only_to_a_player_in_that_lines_blaze_during_flare_up() {
        let layout = MapId::Refinery.layout();
        let in_blaze = |line: usize, blaze: usize| layout.fuel_lines[line].blazes[blaze].centre();
        // The Rack Line's two Blazes, the Bund Line's and the Furnace Blaze: each says its own line's name.
        for (line, blaze, name) in [(0, 0, "Rack Line"), (0, 1, "Rack Line"), (1, 0, "Bund Line"), (4, 0, "Furnace Line")] {
            let lines = flaring(line as u8);
            assert_eq!(gas_text(layout, &lines, in_blaze(line, blaze)).as_deref(), Some(format!("Gas! Get clear of the {name}").as_str()), "{name}");
        }
        let lines = flaring(0);
        // Outside the Blaze, even beside it, and in another line's Blaze while only this one flares: nothing.
        assert_eq!(gas_text(layout, &lines, in_blaze(0, 0) + Vec3::new(6.0, 0.0, 0.0)), None);
        assert_eq!(gas_text(layout, &lines, in_blaze(1, 0)), None);
        // Above the Blaze, on another floor: nothing.
        assert_eq!(gas_text(layout, &lines, in_blaze(0, 0) + Vec3::Y * 4.0), None);
        // Lit, Shut and held-by-Sabotage lines say nothing in their own Blaze.
        let lit = FuelLines::start(6);
        assert_eq!(gas_text(layout, &lit, in_blaze(0, 0)), None);
        let mut shut = FuelLines::start(6);
        shut.show(FuelLineId(0), LinePhase::Shut { elapsed: 10.0 });
        assert_eq!(gas_text(layout, &shut, in_blaze(0, 0)), None);
        // Another Map has no Blazes.
        assert_eq!(gas_text(MapId::Greybox.layout(), &FuelLines::default(), Vec3::ZERO), None);
    }
}
