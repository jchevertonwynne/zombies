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
    /// A line for `espeak-ng` to speak, mixed over an optional synthesised bed.
    Speak(&'static str, Option<Vec<f32>>),
}

fn recipe(cue: Cue) -> Recipe {
    match cue {
        Cue::RoundStart => Recipe::Synth(round_start()),
        Cue::HellhoundRoundStart => Recipe::Synth(hellhound_round_start()),
        Cue::GunshotPistol => Recipe::Synth(gunshot()),
        Cue::Hit => Recipe::Synth(thud(0.06, 220.0)),
        Cue::Kill => Recipe::Synth(thud(0.18, 90.0)),
        Cue::Buy => Recipe::Synth(buy()),
        Cue::CantAfford => Recipe::Synth(buzzer()),
        Cue::PlankTear => Recipe::Synth(wood_crack()),
        Cue::PlankRebuild => Recipe::Synth(plank_rebuild()),
        Cue::HoundSpawn => Recipe::Synth(lightning_crack()),
        Cue::HoundBite => Recipe::Synth(snarl_snap()),
        Cue::HoundDeath => Recipe::Synth(yelp()),
        Cue::AnnouncerMaxAmmo => Recipe::Speak("Max Ammo!", None),
        Cue::AnnouncerInstaKill => Recipe::Speak("Insta-Kill!", None),
        Cue::AnnouncerDoublePoints => Recipe::Speak("Double Points!", None),
        Cue::AnnouncerNuke => Recipe::Speak("Kaboom!", None),
        Cue::AnnouncerCarpenter => Recipe::Speak("Carpenter!", None),
        Cue::AnnouncerFireSale => Recipe::Speak("Fire Sale!", None),
        Cue::AnnouncerHellhoundRound => Recipe::Speak("Fetch me their souls!", None),
        Cue::SapperWarning => Recipe::Speak("Sapper inbound!", Some(bellow())),
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

    for &cue in Cue::ALL {
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
        Recipe::Speak(line, bed) => {
            let (rate, mut voice) = speak(line)?;
            if let Some(bed) = bed {
                if rate != SAMPLE_RATE {
                    return Err(format!("espeak-ng spoke at {rate} Hz, not {SAMPLE_RATE}").into());
                }
                mix_under(&mut voice, &bed);
            }
            (rate, voice)
        }
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
    // A slower, lower voice than espeak-ng's default reads more like an announcer.
    let status = Command::new("espeak-ng")
        .args(["-v", "en-us", "-s", "135", "-p", "25"])
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

/// Mixes `bed` under `voice`, lengthening `voice` to fit.
fn mix_under(voice: &mut Vec<f32>, bed: &[f32]) {
    if voice.len() < bed.len() {
        voice.resize(bed.len(), 0.0);
    }
    for (v, b) in voice.iter_mut().zip(bed) {
        *v = (*v + b).clamp(-1.0, 1.0);
    }
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
    descending_notes(&[(110.0, 0.0), (98.0, 0.45), (73.4, 0.9)], 2.4)
}

/// A thunder crack, then a slower, darker `round_start`.
fn hellhound_round_start() -> Vec<f32> {
    let mut seed = 0x7e11_d06;
    let notes = descending_notes(&[(82.4, 0.6), (73.4, 1.2), (55.0, 1.8)], 3.6);
    samples(3.6)
        .zip(notes)
        .map(|(t, note)| {
            let crack = noise(&mut seed) * (-t * 8.0).exp() * 0.7;
            let rumble = (TAU * 40.0 * t).sin() * (-t * 1.5).exp() * 0.3 * (1.0 + 0.5 * noise(&mut seed));
            (crack + rumble + note).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A sharp, bright crack with a sizzle tail, for a hound's lightning strike.
fn lightning_crack() -> Vec<f32> {
    let mut seed = 0x0b01_7000;
    samples(0.9)
        .map(|t| {
            let crack = noise(&mut seed) * (-t * 25.0).exp();
            let sizzle = noise(&mut seed) * (-t * 4.0).exp() * 0.25 * (TAU * 60.0 * t).sin().abs();
            let boom = (TAU * 55.0 * t).sin() * (-t * 6.0).exp() * 0.4;
            (crack * 0.8 + sizzle + boom).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A short rasping snarl that ends in a snap of jaws.
fn snarl_snap() -> Vec<f32> {
    let mut seed = 0x5a4a_1000;
    samples(0.35)
        .map(|t| {
            let snarl = if t < 0.25 {
                let growl = (TAU * 140.0 * t).sin() * (1.0 + 0.6 * noise(&mut seed));
                growl * (t * 40.0).min(1.0) * 0.35
            } else {
                0.0
            };
            let snap = if t >= 0.25 {
                let local = t - 0.25;
                noise(&mut seed) * (-local * 80.0).exp() * 0.8
            } else {
                0.0
            };
            (snarl + snap).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A high yelp falling away.
fn yelp() -> Vec<f32> {
    let secs = 0.5;
    samples(secs)
        .map(|t| {
            let freq = 900.0 - 600.0 * t / secs;
            let tone = (TAU * freq * t).sin() + 0.3 * (TAU * freq * 2.0 * t).sin();
            tone * (t * 60.0).min(1.0) * (-t * 6.0).exp() * 0.4
        })
        .collect()
}

/// Low, descending, wobbling `(frequency, start)` notes over `secs`.
fn descending_notes(notes: &[(f32, f32)], secs: f32) -> Vec<f32> {
    samples(secs)
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

/// A distant, low bellow: a slow-swelling, falling growl kept quiet so speech sits on top.
fn bellow() -> Vec<f32> {
    let mut seed = 0x0bad_f00d;
    let secs = 2.2;
    samples(secs)
        .map(|t| {
            let swell = (t / 0.6).min(1.0) * (-(t - 0.6).max(0.0) * 1.4).exp();
            let freq = 70.0 - 20.0 * t / secs;
            let growl = (TAU * freq * t).sin() + 0.5 * (TAU * freq * 2.0 * t).sin();
            let rasp = 1.0 + 0.4 * noise(&mut seed);
            growl * rasp * swell * 0.25
        })
        .collect()
}

/// Two bright rising notes for a purchase.
fn buy() -> Vec<f32> {
    samples(0.4)
        .map(|t| {
            let (freq, local) = if t < 0.12 { (880.0, t) } else { (1320.0, t - 0.12) };
            let envelope = (local * 300.0).min(1.0) * (-local * 9.0).exp();
            (TAU * freq * local).sin() * envelope * 0.4
        })
        .collect()
}

/// A harsh low square-wave buzz for a buy the Player can't afford.
fn buzzer() -> Vec<f32> {
    samples(0.35)
        .map(|t| {
            let square = if (TAU * 110.0 * t).sin() >= 0.0 { 1.0 } else { -1.0 };
            let envelope = (t * 200.0).min(1.0) * (1.0 - t / 0.35);
            square * envelope * 0.35
        })
        .collect()
}

/// A sharp splintering crack with a hollow knock under it, for a Plank torn off.
fn wood_crack() -> Vec<f32> {
    let mut seed = 0x5eed_c0de;
    samples(0.45)
        .map(|t| {
            let splinter = noise(&mut seed) * (-t * 18.0).exp() * (1.0 + (TAU * 37.0 * t).sin());
            let knock = (TAU * 180.0 * t).sin() * (-t * 25.0).exp();
            (splinter * 0.5 + knock * 0.5).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A wooden knock, then a small ding, for a Plank put back.
fn plank_rebuild() -> Vec<f32> {
    let mut seed = 0x0dd_ba11;
    samples(0.5)
        .map(|t| {
            let knock = ((TAU * 150.0 * t).sin() * 0.7 + noise(&mut seed) * 0.3) * (-t * 30.0).exp();
            let ding = if t >= 0.15 {
                let local = t - 0.15;
                (TAU * 1760.0 * local).sin() * (local * 300.0).min(1.0) * (-local * 10.0).exp() * 0.25
            } else {
                0.0
            };
            (knock * 0.6 + ding).clamp(-1.0, 1.0)
        })
        .collect()
}
