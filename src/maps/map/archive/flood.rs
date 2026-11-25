//! The Archive's flooded cellar (#539): what the Flooded Cellar's water looks like beyond its
//! colour (`Look.water`, drawn by `maps::water`). 24 loose pages float on the Pool, 6 drip points
//! (3 over the Pool, 2 in the Undercroft, 1 in the Well Stair) send out ripple rings, and a darker
//! tide mark runs round the Cellar's walls, piers and the Causeway's sides at the water line.
//!
//! All of it is a cosmetic that every machine builds from the Layout and the consts below: the
//! pages' lines and the drips' schedule come from hashes of the Map's own numbers and the local
//! clock, so nothing is on the wire and no machine can see something another can't. A drip lands
//! as a `DripLanded` message for the sound ticket to play `Drip` from. No asset file is needed:
//! the ripples are code-built, the pages are `DecalPiece`s that wait for their file or are left
//! out, and the tide mark is a textured box that stays plain paint until its material loads.

use std::f32::consts::FRAC_PI_2;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::super::build::Builder;
use super::super::decals::{Decal, DecalPiece, PAGE_SIZE, Page, page_transform};
use super::super::textures::{Material, Tint};
use super::super::{DOOR_WIDTH, Layout, PropKind, WALL_THICKNESS, WINDOW_WIDTH, palette};
use super::{PEN, PEN_HALF};
use crate::maps::darkness::lamp_flicker::position_hash;
use crate::maps::water::{Drifter, drifters};
use crate::maps::zone::ZoneId;
use crate::state::InRun;

/// How many pages float on the Pool.
pub const PAGES: usize = 24;
/// How far a page's middle rides over the surface when it's level, so it floats on it.
const PAGE_LIFT: f32 = 0.004;
/// How far the Causeway's keep-out reaches past it, to take in its steps and a wader's room.
const CAUSEWAY_MARGIN: f32 = 1.5;
/// How far a pier or a hatch's pen is kept clear of.
const KEEP_CLEAR: f32 = 0.3;

/// A place the ceiling drips: the spot a ripple spreads from, on the water (or the wet floor).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DripPoint {
    pub zone: ZoneId,
    pub at: Vec3,
}

const fn drip(zone: ZoneId, x: f32, y: f32, z: f32) -> DripPoint {
    DripPoint { zone, at: Vec3::new(x, y, z) }
}

/// The 6 drip points: between the piers over the Pool (at its surface), between the Undercroft's
/// columns and at the foot of the Well Stair (on the floor, in a puddle).
pub const DRIPS: [DripPoint; 6] = [
    drip(ZoneId::FloodedCellar, -8.0, 0.5, 14.0),
    drip(ZoneId::FloodedCellar, 0.0, 0.5, 6.0),
    drip(ZoneId::FloodedCellar, 8.0, 0.5, 14.0),
    drip(ZoneId::Undercroft, -12.5, 0.0, 26.0),
    drip(ZoneId::Undercroft, -7.5, 0.0, 31.0),
    drip(ZoneId::WellStair, -22.5, 0.0, 1.0),
];

/// How long a ripple takes to spread and fade, and how wide it gets.
pub const RIPPLE_SECONDS: f32 = 0.8;
const RIPPLE_RADIUS: f32 = 0.6;
/// How long between drips at one point: from this to this, on a hash.
pub const DRIP_EVERY: (f32, f32) = (1.5, 4.0);
/// How far over the surface a ripple lies.
const RIPPLE_LIFT: f32 = 0.01;
const RIPPLE: Color = Color::srgba(0.75, 0.62, 0.42, 0.5);

fn unit(hash: u32) -> f32 {
    (hash >> 8) as f32 / (1u32 << 24) as f32
}

/// How long drip `n` at `point` falls after drip `n - 1` (after the start, for the first).
pub fn drip_interval(point: usize, n: u32) -> f32 {
    let at = DRIPS[point].at;
    DRIP_EVERY.0 + (DRIP_EVERY.1 - DRIP_EVERY.0) * unit(position_hash(Vec3::new(at.x, at.z, n as f32 + 0.5)))
}

/// When drip `n` falls at `point`, in seconds on the clock: the same on every machine.
pub fn drip_time(point: usize, n: u32) -> f32 {
    (0..=n).map(|k| drip_interval(point, k)).sum()
}

/// A drip has fallen at `at` (a ripple spreads from there): what the sound ticket plays `Drip` for.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DripLanded {
    pub zone: ZoneId,
    pub at: Vec3,
}

/// A drip point's ripple ring: which point, which drip is next and when, and when the last fell.
#[derive(Component, Debug)]
pub struct Ripple {
    point: usize,
    next: u32,
    next_at: f32,
    last: f32,
}

/// A page afloat on the Pool: where it drifts, and the surface it floats on.
#[derive(Component, Debug)]
pub struct FloatingPage {
    drifter: Drifter,
    surface: f32,
}

/// Where no page may float in `zone`: the Causeway and its steps (a margin round it), each pier and each hatch's pen.
pub fn page_keep_out(layout: &Layout, zone: ZoneId) -> Vec<(Vec2, Vec2)> {
    let mut out = Vec::new();
    for raised in layout.raised_floors.iter().filter(|raised| raised.zone == zone) {
        out.push((raised.min - Vec2::splat(CAUSEWAY_MARGIN), raised.max + Vec2::splat(CAUSEWAY_MARGIN)));
    }
    for prop in layout.props.iter().filter(|prop| prop.zone == zone && prop.kind == PropKind::Pillar) {
        let half = Vec2::new(prop.size.x, prop.size.z) / 2.0 + Vec2::splat(KEEP_CLEAR);
        let at = Vec2::new(prop.at.x, prop.at.z);
        out.push((at - half, at + half));
    }
    for &(_, centre, out_of) in layout.windows.iter().filter(|(window_zone, ..)| *window_zone == zone) {
        let (at, along) = (Vec2::new(centre.x, centre.z), Vec2::new(out_of.y.abs(), out_of.x.abs()));
        let (a, b) = (at - along * PEN_HALF, at + out_of * PEN + along * PEN_HALF);
        out.push((a.min(b) - Vec2::splat(KEEP_CLEAR), a.max(b) + Vec2::splat(KEEP_CLEAR)));
    }
    out
}

/// The Pool's pages: `PAGES` of them, each adrift on a line wholly inside the Pool and off the keep-out.
pub fn pages(layout: &Layout) -> Vec<FloatingPage> {
    let Some(pool) = layout.water.first() else { return Vec::new() };
    let keep_out = page_keep_out(layout, pool.zone);
    let reach = PAGE_SIZE.length() / 2.0;
    let surface = pool.surface(layout.canals, Default::default());
    drifters(pool, &keep_out, reach, PAGES).into_iter().map(|drifter| FloatingPage { drifter, surface }).collect()
}

impl FloatingPage {
    /// Its transform at `now` seconds.
    pub fn transform(&self, now: f32) -> Transform {
        let (at, bob) = self.drifter.at(now);
        page_transform(&Page { at, yaw: self.drifter.yaw }, self.surface + PAGE_LIFT + bob)
    }
}

/// Builds the pages and the ripple rings.
pub fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, layout: &Layout) {
    for page in pages(layout) {
        commands.spawn((InRun, DecalPiece(Decal::Page), page.transform(0.0), page));
    }
    let ring = meshes.add(Annulus::new(0.85, 1.0));
    for (point, spot) in DRIPS.iter().enumerate() {
        let material = materials.add(StandardMaterial { base_color: RIPPLE.with_alpha(0.0), unlit: true, alpha_mode: AlphaMode::Add, cull_mode: None, double_sided: true, ..default() });
        commands.spawn((
            InRun,
            Ripple { point, next: 0, next_at: drip_time(point, 0), last: f32::NEG_INFINITY },
            Mesh3d(ring.clone()),
            MeshMaterial3d(material),
            Transform { translation: spot.at + Vec3::Y * RIPPLE_LIFT, rotation: Quat::from_rotation_x(-FRAC_PI_2), scale: Vec3::ZERO },
            Visibility::Hidden,
            NotShadowCaster,
        ));
    }
}

/// The pages drift and bob.
pub fn drift_pages(time: Res<Time>, mut pages: Query<(&FloatingPage, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (page, mut at) in &mut pages {
        *at = page.transform(now);
    }
}

/// Each drip point's ripple spreads and fades after a drip, and a drip that has just fallen is announced.
pub fn ripple_rings(
    time: Res<Time>,
    mut rings: Query<(&mut Ripple, &mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut landed: MessageWriter<DripLanded>,
) {
    let now = time.elapsed_secs();
    for (mut ripple, mut at, mut visibility, material) in &mut rings {
        while ripple.next_at <= now {
            ripple.last = ripple.next_at;
            ripple.next += 1;
            ripple.next_at += drip_interval(ripple.point, ripple.next);
            // Not the drips a long first frame caught up on, only one that has just fallen.
            if now - ripple.last < 0.25 {
                let spot = DRIPS[ripple.point];
                landed.write(DripLanded { zone: spot.zone, at: spot.at });
            }
        }
        let age = (now - ripple.last) / RIPPLE_SECONDS;
        if (0.0..1.0).contains(&age) {
            let radius = RIPPLE_RADIUS * age.sqrt();
            at.scale = Vec3::new(radius, radius, 1.0);
            *visibility = Visibility::Inherited;
            if let Some(mut material) = materials.get_mut(&material.0) {
                material.base_color = RIPPLE.with_alpha(RIPPLE.alpha() * (1.0 - age));
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

// ---------------------------------------------------------------- The tide mark

/// How high the tide mark's band stands (the water line, up 0.15 m) and how thin it is.
const MARK_LOW: f32 = 0.5;
const MARK_HIGH: f32 = 0.65;
const MARK_THICKNESS: f32 = 0.02;
/// How far its middle stands out from the face it lies on, so it sits just proud of it.
const MARK_OUT: f32 = MARK_THICKNESS / 2.0 - 0.002;
/// The tint of the wall material it is drawn in.
const MARK_TINT: f32 = 0.6;
/// How much room the band leaves at a doorway's jambs.
const JAMB: f32 = 0.05;

/// The parts of `lo..hi` left after cutting out each of `gaps`.
fn minus(lo: f32, hi: f32, gaps: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut gaps = gaps.to_vec();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut from = lo;
    for (a, b) in gaps {
        if a > from {
            out.push((from, a.min(hi)));
        }
        from = from.max(b);
    }
    if from < hi {
        out.push((from, hi));
    }
    out.into_iter().filter(|(a, b)| b - a > 0.1).collect()
}

/// Each hatch in the Cellar's east wall: its window's z, and the x its hatch wall stands at.
fn hatches(layout: &Layout) -> Vec<(f32, f32)> {
    layout.windows.iter().filter(|(zone, ..)| *zone == ZoneId::FloodedCellar).map(|&(_, at, _)| (at.z, at.x)).collect()
}

/// The band's boxes (size, middle) round the Flooded Cellar's walls, hatch walls, piers and the Causeway's sides.
pub fn tide_bands(layout: &Layout) -> Vec<(Vec3, Vec3)> {
    let (half, door, window) = (WALL_THICKNESS / 2.0, DOOR_WIDTH / 2.0 + JAMB, WINDOW_WIDTH / 2.0 + JAMB);
    let (mid, tall) = ((MARK_LOW + MARK_HIGH) / 2.0, MARK_HIGH - MARK_LOW);
    let mut out: Vec<(Vec3, Vec3)> = Vec::new();
    let along_x = |a: f32, b: f32, z: f32| (Vec3::new(b - a, tall, MARK_THICKNESS), Vec3::new((a + b) / 2.0, mid, z));
    let along_z = |a: f32, b: f32, x: f32| (Vec3::new(MARK_THICKNESS, tall, b - a), Vec3::new(x, mid, (a + b) / 2.0));
    // The south wall (z = 0) and the north (z = 20, with D10 and the Power Door), seen from inside.
    for (a, b) in minus(-20.0 + half, 20.0 - half, &[]) {
        out.push(along_x(a, b, half + MARK_OUT));
    }
    for (a, b) in minus(-20.0 + half, 20.0 - half, &[(-10.0 - door, -10.0 + door), (10.0 - door, 10.0 + door)]) {
        out.push(along_x(a, b, 20.0 - half - MARK_OUT));
    }
    // The west wall (x = -20, with the Well Stair's arch) and the east (x = 20, with D9), seen from inside; the east wall is
    // hidden where the hatches' pens stand against it.
    for (a, b) in minus(half, 20.0 - half, &[(1.3 - 1.2 - JAMB, 1.3 + 1.2 + JAMB)]) {
        out.push(along_z(a, b, -20.0 + half + MARK_OUT));
    }
    let hatches = hatches(layout);
    let hidden: Vec<(f32, f32)> = hatches.iter().map(|&(z, _)| (z - PEN_HALF - half, z + PEN_HALF + half)).chain([(10.0 - door, 10.0 + door)]).collect();
    for (a, b) in minus(half, 20.0 - half, &hidden) {
        out.push(along_z(a, b, 20.0 - half - MARK_OUT));
    }
    // Each hatch wall's face into the Cellar, round its window, and its pen's two side walls' outer faces.
    for &(z, x) in &hatches {
        for (a, b) in minus(z - PEN_HALF, z + PEN_HALF, &[(z - window, z + window)]) {
            out.push(along_z(a, b, x - half - MARK_OUT));
        }
        let (x0, x1) = (x - half, 20.0 - half);
        for (side, sign) in [(z - PEN_HALF, -1.0), (z + PEN_HALF, 1.0)] {
            out.push((Vec3::new(x1 - x0, tall, MARK_THICKNESS), Vec3::new((x0 + x1) / 2.0, mid, side + sign * (half + MARK_OUT))));
        }
    }
    // The piers, on all four faces.
    for prop in layout.props.iter().filter(|prop| prop.zone == ZoneId::FloodedCellar && prop.kind == PropKind::Pillar) {
        let (w, d) = (prop.size.x / 2.0, prop.size.z / 2.0);
        for sign in [-1.0, 1.0] {
            out.push((Vec3::new(prop.size.x + 2.0 * MARK_THICKNESS, tall, MARK_THICKNESS), Vec3::new(prop.at.x, mid, prop.at.z + sign * (d + MARK_OUT))));
            out.push((Vec3::new(MARK_THICKNESS, tall, prop.size.z), Vec3::new(prop.at.x + sign * (w + MARK_OUT), mid, prop.at.z)));
        }
    }
    // The Causeway's two sides, up to its top.
    for raised in layout.raised_floors.iter().filter(|raised| raised.zone == ZoneId::FloodedCellar) {
        let (low, high) = (MARK_LOW, raised.height.min(MARK_HIGH));
        for (z, sign) in [(raised.min.y, -1.0), (raised.max.y, 1.0)] {
            out.push((Vec3::new(raised.max.x - raised.min.x, high - low, MARK_THICKNESS), Vec3::new((raised.min.x + raised.max.x) / 2.0, (low + high) / 2.0, z + sign * MARK_OUT)));
        }
    }
    out
}

/// Draws the tide mark: a darker band of the Cellar's wall material (at Tint 0.6) at the water line.
pub(in crate::maps::map) fn tide_mark(b: &mut Builder, layout: &Layout) {
    let wall = palette(Some(ZoneId::FloodedCellar)).wall.to_srgba();
    let dark = Color::srgb(wall.red * MARK_TINT, wall.green * MARK_TINT, wall.blue * MARK_TINT);
    for (size, at) in tide_bands(layout) {
        let band = b.shape_in(size, at, Quat::IDENTITY, dark, Some(Material::DarkBrickWall));
        b.tag(band, (Tint(Color::srgb(MARK_TINT, MARK_TINT, MARK_TINT)), NotShadowCaster));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::map::MapId;

    fn archive() -> &'static Layout {
        MapId::Archive.layout()
    }

    #[test]
    fn floating_pages_stay_in_the_pool_and_off_the_causeway() {
        let layout = archive();
        let pool = layout.water[0];
        let pages = pages(layout);
        assert_eq!(pages.len(), PAGES);
        let causeway = layout.raised_floors.iter().find(|raised| raised.zone == pool.zone).expect("a Causeway");
        let reach = PAGE_SIZE.length() / 2.0;
        let keep_out = page_keep_out(layout, pool.zone);
        let surface = pool.surface(layout.canals, Default::default());
        for page in &pages {
            for step in 0..400 {
                let now = step as f32 * 7.3;
                let (at, bob) = page.drifter.at(now);
                assert!(bob.abs() <= crate::maps::water::BOB / 2.0 + 1e-6);
                // The page's corners all lie in the Pool and off the Causeway.
                for corner in [Vec2::splat(reach), Vec2::splat(-reach), Vec2::new(reach, -reach), Vec2::new(-reach, reach)] {
                    let spot = at + corner;
                    assert!(pool.covers(spot), "{at} left the Pool at {now}");
                    assert!(!(spot.cmpge(causeway.min).all() && spot.cmple(causeway.max).all()), "{at} is on the Causeway at {now}");
                }
                for &(min, max) in &keep_out {
                    assert!(!(at.cmpgt(min - Vec2::splat(reach - 1e-3)).all() && at.cmplt(max + Vec2::splat(reach - 1e-3)).all()), "{at} is in a keep-out at {now}");
                }
                let height = page.transform(now).translation.y;
                assert!((height - (surface + PAGE_LIFT)).abs() < 0.0071, "{height}");
            }
        }
    }

    #[test]
    fn pages_drift_slowly() {
        let page = &pages(archive())[0];
        let (a, _) = page.drifter.at(10.0);
        let (b, _) = page.drifter.at(11.0);
        assert!(a.distance(b) <= crate::maps::water::DRIFT_SPEED + 1e-4);
    }

    #[test]
    fn drip_points_are_the_same_on_every_machine() {
        // Six, in the Zones the look names, each inside its Zone's volume and (over the Pool) clear of piers and the Causeway.
        let count = |zone| DRIPS.iter().filter(|drip| drip.zone == zone).count();
        assert_eq!((count(ZoneId::FloodedCellar), count(ZoneId::Undercroft), count(ZoneId::WellStair)), (3, 2, 1));
        let layout = archive();
        let keep_out = page_keep_out(layout, ZoneId::FloodedCellar);
        let surface = layout.water[0].surface(layout.canals, Default::default());
        for (point, spot) in DRIPS.iter().enumerate() {
            let (lo, hi) = spot.zone.volume();
            assert!(spot.at.cmpgt(lo).all() && spot.at.cmplt(hi).all(), "{spot:?} is outside {:?}", spot.zone);
            if spot.zone == ZoneId::FloodedCellar {
                let flat = Vec2::new(spot.at.x, spot.at.z);
                assert!(layout.water[0].covers(flat));
                assert_eq!(spot.at.y, surface);
                assert!(!keep_out.iter().any(|&(min, max)| flat.cmpgt(min).all() && flat.cmplt(max).all()), "{spot:?}");
            }
            // The schedule is a function of the point and the drip, not of any state: asked twice, or by a client, it's the same,
            // and every gap is between 1.5 and 4 s.
            let mut last = 0.0;
            for n in 0..50 {
                let time = drip_time(point, n);
                assert_eq!(time, drip_time(point, n));
                let gap = time - last;
                assert!((DRIP_EVERY.0..=DRIP_EVERY.1).contains(&gap), "{gap}");
                last = time;
            }
        }
        // The pages are built from the Layout alone, and no two points drip in step.
        let lines = |layout| pages(layout).iter().map(|page| page.drifter).collect::<Vec<_>>();
        assert_eq!(lines(layout), lines(layout));
        let firsts: Vec<f32> = (0..DRIPS.len()).map(|point| drip_time(point, 0)).collect();
        for (i, a) in firsts.iter().enumerate() {
            assert!(firsts[i + 1..].iter().all(|b| a != b));
        }
    }

    #[test]
    fn a_drip_is_announced_once_and_its_ripple_fades_in_under_a_second() {
        use bevy::ecs::system::RunSystemOnce;
        use std::time::Duration;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_message::<DripLanded>().init_resource::<Assets<StandardMaterial>>();
        let drip_at = drip_time(0, 0);
        app.world_mut().spawn((
            Ripple { point: 0, next: 0, next_at: drip_at, last: f32::NEG_INFINITY },
            Transform::default(),
            Visibility::Hidden,
            MeshMaterial3d(Handle::<StandardMaterial>::default()),
        ));
        app.world_mut().resource_mut::<Time>().advance_to(Duration::from_secs_f32(drip_at + 0.1));
        app.world_mut().run_system_once(ripple_rings).unwrap();
        let heard: Vec<DripLanded> = app.world_mut().resource_mut::<Messages<DripLanded>>().drain().collect();
        assert_eq!(heard, [DripLanded { zone: DRIPS[0].zone, at: DRIPS[0].at }]);
        let visibility = *app.world_mut().query::<&Visibility>().single(app.world()).unwrap();
        assert_eq!(visibility, Visibility::Inherited);
        // After its 0.8 s it's gone again, and nothing more is announced before the next drip.
        app.world_mut().resource_mut::<Time>().advance_to(Duration::from_secs_f32(drip_at + RIPPLE_SECONDS + 0.1));
        app.world_mut().run_system_once(ripple_rings).unwrap();
        let visibility = *app.world_mut().query::<&Visibility>().single(app.world()).unwrap();
        assert_eq!(visibility, Visibility::Hidden);
        assert!(app.world_mut().resource_mut::<Messages<DripLanded>>().drain().next().is_none());
    }

    #[test]
    fn the_tide_mark_is_a_band_at_the_water_line_round_walls_piers_and_causeway() {
        let layout = archive();
        let bands = tide_bands(layout);
        let surface = layout.water[0].surface(layout.canals, Default::default());
        for (size, at) in &bands {
            let (low, high) = (at.y - size.y / 2.0, at.y + size.y / 2.0);
            assert!((low - surface).abs() < 1e-5, "{at}");
            assert!(high <= surface + 0.15 + 1e-5 && size.y > 0.09, "{at}");
            assert!(size.min_element() >= MARK_THICKNESS - 1e-6);
        }
        // Four faces for each of the 8 piers, and the Causeway's two sides, are among them.
        let piers = layout.props.iter().filter(|prop| prop.zone == ZoneId::FloodedCellar && prop.kind == PropKind::Pillar).count();
        assert_eq!(piers, 8);
        assert!(bands.len() >= piers * 4 + 2 + 4);
        // None of it crosses a doorway: not D9, D10 or the Power Door.
        for (size, at) in &bands {
            let (centre, half) = (Vec2::new(at.x, at.z), Vec2::new(size.x, size.z) / 2.0 + Vec2::splat(0.3));
            for door_at in [Vec2::new(20.0, 10.0), Vec2::new(-10.0, 20.0), Vec2::new(10.0, 20.0)] {
                assert!(!(door_at.cmpgt(centre - half).all() && door_at.cmplt(centre + half).all()), "a band crosses the doorway at {door_at}");
            }
        }
    }
}
