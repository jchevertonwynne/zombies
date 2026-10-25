//! Writes a synthesised OGG placeholder for every audio slot the game declares.
//!
//! `cargo gen-sounds` (an alias in `.cargo/config.toml`) fills in slots that have no files yet. Slots that already have
//! files are left alone, so real recordings are never overwritten, unless `--force` is given.
//! Announcer lines are spoken by `espeak-ng`, which must be on the `PATH` for them.

#[allow(dead_code, unfulfilled_lint_expectations)]
#[path = "../../../src/audio/cues.rs"]
mod cues;
#[allow(dead_code, unfulfilled_lint_expectations)]
#[path = "../../../src/audio/slots.rs"]
mod slots;

use std::error::Error;
use std::f32::consts::TAU;
use std::fs::File;
use std::io::BufWriter;
use std::num::{NonZeroU8, NonZeroU32};
use std::path::Path;
use std::process::{Command, ExitCode};

use cues::{AUDIO_DIR, Cue};
use vorbis_rs::VorbisEncoderBuilder;

const SAMPLE_RATE: u32 = 22_050;
/// A fixed Ogg stream serial, so regenerating a placeholder gives the same bytes.
const STREAM_SERIAL: i32 = 0x2b1d;

/// How a cue's placeholder is made.
enum Recipe {
    /// Mono samples in `[-1, 1]` at `SAMPLE_RATE`.
    Synth(Vec<f32>),
    /// A line for `espeak-ng` to speak.
    #[expect(
        dead_code,
        reason = "the first announcer lines arrive with the Appendix C cues"
    )]
    Speak(&'static str),
}

fn recipe(cue: Cue) -> Recipe {
    match cue {
        Cue::RoundStart => Recipe::Synth(round_start()),
        Cue::GunshotPistol => Recipe::Synth(gunshot()),
        Cue::Hit => Recipe::Synth(thud(0.06, 220.0)),
        Cue::Kill => Recipe::Synth(thud(0.18, 90.0)),
    }
}

fn main() -> ExitCode {
    let mut force = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--force" => force = true,
            _ => {
                eprintln!("usage: gen-sounds [--force]");
                return ExitCode::FAILURE;
            }
        }
    }

    // This crate sits in `tools/gen-sounds`; the game's assets are at the workspace root.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is two levels down");
    let dir = root.join("assets").join(AUDIO_DIR);
    let files = slots::list_files(&dir);
    let mut failed = false;

    for cue in Cue::ALL {
        let stem = cue.stem();
        let existing = slots::files_for(&stem, &files);
        if !existing.is_empty() && !force {
            println!("kept   {stem}");
            continue;
        }
        match write_placeholder(&dir, &stem, &existing, recipe(cue)) {
            Ok(()) => println!("wrote  {stem}.ogg"),
            Err(error) => {
                eprintln!("failed {stem}: {error}");
                failed = true;
            }
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Writes `<stem>.ogg`, removing the slot's other files so it stays a single, valid slot.
fn write_placeholder(
    dir: &Path,
    stem: &str,
    existing: &[String],
    recipe: Recipe,
) -> Result<(), Box<dyn Error>> {
    let (rate, samples) = match recipe {
        Recipe::Synth(samples) => (SAMPLE_RATE, samples),
        Recipe::Speak(line) => speak(line)?,
    };
    let path = dir.join(format!("{stem}.ogg"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_ogg(&path, rate, &samples)?;
    for file in existing.iter().filter(|f| **f != format!("{stem}.ogg")) {
        std::fs::remove_file(dir.join(file))?;
        println!("removed {file}");
    }
    Ok(())
}

/// Speaks `line` with espeak-ng, returning its sample rate and mono samples.
fn speak(line: &str) -> Result<(u32, Vec<f32>), Box<dyn Error>> {
    let wav = std::env::temp_dir().join(format!("gen-sounds-{}.wav", std::process::id()));
    let status = Command::new("espeak-ng")
        .arg("-w")
        .arg(&wav)
        .arg(line)
        .status()
        .map_err(|e| format!("couldn't run espeak-ng ({e}); is it installed and on the PATH?"))?;
    if !status.success() {
        return Err(format!("espeak-ng exited with {status}").into());
    }

    let mut reader = hound::WavReader::open(&wav)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_format != hound::SampleFormat::Int
        || spec.bits_per_sample != 16
    {
        return Err(format!("unexpected espeak-ng output format: {spec:?}").into());
    }
    let samples = reader
        .samples::<i16>()
        .map(|s| s.map(|s| s as f32 / i16::MAX as f32))
        .collect::<Result<_, _>>()?;
    std::fs::remove_file(&wav)?;
    Ok((spec.sample_rate, samples))
}

fn write_ogg(path: &Path, rate: u32, samples: &[f32]) -> Result<(), Box<dyn Error>> {
    let mut encoder = VorbisEncoderBuilder::new_with_serial(
        NonZeroU32::new(rate).ok_or("zero sample rate")?,
        NonZeroU8::MIN,
        BufWriter::new(File::create(path)?),
        STREAM_SERIAL,
    )
    .build()?;
    for block in samples.chunks(1024) {
        encoder.encode_audio_block([block])?;
    }
    encoder.finish()?;
    Ok(())
}

fn samples(secs: f32) -> impl Iterator<Item = f32> {
    (0..(secs * SAMPLE_RATE as f32) as u32).map(|i| i as f32 / SAMPLE_RATE as f32)
}

/// Cheap deterministic white noise in [-1, 1].
fn noise(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn gunshot() -> Vec<f32> {
    let mut seed = 0x1234_5678;
    samples(0.3)
        .map(|t| {
            let crack = noise(&mut seed) * (-t * 30.0).exp();
            let boom = (TAU * 70.0 * t).sin() * (-t * 15.0).exp();
            (crack * 0.8 + boom * 0.6).clamp(-1.0, 1.0)
        })
        .collect()
}

fn thud(secs: f32, freq: f32) -> Vec<f32> {
    let mut seed = 0x9e37_79b9;
    samples(secs)
        .map(|t| {
            let decay = (-t * 6.0 / secs).exp();
            ((TAU * freq * t).sin() * 0.7 + noise(&mut seed) * 0.3) * decay
        })
        .collect()
}

/// Three low, descending, wobbling notes.
fn round_start() -> Vec<f32> {
    let notes = [(110.0, 0.0), (98.0, 0.45), (73.4, 0.9)];
    samples(2.4)
        .map(|t| {
            notes
                .iter()
                .filter(|(_, start)| t >= *start)
                .map(|(freq, start)| {
                    let local = t - start;
                    let envelope = (local * 20.0).min(1.0) * (-local * 1.8).exp();
                    let tremolo = 1.0 + 0.3 * (TAU * 6.0 * local).sin();
                    // Odd harmonics give it a hollow, horn-ish tone.
                    let tone = (TAU * freq * local).sin()
                        + (TAU * freq * 3.0 * local).sin() / 3.0
                        + (TAU * freq * 5.0 * local).sin() / 5.0;
                    tone * envelope * tremolo * 0.3
                })
                .sum::<f32>()
                .clamp(-1.0, 1.0)
        })
        .collect()
}
