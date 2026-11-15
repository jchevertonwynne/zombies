//! Doors: bought from either side with the Use key, then open for the rest of the Run.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::authority::HostSimulation;
use crate::economy::buy::{Buyable, Purchased};
use crate::maps::power::Power;
use crate::maps::teleporter::Teleporter;
use crate::state::world_runs;
use crate::tuning::*;

/// A Door in the Map. While closed it is solid to the Player and to Zombies alike.
#[derive(Component)]
pub struct Door {
    /// The two Zones it joins. The Gazebo's Door joins the Yard to itself, since the Gazebo is
    /// in the Yard.
    pub joins: Option<(crate::maps::zone::ZoneId, crate::maps::zone::ZoneId)>,
    /// `None` for a Power Door, which can't be bought.
    pub price: Option<u64>,
    pub open: bool,
    /// For the gate Door of a Swing Bridge: the two Zones whose landings it guards. It joins
    /// nothing itself (`joins` is `None`); the bridge's Rest does, once this is open.
    pub gate: Option<(crate::maps::zone::ZoneId, crate::maps::zone::ZoneId)>,
}

impl Door {
    /// The pair of Zones planners treat this Door as the way between once it's open: what it
    /// joins, or for a gate Door the Zones it guards.
    pub fn route(&self) -> Option<(crate::maps::zone::ZoneId, crate::maps::zone::ZoneId)> {
        self.joins.or(self.gate)
    }
}

/// A Door that can't be bought and waits for the Teleporter, not the Power: it opens once every
/// Teleporter Pad is Linked. The Gazebo's, so Pack-a-Punch is earned in the south wing.
#[derive(Component)]
pub struct OpensOnLinks;

/// What a Door that `OpensOnLinks` says while it's shut.
pub const LINKED_DOOR_PROMPT: &str = "Link both Teleporter Pads to open";

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
            (
                price_doors,
                open_bought_doors.in_set(HostSimulation),
                open_power_doors.in_set(HostSimulation),
                open_linked_doors.in_set(HostSimulation),
                slide_open_doors,
            )
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
    mut doors: Query<(Entity, &mut Door, &Transform), Without<OpensOnLinks>>,
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

/// Doors that wait for the Teleporter open on their own once every Pad is Linked.
fn open_linked_doors(
    mut commands: Commands,
    teleporter: Option<Res<Teleporter>>,
    mut doors: Query<(Entity, &mut Door, &Transform), With<OpensOnLinks>>,
) {
    if !teleporter.is_some_and(|teleporter| teleporter.both_linked()) {
        return;
    }
    for (entity, mut door, transform) in &mut doors {
        if !door.open {
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

    /// The Gazebo stays shut through the Power and the first Link, and opens with the second, so
    /// Pack-a-Punch on the Greybox is earned in the south wing. Until then its Prompt says why.
    #[test]
    fn the_gazebo_opens_once_both_pads_are_linked_and_not_before() {
        use crate::economy::buy::Refused;
        use crate::enemies::zombie::headless::Sim;
        use crate::maps::teleporter::PadId;

        let mut sim = Sim::new(60.0, |app| {
            app.add_message::<Purchased>().init_resource::<Power>().init_resource::<Teleporter>().add_plugins(DoorPlugin);
        });
        let gazebo = |sim: &mut Sim| {
            let world = sim.world_mut();
            let (door, buyable, refused) = world
                .query_filtered::<(&Door, Option<&Buyable>, Has<Refused>), With<OpensOnLinks>>()
                .single(world)
                .expect("the Greybox has one Door that waits for the Teleporter");
            (door.open, buyable.map(|buyable| buyable.prompt.clone()), refused)
        };
        let shut = (false, Some(LINKED_DOOR_PROMPT.to_string()), true);
        assert_eq!(gazebo(&mut sim), shut);

        sim.world_mut().resource_mut::<Power>().switch_on();
        sim.run(2);
        assert_eq!(gazebo(&mut sim), shut, "the Power alone doesn't open the Gazebo");

        let link = |sim: &mut Sim, pad| {
            let mut teleporter = sim.world_mut().resource_mut::<Teleporter>();
            teleporter.activate(pad);
            teleporter.link();
            sim.run(2);
        };
        link(&mut sim, PadId::Workshop);
        assert_eq!(gazebo(&mut sim), shut, "one Link isn't enough");
        link(&mut sim, PadId::Generator);
        let (open, prompt, _) = gazebo(&mut sim);
        assert!(open && prompt.is_none(), "both Links open the Gazebo and take its Prompt away");
    }

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
