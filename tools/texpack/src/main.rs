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
//! See `README.md` next to this crate for fetching a Poly Haven set and recording it in the
//! manifest.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ctt::encoders::Encoder;
use ctt::{AlphaMode, ColorSpace, Container, ConvertSettings, Format, Image, PipelineOutput, Quality, Surface, TargetFormat, TextureKind};

/// zstd level for the supercompression: slow to write, as fast as any to read.
const ZSTD_LEVEL: i32 = 19;

struct Args {
    color: PathBuf,
    normal: PathBuf,
    orm: PathBuf,
    out: PathBuf,
}

fn parse() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let (mut color, mut normal, mut orm, mut out) = (None, None, None, None);
    while let Some(flag) = args.next() {
        let value = args.next().with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--color" => color = Some(value.into()),
            "--normal" => normal = Some(value.into()),
            "--orm" => orm = Some(value.into()),
            "--out" => out = Some(value.into()),
            _ => bail!("unknown flag {flag}"),
        }
    }
    let usage = "usage: texpack --color <png> --normal <png> --orm <png> --out <dir>";
    Ok(Args {
        color: color.context(usage)?,
        normal: normal.context(usage)?,
        orm: orm.context(usage)?,
        out: out.context(usage)?,
    })
}

/// A source map as 8-bit RGBA, whatever depth it was stored at.
fn load(path: &Path, color_space: ColorSpace) -> Result<Image> {
    let source = image::open(path).with_context(|| format!("reading {}", path.display()))?;
    let rgba = source.to_rgba8();
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
        alpha: AlphaMode::Opaque,
    };
    Ok(Image {
        surfaces: vec![vec![surface]],
        kind: TextureKind::Texture2D,
    })
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

fn main() -> Result<()> {
    let args = parse()?;
    std::fs::create_dir_all(&args.out)?;
    // ctt names the BC7 block format once; an sRGB source makes it write BC7_SRGB.
    for (source, name, color_space, format) in [
        (&args.color, "color", ColorSpace::Srgb, Format::BC7_UNORM_BLOCK),
        (&args.normal, "normal", ColorSpace::Linear, Format::BC5_UNORM_BLOCK),
        (&args.orm, "orm", ColorSpace::Linear, Format::BC7_UNORM_BLOCK),
    ] {
        let bytes = encode(load(source, color_space)?, format).with_context(|| format!("encoding {}", source.display()))?;
        let path = args.out.join(format!("{name}.ktx2"));
        std::fs::write(&path, &bytes)?;
        println!("{} ({:.2} MB)", path.display(), bytes.len() as f64 / 1e6);
    }
    Ok(())
}
