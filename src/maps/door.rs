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

/// Open every Door in the Map now, free, whatever it waits for: the Zones they join follow. The
/// Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAllDoors;

pub struct DoorPlugin;

impl Plugin for DoorPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenAllDoors>().add_systems(
            Update,
            (
                price_doors,
                open_every_door.in_set(HostSimulation),
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

/// An `OpenAllDoors` opens every Door still shut.
fn open_every_door(
    mut commands: Commands,
    mut asked: MessageReader<OpenAllDoors>,
    mut doors: Query<(Entity, &mut Door, &Transform)>,
) {
    if asked.read().count() == 0 {
        return;
    }
    for (entity, mut door, transform) in &mut doors {
        if !door.open {
            open(&mut commands, entity, &mut door, transform);
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

    /// Whether `entity` is drawn: nothing from it up to the root is hidden.
    fn shown(world: &World, mut entity: Entity) -> bool {
        loop {
            if world.get::<Visibility>(entity).is_some_and(|visibility| *visibility == Visibility::Hidden) {
                return false;
            }
            match world.get::<ChildOf>(entity) {
                Some(parent) => entity = parent.parent(),
                None => return true,
            }
        }
    }

    /// Once every Door is open and out of sight, no drawn piece without a collider is left across
    /// a doorway, on any Map.
    #[test]
    fn nothing_drawn_without_a_collider_fills_an_open_doorway() {
        use crate::enemies::zombie::headless::Sim;
        use crate::maps::map::MapId;
        for map in MapId::ALL {
            let mut sim = Sim::on(map, 60.0, |app| {
                app.add_message::<Purchased>().init_resource::<Power>().init_resource::<Teleporter>().add_plugins(DoorPlugin);
            });
            let doors: Vec<Vec3> = {
                let world = sim.world_mut();
                world.query::<(&Door, &Transform)>().iter(world).map(|(_, transform)| transform.translation).collect()
            };
            sim.send(OpenAllDoors);
            sim.run(120);
            let world = sim.world_mut();
            let mut bad = Vec::new();
            let mut query = world.query::<(Entity, &Mesh3d, &GlobalTransform, Option<&Collider>)>();
            let meshes = world.resource::<Assets<Mesh>>();
            for (entity, mesh, global, collider) in query.iter(world) {
                if collider.is_some() || !shown(world, entity) {
                    continue;
                }
                let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) = meshes.get(&mesh.0).and_then(|mesh| mesh.attribute(Mesh::ATTRIBUTE_POSITION)) else { continue };
                let matrix = global.affine();
                let corners: Vec<Vec3> = positions.iter().map(|p| matrix.transform_point3(Vec3::from(*p))).collect();
                let order: Vec<usize> = match meshes.get(&mesh.0).and_then(Mesh::indices) {
                    Some(indices) => indices.iter().collect(),
                    None => (0..corners.len()).collect(),
                };
                for triangle in order.as_chunks::<3>().0 {
                    let [a, b, c] = [corners[triangle[0]], corners[triangle[1]], corners[triangle[2]]];
                    let (lo, hi) = (a.min(b).min(c) - Vec3::splat(0.005), a.max(b).max(c) + Vec3::splat(0.005));
                    // A flat sheet is a floor or water, which a Door stands on and in.
                    if hi.y - lo.y < 0.02 {
                        continue;
                    }
                    for door in &doors {
                        for up in [-1.0f32, 0.0, 1.0] {
                            let p = *door + Vec3::Y * up;
                            if p.cmpgt(lo).all() && p.cmplt(hi).all() {
                                bad.push(format!("{entity} {lo}..{hi} over the doorway at {door}"));
                            }
                        }
                    }
                }
            }
            assert!(bad.is_empty(), "{}: {bad:#?}", map.name());
        }
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
