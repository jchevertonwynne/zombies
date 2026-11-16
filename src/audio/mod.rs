//! Plays every cue from its slot in `assets/audio`. The placeholders there come from the
//! `gen-sounds` binary, and real recordings can replace them file by file.
//!
//! Most cues play from the messages and state changes the game's systems already make. Anything
//! else asks for its cue with a [`PlayCue`]. Loops are children of what they belong to, so they
//! stop when it goes, or flat loops that last while their trigger does. The mix rules live in
//! [`mix`].

pub mod cues;
pub mod mix;
pub mod slots;

use std::collections::HashMap;

use bevy::asset::io::file::FileAssetReader;
use bevy::audio::Volume;
use bevy::prelude::*;

use crate::weapons::attachment::FlameFired;
use crate::maps::barrier::{PlankRebuilt, PlankTorn};
use crate::economy::buy::{BuyRefused, CantAfford, HoldPurchased, Purchased};
use crate::maps::door::Door;
use crate::weapons::equipment::{Claymore, MonkeyBomb};
use crate::weapons::arsenal::WeaponId;
use crate::weapons::explosive::Explosion;
use crate::enemies::hellhound::Telegraph;
use crate::players::last_stand::{Down, SelfRevived};
use crate::weapons::mystery_box::MysteryBox;
use crate::enemies::nova::{GasCloud, VentDrop};
use crate::weapons::pack_a_punch::{PackAPunch, Stage};
use crate::economy::perk::{Drinking, PerkMachine};
use crate::players::player::{Health, LocalPlayer, Player, PlayerCamera, PlayerHit};
use crate::maps::power::{Power, Supply};
use crate::economy::power_up::{ActivePowerUps, Dropped, PowerUp, PowerUpTaken};
use crate::run::round::{Round, RoundEnded, RoundStarted};
use crate::enemies::sapper::{Sabotaged, Sabotaging, Sapper};
use crate::players::shellshock::{ShellshockStarted, Shellshocked};
use crate::enemies::special::{FirePool, Napalm, SpecialCue};
use crate::state::{GameState, InRun};
use crate::players::teammate::{HeldWeapon, Pose, Shots};
use crate::maps::teleporter::{MAINFRAME_AT, Teleporter};
use crate::maps::trap::Trap;
use crate::tuning::{PLAYER_CAPSULE_LENGTH, PLAYER_RADIUS, IDLE_VOCAL_CAP, IDLE_VOCAL_RETRY_SECS, LOW_HEALTH_FRACTION, NAPALM_HEAT_DIST, SHELLSHOCK_MUFFLE_GAIN, SHRIEKER_BLUR_SECS};
use crate::weapons::weapon::{Loadout, ShotFired};
use crate::enemies::zombie::{Attack, Dying, Gibbed, Rising, Zombie, ZombieHit, ZombieKilled};
use cues::{AUDIO_DIR, Cue, Mix};
use mix::BoxPhase;

/// Distance between the Player's ears, in metres, for spatial cues.
const EAR_GAP: f32 = 0.2;
/// A spawn fire pool crackles at this fraction of a death pool's volume.
const SPAWN_POOL_GAIN: f32 = 0.5;
/// The Flamethrower's roar lingers this long after its last tick, so it holds between ticks.
const FLAME_LINGER_SECS: f32 = 0.2;
/// A Crawler still for this long stops dragging.
const DRAG_STILL_SECS: f32 = 0.3;

/// Asks for a cue: heard from `at` if the cue is spatial, or flat without it, at `pitch` (its
/// playback speed) and `gain` (a share of its usual volume). Classic asks at 1.0 and 1.0.
#[derive(Message, Debug, Clone, Copy)]
pub struct PlayCue {
    pub cue: Cue,
    pub at: Option<Vec3>,
    pub pitch: f32,
    pub gain: f32,
}

impl PlayCue {
    /// `cue` as it usually sounds.
    pub fn new(cue: Cue, at: Option<Vec3>) -> Self {
        Self { cue, at, pitch: 1.0, gain: 1.0 }
    }
}

/// A cue's loaded variants. A slot with none is a bad one, and its cue plays nothing.
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

/// Each slot's files, found by the startup check, waiting to be loaded. A bad slot has none.
#[derive(Resource)]
struct SlotFiles(Vec<Option<Vec<String>>>);

/// A looping cue, a child of the entity it belongs to, so it stops when that goes.
#[derive(Component)]
struct LoopOf(Cue);

/// A flat looping cue, playing while its trigger lasts, or for `left` more seconds.
#[derive(Component)]
struct FlatLoop {
    cue: Cue,
    left: Option<f32>,
}

/// When a Zombie next makes its idle vocal.
#[derive(Component)]
struct IdleVoice {
    next: f32,
}

/// An idle vocal playing, a child of its Zombie. These are what the voice cap counts.
#[derive(Component)]
struct IdleVocal;

/// When a powered Perk machine next plays its jingle.
#[derive(Component)]
struct Jingle {
    next: f32,
}

/// What a teammate looked like last frame, for hearing what they've done since.
#[derive(Component)]
struct Heard {
    pose: Pose,
    shots: u8,
    weapon: WeaponId,
    last: Vec3,
    /// Seconds since their last footstep.
    step: f32,
}

/// Where a Crawler was last frame, and how long it has stood still, for its drag.
#[derive(Component)]
struct Drag {
    last: Vec3,
    still: f32,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        let dir = FileAssetReader::get_base_path()
            .join("assets")
            .join(AUDIO_DIR);
        let stems: Vec<String> = Cue::ALL.iter().map(|cue| cue.stem()).collect();
        let files = slots::list_files(&dir);
        let check = slots::check_slots(stems.iter().map(String::as_str), &files);
        if !check.errors.is_empty() {
            error!(
                "bad audio slots in {}, so their cues will be silent:\n  {}\nRun `cargo gen-sounds` to write placeholders for missing slots.",
                dir.display(),
                check.errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n  ")
            );
        }
        app.insert_resource(SlotFiles(check.found));

        app.add_message::<PlayCue>()
            .add_systems(Startup, load_sounds)
            .add_systems(OnEnter(GameState::GameOver), |mut commands: Commands, mut sounds: ResMut<Sounds>| {
                sounds.play(&mut commands, Cue::GameOver, None);
            })
            .add_systems(
                Update,
                (
                    add_listener,
                    muffle,
                    play_cues,
                    play_round_sounds,
                    play_buy_sounds,
                    play_player_sounds,
                    play_teammate_sounds,
                    play_zombie_sounds,
                    play_splashes,
                    play_map_sounds,
                    idle_vocals,
                    perk_jingles,
                    crawler_drag,
                    entity_loops,
                    flat_loops,
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
                .flatten()
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
    /// The variant of `cue` to play next, or `None` if its slot is bad.
    fn next(&mut self, cue: Cue) -> Option<Handle<AudioSource>> {
        let slot = &mut self.slots[cue.index()];
        if slot.variants.is_empty() {
            return None;
        }
        let pick = slots::pick_variant(slot.variants.len(), slot.last, rand::random());
        slot.last = Some(pick);
        Some(slot.variants[pick].clone())
    }

    /// A cue's volume: its gain, turned down under a Shellshock's muffle (bar the rings in the
    /// Player's own ears).
    fn gain(&self, cue: Cue) -> f32 {
        let gain = cue.spec().gain;
        if self.muffled && !matches!(cue, Cue::Shellshock | Cue::ScreamHit) { gain * SHELLSHOCK_MUFFLE_GAIN } else { gain }
    }

    /// Plays `cue`. Spatial cues are heard from `at`; without it, they play flat.
    fn play(&mut self, commands: &mut Commands, cue: Cue, at: Option<Vec3>) {
        self.play_at_speed(commands, cue, at, 1.0);
    }

    /// Plays `cue` at a playback `speed`, which shifts its pitch.
    fn play_at_speed(&mut self, commands: &mut Commands, cue: Cue, at: Option<Vec3>, speed: f32) {
        self.play_scaled(commands, cue, at, speed, 1.0);
    }

    /// `play_at_speed`, at `scale` of the cue's volume.
    fn play_scaled(&mut self, commands: &mut Commands, cue: Cue, at: Option<Vec3>, speed: f32, scale: f32) {
        let Some(sound) = self.next(cue) else { return };
        let player = AudioPlayer::new(sound);
        let settings = PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(self.gain(cue) * scale))
            .with_speed(speed);
        match (cue.spec().mix, at) {
            (Mix::Spatial, Some(at)) => {
                commands.spawn((player, settings.with_spatial(true), Transform::from_translation(at)));
            }
            _ => {
                commands.spawn((player, settings));
            }
        }
    }

    /// Plays `cue` heard from `at` whatever its mix: how a teammate's own-Player cue is heard from
    /// where they stand, at a playback `speed`.
    fn play_from(&mut self, commands: &mut Commands, cue: Cue, at: Vec3, speed: f32) {
        self.play_from_scaled(commands, cue, at, speed, 1.0);
    }

    /// `play_from`, at `scale` of the cue's volume.
    fn play_from_scaled(&mut self, commands: &mut Commands, cue: Cue, at: Vec3, speed: f32, scale: f32) {
        let Some(sound) = self.next(cue) else { return };
        let settings = PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(self.gain(cue) * scale))
            .with_speed(speed)
            .with_spatial(true);
        commands.spawn((AudioPlayer::new(sound), settings, Transform::from_translation(at)));
    }

    /// `cue` played once as a child of an entity, heard from wherever it goes. `None` if its slot
    /// is bad.
    fn voice(&mut self, cue: Cue) -> Option<impl Bundle + use<>> {
        let sound = self.next(cue)?;
        let settings = PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(self.gain(cue)))
            .with_spatial(cue.spec().mix == Mix::Spatial);
        Some((AudioPlayer::new(sound), settings, Transform::default()))
    }

    /// `cue` looping at `scale` of its volume. A spatial one, as a child, is heard from its parent.
    /// `None` if its slot is bad.
    fn looping(&mut self, cue: Cue, scale: f32) -> Option<impl Bundle + use<>> {
        let sound = self.next(cue)?;
        let settings = PlaybackSettings::LOOP
            .with_volume(Volume::Linear(self.gain(cue) * scale))
            .with_spatial(cue.spec().mix == Mix::Spatial);
        Some((AudioPlayer::new(sound), settings, Transform::default()))
    }
}

/// Adds `bundle` as a child of `parent`, unless `parent` is gone by the time it lands.
fn spawn_child(commands: &mut Commands, parent: Entity, bundle: impl Bundle) {
    commands.entity(parent).queue_silenced(move |mut parent: EntityWorldMut| {
        parent.with_child(bundle);
    });
}

/// Keeps one `cue` loop on each entity in `wanted` (at its volume scale) and none on any other.
fn sync_loops(
    commands: &mut Commands,
    sounds: &mut Sounds,
    loops: &Query<(Entity, &LoopOf, &ChildOf)>,
    cue: Cue,
    wanted: &[(Entity, f32)],
) {
    let owners: Vec<Entity> = wanted.iter().map(|&(owner, _)| owner).collect();
    let playing: Vec<(Entity, Entity)> = loops
        .iter()
        .filter(|(_, sound, _)| sound.0 == cue)
        .map(|(entity, _, parent)| (entity, parent.parent()))
        .collect();
    let (start, stop) = mix::loop_changes(&owners, &playing);
    for owner in start {
        let scale = wanted.iter().find(|w| w.0 == owner).map_or(1.0, |w| w.1);
        let Some(sound) = sounds.looping(cue, scale) else { break };
        spawn_child(commands, owner, (LoopOf(cue), sound));
    }
    for sound in stop {
        commands.entity(sound).try_despawn();
    }
}

/// Starts a flat loop of `cue` for `secs`, or stretches the one playing to last that long.
fn hold_flat_loop(commands: &mut Commands, sounds: &mut Sounds, loops: &mut Query<&mut FlatLoop>, cue: Cue, secs: f32) {
    match loops.iter_mut().find(|sound| sound.cue == cue) {
        Some(mut sound) => sound.left = Some(sound.left.map_or(secs, |left| left.max(secs))),
        None => {
            if let Some(sound) = sounds.looping(cue, 1.0) {
                commands.spawn((InRun, FlatLoop { cue, left: Some(secs) }, sound));
            }
        }
    }
}

/// The mix is muffled while the Player is Shellshocked.
fn muffle(shellshocked: Query<&Shellshocked, With<LocalPlayer>>, mut sounds: ResMut<Sounds>) {
    let muffled = shellshocked.iter().any(Shellshocked::active);
    if sounds.muffled != muffled {
        sounds.muffled = muffled;
    }
}

/// Cues other systems ask for, and the Shellshock's ring. A Shrieker's scream rings in the
/// Player's ears for the length of its blur, in place of the Shellshock's own ring.
fn play_cues(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut asked: MessageReader<PlayCue>,
    mut specials: MessageReader<SpecialCue>,
    mut shellshocks: MessageReader<ShellshockStarted>,
    mut loops: Query<&mut FlatLoop>,
) {
    let requests = asked
        .read()
        .copied()
        .chain(specials.read().map(|cue| PlayCue::new(cue.cue, cue.at)))
        .collect::<Vec<_>>();
    let mut screamed = loops.iter().any(|sound| sound.cue == Cue::ScreamHit);
    for PlayCue { cue, at, pitch, gain } in requests {
        if cue == Cue::ScreamHit {
            hold_flat_loop(&mut commands, &mut sounds, &mut loops, cue, SHRIEKER_BLUR_SECS);
            screamed = true;
        } else {
            sounds.play_scaled(&mut commands, cue, at, pitch, gain);
        }
    }
    for _ in shellshocks.read() {
        if !screamed {
            sounds.play(&mut commands, Cue::Shellshock, None);
        }
    }
}

/// A cue that plays in place of `RoundStart` when a normal Round begins. Classic leaves it unset;
/// anything layered on top may set it for a Round of its own.
#[derive(Resource, Default)]
pub struct RoundStartCue(pub Option<Cue>);

/// Round stings, the Down and the Self-Revive.
fn play_round_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut started: MessageReader<RoundStarted>,
    mut ended: MessageReader<RoundEnded>,
    mut downs: MessageReader<Down>,
    mut revives: MessageReader<SelfRevived>,
    local_player: Query<(), With<LocalPlayer>>,
    round: Option<Res<Round>>,
    standing_in: Option<Res<RoundStartCue>>,
) {
    for _ in started.read() {
        // A Hellhound Round opens with thunder and the announcer instead.
        if round.as_ref().is_some_and(|round| round.hellhound.is_some()) {
            sounds.play(&mut commands, Cue::HellhoundRoundStart, None);
            sounds.play(&mut commands, Cue::AnnouncerHellhoundRound, None);
        } else {
            let cue = standing_in.as_ref().and_then(|standing_in| standing_in.0).unwrap_or(Cue::RoundStart);
            sounds.play(&mut commands, cue, None);
        }
    }
    for _ in ended.read() {
        sounds.play(&mut commands, Cue::RoundEnd, None);
    }
    // A teammate's Down is heard from them instead (see `play_teammate_sounds`).
    for down in downs.read() {
        if local_player.contains(down.player) {
            sounds.play(&mut commands, Cue::Down, None);
        }
    }
    for _ in revives.read() {
        sounds.play(&mut commands, Cue::SelfRevive, None);
    }
}

/// Buys, Planks and Power-ups.
fn play_buy_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut purchases: MessageReader<Purchased>,
    mut holds: MessageReader<HoldPurchased>,
    mut cant_afford: MessageReader<CantAfford>,
    mut refusals: MessageReader<BuyRefused>,
    local: Query<Entity, With<LocalPlayer>>,
    mut torn: MessageReader<PlankTorn>,
    mut rebuilt: MessageReader<PlankRebuilt>,
    mut power_ups: MessageReader<PowerUpTaken>,
) {
    for _ in 0..purchases.read().count() + holds.read().count() {
        sounds.play(&mut commands, Cue::Buy, None);
    }
    // Not affording a buy and a refused buy share the buzzer, heard only by the one refused.
    let buzzes = cant_afford.read().filter(|refused| local.contains(refused.player)).count()
        + refusals.read().filter(|refused| local.contains(refused.player)).count();
    for _ in 0..buzzes {
        sounds.play(&mut commands, Cue::CantAfford, None);
    }
    for plank in torn.read() {
        sounds.play(&mut commands, Cue::PlankTear, Some(plank.at));
    }
    for plank in rebuilt.read() {
        sounds.play(&mut commands, Cue::PlankRebuild, Some(plank.at));
    }
    for PowerUpTaken(power_up, _) in power_ups.read() {
        sounds.play(&mut commands, Cue::PowerupPickup, None);
        sounds.play(&mut commands, power_up.announcer(), None);
        if *power_up == PowerUp::Nuke {
            sounds.play(&mut commands, Cue::NukeBlast, None);
        }
    }
}

/// The Player's own sounds: gunshots (by class, at the Weapon's pitch, with Pack-a-Punch's zap on
/// top), the Flamethrower's roar, hits and kills, explosions, getting hurt, Reloads and switches,
/// Claymores and Perk drinks.
fn play_player_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut shots: MessageReader<ShotFired>,
    mut flames: MessageReader<FlameFired>,
    mut hits: MessageReader<ZombieHit>,
    mut kills: MessageReader<ZombieKilled>,
    mut explosions: MessageReader<Explosion>,
    mut hurts: MessageReader<PlayerHit>,
    local_player: Query<(), With<LocalPlayer>>,
    mut loops: Query<&mut FlatLoop>,
    loadout: Query<&Loadout, With<LocalPlayer>>,
    claymores: Query<(&Claymore, &Transform), Added<Claymore>>,
    drinks: Query<(), (With<LocalPlayer>, Added<Drinking>)>,
    // Whether the Player was reloading, and switching, last frame.
    mut was: Local<(bool, bool)>,
    rider: Query<&crate::players::player::Locomotion, With<LocalPlayer>>,
    mut was_riding: Local<bool>,
) {
    // A Zipline ride: its whine as it starts, the thump of its landing.
    if let Ok(locomotion) = rider.single() {
        match (*was_riding, locomotion.riding()) {
            (false, true) => {
                sounds.play(&mut commands, Cue::ZiplineRide, None);
            }
            (true, false) => {
                sounds.play(&mut commands, Cue::ZiplineLand, None);
            }
            _ => {}
        }
        *was_riding = locomotion.riding();
    }
    for shot in shots.read() {
        let (cue, speed) = mix::shot_cue(shot.weapon, shot.underbarrel);
        sounds.play_at_speed(&mut commands, cue, None, speed);
        if shot.upgraded {
            sounds.play(&mut commands, Cue::PapShotLayer, None);
        }
    }
    if flames.read().count() > 0 {
        hold_flat_loop(&mut commands, &mut sounds, &mut loops, Cue::Flamethrower, FLAME_LINGER_SECS);
    }
    // Hits and kills are heard by whoever made them.
    let own = |player: Option<Entity>| player.is_none_or(|player| local_player.contains(player));
    for hit in hits.read().filter(|hit| own(hit.player)) {
        sounds.play(&mut commands, if hit.headshot { Cue::Headshot } else { Cue::Hit }, None);
    }
    for kill in kills.read() {
        if own(kill.player) {
            sounds.play(&mut commands, Cue::Kill, None);
            if kill.headshot {
                sounds.play(&mut commands, Cue::Headshot, None);
            }
        }
        if kill.attack == Attack::Trap {
            sounds.play(&mut commands, Cue::TrapKill, Some(kill.at));
        }
    }
    for explosion in explosions.read() {
        sounds.play(&mut commands, Cue::Explosion, Some(explosion.at));
    }
    for hurt in hurts.read() {
        if local_player.contains(hurt.player) {
            sounds.play(&mut commands, Cue::PlayerHurt, None);
        }
    }
    if let Ok(loadout) = loadout.single() {
        let now = (loadout.reload.is_some(), loadout.swap.is_some());
        if now.0 && !was.0 {
            sounds.play(&mut commands, Cue::Reload, None);
        }
        if now.1 && !was.1 {
            sounds.play(&mut commands, Cue::WeaponSwap, None);
        }
        *was = now;
    }
    for (claymore, at) in &claymores {
        if local_player.contains(claymore.thrower()) {
            sounds.play(&mut commands, Cue::ClaymorePlant, None);
        } else {
            sounds.play_from(&mut commands, Cue::ClaymorePlant, at.translation, 1.0);
        }
    }
    for _ in &drinks {
        sounds.play(&mut commands, Cue::PerkDrink, None);
    }
}

/// A teammate's sounds, heard from where they stand: their gunshots, Reloads, Weapon switches,
/// Knife swings, Perk drinks, Downs and getting back up, stance rustles and Dive landings, and
/// their footsteps, quicker when they sprint, or the drag of their Crawl, Prone or Down (the
/// Player's own steps and Crawl are silent). The same cues the Player hears flat from themselves.
fn play_teammate_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut teammates: Query<
        (Entity, &Transform, &Pose, &Shots, &HeldWeapon, Option<&mut Heard>),
        (With<Player>, Without<LocalPlayer>),
    >,
    time: Res<Time>,
    map: Option<Res<crate::maps::map::MapId>>,
    tide: Option<Res<crate::maps::tide::Tide>>,
) {
    let tide = tide.map_or_else(Default::default, |tide| tide.level());
    let dt = time.delta_secs();
    for (teammate, transform, pose, shots, held, heard) in &mut teammates {
        let at = transform.translation;
        let Some(mut heard) = heard else {
            let heard = Heard { pose: *pose, shots: shots.count(), weapon: held.id, last: at, step: 0.0 };
            commands.entity(teammate).insert(heard);
            continue;
        };
        if shots.count() != heard.shots {
            let (cue, speed) = mix::shot_cue(held.id, None);
            sounds.play_from(&mut commands, cue, at, speed);
            if held.glint.is_some() {
                sounds.play_from(&mut commands, Cue::PapShotLayer, at, 1.0);
            }
        }
        if held.id != heard.weapon {
            sounds.play_from(&mut commands, Cue::WeaponSwap, at, 1.0);
        }
        if let Some(cue) = mix::pose_cue(&heard.pose, pose) {
            sounds.play_from(&mut commands, cue, at, 1.0);
        }
        if let Some(cue) = mix::stance_cue(heard.pose.feel(), pose.feel()) {
            sounds.play_from_scaled(&mut commands, cue, at, 1.0, mix::teammate_gain(cue));
        }
        if dt > 0.0 {
            let speed = Vec2::new(at.x - heard.last.x, at.z - heard.last.z).length() / dt;
            heard.step += dt;
            match mix::movement_cue(pose, speed) {
                Some((cue, interval)) if heard.step >= interval => {
                    heard.step = 0.0;
                    // Footsteps from the feet; a crawling body drags along the ground it lies on.
                    let feet = at - Vec3::Y * (PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0);
                    let from = match cue {
                        Cue::Footstep => feet,
                        _ => at,
                    };
                    // Through Shallow Water a footstep is a wade.
                    let cue = match (cue, map.as_ref()) {
                        (Cue::Footstep, Some(map)) if crate::maps::water::wading(map.layout(), tide, feet) => Cue::WadeStep,
                        _ => cue,
                    };
                    sounds.play_from(&mut commands, cue, from, 1.0);
                }
                Some(_) => {}
                None => heard.step = 0.0,
            }
        }
        heard.pose = *pose;
        heard.shots = shots.count();
        heard.weapon = held.id;
        heard.last = at;
    }
}

/// A Player or Zombie entering Shallow Water splashes, heard from their feet.
fn play_splashes(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    entered: Query<&GlobalTransform, Added<crate::maps::water::Wet>>,
) {
    for at in &entered {
        sounds.play(&mut commands, Cue::Splash, Some(at.translation() - Vec3::Y * ZOMBIE_CENTRE_HEIGHT));
    }
}

/// About how high a Player's or Zombie's centre is over their feet, for a sound from the feet.
const ZOMBIE_CENTRE_HEIGHT: f32 = 0.9;

/// Zombies, Hellhounds and Nova Crawlers arriving, losing their legs and dying.
fn play_zombie_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    strikes: Query<&GlobalTransform, Added<Telegraph>>,
    deaths: Query<(&Zombie, &GlobalTransform), Added<Dying>>,
    drops: Query<&GlobalTransform, Added<VentDrop>>,
    clouds: Query<&GlobalTransform, Added<GasCloud>>,
    positions: Query<&GlobalTransform>,
    mut gibbed: MessageReader<Gibbed>,
) {
    for at in &strikes {
        sounds.play(&mut commands, Cue::HoundSpawn, Some(at.translation()));
    }
    // A Zombie rising plays `ZombieRiser` or, in Shallow Water, `ZombieWade`: see `water::start_rising`.
    for (zombie, at) in &deaths {
        if zombie.is_hellhound() {
            sounds.play(&mut commands, Cue::HoundDeath, Some(at.translation()));
        } else if !zombie.is_special() {
            sounds.play(&mut commands, Cue::ZombieDeath, Some(at.translation()));
        }
    }
    for &Gibbed { zombie } in gibbed.read() {
        if let Ok(at) = positions.get(zombie) {
            sounds.play(&mut commands, Cue::CrawlerGib, Some(at.translation()));
        }
    }
    for at in &drops {
        sounds.play(&mut commands, Cue::NovaVent, Some(at.translation()));
    }
    for at in &clouds {
        sounds.play(&mut commands, Cue::NovaBurst, Some(at.translation()));
    }
}

/// The Map's state changes: Doors opening, the Power coming on, Sabotages and restores, the
/// Mystery Box and Pack-a-Punch.
fn play_map_sounds(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    doors: Query<(&Door, &GlobalTransform), Changed<Door>>,
    power: Res<Power>,
    teleporter: Option<Res<Teleporter>>,
    sabotaged: Query<&GlobalTransform, Added<Sabotaged>>,
    mut restored: RemovedComponents<Sabotaged>,
    standing: Query<(), Without<Sabotaged>>,
    boxes: Query<(Entity, &MysteryBox, &GlobalTransform)>,
    machines: Query<(Entity, &PackAPunch, &GlobalTransform)>,
    // Whether the Power was on and the Teleporter Sabotaged last frame, and where each Box and
    // Pack-a-Punch was up to.
    mut was: Local<(bool, bool)>,
    mut box_phases: Local<HashMap<Entity, BoxPhase>>,
    mut pap_stages: Local<HashMap<Entity, Option<Stage>>>,
) {
    for (door, at) in &doors {
        if door.open {
            sounds.play(&mut commands, Cue::DoorOpen, Some(at.translation()));
        }
    }
    if power.is_on() && !was.0 {
        sounds.play(&mut commands, Cue::PowerOn, None);
    }
    for at in &sabotaged {
        sounds.play(&mut commands, Cue::PowerDown, Some(at.translation()));
    }
    let teleporter_down = teleporter.is_some_and(|teleporter| teleporter.sabotaged());
    if teleporter_down && !was.1 {
        sounds.play(&mut commands, Cue::PowerDown, Some(MAINFRAME_AT));
    }
    // A restored system is still standing; one despawned with the Run isn't.
    let restores = restored.read().filter(|&entity| standing.contains(entity)).count();
    if restores > 0 || (was.1 && !teleporter_down) {
        sounds.play(&mut commands, Cue::PowerRestore, None);
    }
    *was = (power.is_on(), teleporter_down);

    let mut phases = HashMap::new();
    for (entity, mystery_box, at) in &boxes {
        let now = BoxPhase::from(&mystery_box.state);
        let before = box_phases.get(&entity).copied().unwrap_or(now);
        if let Some(cue) = mix::box_cue(before, now) {
            sounds.play(&mut commands, cue, Some(at.translation()));
        }
        phases.insert(entity, now);
    }
    *box_phases = phases;

    let mut stages = HashMap::new();
    for (entity, machine, at) in &machines {
        let now = machine.stage();
        let before = pap_stages.get(&entity).copied().unwrap_or(now);
        if let Some(cue) = mix::pap_cue(before, now) {
            sounds.play(&mut commands, cue, Some(at.translation()));
        }
        stages.insert(entity, now);
    }
    *pap_stages = stages;
}

/// Zombies moan, grunt and shriek now and then, as do Crawlers, Nova Crawlers and Hellhounds,
/// but only the nearest `IDLE_VOCAL_CAP` are heard at once. A Zombie's idle stops when it dies.
fn idle_vocals(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    camera: Query<&GlobalTransform, With<PlayerCamera>>,
    mut zombies: Query<
        (Entity, &Zombie, &GlobalTransform, Option<&mut IdleVoice>),
        (Without<Dying>, Without<Rising>, Without<Telegraph>),
    >,
    vocals: Query<(Entity, &ChildOf), With<IdleVocal>>,
    positions: Query<&GlobalTransform, Without<Dying>>,
    time: Res<Time>,
) {
    let Ok(ears) = camera.single() else {
        return;
    };
    let ears = ears.translation();
    let dt = time.delta_secs();
    let mut due = Vec::new();
    for (entity, zombie, at, voice) in &mut zombies {
        let Some((_, every)) = mix::idle_vocal(zombie) else { continue };
        match voice {
            Some(mut voice) => {
                voice.next -= dt;
                if voice.next <= 0.0 {
                    due.push((entity, at.translation().distance(ears)));
                }
            }
            None => {
                commands.entity(entity).insert(IdleVoice {
                    next: mix::within(&every, rand::random()),
                });
            }
        }
    }
    let mut playing = Vec::new();
    for (vocal, parent) in &vocals {
        match positions.get(parent.parent()) {
            Ok(at) => playing.push((vocal, at.translation().distance(ears))),
            // Its Zombie is dying, and makes its death sound instead.
            Err(_) => commands.entity(vocal).try_despawn(),
        }
    }
    let voices = mix::choose_voices(&playing, &due, IDLE_VOCAL_CAP);
    for vocal in voices.stop {
        commands.entity(vocal).try_despawn();
    }
    for (entity, _) in due {
        let Ok((_, zombie, _, Some(mut voice))) = zombies.get_mut(entity) else { continue };
        let Some((cue, every)) = mix::idle_vocal(zombie) else { continue };
        if voices.start.contains(&entity) {
            voice.next = mix::within(&every, rand::random());
            if let Some(sound) = sounds.voice(cue) {
                spawn_child(&mut commands, entity, (IdleVocal, sound));
            }
        } else {
            voice.next = IDLE_VOCAL_RETRY_SECS;
        }
    }
}

/// Every Perk machine plays its jingle when bought, and every 2–4 minutes while it's powered.
fn perk_jingles(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut machines: Query<(Entity, &PerkMachine, &GlobalTransform, Option<&mut Jingle>, Has<Sabotaged>)>,
    mut purchases: MessageReader<Purchased>,
    supply: Supply,
    run: Res<crate::state::RunType>,
    map: Res<crate::maps::map::MapId>,
    time: Res<Time>,
) {
    let bought: Vec<Entity> = purchases.read().map(|purchase| purchase.buyable).collect();
    for (entity, machine, at, jingle, sabotaged) in &mut machines {
        let Some(mut jingle) = jingle else {
            commands.entity(entity).insert(Jingle {
                next: mix::next_jingle_secs(rand::random()),
            });
            continue;
        };
        let powered = machine.powered(&supply, *run, *map) && !sabotaged;
        if powered {
            jingle.next -= time.delta_secs();
        }
        if bought.contains(&entity) || (powered && jingle.next <= 0.0) {
            sounds.play(&mut commands, mix::jingle(machine.perk), Some(at.translation()));
            jingle.next = mix::next_jingle_secs(rand::random());
        }
    }
}

/// A Crawler drags itself along with a scrape and a slap, for as long as it's moving.
fn crawler_drag(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut zombies: Query<(Entity, &Zombie, &GlobalTransform, Option<&mut Drag>), Without<Dying>>,
    loops: Query<(Entity, &LoopOf, &ChildOf)>,
    time: Res<Time>,
) {
    let mut wanted = Vec::new();
    for (entity, zombie, at, drag) in &mut zombies {
        if !zombie.is_crawler() {
            continue;
        }
        let at = at.translation();
        match drag {
            Some(mut drag) => {
                drag.still = if at.distance(drag.last) > 1e-3 { 0.0 } else { drag.still + time.delta_secs() };
                drag.last = at;
                if drag.still < DRAG_STILL_SECS {
                    wanted.push((entity, 1.0));
                }
            }
            None => {
                commands.entity(entity).insert(Drag { last: at, still: 0.0 });
            }
        }
    }
    sync_loops(&mut commands, &mut sounds, &loops, Cue::CrawlerDrag, &wanted);
}

/// The loops that belong to something in the world, each lasting as long as its trigger: the
/// gas cloud's hiss, the Sapper's breathing and its Sabotage, a living Napalm, a fire pool, a
/// landed Monkey Bomb, a Power-up on the ground, a running Trap and a Box in a Fire Sale.
fn entity_loops(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    loops: Query<(Entity, &LoopOf, &ChildOf)>,
    clouds: Query<Entity, With<GasCloud>>,
    sappers: Query<(Entity, Has<Sabotaging>), (With<Sapper>, Without<Dying>)>,
    napalms: Query<Entity, (With<Napalm>, Without<Dying>)>,
    pools: Query<(Entity, &FirePool)>,
    bombs: Query<(Entity, &MonkeyBomb)>,
    dropped: Query<Entity, With<Dropped>>,
    traps: Query<(Entity, &Trap)>,
    pieces: Query<(Entity, &crate::maps::shifting::PieceState)>,
    boxes: Query<(Entity, &MysteryBox)>,
    active: Res<ActivePowerUps>,
) {
    let all = |entities: Vec<Entity>| entities.into_iter().map(|entity| (entity, 1.0)).collect::<Vec<_>>();
    let fire_sale = active.active(PowerUp::FireSale);
    let wanted: [(Cue, Vec<(Entity, f32)>); 11] = [
        (Cue::NovaGas, all(clouds.iter().collect())),
        (Cue::SapperIdle, all(sappers.iter().map(|(entity, _)| entity).collect())),
        (Cue::SapperSabotage, all(sappers.iter().filter(|(_, working)| *working).map(|(entity, _)| entity).collect())),
        (Cue::NapalmIdle, all(napalms.iter().collect())),
        (
            Cue::FirePool,
            pools.iter().map(|(entity, pool)| (entity, if pool.spawned { SPAWN_POOL_GAIN } else { 1.0 })).collect(),
        ),
        (Cue::MonkeyBomb, all(bombs.iter().filter(|(_, bomb)| bomb.landed()).map(|(entity, _)| entity).collect())),
        (Cue::PowerupHum, all(dropped.iter().collect())),
        (Cue::TrapActive, all(traps.iter().filter(|(_, trap)| trap.running()).map(|(entity, _)| entity).collect())),
        (Cue::BridgeBell, all(pieces.iter().filter(|(_, state)| state.phase().bell()).map(|(entity, _)| entity).collect())),
        (Cue::BridgeGrind, all(pieces.iter().filter(|(_, state)| state.phase().swinging()).map(|(entity, _)| entity).collect())),
        (
            Cue::FireSale,
            all(boxes.iter().filter(|(_, mystery_box)| fire_sale && mystery_box.present()).map(|(entity, _)| entity).collect()),
        ),
    ];
    for (cue, wanted) in wanted {
        sync_loops(&mut commands, &mut sounds, &loops, cue, &wanted);
    }
}

/// The flat loops: the heartbeat at low Health and the Napalm's heat close by last while they
/// do; the timed ones (the Flamethrower, a scream's ring) run out.
fn flat_loops(
    mut commands: Commands,
    mut sounds: ResMut<Sounds>,
    mut loops: Query<(Entity, &mut FlatLoop)>,
    player: Query<(&Health, &GlobalTransform), With<LocalPlayer>>,
    napalms: Query<&GlobalTransform, (With<Napalm>, Without<Dying>)>,
    state: Res<State<GameState>>,
    time: Res<Time>,
) {
    let playing = *state.get() == GameState::Playing;
    let (low, hot) = match player.single() {
        Ok((health, at)) if playing => (
            health.fraction() < LOW_HEALTH_FRACTION,
            napalms.iter().any(|napalm| napalm.translation().distance(at.translation()) <= NAPALM_HEAT_DIST),
        ),
        _ => (false, false),
    };
    let mut on = [(Cue::LowHealth, low), (Cue::NapalmHeat, hot)];
    for (entity, mut sound) in &mut loops {
        match &mut sound.left {
            Some(left) => {
                *left -= time.delta_secs();
                if *left <= 0.0 || !playing {
                    commands.entity(entity).despawn();
                }
            }
            None => match on.iter_mut().find(|(cue, _)| *cue == sound.cue) {
                // Already playing: nothing to start.
                Some((_, want)) if *want => *want = false,
                _ => commands.entity(entity).despawn(),
            },
        }
    }
    for (cue, start) in on {
        if start
            && let Some(sound) = sounds.looping(cue, 1.0) {
                commands.spawn((InRun, FlatLoop { cue, left: None }, sound));
            }
    }
}
