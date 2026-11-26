use super::*;
use crate::enemies::zombie::headless::Sim;
use crate::maps::fire::{FirePlugin, FuelLines};
use crate::maps::zone::{ActiveZones, ZoneSet};
use crate::tuning::{FLARE_UP_SECS, SHUT_SECS};

fn lit() -> LinePhase {
    LinePhase::Lit
}

fn shut(elapsed: f32) -> LinePhase {
    LinePhase::Shut { elapsed }
}

fn flare(elapsed: f32) -> LinePhase {
    LinePhase::FlareUp { elapsed }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn blaze_pose_by_phase() {
    // Lit: everything on, three smoke cards.
    let pose = blaze_pose(lit());
    assert_eq!((pose.flames, pose.light, pose.glow, pose.pilot, pose.shimmer), (1.0, 1.0, 1.0, 0.0, 0.0));
    assert_eq!(pose.smoke, Smoke { cards: 3, alpha: 0.6 });
    // The Shut transition: the flames shrink and the light fades over 1 s, the glow goes to scorch over 2 s, and the smoke thins to a wisp.
    let pose = blaze_pose(shut(0.0));
    assert_eq!((pose.flames, pose.light, pose.glow), (1.0, 1.0, 1.0));
    let pose = blaze_pose(shut(0.5));
    assert!(near(pose.flames, 0.5) && near(pose.light, 0.5) && near(pose.glow, 0.75), "{pose:?}");
    assert_eq!(pose.smoke.cards, 3);
    assert!(pose.smoke.alpha < 0.6 && pose.smoke.alpha > 0.25);
    let pose = blaze_pose(shut(1.0));
    assert_eq!((pose.flames, pose.light), (0.0, 0.0));
    assert!(near(pose.glow, 0.5));
    assert_eq!(pose.smoke, Smoke { cards: 1, alpha: 0.25 });
    assert_eq!(blaze_pose(shut(2.0)).glow, 0.0);
    // Shut: nothing but the scorch and the wisp, the light kept at 0.
    for elapsed in [2.0, 10.0, SHUT_SECS] {
        let pose = blaze_pose(shut(elapsed));
        assert_eq!((pose.flames, pose.light, pose.glow, pose.pilot, pose.shimmer), (0.0, 0.0, 0.0, 0.0, 0.0), "{elapsed}");
        assert_eq!(pose.smoke, Smoke { cards: 1, alpha: 0.25 });
    }
    // Each second of the Flare-up: no flames, the light 0 to 0.15, the dull glow 0 to 0.3, the shimmer all through, the blue pilot only
    // in the last 1.5 s.
    let mut last_light = -1.0;
    for second in 0..=4 {
        let elapsed = second as f32;
        let pose = blaze_pose(flare(elapsed));
        assert_eq!(pose.flames, 0.0, "{elapsed}");
        assert!(near(pose.light, 0.15 * elapsed / FLARE_UP_SECS) && near(pose.glow, 0.3 * elapsed / FLARE_UP_SECS), "{elapsed}: {pose:?}");
        assert!(pose.light > last_light);
        last_light = pose.light;
        assert_eq!(pose.shimmer, 1.0);
        assert_eq!(pose.smoke, Smoke { cards: 1, alpha: 0.25 });
        assert_eq!(pose.pilot, if elapsed >= 2.5 { 1.0 } else { 0.0 }, "{elapsed}");
    }
    assert_eq!(blaze_pose(flare(2.4)).pilot, 0.0);
    assert_eq!(blaze_pose(flare(2.5)).pilot, 1.0);
    assert!(near(blaze_pose(flare(FLARE_UP_SECS)).light, 0.15));
    // Into Lit: the flames grow back over 0.4 s, to the Lit pose.
    assert_eq!(ignite_ramp(0.0), 0.0);
    assert!(near(ignite_ramp(0.2), 0.5));
    assert_eq!(ignite_ramp(IGNITE_SECS), 1.0);
    assert_eq!(ignite_ramp(10.0), 1.0);
}

#[test]
fn valve_lamp_needle_and_wheel_by_phase() {
    // Lit: steady red-orange, the needle at 70%, the wheel at rest, whatever the clock says.
    for now in [0.0, 0.2, 0.37, 5.1] {
        assert_eq!(valve_pose(lit(), false, now), ValvePose { lamp: Lamp::RedOrange, needle: LIT_NEEDLE, wheel: 0.0 }, "{now}");
    }
    // Shut: steady green, the needle at 0, the wheel turning 1.5 revolutions over 0.8 s on the change and staying there.
    for now in [0.0, 0.2, 0.37, 5.1] {
        let pose = valve_pose(shut(0.0), false, now);
        assert_eq!((pose.lamp, pose.needle, pose.wheel), (Lamp::Green, 0.0, 0.0));
        assert_eq!(valve_pose(shut(30.0), false, now).lamp, Lamp::Green);
    }
    assert!(near(valve_pose(shut(0.4), false, 0.0).wheel, 0.75));
    assert!(near(valve_pose(shut(0.8), false, 0.0).wheel, 1.5));
    assert!(near(valve_pose(shut(40.0), false, 0.0).wheel, 1.5));
    // Flare-up: amber, blinking at 3 Hz, the needle swinging from 0 to full over 4 s with a little jitter, the wheel where it was.
    assert_eq!(valve_pose(flare(1.0), false, 0.0).lamp, Lamp::Amber);
    assert_eq!(valve_pose(flare(1.0), false, 0.2).lamp, Lamp::Dark);
    assert_eq!(valve_pose(flare(1.0), false, 0.34).lamp, Lamp::Amber);
    let blinks = (0..300).filter(|step| valve_pose(flare(1.0), false, *step as f32 * 0.01).lamp == Lamp::Amber).count();
    assert!((140..=160).contains(&blinks), "on half the time: {blinks} of 300");
    for second in 0..=4 {
        let mut seen = Vec::new();
        for step in 0..100 {
            let pose = valve_pose(flare(second as f32), false, step as f32 * 0.037);
            assert_eq!(pose.wheel, WHEEL_REVS);
            assert!((pose.needle - second as f32 / FLARE_UP_SECS).abs() <= 0.04 + 1e-4, "{second}: {}", pose.needle);
            assert!((0.0..=1.0).contains(&pose.needle));
            seen.push(pose.needle);
        }
        assert!(seen.iter().any(|&needle| (needle - seen[0]).abs() > 1e-3), "it jitters");
    }
    // Sabotaged: red and fast at 6 Hz, the needle pinned at 100%, in any phase.
    for phase in [lit(), shut(10.0), flare(2.0)] {
        assert_eq!(valve_pose(phase, true, 0.05).lamp, Lamp::Alarm);
        assert_eq!(valve_pose(phase, true, 0.1).lamp, Lamp::Dark);
        assert_eq!(valve_pose(phase, true, 0.18).lamp, Lamp::Alarm);
        assert_eq!(valve_pose(phase, true, 0.05).needle, 1.0);
    }
    // The lamp is a different material for each look, and the needle sweeps 270 degrees.
    assert_eq!(Lamp::ALL.iter().map(|&lamp| lamp_index(lamp)).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4]);
    assert!(near(needle_angle(0.0), 135f32.to_radians()) && near(needle_angle(1.0), -135f32.to_radians()));
}

#[test]
fn flames_are_capped_at_8_a_blaze() {
    assert_eq!(flame_count(1.0), 1);
    assert_eq!(flame_count(2.5), 1);
    assert_eq!(flame_count(2.6), 2);
    assert_eq!(flame_count(16.0), 7);
    assert_eq!(flame_count(20.0), 8);
    assert_eq!(flame_count(1000.0), 8);
    for blaze in MapId::Refinery.layout().fuel_lines.iter().flat_map(|line| line.blazes) {
        assert!(flame_count((blaze.max - blaze.min).element_product()) <= MAX_FLAMES);
    }
    // Alternating sprites every 0.12 s, heights 1.2 to 1.8 pulsed 0.85 to 1.15.
    for seed in 0..40 {
        let mut sprites = std::collections::HashSet::new();
        for step in 0..200 {
            let pose = flame_pose(seed, step as f32 * 0.01);
            sprites.insert(pose.sprite);
            assert!(pose.height >= 1.2 * 0.85 - 1e-4 && pose.height <= 1.8 * 1.15 + 1e-4, "{}", pose.height);
        }
        assert_eq!(sprites.len(), 2);
    }
    for step in 0..500 {
        assert!((0.8..=1.0).contains(&flicker(3, step as f32 * 0.003)));
    }
}

fn refinery_sim() -> Sim {
    use crate::economy::buy::{BuyRefused, CantAfford, HoldPurchased, RefusedByPlayer, UseRequest};
    let mut sim = Sim::on(MapId::Refinery, 60.0, |app| {
        app.add_plugins((FirePlugin, FireLookPlugin))
            .insert_resource(FireSprites::stand_in())
            .insert_resource(ActiveZones(Default::default()))
            .add_message::<UseRequest>()
            .add_message::<HoldPurchased>()
            .add_message::<CantAfford>()
            .add_message::<BuyRefused>()
            .add_message::<RefusedByPlayer>();
    });
    sim.world_mut().resource_mut::<ActiveZones>().0 = ZoneSet::from_iter(crate::maps::zone::ZoneId::ALL);
    sim.run(5);
    sim
}

#[test]
fn the_built_refinery_has_a_bed_kerb_pipe_light_and_valve_prop_to_every_fuel_line() {
    let mut sim = refinery_sim();
    let layout = MapId::Refinery.layout();
    let world = sim.world_mut();
    let sites: Vec<(FuelLineId, f32, f32, usize, PointLight)> = world
        .query::<(&BlazeSite, &PointLight, &Transform)>()
        .iter(world)
        .map(|(site, light, at)| (site.line, site.floor, at.translation.y, site.nozzles.len(), *light))
        .collect();
    assert_eq!(sites.len(), 7, "a light a Blaze");
    for (line, floor, y, nozzles, light) in sites {
        assert_eq!((light.color, light.range, light.shadow_maps_enabled), (BLAZE_COLOUR, BLAZE_RANGE, false));
        assert!(light.intensity > 0.0 && light.intensity <= BLAZE_LUMENS);
        // Its light stands 0.8 m up, or 1.5 m for the Furnace Blaze under the grating.
        assert!(near(y - floor, if line == FuelLineId(4) { 1.5 } else { 0.8 }), "{line:?}: {}", y - floor);
        assert!(nozzles >= 3, "burner nozzles every metre: {nozzles}");
    }
    assert_eq!(world.query::<&BedGlow>().iter(world).count(), 7);
    assert_eq!(world.query::<&GlowStrip>().iter(world).count(), 1, "only the Furnace Blaze has a glow strip under the grating");
    assert_eq!(world.query::<&ValveLamp>().iter(world).count(), 12);
    assert_eq!(world.query::<&ValveWheel>().iter(world).count(), 12);
    assert_eq!(world.query::<&ValveNeedle>().iter(world).count(), 12);
    assert_eq!(world.query::<&StandIn>().iter(world).count(), 36, "a stand, a wheel and a needle each, drawn from code with no model files");
    let pipes = world.query::<&Name>().iter(world).filter(|name| name.as_str().starts_with("Feed pipe")).count();
    assert_eq!(pipes, 6);
    let kerbs: usize = layout.fuel_lines.iter().flat_map(|line| line.blazes).map(|blaze| kerb(blaze.min, blaze.max).len()).sum();
    let stripes = world.query::<&DecalPiece>().iter(world).filter(|piece| piece.0 == Decal::HazardStripe).count();
    assert_eq!(stripes, kerbs);
    assert_eq!(world.query::<&crate::maps::fire::Valve>().iter(world).count(), 12);
}

#[test]
fn a_blazes_cards_are_flames_smoke_pilots_and_shimmer() {
    let mut sim = refinery_sim();
    sim.run(5);
    let world = sim.world_mut();
    let sites: Vec<(Entity, usize, f32)> = world.query::<(Entity, &BlazeSite)>().iter(world).map(|(entity, site)| (entity, site.nozzles.len(), (site.max - site.min).element_product())).collect();
    let flames: Vec<Entity> = world.query::<&FlameCard>().iter(world).map(|card| card.site).collect();
    let smoke: Vec<Entity> = world.query::<&SmokeCard>().iter(world).map(|card| card.site).collect();
    let pilots: Vec<Entity> = world.query::<&PilotCard>().iter(world).map(|card| card.site).collect();
    let shimmer: Vec<Entity> = world.query::<&ShimmerCard>().iter(world).map(|card| card.site).collect();
    for (site, nozzles, area) in sites {
        let count = |list: &[Entity]| list.iter().filter(|&&owner| owner == site).count();
        assert!((1..=8).contains(&count(&flames)), "{}", count(&flames));
        assert_eq!(count(&flames), flame_count(area));
        assert_eq!(count(&smoke), 3);
        assert_eq!(count(&pilots), nozzles);
        assert_eq!(count(&shimmer), 3);
    }
    // Lit: every flame card shows, and no pilot or shimmer.
    assert!(world.query::<(&FlameCard, &Visibility)>().iter(world).all(|(_, visibility)| *visibility != Visibility::Hidden));
    assert!(world.query::<(&PilotCard, &Visibility)>().iter(world).all(|(_, visibility)| *visibility == Visibility::Hidden));
    assert!(world.query::<(&ShimmerCard, &Visibility)>().iter(world).all(|(_, visibility)| *visibility == Visibility::Hidden));
}

/// What a Blaze shows: its light (0 off, 1 dull, 2 bright), which bed material, and how many flame, smoke, pilot and shimmer cards show.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) struct Shown {
    pub level: u8,
    pub glow_bed: bool,
    pub flames: usize,
    pub smoke: usize,
    pub pilots: usize,
    pub shimmer: usize,
}

fn visible<T: Component>(world: &mut World, owner: impl Fn(&T) -> Entity) -> Vec<Entity> {
    world.query::<(&T, &Visibility)>().iter(world).filter(|(_, visibility)| **visibility != Visibility::Hidden).map(|(card, _)| owner(card)).collect()
}

/// What every Blaze of `line` shows in `world`, in the order of its sites.
pub(crate) fn blazes_shown(world: &mut World, line: u8) -> Vec<Shown> {
    let kit = world.resource::<FireKit>().clone();
    let mut sites: Vec<(Entity, f32)> = world
        .query::<(Entity, &BlazeSite, &PointLight)>()
        .iter(world)
        .filter(|(_, site, _)| site.line == FuelLineId(line))
        .map(|(entity, _, light)| (entity, light.intensity / BLAZE_LUMENS))
        .collect();
    sites.sort_by_key(|&(entity, _)| entity);
    let glow_bed = world
        .query::<(&BedGlow, &MeshMaterial3d<StandardMaterial>)>()
        .iter(world)
        .find(|(bed, _)| bed.line == FuelLineId(line))
        .map(|(_, material)| material.0 == kit.glow)
        .unwrap();
    let flames = visible::<FlameCard>(world, |card| card.site);
    let smoke = visible::<SmokeCard>(world, |card| card.site);
    let pilots = visible::<PilotCard>(world, |card| card.site);
    let shimmer = visible::<ShimmerCard>(world, |card| card.site);
    sites
        .into_iter()
        .map(|(site, light)| {
            let count = |list: &[Entity]| list.iter().filter(|&&owner| owner == site).count();
            Shown {
                level: if light < 0.01 { 0 } else if light < 0.2 { 1 } else { 2 },
                glow_bed,
                flames: count(&flames),
                smoke: count(&smoke),
                pilots: count(&pilots),
                shimmer: count(&shimmer),
            }
        })
        .collect()
}

/// Which look each Valve lamp of `line` wears in `world`.
pub(crate) fn valve_lamps(world: &mut World, line: u8) -> Vec<Lamp> {
    let kit = world.resource::<FireKit>().clone();
    world
        .query::<(&ValveLamp, &MeshMaterial3d<StandardMaterial>)>()
        .iter(world)
        .filter(|(lamp, _)| lamp.line == FuelLineId(line))
        .map(|(_, material)| Lamp::ALL[kit.lamps.iter().position(|handle| *handle == material.0).expect("a lamp wears one of the kit's materials")])
        .collect()
}

#[test]
fn blazes_and_valves_follow_their_lines_phase() {
    let mut sim = refinery_sim();
    let line = 0u8;
    // Lit: full light, glowing bed, every flame, three smoke cards, no pilot or shimmer; red-orange lamps.
    sim.run(5);
    let lit = blazes_shown(sim.world_mut(), line);
    assert_eq!(lit.len(), 2);
    for shown in &lit {
        assert_eq!((shown.level, shown.glow_bed, shown.smoke, shown.pilots, shown.shimmer), (2, true, 3, 0, 0));
        assert!(shown.flames >= 1);
    }
    assert_eq!(valve_lamps(sim.world_mut(), line), vec![Lamp::RedOrange; 2]);
    // Shut for 20 s: the scorched bed, no flames or light, a wisp, a green lamp.
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(line), shut(20.0));
    sim.run(5);
    for shown in blazes_shown(sim.world_mut(), line) {
        assert_eq!((shown.level, shown.glow_bed, shown.flames, shown.smoke, shown.pilots, shown.shimmer), (0, false, 0, 1, 0, 0));
    }
    assert_eq!(valve_lamps(sim.world_mut(), line), vec![Lamp::Green; 2]);
    // Flaring up late: a dull light, the shimmer and the blue pilot, an amber (or between-blinks) lamp.
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(line), flare(3.0));
    sim.run(5);
    for shown in blazes_shown(sim.world_mut(), line) {
        assert_eq!((shown.level, shown.glow_bed, shown.flames, shown.shimmer), (1, false, 0, 3));
        assert!(shown.pilots >= 3);
    }
    assert!(valve_lamps(sim.world_mut(), line).iter().all(|lamp| matches!(lamp, Lamp::Amber | Lamp::Dark)));
    // Early in the Flare-up there is no pilot yet.
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(line), flare(1.0));
    sim.run(5);
    assert!(blazes_shown(sim.world_mut(), line).iter().all(|shown| shown.pilots == 0 && shown.shimmer == 3));
    // The other lines were not touched.
    assert_eq!(blazes_shown(sim.world_mut(), 1)[0].level, 2);
    assert_eq!(valve_lamps(sim.world_mut(), 1), vec![Lamp::RedOrange; 2]);
}

#[test]
fn a_line_that_lights_regrows_its_flames_and_a_sabotaged_one_shows_red() {
    let mut sim = refinery_sim();
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(1), flare(3.9));
    sim.run(5);
    assert_eq!(blazes_shown(sim.world_mut(), 1)[0].flames, 0);
    // It lights: the flames are back within a few frames.
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(1), lit());
    sim.run(40);
    let all = blazes_shown(sim.world_mut(), 1);
    assert!(all[0].flames >= 1);
    assert_eq!((all[0].level, all[0].glow_bed), (2, true));
    // Sabotaged: the Valve lamps blink red and the line stays Lit.
    let node = {
        let world = sim.world_mut();
        world.query::<(Entity, &crate::maps::fire::LineNode)>().iter(world).find(|(_, node)| node.line == FuelLineId(2)).unwrap().0
    };
    sim.world_mut().entity_mut(node).insert(crate::enemies::sapper::Sabotaged);
    sim.run(5);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..40 {
        sim.run(1);
        seen.insert(valve_lamps(sim.world_mut(), 2)[0]);
    }
    assert!(seen.contains(&Lamp::Alarm) && seen.contains(&Lamp::Dark), "blinking red: {seen:?}");
    assert!(seen.iter().all(|lamp| matches!(lamp, Lamp::Alarm | Lamp::Dark)), "{seen:?}");
    assert_eq!(blazes_shown(sim.world_mut(), 2)[0].level, 2);
    assert_eq!(valve_lamps(sim.world_mut(), 3), vec![Lamp::RedOrange; 2]);
}

#[test]
fn nozzles_stand_every_metre_along_the_pipe_through_each_blaze() {
    let layout = MapId::Refinery.layout();
    for line in layout.fuel_lines {
        let run = pipe_run(line);
        assert!(run.is_some(), "{}", line.name);
        for blaze in line.blazes {
            let spots = nozzles(blaze, run);
            assert!(spots.len() >= 3, "{}: {}", line.name, spots.len());
            for spot in &spots {
                assert!(spot.x >= blaze.min.x - 1e-3 && spot.x <= blaze.max.x + 1e-3 && spot.z >= blaze.min.y - 1e-3 && spot.z <= blaze.max.y + 1e-3, "{spot:?} in {blaze:?}");
                assert!(near(spot.y, blaze.floor + 2.0 * PIPE_RADIUS));
            }
            for pair in spots.windows(2) {
                assert!((pair[0].distance(pair[1]) - NOZZLE_GAP).abs() < 0.3, "about a metre apart: {}", pair[0].distance(pair[1]));
            }
            // On the pipe: in line with it.
            let (a, b) = run.unwrap();
            let dir = (b - a).normalize();
            for spot in &spots {
                let off = (*spot - a) - dir * (*spot - a).dot(dir);
                assert!(off.with_y(0.0).length() < 1e-2, "{off:?}");
            }
        }
    }
    // With no pipe a Blaze has them along its long middle.
    assert!(nozzles(&layout.fuel_lines[0].blazes[0], None).len() >= 3);
}

#[test]
fn the_hazard_stripe_kerb_rings_each_blazes_bed() {
    let layout = MapId::Refinery.layout();
    for blaze in layout.fuel_lines.iter().flat_map(|line| line.blazes) {
        let tiles = kerb(blaze.min, blaze.max);
        let (outer_min, outer_max) = (blaze.min - Vec2::splat(BED_MARGIN), blaze.max + Vec2::splat(BED_MARGIN));
        let mut area = 0.0;
        for tile in &tiles {
            let (lo, hi) = (tile.at - tile.size / 2.0, tile.at + tile.size / 2.0);
            // Within the bed and clear of the Blaze's rectangle: a ring on the bed's outer edge.
            assert!(lo.x >= outer_min.x - 1e-3 && lo.y >= outer_min.y - 1e-3 && hi.x <= outer_max.x + 1e-3 && hi.y <= outer_max.y + 1e-3, "{tile:?}");
            assert!(lo.x >= blaze.max.x || hi.x <= blaze.min.x || lo.y >= blaze.max.y || hi.y <= blaze.min.y, "{tile:?} is in the fire");
            assert!(tile.size.x.min(tile.size.y) <= KERB_WIDTH + 1e-4 && tile.size.x.max(tile.size.y) <= 1.5);
            area += tile.size.x * tile.size.y;
        }
        let (outside, inside) = ((outer_max - outer_min).element_product(), (outer_max - outer_min - Vec2::splat(2.0 * KERB_WIDTH)).element_product());
        assert!((area - (outside - inside)).abs() < 1e-2, "the tiles cover the ring exactly: {area} of {}", outside - inside);
    }
}

// ---- Burning Zombies ----

fn flames_on(sim: &mut Sim, zombie: Entity) -> usize {
    let world = sim.world_mut();
    world.query::<&ZombieFlame>().iter(world).filter(|card| card.zombie == zombie).count()
}

/// Puts `zombie`'s feet at `feet`.
fn pin(sim: &mut Sim, zombie: Entity, feet: Vec3) {
    let half = sim.world_mut().get::<crate::enemies::zombie::Zombie>(zombie).unwrap().half_height();
    let at = feet + Vec3::Y * half;
    sim.world_mut().get_mut::<Transform>(zombie).unwrap().translation = at;
    sim.world_mut().entity_mut(zombie).insert((avian3d::prelude::Position(at), avian3d::prelude::LinearVelocity::ZERO));
}

#[test]
fn at_most_12_burning_zombies_are_drawn_nearest_first() {
    // The selection: the nearest twelve of twenty, in order.
    let candidates: Vec<(u32, f32)> = (0..20u32).map(|i| (i, ((i * 7) % 20) as f32 + 1.0)).collect();
    let drawn = nearest(&candidates, MAX_BURNING_ZOMBIES);
    assert_eq!(drawn.len(), 12);
    let mut sorted = candidates.clone();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    assert_eq!(drawn, sorted.iter().take(12).map(|c| c.0).collect::<Vec<_>>());
    assert!(drawn.iter().all(|&i| candidates[i as usize].1 <= 12.0));
    assert_eq!(nearest(&candidates[..5], 12).len(), 5);

    // In the world: fifteen Zombies stand in a Lit Blaze with the eye at one end; twelve wear flames, the nearest twelve.
    let mut sim = refinery_sim();
    let blaze = MapId::Refinery.layout().fuel_lines[0].blazes[0];
    let eye = Vec3::new(blaze.min.x, blaze.floor + 1.6, blaze.min.y - 6.0);
    sim.world_mut().spawn((Camera3d::default(), Transform::from_translation(eye), GlobalTransform::from_translation(eye)));
    let zombies: Vec<Entity> = (0..15).map(|i| sim.spawn(Vec3::new(blaze.min.x + 0.2 + (0.4 * i as f32) % 6.0, blaze.floor, blaze.min.y + 0.4 + 0.2 * i as f32), None)).collect();
    for (i, zombie) in zombies.iter().enumerate() {
        pin(&mut sim, *zombie, Vec3::new(blaze.min.x + 0.2 + (0.4 * i as f32) % 6.0, blaze.floor, blaze.min.y + 0.4 + 0.2 * i as f32));
    }
    sim.run(2);
    let world = sim.world_mut();
    let drawn: Vec<Entity> = world.query::<&ZombieFlame>().iter(world).map(|card| card.zombie).collect();
    assert_eq!(world.resource::<Aflame>().drawn(), 12);
    assert_eq!(drawn.len(), 24, "two cards each");
    // The ones drawn are the nearest to the eye.
    let mut by_distance: Vec<(Entity, f32)> = zombies.iter().map(|&z| (z, world.get::<Transform>(z).unwrap().translation.distance_squared(eye))).collect();
    by_distance.sort_by(|a, b| a.1.total_cmp(&b.1));
    let farthest_drawn = drawn.iter().map(|z| by_distance.iter().find(|(e, _)| e == z).unwrap().1).fold(0.0, f32::max);
    let nearest_left_out = by_distance.iter().filter(|(z, _)| !drawn.contains(z)).map(|c| c.1).fold(f32::MAX, f32::min);
    assert!(farthest_drawn <= nearest_left_out + 1.0, "drawn out to {farthest_drawn}, left out from {nearest_left_out}");
}

#[test]
fn burning_zombie_flames_follow_the_feet_rule() {
    let mut sim = refinery_sim();
    let blaze = MapId::Refinery.layout().fuel_lines[1].blazes[0];
    let inside = blaze.centre();
    let outside = Vec3::new(blaze.max.x + 3.0, blaze.floor, blaze.max.y + 3.0);
    let zombie = sim.spawn(outside, None);
    // Feet outside every Blaze: no flames.
    pin(&mut sim, zombie, outside);
    sim.run(3);
    assert_eq!(flames_on(&mut sim, zombie), 0);
    // Feet in a Lit Blaze: two cards (chest and legs), the chest above the legs.
    pin(&mut sim, zombie, inside);
    sim.run(4);
    assert_eq!(flames_on(&mut sim, zombie), 2);
    let ys: Vec<(usize, f32)> = {
        let world = sim.world_mut();
        world.query::<(&ZombieFlame, &Transform)>().iter(world).map(|(card, at)| (card.part, at.translation.y)).collect()
    };
    let (chest, legs) = (ys.iter().find(|c| c.0 == 0).unwrap().1, ys.iter().find(|c| c.0 == 1).unwrap().1);
    assert!(chest > legs && legs > blaze.floor);
    // Out of it, the flames stay for 2 s and then go.
    pin(&mut sim, zombie, outside);
    let frames = sim.frames(1.5);
    sim.run(frames);
    assert_eq!(flames_on(&mut sim, zombie), 2, "still burning 1.5 s after");
    let frames = sim.frames(1.0);
    sim.run(frames);
    assert_eq!(flames_on(&mut sim, zombie), 0, "out 2.5 s after");
    // Back in, but the line Shut or Flaring up: nothing burns.
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(1), shut(5.0));
    pin(&mut sim, zombie, inside);
    sim.run(3);
    assert_eq!(flames_on(&mut sim, zombie), 0, "a Shut line burns no one");
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(1), flare(1.0));
    pin(&mut sim, zombie, inside);
    sim.run(3);
    assert_eq!(flames_on(&mut sim, zombie), 0, "a Flaring line burns nothing yet");
    // Lit again: it burns.
    sim.world_mut().resource_mut::<FuelLines>().show(FuelLineId(1), lit());
    pin(&mut sim, zombie, inside);
    sim.run(3);
    assert_eq!(flames_on(&mut sim, zombie), 2);
    // A dead Zombie's flames go with it.
    sim.world_mut().entity_mut(zombie).despawn();
    sim.run(3);
    assert_eq!(sim.world_mut().query::<&ZombieFlame>().iter(sim.world_mut()).count(), 0);
    assert_eq!(sim.world_mut().resource::<Aflame>().drawn(), 0);
}

#[test]
fn a_napalm_does_not_burn_in_a_blaze_but_a_hellhound_does() {
    use crate::enemies::special::SpecialKind;
    let mut sim = refinery_sim();
    let blaze = MapId::Refinery.layout().fuel_lines[1].blazes[0];
    let spot = blaze.centre();
    let napalm = sim.spawn(spot, None);
    sim.world_mut().entity_mut(napalm).insert(crate::enemies::zombie::Zombie::special(SpecialKind::Napalm, 1000.0, spot));
    let hound = sim.spawn(spot + Vec3::X, None);
    sim.world_mut().entity_mut(hound).insert(crate::enemies::zombie::Zombie::hellhound(1000.0, spot + Vec3::X));
    pin(&mut sim, napalm, spot);
    pin(&mut sim, hound, spot + Vec3::X);
    sim.run(2);
    assert_eq!(flames_on(&mut sim, napalm), 0);
    assert_eq!(flames_on(&mut sim, hound), 2);
}
