//! The built Map drawn from above as an SVG, for placing things by eye. Writes
//! `target/map-plan.svg`: `cargo test map::plan -- --ignored`.

use std::fmt::Write as _;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::map::{SpawnKind, SpawnPoint};
use crate::nav::{Gate, NavGraph};

/// Pixels per metre.
const SCALE: f32 = 12.0;
const MIN: Vec2 = Vec2::new(-40.0, -56.0);
const MAX: Vec2 = Vec2::new(40.0, 32.0);

fn px(at: Vec2) -> (f32, f32) {
    ((at.x - MIN.x) * SCALE, (MAX.y - at.y) * SCALE)
}

#[test]
#[ignore = "draws the Map; asserts nothing"]
fn draw() {
    let mut sim = crate::zombie::headless::Sim::new(60.0, |_| {});
    let world = sim.world_mut();
    // Everything solid, as boxes from above: its bounds, and whether it's a window's opening.
    let mut boxes: Vec<(Vec3, Vec3, bool)> = world
        .query::<(&Collider, &Transform, &RigidBody, Option<&CollisionLayers>)>()
        .iter(world)
        .filter(|(_, _, body, _)| **body == RigidBody::Static)
        .map(|(collider, transform, _, layers)| {
            let aabb = collider.aabb(transform.translation, transform.rotation, 0.0);
            (aabb.min, aabb.max, layers.is_some_and(|l| l.memberships.has_all(GameLayer::Window)))
        })
        .collect();
    boxes.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));

    let size = (MAX - MIN) * SCALE;
    let mut svg = String::new();
    writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" font-family="monospace" font-size="7">"#, size.x, size.y).unwrap();
    writeln!(svg, r##"<rect width="100%" height="100%" fill="#101014"/>"##).unwrap();
    // Every metre a faint line, every 2 m a stronger one, labelled every 10.
    for x in (MIN.x as i32)..=(MAX.x as i32) {
        let (sx, _) = px(Vec2::new(x as f32, 0.0));
        let stroke = if x % 10 == 0 { "#444" } else if x % 2 == 0 { "#262626" } else { "#1a1a1a" };
        writeln!(svg, r#"<line x1="{sx}" y1="0" x2="{sx}" y2="{}" stroke="{stroke}"/>"#, size.y).unwrap();
    }
    for z in (MIN.y as i32)..=(MAX.y as i32) {
        let (_, sy) = px(Vec2::new(0.0, z as f32));
        let stroke = if z % 10 == 0 { "#444" } else if z % 2 == 0 { "#262626" } else { "#1a1a1a" };
        writeln!(svg, r#"<line x1="0" y1="{sy}" x2="{}" y2="{sy}" stroke="{stroke}"/>"#, size.x).unwrap();
    }
    for (min, max, window) in &boxes {
        let (x0, y0) = px(Vec2::new(min.x, max.z));
        let (x1, y1) = px(Vec2::new(max.x, min.z));
        // Floors dark, sunken ones darker; what a Player can get on top of amber; walls and tall
        // props pale; anything up at the first storey see-through.
        let tall = max.y - min.y.max(0.0);
        let (fill, opacity, stroke) = if *window {
            ("#3070ff", 0.5, "#80a0ff")
        } else if max.y < -0.05 {
            ("#1d1d24", 0.9, "none")
        } else if max.y < 0.05 {
            ("#34343e", 0.9, "none")
        } else if min.y > 2.5 {
            ("#9a9aa4", 0.35, "#000")
        } else if tall <= 1.3 {
            ("#c0802c", 0.95, "#000")
        } else {
            ("#9a9aa4", 0.95, "#000")
        };
        writeln!(
            svg,
            r#"<rect x="{x0}" y="{y0}" width="{}" height="{}" fill="{fill}" fill-opacity="{opacity}" stroke="{stroke}" stroke-width="0.5"/>"#,
            x1 - x0,
            y1 - y0
        )
        .unwrap();
        // Label the tops of things you stand on or weave round.
        if max.y > 0.05 && max.y < 2.5 && (max.x - min.x) * (max.z - min.z) < 40.0 {
            writeln!(svg, r##"<text x="{}" y="{}" fill="#fff">{:.2}</text>"##, x0 + 1.0, y0 + 7.0, max.y).unwrap();
        }
    }
    let graph = NavGraph::new();
    for edge in &graph.edges {
        let (a, b) = (graph.nodes[edge.from].at, graph.nodes[edge.to].at);
        let colour = match edge.gate {
            Gate::Open => "#9a9a9a",
            Gate::Door(..) => "#ff5050",
            Gate::Barrier(_) => "#ffe040",
            Gate::OneWay => "#50a0ff",
        };
        let (x0, y0) = px(Vec2::new(a.x, a.z));
        let (x1, y1) = px(Vec2::new(b.x, b.z));
        writeln!(svg, r#"<line x1="{x0}" y1="{y0}" x2="{x1}" y2="{y1}" stroke="{colour}" stroke-width="1"/>"#).unwrap();
    }
    for (i, node) in graph.nodes.iter().enumerate() {
        let (x, y) = px(Vec2::new(node.at.x, node.at.z));
        writeln!(svg, r##"<circle cx="{x}" cy="{y}" r="2.5" fill="#fff"/>"##).unwrap();
        writeln!(svg, r##"<text x="{}" y="{}" fill="#cfc">{} {:.1}</text>"##, x + 3.0, y - 2.0, crate::nav::node_name(i), node.at.y).unwrap();
    }
    let spawns: Vec<(Vec3, SpawnKind)> = world
        .query::<(&SpawnPoint, &Transform)>()
        .iter(world)
        .map(|(spawn, transform)| (transform.translation, spawn.kind))
        .collect();
    for (at, kind) in spawns {
        let (x, y) = px(Vec2::new(at.x, at.z));
        let colour = match kind {
            SpawnKind::Window(_) => "#40c0ff",
            SpawnKind::Riser => "#c08040",
            SpawnKind::Hellhound => "#ff3030",
            SpawnKind::Special => "#ff40ff",
            SpawnKind::Vent => "#40ffff",
        };
        writeln!(svg, r#"<rect x="{}" y="{}" width="6" height="6" fill="none" stroke="{colour}" stroke-width="1.5"/>"#, x - 3.0, y - 3.0).unwrap();
    }
    for (name, at, clear) in crate::map::tests::fixtures() {
        let (x, y) = px(Vec2::new(at.x, at.z));
        writeln!(svg, r##"<circle cx="{x}" cy="{y}" r="{}" fill="none" stroke="#ff9000" stroke-width="1"/>"##, clear * SCALE).unwrap();
        writeln!(svg, r##"<text x="{}" y="{}" fill="#ffb060">{name}</text>"##, x - 10.0, y + 10.0).unwrap();
    }
    for zone in crate::zone::ZoneId::ALL {
        let (lo, hi) = zone.volume();
        let (x0, y0) = px(Vec2::new(lo.x, hi.z));
        let (x1, y1) = px(Vec2::new(hi.x, lo.z));
        writeln!(
            svg,
            r##"<rect x="{x0}" y="{y0}" width="{}" height="{}" fill="none" stroke="#ffffff" stroke-opacity="0.25" stroke-dasharray="4 3"/>"##,
            x1 - x0,
            y1 - y0
        )
        .unwrap();
        writeln!(svg, r##"<text x="{}" y="{}" fill="#fff" font-size="10">{}</text>"##, x0 + 3.0, y0 + 12.0, zone.name()).unwrap();
    }
    writeln!(svg, "</svg>").unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/map-plan.svg");
    std::fs::write(&path, svg).unwrap();
    println!("wrote {}", path.display());
}

