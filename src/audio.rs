//! Plays every cue from its slot in `assets/audio`. The placeholders there come from the
//! `gen-sounds` binary, and real recordings can replace them file by file.

pub mod cues;
pub mod slots;

use bevy::asset::io::file::FileAssetReader;
use bevy::audio::Volume;
use bevy::prelude::*;

use crate::barrier::{PlankRebuilt, PlankTorn};
use crate::buy::{BuyRefused, CantAfford, HoldPurchased, Purchased};
use crate::hellhound::{HellhoundBite, Telegraph};
use crate::player::PlayerCamera;
use crate::power_up::PowerUpTaken;
use crate::round::{Round, RoundStarted};
use crate::weapon::ShotFired;
use crate::zombie::{Dying, Zombie, ZombieHit, ZombieKilled};
use cues::{AUDIO_DIR, Cue, Mix};

/// Distance between the Player's ears, in metres, for spatial cues.
const EAR_GAP: f32 = 0.2;

struct Slot {
    variants: Vec<Handle<AudioSource>>,
    last: Option<usize>,
}

/// Each slot's loaded variants, in `Cue::ALL` order.
#[derive(Resource)]
struct Sounds(Vec<Slot>);

/// Each slot's files, found by the startup check, waiting to be loaded.
#[derive(Resource)]
struct SlotFiles(Vec<Vec<String>>);

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        let dir = FileAssetReader::get_base_path()
            .join("assets")
            .join(AUDIO_DIR);
        let stems: Vec<String> = Cue::ALL.iter().map(|cue| cue.stem()).collect();
        let files = slots::list_files(&dir);
        match slots::check_slots(stems.iter().map(String::as_str), &files) {
            Ok(found) => {
                app.insert_resource(SlotFiles(found));
            }
            Err(errors) => {
                eprintln!("error: bad audio slots in {}:", dir.display());
                for error in &errors {
                    eprintln!("  {error}");
                }
                eprintln!("Run `cargo gen-sounds` to write placeholders for missing slots.");
                std::process::exit(1);
            }
        }

        app.add_systems(Startup, load_sounds)
            .add_systems(Update, (add_listener, play_sounds, play_hound_sounds));
    }
}

fn load_sounds(mut commands: Commands, files: Res<SlotFiles>, assets: Res<AssetServer>) {
    let slots = files
        .0
        .iter()
        .map(|variants| Slot {
            variants: variants
                .iter()
                .map(|file| assets.load(format!("{AUDIO_DIR}/{file}")))
                .collect(),
            last: None,
        })
        .collect();
    commands.insert_resource(Sounds(slots));
    commands.remove_resource::<SlotFiles>();
}

fn add_listener(mut commands: Commands, cameras: Query<Entity, Added<PlayerCamera>>) {
    for camera in &cameras {
        commands
            .entity(camera)
            .insert(SpatialListener::new(EAR_GAP));
    }
}

impl Sounds {
    /// The variant of `cue` to play next.
    fn next(&mut self, cue: Cue) -> Handle<AudioSource> {
        let slot = &mut self.0[cue.index()];
        let pick = slots::pick_variant(slot.variants.len(), slot.last, rand::random());
        slot.last = Some(pick);
        slot.variants[pick].clone()
    }

    /// Plays `cue`. Spatial cues are heard from `at`; without it, they play flat.
    fn play(&mut self, commands: &mut Commands, cue: Cue, at: Option<Vec3>) {
        let player = AudioPlayer::new(self.next(cue));
        let spec = cue.spec();
        let settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(spec.gain));
        match (spec.mix, at) {
            (Mix::Spatial, Some(at)) => {
                commands.spawn((
                    player,
                    settings.with_spatial(true),
                    Transform::from_translation(at),
                ));
            }
            _ => {
                commands.spawn((player, settings));
            }
        }
    }
}

fn play_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut shots: MessageReader<ShotFired>,
    mut hits: MessageReader<ZombieHit>,
    mut kills: MessageReader<ZombieKilled>,
    mut rounds: MessageReader<RoundStarted>,
    mut purchases: MessageReader<Purchased>,
    mut holds: MessageReader<HoldPurchased>,
    mut cant_afford: MessageReader<CantAfford>,
    mut refusals: MessageReader<BuyRefused>,
    mut torn: MessageReader<PlankTorn>,
    mut rebuilt: MessageReader<PlankRebuilt>,
    mut power_ups: MessageReader<PowerUpTaken>,
    round: Option<Res<Round>>,
) {
    for _ in shots.read() {
        sounds.play(&mut commands, Cue::GunshotPistol, None);
    }
    for _ in hits.read() {
        sounds.play(&mut commands, Cue::Hit, None);
    }
    for _ in kills.read() {
        sounds.play(&mut commands, Cue::Kill, None);
    }
    for _ in rounds.read() {
        // A Hellhound Round opens with thunder and the announcer instead.
        if round.as_ref().is_some_and(|round| round.hellhound.is_some()) {
            sounds.play(&mut commands, Cue::HellhoundRoundStart, None);
            sounds.play(&mut commands, Cue::AnnouncerHellhoundRound, None);
        } else {
            sounds.play(&mut commands, Cue::RoundStart, None);
        }
    }
    for _ in 0..purchases.read().count() + holds.read().count() {
        sounds.play(&mut commands, Cue::Buy, None);
    }
    for plank in torn.read() {
        sounds.play(&mut commands, Cue::PlankTear, Some(plank.at));
    }
    for _ in rebuilt.read() {
        sounds.play(&mut commands, Cue::PlankRebuild, None);
    }
    for PowerUpTaken(power_up) in power_ups.read() {
        sounds.play(&mut commands, power_up.announcer(), None);
    }
    // Not affording a buy and a refused buy share the buzzer.
    for _ in 0..cant_afford.read().count() + refusals.read().count() {
        sounds.play(&mut commands, Cue::CantAfford, None);
    }
}

/// A hound's lightning strike, its bites and its death yelp.
fn play_hound_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    strikes: Query<&Transform, Added<Telegraph>>,
    deaths: Query<(&Zombie, &Transform), Added<Dying>>,
    mut bites: MessageReader<HellhoundBite>,
) {
    for at in &strikes {
        sounds.play(&mut commands, Cue::HoundSpawn, Some(at.translation));
    }
    for _ in bites.read() {
        sounds.play(&mut commands, Cue::HoundBite, None);
    }
    for (zombie, at) in &deaths {
        if zombie.is_hellhound() {
            sounds.play(&mut commands, Cue::HoundDeath, Some(at.translation));
        }
    }
}
