//! How a Door and a Barrier's window frame are dressed: the pure layout of the boards, frame and
//! ribs fitted to a Door's box, in the Door's own frame (x along the wall, y up, z through it,
//! centred on the box). The builder turns each piece into a mesh.

use bevy::prelude::*;

/// How far a piece may stand proud of a Door's faces, so boards and frames read as raised.
pub const PROUD: f32 = 0.04;

/// What a piece is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// A bar of the Zone's trim paint (a boarded Door's frame, a window's jambs), or a shutter's
    /// housing and rails.
    Frame,
    /// A weathered board (a bought Door) or a steel rib (a shutter).
    Cover,
}

/// One box of a Door's look.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub part: Part,
    pub size: Vec3,
    pub at: Vec3,
    /// Turned about z, for boards nailed a little askew.
    pub tilt: f32,
}

impl Piece {
    fn new(part: Part, size: Vec3, at: Vec3) -> Self {
        Self { part, size, at, tilt: 0.0 }
    }

    /// The half-extents of the box round the piece once tilted.
    #[cfg(test)]
    pub fn half_extent(&self) -> Vec3 {
        let (s, c) = self.tilt.sin_cos();
        let (w, h) = (self.size.x / 2.0, self.size.y / 2.0);
        Vec3::new(w * c.abs() + h * s.abs(), w * s.abs() + h * c.abs(), self.size.z / 2.0)
    }
}

const FRAME_BAR: f32 = 0.18;
const BOARD_HEIGHT: f32 = 0.2;
const BOARD_THICKNESS: f32 = 0.03;
pub const BOARDS: usize = 6;

/// A bought Door: a trim-paint frame and horizontal boards across both faces. `size` is the Door's
/// box: length, height and depth.
pub fn boarded(size: Vec3) -> Vec<Piece> {
    let (len, height, depth) = (size.x, size.y, size.z);
    let thick = depth + PROUD * 2.0;
    let side = (len - FRAME_BAR) / 2.0;
    let mut pieces = vec![
        Piece::new(Part::Frame, Vec3::new(FRAME_BAR, height, thick), Vec3::new(-side, 0.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(FRAME_BAR, height, thick), Vec3::new(side, 0.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(len - 2.0 * FRAME_BAR, FRAME_BAR, thick), Vec3::new(0.0, (height - FRAME_BAR) / 2.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(len - 2.0 * FRAME_BAR, FRAME_BAR, thick), Vec3::new(0.0, -(height - FRAME_BAR) / 2.0, 0.0)),
    ];
    // Boards spread over the opening, each a little askew, on both faces.
    let inner = Vec2::new(len - 2.0 * FRAME_BAR, height - 2.0 * FRAME_BAR);
    let board_len = inner.x - 0.1;
    let span = inner.y - BOARD_HEIGHT - 0.1;
    for face in [1.0, -1.0] {
        for i in 0..BOARDS {
            let t = i as f32 / (BOARDS - 1) as f32 - 0.5;
            let tilt = [0.05, -0.04, 0.03, -0.06, 0.04, -0.03][(i + if face < 0.0 { 2 } else { 0 }) % 6];
            let jitter: f32 = [0.0, 0.04, -0.03, 0.05, -0.04, 0.02][i];
            pieces.push(Piece {
                part: Part::Cover,
                size: Vec3::new(board_len - jitter.abs() * 2.0, BOARD_HEIGHT, BOARD_THICKNESS),
                at: Vec3::new(jitter, t * span, face * (depth / 2.0 + BOARD_THICKNESS / 2.0)),
                tilt,
            });
        }
    }
    pieces
}

const RIBS: usize = 9;

/// A metal shutter: a roller housing over the top, guide rails down both sides, a bottom rail and
/// a ladder of ribs across both faces.
pub fn shutter(size: Vec3) -> Vec<Piece> {
    let (len, height, depth) = (size.x, size.y, size.z);
    let thick = depth + PROUD * 2.0;
    let housing = 0.34;
    let rail = 0.14;
    let foot = 0.16;
    let mut pieces = vec![
        Piece::new(Part::Frame, Vec3::new(len, housing, thick), Vec3::new(0.0, (height - housing) / 2.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(rail, height - housing, thick), Vec3::new(-(len - rail) / 2.0, -housing / 2.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(rail, height - housing, thick), Vec3::new((len - rail) / 2.0, -housing / 2.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(len - 2.0 * rail, foot, thick), Vec3::new(0.0, -(height - foot) / 2.0, 0.0)),
    ];
    let top = height / 2.0 - housing - 0.12;
    let bottom = -height / 2.0 + foot + 0.2;
    for face in [1.0, -1.0] {
        for i in 0..RIBS {
            let t = i as f32 / (RIBS - 1) as f32;
            pieces.push(Piece::new(
                Part::Cover,
                Vec3::new(len - 2.0 * rail, 0.07, 0.02),
                Vec3::new(0.0, bottom + (top - bottom) * t, face * (depth / 2.0 + 0.01)),
            ));
        }
    }
    pieces
}

/// How far a window frame's sill and jambs stand out past the wall on each face.
pub const FRAME_PROUD: f32 = 0.1;

/// The pieces of a window frame in a wall `depth` thick: the jambs, a head and a sill, around an
/// opening `width` across and `opening` tall, centred on the opening. They stay when every Plank
/// is gone.
pub fn window_frame(width: f32, opening: f32, depth: f32) -> Vec<Piece> {
    let thick = depth + 0.08;
    let bar = 0.12;
    vec![
        Piece::new(Part::Frame, Vec3::new(bar, opening, thick), Vec3::new(-(width - bar) / 2.0, 0.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(bar, opening, thick), Vec3::new((width - bar) / 2.0, 0.0, 0.0)),
        Piece::new(Part::Frame, Vec3::new(width - 2.0 * bar, bar, thick), Vec3::new(0.0, (opening - bar) / 2.0, 0.0)),
        // The sill sits on the wall below and juts out past it on both sides.
        Piece::new(Part::Frame, Vec3::new(width + 0.2, 0.07, depth + 2.0 * FRAME_PROUD), Vec3::new(0.0, -opening / 2.0 + 0.035, 0.0)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOOR: Vec3 = Vec3::new(2.4, 3.0, 0.25);

    fn inside_box(pieces: &[Piece], size: Vec3, proud: f32) {
        for piece in pieces {
            let extent = piece.half_extent() + piece.at.abs();
            assert!(extent.x <= size.x / 2.0 + 1e-4, "{piece:?} sticks out along the wall");
            assert!(extent.y <= size.y / 2.0 + 1e-4, "{piece:?} sticks out of the top or foot");
            assert!(extent.z <= size.z / 2.0 + proud + 1e-4, "{piece:?} stands too far proud");
        }
    }

    #[test]
    fn a_boarded_door_stays_in_its_box() {
        let pieces = boarded(DOOR);
        inside_box(&pieces, DOOR, PROUD);
        assert_eq!(pieces.iter().filter(|p| p.part == Part::Cover).count(), BOARDS * 2);
        assert_eq!(pieces.iter().filter(|p| p.part == Part::Frame).count(), 4);
    }

    #[test]
    fn a_shutter_stays_in_its_box() {
        inside_box(&shutter(DOOR), DOOR, PROUD);
        let thin = Vec3::new(2.4, 3.0, 0.1);
        inside_box(&boarded(thin), thin, PROUD);
    }

    #[test]
    fn boards_cover_both_faces() {
        let pieces = boarded(DOOR);
        for face in [1.0, -1.0] {
            assert!(pieces.iter().any(|p| p.part == Part::Cover && p.at.z * face > 0.0));
        }
    }

    #[test]
    fn a_window_frame_hugs_its_opening() {
        let (width, opening, depth) = (1.6, 1.3, 0.4);
        for piece in window_frame(width, opening, depth) {
            let extent = piece.half_extent() + piece.at.abs();
            assert!(extent.x <= width / 2.0 + 0.1 + 1e-4);
            assert!(extent.y <= opening / 2.0 + 1e-4);
            assert!(extent.z <= depth / 2.0 + FRAME_PROUD + 1e-4);
        }
    }
}
