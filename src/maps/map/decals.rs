//! Decals: thin alpha-blended quads laid a few millimetres off a wall or floor. Leak streaks hang
//! from under the cornice in the house's rooms, and oil stains lie on the Workshop floor. Where each
//! goes is decided from its spot alone, so a Map dresses the same way every Run.
//!
//! The builder spawns each as a bare `DecalPiece`. It's only drawn once its file has loaded (see
//! `textures`); a missing file leaves no decal at all, and headless runs never load one.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::image::{ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use super::textures::{Dress, dress, load_of, on_disk};

/// One decal image, under `assets/decals/<dir>/color.ktx2` (made by `tools/texpack`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decal {
    Leak003,
    Leak004,
    Leak005,
    OilStain1,
    OilStain2,
    /// A black-on-yellow biohazard sign (ambientCG `Sign004`), 0.25 m across, for the Quarantine Doors.
    Biohazard,
    /// A loose sheet of paper (ambientCG `Paper006`, the same file as its Material Set), 0.21 by 0.3 m, for the Archive's floors.
    Page,
    /// A Tree's crown: a leaf-cluster alpha card, cut out rather than blended, seen from both sides.
    Foliage,
}

const LEAKS: [Decal; 3] = [Decal::Leak003, Decal::Leak004, Decal::Leak005];
const OIL_STAINS: [Decal; 2] = [Decal::OilStain1, Decal::OilStain2];

impl Decal {
    fn file(self) -> String {
        let dir = match self {
            Decal::Leak003 => "leaking003",
            Decal::Leak004 => "leaking004",
            Decal::Leak005 => "leaking005",
            Decal::OilStain1 => "oil_stain_1",
            Decal::OilStain2 => "oil_stain_2",
            Decal::Biohazard => "sign004",
            Decal::Foliage => "foliage",
            Decal::Page => return super::textures::Material::Paper006.files()[0].clone(),
        };
        format!("decals/{dir}/color.ktx2")
    }

    /// How rough it leaves the surface, and how much it reflects head on. Oil soaked into
    /// concrete is dull, and must stay dark at a glancing look, so it hardly reflects at all.
    fn finish(self) -> (f32, f32) {
        if OIL_STAINS.contains(&self) {
            (0.8, 0.1)
        } else if self == Decal::Foliage {
            (0.95, 0.2)
        } else if self == Decal::Page {
            (0.9, 0.2)
        } else {
            (0.7, 0.5)
        }
    }
}

/// Which decals a Zone wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecalFamily {
    /// Damp streaking down the walls from under the cornice.
    Leaks,
    /// Oil pooled on the floor.
    OilStains,
    /// Loose pages lying on the floor (the Archive's).
    Pages,
}

/// A decal to draw once its file has loaded: a unit quad facing along its `Transform`'s +Z,
/// scaled to its size.
#[derive(Component, Debug, Clone, Copy)]
pub struct DecalPiece(pub Decal);

/// Pieces drawn only until this decal is: a Tree's boxy crown, which its leaf cards replace once
/// their file has loaded.
#[derive(Component, Debug, Clone)]
pub struct Instead(pub Vec<Entity>);

/// A `DecalPiece` that's been settled: drawn, or left out for good.
#[derive(Component)]
pub(super) struct Placed;

/// How far off its surface a decal floats, so it never fights the surface for depth.
pub const DECAL_OFFSET: f32 = 0.005;

/// A value from 0 to 1 for a spot (in centimetres) and a salt, the same every Run.
fn chance(a: f32, b: f32, salt: u32) -> f32 {
    let (a, b) = ((a * 100.0).round() as i32 as u32, (b * 100.0).round() as i32 as u32);
    let mut h = a.wrapping_mul(0x8da6_b343) ^ b.wrapping_mul(0xd816_3841) ^ salt.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// How much wider than long a leak streak is drawn: its image is twice as wide, so it's stretched
/// down the wall into drips.
const LEAK_ASPECT: f32 = 1.25;

/// A leak streak on a wall: its middle along the wall, its top, its width and height, and which.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leak {
    pub along: f32,
    pub top: f32,
    pub size: Vec2,
    pub decal: Decal,
}

/// The leaks on a stretch of wall face from `from` to `to` along it, hanging from `top` and never
/// past `bottom`. `line` tells one wall from another. The streaks run 1.4 to 2.4 m down (less on a
/// short strip of wall), a quarter again as wide as long, on about half the wall.
pub fn leaks(from: f32, to: f32, bottom: f32, top: f32, line: f32) -> Vec<Leak> {
    let room = top - bottom - 0.3;
    if room < 0.4 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cursor = from + 0.3;
    while cursor < to - 0.3 {
        let roll = |salt| chance(cursor, line, salt);
        let height = (1.4 + 1.0 * roll(1)).min(room);
        let width = height * LEAK_ASPECT;
        if cursor + width > to - 0.2 {
            break;
        }
        if roll(2) < 0.55 {
            let decal = LEAKS[(roll(3) * LEAKS.len() as f32) as usize % LEAKS.len()];
            out.push(Leak { along: cursor + width / 2.0, top, size: Vec2::new(width, height), decal });
        }
        cursor += width + 0.4 + 1.6 * roll(4);
    }
    out
}

/// An oil stain on a floor: its middle (x, z), its size, how it's turned, and which.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stain {
    pub at: Vec2,
    pub size: f32,
    pub yaw: f32,
    pub decal: Decal,
}

/// The oil stains on a floor from `min` to `max` (x, z): about one in three of its 3 m squares
/// has one, 0.8 to 2 m across, and none reaches past the floor's edge.
pub fn stains(min: Vec2, max: Vec2) -> Vec<Stain> {
    const CELL: f32 = 3.0;
    let mut out = Vec::new();
    let mut z = min.y;
    while z + CELL <= max.y + 0.01 {
        let mut x = min.x;
        while x + CELL <= max.x + 0.01 {
            let roll = |salt| chance(x, z, salt);
            if roll(1) < 0.35 {
                let size = 0.8 + 1.2 * roll(2);
                let slack = (CELL - size) / 2.0;
                let at = Vec2::new(x + CELL / 2.0 + (roll(3) - 0.5) * slack, z + CELL / 2.0 + (roll(4) - 0.5) * slack);
                let decal = OIL_STAINS[(roll(5) * OIL_STAINS.len() as f32) as usize % OIL_STAINS.len()];
                out.push(Stain { at, size, yaw: roll(6) * TAU, decal });
            }
            x += CELL;
        }
        z += CELL;
    }
    out
}

/// A leak's transform on the wall face whose surface is at `face` (a point on it at the leak's
/// middle and top) looking along `normal`.
pub fn leak_transform(leak: &Leak, face: Vec3, normal: Vec3) -> Transform {
    Transform {
        translation: face + normal * DECAL_OFFSET - Vec3::Y * leak.size.y / 2.0,
        rotation: Quat::from_rotation_arc(Vec3::Z, normal),
        scale: leak.size.extend(1.0),
    }
}

/// A stain's transform on a floor at `height`.
pub fn stain_transform(stain: &Stain, height: f32) -> Transform {
    Transform {
        translation: Vec3::new(stain.at.x, height + DECAL_OFFSET, stain.at.y),
        rotation: Quat::from_rotation_y(stain.yaw) * Quat::from_rotation_x(-FRAC_PI_2),
        scale: Vec3::new(stain.size, stain.size, 1.0),
    }
}

/// How far off the floor a loose page lies: 2 mm, as a sheet would.
pub const PAGE_OFFSET: f32 = 0.002;
/// A loose page's size (A-series paper, 0.21 by 0.3 m).
pub const PAGE_SIZE: Vec2 = Vec2::new(0.21, 0.3);

/// A loose page on a floor: its middle (x, z), and how it's turned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Page {
    pub at: Vec2,
    pub yaw: f32,
}

/// How much floor each page of a Zone stands for, and the fewest and most a Zone gets.
const PAGE_AREA: f32 = 25.0;
const PAGES: (usize, usize) = (12, 30);

/// The loose pages on a Zone's floor, which is the rectangles `rects` (x, z), scattered the way the stains are, from the spot alone:
/// one per 25 m^2 of floor, never fewer than 12 or more than 30, turned at random, each wholly on the floor (past its edges by a
/// hand's breadth) and clear of every rectangle in `keep_out`. A page that finds no place in a few tries is left out.
pub fn pages(rects: &[(Vec2, Vec2)], keep_out: &[(Vec2, Vec2)]) -> Vec<Page> {
    let areas: Vec<f32> = rects.iter().map(|(min, max)| (max.x - min.x).max(0.0) * (max.y - min.y).max(0.0)).collect();
    let total: f32 = areas.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let count = ((total / PAGE_AREA).round() as usize).clamp(PAGES.0, PAGES.1);
    let seed = rects[0].0;
    // Half a page's diagonal: any turn of it lies within this of its middle.
    let reach = PAGE_SIZE.length() / 2.0;
    let mut out = Vec::new();
    for i in 0..count {
        for attempt in 0..16u32 {
            let roll = |salt| chance(seed.x + i as f32 * 0.07, seed.y + attempt as f32 * 0.03, salt);
            // Pick a rectangle by area, then a spot in it.
            let mut pick = roll(1) * total;
            let mut index = areas.len() - 1;
            for (k, &area) in areas.iter().enumerate() {
                if pick < area {
                    index = k;
                    break;
                }
                pick -= area;
            }
            let (min, max) = rects[index];
            let (lo, hi) = (min + Vec2::splat(reach + 0.15), max - Vec2::splat(reach + 0.15));
            if lo.x >= hi.x || lo.y >= hi.y {
                continue;
            }
            let at = Vec2::new(lo.x + (hi.x - lo.x) * roll(2), lo.y + (hi.y - lo.y) * roll(3));
            let clear = keep_out.iter().all(|&(a, b)| at.x < a.x - reach || at.x > b.x + reach || at.y < a.y - reach || at.y > b.y + reach);
            if clear {
                out.push(Page { at, yaw: roll(4) * TAU });
                break;
            }
        }
    }
    out
}

/// A page's transform on a floor at `height`.
pub fn page_transform(page: &Page, height: f32) -> Transform {
    Transform {
        translation: Vec3::new(page.at.x, height + PAGE_OFFSET, page.at.y),
        rotation: Quat::from_rotation_y(page.yaw) * Quat::from_rotation_x(-FRAC_PI_2),
        scale: PAGE_SIZE.extend(1.0),
    }
}

/// Where no page may lie on `map`'s floor at `height`: each of the Archive's Ranges' sweeps and mouth strips on that level, which a
/// Bookcase would roll over.
pub fn page_keep_out(map: super::MapId, height: f32) -> Vec<(Vec2, Vec2)> {
    if map != super::MapId::Archive {
        return Vec::new();
    }
    super::archive::RANGES
        .iter()
        .filter(|range| (range.floor - height).abs() < 1.0)
        .flat_map(|range| [range.sweep(), range.mouth_strips()[0], range.mouth_strips()[1]])
        .collect()
}

/// Each decal asked for so far: its material once its file is loading, or `None` if it's missing.
#[derive(Resource, Default)]
pub(super) struct DecalLibrary {
    entries: HashMap<Decal, Option<(Handle<StandardMaterial>, Handle<Image>)>>,
    quad: Option<Handle<Mesh>>,
}

pub(super) fn request_decals(
    mut library: ResMut<DecalLibrary>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    pieces: Query<&DecalPiece, Added<DecalPiece>>,
) {
    for &DecalPiece(decal) in &pieces {
        library.entries.entry(decal).or_insert_with(|| {
            let file = decal.file();
            if !on_disk(&file) {
                debug!("{decal:?}: file missing, left out");
                return None;
            }
            let image: Handle<Image> = asset_server
                .load_builder()
                .with_settings(|settings: &mut ImageLoaderSettings| {
                    settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { anisotropy_clamp: 8, ..ImageSamplerDescriptor::linear() });
                })
                .load(file);
            let (perceptual_roughness, reflectance) = decal.finish();
            let cut_out = decal == Decal::Foliage;
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(image.clone()),
                alpha_mode: if cut_out {
                    AlphaMode::Mask(0.5)
                } else if decal == Decal::Page {
                    AlphaMode::Opaque
                } else {
                    AlphaMode::Blend
                },
                double_sided: cut_out,
                cull_mode: if cut_out { None } else { Some(bevy::render::render_resource::Face::Back) },
                perceptual_roughness,
                reflectance,
                depth_bias: 1.0,
                ..default()
            });
            Some((material, image))
        });
    }
}

/// Draws each waiting decal once its file has loaded, or leaves it out for good.
pub(super) fn place_decals(
    mut commands: Commands,
    mut library: ResMut<DecalLibrary>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    pieces: Query<(Entity, &DecalPiece, Option<&Instead>), Without<Placed>>,
) {
    for (entity, &DecalPiece(decal), instead) in &pieces {
        let Some(entry) = library.entries.get(&decal).cloned() else { continue };
        let choice = match &entry {
            None => dress(false, &[]),
            Some((_, image)) => dress(true, &[load_of(&asset_server, image)]),
        };
        match (choice, entry) {
            (Dress::Wait, _) => continue,
            (Dress::Textured, Some((material, _))) => {
                let quad = library.quad.get_or_insert_with(|| meshes.add(Rectangle::new(1.0, 1.0))).clone();
                commands.entity(entity).insert((Mesh3d(quad), MeshMaterial3d(material), NotShadowCaster));
                for &stand_in in instead.iter().flat_map(|instead| &instead.0) {
                    commands.entity(stand_in).try_despawn();
                }
            }
            _ => {}
        }
        commands.entity(entity).insert(Placed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaks_hang_inside_their_stretch_of_wall() {
        // A 28 m wall, 4 m tall, hung from under its cornice.
        let all = leaks(-14.0, 14.0, 0.0, 3.82, 14.0);
        assert!(all.len() >= 2, "a long wall gets a few: {all:?}");
        for leak in &all {
            assert!(leak.along - leak.size.x / 2.0 >= -14.0 && leak.along + leak.size.x / 2.0 <= 14.0, "{leak:?}");
            assert!(leak.top - leak.size.y >= 0.0);
            assert!((leak.size.x - LEAK_ASPECT * leak.size.y).abs() < 1e-5);
        }
        for pair in all.windows(2) {
            assert!(pair[0].along + pair[0].size.x / 2.0 < pair[1].along - pair[1].size.x / 2.0, "no two overlap");
        }
        // The same wall dresses the same way every time.
        assert_eq!(all, leaks(-14.0, 14.0, 0.0, 3.82, 14.0));
    }

    #[test]
    fn a_strip_of_wall_above_a_door_gets_only_short_leaks_or_none() {
        // The 1 m of wall over a 3 m Door: too short for a streak.
        assert!(leaks(-1.2, 1.2, 3.0, 3.82, 14.0).is_empty());
        for leak in leaks(-10.0, 10.0, 2.8, 3.82, 5.0) {
            assert!(leak.top - leak.size.y >= 2.8 - 1e-5, "{leak:?}");
        }
    }

    #[test]
    fn stains_lie_on_the_floor_they_were_scattered_over() {
        let (min, max) = (Vec2::new(-34.0, -50.0), Vec2::new(-4.0, -38.0));
        let all = stains(min, max);
        assert!(!all.is_empty());
        for stain in &all {
            let half = stain.size / 2.0;
            assert!(stain.at.x - half >= min.x && stain.at.x + half <= max.x, "{stain:?}");
            assert!(stain.at.y - half >= min.y && stain.at.y + half <= max.y, "{stain:?}");
        }
    }

    #[test]
    fn a_decal_faces_out_of_its_surface() {
        let leak = Leak { along: 0.0, top: 3.8, size: Vec2::new(1.25, 1.0), decal: Decal::Leak003 };
        let t = leak_transform(&leak, Vec3::new(0.0, 3.8, 5.0), Vec3::NEG_Z);
        assert!((t.rotation * Vec3::Z).abs_diff_eq(Vec3::NEG_Z, 1e-5));
        assert!((t.translation.y - 3.3).abs() < 1e-5, "its top at the face's");
        let stain = Stain { at: Vec2::new(1.0, 2.0), size: 1.5, yaw: 0.7, decal: Decal::OilStain1 };
        let t = stain_transform(&stain, 0.0);
        assert!((t.rotation * Vec3::Z).abs_diff_eq(Vec3::Y, 1e-5), "lying face up");
    }

    /// A page's four corners on the floor.
    fn corners(page: &Page) -> [Vec2; 4] {
        let (sin, cos) = page.yaw.sin_cos();
        [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
            let local = Vec2::new(sx, sy) * PAGE_SIZE / 2.0;
            page.at + Vec2::new(local.x * cos - local.y * sin, local.x * sin + local.y * cos)
        })
    }

    /// Every Archive Zone that wears Pages gets 12 to 30, wholly on its floor, and none is in a Range's sweep or a mouth strip.
    #[test]
    fn no_pages_decal_in_a_range_sweep_or_mouth_strip() {
        use super::super::{MapId, build::floor_rects, materials};
        let archive = MapId::Archive.layout();
        let mut dressed = 0;
        for &zone in archive.zones {
            if materials(Some(zone)).and_then(|set| set.decals) != Some(DecalFamily::Pages) {
                continue;
            }
            dressed += 1;
            let holes: Vec<_> = archive.sunken.iter().copied().filter(|sunken| sunken.zone == zone).collect();
            let rects = floor_rects(zone, &holes);
            let keep_out = page_keep_out(MapId::Archive, zone.floor());
            let all = pages(&rects, &keep_out);
            assert!((12..=30).contains(&all.len()), "{} gets {} pages", zone.name(), all.len());
            for page in &all {
                assert!(rects.iter().any(|&(lo, hi)| corners(page).iter().all(|c| c.x > lo.x && c.x < hi.x && c.y > lo.y && c.y < hi.y)), "{} page {page:?} is off the floor", zone.name());
            }
            // Against every Range on the Zone's level, not just the ones it joins.
            for range in super::super::archive::RANGES.iter().filter(|range| (range.floor - zone.floor()).abs() < 1.0) {
                for (lo, hi) in [range.sweep(), range.mouth_strips()[0], range.mouth_strips()[1]] {
                    for page in &all {
                        for c in corners(page) {
                            assert!(!(c.x > lo.x && c.x < hi.x && c.y > lo.y && c.y < hi.y), "{} page at {:?} is in {}'s sweep or mouth strip", zone.name(), page.at, range.name);
                        }
                    }
                }
            }
            // The same Zone is dressed the same way every Run.
            assert_eq!(all, pages(&rects, &keep_out));
        }
        assert_eq!(dressed, 8, "the reading halls and the four stack halls wear Pages");
        // No other Map has anything to keep pages out of.
        assert!(page_keep_out(MapId::Polder, 0.0).is_empty());
    }

    #[test]
    fn a_page_lies_flat_two_millimetres_over_the_floor_and_wears_the_paper_set() {
        let t = page_transform(&Page { at: Vec2::new(1.0, 2.0), yaw: 1.1 }, 4.0);
        assert!((t.translation.y - 4.002).abs() < 1e-5);
        assert!((t.rotation * Vec3::Z).abs_diff_eq(Vec3::Y, 1e-5), "lying face up");
        assert!(t.scale.abs_diff_eq(Vec3::new(0.21, 0.3, 1.0), 1e-6));
        assert_eq!(Decal::Page.file(), "materials/paper006/color.ktx2");
    }
}
