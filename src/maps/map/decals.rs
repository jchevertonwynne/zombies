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
            Decal::Foliage => "foliage",
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
                alpha_mode: if cut_out { AlphaMode::Mask(0.5) } else { AlphaMode::Blend },
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
}
