//! Running jumps off every edge of every Map: a Player must never get Out of Bounds. The sweep is
//! a little stricter than the game's rule, flagging a fall 1 m below the lowest floor rather than
//! 3 m, so it sees a fall before the game catches it and puts the Player back.

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

/// Out of the Map: Out of Bounds by the game's rule, or already 1 m below its lowest floor.
fn lost(map: MapId, feet: Vec3) -> bool {
    let layout = map.layout();
    layout.out_of_bounds(crate::maps::water::TideLevel::Low, feet) || feet.y < layout.lowest_floor() - 1.0
}

fn starts(map: MapId) -> Vec<(String, Vec3)> {
    let layout = map.layout();
    let mut out: Vec<(String, Vec3)> = layout
        .nav_nodes
        .iter()
        .filter(|(name, ..)| !name.contains("_out") && !name.starts_with("vent"))
        .map(|&(name, x, y, z)| (name.to_string(), Vec3::new(x, y, z)))
        .collect();
    // Set SWEEP_GRID to also start from a 3 m grid across every Zone, not just the nav nodes.
    if std::env::var("SWEEP_GRID").is_err() {
        return out;
    }
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
    end: Vec3,
}

fn sweep(map: MapId) {
    let fps = 30.0;
    let mut sim = Sim::on(map, fps);
    sim.power_on();
    let mut escapes: Vec<Escape> = Vec::new();
    let mut queue = starts(map);
    let mut seen: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut i = 0;
    // Capped, so exploring onward from perches can't run away.
    while i < queue.len() && i < 600 {
        let (name, from) = queue[i].clone();
        i += 1;
        sim.place(from, 0.0);
        let settled = feet(&sim.frame());
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
                if let Some((first, _)) = first {
                    let end = feet(samples.last().unwrap());
                    if samples.iter().map(feet).any(|f| lost(map, f)) {
                        escapes.push(Escape { start: name.clone(), from, yaw_deg: yaw.to_degrees(), run_up, jump, first, end });
                    }
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
    let lines: Vec<String> = escapes
        .iter()
        .map(|e| format!("from {} {} yaw {:.0} run {} jump {}: first {} end {}", e.start, e.from, e.yaw_deg, e.run_up, e.jump, e.first, e.end))
        .collect();
    assert!(escapes.is_empty(), "{}: {} of {} starts' jumps ended out of the Map:\n{}", map.name(), escapes.len(), i, lines.join("\n"));
}

/// One test per Map, so a failure names its Map and the Maps run side by side.
macro_rules! per_map {
    ($($name:ident: $map:ident,)*) => {
        $(
            #[test]
            #[ignore = "slow: run by `cargo test --release void_sweep -- --ignored`, and in CI"]
            fn $name() {
                sweep(MapId::$map);
            }
        )*
    };
}

per_map! {
    void_sweep_greybox: Greybox,
    void_sweep_quarry: Quarry,
    void_sweep_village: Village,
    void_sweep_dam: Dam,
    void_sweep_cove: Cove,
    void_sweep_polder: Polder,
}

/// Dense running jumps off every slope's sides and top.
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
                        if let Some(first) = samples.iter().map(feet).find(|f| lost(map, *f)) {
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
    let lines: Vec<&String> = hits.values().collect();
    assert!(hits.is_empty(), "{}: {} distinct exits off the slopes:\n{}", map.name(), hits.len(), lines.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n"));
}

macro_rules! per_map_slopes {
    ($($name:ident: $map:ident,)*) => {
        $(
            #[test]
            #[ignore = "slow: run by `cargo test --release void_sweep -- --ignored`, and in CI"]
            fn $name() {
                slope_probe(MapId::$map);
            }
        )*
    };
}

per_map_slopes! {
    void_slopes_greybox: Greybox,
    void_slopes_quarry: Quarry,
    void_slopes_village: Village,
    void_slopes_dam: Dam,
    void_slopes_cove: Cove,
    void_slopes_polder: Polder,
}

/// A Player who somehow reaches the Rail Yard's window ledge can't jump, bunny-hop or run over its
/// parapets into the void: the ledge is x 220..224, z 2..9, its top at the first terrace.
#[test]
fn a_player_on_a_window_ledge_cannot_get_over_its_parapets() {
    let top = crate::maps::map::STOREY;
    let max = Vec2::new(224.0, 9.0);
    let mut sim = Sim::on(MapId::Quarry, 60.0);
    // Yaw 0 faces -Z; a positive yaw turns towards -X.
    for (name, toward) in [
        ("south", 0.0_f32),
        ("south-east", -FRAC_PI_2 / 2.0),
        ("east", -FRAC_PI_2),
        ("north-east", -3.0 * FRAC_PI_2 / 2.0),
        ("north", std::f32::consts::PI),
    ] {
        let start = Vec3::new(222.0, top, 5.5);
        for hold_jump in [false, true] {
            for run_up in [0.0, 0.3] {
                sim.place(start, toward);
                let keys: &[KeyCode] = if hold_jump { &[KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space] } else { &[KeyCode::KeyW, KeyCode::ShiftLeft] };
                sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                let mut samples = sim.run_secs(run_up);
                sim.hold(keys);
                samples.extend(sim.run_secs(2.0));
                if !hold_jump {
                    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space]);
                    samples.extend(sim.run_secs(0.1));
                    sim.hold(&[KeyCode::KeyW, KeyCode::ShiftLeft]);
                    samples.extend(sim.run_secs(1.5));
                }
                sim.hold(&[]);
                samples.extend(sim.run_secs(0.5));
                for f in samples.iter().map(feet) {
                    assert!(f.x < max.x && f.z > 2.0 && f.z < max.y && f.y > top - 0.5, "{name} hold {hold_jump} run {run_up}: over the parapet at {f}");
                }
            }
        }
    }
}
