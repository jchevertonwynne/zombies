//! The Quarantine's text (#524), for the Player inside the Wing that is out and no one else:
//! "Quarantine: the West Wing seals in 7" through Warning, and "Sealed: push a Crash Bar to get out"
//! while Sealed. There is no timer for anyone else; like the Tide it stays diegetic. It reads the
//! `Quarantine` stage every machine shows, so a client and a late joiner see it too.

use bevy::prelude::*;

use crate::enemies::nav::zone_at;
use crate::enemies::target::half_height_above_feet;
use crate::maps::map::MapId;
use crate::maps::quarantine::{Quarantine, Stage, wing_of_zone};
use crate::maps::zone::ZoneId;
use crate::players::player::{LocalPlayer, Locomotion};
use crate::state::{GameState, InRun};
use crate::tuning::QUARANTINE_WARNING_SECS;

const WARNING_RED: Color = Color::srgb(1.0, 0.25, 0.18);

/// What the Quarantine says to a Player standing in `zone` on `map`, if anything: only to one inside
/// the Wing that is out.
pub fn stage_text(map: MapId, quarantine: &Quarantine, zone: Option<ZoneId>) -> Option<String> {
    let wing = quarantine.wing()?;
    if wing_of_zone(map, zone?) != Some(wing) {
        return None;
    }
    match quarantine.stage() {
        Stage::Warning => {
            let name = map.layout().wings.get(usize::from(wing.0))?.name;
            let left = (QUARANTINE_WARNING_SECS - quarantine.elapsed()).ceil().max(1.0) as u32;
            Some(format!("Quarantine: the {name} seals in {left}"))
        }
        Stage::Sealed => Some("Sealed: push a Crash Bar to get out".to_string()),
        _ => None,
    }
}

#[derive(Component)]
struct QuarantineText;

fn spawn_text(mut commands: Commands) {
    commands
        .spawn((
            InRun,
            Node { position_type: PositionType::Absolute, width: percent(100.0), top: percent(14.0), justify_content: JustifyContent::Center, ..default() },
        ))
        .with_child((QuarantineText, super::text("", 34.0, WARNING_RED), Visibility::Hidden));
}

fn show_text(
    quarantine: Res<Quarantine>,
    map: Res<MapId>,
    player: Query<(&Transform, Option<&Locomotion>), With<LocalPlayer>>,
    mut line: Single<(&mut Text, &mut Visibility), With<QuarantineText>>,
) {
    let zone = player.single().ok().and_then(|(transform, locomotion)| zone_at(*map, transform.translation - Vec3::Y * half_height_above_feet(locomotion)));
    let (text, visibility) = &mut *line;
    match stage_text(*map, &quarantine, zone) {
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

pub struct QuarantineHudPlugin;

impl Plugin for QuarantineHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(GameState::Playing), spawn_text)
            .add_systems(Update, show_text.run_if(in_state(GameState::Playing)).run_if(resource_exists::<Quarantine>).run_if(resource_exists::<MapId>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::quarantine::WingId;

    fn showing(stage: Stage, wing: Option<u8>, elapsed: f32) -> Quarantine {
        let mut quarantine = Quarantine::default();
        quarantine.show(stage, wing.map(WingId), elapsed);
        quarantine
    }

    #[test]
    fn stage_text_shows_only_to_players_inside_the_wing() {
        let map = MapId::Sanatorium;
        let warning = showing(Stage::Warning, Some(0), 2.5);
        // Inside the West Wing (the Long Ward and the Day Room): the countdown.
        for zone in [ZoneId::LongWard, ZoneId::DayRoom] {
            assert_eq!(stage_text(map, &warning, Some(zone)).as_deref(), Some("Quarantine: the West Wing seals in 8"), "{zone:?}");
        }
        // Anywhere else: nothing. The corridor outside, another Wing, and out of doors.
        for zone in [Some(ZoneId::WestCorridor), Some(ZoneId::Refectory), Some(ZoneId::BathHall), Some(ZoneId::IsolationWard), Some(ZoneId::Forecourt), None] {
            assert_eq!(stage_text(map, &warning, zone), None, "{zone:?}");
        }
        // The count runs down to 1, never 0.
        assert_eq!(stage_text(map, &showing(Stage::Warning, Some(1), 9.9), Some(ZoneId::BathHall)).as_deref(), Some("Quarantine: the East Wing seals in 1"));
        assert_eq!(stage_text(map, &showing(Stage::Warning, Some(0), 0.0), Some(ZoneId::LongWard)).as_deref(), Some("Quarantine: the West Wing seals in 10"));
        // Sealed: the way out, to the same Players only.
        let sealed = showing(Stage::Sealed, Some(2), 5.0);
        assert_eq!(stage_text(map, &sealed, Some(ZoneId::ChildrensWard)).as_deref(), Some("Sealed: push a Crash Bar to get out"));
        assert_eq!(stage_text(map, &sealed, Some(ZoneId::LongWard)), None);
        assert_eq!(stage_text(map, &sealed, Some(ZoneId::SunRoof)), None);
        // Every other stage says nothing to anyone.
        for stage in [Stage::Dormant, Stage::AllClear, Stage::Sealing, Stage::Release, Stage::Opening] {
            for zone in [ZoneId::LongWard, ZoneId::DayRoom] {
                assert_eq!(stage_text(map, &showing(stage, Some(0), 1.0), Some(zone)), None, "{stage:?}");
            }
        }
    }
}
