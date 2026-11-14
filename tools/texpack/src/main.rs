//! Turns a material's source maps (PNG or JPG, 8- or 16-bit) into the three KTX2 files the game
//! loads for it, each with a full mip chain and zstd supercompression:
//!
//! - `color.ktx2`: BC7, sRGB
//! - `normal.ktx2`: BC5 (red and green; the game rebuilds blue), linear, OpenGL convention
//! - `orm.ktx2`: BC7, linear: occlusion in red, roughness in green, metallic in blue
//!
//! ```text
//! texpack --color diff.png --normal nor_gl.png --orm arm.png --out assets/materials/<id>
//! ```
//!
//! A decal is one file, `color.ktx2` (BC7, sRGB, with its opacity as alpha), from a colour map and
//! an opacity map (`--color`, `--opacity`), or made here as a procedural oil stain (`--stain <seed>`).
//!
//! See `README.md` next to this crate for fetching a Poly Haven set and recording it in the
//! manifest.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ctt::encoders::Encoder;
use ctt::{AlphaMode, ColorSpace, Container, ConvertSettings, Format, Image, PipelineOutput, Quality, Surface, TargetFormat, TextureKind};

/// zstd level for the supercompression: slow to write, as fast as any to read.
const ZSTD_LEVEL: i32 = 19;

/// What to make, from the command line.
enum Job {
    /// A tiling material: colour, normal and ORM.
    Material { color: PathBuf, normal: PathBuf, orm: PathBuf },
    /// A decal: its colour with an opacity map as alpha, and nothing else.
    Decal { color: PathBuf, opacity: PathBuf, gain: f32 },
    /// A procedural oil stain, made here, from a seed.
    Stain { seed: u32 },
    /// A sky: an equirectangular HDR image as a cubemap, turned `yaw` degrees about the up axis.
    Sky { hdr: PathBuf, size: u32, yaw: f32 },
}

const USAGE: &str = "usage: texpack --color <png> --normal <png> --orm <png> --out <dir>
       texpack --color <png> --opacity <png> [--gain <x>] --out <dir>   (a decal)
       texpack --stain <seed> --out <dir>                  (a procedural oil stain)
       texpack --sky <hdr> [--size <face px>] [--yaw <degrees>] --out <dir>   (an HDRI as a cubemap)";

fn parse() -> Result<(Job, PathBuf)> {
    let mut args = std::env::args().skip(1);
    let mut flags = std::collections::HashMap::new();
    while let Some(flag) = args.next() {
        let value = args.next().with_context(|| format!("{flag} needs a value\n{USAGE}"))?;
        if !["--color", "--normal", "--orm", "--opacity", "--gain", "--stain", "--sky", "--size", "--yaw", "--out"].contains(&flag.as_str()) {
            bail!("unknown flag {flag}\n{USAGE}");
        }
        flags.insert(flag, value);
    }
    let mut take = |flag: &str| flags.remove(flag).map(PathBuf::from);
    let out = take("--out").context(USAGE)?;
    let job = if let Some(hdr) = take("--sky") {
        let number = |value: Option<PathBuf>, default: f32| match value {
            Some(value) => value.to_string_lossy().parse().context("--size and --yaw take numbers"),
            None => Ok(default),
        };
        Job::Sky { hdr, size: number(take("--size"), 512.0)? as u32, yaw: number(take("--yaw"), 0.0)? }
    } else if let Some(seed) = take("--stain") {
        Job::Stain { seed: seed.to_string_lossy().parse().context("--stain takes a number")? }
    } else if let Some(opacity) = take("--opacity") {
        let gain = match take("--gain") {
            Some(gain) => gain.to_string_lossy().parse().context("--gain takes a number")?,
            None => 1.0,
        };
        Job::Decal { color: take("--color").context(USAGE)?, opacity, gain }
    } else {
        Job::Material {
            color: take("--color").context(USAGE)?,
            normal: take("--normal").context(USAGE)?,
            orm: take("--orm").context(USAGE)?,
        }
    };
    Ok((job, out))
}

/// A source map as 8-bit RGBA, whatever depth it was stored at.
fn load(path: &Path, color_space: ColorSpace) -> Result<Image> {
    let source = image::open(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(image_of(source.to_rgba8(), color_space, AlphaMode::Opaque))
}

/// A colour map with an opacity map as its alpha, scaled by `gain` (to make a faint decal read).
fn load_with_alpha(color: &Path, opacity: &Path, gain: f32) -> Result<Image> {
    let mut rgba = image::open(color).with_context(|| format!("reading {}", color.display()))?.to_rgba8();
    let alpha = image::open(opacity).with_context(|| format!("reading {}", opacity.display()))?.to_luma8();
    if alpha.dimensions() != rgba.dimensions() {
        bail!("{} and {} differ in size", color.display(), opacity.display());
    }
    for (pixel, a) in rgba.pixels_mut().zip(alpha.pixels()) {
        pixel.0[3] = (a.0[0] as f32 * gain).min(255.0) as u8;
    }
    Ok(image_of(rgba, ColorSpace::Srgb, AlphaMode::Straight))
}

/// An oil stain, 512 pixels square: a near-black, faintly brown pool with a ragged edge and
/// darker, thicker patches, fading out well inside the square. Made here, so CC0.
fn stain(seed: u32) -> image::RgbaImage {
    const SIZE: u32 = 512;
    let noise = |x: f32, y: f32| fbm(x, y, seed);
    image::RgbaImage::from_fn(SIZE, SIZE, |px, py| {
        let (x, y) = (px as f32 / SIZE as f32 * 2.0 - 1.0, py as f32 / SIZE as f32 * 2.0 - 1.0);
        // A blob whose edge wanders with the noise, inside a radius of about 0.8.
        let r = (x * x + y * y).sqrt() + (noise(x * 2.5, y * 2.5) - 0.5) * 0.55;
        let body = smoothstep(0.82, 0.55, r);
        let thick = 0.55 + 0.45 * noise(x * 6.0 + 17.0, y * 6.0 - 5.0);
        let alpha = (body * thick * 255.0).clamp(0.0, 255.0) as u8;
        let shade = 0.9 + 0.2 * noise(x * 12.0 - 3.0, y * 12.0 + 9.0);
        let (r, g, b) = (14.0 * shade, 11.0 * shade, 8.0 * shade);
        image::Rgba([r as u8, g as u8, b as u8, alpha])
    })
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise at `(x, y)`, from 0 to 1, summed over four octaves.
fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let mut total = 0.0;
    let (mut amplitude, mut frequency, mut sum) = (0.5, 1.0, 0.0);
    for octave in 0..4 {
        total += amplitude * value_noise(x * frequency, y * frequency, seed.wrapping_add(octave * 1013));
        sum += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    total / sum
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let corner = |dx: f32, dy: f32| lattice((x0 + dx) as i32, (y0 + dy) as i32, seed);
    let top = corner(0.0, 0.0) + (corner(1.0, 0.0) - corner(0.0, 0.0)) * sx;
    let bottom = corner(0.0, 1.0) + (corner(1.0, 1.0) - corner(0.0, 1.0)) * sx;
    top + (bottom - top) * sy
}

/// A random value from 0 to 1 for a lattice point.
fn lattice(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

fn image_of(rgba: image::RgbaImage, color_space: ColorSpace, alpha: AlphaMode) -> Image {
    let (width, height) = rgba.dimensions();
    let surface = Surface {
        data: rgba.into_raw(),
        width,
        height,
        depth: 1,
        stride: width * 4,
        slice_stride: 0,
        format: if color_space == ColorSpace::Srgb { Format::R8G8B8A8_SRGB } else { Format::R8G8B8A8_UNORM },
        color_space,
        alpha,
    };
    Image {
        surfaces: vec![vec![surface]],
        kind: TextureKind::Texture2D,
    }
}

/// A sky as `sky.ktx2`: six BC6H faces (+X, -X, +Y, -Y, +Z, -Z) with a full mip chain, zstd
/// supercompressed. Bevy's `Skybox` shows it and `GeneratedEnvironmentMapLight` filters it, so one
/// file is both the backdrop and the outdoor reflections. Prints the sun's bearing, to pick a yaw.
fn write_sky(out: &Path, hdr: &Path, size: u32, yaw: f32) -> Result<()> {
    let source = image::open(hdr).with_context(|| format!("reading {}", hdr.display()))?.to_rgb32f();
    let (sw, sh) = source.dimensions();
    let yaw = yaw.to_radians();
    // Bilinear lookup of a direction, wrapping around in longitude. The image's middle is -Z.
    let sample = |d: [f32; 3]| -> [f32; 3] {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let (x, y, z) = (d[0] / len, d[1] / len, d[2] / len);
        let lon = x.atan2(-z) - yaw;
        let u = (lon / std::f32::consts::TAU + 0.5).rem_euclid(1.0) * sw as f32 - 0.5;
        let v = ((0.5 - y.clamp(-1.0, 1.0).asin() / std::f32::consts::PI) * sh as f32 - 0.5).clamp(0.0, sh as f32 - 1.0);
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let at = |px: f32, py: f32| source.get_pixel((px as i32).rem_euclid(sw as i32) as u32, (py as u32).min(sh - 1)).0;
        let (a, b, c, e) = (at(x0, y0), at(x0 + 1.0, y0), at(x0, y0 + 1.0), at(x0 + 1.0, y0 + 1.0));
        std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + e[i] * fx) * fy)
    };
    // The sun is the brightest pixel; say which way it lies, as a bearing in the game's world.
    let (mut best, mut at) = (0.0_f32, (0, 0));
    for (x, y, pixel) in source.enumerate_pixels() {
        let luma = pixel.0[0] + pixel.0[1] + pixel.0[2];
        if luma > best {
            (best, at) = (luma, (x, y));
        }
    }
    let lon = (at.0 as f32 + 0.5) / sw as f32 * std::f32::consts::TAU - std::f32::consts::PI + yaw;
    let lat = (0.5 - (at.1 as f32 + 0.5) / sh as f32) * std::f32::consts::PI;
    println!("brightest pixel {best:.0} at direction ({:.2}, {:.2}, {:.2})", lat.cos() * lon.sin(), lat.sin(), -lat.cos() * lon.cos());

    let faces = (0..6)
        .map(|face| {
            let mut data = Vec::with_capacity((size * size * 16) as usize);
            for py in 0..size {
                for px in 0..size {
                    let (sc, tc) = ((px as f32 + 0.5) / size as f32 * 2.0 - 1.0, (py as f32 + 0.5) / size as f32 * 2.0 - 1.0);
                    let direction = match face {
                        0 => [1.0, -tc, -sc],
                        1 => [-1.0, -tc, sc],
                        2 => [sc, 1.0, tc],
                        3 => [sc, -1.0, -tc],
                        4 => [sc, -tc, 1.0],
                        _ => [-sc, -tc, -1.0],
                    };
                    // BC6H holds up to half-float range; a sun disc far past that only blows out.
                    for channel in sample(direction) {
                        data.extend_from_slice(&channel.clamp(0.0, 2000.0).to_le_bytes());
                    }
                    data.extend_from_slice(&1.0_f32.to_le_bytes());
                }
            }
            vec![Surface {
                data,
                width: size,
                height: size,
                depth: 1,
                stride: size * 16,
                slice_stride: 0,
                format: Format::R32G32B32A32_SFLOAT,
                color_space: ColorSpace::Linear,
                alpha: AlphaMode::Opaque,
            }]
        })
        .collect();
    write(out, "sky", Image { surfaces: faces, kind: TextureKind::Cubemap }, Format::BC6H_UFLOAT_BLOCK)
}

fn encode(image: Image, format: Format) -> Result<Vec<u8>> {
    let output = ctt::convert(
        image,
        ConvertSettings {
            format: Some(TargetFormat::Compressed { format, encoder: Encoder::Auto }),
            container: Container::ktx2_zstd(ZSTD_LEVEL),
            quality: Quality::Slow,
            allow_discarding_alpha: true,
            mipmap: true,
            ..Default::default()
        },
    )?;
    match output {
        PipelineOutput::Encoded(bytes) => Ok(bytes),
        PipelineOutput::Raw(_) => bail!("ctt returned a raw image for a KTX2 container"),
    }
}

fn write(out: &Path, name: &str, image: Image, format: Format) -> Result<()> {
    let bytes = encode(image, format).with_context(|| format!("encoding {name}"))?;
    let path = out.join(format!("{name}.ktx2"));
    std::fs::write(&path, &bytes)?;
    println!("{} ({:.2} MB)", path.display(), bytes.len() as f64 / 1e6);
    Ok(())
}

fn main() -> Result<()> {
    let (job, out) = parse()?;
    std::fs::create_dir_all(&out)?;
    // ctt names the BC7 block format once; an sRGB source makes it write BC7_SRGB.
    match job {
        Job::Material { color, normal, orm } => {
            write(&out, "color", load(&color, ColorSpace::Srgb)?, Format::BC7_UNORM_BLOCK)?;
            write(&out, "normal", load(&normal, ColorSpace::Linear)?, Format::BC5_UNORM_BLOCK)?;
            write(&out, "orm", load(&orm, ColorSpace::Linear)?, Format::BC7_UNORM_BLOCK)?;
        }
        Job::Decal { color, opacity, gain } => write(&out, "color", load_with_alpha(&color, &opacity, gain)?, Format::BC7_UNORM_BLOCK)?,
        Job::Sky { hdr, size, yaw } => write_sky(&out, &hdr, size, yaw)?,
        Job::Stain { seed } => write(&out, "color", image_of(stain(seed), ColorSpace::Srgb, AlphaMode::Straight), Format::BC7_UNORM_BLOCK)?,
    }
    Ok(())
}
