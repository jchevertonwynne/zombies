//! Water: still pools over a Zone's floor, and what depth makes of them.
//!
//! A `Pool` is a rectangle of still water in one Zone, with a surface `Level` over a `bed`. How
//! deep it is over the bed is all there is to decide what it does: under `DEEP_WATER_DEPTH` it's
//! Shallow Water, which slows anyone wading in it (a Player at `WADE_SPEED_MULT` through the
//! movement step, a Zombie through its `ZombieMoveFactor`), and from there it's Deep Water, a wall
//! to Players (invisible `GameLayer::WaterEdge` colliders), a gap in the path-node graph, and Out
//! of Bounds for anything that ends up in it anyway.
//!
//! `water_at` is the only function that reads pools; everything else asks it. Where a pool's
//! surface comes from is behind it: for now every Map's pools are `Level::Still`, and `canals` is
//! empty. The Tide (#374) will fill in `Level::Canal` and pass the current `TideLevel` in.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::enemies::target::half_height_above_feet;
use crate::layers::GameLayer;
use crate::maps::map::Layout;
use crate::players::player::{Locomotion, Player};
use crate::state::InRun;
use crate::tuning::*;

/// Which of a Map's `canals` a pool's surface follows, by its place in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanalId(pub u8);

/// A set of pools that share one surface, which rises and falls with the Tide through three
/// authored levels. No Map has one yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canal {
    pub name: &'static str,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

/// How high the Tide stands. With no Canals on a Map it never matters, and it's always `Low`
/// until the Tide has a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TideLevel {
    #[default]
    Low,
    Mid,
    High,
}

/// Where a pool's surface is: fixed, or following a Canal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    /// Still water at this height, always.
    Still(f32),
    /// The surface of this Canal, at whatever the Tide gives it.
    Canal(CanalId),
}

/// Still water over part of a Zone's floor: its surface at `level`, over a bed at `bed` (the
/// floor it lies on, authored and checked against what's built), across `min`..`max` (x, z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pool {
    pub zone: crate::maps::zone::ZoneId,
    pub min: Vec2,
    pub max: Vec2,
    pub level: Level,
    pub bed: f32,
}

/// What water feet are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wading {
    Shallow,
    Deep,
}

impl Pool {
    /// The height of its surface when the Tide is at `tide`.
    pub fn surface(&self, canals: &[Canal], tide: TideLevel) -> f32 {
        match self.level {
            Level::Still(height) => height,
            Level::Canal(CanalId(id)) => {
                let canal = canals[usize::from(id)];
                match tide {
                    TideLevel::Low => canal.low,
                    TideLevel::Mid => canal.mid,
                    TideLevel::High => canal.high,
                }
            }
        }
    }

    /// How deep it is over its bed.
    pub fn depth(&self, canals: &[Canal], tide: TideLevel) -> f32 {
        self.surface(canals, tide) - self.bed
    }

    /// What it is to stand in: Shallow under `DEEP_WATER_DEPTH`, Deep from there, or nothing if
    /// it has no depth at all.
    pub fn kind(&self, canals: &[Canal], tide: TideLevel) -> Option<Wading> {
        let depth = self.depth(canals, tide);
        if depth >= DEEP_WATER_DEPTH {
            Some(Wading::Deep)
        } else if depth > 0.0 {
            Some(Wading::Shallow)
        } else {
            None
        }
    }

    /// Whether (x, z) is over the pool, edges included.
    pub fn covers(&self, at: Vec2) -> bool {
        at.cmpge(self.min).all() && at.cmple(self.max).all()
    }
}

/// What water `feet` are in on `layout`'s Map with the Tide at `tide`: over a pool, below its
/// surface, and no more than `WATER_BED_SLACK` under its bed. Deep wins where pools overlap. This
/// is the only reader of `Pool`.
pub fn water_at(layout: &Layout, tide: TideLevel, feet: Vec3) -> Option<Wading> {
    water_in(layout.water, layout.canals, tide, feet)
}

/// `water_at` over a Map's pools and Canals themselves.
fn water_in(water: &[Pool], canals: &[Canal], tide: TideLevel, feet: Vec3) -> Option<Wading> {
    let at = Vec2::new(feet.x, feet.z);
    let mut found = None;
    for pool in water {
        if !pool.covers(at) || feet.y >= pool.surface(canals, tide) || feet.y < pool.bed - WATER_BED_SLACK {
            continue;
        }
        match pool.kind(canals, tide) {
            Some(Wading::Deep) => return Some(Wading::Deep),
            Some(Wading::Shallow) => found = Some(Wading::Shallow),
            None => {}
        }
    }
    found
}

/// Whether feet at `feet` are in Shallow Water, with the Tide at its usual `Low`.
pub fn wading(layout: &Layout, feet: Vec3) -> bool {
    water_at(layout, TideLevel::default(), feet) == Some(Wading::Shallow)
}

// ---- What it looks like, and the walls round Deep Water ----

/// Shallow Water: pale green-blue, a little over half opaque so the bed shows.
const SHALLOW: Color = Color::srgba(0.5, 0.78, 0.78, 0.55);
/// Deep Water: dark blue, near enough opaque to hide the sea bed.
const DEEP: Color = Color::srgba(0.01, 0.04, 0.1, 0.9);
/// The foam where one meets the other.
const FOAM: Color = Color::srgba(0.92, 0.97, 0.97, 0.85);
/// How far past the plan's south edge the sea is drawn, and how far to the sides.
const HORIZON: f32 = 800.0;
/// How far foam floats over the surface it lies on.
const FOAM_LIFT: f32 = 0.02;
/// How thick a Deep Water wall is.
const EDGE_THICKNESS: f32 = 0.4;
/// How near two edges are to be one (a pool's edge across a wall's thickness from the next).
const TOUCH: f32 = 0.35;

type Rect = (Vec2, Vec2, f32);

/// The foam strips of `layout`: along each side of a Shallow pool that meets a Deep one, as wide
/// as `WATER_FOAM_WIDTH` and as long as the two share.
fn foam(layout: &Layout) -> Vec<Rect> {
    let tide = TideLevel::default();
    let canals = layout.canals;
    let mut strips = Vec::new();
    for shallow in layout.water.iter().filter(|pool| pool.kind(canals, tide) == Some(Wading::Shallow)) {
        for deep in layout.water.iter().filter(|pool| pool.kind(canals, tide) == Some(Wading::Deep)) {
            let y = shallow.surface(canals, tide) + FOAM_LIFT;
            let across = (shallow.min.x.max(deep.min.x), shallow.max.x.min(deep.max.x));
            let along = (shallow.min.y.max(deep.min.y), shallow.max.y.min(deep.max.y));
            // Its south or north side meets the Deep pool's.
            if across.0 < across.1 {
                if (shallow.min.y - deep.max.y).abs() <= TOUCH {
                    strips.push((Vec2::new(across.0, shallow.min.y), Vec2::new(across.1, shallow.min.y + WATER_FOAM_WIDTH), y));
                }
                if (shallow.max.y - deep.min.y).abs() <= TOUCH {
                    strips.push((Vec2::new(across.0, shallow.max.y - WATER_FOAM_WIDTH), Vec2::new(across.1, shallow.max.y), y));
                }
            }
            if along.0 < along.1 {
                if (shallow.min.x - deep.max.x).abs() <= TOUCH {
                    strips.push((Vec2::new(shallow.min.x, along.0), Vec2::new(shallow.min.x + WATER_FOAM_WIDTH, along.1), y));
                }
                if (shallow.max.x - deep.min.x).abs() <= TOUCH {
                    strips.push((Vec2::new(shallow.max.x - WATER_FOAM_WIDTH, along.0), Vec2::new(shallow.max.x, along.1), y));
                }
            }
        }
    }
    strips
}

/// One flat mesh of every rectangle in `rects`, facing up.
fn quads(rects: &[Rect]) -> Mesh {
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for &(min, max, y) in rects {
        let first = positions.len() as u32;
        positions.extend([[min.x, y, min.y], [max.x, y, min.y], [max.x, y, max.y], [min.x, y, max.y]]);
        indices.extend([first, first + 2, first + 1, first, first + 3, first + 2]);
    }
    let count = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
        .with_inserted_indices(Indices::U32(indices))
}

fn surface_material(colour: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: colour,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.12,
        reflectance: 0.6,
        cull_mode: None,
        double_sided: true,
        ..default()
    }
}

/// Builds the Layout's water: one flat alpha-blended surface per pool (merged into one mesh each
/// for Shallow and Deep, so a Map's water is a few draws), foam where Shallow meets Deep, the sea
/// out to the horizon past a plan edge it reaches, and the invisible walls round Deep Water.
pub fn spawn_water(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, layout: &Layout) {
    if layout.water.is_empty() {
        return;
    }
    let tide = TideLevel::default();
    let canals = layout.canals;
    let mut shallow = Vec::new();
    let mut deep = Vec::new();
    let mut deeps = Vec::new();
    for pool in layout.water {
        let rect = (pool.min, pool.max, pool.surface(canals, tide));
        match pool.kind(canals, tide) {
            Some(Wading::Shallow) => shallow.push(rect),
            Some(Wading::Deep) => {
                deep.push(rect);
                deeps.push(*pool);
            }
            None => {}
        }
    }
    // A Deep pool that reaches the plan's south edge is the open sea: it runs on to the horizon,
    // and out to either side of the pools along that edge.
    let (plan_min, _) = layout.plan;
    let sea: Vec<&Pool> = deeps.iter().filter(|pool| pool.min.y <= plan_min.y + 0.01).collect();
    if let Some(&&first) = sea.first() {
        let level = first.surface(canals, tide);
        let west = sea.iter().map(|pool| pool.min.x).fold(f32::INFINITY, f32::min);
        let east = sea.iter().map(|pool| pool.max.x).fold(f32::NEG_INFINITY, f32::max);
        let top = deeps.iter().map(|pool| pool.max.y).fold(f32::NEG_INFINITY, f32::max);
        let middle = (west + east) / 2.0;
        deep.push((Vec2::new(middle - HORIZON, plan_min.y - HORIZON), Vec2::new(middle + HORIZON, plan_min.y), level));
        deep.push((Vec2::new(middle - HORIZON, plan_min.y), Vec2::new(west, top), level));
        deep.push((Vec2::new(east, plan_min.y), Vec2::new(middle + HORIZON, top), level));
    }
    for (rects, colour) in [(shallow, SHALLOW), (deep, DEEP), (foam(layout), FOAM)] {
        if rects.is_empty() {
            continue;
        }
        commands.spawn((
            InRun,
            Mesh3d(meshes.add(quads(&rects))),
            MeshMaterial3d(materials.add(surface_material(colour))),
            Transform::default(),
            bevy::light::NotShadowCaster,
        ));
    }
    // The walls: round every side of a Deep pool that doesn't run into another Deep pool.
    for pool in &deeps {
        let top = pool.surface(canals, tide) + WATER_EDGE_HEIGHT;
        let (low, high) = (pool.bed, top);
        let (size_y, y) = (high - low, (high + low) / 2.0);
        for (centre, size, outside) in [
            (Vec2::new((pool.min.x + pool.max.x) / 2.0, pool.min.y), Vec2::new(pool.max.x - pool.min.x, EDGE_THICKNESS), Vec2::NEG_Y),
            (Vec2::new((pool.min.x + pool.max.x) / 2.0, pool.max.y), Vec2::new(pool.max.x - pool.min.x, EDGE_THICKNESS), Vec2::Y),
            (Vec2::new(pool.min.x, (pool.min.y + pool.max.y) / 2.0), Vec2::new(EDGE_THICKNESS, pool.max.y - pool.min.y), Vec2::NEG_X),
            (Vec2::new(pool.max.x, (pool.min.y + pool.max.y) / 2.0), Vec2::new(EDGE_THICKNESS, pool.max.y - pool.min.y), Vec2::X),
        ] {
            let beyond = centre + outside * 0.05;
            if deeps.iter().any(|other| other != pool && other.covers(beyond)) {
                continue;
            }
            commands.spawn((
                InRun,
                Transform::from_xyz(centre.x, y, centre.y),
                RigidBody::Static,
                Collider::cuboid(size.x, size_y, size.y),
                CollisionLayers::new(GameLayer::WaterEdge, GameLayer::Player),
            ));
        }
    }
}

// ---- Who is in the water ----

/// On a Player or Zombie while its feet are in Shallow Water, so entering it can splash.
#[derive(Component, Debug)]
pub struct Wet;

/// Keeps `Wet` on whoever is in Shallow Water.
pub fn mark_wet(
    mut commands: Commands,
    bodies: Query<
        (Entity, &Transform, Option<&Locomotion>, Option<&crate::enemies::zombie::Zombie>, Has<Wet>),
        Or<(With<Player>, With<crate::enemies::zombie::Zombie>)>,
    >,
    map: Option<Res<crate::maps::map::MapId>>,
) {
    let Some(layout) = map.map(|map| map.layout()).filter(|layout| !layout.water.is_empty()) else {
        return;
    };
    for (entity, transform, locomotion, zombie, wet) in &bodies {
        let lift = zombie.map_or_else(|| half_height_above_feet(locomotion), |zombie| zombie.half_height());
        let now = wading(layout, transform.translation - Vec3::Y * lift);
        if now && !wet {
            commands.entity(entity).insert(Wet);
        } else if !now && wet {
            commands.entity(entity).remove::<Wet>();
        }
    }
}

/// Marks who's wading, for the splash.
pub struct WaterPlugin;

impl Plugin for WaterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, mark_wet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::zone::ZoneId;

    const BEACH: Pool = Pool { zone: ZoneId::Strand, min: Vec2::new(0.0, 0.0), max: Vec2::new(10.0, 10.0), level: Level::Still(0.5), bed: 0.0 };

    /// `water_at` over `water`, as the tests below ask it.
    struct Pools(&'static [Pool], &'static [Canal]);

    impl Pools {
        fn at(&self, tide: TideLevel, feet: Vec3) -> Option<Wading> {
            water_in(self.0, self.1, tide, feet)
        }
    }

    fn layout_of(water: &'static [Pool], canals: &'static [Canal]) -> Pools {
        Pools(water, canals)
    }

    #[test]
    fn a_pool_is_shallow_under_a_metre_and_deep_from_there() {
        let shallow = Pool { level: Level::Still(0.99), ..BEACH };
        let deep = Pool { level: Level::Still(1.0), ..BEACH };
        let none = Pool { level: Level::Still(0.0), ..BEACH };
        assert_eq!(shallow.kind(&[], TideLevel::Low), Some(Wading::Shallow));
        assert_eq!(deep.kind(&[], TideLevel::Low), Some(Wading::Deep));
        assert_eq!(none.kind(&[], TideLevel::Low), None);
    }

    #[test]
    fn water_at_answers_at_the_edges_and_under_the_surface() {
        static POOLS: [Pool; 1] = [BEACH];
        let layout = layout_of(&POOLS, &[]);
        let tide = TideLevel::Low;
        assert_eq!(layout.at(tide, Vec3::new(5.0, 0.0, 5.0)), Some(Wading::Shallow));
        // The edges are in; just past them is out.
        for (x, z) in [(0.0, 0.0), (10.0, 10.0), (0.0, 10.0), (10.0, 0.0)] {
            assert_eq!(layout.at(tide, Vec3::new(x, 0.0, z)), Some(Wading::Shallow), "corner {x},{z}");
        }
        for (x, z) in [(-0.01, 5.0), (10.01, 5.0), (5.0, -0.01), (5.0, 10.01)] {
            assert_eq!(layout.at(tide, Vec3::new(x, 0.0, z)), None, "past {x},{z}");
        }
        // Dry at the surface and over it, so a Player on a ledge over the water isn't wading.
        assert_eq!(layout.at(tide, Vec3::new(5.0, 0.49, 5.0)), Some(Wading::Shallow));
        assert_eq!(layout.at(tide, Vec3::new(5.0, 0.5, 5.0)), None);
        // And it isn't there under the floor it lies on.
        assert_eq!(layout.at(tide, Vec3::new(5.0, -WATER_BED_SLACK - 0.01, 5.0)), None);
    }

    #[test]
    fn water_at_splits_shallow_from_deep_round_the_one_metre_line() {
        static POOLS: [Pool; 3] = [
            Pool { level: Level::Still(0.99), ..BEACH },
            Pool { min: Vec2::new(20.0, 0.0), max: Vec2::new(30.0, 10.0), level: Level::Still(1.0), ..BEACH },
            Pool { min: Vec2::new(40.0, 0.0), max: Vec2::new(50.0, 10.0), level: Level::Still(1.01), ..BEACH },
        ];
        let layout = layout_of(&POOLS, &[]);
        let at = |x: f32| layout.at(TideLevel::Low, Vec3::new(x, 0.0, 5.0));
        assert_eq!(at(5.0), Some(Wading::Shallow));
        assert_eq!(at(25.0), Some(Wading::Deep));
        assert_eq!(at(45.0), Some(Wading::Deep));
    }

    #[test]
    fn a_canal_pool_follows_the_tide_and_deep_wins_where_pools_overlap() {
        static CANALS: [Canal; 1] = [Canal { name: "Quay", low: 0.2, mid: 0.6, high: 1.4 }];
        static POOLS: [Pool; 1] = [Pool { level: Level::Canal(CanalId(0)), ..BEACH }];
        let layout = layout_of(&POOLS, &CANALS);
        let feet = Vec3::new(5.0, 0.0, 5.0);
        assert_eq!(layout.at(TideLevel::Low, feet), Some(Wading::Shallow));
        assert_eq!(layout.at(TideLevel::Mid, feet), Some(Wading::Shallow));
        assert_eq!(layout.at(TideLevel::High, feet), Some(Wading::Deep));
        static OVERLAP: [Pool; 2] = [BEACH, Pool { level: Level::Still(2.0), ..BEACH }];
        assert_eq!(layout_of(&OVERLAP, &[]).at(TideLevel::Low, feet), Some(Wading::Deep));
    }

    #[test]
    fn a_map_without_pools_is_dry_everywhere() {
        for map in crate::maps::map::MapId::ALL {
            if map == crate::maps::map::MapId::Cove {
                continue;
            }
            assert!(map.layout().water.is_empty() && map.layout().canals.is_empty(), "{}", map.name());
            assert_eq!(water_at(map.layout(), TideLevel::Low, Vec3::ZERO), None);
        }
    }
}
