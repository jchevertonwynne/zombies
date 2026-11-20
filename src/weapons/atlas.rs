//! The weapon atlas: one 1K image holding every finish a code-built gun, glove or sleeve wears
//! (see `tools/texpack/weapon_atlas.py`, which builds it). Every part of a model carries a `Finish`,
//! and its UVs point into that finish's tile.
//!
//! The image is two halves, one above the other. The top half is a Weapon as it comes. The bottom
//! half is the same, but with the shared purple camo in place of gunmetal, blued steel and worn
//! wood, so an Upgraded Weapon wears the same meshes in a material whose UVs are shifted down
//! half the image (`UPGRADED_SHIFT`). Each half is a grid of `COLUMNS` by `ROWS` tiles, a finish's
//! tile being its place in `Finish::ALL`.

use bevy::prelude::*;

/// Tiles across, and down, each half of the atlas.
const COLUMNS: usize = 5;
const ROWS: usize = 2;
/// How far inside its tile a finish's UVs stay, as a share of the atlas, so mip levels and
/// filtering never reach a neighbour: across, and down.
const INSET: Vec2 = Vec2::new(0.012, 0.02);
/// How many metres of a gun a whole tile covers: a face bigger than that shows a smaller share of
/// the tile than its size, so the texture never magnifies past the tile's own resolution.
const TILE_METRES: f32 = 0.25;
/// What an Upgraded Weapon's material shifts every UV by, to land in the atlas's lower half.
pub const UPGRADED_SHIFT: Vec2 = Vec2::new(0.0, 0.5);

/// What a part of a model is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finish {
    Gunmetal,
    BluedSteel,
    Wood,
    OlivePaint,
    Brass,
    Tape,
    /// A leather glove.
    Glove,
    /// An olive wool sleeve.
    Sleeve,
    /// No texture: the part keeps its own colour (red dots, lenses, bottle glass and labels).
    Plain,
}

impl Finish {
    pub const ALL: [Finish; 9] = [
        Finish::Gunmetal,
        Finish::BluedSteel,
        Finish::Wood,
        Finish::OlivePaint,
        Finish::Brass,
        Finish::Tape,
        Finish::Glove,
        Finish::Sleeve,
        Finish::Plain,
    ];

    /// Its place in the grid.
    fn slot(self) -> usize {
        Self::ALL.iter().position(|&f| f == self).expect("ALL lists every finish")
    }

    /// Its tile in the atlas's top half, as UV corners, `(min, max)`.
    pub fn tile(self) -> (Vec2, Vec2) {
        let (column, row) = (self.slot() % COLUMNS, self.slot() / COLUMNS);
        let size = Vec2::new(1.0 / COLUMNS as f32, 0.5 / ROWS as f32);
        let min = Vec2::new(column as f32, row as f32) * size;
        (min, min + size)
    }

    /// The part of its tile its UVs stay in.
    pub fn inner(self) -> (Vec2, Vec2) {
        let (min, max) = self.tile();
        (min + INSET, max - INSET)
    }

    /// The finish whose tile (in either half) holds `uv`.
    pub fn at(uv: Vec2) -> Option<Finish> {
        let v = if uv.y >= 0.5 { uv.y - 0.5 } else { uv.y };
        let column = (uv.x * COLUMNS as f32).floor();
        let row = (v * 2.0 * ROWS as f32).floor();
        if !(0.0..COLUMNS as f32).contains(&column) || !(0.0..ROWS as f32).contains(&row) {
            return None;
        }
        Self::ALL.get(row as usize * COLUMNS + column as usize).copied()
    }

    /// Whether an Upgraded Weapon swaps it for the camo.
    pub fn is_camo_when_upgraded(self) -> bool {
        matches!(self, Finish::Gunmetal | Finish::BluedSteel | Finish::Wood)
    }

    /// The atlas UV of the corner `unit` (each of x and y from 0 to 1) of a face `face` metres
    /// across, in this finish's tile. The face shows as much of the tile as its size, at
    /// `TILE_METRES` a tile, and a larger one shrinks to fit. `jitter` (each 0 to 1) moves the
    /// shown patch about the tile, so two parts don't wear the same patch.
    pub fn place(self, unit: Vec2, face: Vec2, jitter: Vec2) -> Vec2 {
        let (min, max) = self.inner();
        let room = max - min;
        if self == Finish::Plain {
            return min + room * 0.5;
        }
        let share = face / TILE_METRES;
        let shrink = (1.0 / share.max_element()).min(1.0);
        let extent = share * shrink * room;
        min + (room - extent) * jitter + unit * extent
    }
}

/// A part's paint: the finish it wears once the atlas has loaded, and the flat colour it keeps in
/// the vertex colours until then (and for good with the atlas missing).
#[derive(Debug, Clone, Copy)]
pub struct Paint {
    pub finish: Finish,
    pub colour: Color,
}

impl Paint {
    pub const fn new(finish: Finish, colour: Color) -> Self {
        Self { finish, colour }
    }
}

impl From<Color> for Paint {
    fn from(colour: Color) -> Self {
        Self::new(Finish::Plain, colour)
    }
}

/// A repeatable value from 0 to 1 for jittering each part's patch, from where it sits.
pub fn jitter(at: Vec3, salt: f32) -> f32 {
    ((at.dot(Vec3::new(127.1, 311.7, 74.7)) + salt * 19.19).sin() * 43758.547).fract().abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_finish_has_its_own_tile_inside_the_top_half() {
        for (i, a) in Finish::ALL.iter().enumerate() {
            let (min, max) = a.tile();
            assert!(min.x >= 0.0 && max.x <= 1.0 && min.y >= 0.0 && max.y <= 0.5, "{a:?} is off the top half");
            for b in &Finish::ALL[i + 1..] {
                let (other_min, other_max) = b.tile();
                let apart = max.x <= other_min.x + 1e-6 || other_max.x <= min.x + 1e-6 || max.y <= other_min.y + 1e-6 || other_max.y <= min.y + 1e-6;
                assert!(apart, "{a:?} and {b:?} overlap");
            }
        }
    }

    #[test]
    fn a_uv_placed_in_a_finish_is_found_in_it_in_either_half() {
        for finish in Finish::ALL {
            for (unit, face, jitter) in [
                (Vec2::ZERO, Vec2::splat(0.01), Vec2::ZERO),
                (Vec2::ONE, Vec2::splat(0.01), Vec2::ONE),
                (Vec2::ONE, Vec2::new(0.9, 0.05), Vec2::splat(0.5)),
                (Vec2::new(1.0, 0.0), Vec2::splat(3.0), Vec2::new(0.2, 0.9)),
            ] {
                let uv = finish.place(unit, face, jitter);
                let (min, max) = finish.inner();
                assert!(uv.cmpge(min - 1e-5).all() && uv.cmple(max + 1e-5).all(), "{finish:?} {uv} leaves its inner tile");
                assert_eq!(Finish::at(uv), Some(finish));
                assert_eq!(Finish::at(uv + UPGRADED_SHIFT), Some(finish), "the lower half lines up");
            }
        }
    }

    #[test]
    fn a_small_face_keeps_the_texel_density_and_a_big_one_fits() {
        let (min, max) = Finish::Wood.inner();
        let room = max - min;
        let small = Finish::Wood.place(Vec2::ONE, Vec2::splat(TILE_METRES / 2.0), Vec2::ZERO) - Finish::Wood.place(Vec2::ZERO, Vec2::splat(TILE_METRES / 2.0), Vec2::ZERO);
        assert!((small - room / 2.0).abs().max_element() < 1e-6);
        let big = Finish::Wood.place(Vec2::ONE, Vec2::splat(TILE_METRES * 4.0), Vec2::ZERO) - Finish::Wood.place(Vec2::ZERO, Vec2::splat(TILE_METRES * 4.0), Vec2::ZERO);
        assert!((big - room).abs().max_element() < 1e-6);
    }

    #[test]
    fn only_metal_and_wood_become_camo() {
        let camo: Vec<_> = Finish::ALL.into_iter().filter(|f| f.is_camo_when_upgraded()).collect();
        assert_eq!(camo, [Finish::Gunmetal, Finish::BluedSteel, Finish::Wood]);
    }

    #[test]
    fn outside_every_tile_is_no_finish() {
        assert_eq!(Finish::at(Vec2::new(-0.1, 0.1)), None);
        // The tenth slot of each half is spare.
        assert_eq!(Finish::at(Vec2::new(0.95, 0.4)), None);
    }

    #[test]
    fn the_jitter_is_steady_and_in_range() {
        let at = Vec3::new(0.1, -0.2, 0.3);
        assert_eq!(jitter(at, 1.0), jitter(at, 1.0));
        assert_ne!(jitter(at, 1.0), jitter(at, 2.0));
        assert!((0.0..=1.0).contains(&jitter(at, 1.0)));
    }
}
