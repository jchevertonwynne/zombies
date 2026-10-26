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
use crate::nova::{GasCloud, NovaSwipe, VentDrop};
use crate::player::PlayerCamera;
use crate::power_up::PowerUpTaken;
use crate::round::{Round, RoundStarted};
use crate::shellshock::{ShellshockStarted, Shellshocked};
use crate::sapper::{Sabotaging, Sapper};
use crate::special::SpecialCue;
use crate::tuning::SHELLSHOCK_MUFFLE_GAIN;
use crate::weapon::ShotFired;
use crate::zombie::{Dying, Zombie, ZombieHit, ZombieKilled};
use cues::{AUDIO_DIR, Cue, Mix};

/// Distance between the Player's ears, in metres, for spatial cues.
const EAR_GAP: f32 = 0.2;

struct Slot {
    variants: Vec<Handle<AudioSource>>,
    last: Option<usize>,
}

/// Each slot's loaded variants, in `Cue::ALL` order, and whether the mix is muffled by a
/// Shellshock.
#[derive(Resource)]
struct Sounds {
    slots: Vec<Slot>,
    muffled: bool,
}

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
            .add_systems(
                Update,
                (
                    add_listener,
                    muffle,
                    (play_sounds, play_hound_sounds, play_nova_sounds, play_special_sounds, play_sapper_loops),
                )
                    .chain(),
            );
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
    commands.insert_resource(Sounds { slots, muffled: false });
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
        let slot = &mut self.slots[cue.index()];
        let pick = slots::pick_variant(slot.variants.len(), slot.last, rand::random());
        slot.last = Some(pick);
        slot.variants[pick].clone()
    }

    /// A cue's volume: its gain, turned down under a Shellshock's muffle (bar the Shellshock's
    /// own ring).
    fn gain(&self, cue: Cue) -> f32 {
        let gain = cue.spec().gain;
        if self.muffled && cue != Cue::Shellshock { gain * SHELLSHOCK_MUFFLE_GAIN } else { gain }
    }

    /// Plays `cue`. Spatial cues are heard from `at`; without it, they play flat.
    fn play(&mut self, commands: &mut Commands, cue: Cue, at: Option<Vec3>) {
        let player = AudioPlayer::new(self.next(cue));
        let spec = cue.spec();
        let settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(self.gain(cue)));
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

/// The mix is muffled while the Player is Shellshocked.
fn muffle(shellshocked: Res<Shellshocked>, mut sounds: ResMut<Sounds>) {
    let muffled = shellshocked.active();
    if sounds.muffled != muffled {
        sounds.muffled = muffled;
    }
}

/// A Nova Crawler's Vent drop, its swipes, its gas burst with the cloud's hiss, and the
/// Shellshock's ring.
fn play_nova_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    drops: Query<&Transform, Added<VentDrop>>,
    clouds: Query<(Entity, &Transform), Added<GasCloud>>,
    mut swipes: MessageReader<NovaSwipe>,
    mut shellshocks: MessageReader<ShellshockStarted>,
) {
    for at in &drops {
        sounds.play(&mut commands, Cue::NovaVent, Some(at.translation));
    }
    for _ in swipes.read() {
        sounds.play(&mut commands, Cue::NovaAttack, None);
    }
    for (cloud, at) in &clouds {
        sounds.play(&mut commands, Cue::NovaBurst, Some(at.translation));
        // The hiss loops for as long as the cloud lasts, and goes with it.
        let hiss = AudioPlayer::new(sounds.next(Cue::NovaGas));
        let settings = PlaybackSettings::LOOP
            .with_volume(Volume::Linear(sounds.gain(Cue::NovaGas)))
            .with_spatial(true);
        commands.entity(cloud).with_child((hiss, settings, Transform::default()));
    }
    for _ in shellshocks.read() {
        sounds.play(&mut commands, Cue::Shellshock, None);
    }
}

/// The Napalm's and the Shrieker's sounds, as their module asks for them.
fn play_special_sounds(mut commands: Commands, mut sounds: ResMut<Sounds>, mut cues: MessageReader<SpecialCue>) {
    for cue in cues.read() {
        sounds.play(&mut commands, cue.cue, cue.at);
    }
}

/// One of the Sapper's looping sounds, a child of the Sapper: its breathing while it lives, or
/// its Sabotage while it works.
#[derive(Component)]
struct SapperLoop {
    sabotage: bool,
}

/// The Sapper's idle loops from its arrival to its death, and its Sabotage loop while it works.
fn play_sapper_loops(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    arrived: Query<Entity, Added<Sapper>>,
    working: Query<Entity, (With<Sapper>, Added<Sabotaging>)>,
    died: Query<Entity, (With<Sapper>, Added<Dying>)>,
    mut stopped: RemovedComponents<Sabotaging>,
    loops: Query<(Entity, &SapperLoop, &ChildOf)>,
) {
    let mut start = |commands: &mut Commands, sapper: Entity, cue: Cue, sabotage: bool| {
        let player = AudioPlayer::new(sounds.next(cue));
        let settings = PlaybackSettings::LOOP
            .with_volume(Volume::Linear(sounds.gain(cue)))
            .with_spatial(true);
        commands.entity(sapper).with_child((SapperLoop { sabotage }, player, settings, Transform::default()));
    };
    for sapper in &arrived {
        start(&mut commands, sapper, Cue::SapperIdle, false);
    }
    for sapper in &working {
        start(&mut commands, sapper, Cue::SapperSabotage, true);
    }
    let stopped: Vec<Entity> = stopped.read().collect();
    for (entity, sound, parent) in &loops {
        let sapper = parent.parent();
        if (sound.sabotage && stopped.contains(&sapper)) || died.contains(sapper) {
            commands.entity(entity).despawn();
        }
    }
}
