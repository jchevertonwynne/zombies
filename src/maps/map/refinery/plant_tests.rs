use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::enemies::zombie::headless::Sim;
use crate::layers::GameLayer;
use crate::maps::map::MapId;

/// The heater's eight peepholes glow orange-red at 6 times over, and they and the three aviation lamps (the flare's tip, the column's top and the
/// heater stack's) emit no light: no point or spot light on them or in a child.
#[test]
fn heater_peepholes_and_aviation_lamps_emit_no_light() {
    let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
    let world = sim.world_mut();
    let peepholes: Vec<(Entity, Vec3, LinearRgba)> = world
        .run_system_once(|holes: Query<(Entity, &Transform, &MeshMaterial3d<StandardMaterial>), With<Peephole>>, materials: Res<Assets<StandardMaterial>>| {
            holes.iter().map(|(entity, at, material)| (entity, at.translation, materials.get(&material.0).unwrap().emissive)).collect::<Vec<_>>()
        })
        .unwrap();
    assert_eq!(peepholes.len(), 8);
    for (_, at, emissive) in &peepholes {
        assert_eq!(*emissive, LinearRgba::from(PEEPHOLE_COLOUR) * 6.0, "the peephole at {at}");
        assert!(PEEPHOLES.contains(at));
        // On the heater's faces (x 8..16, z 70..82), 1.4 m up.
        assert!((7.95..=16.05).contains(&at.x) && (69.95..=82.05).contains(&at.z) && at.y == 1.4);
    }
    let lamps: Vec<(Entity, Vec3)> = world.run_system_once(|lamps: Query<(Entity, &Transform), With<Aviation>>| lamps.iter().map(|(entity, at)| (entity, at.translation)).collect::<Vec<_>>()).unwrap();
    assert_eq!(lamps.len(), 3, "the flare's tip, the column's top and the heater stack's");
    assert!(lamps.iter().any(|(_, at)| at.y > COLUMN_TOP) && lamps.iter().any(|(_, at)| (at.y - (STACK_TOP + 0.3)).abs() < 0.01));
    let lit: Vec<Entity> = peepholes.iter().map(|p| p.0).chain(lamps.iter().map(|l| l.0)).collect();
    let (lights, children) = world
        .run_system_once(move |entities: Query<(Has<PointLight>, Has<SpotLight>, Has<DirectionalLight>, Option<&Children>)>| {
            let mut lights = 0;
            let mut children = 0;
            for &entity in &lit {
                let (point, spot, sun, kids) = entities.get(entity).unwrap();
                lights += usize::from(point) + usize::from(spot) + usize::from(sun);
                children += kids.map_or(0, |kids| kids.len());
            }
            (lights, children)
        })
        .unwrap();
    assert_eq!((lights, children), (0, 0));
}

/// The peepholes' flutter and the aviation lamps' blink are pure functions of where they stand and the clock: the same on every machine.
#[test]
fn peephole_and_aviation_flicker_is_the_same_on_every_machine() {
    use super::super::sky::{AVIATION_AT, aviation_lit};
    let strengths = |now: f32| PEEPHOLES.map(|at| peephole_strength(at, now));
    for step in 0..120 {
        let now = step as f32 * 0.137;
        assert_eq!(strengths(now), strengths(now));
        for strength in strengths(now) {
            assert!((PEEPHOLE_GLOW * 0.6 - 1e-4..=PEEPHOLE_GLOW + 1e-4).contains(&strength), "{strength}");
        }
    }
    // Each has its own phase, and each moves.
    assert!(peephole_strength(PEEPHOLES[0], 1.0) != peephole_strength(PEEPHOLES[1], 1.0));
    assert!(peephole_strength(PEEPHOLES[0], 1.0) != peephole_strength(PEEPHOLES[0], 1.1));
    // Pinned, so a change to the hash or the flutter (which would put two machines' heaters out of step) fails here.
    let pinned: Vec<f32> = strengths(2.5).iter().map(|s| (s * 1000.0).round() / 1000.0).collect();
    assert_eq!(pinned, PINNED_STRENGTHS);
    // The lamps blink at 1 Hz, lit for half of each second, on the hash of where each stands.
    for at in [AVIATION_AT, Vec3::new(0.0, COLUMN_TOP + 1.65, 76.0), Vec3::new(12.0, STACK_TOP + 0.3, 83.0)] {
        let lit = (0..100).filter(|&step| aviation_lit(at, step as f32 * 0.01)).count();
        assert_eq!(lit, 50, "the lamp at {at}");
        assert_eq!(aviation_lit(at, 3.3), aviation_lit(at, 3.3));
    }
}

const PINNED_STRENGTHS: [f32; 8] = [4.821, 5.06, 5.122, 4.391, 4.249, 5.517, 4.93, 4.531];

/// Dressing a structure moves no collider: the tops and sides a ray finds are the walkable slice's own (the tanks to 10 m and 6 m round, the
/// column to 12, the drum to 3.5, the heater to its 4 m roof, the sleepers to 1, the rack's posts to 4, the booth and the weighbridge).
#[test]
fn refinery_structures_keep_the_walkable_colliders() {
    let mut sim = Sim::on(MapId::Refinery, 60.0, |_| {});
    let wrong: Vec<String> = sim
        .world_mut()
        .run_system_once(|spatial: SpatialQuery| -> Vec<String> {
            let filter = SpatialQueryFilter::from_mask(GameLayer::World);
            let top = |x: f32, z: f32| spatial.cast_ray(Vec3::new(x, 40.0, z), Dir3::NEG_Y, 60.0, true, &filter).map(|hit| 40.0 - hit.distance);
            // Under the Rack Walk, from below its deck.
            let top_under = |x: f32, z: f32| spatial.cast_ray(Vec3::new(x, 3.5, z), Dir3::NEG_Y, 60.0, true, &filter).map(|hit| 3.5 - hit.distance);
            let reach = |from: Vec3, dir: Dir3| spatial.cast_ray(from, dir, 80.0, true, &filter).map(|hit| hit.distance);
            let mut out = Vec::new();
            for (what, x, z, want) in [
                ("Tank A", -48.0, 40.0, 10.0),
                ("Tank B", -32.0, 40.0, 10.0),
                ("the column", 0.0, 76.0, 12.0),
                ("the knockout drum", -31.5, 82.0, 3.5),
                ("the flare stack's base", -40.0, 82.0, 4.0),
                ("the heater's roof", 10.0, 75.0, 4.0),
                ("the heater's stack", 12.0, 83.0, STACK_TOP),
                ("the weighbridge", 0.0, 11.0, 2.5),
                ("the booth", 3.5, 15.0, 3.0),
            ] {
                match top(x, z) {
                    Some(got) if (got - want).abs() < 0.01 => {}
                    got => out.push(format!("{what} tops out at {got:?}, not {want}")),
                }
            }
            match top_under(-3.9, 28.0) {
                Some(got) if (got - 1.0).abs() < 0.01 => {}
                got => out.push(format!("a sleeper tops out at {got:?}, not 1")),
            }
            // The tanks are 6 m round and the column 3: the collider's side, not the skin's (which stands 2 cm proud).
            for (what, from, dir, want) in [
                ("Tank A's west side", Vec3::new(-57.0, 2.0, 40.0), Dir3::X, 3.0),
                ("Tank B's east side", Vec3::new(-23.0, 2.0, 40.0), Dir3::NEG_X, 3.0),
                ("the column's south side", Vec3::new(0.0, 2.0, 68.0), Dir3::Z, 5.0),
                ("a bent's post", Vec3::new(2.0, 2.0, 34.0), Dir3::X, 1.2),
            ] {
                match reach(from, dir) {
                    Some(got) if (got - want).abs() < 0.05 => {}
                    got => out.push(format!("{what} is {got:?} away, not {want}")),
                }
            }
            out
        })
        .unwrap();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
