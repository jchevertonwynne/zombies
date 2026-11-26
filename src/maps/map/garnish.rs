//! What dresses a Prop without being one: the chairs under the Reading Room's tables, the lamps and encyclopedias on them with the
//! warm pool of light each lamp throws on the table top, and the busts on the plinths. Each is a unit model with no collider of its
//! own, placed inside or on top of its Prop's box, and built by every machine from the same `Layout`: `garnish` is a pure function of
//! the Prop, so a Co-op client lays the same room as the Host (nothing of it is on the wire).
//!
//! A model that isn't on disk (a headless run, or a missing file) is simply not drawn; the Prop's own box is what's solid either way.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::textures::on_disk;
use super::{ENCYCLOPEDIAS, MARBLE_BUST, Prop, PropKind, READING_LAMP, SCHOOL_CHAIR};
use crate::maps::zone::ZoneId;

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
        _ => Vec::new(),
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
            } else if !table {
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
        // Six reading tables with four chairs, two lamps and a set of encyclopedias each, and three busts.
        assert_eq!(markers, 6 * 7 + 3);
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
}
