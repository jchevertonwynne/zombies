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
    Ok(image_of(block_aligned(source.to_rgba8()), color_space, AlphaMode::Opaque))
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

/// Resamples an image whose sides aren't multiples of 4 (a photographed swatch that isn't square
/// comes at 1024 x 1018, say) to the nearest ones, since BC formats need whole 4 x 4 blocks.
fn block_aligned(rgba: image::RgbaImage) -> image::RgbaImage {
    let round = |side: u32| ((side + 2) / 4 * 4).max(4);
    let (width, height) = (round(rgba.width()), round(rgba.height()));
    if (width, height) == rgba.dimensions() {
        rgba
    } else {
        image::imageops::resize(&rgba, width, height, image::imageops::FilterType::Lanczos3)
    }
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

    // The cubemap at full size, face by face (+X, -X, +Y, -Y, +Z, -Z), as linear RGB.
    // BC6H holds up to half-float range; a sun disc far past that only blows out.
    let cube: Vec<Vec<[f32; 3]>> = (0..6)
        .map(|face| {
            let mut texels = Vec::with_capacity((size * size) as usize);
            for py in 0..size {
                for px in 0..size {
                    let (sc, tc) = ((px as f32 + 0.5) / size as f32 * 2.0 - 1.0, (py as f32 + 0.5) / size as f32 * 2.0 - 1.0);
                    texels.push(sample(face_direction(face, sc, tc)).map(|c| c.clamp(0.0, 2000.0)));
                }
            }
            texels
        })
        .collect();
    write(out, "sky", Image { surfaces: cube.iter().map(|face| vec![surface_of(face, size)]).collect(), kind: TextureKind::Cubemap }, Format::BC6H_UFLOAT_BLOCK, true)?;
    prefilter(out, cube, size)
}

/// The direction a cubemap face's texel looks along, for face coordinates `sc`, `tc` in -1..1.
fn face_direction(face: usize, sc: f32, tc: f32) -> [f32; 3] {
    match face {
        0 => [1.0, -tc, -sc],
        1 => [-1.0, -tc, sc],
        2 => [sc, 1.0, tc],
        3 => [sc, -1.0, -tc],
        4 => [sc, -tc, 1.0],
        _ => [-sc, -tc, -1.0],
    }
}

/// A float cubemap with a full mip chain (a 2x2 box filter), which can be sampled by direction.
struct CubeChain {
    /// `levels[mip][face]`, each `(size >> mip)` squared.
    levels: Vec<Vec<Vec<[f32; 3]>>>,
}

impl CubeChain {
    fn new(cube: Vec<Vec<[f32; 3]>>, size: u32) -> Self {
        let mut levels = vec![cube];
        let mut width = size as usize;
        while width > 1 {
            let last = levels.last().unwrap();
            let half = width / 2;
            let next = last
                .iter()
                .map(|face| {
                    (0..half * half)
                        .map(|i| {
                            let (x, y) = ((i % half) * 2, (i / half) * 2);
                            let texels = [face[y * width + x], face[y * width + x + 1], face[(y + 1) * width + x], face[(y + 1) * width + x + 1]];
                            std::array::from_fn(|c| texels.iter().map(|t| t[c]).sum::<f32>() / 4.0)
                        })
                        .collect()
                })
                .collect();
            levels.push(next);
            width = half;
        }
        CubeChain { levels }
    }

    fn width(&self, mip: usize) -> usize {
        (self.levels[0][0].len() as f32).sqrt() as usize >> mip
    }

    /// Bilinear within a face, at one mip.
    fn sample_mip(&self, d: [f32; 3], mip: usize) -> [f32; 3] {
        let (ax, ay, az) = (d[0].abs(), d[1].abs(), d[2].abs());
        let (face, sc, tc) = if ax >= ay && ax >= az {
            if d[0] > 0.0 { (0, -d[2] / ax, -d[1] / ax) } else { (1, d[2] / ax, -d[1] / ax) }
        } else if ay >= az {
            if d[1] > 0.0 { (2, d[0] / ay, d[2] / ay) } else { (3, d[0] / ay, -d[2] / ay) }
        } else if d[2] > 0.0 {
            (4, d[0] / az, -d[1] / az)
        } else {
            (5, -d[0] / az, -d[1] / az)
        };
        let w = self.width(mip);
        let (u, v) = ((sc * 0.5 + 0.5) * w as f32 - 0.5, (tc * 0.5 + 0.5) * w as f32 - 0.5);
        let (x0, y0) = (u.floor(), v.floor());
        let (fx, fy) = (u - x0, v - y0);
        let at = |x: f32, y: f32| self.levels[mip][face][(y.clamp(0.0, w as f32 - 1.0) as usize) * w + x.clamp(0.0, w as f32 - 1.0) as usize];
        let (a, b, c, e) = (at(x0, y0), at(x0 + 1.0, y0), at(x0, y0 + 1.0), at(x0 + 1.0, y0 + 1.0));
        std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + e[i] * fx) * fy)
    }

    /// Trilinear, at a fractional mip.
    fn sample(&self, d: [f32; 3], lod: f32) -> [f32; 3] {
        let lod = lod.clamp(0.0, (self.levels.len() - 1) as f32);
        let (lo, f) = (lod.floor() as usize, lod.fract());
        let a = self.sample_mip(d, lo);
        if f == 0.0 || lo + 1 >= self.levels.len() {
            return a;
        }
        let b = self.sample_mip(d, lo + 1);
        std::array::from_fn(|i| a[i] * (1.0 - f) + b[i] * f)
    }
}

fn surface_of(texels: &[[f32; 3]], width: u32) -> Surface {
    let mut data = Vec::with_capacity(texels.len() * 16);
    for texel in texels {
        for channel in texel {
            data.extend_from_slice(&channel.to_le_bytes());
        }
        data.extend_from_slice(&1.0_f32.to_le_bytes());
    }
    Surface {
        data,
        width,
        height: width,
        depth: 1,
        stride: width * 16,
        slice_stride: 0,
        format: Format::R32G32B32A32_SFLOAT,
        color_space: ColorSpace::Linear,
        alpha: AlphaMode::Opaque,
    }
}

/// Runs `texel(face, sc, tc)` over every texel of a `width`-px cubemap face set, on all cores.
fn bake_faces(width: usize, texel: impl Fn([f32; 3]) -> [f32; 3] + Sync) -> Vec<Vec<[f32; 3]>> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    (0..6)
        .map(|face| {
            let mut texels = vec![[0.0; 3]; width * width];
            let rows = width.div_ceil(threads).max(1);
            std::thread::scope(|scope| {
                for (chunk, band) in texels.chunks_mut(rows * width).enumerate() {
                    let texel = &texel;
                    scope.spawn(move || {
                        for (i, out) in band.iter_mut().enumerate() {
                            let index = chunk * rows * width + i;
                            let (px, py) = (index % width, index / width);
                            let (sc, tc) = ((px as f32 + 0.5) / width as f32 * 2.0 - 1.0, (py as f32 + 0.5) / width as f32 * 2.0 - 1.0);
                            *out = texel(face_direction(face, sc, tc));
                        }
                    });
                }
            });
            texels
        })
        .collect()
}

/// The sky, prefiltered for image-based lighting, as `diffuse.ktx2` (a 32 px cosine-convolved
/// irradiance cubemap) and `specular.ktx2` (a GGX-filtered chain whose mip `m` of `n` is
/// perceptual roughness `m / (n - 1)`, which is how Bevy's `EnvironmentMapLight` reads it). This is
/// what Bevy's `GeneratedEnvironmentMapLight` would filter on the GPU, done once, here.
fn prefilter(out: &Path, cube: Vec<Vec<[f32; 3]>>, size: u32) -> Result<()> {
    use std::f32::consts::PI;
    let chain = CubeChain::new(cube, size);
    let normalise = |d: [f32; 3]| {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        [d[0] / len, d[1] / len, d[2] / len]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

    // Diffuse: the sky's 64 px mip, weighted by cosine and each texel's solid angle.
    let source_mip = (size.trailing_zeros() - 6) as usize;
    let source_width = chain.width(source_mip);
    let mut texels: Vec<([f32; 3], f32, [f32; 3])> = Vec::new();
    for face in 0..6 {
        for i in 0..source_width * source_width {
            let (px, py) = (i % source_width, i / source_width);
            let (sc, tc) = ((px as f32 + 0.5) / source_width as f32 * 2.0 - 1.0, (py as f32 + 0.5) / source_width as f32 * 2.0 - 1.0);
            let omega = 4.0 / (source_width * source_width) as f32 / (1.0 + sc * sc + tc * tc).powf(1.5);
            texels.push((normalise(face_direction(face, sc, tc)), omega, chain.levels[source_mip][face][i]));
        }
    }
    let diffuse = bake_faces(32, |n| {
        let mut sum = [0.0; 3];
        for (d, omega, colour) in &texels {
            let cos = dot(n, *d);
            if cos > 0.0 {
                for c in 0..3 {
                    sum[c] += colour[c] * cos * omega;
                }
            }
        }
        sum.map(|c| c / PI)
    });
    write(out, "diffuse", Image { surfaces: diffuse.iter().map(|face| vec![surface_of(face, 32)]).collect(), kind: TextureKind::Cubemap }, Format::BC6H_UFLOAT_BLOCK, false)?;

    // Specular: GGX importance sampling with the view along the normal, off a filtered lookup.
    let mips = size.trailing_zeros() as usize + 1;
    let mut by_mip: Vec<Vec<Vec<[f32; 3]>>> = Vec::new();
    for mip in 0..mips {
        let width = (size >> mip) as usize;
        let roughness = mip as f32 / (mips - 1) as f32;
        let alpha = roughness * roughness;
        if mip == 0 {
            by_mip.push(chain.levels[0].clone());
            continue;
        }
        let samples = 4 * 32 * 2u32.pow((roughness * 4.0) as u32);
        let omega_p = 4.0 * PI / (6.0 * (size * size) as f32);
        by_mip.push(bake_faces(width, |n| {
            let up = if n[1].abs() < 0.99 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
            let t = normalise([up[1] * n[2] - up[2] * n[1], up[2] * n[0] - up[0] * n[2], up[0] * n[1] - up[1] * n[0]]);
            let b = [n[1] * t[2] - n[2] * t[1], n[2] * t[0] - n[0] * t[2], n[0] * t[1] - n[1] * t[0]];
            let (mut sum, mut weight) = ([0.0_f32; 3], 0.0_f32);
            for i in 0..samples {
                let (u, v) = ((i as f32 + 0.5) / samples as f32, i.reverse_bits() as f32 / 4294967296.0);
                let cos_h = ((1.0 - v) / (1.0 + (alpha * alpha - 1.0) * v)).sqrt();
                let sin_h = (1.0 - cos_h * cos_h).max(0.0).sqrt();
                let phi = 2.0 * PI * u;
                let h = [t[0] * sin_h * phi.cos() + b[0] * sin_h * phi.sin() + n[0] * cos_h, t[1] * sin_h * phi.cos() + b[1] * sin_h * phi.sin() + n[1] * cos_h, t[2] * sin_h * phi.cos() + b[2] * sin_h * phi.sin() + n[2] * cos_h];
                let l = [2.0 * cos_h * h[0] - n[0], 2.0 * cos_h * h[1] - n[1], 2.0 * cos_h * h[2] - n[2]];
                let n_dot_l = dot(n, l);
                if n_dot_l <= 0.0 {
                    continue;
                }
                let denominator = cos_h * cos_h * (alpha * alpha - 1.0) + 1.0;
                let pdf = alpha * alpha / (PI * denominator * denominator) / 4.0;
                let lod = 0.5 * (1.0 / (samples as f32 * pdf) / omega_p).log2();
                let colour = chain.sample(l, lod);
                for c in 0..3 {
                    sum[c] += colour[c] * n_dot_l;
                }
                weight += n_dot_l;
            }
            sum.map(|c| c / weight.max(1e-6))
        }));
    }
    let surfaces = (0..6)
        .map(|face| (0..mips).map(|mip| surface_of(&by_mip[mip][face], size >> mip)).collect())
        .collect();
    write(out, "specular", Image { surfaces, kind: TextureKind::Cubemap }, Format::BC6H_UFLOAT_BLOCK, false)
}

fn encode(image: Image, format: Format, mipmap: bool) -> Result<Vec<u8>> {
    let output = ctt::convert(
        image,
        ConvertSettings {
            format: Some(TargetFormat::Compressed { format, encoder: Encoder::Auto }),
            container: Container::ktx2_zstd(ZSTD_LEVEL),
            quality: Quality::Slow,
            allow_discarding_alpha: true,
            mipmap,
            ..Default::default()
        },
    )?;
    match output {
        PipelineOutput::Encoded(bytes) => Ok(bytes),
        PipelineOutput::Raw(_) => bail!("ctt returned a raw image for a KTX2 container"),
    }
}

fn write(out: &Path, name: &str, image: Image, format: Format, mipmap: bool) -> Result<()> {
    let bytes = encode(image, format, mipmap).with_context(|| format!("encoding {name}"))?;
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
            write(&out, "color", load(&color, ColorSpace::Srgb)?, Format::BC7_UNORM_BLOCK, true)?;
            write(&out, "normal", load(&normal, ColorSpace::Linear)?, Format::BC5_UNORM_BLOCK, true)?;
            write(&out, "orm", load(&orm, ColorSpace::Linear)?, Format::BC7_UNORM_BLOCK, true)?;
        }
        Job::Decal { color, opacity, gain } => write(&out, "color", load_with_alpha(&color, &opacity, gain)?, Format::BC7_UNORM_BLOCK, true)?,
        Job::Sky { hdr, size, yaw } => write_sky(&out, &hdr, size, yaw)?,
        Job::Stain { seed } => write(&out, "color", image_of(stain(seed), ColorSpace::Srgb, AlphaMode::Straight), Format::BC7_UNORM_BLOCK, true)?,
    }
    Ok(())
}
