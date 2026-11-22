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
//! surface comes from is behind it: a `Level::Still` pool stands at one height, and a
//! `Level::Canal` pool follows its Canal through the Tide's three levels (see `maps::tide`), which
//! `water_at` takes as the `TideLevel` the rules stand at. The drawn surface moves over a Turn; the
//! rules change only when the Tide lands.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::enemies::target::half_height_above_feet;
use crate::layers::GameLayer;
use crate::maps::map::Layout;
use crate::players::player::{Locomotion, Player};
use crate::enemies::zombie::rig::{Activity, Shown};
use crate::enemies::zombie::{Rising, Zombie, rising_floor};
use crate::net::play::Puppet;
use crate::maps::tide::Tide;
use crate::state::InRun;
use crate::tuning::*;

/// Which of a Map's `canals` a pool's surface follows, by its place in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanalId(pub u8);

/// A set of pools that share one surface, which rises and falls with the Tide through three
/// authored levels. Only the Polder has any.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Canal {
    pub name: &'static str,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

impl Canal {
    /// The height of its surface with the Tide at `tide`.
    pub fn at(&self, tide: TideLevel) -> f32 {
        match tide {
            TideLevel::Low => self.low,
            TideLevel::Mid => self.mid,
            TideLevel::High => self.high,
        }
    }
}

/// How high the Tide stands. With no Canals on a Map it never matters, and it's always `Low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TideLevel {
    #[default]
    Low,
    Mid,
    High,
}

impl TideLevel {
    pub const ALL: [TideLevel; 3] = [TideLevel::Low, TideLevel::Mid, TideLevel::High];

    /// Its place in a table of the three levels.
    pub fn index(self) -> usize {
        self as usize
    }
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
            Level::Canal(CanalId(id)) => canals[usize::from(id)].at(tide),
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

/// Whether feet at `feet` are in Shallow Water, with the Tide at `tide`.
pub fn wading(layout: &Layout, tide: TideLevel, feet: Vec3) -> bool {
    water_at(layout, tide, feet) == Some(Wading::Shallow)
}

/// Whether a body with its feet at `feet` is caught by the Tide landing at `tide`: in Deep Water,
/// or in the air over it, within `WASHED_UP_AIR_REACH` of the surface. Whoever is, is Washed Up.
pub fn swamped(layout: &Layout, tide: TideLevel, feet: Vec3) -> bool {
    if water_at(layout, tide, feet) == Some(Wading::Deep) {
        return true;
    }
    let at = Vec2::new(feet.x, feet.z);
    layout.water.iter().any(|pool| {
        pool.covers(at)
            && pool.kind(layout.canals, tide) == Some(Wading::Deep)
            && (0.0..=WASHED_UP_AIR_REACH).contains(&(feet.y - pool.surface(layout.canals, tide)))
    })
}

/// How high the surface of the Shallow Water is that `feet` stand in, with the Tide at `tide`.
pub fn surface_at(layout: &Layout, tide: TideLevel, feet: Vec3) -> Option<f32> {
    let at = Vec2::new(feet.x, feet.z);
    if water_at(layout, tide, feet) != Some(Wading::Shallow) {
        return None;
    }
    layout
        .water
        .iter()
        .filter(|pool| pool.covers(at) && pool.kind(layout.canals, tide) == Some(Wading::Shallow))
        .map(|pool| pool.surface(layout.canals, tide))
        .find(|&surface| feet.y < surface)
}

/// At which Tide levels a Wading Point at `at` (a spot on the bed) is in use: where the water over
/// it is Shallow, with `WADING_DEEP_CLEARANCE` between it and Deep Water and dry ground all round
/// (a hair under, as Deep Water and the shallows share an edge). Worked out once per point and
/// remembered. On the Cove everything is Still, so every one always is; when the Tide raises the
/// water to Deep, or drains it, a point goes quiet.
pub fn wading_levels(layout: &Layout, at: Vec3) -> [bool; 3] {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static TABLES: OnceLock<Mutex<HashMap<(usize, [u32; 3]), [bool; 3]>>> = OnceLock::new();
    let key = (std::ptr::from_ref(layout) as usize, at.to_array().map(f32::to_bits));
    let tables = TABLES.get_or_init(Mutex::default);
    if let Some(&table) = tables.lock().expect("the table is never held across a panic").get(&key) {
        return table;
    }
    let table = TideLevel::ALL.map(|tide| water_at(layout, tide, at) == Some(Wading::Shallow) && shallow_all_round(layout, tide, at, WADING_DEEP_CLEARANCE - 0.01));
    tables.lock().expect("the table is never held across a panic").insert(key, table);
    table
}

/// Whether a Wading Point at `at` is in use with the Tide at `tide`: see `wading_levels`.
pub fn wading_point_active(layout: &Layout, tide: TideLevel, at: Vec3) -> bool {
    wading_levels(layout, at)[tide.index()]
}

/// Whether all the ground within `radius` of the spot `at` (the bed under it) is Shallow Water:
/// no Deep Water and no dry ground that close.
pub fn shallow_all_round(layout: &Layout, tide: TideLevel, at: Vec3, radius: f32) -> bool {
    (0..4).all(|ring| {
        let reach = radius * (ring + 1) as f32 / 4.0;
        (0..48).all(|step| {
            let angle = step as f32 / 48.0 * std::f32::consts::TAU;
            let feet = at + Vec3::new(angle.cos(), 0.0, angle.sin()) * reach;
            water_at(layout, tide, feet) == Some(Wading::Shallow)
        })
    })
}

/// The Zone of a pool that is Shallow over `feet`, if one is.
#[cfg(test)]
pub fn shallow_zone_at(layout: &Layout, tide: TideLevel, feet: Vec3) -> Option<crate::maps::zone::ZoneId> {
    let at = Vec2::new(feet.x, feet.z);
    layout
        .water
        .iter()
        .find(|pool| {
            pool.covers(at) && feet.y < pool.surface(layout.canals, tide) && pool.kind(layout.canals, tide) == Some(Wading::Shallow)
        })
        .map(|pool| pool.zone)
}

/// The cue for a Zombie rising with its feet at `floor`: the surge of water in Shallow Water,
/// otherwise the Riser's crunch of dirt. The water decides, not the Spawn Point's kind, so every
/// machine, a Co-op client's puppets included, answers alike with no message.
pub fn rise_cue(layout: &Layout, tide: TideLevel, floor: Vec3) -> Cue {
    if water_at(layout, tide, floor) == Some(Wading::Shallow) {
        Cue::ZombieWade
    } else {
        Cue::ZombieRiser
    }
}

// ---- What it looks like, and the walls round Deep Water ----

/// Shallow Water: pale green-blue, a little over half opaque so the bed shows.
const SHALLOW: Color = Color::srgba(0.5, 0.78, 0.78, 0.55);
/// Deep Water: a teal-blue the sky and the sun can lift (not black: the Cove lies under a golden
/// hour), near enough opaque to hide the sea bed.
const DEEP: Color = Color::srgba(0.03, 0.2, 0.3, 0.9);
/// The foam where one meets the other.
const FOAM: Color = Color::srgba(0.92, 0.97, 0.97, 0.85);
/// The lighter silt a Canal shows over a bed it barely covers: the Fords at Low.
const SILT: Color = Color::srgba(0.66, 0.62, 0.45, 0.5);
/// How far past the plan's south edge the sea is drawn, and how far to the sides.
const HORIZON: f32 = 800.0;
/// How far foam floats over the surface it lies on.
const FOAM_LIFT: f32 = 0.02;
/// How thick a Deep Water wall is.
const EDGE_THICKNESS: f32 = 0.4;
/// How near two edges are to be one (a pool's edge across a wall's thickness from the next).
const TOUCH: f32 = 0.35;

type Rect = (Vec2, Vec2, f32);

/// What a Canal's surface looks like over a bed `depth` under it: silt over almost nothing, then
/// the shallows, then the deep, darkening to the deepest reach. Nothing over a bed it doesn't cover.
fn depth_colour(depth: f32) -> Color {
    if depth <= 0.0 {
        return Color::srgba(0.0, 0.0, 0.0, 0.0);
    }
    const STOPS: [(f32, Color); 4] = [(0.0, SILT), (0.7, SHALLOW), (1.1, Color::srgba(0.05, 0.28, 0.36, 0.8)), (3.0, DEEP)];
    for pair in STOPS.windows(2) {
        let ((d0, c0), (d1, c1)) = (pair[0], pair[1]);
        if depth <= d1 {
            return Color::Srgba(c0.to_srgba().mix(&c1.to_srgba(), (depth - d0) / (d1 - d0)));
        }
    }
    DEEP
}

/// The foam strips of `layout` with the Tide at `tide`: along each side of a Shallow pool that
/// meets a Deep one, as wide as `WATER_FOAM_WIDTH` and as long as the two share. `of` picks the
/// Shallow pools: the Still ones (`None`) or one Canal's.
fn foam(layout: &Layout, tide: TideLevel, of: Option<CanalId>) -> Vec<Rect> {
    let canals = layout.canals;
    let wanted = |pool: &Pool| match (pool.level, of) {
        (Level::Still(_), None) => true,
        (Level::Canal(canal), Some(id)) => canal == id,
        _ => false,
    };
    let mut strips = Vec::new();
    for shallow in layout.water.iter().filter(|pool| wanted(pool) && pool.kind(canals, tide) == Some(Wading::Shallow)) {
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
    strips.extend(bank_foam(layout, tide, &wanted));
    strips
}

/// The foam along the sides of each Deep pool `wanted` picks that meet dry ground, a strip as wide
/// as `WATER_FOAM_WIDTH` inside the pool: the line that says "Deep from here" before a Player
/// reaches it. Not along a side that runs into other water (Deep, or Shallow, which has its own
/// foam), or the plan's edge (the open sea to the horizon).
fn bank_foam(layout: &Layout, tide: TideLevel, wanted: &dyn Fn(&Pool) -> bool) -> Vec<Rect> {
    let canals = layout.canals;
    let (plan_min, plan_max) = layout.plan;
    let mut strips = Vec::new();
    for pool in layout.water.iter().filter(|pool| wanted(pool) && pool.kind(canals, tide) == Some(Wading::Deep)) {
        let y = pool.surface(canals, tide) + FOAM_LIFT;
        let width = WATER_FOAM_WIDTH;
        let (min, max) = (pool.min, pool.max);
        let mid = (min + max) / 2.0;
        // (strip, a point just outside that side, whether the side is the plan's edge)
        let sides = [
            ((min, Vec2::new(max.x, min.y + width)), Vec2::new(mid.x, min.y - 0.1), min.y <= plan_min.y + 0.01),
            ((Vec2::new(min.x, max.y - width), max), Vec2::new(mid.x, max.y + 0.1), max.y >= plan_max.y - 0.01),
            ((min, Vec2::new(min.x + width, max.y)), Vec2::new(min.x - 0.1, mid.y), min.x <= plan_min.x + 0.01),
            ((Vec2::new(max.x - width, min.y), max), Vec2::new(max.x + 0.1, mid.y), max.x >= plan_max.x - 0.01),
        ];
        for ((from, to), beyond, plan_edge) in sides {
            if plan_edge || layout.water.iter().any(|other| other != pool && other.covers(beyond) && other.kind(canals, tide).is_some()) {
                continue;
            }
            strips.push((from, to, y));
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

/// On a mesh of the pools of one Canal over beds at `bed`, drawn flat at height 0 and posed by its
/// transform at the Canal's drawn level: its colour follows the depth over `bed` there.
#[derive(Component, Debug)]
pub struct CanalSurface {
    pub canal: CanalId,
    pub bed: f32,
}

/// On the foam of one Canal's Shallow pools at one Tide level, drawn at that level's surface and
/// moved with the Canal's drawn one: it fades in as the Tide turns to its level and out as it
/// turns away.
#[derive(Component, Debug)]
pub struct CanalFoam {
    pub canal: CanalId,
    pub level: TideLevel,
}

/// On an invisible wall round Deep Water at one Tide level: only the current level's set is on.
#[derive(Component, Debug)]
pub struct TideWall(pub TideLevel);

/// The walls round every side of a Deep pool in `deeps` that doesn't run into another one, as
/// (centre, size, lowest, highest) of each.
pub fn wall_rects(deeps: &[Pool], canals: &[Canal], tide: TideLevel) -> Vec<(Vec2, Vec2, f32, f32)> {
    let mut walls = Vec::new();
    for pool in deeps {
        let top = pool.surface(canals, tide) + WATER_EDGE_HEIGHT;
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
            walls.push((centre, size, pool.bed, top));
        }
    }
    walls
}

/// Builds the Layout's water: one flat alpha-blended surface per kind of Still pool (merged into
/// one mesh each for Shallow and Deep, so a Map's water is a few draws), one per Canal and bed
/// that is posed each frame at the Canal's drawn level, foam where Shallow meets Deep (a set per
/// Tide level for each Canal), the sea out to the horizon past a plan edge it reaches, and the
/// invisible walls round Deep Water (a set per Tide level on a Map with Canals).
pub fn spawn_water(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, layout: &Layout) {
    if layout.water.is_empty() {
        return;
    }
    let tide = TideLevel::default();
    let canals = layout.canals;
    let mut shallow = Vec::new();
    let mut deep = Vec::new();
    let mut deeps = Vec::new();
    // Each Canal's pools over each bed, drawn flat at height 0.
    let mut surfaces: Vec<(CanalId, f32, Vec<Rect>)> = Vec::new();
    fn add(surfaces: &mut Vec<(CanalId, f32, Vec<Rect>)>, canal: CanalId, bed: f32, rect: Rect) {
        match surfaces.iter_mut().find(|(id, at, _)| *id == canal && at.to_bits() == bed.to_bits()) {
            Some((.., rects)) => rects.push(rect),
            None => surfaces.push((canal, bed, vec![rect])),
        }
    }
    for pool in layout.water {
        match pool.level {
            Level::Still(height) => match pool.kind(canals, tide) {
                Some(Wading::Shallow) => shallow.push((pool.min, pool.max, height)),
                Some(Wading::Deep) => deep.push((pool.min, pool.max, height)),
                None => {}
            },
            Level::Canal(canal) => add(&mut surfaces, canal, pool.bed, (pool.min, pool.max, 0.0)),
        }
        if pool.kind(canals, tide) == Some(Wading::Deep) {
            deeps.push(*pool);
        }
    }
    // A Deep pool that reaches the plan's south edge is the open sea: it runs on to the horizon,
    // and out to either side of the pools along that edge. The same on the north edge.
    let (plan_min, plan_max) = layout.plan;
    let mut horizon = |pools: &[&Pool], rects: [(Vec2, Vec2); 3]| {
        let Some(&&first) = pools.first() else { return };
        for (min, max) in rects {
            match first.level {
                Level::Still(height) => deep.push((min, max, height)),
                Level::Canal(canal) => add(&mut surfaces, canal, first.bed, (min, max, 0.0)),
            }
        }
    };
    let span = |pools: &[&Pool]| {
        let west = pools.iter().map(|pool| pool.min.x).fold(f32::INFINITY, f32::min);
        let east = pools.iter().map(|pool| pool.max.x).fold(f32::NEG_INFINITY, f32::max);
        (west, east, (west + east) / 2.0)
    };
    let sea: Vec<&Pool> = deeps.iter().filter(|pool| pool.min.y <= plan_min.y + 0.01).collect();
    if !sea.is_empty() {
        let (west, east, middle) = span(&sea);
        let top = deeps.iter().map(|pool| pool.max.y).fold(f32::NEG_INFINITY, f32::max);
        horizon(
            &sea,
            [
                (Vec2::new(middle - HORIZON, plan_min.y - HORIZON), Vec2::new(middle + HORIZON, plan_min.y)),
                (Vec2::new(middle - HORIZON, plan_min.y), Vec2::new(west, top)),
                (Vec2::new(east, plan_min.y), Vec2::new(middle + HORIZON, top)),
            ],
        );
    }
    let north: Vec<&Pool> = deeps.iter().filter(|pool| pool.max.y >= plan_max.y - 0.01).collect();
    if !north.is_empty() {
        let (west, east, middle) = span(&north);
        let bottom = north.iter().map(|pool| pool.min.y).fold(f32::INFINITY, f32::min);
        horizon(
            &north,
            [
                (Vec2::new(middle - HORIZON, plan_max.y), Vec2::new(middle + HORIZON, plan_max.y + HORIZON)),
                (Vec2::new(middle - HORIZON, bottom), Vec2::new(west, plan_max.y)),
                (Vec2::new(east, bottom), Vec2::new(middle + HORIZON, plan_max.y)),
            ],
        );
    }
    for (rects, colour) in [(shallow, SHALLOW), (deep, DEEP), (foam(layout, tide, None), FOAM)] {
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
    for (canal, bed, rects) in surfaces {
        let at = canals[usize::from(canal.0)].at(tide);
        commands.spawn((
            InRun,
            CanalSurface { canal, bed },
            Mesh3d(meshes.add(quads(&rects))),
            MeshMaterial3d(materials.add(surface_material(depth_colour(at - bed)))),
            Transform::from_xyz(0.0, at, 0.0),
            bevy::light::NotShadowCaster,
        ));
    }
    for index in 0..canals.len() {
        let canal = CanalId(index as u8);
        for level in TideLevel::ALL {
            let rects = foam(layout, level, Some(canal));
            if rects.is_empty() {
                continue;
            }
            let alpha = if level == tide { 1.0 } else { 0.0 };
            commands.spawn((
                InRun,
                CanalFoam { canal, level },
                Mesh3d(meshes.add(quads(&rects))),
                MeshMaterial3d(materials.add(surface_material(FOAM.with_alpha(FOAM.alpha() * alpha)))),
                Transform::default(),
                bevy::light::NotShadowCaster,
            ));
        }
    }
    // The walls: round every side of a Deep pool that doesn't run into another Deep pool. A Map
    // with Canals has a set for each Tide level, and only the current one is on; any other Map's
    // are always on.
    let levels: &[TideLevel] = if canals.is_empty() { &[TideLevel::Low] } else { &TideLevel::ALL };
    for &level in levels {
        let at_level: Vec<Pool> = layout.water.iter().copied().filter(|pool| pool.kind(canals, level) == Some(Wading::Deep)).collect();
        for (centre, size, low, high) in wall_rects(&at_level, canals, level) {
            let (size_y, y) = (high - low, (high + low) / 2.0);
            // A Leaf of a Piece standing there at the start lets the way onto its deck through.
            let at = Vec3::new(centre.x, 0.0, centre.y);
            // A Lock Gate's mouths are let through at either Rest: Shut, its walkway crosses them,
            // and Open, the rail's gap leads to Deep Water, which is Out of Bounds like any other.
            if layout.shifting.iter().any(|piece| {
                let rests = if matches!(piece.driver, crate::maps::shifting::Driver::Tide(_)) { 0..piece.rests.len() } else { usize::from(piece.start)..usize::from(piece.start) + 1 };
                rests.into_iter().any(|rest| piece.leaves.iter().enumerate().any(|(i, leaf)| leaf.over(at.with_y(leaf.hinge.y), piece.rests[rest].yaws[i], 0.3)))
            }) {
                continue;
            }
            let mut wall = commands.spawn((
                InRun,
                Transform::from_xyz(centre.x, y, centre.y),
                RigidBody::Static,
                Collider::cuboid(size.x, size_y, size.y),
                CollisionLayers::new(GameLayer::WaterEdge, GameLayer::Player),
            ));
            if !canals.is_empty() {
                wall.insert(TideWall(level));
                if level != tide {
                    wall.insert(ColliderDisabled);
                }
            }
        }
    }
}

/// Poses each Canal's water at its drawn level and colours it by the depth there, and fades the
/// foam of each Tide level in and out, whenever the Tide moves.
fn pose_canals(
    tide: Option<Res<Tide>>,
    map: Option<Res<crate::maps::map::MapId>>,
    mut surfaces: Query<(&CanalSurface, &mut Transform, &MeshMaterial3d<StandardMaterial>)>,
    mut foams: Query<(&CanalFoam, &mut Transform, &MeshMaterial3d<StandardMaterial>, &mut Visibility), Without<CanalSurface>>,
    fresh: Query<(), Or<(Added<CanalSurface>, Added<CanalFoam>)>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(map) = map else { return };
    let canals = map.layout().canals;
    if canals.is_empty() {
        return;
    }
    let tide_now = tide.as_ref().map_or_else(Tide::default, |tide| **tide);
    if tide.as_ref().is_some_and(|tide| !tide.is_changed()) && fresh.is_empty() {
        return;
    }
    for (surface, mut transform, material) in &mut surfaces {
        let at = tide_now.drawn(&canals[usize::from(surface.canal.0)]);
        transform.translation.y = at;
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base_color = depth_colour(at - surface.bed);
        }
    }
    for (foam, mut transform, material, mut visibility) in &mut foams {
        let canal = &canals[usize::from(foam.canal.0)];
        transform.translation.y = tide_now.drawn(canal) - canal.at(foam.level);
        let weight = tide_now.foam_weight(foam.level);
        *visibility = if weight > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.base_color = FOAM.with_alpha(FOAM.alpha() * weight);
        }
    }
}

/// Switches the Deep Water walls to the current Tide level's set: only that one is on.
fn switch_walls(mut commands: Commands, tide: Option<Res<Tide>>, walls: Query<(Entity, &TideWall, Has<ColliderDisabled>)>) {
    let level = tide.map_or_else(TideLevel::default, |tide| tide.level());
    for (entity, wall, off) in &walls {
        match (wall.0 == level, off) {
            (true, true) => {
                commands.entity(entity).remove::<ColliderDisabled>();
            }
            (false, false) => {
                commands.entity(entity).insert(ColliderDisabled);
            }
            _ => {}
        }
    }
}

// ---- Standing up out of the surf ----

/// On a Zombie that has started rising, once its cue and look have been chosen: where its feet
/// meet the surface if it is rising in Shallow Water (a Wading Point's Zombie, or any that rises
/// there), and `None` on dry ground. Host and puppet alike, each by the water at its own feet.
#[derive(Component, Debug)]
pub struct Rise {
    surface: Option<Vec3>,
}

/// The ring of foam spreading over the surface from where a Zombie surfaces.
#[derive(Component)]
struct FoamRing {
    age: f32,
    size: f32,
    material: Handle<StandardMaterial>,
}

/// One drop of spray, thrown up on a fixed arc and back down: no physics.
#[derive(Component)]
struct Spray {
    age: f32,
    from: Vec3,
    drift: Vec3,
    up: f32,
    gravity: f32,
}

/// The meshes and material of the splash, made when first needed.
struct SplashLook {
    ring: Handle<Mesh>,
    drop: Handle<Mesh>,
    spray: Handle<StandardMaterial>,
}

/// The splash's meshes and material, made the first time they're needed.
fn splash_look<'a>(look: &'a mut Option<SplashLook>, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> &'a SplashLook {
    look.get_or_insert_with(|| SplashLook {
        ring: meshes.add(Annulus::new(0.38, 0.5)),
        drop: meshes.add(Sphere::new(0.05)),
        spray: materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.98, 1.0, 0.8),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    })
}

/// A ring of foam and a burst of spray at `surface`, `scale` times the full size.
fn splash(
    commands: &mut Commands,
    look: &SplashLook,
    materials: &mut Assets<StandardMaterial>,
    surface: Vec3,
    scale: f32,
    drops: usize,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.98, 1.0, 0.7),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        InRun,
        FoamRing { age: 0.0, size: WADE_FOAM_RING_SIZE * scale, material: material.clone() },
        Mesh3d(look.ring.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(surface + Vec3::Y * 0.03).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(0.01)),
    ));
    let height = WADE_SPRAY_HEIGHT * scale;
    for _ in 0..drops {
        let angle = rand::random_range(0.0..std::f32::consts::TAU);
        let reach = rand::random_range(0.1..0.45) * scale;
        commands.spawn((
            InRun,
            Spray {
                age: 0.0,
                from: surface,
                drift: Vec3::new(angle.cos(), 0.0, angle.sin()) * reach / WADE_SPRAY_SECS,
                up: 4.0 * height / WADE_SPRAY_SECS * rand::random_range(0.75..1.0),
                gravity: 8.0 * height / (WADE_SPRAY_SECS * WADE_SPRAY_SECS),
            },
            Mesh3d(look.drop.clone()),
            MeshMaterial3d(look.spray.clone()),
            Transform::from_translation(surface),
        ));
    }
}

/// Gives each Zombie that has started rising its cue and, in Shallow Water, its look: a ring of
/// foam spreading over the surface and a burst of spray, and the surge of water instead of the
/// crunch of dirt. It is chosen by `water_at` at the Zombie's feet, so the Host's Zombies and a
/// Co-op client's puppets (which the Host tells only that they are `Activity::Rising`) answer the
/// same way, and no message is needed.
fn start_rising(
    mut commands: Commands,
    hosts: Query<(Entity, &Transform, &Zombie, &Rising), (Added<Rising>, Without<Rise>)>,
    puppets: Query<(Entity, &Transform, &Zombie, &Shown), (With<Puppet>, Changed<Shown>, Without<Rise>)>,
    map: Option<Res<crate::maps::map::MapId>>,
    tide: Option<Res<Tide>>,
    mut cues: MessageWriter<PlayCue>,
    mut look: Local<Option<SplashLook>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(map) = map else { return };
    let layout = map.layout();
    let tide = tide.map_or_else(TideLevel::default, |tide| tide.level());
    let rising = hosts
        .iter()
        .map(|(entity, transform, zombie, rising)| (entity, transform.translation, zombie, rising.buried()))
        .chain(
            puppets
                .iter()
                .filter(|(.., shown)| shown.0 == Activity::Rising)
                .map(|(entity, transform, zombie, _)| (entity, transform.translation, zombie, transform.translation.y)),
        );
    for (entity, at, zombie, centre_y) in rising {
        let floor = Vec3::new(at.x, rising_floor(centre_y), at.z);
        let surface = surface_at(layout, tide, floor).map(|height| Vec3::new(at.x, height, at.z));
        commands.entity(entity).try_insert(Rise { surface });
        // A Nova Crawler drops from its Vent, and a special rises with its own sound.
        if !zombie.is_special() && !zombie.is_hellhound() {
            cues.write(PlayCue::new(rise_cue(layout, tide, floor), Some(surface.unwrap_or(at))));
        }
        let (Some(surface), Some(meshes), Some(materials)) = (surface, meshes.as_mut(), materials.as_mut()) else { continue };
        let look = splash_look(&mut look, meshes, materials);
        splash(&mut commands, look, materials, surface, 1.0, WADE_SPRAY_DROPS);
    }
}

/// A smaller splash as a Zombie finishes standing up out of the water.
fn finish_rising(
    mut commands: Commands,
    risen: Query<(Entity, &Rise, Has<Rising>, Option<&Shown>, Has<Puppet>)>,
    mut cues: MessageWriter<PlayCue>,
    mut look: Local<Option<SplashLook>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    for (entity, rise, rising, shown, puppet) in &risen {
        let still = if puppet { shown.is_some_and(|shown| shown.0 == Activity::Rising) } else { rising };
        if still {
            continue;
        }
        commands.entity(entity).remove::<Rise>();
        let Some(surface) = rise.surface else { continue };
        cues.write(PlayCue { cue: Cue::Splash, at: Some(surface), pitch: 1.0, gain: 0.6 });
        let (Some(meshes), Some(materials)) = (meshes.as_mut(), materials.as_mut()) else { continue };
        let look = splash_look(&mut look, meshes, materials);
        splash(&mut commands, look, materials, surface, 0.5, WADE_SPRAY_DROPS / 2);
    }
}

/// Spreads and fades the foam rings, and flies the spray.
fn animate_splashes(
    mut commands: Commands,
    mut rings: Query<(Entity, &mut FoamRing, &mut Transform), Without<Spray>>,
    mut drops: Query<(Entity, &mut Spray, &mut Transform), Without<FoamRing>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (entity, mut ring, mut transform) in &mut rings {
        ring.age += dt;
        let t = (ring.age / WADE_FOAM_RING_SECS).min(1.0);
        transform.scale = Vec3::splat(ring.size * (0.2 + 0.8 * t.sqrt()));
        if let Some(mut material) = materials.as_mut().and_then(|materials| materials.get_mut(&ring.material)) {
            material.base_color = material.base_color.with_alpha(0.7 * (1.0 - t));
        }
        if t >= 1.0 {
            if let Some(materials) = materials.as_mut() {
                materials.remove(&ring.material);
            }
            commands.entity(entity).despawn();
        }
    }
    for (entity, mut drop, mut transform) in &mut drops {
        drop.age += dt;
        let t = drop.age;
        if t >= WADE_SPRAY_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation = drop.from + drop.drift * t + Vec3::Y * (drop.up * t - 0.5 * drop.gravity * t * t);
        transform.scale = Vec3::splat(1.0 - 0.6 * t / WADE_SPRAY_SECS);
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
    tide: Option<Res<Tide>>,
) {
    let Some(layout) = map.map(|map| map.layout()).filter(|layout| !layout.water.is_empty()) else {
        return;
    };
    let tide = tide.map_or_else(TideLevel::default, |tide| tide.level());
    for (entity, transform, locomotion, zombie, wet) in &bodies {
        let lift = zombie.map_or_else(|| half_height_above_feet(locomotion), |zombie| zombie.half_height());
        let now = wading(layout, tide, transform.translation - Vec3::Y * lift);
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
        app.add_message::<PlayCue>()
            .add_plugins(crate::maps::tide::TidePlugin)
            .add_systems(Update, (mark_wet, start_rising, finish_rising, animate_splashes, pose_canals, switch_walls));
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
            if matches!(map, crate::maps::map::MapId::Cove | crate::maps::map::MapId::Polder) {
                continue;
            }
            assert!(map.layout().water.is_empty() && map.layout().canals.is_empty(), "{}", map.name());
            assert_eq!(water_at(map.layout(), TideLevel::Low, Vec3::ZERO), None);
        }
    }

    /// A Wading Point is in use only while the water over it is Shallow at the Tide, which is how
    /// the Polder's Tide will switch its points off at High; the Cove is Still, so always on.
    #[test]
    fn a_wading_point_is_active_only_while_its_water_is_shallow() {
        static CANALS: [Canal; 1] = [Canal { name: "Quay", low: 0.4, mid: 0.9, high: 1.4 }];
        static POOLS: [Pool; 1] = [Pool { level: Level::Canal(CanalId(0)), ..BEACH }];
        let layout = layout_of(&POOLS, &CANALS);
        let on_the_bed = Vec3::new(5.0, 0.0, 5.0);
        let active = |tide| layout.at(tide, on_the_bed) == Some(Wading::Shallow);
        assert!(active(TideLevel::Low) && active(TideLevel::Mid));
        assert!(!active(TideLevel::High), "Deep Water is no place to rise from");
        assert!(!wading_point_active(crate::maps::map::MapId::Greybox.layout(), TideLevel::Low, on_the_bed), "no water at all");
        let cove = crate::maps::map::MapId::Cove.layout();
        for tide in [TideLevel::Low, TideLevel::Mid, TideLevel::High] {
            assert!(cove.wading.iter().all(|&(_, at)| wading_point_active(cove, tide, at)), "the Cove is Still");
        }
    }

    /// The cue follows the water at the feet and not the Spawn Point's kind: a Riser on dry ground,
    /// on the Cove's sand or a Map with no water, still plays the dirt.
    #[test]
    fn the_cue_follows_the_water_at_the_feet_not_the_spawn_point() {
        use crate::maps::map::MapId;
        let cove = MapId::Cove.layout();
        let surf = cove.wading[0].1;
        assert_eq!(rise_cue(cove, TideLevel::Low, surf), Cue::ZombieWade);
        let sand = Vec3::new(surf.x, 0.0, 3.0);
        assert_eq!(rise_cue(cove, TideLevel::Low, sand), Cue::ZombieRiser);
        for map in MapId::ALL.into_iter().filter(|&map| map != MapId::Cove) {
            for &(_, at) in map.layout().risers {
                assert_eq!(rise_cue(map.layout(), TideLevel::Low, at), Cue::ZombieRiser, "{}", map.name());
            }
        }
        // The foam goes on the surface.
        assert_eq!(surface_at(cove, TideLevel::Low, surf), Some(0.0));
        assert_eq!(surface_at(cove, TideLevel::Low, sand), None);
    }

    use crate::enemies::zombie::headless::Sim;
    use crate::maps::map::MapId;

    /// The cues a test app has been asked to play.
    #[derive(Resource, Default)]
    struct Heard(Vec<PlayCue>);

    fn hear(mut asked: MessageReader<PlayCue>, mut heard: ResMut<Heard>) {
        heard.0.extend(asked.read().copied());
    }

    fn cove_sim() -> Sim {
        Sim::on(MapId::Cove, 60.0, |app| {
            app.add_plugins(WaterPlugin).init_resource::<Heard>().add_systems(Update, hear);
        })
    }

    /// How many foam rings and spray drops there are in the world.
    fn splashes(sim: &mut Sim) -> (usize, usize) {
        let world = sim.world_mut();
        (world.query::<&FoamRing>().iter(world).count(), world.query::<&Spray>().iter(world).count())
    }

    /// On the Host, a Zombie rising in the surf plays `ZombieWade` and throws up a ring of foam
    /// and spray, then splashes again as it's up; one rising on dry sand plays `ZombieRiser`, with
    /// no foam.
    #[test]
    fn the_host_chooses_the_wading_look_and_cue_by_the_water_at_the_feet() {
        use crate::tuning::{ZOMBIE_RISE_SECS, ZOMBIE_WADE_RISE_SECS};
        let mut sim = cove_sim();
        let cove = MapId::Cove.layout();
        let surf = cove.wading[2].1;
        sim.spawn(surf, Some(ZOMBIE_WADE_RISE_SECS));
        sim.run(3);
        let heard: Vec<_> = sim.world_mut().resource::<Heard>().0.iter().map(|p| p.cue).collect();
        assert_eq!(heard, vec![Cue::ZombieWade]);
        let (rings, drops) = splashes(&mut sim);
        assert_eq!((rings, drops), (1, WADE_SPRAY_DROPS));
        // As it finishes there is a smaller splash, and the foam of the first dies away.
        sim.run(sim.frames(ZOMBIE_WADE_RISE_SECS) + 5);
        let heard: Vec<_> = sim.world_mut().resource::<Heard>().0.iter().map(|p| p.cue).collect();
        assert_eq!(heard, vec![Cue::ZombieWade, Cue::Splash]);
        sim.run(sim.frames(WADE_FOAM_RING_SECS + 0.2));
        assert_eq!(splashes(&mut sim), (0, 0));

        // A Riser on the sand above the surf: the dirt, and no foam.
        let mut sim = cove_sim();
        sim.spawn(Vec3::new(surf.x, 0.0, 3.0), Some(ZOMBIE_RISE_SECS));
        sim.run(3);
        let heard: Vec<_> = sim.world_mut().resource::<Heard>().0.iter().map(|p| p.cue).collect();
        assert_eq!(heard, vec![Cue::ZombieRiser]);
        assert_eq!(splashes(&mut sim), (0, 0));
        sim.run(sim.frames(ZOMBIE_RISE_SECS) + 5);
        assert_eq!(sim.world_mut().resource::<Heard>().0.len(), 1, "no second splash on dry land");
    }

    /// A Co-op client's puppet is told only that it is `Activity::Rising`, and chooses the look and
    /// the cue from the water at its own feet, as the Host does: no new message.
    #[test]
    fn a_coop_clients_puppet_chooses_the_same_by_the_water_at_its_feet() {
        use crate::enemies::zombie::rig::{Activity, Shown};
        use crate::enemies::zombie::{rising_floor, spawn_puppet};
        use bevy::ecs::system::RunSystemOnce;
        use crate::net::play::Puppet;
        let mut sim = cove_sim();
        let surf = MapId::Cove.layout().wading[0].1;
        // Where the Host's rising Zombie would be as the client first sees it: its buried centre.
        let buried = |floor: Vec3| floor.with_y(floor.y - rising_floor(0.0));
        for (floor, cue, rings) in [(surf, Cue::ZombieWade, 1), (Vec3::new(surf.x, 0.0, 3.0), Cue::ZombieRiser, 0)] {
            sim.world_mut().resource_mut::<Heard>().0.clear();
            let before = splashes(&mut sim).0;
            let centre = buried(floor);
            sim.world_mut()
                .run_system_once(move |mut commands: Commands, assets: crate::enemies::zombie::ZombieAssetsParam| {
                    let puppet = spawn_puppet(&mut commands, &assets, centre, 0.0, false);
                    commands.entity(puppet).insert((Puppet, Shown(Activity::Rising)));
                })
                .unwrap();
            sim.run(3);
            let heard: Vec<_> = sim.world_mut().resource::<Heard>().0.iter().map(|p| p.cue).collect();
            assert_eq!(heard, vec![cue], "a puppet rising at {floor}");
            assert_eq!(splashes(&mut sim).0 - before, rings);
            // Standing up ends it, with the second splash only where there is water.
            let puppets: Vec<Entity> = sim.world_mut().query_filtered::<Entity, With<Puppet>>().iter(sim.app.world()).collect();
            for puppet in puppets {
                sim.world_mut().entity_mut(puppet).insert(Shown(Activity::Moving));
            }
            sim.run(3);
            let heard = sim.world_mut().resource::<Heard>().0.len();
            assert_eq!(heard, 1 + rings);
            let puppets: Vec<Entity> = sim.world_mut().query_filtered::<Entity, With<Puppet>>().iter(sim.app.world()).collect();
            for puppet in puppets {
                sim.world_mut().entity_mut(puppet).despawn();
            }
        }
    }

    /// The Deep Water walls are built once per level, and only the current level's set is on: on
    /// landing the walls switch with `water_at`, on a client as on the Host.
    #[test]
    fn the_wall_sets_switch_when_the_tide_lands() {
        use crate::maps::tide::{Tide, TidePhase, TurnState};
        let mut sim = Sim::on(MapId::Polder, 60.0, |app| {
            app.add_plugins(WaterPlugin);
        });
        let on = |sim: &mut Sim| {
            let world = sim.world_mut();
            let mut walls = world.query::<(&TideWall, Has<ColliderDisabled>)>();
            let mut on = [0usize; 3];
            let mut total = [0usize; 3];
            for (wall, off) in walls.iter(world) {
                total[wall.0.index()] += 1;
                on[wall.0.index()] += usize::from(!off);
            }
            (on, total)
        };
        let (on_now, total) = on(&mut sim);
        assert!(total.iter().all(|&n| n > 0), "a set for each level: {total:?}");
        assert_eq!(on_now, [total[0], 0, 0]);
        for (phase, level) in [(TidePhase::Rising, 1), (TidePhase::High, 2), (TidePhase::Falling, 1), (TidePhase::Low, 0)] {
            *sim.world_mut().resource_mut::<Tide>() = Tide { phase, turn: TurnState::Still };
            sim.run(3);
            let (on_now, total) = on(&mut sim);
            let mut want = [0; 3];
            want[level] = total[level];
            assert_eq!(on_now, want, "at {phase:?}");
        }
    }
}
