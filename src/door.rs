//! Doors: bought from either side with the Use key, then open for the rest of the Run.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::buy::{Buyable, Purchased};
use crate::power::Power;
use crate::state::Pause;

/// A Door in the Map. While closed it is solid to the Player and to Zombies alike.
#[derive(Component)]
pub struct Door {
    /// The two Zones it joins. The Gazebo's Power Door joins none, since the Gazebo is in the Yard.
    pub joins: Option<(crate::zone::ZoneId, crate::zone::ZoneId)>,
    /// `None` for a Power Door, which can't be bought.
    pub price: Option<u64>,
    pub open: bool,
}

pub struct DoorPlugin;

impl Plugin for DoorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (price_doors, open_bought_doors, open_power_doors).run_if(in_state(Pause::Running)),
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
    mut doors: Query<(&mut Door, &mut Visibility)>,
) {
    for Purchased(entity) in purchases.read() {
        if let Ok((mut door, mut visibility)) = doors.get_mut(*entity) {
            open(&mut commands, *entity, &mut door, &mut visibility);
        }
    }
}

/// Power Doors open on their own when the Power comes on.
fn open_power_doors(
    mut commands: Commands,
    power: Res<Power>,
    mut doors: Query<(Entity, &mut Door, &mut Visibility)>,
) {
    if !power.is_on() {
        return;
    }
    for (entity, mut door, mut visibility) in &mut doors {
        if door.price.is_none() && !door.open {
            open(&mut commands, entity, &mut door, &mut visibility);
        }
    }
}

/// Opens a Door for the rest of the Run: it vanishes and stops being solid.
fn open(commands: &mut Commands, entity: Entity, door: &mut Door, visibility: &mut Visibility) {
    door.open = true;
    *visibility = Visibility::Hidden;
    commands
        .entity(entity)
        .remove::<(Buyable, RigidBody, Collider, CollisionLayers)>();
}
