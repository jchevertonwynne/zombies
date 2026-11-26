//! What dresses a Prop without being one: the chairs under the Reading Room's tables, the lamps and encyclopedias on them with the
//! warm pool of light each lamp throws on the table top, and the busts on the plinths. Each is a unit model with no collider of its
//! own, placed inside or on top of its Prop's box, and built by every machine from the same `Layout`: `garnish` is a pure function of
//! the Prop, so a Co-op client lays the same room as the Host (nothing of it is on the wire).
//!
//! A model that isn't on disk (a headless run, or a missing file) is simply not drawn; the Prop's own box is what's solid either way.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::textures::on_disk;
use super::{CANDLES, ENCYCLOPEDIAS, MARBLE_BUST, Prop, PropKind, READING_LAMP, SCHOOL_CHAIR};
use crate::maps::zone::ZoneId;

/// Where each of a candle cluster's three flames burns over its foot (the unit's x, up and z), above its wicks.
const FLAMES: [Vec3; 3] = [Vec3::new(-0.05, 0.184, -0.03), Vec3::new(0.06, 0.234, 0.02), Vec3::new(0.01, 0.144, -0.07)];
/// A flame is this tall and wide, before it flickers.
pub const FLAME_HEIGHT: f32 = 0.06;
pub const FLAME_WIDTH: f32 = 0.03;
/// How far a candle lectern's flanking pair stands each side of its middle, and how far back from its front edge.
const LECTERN_CANDLE: Vec3 = Vec3::new(0.26, 0.0, -0.2);

/// How far a lamp's light pool reaches over its table top, and how bright its middle is.
pub const POOL_RADIUS: f32 = 0.5;
pub const POOL_GLOW: f32 = 0.22;
/// How far a chair is tucked in from the edge of its table's box, to the near side of its seat.
const CHAIR_TUCK: f32 = 0.02;
/// A reading table's length: the issue desk is longer and has no chairs, lamps or books of its own.
const READING_TABLE_MAX: f32 = 5.0;

/// One thing laid on or in a Prop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Garnished {
    /// A unit model (its path under `assets/`) standing at `at`, its foot's middle, turned `yaw` about Y.
    Model { path: &'static str, at: Vec3, yaw: f32 },
    /// A warm pool of light on a table top at `at`.
    Pool { at: Vec3 },
    /// A candle flame burning at `at`, whose size flickers (`candle_flicker`).
    Flame { at: Vec3 },
}

/// What dresses `prop`: nothing for most. A reading table (a Table in the Reading Room shorter than the issue desk) has four chairs
/// tucked in, two a side, two reading lamps with their light pools and a set of encyclopedias; a plinth (a Pillar in the Catalogue Hall
/// or Reading Room) carries a bust, which faces the way the Prop's `turn` says.
pub fn garnish(prop: &Prop) -> Vec<Garnished> {
    let top = prop.at + Vec3::Y * prop.size.y;
    match (prop.zone, prop.kind) {
        (ZoneId::ReadingRoom, PropKind::Table) if prop.size.x < READING_TABLE_MAX => {
            let mut out = Vec::new();
            // Chairs: two to a long side, facing the table (the unit's front is +z), inside the table's box.
            let off = prop.size.z / 2.0 - SCHOOL_CHAIR.1.z / 2.0 - CHAIR_TUCK;
            for side in [-1.0_f32, 1.0] {
                for x in [-1.0_f32, 1.0] {
                    let at = prop.at + Vec3::new(x * prop.size.x * 0.22, 0.0, side * off);
                    out.push(Garnished::Model { path: SCHOOL_CHAIR.0, at, yaw: if side < 0.0 { 0.0 } else { std::f32::consts::PI } });
                }
            }
            // A lamp at each end of the table top, with its pool, and a set of encyclopedias between them.
            for x in [-1.0_f32, 1.0] {
                let at = top + Vec3::new(x * prop.size.x * 0.34, 0.0, 0.0);
                out.push(Garnished::Model { path: READING_LAMP.0, at, yaw: 0.0 });
                out.push(Garnished::Pool { at });
            }
            out.push(Garnished::Model { path: ENCYCLOPEDIAS.0, at: top + Vec3::new(0.0, 0.0, 0.12), yaw: 0.0 });
            out
        }
        (ZoneId::CatalogueHall | ZoneId::ReadingRoom, PropKind::Pillar) => {
            vec![Garnished::Model { path: MARBLE_BUST.0, at: top, yaw: f32::from(prop.turn) * FRAC_PI_2 }]
        }
        // The Theatre's lectern (the projector is wider): a candle cluster at each end of its back ledge, a hair under its top.
        (ZoneId::LectureTheatre, PropKind::Table) if prop.size.x < 0.8 => {
            let yaw = f32::from(prop.turn) * FRAC_PI_2;
            let turn = Quat::from_rotation_y(yaw);
            [-1.0_f32, 1.0].into_iter().flat_map(|side| candles(top - Vec3::Y * 0.04 + turn * (LECTERN_CANDLE * Vec3::new(side, 0.0, 1.0)), yaw)).collect()
        }
        // The Undercroft's deed-box racks, the Strong Room's deed-box stacks (by Pack-a-Punch's plinth) and its display case: one cluster on
        // top of each.
        (ZoneId::Undercroft | ZoneId::StrongRoom, PropKind::Crate) | (ZoneId::StrongRoom, PropKind::Case) => candles(top, f32::from(prop.turn) * FRAC_PI_2),
        _ => Vec::new(),
    }
}

/// A candle cluster standing at `at` turned `yaw`: its model and its three flames.
fn candles(at: Vec3, yaw: f32) -> Vec<Garnished> {
    let turn = Quat::from_rotation_y(yaw);
    let mut out = vec![Garnished::Model { path: CANDLES.0, at, yaw }];
    out.extend(FLAMES.map(|flame| Garnished::Flame { at: at + turn * flame }));
    out
}

/// How much a flame at `at` is stretched `t` seconds into a Run, 0.7 to 1.3: a pure function of where it burns and the time, so every machine
/// draws the same flame and none uses a random number. A hash of the flame's place (in millimetres) and of each tenth of a second gives a value in
/// 0 to 1, which is smoothed between the tenths.
pub fn candle_flicker(at: Vec3, t: f32) -> f32 {
    let place = [at.x, at.y, at.z].map(|c| (c * 1000.0).round() as i64 as u64);
    let hash = |step: i64| -> f32 {
        let mut h = 0x9E37_79B9_7F4A_7C15_u64 ^ (step as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        for c in place {
            h = (h ^ c).wrapping_mul(0x94D0_49BB_1331_11EB);
            h ^= h >> 29;
        }
        h ^= h >> 32;
        (h & 0xFFFF) as f32 / 65535.0
    };
    let ticks = t.max(0.0) * 10.0;
    let step = ticks.floor();
    let frac = ticks - step;
    let (a, b) = (hash(step as i64), hash(step as i64 + 1));
    let eased = frac * frac * (3.0 - 2.0 * frac);
    0.7 + 0.6 * (a + (b - a) * eased)
}

/// A candle flame: a little additive quad pair whose height follows `candle_flicker`.
#[derive(Component, Debug, Clone, Copy)]
pub struct Flame {
    pub at: Vec3,
}

/// Stretches each flame to its flicker, off the Run's clock.
pub(super) fn flicker_flames(time: Res<Time>, mut flames: Query<(&Flame, &mut Transform)>) {
    for (flame, mut transform) in &mut flames {
        let stretch = candle_flicker(flame.at, time.elapsed_secs());
        transform.scale = Vec3::new(FLAME_WIDTH * (2.0 - stretch), FLAME_HEIGHT * stretch, FLAME_WIDTH * (2.0 - stretch));
    }
}

/// A unit model to stand in the world once its glTF has loaded: a chair, a lamp, a bust.
#[derive(Component, Debug, Clone, Copy)]
pub struct Garnish {
    pub path: &'static str,
}

/// Starts each new `Garnish`'s glTF if it's on disk, as a scene in the world; a model that isn't there is skipped.
pub(super) fn put_on_garnish(mut commands: Commands, asset_server: Res<AssetServer>, new: Query<(Entity, &Garnish), Added<Garnish>>) {
    for (entity, garnish) in &new {
        if on_disk(garnish.path) {
            let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(garnish.path));
            commands.entity(entity).insert(WorldAssetRoot(scene));
        } else {
            debug!("{}: missing, left undrawn", garnish.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    /// Whether `prop` carries candles: the Theatre's lectern, the Undercroft's and Strong Room's deed boxes and the Strong Room's case.
    fn candlelit(prop: &Prop) -> bool {
        match (prop.zone, prop.kind) {
            (ZoneId::LectureTheatre, PropKind::Table) => prop.size.x < 0.8,
            (ZoneId::Undercroft | ZoneId::StrongRoom, PropKind::Crate) | (ZoneId::StrongRoom, PropKind::Case) => true,
            _ => false,
        }
    }

    #[test]
    fn a_reading_table_gets_four_chairs_two_lamps_with_pools_and_encyclopedias_inside_or_on_its_box() {
        let layout = MapId::Archive.layout();
        let tables: Vec<&Prop> = layout.props.iter().filter(|prop| prop.zone == ZoneId::ReadingRoom && prop.kind == PropKind::Table && prop.size.x < READING_TABLE_MAX).collect();
        assert_eq!(tables.len(), 6, "six reading tables");
        for table in tables {
            let things = garnish(table);
            let count = |wanted: &str| things.iter().filter(|g| matches!(g, Garnished::Model { path, .. } if *path == wanted)).count();
            assert_eq!((count(SCHOOL_CHAIR.0), count(READING_LAMP.0), count(ENCYCLOPEDIAS.0)), (4, 2, 1));
            assert_eq!(things.iter().filter(|g| matches!(g, Garnished::Pool { .. })).count(), 2);
            let (lo, hi) = (table.at - Vec3::new(table.size.x, 0.0, table.size.z) / 2.0, table.at + table.size);
            for thing in &things {
                match *thing {
                    Garnished::Model { path, at, .. } if path == SCHOOL_CHAIR.0 => {
                        // A chair's seat is wholly inside the table's footprint (its back may rise over it).
                        let half = Vec3::new(SCHOOL_CHAIR.1.x, 0.0, SCHOOL_CHAIR.1.z) / 2.0;
                        assert!((at - half).cmpge(lo).all() && (at + half).cmple(Vec3::new(hi.x, at.y, hi.z)).all(), "a chair at {at} is outside the table {lo}..{hi}");
                        assert_eq!(at.y, table.at.y);
                    }
                    Garnished::Model { at, .. } | Garnished::Pool { at } => {
                        assert!((at.y - hi.y).abs() < 1e-5, "{thing:?} is not on the table top");
                        assert!(at.x > lo.x && at.x < hi.x && at.z > lo.z && at.z < hi.z);
                    }
                    Garnished::Flame { .. } => panic!("a reading table burns no candle"),
                }
            }
        }
    }

    #[test]
    fn a_plinth_carries_a_bust_and_nothing_else_is_garnished() {
        let layout = MapId::Archive.layout();
        let mut plinths = 0;
        for prop in layout.props {
            let things = garnish(prop);
            let plinth = matches!(prop.zone, ZoneId::CatalogueHall | ZoneId::ReadingRoom) && prop.kind == PropKind::Pillar;
            let table = prop.zone == ZoneId::ReadingRoom && prop.kind == PropKind::Table && prop.size.x < READING_TABLE_MAX;
            if plinth {
                plinths += 1;
                assert_eq!(things, vec![Garnished::Model { path: MARBLE_BUST.0, at: prop.at + Vec3::Y * prop.size.y, yaw: f32::from(prop.turn) * FRAC_PI_2 }]);
            } else if !table && !candlelit(prop) {
                assert!(things.is_empty(), "the {:?} at {} in the {} is garnished", prop.kind, prop.at, prop.zone.name());
            }
        }
        assert_eq!(plinths, 3, "three busts: one in the Catalogue Hall, two in the Reading Room");
    }

    /// A headless run needs no asset files: it builds every chair, lamp, encyclopedia set and bust as a bare marker, loads nothing, and the pools are
    /// plain meshes.
    #[test]
    fn a_headless_run_builds_the_garnish_without_asset_files() {
        use bevy::ecs::system::RunSystemOnce;
        let mut sim = crate::enemies::zombie::headless::Sim::on(MapId::Archive, 60.0, |_| {});
        let (markers, loaded) = sim
            .world_mut()
            .run_system_once(|markers: Query<&Garnish>, loaded: Query<&WorldAssetRoot>| (markers.iter().count(), loaded.iter().count()))
            .unwrap();
        // Six reading tables with four chairs, two lamps and a set of encyclopedias each, three busts, and the eight candle clusters.
        assert_eq!(markers, 6 * 7 + 3 + 8);
        assert_eq!(loaded, 0, "nothing asks for a glTF without the graphical plugins");
    }

    /// Every machine builds the same room from `Layout`: garnishing is a pure function of the Prop, so two calls, or two peers, agree.
    #[test]
    fn garnish_is_a_pure_function_of_the_prop_so_every_co_op_peer_builds_the_same_room() {
        for prop in MapId::Archive.layout().props {
            assert_eq!(garnish(prop), garnish(prop));
        }
        assert_eq!(MapId::Archive.layout().props, MapId::Archive.layout().props);
    }

    /// Eight candle clusters in all: two on the Theatre's lectern, one on each of the Undercroft's two racks, three on the Strong Room's deed-box
    /// stacks by Pack-a-Punch's plinth and one on its display case; each with its three flames, standing on the Prop it is on.
    #[test]
    fn eight_candle_clusters_burn_on_the_lectern_the_racks_the_stacks_and_the_case() {
        let layout = MapId::Archive.layout();
        let (mut clusters, mut by_the_plinth) = (0, 0);
        for prop in layout.props.iter().filter(|prop| candlelit(prop)) {
            let things = garnish(prop);
            let models = things.iter().filter(|g| matches!(g, Garnished::Model { path, .. } if *path == CANDLES.0)).count();
            let flames = things.iter().filter(|g| matches!(g, Garnished::Flame { .. })).count();
            assert_eq!(flames, 3 * models, "the {:?} at {}", prop.kind, prop.at);
            assert_eq!(things.len(), models + flames);
            clusters += models;
            let (lo, hi) = (prop.at - Vec3::new(prop.size.x, 0.0, prop.size.z) / 2.0, prop.at + Vec3::new(prop.size.x / 2.0, prop.size.y, prop.size.z / 2.0));
            for thing in &things {
                let (Garnished::Model { at, .. } | Garnished::Flame { at }) = *thing else { panic!("{thing:?}") };
                assert!(at.x >= lo.x - 1e-4 && at.x <= hi.x + 1e-4 && at.z >= lo.z - 1e-4 && at.z <= hi.z + 1e-4, "{thing:?} is off the {:?} at {}", prop.kind, prop.at);
                assert!(at.y >= prop.at.y + prop.size.y - 0.05, "{thing:?} is not on top");
            }
            if prop.zone == ZoneId::StrongRoom && prop.kind == PropKind::Crate {
                by_the_plinth += 1;
                let plinth = crate::weapons::pack_a_punch::machine_at(MapId::Archive);
                assert!(Vec2::new(prop.at.x - plinth.x, prop.at.z - plinth.z).length() < 8.0, "the stack at {} is far from Pack-a-Punch", prop.at);
            }
        }
        assert_eq!((clusters, by_the_plinth), (8, 3));
    }

    /// A flame's flicker is a pure function of where it burns and the time: the same on every machine (nothing random, nothing local), always 0.7 to
    /// 1.3, smooth from one tenth of a second to the next, and different flames burn differently.
    #[test]
    fn candle_flicker_is_the_same_on_every_machine() {
        let at = Vec3::new(12.34, 0.9, 30.5);
        for t in [0.0, 0.05, 0.1, 0.77, std::f32::consts::PI, 61.0, 3600.5] {
            assert_eq!(candle_flicker(at, t), candle_flicker(at, t));
            assert!((0.7..=1.3).contains(&candle_flicker(at, t)), "{}", candle_flicker(at, t));
        }
        // Smooth: a hundredth of a second moves it by little.
        for k in 0..200 {
            let t = k as f32 * 0.037;
            assert!((candle_flicker(at, t) - candle_flicker(at, t + 0.01)).abs() < 0.25, "t = {t}");
        }
        // Two flames burn differently, and one is not constant over time.
        let other = Vec3::new(12.4, 0.9, 30.5);
        assert!((0..50).any(|k| (candle_flicker(at, k as f32 * 0.1) - candle_flicker(other, k as f32 * 0.1)).abs() > 0.05));
        assert!((0..50).any(|k| (candle_flicker(at, k as f32 * 0.1) - candle_flicker(at, 0.0)).abs() > 0.05));
        // Every flame the Archive lights flickers within bounds, from the same Layout on every peer.
        for prop in MapId::Archive.layout().props {
            for thing in garnish(prop) {
                if let Garnished::Flame { at } = thing {
                    assert!((0..100).all(|k| (0.7..=1.3).contains(&candle_flicker(at, k as f32 * 0.137))));
                }
            }
        }
    }
}
