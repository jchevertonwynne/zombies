//! The Open rails' tests (`Builder::rail_as`).

use avian3d::prelude::SimpleCollider;
use bevy::ecs::system::RunSystemOnce;

use super::build::Builder;
use super::*;

/// What a rail builds in `style`: every collider (as the size and the middle of its box) and how many pieces are drawn.
fn built(style: RailStyle, a: Vec2, b: Vec2, y0: f32) -> (Vec<(Vec3, Vec3)>, usize) {
    let mut world = World::new();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<StandardMaterial>>();
    world
        .run_system_once(move |mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>| {
            let mut builder = Builder::new(&mut commands, &mut meshes, &mut materials, &[], MapId::Refinery);
            builder.rail_as(style, a, b, y0, 1.1);
        })
        .unwrap();
    let mut solids = world.query::<(&avian3d::prelude::Collider, &Transform)>();
    let mut boxes: Vec<(Vec3, Vec3)> = solids
        .iter(&world)
        .map(|(collider, at)| {
            let aabb = collider.aabb(at.translation, at.rotation, 0.0);
            (aabb.max - aabb.min, (aabb.max + aabb.min) / 2.0)
        })
        .collect();
    boxes.sort_by(|x, y| x.0.to_array().partial_cmp(&y.0.to_array()).unwrap().then(x.1.to_array().partial_cmp(&y.1.to_array()).unwrap()));
    let drawn = world.query::<&Mesh3d>().iter(&world).count();
    (boxes, drawn)
}

/// An Open railing is the very block a Solid one is, as far as anything can bump into it: the same collider, 1.1 m high and a wall thick, on
/// the same line; what it adds is drawn, never solid.
#[test]
fn open_rails_keep_the_rail_collider() {
    for (a, b, y0) in [(Vec2::new(-4.0, 20.0), Vec2::new(-4.0, 32.0), 4.0), (Vec2::new(-58.0, 30.0), Vec2::new(-50.0, 30.0), 4.0), (Vec2::new(4.0, 28.5), Vec2::new(5.0, 28.5), 4.0)] {
        let (solid, solid_drawn) = built(RailStyle::Solid, a, b, y0);
        let (open, open_drawn) = built(RailStyle::Open, a, b, y0);
        assert_eq!(solid.len(), 1);
        assert_eq!(open, solid, "the rail from {a} to {b}");
        let length = (b - a).length();
        let size = open[0].0;
        assert!((size.y - 1.1).abs() < 1e-4 && (size.x.max(size.z) - length).abs() < 1e-4 && (size.x.min(size.z) - WALL_THICKNESS).abs() < 1e-4);
        // A top rail, a mid rail, a kick plate and a post every 1.5 m (one at each end): drawn pieces, and not one solid.
        let posts = (length / 1.5).ceil().max(1.0) as usize + 1;
        assert_eq!(open_drawn, 3 + posts);
        assert_ne!(solid_drawn, open_drawn);
    }
}

/// Every Map but the Refinery draws its railings as plain solid blocks, as it always has: only the Refinery's catwalks have Open ones.
#[test]
fn other_maps_rails_are_solid() {
    for map in MapId::ALL {
        let want = if map == MapId::Refinery { RailStyle::Open } else { RailStyle::Solid };
        assert_eq!(map.layout().rail_style, want, "{map:?}");
    }
}

/// The Refinery has Infills in its three sheds and none in a Zone out of doors.
#[test]
fn the_refinery_has_no_infill_out_of_doors() {
    let layout = MapId::Refinery.layout();
    assert_eq!(layout.infills.len(), 7);
    for infill in layout.infills {
        assert!(layout.indoor(infill.zone), "the Infill at {}..{} is in the out-of-doors {}", infill.min, infill.max, infill.zone.name());
    }
    // The sheds' Infills stand to the top of their 3.6 m walls (not the storey over them), and the Control House's a storey up.
    assert_eq!(layout.infills[0].span(), (0.0, 3.6));
    assert_eq!(layout.infills[6].span(), (4.0, 8.0));
}
