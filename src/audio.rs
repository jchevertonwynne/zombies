//! Placeholder sounds, synthesised at startup so there are no asset files yet.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::buy::{CantAfford, Purchased};
use crate::round::RoundStarted;
use crate::weapon::ShotFired;
use crate::zombie::{ZombieHit, ZombieKilled};

const SAMPLE_RATE: u32 = 22_050;

#[derive(Resource)]
struct Sounds {
    gunshot: Handle<AudioSource>,
    hit: Handle<AudioSource>,
    kill: Handle<AudioSource>,
    round_sting: Handle<AudioSource>,
    buzzer: Handle<AudioSource>,
    purchase: Handle<AudioSource>,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, synthesise)
            .add_systems(Update, play_sounds);
    }
}

fn synthesise(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut add = |samples: Vec<f32>| sources.add(AudioSource { bytes: wav(&samples).into() });
    commands.insert_resource(Sounds {
        gunshot: add(gunshot()),
        hit: add(thud(0.06, 220.0)),
        kill: add(thud(0.18, 90.0)),
        round_sting: add(round_sting()),
        buzzer: add(buzzer()),
        purchase: add(purchase()),
    });
}

fn play_sounds(
    mut commands: Commands,
    sounds: Res<Sounds>,
    mut shots: MessageReader<ShotFired>,
    mut hits: MessageReader<ZombieHit>,
    mut kills: MessageReader<ZombieKilled>,
    mut rounds: MessageReader<RoundStarted>,
    mut purchases: MessageReader<Purchased>,
    mut refusals: MessageReader<CantAfford>,
) {
    let mut play = |sound: &Handle<AudioSource>, volume: f32| {
        commands.spawn((
            AudioPlayer::new(sound.clone()),
            PlaybackSettings::DESPAWN.with_volume(bevy::audio::Volume::Linear(volume)),
        ));
    };
    for _ in shots.read() {
        play(&sounds.gunshot, 0.6);
    }
    for _ in hits.read() {
        play(&sounds.hit, 0.5);
    }
    for _ in kills.read() {
        play(&sounds.kill, 0.8);
    }
    for _ in rounds.read() {
        play(&sounds.round_sting, 0.9);
    }
    for _ in purchases.read() {
        play(&sounds.purchase, 0.7);
    }
    for _ in refusals.read() {
        play(&sounds.buzzer, 0.6);
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
fn round_sting() -> Vec<f32> {
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

/// Two bright rising notes for a purchase.
fn purchase() -> Vec<f32> {
    samples(0.4)
        .map(|t| {
            let (freq, local) = if t < 0.12 { (880.0, t) } else { (1320.0, t - 0.12) };
            let envelope = (local * 300.0).min(1.0) * (-local * 9.0).exp();
            (TAU * freq * local).sin() * envelope * 0.4
        })
        .collect()
}

/// Encodes mono samples as a 16-bit PCM WAV file.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}
