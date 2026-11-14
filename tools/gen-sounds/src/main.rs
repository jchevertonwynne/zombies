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
use std::f32::consts::{PI, TAU};
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
        Cue::Shellshock => Recipe::Synth(ear_ring()),
        Cue::NovaVent => Recipe::Synth(vent_bang()),
        Cue::NovaAttack => Recipe::Synth(screech()),
        Cue::NovaBurst => Recipe::Synth(wet_pop()),
        Cue::NovaGas => Recipe::Synth(gas_hiss()),
        Cue::NapalmRise => Recipe::Synth(rumble_flare()),
        Cue::NapalmWindup => Recipe::Synth(rising_whoosh(0.75, 0x0a7a_1001)),
        Cue::NapalmExplode => Recipe::Synth(fireball_boom()),
        Cue::FireBurn => Recipe::Synth(sizzle()),
        Cue::ShriekerRise => Recipe::Synth(shriek(0.6, 500.0, 1400.0, 0x5e1e_0001)),
        Cue::ShriekerIdle => Recipe::Synth(gasps()),
        Cue::ShriekerWindup => Recipe::Synth(rising_whoosh(0.5, 0x5e1e_0002)),
        Cue::ShriekerScream => Recipe::Synth(shriek(1.0, 1800.0, 1500.0, 0x5e1e_0003)),
        Cue::ShriekerDeath => Recipe::Synth(shriek(1.4, 1100.0, 600.0, 0x5e1e_0004)),
        Cue::ScreamPop => Recipe::Synth(thud(0.12, 140.0)),
        Cue::SapperSmash => Recipe::Synth(sapper_smash()),
        Cue::SapperIdle => Recipe::Synth(sapper_idle()),
        Cue::SapperStep => Recipe::Synth(sapper_step()),
        Cue::SapperSabotage => Recipe::Synth(sapper_sabotage()),
        Cue::SapperWindup => Recipe::Synth(rising_whoosh(0.8, 0x5a99_0001)),
        Cue::SapperSlam => Recipe::Synth(thud(0.7, 45.0)),
        Cue::HelmetOff => Recipe::Synth(clang()),
        Cue::SapperStun => Recipe::Synth(thud(0.35, 120.0)),
        Cue::SapperDeath => Recipe::Synth(bellow()),
        Cue::RoundEnd => Recipe::Synth(descending_notes(&[(73.4, 0.0), (98.0, 0.45), (146.8, 0.9)], 2.6)),
        Cue::BoonOffer => Recipe::Synth(chime(&[1318.5, 1760.0], 1.2, 5.0)),
        Cue::PactOffer => Recipe::Synth(bell_toll()),
        Cue::OfferPick => Recipe::Synth(tick(0.06, 2400.0)),
        Cue::GameOver => Recipe::Synth(game_over()),
        Cue::GunshotSmg => Recipe::Synth(class_gunshot(0.25, 95.0, 38.0, 0x6a00_0002)),
        Cue::GunshotRifle => Recipe::Synth(class_gunshot(0.4, 70.0, 26.0, 0x6a00_0003)),
        Cue::GunshotShotgun => Recipe::Synth(class_gunshot(0.55, 55.0, 16.0, 0x6a00_0004)),
        Cue::GunshotLmg => Recipe::Synth(class_gunshot(0.5, 60.0, 22.0, 0x6a00_0005)),
        Cue::GunshotSniper => Recipe::Synth(class_gunshot(0.8, 48.0, 14.0, 0x6a00_0006)),
        Cue::GunshotLauncher => Recipe::Synth(class_gunshot(0.9, 38.0, 10.0, 0x6a00_0007)),
        Cue::PapShotLayer => Recipe::Synth(sweep(0.25, 2200.0, 300.0, 0.3)),
        Cue::RayGun => Recipe::Synth(ray_gun()),
        Cue::Thundergun => Recipe::Synth(thundergun()),
        Cue::Flamethrower => Recipe::Synth(roar(1.5, 0.25, 0x6a00_0010)),
        Cue::Explosion => Recipe::Synth(boom(2.5, 45.0, 0x6a00_0011)),
        Cue::Reload => Recipe::Synth(reload()),
        Cue::EmptyClick => Recipe::Synth(tick(0.04, 3200.0)),
        Cue::WeaponSwap => Recipe::Synth(weapon_swap()),
        Cue::KnifeSwing => Recipe::Synth(swish(0.25, 0x6a00_0012)),
        Cue::KnifeHit => Recipe::Synth(wet_thud(0.2, 110.0, 0x6a00_0013)),
        Cue::Headshot => Recipe::Synth(tick(0.09, 1800.0)),
        Cue::PlayerHurt => Recipe::Synth(thud(0.25, 80.0)),
        Cue::LowHealth => Recipe::Synth(heartbeat()),
        Cue::Down => Recipe::Synth(down()),
        Cue::SelfRevive => Recipe::Synth(rising_whoosh(1.0, 0x6a00_0014)),
        Cue::MonkeyBomb => Recipe::Synth(monkey_march()),
        Cue::ClaymorePlant => Recipe::Synth(claymore_plant()),
        Cue::Footstep => Recipe::Synth(footstep()),
        Cue::StanceRustle => Recipe::Synth(rustle()),
        Cue::DiveLand => Recipe::Synth(dive_land()),
        Cue::CrawlDrag => Recipe::Synth(crawl_drag()),
        Cue::DoorOpen => Recipe::Synth(door_open()),
        Cue::PowerOn => Recipe::Synth(power_on()),
        Cue::PerkJingleJuggernog => Recipe::Synth(chiptune(&[48, 0, 48, 51, 55, 0, 53, 51, 50, 0, 50, 53, 51, 0, 48, 0], 0.3)),
        Cue::PerkJingleSpeedCola => Recipe::Synth(chiptune(&[72, 74, 76, 79, 76, 74, 72, 0, 74, 76, 79, 81, 84, 0, 79, 0, 84, 0], 0.24)),
        Cue::PerkJingleDoubleTap => Recipe::Synth(chiptune(&[57, 57, 0, 60, 60, 0, 64, 64, 0, 62, 60, 57, 0, 55, 57, 0], 0.28)),
        Cue::PerkJingleQuickRevive => Recipe::Synth(chiptune(&[67, 71, 74, 79, 0, 74, 79, 83, 0, 79, 76, 72, 74, 0, 67, 0], 0.3)),
        Cue::PerkDrink => Recipe::Synth(perk_drink()),
        Cue::PerkMachineLeave => Recipe::Synth(rising_whoosh(1.6, 0x6a00_0015)),
        Cue::BoxSpin => Recipe::Synth(music_box(2.85, 0.19)),
        Cue::BoxTeddy => Recipe::Synth(laugh()),
        Cue::BoxMove => Recipe::Synth(box_move()),
        Cue::FireSale => Recipe::Synth(music_box(2.4, 0.15)),
        Cue::PapProcess => Recipe::Synth(churn()),
        Cue::PapReady => Recipe::Synth(chime(&[2093.0], 1.0, 4.0)),
        Cue::PowerupHum => Recipe::Synth(warble()),
        Cue::PowerupPickup => Recipe::Synth(chime(&[1567.98, 1975.5, 2349.3, 3136.0], 1.2, 3.0)),
        Cue::NukeBlast => Recipe::Synth(nuke_blast()),
        Cue::TrapActive => Recipe::Synth(electric_crackle(1.5, 0x6a00_0016)),
        Cue::TrapKill => Recipe::Synth(zap(0.3, 0x6a00_0017)),
        Cue::TeleporterCharge => Recipe::Synth(sweep(2.0, 200.0, 1600.0, 0.3)),
        Cue::TeleporterFire => Recipe::Synth(boom(1.5, 55.0, 0x6a00_0018)),
        Cue::TeleporterArrive => Recipe::Synth(reversed(boom(1.5, 55.0, 0x6a00_0019))),
        Cue::MainframeLink => Recipe::Synth(mainframe_link()),
        Cue::AmmoModBlastFurnace => Recipe::Synth(ignite()),
        Cue::ProcFrost => Recipe::Synth(frost()),
        Cue::ProcBlast => Recipe::Synth(scaled(boom(0.6, 90.0, 0xb1a5_7001), 0.9)),
        Cue::AmmoModDeadWire => Recipe::Synth(zap_chain()),
        Cue::AmmoModFireworks => Recipe::Synth(pops()),
        Cue::AmmoModThunderWall => Recipe::Synth(gust()),
        Cue::AmmoModTurned => Recipe::Synth(reversed(chime(&[220.0, 261.6, 311.1, 370.0], 1.5, 2.0))),
        Cue::PowerDown => Recipe::Synth(power_down()),
        Cue::PowerRestore => Recipe::Synth(power_restore()),
        Cue::GeneratorHeld => Recipe::Synth(generator_held()),
        Cue::GeneratorLost => Recipe::Synth(generator_lost()),
        Cue::ZombieWalkerIdle => Recipe::Synth(moan(1.8, 95.0, 0x6a00_0020)),
        Cue::ZombieJoggerIdle => Recipe::Synth(grunts()),
        Cue::ZombieSprinterIdle => Recipe::Synth(shriek(0.6, 900.0, 1300.0, 0x6a00_0021)),
        Cue::ZombieAttack => Recipe::Synth(snarl(0.4, 0x6a00_0022)),
        Cue::ZombieDeath => Recipe::Synth(gurgle()),
        Cue::ZombieRiser => Recipe::Synth(dirt_crunch()),
        Cue::CrawlerGib => Recipe::Synth(splatter()),
        Cue::CrawlerVocal => Recipe::Synth(moan(1.6, 70.0, 0x6a00_0023)),
        Cue::CrawlerDrag => Recipe::Synth(drag()),
        Cue::HoundIdle => Recipe::Synth(snarl(1.2, 0x6a00_0024)),
        Cue::NovaVocal => Recipe::Synth(chitter()),
        Cue::NapalmIdle => Recipe::Synth(napalm_idle()),
        Cue::NapalmHeat => Recipe::Synth(roar(2.0, 0.6, 0x6a00_0025)),
        Cue::FirePool => Recipe::Synth(crackle(2.0, 0x6a00_0026)),
        Cue::ScreamHit => Recipe::Synth(tinnitus()),
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
    let mut seed = 0x07e1_1d06;
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

/// A high ear-ringing tone over a dull, muffled thump, fading slowly.
fn ear_ring() -> Vec<f32> {
    let mut seed = 0x5e11_1000;
    samples(2.5)
        .map(|t| {
            let ring = (TAU * 3_100.0 * t).sin() * (-t * 0.9).exp() * 0.25;
            let thump = (TAU * 45.0 * t).sin() * (-t * 5.0).exp() * 0.6;
            let rumble = noise(&mut seed) * (-t * 2.0).exp() * 0.08;
            (ring + thump + rumble).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A metal bang, then a scraping rattle as something drops out of a duct.
fn vent_bang() -> Vec<f32> {
    let mut seed = 0x7e47_0000;
    samples(1.0)
        .map(|t| {
            let bang = ((TAU * 410.0 * t).sin() + 0.6 * (TAU * 1_130.0 * t).sin()) * (-t * 12.0).exp() * 0.5;
            let scrape = noise(&mut seed) * (TAU * 23.0 * t).sin().abs() * (-t * 2.5).exp() * 0.3;
            (bang + scrape).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A short, rising screech.
fn screech() -> Vec<f32> {
    let mut seed = 0x5c4e_0000;
    let secs = 0.3;
    samples(secs)
        .map(|t| {
            let freq = 1_400.0 + 900.0 * t / secs;
            let tone = (TAU * freq * t).sin() * (1.0 + 0.4 * noise(&mut seed));
            tone * (t * 50.0).min(1.0) * (-t * 7.0).exp() * 0.35
        })
        .collect()
}

/// A wet pop, then a hiss of escaping gas.
fn wet_pop() -> Vec<f32> {
    let mut seed = 0x9a50_0000;
    samples(1.2)
        .map(|t| {
            let pop = (TAU * (180.0 - 100.0 * t.min(0.1) * 10.0) * t).sin() * (-t * 25.0).exp() * 0.8;
            let splat = noise(&mut seed) * (-t * 30.0).exp() * 0.4;
            let hiss = noise(&mut seed) * (t * 8.0).min(1.0) * (-t * 2.0).exp() * 0.3;
            (pop + splat + hiss).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A steady low hiss that loops, for a gas cloud.
fn gas_hiss() -> Vec<f32> {
    let mut seed = 0x6a50_0000;
    let mut low = 0.0;
    samples(2.0)
        .map(|t| {
            // Smoothed noise for a darker hiss, with a slow swell.
            low += (noise(&mut seed) - low) * 0.3;
            low * 0.35 * (0.85 + 0.15 * (TAU * 0.5 * t).sin())
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

/// A ground rumble swelling into a roaring flare, for a Napalm rising.
fn rumble_flare() -> Vec<f32> {
    let mut seed = 0x0a7a_1000;
    let secs = 1.5;
    samples(secs)
        .map(|t| {
            let rumble = (TAU * 45.0 * t).sin() * (1.0 + 0.5 * noise(&mut seed)) * 0.35;
            let flare = noise(&mut seed) * (t / secs).powi(2) * 0.6;
            ((rumble + flare) * (1.0 - (t / secs).powi(8))).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A breathy whoosh rising in pitch over `secs`, for a wind-up tell.
fn rising_whoosh(secs: f32, mut seed: u32) -> Vec<f32> {
    let mut low = 0.0;
    samples(secs)
        .map(|t| {
            let p = t / secs;
            // A one-pole filter that opens up as it rises.
            let k = 0.05 + 0.6 * p;
            low += (noise(&mut seed) - low) * k;
            let tone = (TAU * (300.0 + 900.0 * p) * t).sin() * 0.15;
            ((low * 0.7 + tone) * p.sqrt()).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A deep fireball boom with a long, crackling tail.
fn fireball_boom() -> Vec<f32> {
    let mut seed = 0x0a7a_1002;
    samples(2.0)
        .map(|t| {
            let boom = (TAU * 50.0 * t).sin() * (-t * 3.0).exp() * 0.7;
            let blast = noise(&mut seed) * (-t * 6.0).exp() * 0.6;
            let crackle = if noise(&mut seed) > 0.97 { noise(&mut seed) * 0.5 } else { 0.0 } * (-t * 1.2).exp();
            (boom + blast + crackle).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A short hissing sizzle, for the Player burning.
fn sizzle() -> Vec<f32> {
    let mut seed = 0x0a7a_1003;
    samples(0.4)
        .map(|t| noise(&mut seed) * (-t * 8.0).exp() * 0.35 * (1.0 + (TAU * 90.0 * t).sin()) / 2.0)
        .collect()
}

/// A distorted shriek gliding from `from` Hz to `to` Hz over `secs`.
fn shriek(secs: f32, from: f32, to: f32, mut seed: u32) -> Vec<f32> {
    let mut phase = 0.0;
    samples(secs)
        .map(|t| {
            let p = t / secs;
            let freq = from + (to - from) * p;
            phase += TAU * freq / SAMPLE_RATE as f32;
            let tone = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * noise(&mut seed);
            let envelope = (t * 40.0).min(1.0) * (1.0 - p).sqrt();
            (tone * 1.6).clamp(-1.0, 1.0) * envelope * 0.4
        })
        .collect()
}

/// One huge splintering crash, for the Sapper tearing every Plank off at once.
fn sapper_smash() -> Vec<f32> {
    let mut seed = 0x5a99_0002;
    samples(1.0)
        .map(|t| {
            let splinter = noise(&mut seed) * (-t * 5.0).exp() * (1.0 + (TAU * 23.0 * t).sin());
            let boom = (TAU * 55.0 * t).sin() * (-t * 4.0).exp();
            (splinter * 0.55 + boom * 0.6).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Laboured breathing over clanking gear, looped while the Sapper lives.
fn sapper_idle() -> Vec<f32> {
    let mut seed = 0x5a99_0003;
    let secs = 2.4;
    samples(secs)
        .map(|t| {
            // In and out, once each over the loop.
            let breath = (TAU * t / secs).sin().abs() * noise(&mut seed) * 0.25;
            let clank = if (t * 2.5).fract() < 0.03 { (TAU * 900.0 * t).sin() * 0.3 } else { 0.0 };
            breath + clank
        })
        .collect()
}

/// A soft boot on concrete: a short low thud with a scuff of grit.
fn footstep() -> Vec<f32> {
    let mut seed = 0x6a00_0030;
    samples(0.18)
        .map(|t| {
            let thud = (TAU * 95.0 * t).sin() * (-t * 28.0).exp() * 0.6;
            let scuff = noise(&mut seed) * (-t * 40.0).exp() * 0.25;
            (thud + scuff).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Cloth and gear shifting: a soft swell of band-limited noise with a little scuff in it.
fn rustle() -> Vec<f32> {
    let mut seed = 0x57a5_0001;
    let secs = 0.3;
    let (mut low, mut lower) = (0.0, 0.0);
    samples(secs)
        .map(|t| {
            let raw = noise(&mut seed);
            low += (raw - low) * 0.35;
            lower += (low - lower) * 0.35;
            // A highpassed hiss: the cloth, not the rumble under it.
            let cloth = (low - lower) * 2.2;
            let swell = (PI * t / secs).sin().powf(0.6);
            let scuff = (TAU * 18.0 * t).sin().abs() * 0.5 + 0.5;
            (cloth * swell * scuff * 0.7).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A body hitting the floor: a heavy low thump, a slap of cloth and kit, and a short settle.
fn dive_land() -> Vec<f32> {
    let mut seed = 0xd1e0_0001;
    let secs = 0.55;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            let thump = (TAU * (70.0 - 30.0 * t) * t).sin() * (-t * 11.0).exp() * 0.85;
            let slap = noise(&mut seed) * (-t * 35.0).exp() * 0.4;
            thump + slap
        })
        .collect();
    layer(&mut out, 0.12, &scaled(rustle(), 0.5));
    out.truncate((secs * SAMPLE_RATE as f32) as usize);
    clamped(out)
}

/// One pull of a Crawl: a short scrape of cloth over the floor, then the soft plant of an elbow.
fn crawl_drag() -> Vec<f32> {
    let mut seed = 0xc4a1_0001;
    let mut low = 0.0;
    let secs = 0.4;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.12;
            let swell = if t < 0.28 { (t / 0.28) * (1.0 - t / 0.28 * 0.4) } else { 0.0 };
            low * swell * 0.9
        })
        .collect();
    layer(&mut out, 0.26, &scaled(footstep(), 0.5));
    out.truncate((secs * SAMPLE_RATE as f32) as usize);
    clamped(out)
}

/// A deep footstep thud with a metal jangle.
fn sapper_step() -> Vec<f32> {
    let mut seed = 0x5a99_0004;
    samples(0.4)
        .map(|t| {
            let thud = (TAU * 60.0 * t).sin() * (-t * 14.0).exp() * 0.8;
            let jangle = (TAU * 1300.0 * t).sin() * noise(&mut seed).abs() * (-t * 9.0).exp() * 0.2;
            (thud + jangle).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Tearing cables and sparks over a buzz, looped for the 3 s Sabotage.
fn sapper_sabotage() -> Vec<f32> {
    let mut seed = 0x5a99_0005;
    samples(1.5)
        .map(|t| {
            let buzz = ((TAU * 120.0 * t).sin() * 3.0).clamp(-1.0, 1.0) * 0.15;
            let spark = if noise(&mut seed) > 0.96 { noise(&mut seed) * 0.6 } else { 0.0 };
            let tear = noise(&mut seed) * 0.15 * (1.0 + (TAU * 3.0 * t).sin()) / 2.0;
            (buzz + spark + tear).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A metallic clang with a ricochet ring, for the Sapper's helmet coming off.
fn clang() -> Vec<f32> {
    samples(0.9)
        .map(|t| {
            let ring = (TAU * 1450.0 * t).sin() * 0.4 + (TAU * 2210.0 * t).sin() * 0.25;
            let ricochet = (TAU * (2600.0 - 1400.0 * t) * t).sin() * 0.2;
            (ring + ricochet) * (-t * 5.0).exp()
        })
        .collect()
}

/// Ragged, high chattering gasps, for a Shrieker's idle.
fn gasps() -> Vec<f32> {
    let mut seed = 0x5e1e_0005;
    samples(0.7)
        .map(|t| {
            let chatter = ((TAU * 9.0 * t).sin().max(0.0)).powi(2);
            let breath = noise(&mut seed) * 0.4 + (TAU * 900.0 * t).sin() * 0.1;
            breath * chatter * (1.0 - t / 0.7)
        })
        .collect()
}

/// Mixes `part` into `into`, starting `at` seconds in, lengthening `into` to fit.
fn layer(into: &mut Vec<f32>, at: f32, part: &[f32]) {
    let start = (at * SAMPLE_RATE as f32) as usize;
    if into.len() < start + part.len() {
        into.resize(start + part.len(), 0.0);
    }
    for (sample, p) in into[start..].iter_mut().zip(part) {
        *sample += p;
    }
}

/// Clamps every sample into `[-1, 1]`.
fn clamped(samples: Vec<f32>) -> Vec<f32> {
    samples.into_iter().map(|s| s.clamp(-1.0, 1.0)).collect()
}

fn reversed(mut samples: Vec<f32>) -> Vec<f32> {
    samples.reverse();
    samples
}

fn scaled(samples: Vec<f32>, by: f32) -> Vec<f32> {
    samples.into_iter().map(|s| s * by).collect()
}

/// A tone gliding from `from` Hz to `to` Hz over `secs`, fading out as it goes.
fn sweep(secs: f32, from: f32, to: f32, amp: f32) -> Vec<f32> {
    let mut phase = 0.0;
    samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (from + (to - from) * p) / SAMPLE_RATE as f32;
            let tone = phase.sin() + 0.3 * (phase * 2.0).sin();
            tone * (t * 200.0).min(1.0) * (1.0 - p) * amp
        })
        .collect()
}

/// A short, dry tick at `freq`.
fn tick(secs: f32, freq: f32) -> Vec<f32> {
    let mut seed = 0x71c4_0001;
    samples(secs)
        .map(|t| ((TAU * freq * t).sin() * 0.6 + noise(&mut seed) * 0.2) * (-t * 6.0 / secs).exp())
        .collect()
}

/// Bell-like notes struck 0.12 s apart, each ringing down at `decay` a second.
fn chime(freqs: &[f32], secs: f32, decay: f32) -> Vec<f32> {
    samples(secs)
        .map(|t| {
            freqs
                .iter()
                .enumerate()
                .filter(|(i, _)| t >= *i as f32 * 0.12)
                .map(|(i, freq)| {
                    let local = t - i as f32 * 0.12;
                    let tone = (TAU * freq * local).sin() + 0.4 * (TAU * freq * 2.76 * local).sin();
                    tone * (local * 400.0).min(1.0) * (-local * decay).exp() * 0.25
                })
                .sum::<f32>()
                .clamp(-1.0, 1.0)
        })
        .collect()
}

/// A low, dissonant bell tolling once.
fn bell_toll() -> Vec<f32> {
    const PARTIALS: [(f32, f32); 5] = [(110.0, 1.0), (116.5, 0.8), (262.0, 0.5), (311.0, 0.35), (523.0, 0.2)];
    samples(3.0)
        .map(|t| {
            let tone: f32 = PARTIALS.iter().map(|(f, a)| (TAU * f * t).sin() * a).sum();
            tone * (t * 300.0).min(1.0) * (-t * 1.3).exp() * 0.22
        })
        .collect()
}

/// A long, descending low drone.
fn game_over() -> Vec<f32> {
    let secs = 5.0;
    let mut phase = 0.0;
    samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (110.0 - 55.0 * p) / SAMPLE_RATE as f32;
            let tone = phase.sin() + (phase * 3.0).sin() / 3.0 + (phase * 5.0).sin() / 6.0;
            tone * (t / 0.8).min(1.0) * (1.0 - p).powf(1.5) * 0.35
        })
        .collect()
}

/// A gunshot: a noise crack dying at `crack_rate` over a low boom at `boom` Hz, `secs` long.
/// Further up the classes it's longer and lower.
fn class_gunshot(secs: f32, boom: f32, crack_rate: f32, mut seed: u32) -> Vec<f32> {
    let mut low = 0.0;
    samples(secs)
        .map(|t| {
            let crack = noise(&mut seed) * (-t * crack_rate).exp();
            low += (noise(&mut seed) - low) * 0.1;
            let body = (TAU * boom * t).sin() * 0.6 + low * 0.8;
            (crack * 0.8 + body * (-t * 5.0 / secs).exp()).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A bright, buzzy laser chirp.
fn ray_gun() -> Vec<f32> {
    let secs = 0.25;
    let mut phase = 0.0;
    samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (2500.0 - 1600.0 * p) / SAMPLE_RATE as f32;
            let square = if phase.sin() >= 0.0 { 1.0 } else { -1.0 };
            (square * 0.25 + phase.sin() * 0.2) * (1.0 - p)
        })
        .collect()
}

/// A huge whoosh into a boom.
fn thundergun() -> Vec<f32> {
    let mut out = rising_whoosh(0.35, 0x7d00_0001);
    layer(&mut out, 0.3, &boom(1.4, 40.0, 0x7d00_0002));
    clamped(out)
}

/// Roaring filtered noise at a steady level, so it loops: a dark roar with `hiss` of bright noise.
fn roar(secs: f32, hiss: f32, mut seed: u32) -> Vec<f32> {
    let mut low = 0.0;
    samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.08;
            // Two whole swells per loop, so its ends meet.
            let swell = 0.85 + 0.15 * (TAU * 2.0 * t / secs).sin();
            ((low * 1.6 + noise(&mut seed) * hiss * 0.3) * swell * 0.45).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A broadband boom at `freq` Hz with a long, rumbling tail.
fn boom(secs: f32, freq: f32, mut seed: u32) -> Vec<f32> {
    let mut low = 0.0;
    samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.05;
            let thump = (TAU * freq * t).sin() * (-t * 3.0).exp() * 0.6;
            let blast = noise(&mut seed) * (-t * 7.0).exp() * 0.5;
            let tail = low * (-t * 4.0 / secs).exp() * 1.2;
            (thump + blast + tail).clamp(-1.0, 1.0)
        })
        .collect()
}

/// One metallic clack.
fn clack(mut seed: u32) -> Vec<f32> {
    samples(0.12)
        .map(|t| {
            let ring = (TAU * 2200.0 * t).sin() * 0.4 + (TAU * 3400.0 * t).sin() * 0.25;
            (ring * (-t * 50.0).exp() + noise(&mut seed) * (-t * 90.0).exp() * 0.5).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Clack-clack: the magazine out, and back in.
fn reload() -> Vec<f32> {
    let mut out = clack(0x7e10_0001);
    layer(&mut out, 0.35, &clack(0x7e10_0002));
    clamped(out)
}

/// A cloth-and-metal shuffle.
fn weapon_swap() -> Vec<f32> {
    let mut seed = 0x5a40_0001;
    let mut low = 0.0;
    let mut out: Vec<f32> = samples(0.3)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.15;
            low * (PI * t / 0.3).sin() * 0.6
        })
        .collect();
    layer(&mut out, 0.25, &clack(0x5a40_0002));
    clamped(out)
}

/// An airy swish: band-limited noise swelling and fading.
fn swish(secs: f32, mut seed: u32) -> Vec<f32> {
    let (mut slow, mut fast) = (0.0, 0.0);
    samples(secs)
        .map(|t| {
            let n = noise(&mut seed);
            slow += (n - slow) * 0.05;
            fast += (n - fast) * 0.5;
            (fast - slow) * (PI * t / secs).sin().powi(2) * 0.9
        })
        .collect()
}

/// A wet thud: a low knock with a squelch of dark noise.
fn wet_thud(secs: f32, freq: f32, mut seed: u32) -> Vec<f32> {
    let mut low = 0.0;
    samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.2;
            let knock = (TAU * freq * t).sin() * 0.6;
            ((knock + low * 0.9) * (-t * 6.0 / secs).exp()).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A heartbeat, lub-dub, looping a little faster than once a second.
fn heartbeat() -> Vec<f32> {
    let beat = |t: f32, start: f32, amp: f32| {
        let local = t - start;
        if local < 0.0 { 0.0 } else { (TAU * 55.0 * local).sin() * (-local * 22.0).exp() * amp }
    };
    samples(0.9).map(|t| beat(t, 0.0, 0.8) + beat(t, 0.22, 0.55)).collect()
}

/// A heavy thud with a muffled ring after it.
fn down() -> Vec<f32> {
    let mut out = thud(0.6, 50.0);
    let ring: Vec<f32> = samples(1.8)
        .map(|t| (TAU * 1200.0 * t).sin() * (t * 10.0).min(1.0) * (-t * 1.5).exp() * 0.12)
        .collect();
    layer(&mut out, 0.05, &ring);
    clamped(out)
}

/// A tinny cymbal-clash march, looping.
fn monkey_march() -> Vec<f32> {
    let mut seed = 0x3a7c_0001;
    let mut low = 0.0;
    samples(2.0)
        .map(|t| {
            let beat = (t / 0.25).fract() * 0.25;
            let n = noise(&mut seed);
            low += (n - low) * 0.3;
            // A clash on every other beat, a lighter one between.
            let loud = if ((t / 0.25) as u32).is_multiple_of(2) { 0.5 } else { 0.2 };
            let cymbal = (n - low) * (-beat * 18.0).exp() * loud;
            let tin = (TAU * 1800.0 * t).sin() * (-beat * 30.0).exp() * 0.15;
            (cymbal + tin).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A click, then a beep.
fn claymore_plant() -> Vec<f32> {
    let mut out = tick(0.03, 2500.0);
    let beep: Vec<f32> = samples(0.12).map(|t| (TAU * 1500.0 * t).sin() * 0.3 * (1.0 - t / 0.12)).collect();
    layer(&mut out, 0.12, &beep);
    out
}

/// A heavy rumble and slide, ending in a thump.
fn door_open() -> Vec<f32> {
    let mut seed = 0xd00e_0001;
    let mut low = 0.0;
    let secs = 1.6;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.04;
            let rumble = (TAU * 40.0 * t).sin() * 0.3 + low * 1.5;
            rumble * (t * 8.0).min(1.0) * (1.0 - t / secs)
        })
        .collect();
    layer(&mut out, 1.3, &thud(0.3, 60.0));
    clamped(out)
}

/// A big clunk, then an electrical hum rising.
fn power_on() -> Vec<f32> {
    let mut out = thud(0.35, 70.0);
    let mut phase = 0.0;
    let secs = 2.4;
    let hum: Vec<f32> = samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (60.0 + 60.0 * p) / SAMPLE_RATE as f32;
            let tone = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.3 * (phase * 3.0).sin();
            tone * p.sqrt() * (1.0 - p.powi(6)) * 0.25
        })
        .collect();
    layer(&mut out, 0.3, &hum);
    clamped(out)
}

/// A chiptune ditty: square-wave MIDI notes (0 is a rest), `step` seconds each.
fn chiptune(notes: &[u8], step: f32) -> Vec<f32> {
    let secs = notes.len() as f32 * step + 0.3;
    samples(secs)
        .map(|t| {
            let i = (t / step) as usize;
            let note = notes.get(i).copied().unwrap_or(0);
            if note == 0 {
                return 0.0;
            }
            let local = t - i as f32 * step;
            let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
            let square = if (TAU * freq * local).sin() >= 0.0 { 1.0 } else { -1.0 };
            square * (local * 300.0).min(1.0) * (-local * 4.0).exp() * 0.22
        })
        .collect()
}

/// Glugs, then a glass clink, over the 2.8 s drink.
fn perk_drink() -> Vec<f32> {
    let mut out = vec![0.0; (2.8 * SAMPLE_RATE as f32) as usize];
    for start in [0.3, 0.75, 1.2, 1.65, 2.05] {
        layer(&mut out, start, &sweep(0.18, 200.0, 110.0, 0.6));
    }
    layer(&mut out, 2.45, &chime(&[3500.0], 0.35, 14.0));
    clamped(out)
}

/// A music-box tune, a tine every `step` seconds for `secs`.
fn music_box(secs: f32, step: f32) -> Vec<f32> {
    const TUNE: [f32; 12] = [1318.5, 1568.0, 1975.5, 1760.0, 1568.0, 1318.5, 1174.7, 1318.5, 1568.0, 1174.7, 1046.5, 1174.7];
    samples(secs)
        .map(|t| {
            let i = (t / step) as usize;
            let local = t - i as f32 * step;
            let freq = TUNE[i % TUNE.len()];
            let tine = (TAU * freq * local).sin() + 0.3 * (TAU * freq * 3.0 * local).sin();
            tine * (local * 500.0).min(1.0) * (-local * 6.0).exp() * 0.3
        })
        .collect()
}

/// A low, mocking "ha, ha, ha, ha".
fn laugh() -> Vec<f32> {
    let mut seed = 0x7edd_0001;
    samples(1.5)
        .map(|t| {
            let i = (t / 0.3) as u32;
            let local = t - i as f32 * 0.3;
            if i >= 4 || local > 0.2 {
                return 0.0;
            }
            let freq = 130.0 - 12.0 * i as f32 - 60.0 * local;
            // A buzzy voice: a sawtooth with a breathy rasp.
            let saw = 2.0 * (freq * t).fract() - 1.0;
            let voice = saw * 0.4 + noise(&mut seed) * 0.15;
            voice * (PI * local / 0.2).sin()
        })
        .collect()
}

/// A rising whoosh, then a thud on landing.
fn box_move() -> Vec<f32> {
    let mut out = rising_whoosh(1.2, 0xb0c5_0001);
    layer(&mut out, 1.2, &thud(0.4, 60.0));
    clamped(out)
}

/// A grinding, churning machine.
fn churn() -> Vec<f32> {
    let mut seed = 0x9a90_0001;
    let mut low = 0.0;
    let secs = 3.0;
    samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.12;
            let grind = low * (TAU * 12.0 * t).sin().abs() * 1.4;
            let motor = (TAU * (70.0 + 6.0 * (TAU * 2.0 * t).sin()) * t).sin() * 0.25;
            ((grind + motor) * (t * 10.0).min(1.0) * (1.0 - (t / secs).powi(8))).clamp(-1.0, 1.0)
        })
        .collect()
}

/// An eerie warbling hum, looping.
fn warble() -> Vec<f32> {
    let mut phase = 0.0;
    samples(2.0)
        .map(|t| {
            phase += TAU * (330.0 + 20.0 * (TAU * 6.0 * t).sin()) / SAMPLE_RATE as f32;
            let tremolo = 0.8 + 0.2 * (TAU * 1.5 * t).sin();
            (phase.sin() + 0.4 * (phase * 1.5).sin()) * tremolo * 0.22
        })
        .collect()
}

/// A flat, huge white-noise blast.
fn nuke_blast() -> Vec<f32> {
    let mut seed = 0x0c1e_0001;
    samples(3.0)
        .map(|t| {
            let blast = noise(&mut seed) * (t * 50.0).min(1.0) * (-t * 1.2).exp() * 0.8;
            let thump = (TAU * 35.0 * t).sin() * (-t * 2.0).exp() * 0.4;
            (blast + thump).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A buzzing electrical crackle at a steady level, looping.
fn electric_crackle(secs: f32, mut seed: u32) -> Vec<f32> {
    samples(secs)
        .map(|t| {
            let buzz = ((TAU * 100.0 * t).sin() * 4.0).clamp(-1.0, 1.0) * 0.15;
            let spark = if noise(&mut seed) > 0.94 { noise(&mut seed) * 0.6 } else { 0.0 };
            (buzz + spark).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A sharp zap: a falling tone over a crack.
fn zap(secs: f32, mut seed: u32) -> Vec<f32> {
    let crack: Vec<f32> = samples(secs).map(|t| noise(&mut seed) * (-t * 25.0).exp() * 0.4).collect();
    let mut out = sweep(secs, 3000.0, 500.0, 0.35);
    layer(&mut out, 0.0, &crack);
    clamped(out)
}

/// A two-tone confirm over a crackle.
fn mainframe_link() -> Vec<f32> {
    let mut out = chime(&[880.0, 1318.5], 0.8, 6.0);
    layer(&mut out, 0.0, &scaled(electric_crackle(0.6, 0x3a1f_0001), 0.6));
    clamped(out)
}

/// A whoosh as something catches fire, then the flames dying down.
fn ignite() -> Vec<f32> {
    let mut out = rising_whoosh(0.35, 0xb1a5_0001);
    let flames: Vec<f32> = crackle(1.0, 0xb1a5_0002)
        .into_iter()
        .enumerate()
        .map(|(i, s)| s * (1.0 - i as f32 / SAMPLE_RATE as f32))
        .collect();
    layer(&mut out, 0.3, &flames);
    clamped(out)
}

/// A glassy shimmer as Frost takes hold: a high chime over a short hiss.
fn frost() -> Vec<f32> {
    let mut out = chime(&[1760.0, 2217.5, 2637.0], 0.6, 6.0);
    let mut seed = 0xf205_7001;
    let hiss: Vec<f32> = samples(0.3).map(|t| noise(&mut seed) * 0.15 * (1.0 - t / 0.3)).collect();
    layer(&mut out, 0.0, &hiss);
    clamped(out)
}

/// A chain of zaps jumping from Zombie to Zombie.
fn zap_chain() -> Vec<f32> {
    let mut out = Vec::new();
    for i in 0..3 {
        layer(&mut out, i as f32 * 0.12, &zap(0.25, 0xdead_0001 + i));
    }
    clamped(out)
}

/// Crackling pops, like fireworks.
fn pops() -> Vec<f32> {
    let mut seed = 0xf1e0_0001;
    let mut out = vec![0.0; SAMPLE_RATE as usize];
    for i in 0..10 {
        let at = i as f32 * 0.09 + (noise(&mut seed) + 1.0) * 0.03;
        let pop: Vec<f32> = samples(0.08).map(|t| noise(&mut seed) * (-t * 60.0).exp() * 0.7).collect();
        layer(&mut out, at, &pop);
    }
    clamped(out)
}

/// A deep gust of wind.
fn gust() -> Vec<f32> {
    let mut seed = 0x9057_0001;
    let mut low = 0.0;
    let secs = 1.2;
    samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.03;
            let body = low * 3.0 + (TAU * 40.0 * t).sin() * 0.2;
            (body * (PI * t / secs).sin()).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A descending electrical whine, then silence.
fn power_down() -> Vec<f32> {
    let mut out = sweep(1.4, 900.0, 80.0, 0.35);
    out.resize((1.8 * SAMPLE_RATE as f32) as usize, 0.0);
    out
}

/// An ascending hum and a click: a relief sting.
fn power_restore() -> Vec<f32> {
    let mut phase = 0.0;
    let secs = 1.2;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (80.0 + 520.0 * p) / SAMPLE_RATE as f32;
            (phase.sin() + 0.4 * (phase * 2.0).sin()) * p.sqrt() * 0.3
        })
        .collect();
    layer(&mut out, 1.2, &tick(0.04, 2000.0));
    layer(&mut out, 1.25, &chime(&[1046.5, 1568.0], 0.8, 5.0));
    clamped(out)
}

/// A generator spinning up to speed, then a relay's clunk and a two-note chime.
fn generator_held() -> Vec<f32> {
    let mut phase = 0.0;
    let secs = 1.8;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (40.0 + 80.0 * p.sqrt()) / SAMPLE_RATE as f32;
            let tone = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.25 * (phase * 4.0).sin();
            tone * p.sqrt() * (1.0 - p.powi(8)) * 0.3
        })
        .collect();
    layer(&mut out, 1.5, &thud(0.3, 90.0));
    layer(&mut out, 1.6, &chime(&[784.0, 1174.7], 0.9, 4.0));
    clamped(out)
}

/// A breaker's crack, then a generator winding down under a two-note chime: the opposite of
/// `generator_held`.
fn generator_lost() -> Vec<f32> {
    let mut phase = 0.0;
    let secs = 2.0;
    let mut out = zap(0.25, 0x6a00_0090);
    let wind_down: Vec<f32> = samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * (120.0 - 90.0 * p.sqrt()) / SAMPLE_RATE as f32;
            let tone = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.25 * (phase * 4.0).sin();
            tone * (1.0 - p).powi(2) * 0.3
        })
        .collect();
    layer(&mut out, 0.1, &wind_down);
    layer(&mut out, 0.3, &chime(&[1174.7, 587.3], 0.9, 4.0));
    clamped(out)
}

/// A moan around `base` Hz, swelling and fading over `secs`.
fn moan(secs: f32, base: f32, mut seed: u32) -> Vec<f32> {
    let mut phase = 0.0;
    samples(secs)
        .map(|t| {
            let p = t / secs;
            phase += TAU * base * (1.0 + 0.1 * (TAU * 3.0 * t).sin() - 0.15 * p) / SAMPLE_RATE as f32;
            let voice = phase.sin() + 0.5 * (phase * 2.0).sin() + 0.25 * (phase * 3.0).sin();
            let rasp = 1.0 + 0.5 * noise(&mut seed);
            voice * rasp * (PI * p).sin() * 0.3
        })
        .collect()
}

/// Three short grunts.
fn grunts() -> Vec<f32> {
    let mut out = Vec::new();
    for (i, at) in [0.0, 0.35, 0.65].into_iter().enumerate() {
        layer(&mut out, at, &moan(0.18, 130.0 - 10.0 * i as f32, 0x6a77_0001 + i as u32));
    }
    clamped(out)
}

/// A rasping snarl, `secs` long.
fn snarl(secs: f32, mut seed: u32) -> Vec<f32> {
    samples(secs)
        .map(|t| {
            let growl = (TAU * 140.0 * t).sin() * (1.0 + 0.8 * noise(&mut seed));
            growl * (t * 30.0).min(1.0) * (1.0 - t / secs) * 0.35
        })
        .collect()
}

/// A gurgling collapse: bubbling, then a body hitting the floor.
fn gurgle() -> Vec<f32> {
    let mut seed = 0x6a91_0001;
    let mut freq = 150.0;
    let mut phase = 0.0;
    let mut out: Vec<f32> = samples(0.8)
        .enumerate()
        .map(|(i, t)| {
            // A new bubble every 40 ms.
            if i % (SAMPLE_RATE as usize / 25) == 0 {
                freq = 120.0 + 80.0 * (noise(&mut seed) + 1.0);
            }
            phase += TAU * freq / SAMPLE_RATE as f32;
            phase.sin() * (1.0 - t / 0.8) * 0.35
        })
        .collect();
    layer(&mut out, 0.7, &thud(0.35, 70.0));
    clamped(out)
}

/// Dirt crunching as something digs its way up.
fn dirt_crunch() -> Vec<f32> {
    let mut seed = 0xd127_0001;
    let mut low = 0.0;
    let secs = 0.8;
    samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.25;
            let grains = (TAU * 17.0 * t).sin().max(0.0).powi(4);
            (low * grains * 1.5 * (1.0 - t / secs)).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A wet crunch and splatter.
fn splatter() -> Vec<f32> {
    let mut out = wet_thud(0.25, 90.0, 0x5b1a_0001);
    for (i, at) in [0.12, 0.2, 0.31, 0.4].into_iter().enumerate() {
        let drop = wet_thud(0.08, 160.0 + 30.0 * i as f32, 0x5b1a_0002 + i as u32);
        layer(&mut out, at, &scaled(drop, 0.5));
    }
    clamped(out)
}

/// A soft scrape, then the slap of a hand, looping.
fn drag() -> Vec<f32> {
    let mut seed = 0xd2a6_0001;
    let mut low = 0.0;
    let secs = 1.2;
    let mut out: Vec<f32> = samples(secs)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.1;
            if t < 0.7 { low * (t / 0.7) * 0.8 } else { 0.0 }
        })
        .collect();
    layer(&mut out, 0.7, &wet_thud(0.15, 120.0, 0xd2a6_0002));
    out.truncate((secs * SAMPLE_RATE as f32) as usize);
    clamped(out)
}

/// A wet, high chittering hiss.
fn chitter() -> Vec<f32> {
    let mut seed = 0xc417_0001;
    let secs = 0.8;
    samples(secs)
        .map(|t| {
            let clicks = (TAU * 25.0 * t).sin().max(0.0).powi(8);
            let chirp = (TAU * 2500.0 * t).sin() * clicks * 0.35;
            let hiss = noise(&mut seed) * 0.12;
            (chirp + hiss) * (PI * t / secs).sin()
        })
        .collect()
}

/// Crackling fire with a low, wheezing moan, looping.
fn napalm_idle() -> Vec<f32> {
    let secs = 2.4;
    let mut seed = 0x7a9a_0001;
    let mut low = 0.0;
    let fire = crackle(secs, 0x7a9a_0002);
    samples(secs)
        .zip(fire)
        .map(|(t, fire)| {
            low += (noise(&mut seed) - low) * 0.2;
            let wheeze = low * (TAU * t / secs).sin().abs() * 0.5;
            let moan = (TAU * 80.0 * t).sin() * 0.08;
            (fire + wheeze + moan).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A steady crackle of fire, looping.
fn crackle(secs: f32, mut seed: u32) -> Vec<f32> {
    let mut low = 0.0;
    samples(secs)
        .map(|_| {
            low += (noise(&mut seed) - low) * 0.05;
            let pop = if noise(&mut seed) > 0.985 { noise(&mut seed) * 0.6 } else { 0.0 };
            (low * 0.8 + pop).clamp(-1.0, 1.0)
        })
        .collect()
}

/// A high tinnitus ring over a muffled hush, looping.
fn tinnitus() -> Vec<f32> {
    let mut seed = 0x7177_0001;
    let mut low = 0.0;
    samples(1.0)
        .map(|t| {
            low += (noise(&mut seed) - low) * 0.02;
            (TAU * 3500.0 * t).sin() * 0.18 + low * 0.5
        })
        .collect()
}
