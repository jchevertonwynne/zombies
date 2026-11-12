//! A throwaway probe: running jumps off every edge of a Map, logging where a Player leaves the
//! Zones' volumes (into the void, or up onto a wall top). Run with
//! `cargo test --release void_sweep -- --ignored --nocapture`.

use std::collections::{BTreeMap, HashSet};
use std::f32::consts::TAU;

use super::*;
use crate::maps::map::MapId;
use crate::maps::zone::ZoneId;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Out {
    /// Feet outside every Zone of the Map.
    Outside,
    /// Feet inside a Zone but well above its floor: standing on a wall or rail top.
    Perched,
}

fn zones(map: MapId) -> Vec<ZoneId> {
    ZoneId::ALL.into_iter().filter(|z| z.map() == map).collect()
}

fn classify(map: MapId, feet: Vec3) -> Option<(Out, Option<ZoneId>)> {
    let inside: Vec<ZoneId> = zones(map)
        .into_iter()
        .filter(|z| {
            let (lo, hi) = z.volume();
            feet.cmpge(lo - Vec3::Y * 0.05).all() && feet.cmple(hi).all()
        })
        .collect();
    if inside.is_empty() {
        return Some((Out::Outside, None));
    }
    // Perched if above every containing Zone's floor by more than a crate (and not on a ramp:
    // ramp Zones are tall, so only flag above floor + a storey's worth for those).
    let low = inside.iter().all(|z| feet.y > z.floor() + 1.35);
    let on_slope = map.layout().slopes.iter().any(|s| {
        let along = Vec2::new(s.up.x, s.up.y);
        let rel = Vec2::new(feet.x - s.foot.x, feet.z - s.foot.z);
        let t = rel.dot(along);
        let side = rel.perp_dot(along).abs();
        t > -0.5 && t < s.run + 0.5 && side < s.width / 2.0 + 0.5
    });
    let raised = map.layout().raised_floors.iter().any(|r| {
        feet.x > r.min.x - 0.5 && feet.x < r.max.x + 0.5 && feet.z > r.min.y - 0.5 && feet.z < r.max.y + 0.5
    });
    if low && !on_slope && !raised {
        return Some((Out::Perched, inside.first().copied()));
    }
    None
}

fn starts(map: MapId) -> Vec<(String, Vec3)> {
    let layout = map.layout();
    let mut out: Vec<(String, Vec3)> = layout
        .nav_nodes
        .iter()
        .filter(|(name, ..)| !name.contains("_out") && !name.starts_with("vent"))
        .map(|&(name, x, y, z)| (name.to_string(), Vec3::new(x, y, z)))
        .collect();
    if std::env::var("SWEEP_GRID").is_err() { return out; }
    for zone in zones(map) {
        let (lo, hi) = zone.volume();
        let mut x = lo.x + 1.2;
        while x < hi.x - 1.0 {
            let mut z = lo.z + 1.2;
            while z < hi.z - 1.0 {
                out.push((format!("grid {}", zone.name()), Vec3::new(x, zone.floor(), z)));
                z += 3.0;
            }
            x += 3.0;
        }
    }
    out
}

struct Escape {
    start: String,
    from: Vec3,
    yaw_deg: f32,
    run_up: f32,
    jump: bool,
    first: Vec3,
    kind: Out,
    end: Vec3,
}

fn sweep(map: MapId) {
    let fps = 30.0;
    let mut sim = Sim::on(map, fps);
    sim.power_on();
    let (pmin, pmax) = map.layout().plan;
    let mut escapes: Vec<Escape> = Vec::new();
    let mut queue = starts(map);
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut i = 0;
    let t0 = std::time::Instant::now();
    while i < queue.len() && i < 300 {
        let (name, from) = queue[i].clone();
        i += 1;
        sim.place(from, 0.0);
        let settled = feet(&sim.frame());
        println!("start {i}/{} {name} {from} at {:?}", queue.len(), t0.elapsed());
        if settled.distance(from) > 0.4 {
            continue;
        }
        for d in 0..8 {
            let yaw = d as f32 * TAU / 8.0;
            for (run_up, jump) in [(0.6, true)] {
                sim.place(from, yaw);
                let mut samples = Vec::new();
                sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                samples.extend(sim.run_secs(run_up));
                if jump {
                    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space]);
                    samples.extend(sim.run_secs(0.1));
                    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                    samples.extend(sim.run_secs(1.2));
                    // Jump again, in case the first landed on something.
                    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space]);
                    samples.extend(sim.run_secs(0.1));
                    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                    samples.extend(sim.run_secs(1.2));
                }
                sim.hold(&[]);
                samples.extend(sim.run_secs(1.0));
                let first = samples.iter().map(feet).find_map(|f| classify(map, f).map(|c| (f, c.0)));
                if let Some((first, kind)) = first {
                    let end = feet(samples.last().unwrap());
                    println!("HIT {} from {name} {from} yaw {:.0}: {kind:?} first {first} end {end}", map.name(), yaw.to_degrees());
                    escapes.push(Escape { start: name.clone(), from, yaw_deg: yaw.to_degrees(), run_up, jump, first, kind, end });
                    // Explore onward from a perch that held.
                    if classify(map, end).is_some_and(|c| c.0 == Out::Perched) || (classify(map, end).is_some() && end.y > -1.0) {
                        let key = (end.x.round() as i32, end.y.round() as i32, end.z.round() as i32);
                        if seen.insert(key) {
                            queue.push((format!("perch from {name}"), end));
                        }
                    }
                }
            }
        }
    }
    println!("\n=== {} : {} starts, {} trials flagged, {:?}", map.name(), i, escapes.len(), t0.elapsed());
    // Group by where they first left, to a 2 m cell.
    let mut groups: BTreeMap<(i32, i32, i32), Vec<&Escape>> = BTreeMap::new();
    for e in &escapes {
        let key = ((e.first.x / 2.0).floor() as i32 * 2, e.first.y.round() as i32, (e.first.z / 2.0).floor() as i32 * 2);
        groups.entry(key).or_default().push(e);
    }
    for ((x, y, z), es) in &groups {
        let void = es.iter().filter(|e| e.end.y < -2.0 || e.end.x < pmin.x || e.end.x > pmax.x || e.end.z < pmin.y || e.end.z > pmax.y).count();
        let e = es[0];
        println!(
            "cell x={x} y={y} z={z} (local x={}): {} hits, {} ended in void | {:?} e.g. from {} {} yaw {:.0} run {} jump {} first {} end {}",
            *x as f32 - if map == MapId::Quarry { 200.0 } else { 0.0 },
            es.len(),
            void,
            e.kind,
            e.start,
            e.from,
            e.yaw_deg,
            e.run_up,
            e.jump,
            e.first,
            e.end
        );
    }
}

#[test]
#[ignore]
fn void_sweep_quarry() {
    sweep(MapId::Quarry);
}

#[test]
#[ignore]
fn void_sweep_greybox() {
    sweep(MapId::Greybox);
}

/// Dense running jumps off every slope's sides and top, reporting only escapes from every Zone.
fn slope_probe(map: MapId) {
    let mut sim = Sim::on(map, 30.0);
    sim.power_on();
    let mut hits = BTreeMap::new();
    for s in map.layout().slopes {
        let along = Vec3::new(s.up.x, 0.0, s.up.y);
        let across = Vec3::new(-s.up.y, 0.0, s.up.x);
        let mut t = 0.5;
        while t <= s.run + 0.6 {
            for off in [-s.width / 2.0 + 0.4, 0.0, s.width / 2.0 - 0.4] {
                let h = (t / s.run).min(1.0) * s.rise;
                let from = s.foot + along * t + across * off + Vec3::Y * (h + 0.05);
                for d in 0..24 {
                    let yaw = d as f32 * TAU / 24.0;
                    for (run_up, hold_jump) in [(0.05, 0.1), (0.3, 0.1), (0.3, 1.5)] {
                        sim.place(from, yaw);
                        let mut samples = Vec::new();
                        sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                        samples.extend(sim.run_secs(run_up));
                        // Hold jump: bunny-hops on anything it lands on.
                        sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space]);
                        samples.extend(sim.run_secs(hold_jump));
                        sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                        samples.extend(sim.run_secs(1.5));
                        sim.hold(&[]);
                        samples.extend(sim.run_secs(1.0));
                        if let Some(first) = samples.iter().map(feet).find(|f| classify(map, *f).is_some_and(|c| c.0 == Out::Outside)) {
                            let end = feet(samples.last().unwrap());
                            let key = ((first.x * 2.0).round() as i32, (first.y * 2.0).round() as i32, (first.z * 2.0).round() as i32);
                            hits.entry(key).or_insert_with(|| format!("{} slope {:?} from {from} yaw {:.0} run {run_up} jumphold {hold_jump}: first out {first} end {end}", map.name(), s.zone, yaw.to_degrees()));
                        }
                    }
                }
            }
            t += 0.5;
        }
    }
    println!("=== {} slope probe: {} distinct exits", map.name(), hits.len());
    for line in hits.values() {
        println!("{line}");
    }
}

#[test]
#[ignore]
fn void_slopes_quarry() {
    slope_probe(MapId::Quarry);
}

#[test]
#[ignore]
fn void_slopes_greybox() {
    slope_probe(MapId::Greybox);
}
