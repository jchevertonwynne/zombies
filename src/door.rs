//! Doors: bought from either side with the Use key, then open for the rest of the Run.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::buy::{Buyable, Purchased};
use crate::power::Power;
use crate::state::world_runs;
use crate::tuning::*;

/// A Door in the Map. While closed it is solid to the Player and to Zombies alike.
#[derive(Component)]
pub struct Door {
    /// The two Zones it joins. The Gazebo's Power Door joins the Yard to itself, since the Gazebo is
    /// in the Yard.
    pub joins: Option<(crate::zone::ZoneId, crate::zone::ZoneId)>,
    /// `None` for a Power Door, which can't be bought.
    pub price: Option<u64>,
    pub open: bool,
}

/// A Door that has just been opened: it shakes, sinks into the floor, and is then hidden. It is
/// already open and not solid; this is only for show.
#[derive(Component)]
struct Opening {
    elapsed: f32,
    from: Vec3,
}

/// Where an opening Door is after `elapsed` seconds, as an offset from where it stood: a
/// sideways shake, then a sink. `None` once it's out of sight.
pub fn opening_offset(elapsed: f32) -> Option<Vec3> {
    let sinking = elapsed - DOOR_SHAKE_SECS;
    if sinking >= DOOR_SINK_SECS {
        None
    } else if sinking < 0.0 {
        let shake = (elapsed * 90.0).sin() * DOOR_SHAKE_AMPLITUDE;
        Some(Vec3::new(shake, 0.0, shake))
    } else {
        Some(Vec3::NEG_Y * (sinking / DOOR_SINK_SECS * DOOR_SINK_DEPTH))
    }
}

pub struct DoorPlugin;

impl Plugin for DoorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (price_doors, open_bought_doors.in_set(HostSimulation), open_power_doors.in_set(HostSimulation), slide_open_doors)
                .chain()
                .run_if(world_runs),
        );
    }
}

/// Doors own their Buyables, so they keep the price and Prompt current.
fn price_doors(mut doors: Query<(&Door, &mut Buyable)>) {
    for (door, mut buyable) in &mut doors {
        if buyable.price != door.price {
            buyable.price = door.price;
        }
        if buyable.prompt.is_empty() {
            buyable.prompt = "Press F to open Door".into();
        }
    }
}

fn open_bought_doors(
    mut commands: Commands,
    mut purchases: MessageReader<Purchased>,
    mut doors: Query<(&mut Door, &Transform)>,
) {
    for &Purchased { buyable, .. } in purchases.read() {
        if let Ok((mut door, transform)) = doors.get_mut(buyable) {
            open(&mut commands, buyable, &mut door, transform);
        }
    }
}

/// Power Doors open on their own when the Power comes on.
fn open_power_doors(
    mut commands: Commands,
    power: Res<Power>,
    mut doors: Query<(Entity, &mut Door, &Transform)>,
) {
    if !power.is_on() {
        return;
    }
    for (entity, mut door, transform) in &mut doors {
        if door.price.is_none() && !door.open {
            open(&mut commands, entity, &mut door, transform);
        }
    }
}

/// Opens a Door for the rest of the Run: it stops being solid at once, then shakes and sinks away.
pub(crate) fn open(commands: &mut Commands, entity: Entity, door: &mut Door, transform: &Transform) {
    door.open = true;
    commands
        .entity(entity)
        .remove::<(Buyable, RigidBody, Collider, CollisionLayers)>()
        .insert(Opening {
            elapsed: 0.0,
            from: transform.translation,
        });
}

fn slide_open_doors(
    mut commands: Commands,
    mut doors: Query<(Entity, &mut Opening, &mut Transform, &mut Visibility)>,
    time: Res<Time>,
) {
    for (entity, mut opening, mut transform, mut visibility) in &mut doors {
        opening.elapsed += time.delta_secs();
        match opening_offset(opening.elapsed) {
            Some(offset) => transform.translation = opening.from + offset,
            None => {
                *visibility = Visibility::Hidden;
                commands.entity(entity).remove::<Opening>();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_door_shakes_in_place_then_sinks_and_is_gone() {
        let shaking = opening_offset(DOOR_SHAKE_SECS / 2.0).unwrap();
        assert_eq!(shaking.y, 0.0);
        assert!(shaking.x.abs() <= DOOR_SHAKE_AMPLITUDE);
        let halfway = opening_offset(DOOR_SHAKE_SECS + DOOR_SINK_SECS / 2.0).unwrap();
        assert!((halfway.y + DOOR_SINK_DEPTH / 2.0).abs() < 1e-4);
        assert_eq!(halfway.x, 0.0);
        assert!(opening_offset(DOOR_SHAKE_SECS + DOOR_SINK_SECS - 0.01).unwrap().y > -DOOR_SINK_DEPTH);
        assert_eq!(opening_offset(DOOR_SHAKE_SECS + DOOR_SINK_SECS), None);
    }
}
