//! How a Door and a Barrier's window frame are dressed: the pure layout of the boards, frame and
//! ribs fitted to a Door's box, in the Door's own frame (x along the wall, y up, z through it,
//! centred on the box). The builder turns each piece into a mesh.

use bevy::prelude::*;

use super::MapId;

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
    /// A brass bar, rail or scroll of a grille.
    Bar,
}

/// How a Door is dressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorLook {
    /// Weathered boards over a trim-paint frame: a bought Door.
    Boarded,
    /// A steel shutter: a Power Door.
    Shutter,
    /// A brass grille: a Range's gate, the Archive's (#537).
    Grille,
}

/// How a Door of `map` looks: a Power Door is a shutter, a gate of the Archive (one of its Ranges') a brass grille, and
/// every other Door boarded.
pub fn look(map: Option<MapId>, gate: bool, power: bool) -> DoorLook {
    if power {
        DoorLook::Shutter
    } else if gate && map == Some(MapId::Archive) {
        DoorLook::Grille
    } else {
        DoorLook::Boarded
    }
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

/// The grille's bars are this thick and this far apart, centre to centre.
pub const GRILLE_BAR: f32 = 0.03;
pub const GRILLE_PITCH: f32 = 0.15;
const GRILLE_STILE: f32 = 0.08;
const GRILLE_RAIL: f32 = 0.06;
const GRILLE_RAIL_DEEP: f32 = 0.05;
/// How tall the crest over the top rail is.
const GRILLE_CREST: f32 = 0.5;

/// A Range's gate: a brass grille of 0.03 m bars at 0.15 m pitch between two stiles, with a kick rail, a middle rail, a
/// top rail and a scrolled crest over it (a diamond finial between two C-scrolls). `size` is the Door's box.
pub fn grille(size: Vec3) -> Vec<Piece> {
    use std::f32::consts::{FRAC_PI_4, PI};
    let (len, height) = (size.x, size.y);
    let bottom = -height / 2.0;
    let top_rail = height / 2.0 - GRILLE_CREST;
    let mut pieces = Vec::new();
    // Two stiles up to the crest's foot, and the rails across them.
    let side = (len - GRILLE_STILE) / 2.0;
    let stile_h = top_rail + GRILLE_RAIL / 2.0 - bottom;
    for sign in [-1.0, 1.0] {
        pieces.push(Piece::new(Part::Bar, Vec3::new(GRILLE_STILE, stile_h, GRILLE_RAIL_DEEP), Vec3::new(sign * side, bottom + stile_h / 2.0, 0.0)));
    }
    let inner = len - 2.0 * GRILLE_STILE;
    for y in [bottom + 0.25, bottom + (top_rail - bottom) * 0.45, top_rail] {
        pieces.push(Piece::new(Part::Bar, Vec3::new(inner, GRILLE_RAIL, GRILLE_RAIL_DEEP), Vec3::new(0.0, y, 0.0)));
    }
    // The bars, at the pitch, from the floor to the top rail.
    let count = ((inner - GRILLE_BAR) / GRILLE_PITCH).floor() as usize + 1;
    let bar_h = top_rail - bottom;
    for i in 0..count {
        let x = (i as f32 - (count - 1) as f32 / 2.0) * GRILLE_PITCH;
        pieces.push(Piece::new(Part::Bar, Vec3::new(GRILLE_BAR, bar_h, GRILLE_BAR), Vec3::new(x, bottom + bar_h / 2.0, 0.0)));
    }
    // The crest: a tall bar with a diamond finial on it, and a C-scroll either side.
    let base = top_rail + GRILLE_RAIL / 2.0;
    let post = GRILLE_CREST - GRILLE_RAIL / 2.0;
    pieces.push(Piece::new(Part::Bar, Vec3::new(GRILLE_BAR, post, GRILLE_BAR), Vec3::new(0.0, base + post / 2.0, 0.0)));
    pieces.push(Piece { part: Part::Bar, size: Vec3::new(0.12, 0.12, GRILLE_BAR), at: Vec3::new(0.0, base + 0.2, 0.0), tilt: FRAC_PI_4 });
    let (radius, segments) = (0.13, 8);
    for sign in [-1.0f32, 1.0] {
        let centre = Vec2::new(sign * 0.3, base + radius + 0.02);
        // Three quarters of a circle, open towards the finial.
        let angle = |k: f32| (0.5 + 1.5 * k / segments as f32) * PI * sign + if sign > 0.0 { 0.0 } else { PI };
        for k in 0..segments {
            let (a0, a1) = (angle(k as f32), angle(k as f32 + 1.0));
            let (p0, p1) = (centre + Vec2::new(a0.cos(), a0.sin()) * radius, centre + Vec2::new(a1.cos(), a1.sin()) * radius);
            let (mid, d) = ((p0 + p1) / 2.0, p1 - p0);
            pieces.push(Piece { part: Part::Bar, size: Vec3::new(d.length() + 0.01, GRILLE_BAR, GRILLE_BAR), at: Vec3::new(mid.x, mid.y, 0.0), tilt: d.y.atan2(d.x) });
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

/// How thick a culvert grate's bar is, and how far apart they stand.
pub const GRATE_BAR: f32 = 0.04;
pub const GRATE_PITCH: f32 = 0.2;

/// The Archive's culvert grates' iron bars (#541): uprights at a pitch across the window's opening and a rail near its head and sill, in the
/// wall's own plane so they stand behind the boards. Drawn only, inside the frame; the opening stays as it is.
pub fn grate_bars(width: f32, opening: f32) -> Vec<Piece> {
    let inner = width - 0.3;
    let count = (inner / GRATE_PITCH).floor().max(1.0);
    let mut pieces: Vec<Piece> = (0..=count as i32)
        .map(|k| Piece::new(Part::Bar, Vec3::new(GRATE_BAR, opening - 0.1, GRATE_BAR), Vec3::new(-inner / 2.0 + k as f32 * inner / count, 0.0, 0.0)))
        .collect();
    for side in [-1.0, 1.0] {
        pieces.push(Piece::new(Part::Bar, Vec3::new(inner, GRATE_BAR, GRATE_BAR), Vec3::new(0.0, side * (opening / 2.0 - 0.1), 0.0)));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_culvert_grates_bars_stand_inside_the_window_frame_at_a_pitch() {
        let (width, opening) = (2.4, 2.0);
        let pieces = grate_bars(width, opening);
        assert!(pieces.iter().all(|piece| piece.part == Part::Bar));
        let uprights: Vec<&Piece> = pieces.iter().filter(|piece| piece.size.y > 1.0).collect();
        assert!(uprights.len() >= 10, "{} bars", uprights.len());
        for piece in &pieces {
            let extent = piece.half_extent() + piece.at.abs();
            assert!(extent.x <= width / 2.0 - 0.12 + 1e-4 && extent.y <= opening / 2.0 + 1e-4, "{piece:?} sticks out of the opening");
        }
        // Even spacing, near the pitch.
        let mut xs: Vec<f32> = uprights.iter().map(|piece| piece.at.x).collect();
        xs.sort_by(f32::total_cmp);
        for pair in xs.windows(2) {
            assert!((pair[1] - pair[0] - GRATE_PITCH).abs() < 0.03, "{pair:?}");
        }
    }

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
    fn a_grille_is_brass_bars_at_a_pitch_inside_its_box() {
        let pieces = grille(DOOR);
        assert!(pieces.iter().all(|piece| piece.part == Part::Bar));
        inside_box(&pieces, DOOR, PROUD);
        // The upright bars: 0.03 m thick, 0.15 m apart, floor to top rail.
        let mut bars: Vec<&Piece> = pieces.iter().filter(|p| p.size.x == GRILLE_BAR && p.tilt == 0.0 && p.size.y > 1.5).collect();
        bars.sort_by(|a, b| a.at.x.total_cmp(&b.at.x));
        assert!(bars.len() >= 14, "{} bars", bars.len());
        for pair in bars.windows(2) {
            assert!((pair[1].at.x - pair[0].at.x - GRILLE_PITCH).abs() < 1e-4);
        }
        // Rails and a crest: a diamond finial and the scrolls' segments, turned.
        assert!(pieces.iter().filter(|p| p.tilt != 0.0).count() >= 10);
        assert!(pieces.iter().any(|p| p.size.x > 2.0 && p.size.y < 0.1), "a rail across the bars");
        // Even with the wall thin, nothing is proud of its Door.
        let thin = Vec3::new(2.4, 3.0, 0.1);
        inside_box(&grille(thin), thin, PROUD);
    }

    #[test]
    fn only_the_archives_gates_are_grilles() {
        assert_eq!(look(Some(MapId::Archive), true, false), DoorLook::Grille);
        // Its other Doors are boarded, and its Power Door a shutter.
        assert_eq!(look(Some(MapId::Archive), false, false), DoorLook::Boarded);
        assert_eq!(look(Some(MapId::Archive), true, true), DoorLook::Shutter);
        // And the other Maps' gates (the Polder's bridges, the Sanatorium's) keep their boards.
        for map in MapId::ALL.into_iter().filter(|&map| map != MapId::Archive) {
            assert_eq!(look(Some(map), true, false), DoorLook::Boarded, "{map:?}");
        }
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
