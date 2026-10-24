use avian3d::prelude::*;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::state::InRun;
use crate::tuning::*;

/// A fixed place in the Map where Zombies enter play.
#[derive(Component)]
pub struct SpawnPoint;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalAmbientLight {
            color: Color::srgb(0.6, 0.65, 0.8),
            brightness: 120.0,
            ..default()
        })
        .insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)));
        // PROTOTYPE: the greybox layouts in map_prototype replace the Arena on this branch.
        let _ = spawn_arena;
    }
}

fn spawn_arena(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let floor_mat = materials.add(Color::srgb(0.22, 0.2, 0.18));
    let wall_mat = materials.add(Color::srgb(0.35, 0.3, 0.27));
    let pillar_mat = materials.add(Color::srgb(0.45, 0.42, 0.4));
    let crate_mat = materials.add(Color::srgb(0.4, 0.28, 0.15));

    let size = ARENA_HALF_SIZE * 2.0;
    let world_layers = CollisionLayers::new(GameLayer::World, LayerMask::ALL);

    let mut solid = |commands: &mut Commands, shape: Cuboid, mat: &Handle<StandardMaterial>, at: Vec3| {
        commands.spawn((
            InRun,
            Mesh3d(meshes.add(shape)),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at),
            RigidBody::Static,
            Collider::from(shape),
            world_layers,
        ));
    };

    // Floor
    solid(&mut commands, Cuboid::new(size, 0.2, size), &floor_mat, Vec3::new(0.0, -0.1, 0.0));

    // Perimeter walls
    let half = ARENA_HALF_SIZE;
    let wall_y = WALL_HEIGHT / 2.0;
    for (shape, at) in [
        (Cuboid::new(size, WALL_HEIGHT, 0.5), Vec3::new(0.0, wall_y, -half)),
        (Cuboid::new(size, WALL_HEIGHT, 0.5), Vec3::new(0.0, wall_y, half)),
        (Cuboid::new(0.5, WALL_HEIGHT, size), Vec3::new(-half, wall_y, 0.0)),
        (Cuboid::new(0.5, WALL_HEIGHT, size), Vec3::new(half, wall_y, 0.0)),
    ] {
        solid(&mut commands, shape, &wall_mat, at);
    }

    // Pillars
    let pillar = Cuboid::new(1.2, WALL_HEIGHT, 1.2);
    for (x, z) in [(-8.0, -8.0), (8.0, -8.0), (-8.0, 8.0), (8.0, 8.0)] {
        solid(&mut commands, pillar, &pillar_mat, Vec3::new(x, wall_y, z));
    }

    // Crates for cover
    for (x, z, w, d) in [
        (0.0, -12.0, 3.0, 1.0),
        (0.0, 12.0, 3.0, 1.0),
        (-13.0, 0.0, 1.0, 3.0),
        (13.0, 0.0, 1.0, 3.0),
        (4.0, 3.0, 1.2, 1.2),
        (-4.0, -3.0, 1.2, 1.2),
    ] {
        solid(&mut commands, Cuboid::new(w, 1.1, d), &crate_mat, Vec3::new(x, 0.55, z));
    }

    // Spawn Points, just inside each corner and midway along each wall
    let inset = half - 1.5;
    for (x, z) in [
        (-inset, -inset),
        (inset, -inset),
        (-inset, inset),
        (inset, inset),
        (0.0, -inset),
        (0.0, inset),
        (-inset, 6.0),
        (inset, -6.0),
    ] {
        commands.spawn((InRun, SpawnPoint, Transform::from_xyz(x, 0.0, z)));
    }

    // Lighting: a dim moon plus a few warm lamps
    commands.spawn((
        InRun,
        DirectionalLight {
            illuminance: 800.0,
            shadow_maps_enabled: true,
            color: Color::srgb(0.7, 0.75, 1.0),
            ..default()
        },
        Transform::default().looking_at(Vec3::new(-1.0, -3.0, -2.0), Vec3::Y),
    ));
    for (x, z) in [(-10.0, 0.0), (10.0, 0.0), (0.0, -10.0), (0.0, 10.0)] {
        commands.spawn((
            InRun,
            PointLight {
                color: Color::srgb(1.0, 0.75, 0.45),
                intensity: 400_000.0,
                range: 25.0,
                shadow_maps_enabled: true,
                ..default()
            },
            Transform::from_xyz(x, WALL_HEIGHT - 0.5, z),
        ));
    }
}
