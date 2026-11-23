//! The mix rules: which cue an event plays and at what pitch, which idle vocals get a voice,
//! which loops start and stop, and when a Perk machine sings. Plain functions, tested without an
//! `App`.

use std::ops::RangeInclusive;

use crate::weapons::arsenal::{WeaponClass, WeaponId};
use crate::weapons::attachment::Underbarrel;
use crate::weapons::mystery_box::BoxState;
use crate::weapons::pack_a_punch::Stage;
use crate::economy::perk::Perk;
use crate::players::player::{Stance, StanceFeel};
use crate::players::teammate::{Action, Pose};
use crate::tuning::*;
use crate::enemies::zombie::{SpeedTier, Zombie};

use super::cues::{Cue, FloorFamily};

/// Playback speeds the Weapons of one class take in turn, so no two in a class sound alike.
const PITCH_STEPS: [f32; 7] = [1.0, 1.08, 0.92, 1.15, 0.86, 1.04, 0.96];

/// The gunshot a class of Weapon fires with. The wonder weapons have their own; the Crossbow
/// twangs like a sniper rifle and the Ballistic Knife pops like a pistol.
pub fn class_gunshot(class: WeaponClass) -> Cue {
    match class {
        WeaponClass::Pistol | WeaponClass::DualWield | WeaponClass::BallisticKnife => Cue::GunshotPistol,
        WeaponClass::Smg => Cue::GunshotSmg,
        WeaponClass::Rifle => Cue::GunshotRifle,
        WeaponClass::Shotgun => Cue::GunshotShotgun,
        WeaponClass::Sniper | WeaponClass::Crossbow => Cue::GunshotSniper,
        WeaponClass::Lmg => Cue::GunshotLmg,
        WeaponClass::Launcher => Cue::GunshotLauncher,
        WeaponClass::RayGun => Cue::RayGun,
        WeaponClass::Thundergun => Cue::Thundergun,
    }
}

/// How many voices of `cue` may sound at once, or `None` if it isn't capped. The gunshots are: a
/// long clip at a high fire rate would otherwise pile up.
pub fn voice_cap(cue: Cue) -> Option<usize> {
    match cue {
        Cue::GunshotPistol
        | Cue::GunshotSmg
        | Cue::GunshotRifle
        | Cue::GunshotShotgun
        | Cue::GunshotLmg
        | Cue::GunshotSniper
        | Cue::GunshotLauncher => Some(GUNSHOT_MAX_VOICES),
        _ => None,
    }
}

/// Which of one cue's `live` voices to cut to keep `cap`: the oldest first. Each voice comes
/// with the sequence number it started on, so a smaller number is older.
pub fn voices_to_cut<E: Copy>(live: &[(E, u64)], cap: usize) -> Vec<E> {
    let mut by_age = live.to_vec();
    by_age.sort_by_key(|&(_, seq)| seq);
    by_age.iter().take(live.len().saturating_sub(cap)).map(|&(voice, _)| voice).collect()
}

/// A Weapon's fixed pitch offset, as a playback speed: by its place among the Weapons of its
/// class, so every Weapon in a class has its own.
pub fn weapon_pitch(weapon: WeaponId) -> f32 {
    let class = weapon.stats().class;
    let rank = WEAPONS.iter().take_while(|row| row.id != weapon).filter(|row| row.class == class).count();
    PITCH_STEPS[rank % PITCH_STEPS.len()]
}

/// The gunshot a shot plays, and its playback speed. The Masterkey and the Grenade Launcher
/// fire as a shotgun and a launcher, at their own pitch.
pub fn shot_cue(weapon: WeaponId, underbarrel: Option<Underbarrel>) -> (Cue, f32) {
    match underbarrel {
        Some(Underbarrel::Masterkey) => (Cue::GunshotShotgun, 1.0),
        Some(Underbarrel::GrenadeLauncher) => (Cue::GunshotLauncher, 1.0),
        // The Flamethrower roars as a loop instead.
        Some(Underbarrel::Flamethrower) => (Cue::Flamethrower, 1.0),
        None => (class_gunshot(weapon.stats().class), weapon_pitch(weapon)),
    }
}

/// A Weapon's own gunshot slot: files are `gunshot_<weapon>.ogg` or `gunshot_<weapon>_1.ogg`...
/// A shot plays them when the slot has files, and its class's gunshot (`class_gunshot`) when it
/// has none, as a floor family does for footsteps. The slots are optional. It is a slot name, not
/// a `Cue`, so the 28 Weapons add no variants to `Cue::ALL` (every one of which must have files,
/// and which `gen-sounds` writes placeholders for); `WeaponId as usize` indexes them.
pub const fn weapon_gunshot_slot(weapon: WeaponId) -> &'static str {
    match weapon {
        WeaponId::M1911 => "gunshot_m1911",
        WeaponId::Olympia => "gunshot_olympia",
        WeaponId::M14 => "gunshot_m14",
        WeaponId::Mp40 => "gunshot_mp40",
        WeaponId::TrenchGun => "gunshot_trench_gun",
        WeaponId::Ak74u => "gunshot_ak74u",
        WeaponId::Cz75 => "gunshot_cz75",
        WeaponId::Cz75DualWield => "gunshot_cz75_dual_wield",
        WeaponId::Python => "gunshot_python",
        WeaponId::Spectre => "gunshot_spectre",
        WeaponId::G11 => "gunshot_g11",
        WeaponId::Famas => "gunshot_famas",
        WeaponId::Aug => "gunshot_aug",
        WeaponId::Commando => "gunshot_commando",
        WeaponId::Galil => "gunshot_galil",
        WeaponId::FnFal => "gunshot_fn_fal",
        WeaponId::Hs10 => "gunshot_hs10",
        WeaponId::Spas12 => "gunshot_spas12",
        WeaponId::Dragunov => "gunshot_dragunov",
        WeaponId::L96a1 => "gunshot_l96a1",
        WeaponId::Rpk => "gunshot_rpk",
        WeaponId::Hk21 => "gunshot_hk21",
        WeaponId::M72Law => "gunshot_m72_law",
        WeaponId::ChinaLake => "gunshot_china_lake",
        WeaponId::RayGun => "gunshot_ray_gun",
        WeaponId::Thundergun => "gunshot_thundergun",
        WeaponId::Crossbow => "gunshot_crossbow",
        WeaponId::BallisticKnife => "gunshot_ballistic_knife",
    }
}

/// What a shot plays: the Weapon's own slot, or a cue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    Own(WeaponId),
    Cue(Cue),
}

/// The sound a shot plays and its playback speed. A Weapon's own slot (when `has_files` says it
/// has files) plays at its natural speed; a Weapon without one falls back to its class gunshot
/// at its pitch offset. The underbarrels are unchanged.
pub fn shot_sound(weapon: WeaponId, underbarrel: Option<Underbarrel>, has_files: impl Fn(WeaponId) -> bool) -> (Shot, f32) {
    if underbarrel.is_none() && has_files(weapon) {
        return (Shot::Own(weapon), 1.0);
    }
    let (cue, speed) = shot_cue(weapon, underbarrel);
    (Shot::Cue(cue), speed)
}

/// The cue whose gain, and voice cap, a shot takes: its class gunshot for a Weapon's own slot.
pub fn shot_voice_cue(shot: Shot) -> Cue {
    match shot {
        Shot::Own(weapon) => class_gunshot(weapon.stats().class),
        Shot::Cue(cue) => cue,
    }
}

/// The sound a teammate's body makes going from `before` to `now`: the groan of going Down, the
/// rush of getting back up, or the start of a Reload, Knife swing or Perk drink. Throws, and the
/// Revive's kneel, have no sound of their own.
pub fn pose_cue(before: &Pose, now: &Pose) -> Option<Cue> {
    match (before.last_stand, now.last_stand) {
        (false, true) => return Some(Cue::Down),
        (true, false) => return Some(Cue::SelfRevive),
        _ => {}
    }
    if before.action == now.action {
        return None;
    }
    // The landing at the end of a Zipline ride.
    if before.action == Action::Ride {
        return Some(Cue::ZiplineLand);
    }
    match now.action {
        Action::Ride => Some(Cue::ZiplineRide),
        Action::Reload => Some(Cue::Reload),
        Action::Knife => Some(Cue::KnifeSwing),
        Action::Drink => Some(Cue::PerkDrink),
        Action::None | Action::Throw | Action::Revive => None,
    }
}

/// The sound the Player's own body makes going from `before` to `now`: a rustle at the start of
/// every stance change, deliberate or forced, both ways. A Dive's launch is silent and its landing
/// thuds instead. Going Down, or getting up from one, isn't a stance change; it has its own sounds.
pub fn stance_cue(before: StanceFeel, now: StanceFeel) -> Option<Cue> {
    match (before.shown, now.shown) {
        // The landing: still Prone, and the Dive that brought them there is over. (A Down mid-Dive
        // ends it too, but shows no Stance; a Teleporter ride ends it Standing, which rustles.)
        (Some(Stance::Prone), Some(Stance::Prone)) if before.diving && !now.diving => Some(Cue::DiveLand),
        (Some(was), Some(is)) if was != is && !now.diving => Some(Cue::StanceRustle),
        _ => None,
    }
}

/// What a teammate's moving body sounds like at `speed` m/s, and the seconds between each one: a
/// footstep, quicker on a sprint, or the drag of a Crawl, Prone or Down, on the walking cadence.
/// `None` while they make none: still, or in the air (a Dive in flight is Prone and airborne).
pub fn movement_cue(pose: &Pose, speed: f32) -> Option<(Cue, f32)> {
    if speed < TEAMMATE_STEP_MIN_SPEED || pose.airborne {
        None
    } else if pose.last_stand || pose.stance == Stance::Prone {
        Some((Cue::CrawlDrag, TEAMMATE_STEP_SECS))
    } else if pose.sprinting {
        Some((Cue::Footstep, TEAMMATE_SPRINT_STEP_SECS))
    } else {
        Some((Cue::Footstep, TEAMMATE_STEP_SECS))
    }
}

/// The floor family whose slot a footstep plays from: `family`'s own if it has files (`has_files`),
/// otherwise `None`, for the plain `footstep` slot.
pub fn footstep_family(family: Option<FloorFamily>, has_files: impl Fn(FloorFamily) -> bool) -> Option<FloorFamily> {
    family.filter(|&family| has_files(family))
}

/// The factor on `cue`'s volume when a teammate makes it: their rustle is heard at
/// `TEAMMATE_STANCE_RUSTLE_GAIN`, a touch over the Player's own; every other cue as it is.
pub fn teammate_gain(cue: Cue) -> f32 {
    match cue {
        Cue::StanceRustle => TEAMMATE_STANCE_RUSTLE_GAIN / Cue::StanceRustle.spec().gain,
        _ => 1.0,
    }
}

/// A point `roll` in `[0, 1)` of the way through `range`.
pub fn within(range: &RangeInclusive<f32>, roll: f32) -> f32 {
    range.start() + (range.end() - range.start()) * roll
}

/// A Zombie's idle vocal and the seconds between them, or `None` if it has none of its own here
/// (a Napalm, a Shrieker and the Sapper have theirs from their own modules).
pub fn idle_vocal(zombie: &Zombie) -> Option<(Cue, RangeInclusive<f32>)> {
    if zombie.is_hellhound() {
        Some((Cue::HoundIdle, HELLHOUND_IDLE_SECS))
    } else if zombie.is_nova_crawler() {
        Some((Cue::NovaVocal, NOVA_CRAWLER_IDLE_SECS))
    } else if zombie.is_special() {
        None
    } else if zombie.is_crawler() {
        Some((Cue::CrawlerVocal, CRAWLER_IDLE_SECS))
    } else {
        Some(match zombie.tier() {
            SpeedTier::Walker => (Cue::ZombieWalkerIdle, WALKER_IDLE_SECS),
            SpeedTier::Jogger => (Cue::ZombieJoggerIdle, JOGGER_IDLE_SECS),
            SpeedTier::Sprinter => (Cue::ZombieSprinterIdle, SPRINTER_IDLE_SECS),
        })
    }
}

/// Which idle vocals start and which stop.
#[derive(Debug, PartialEq)]
pub struct Voices<E> {
    pub start: Vec<E>,
    pub stop: Vec<E>,
}

/// The idle vocal cap: of the vocals `playing` and the Zombies `due` one, each with its distance
/// from the Player, the nearest `cap` are heard. Due Zombies among them start; playing vocals
/// left out stop, so a Zombie closing in is never drowned out by one far away.
pub fn choose_voices<E: Copy>(playing: &[(E, f32)], due: &[(E, f32)], cap: usize) -> Voices<E> {
    let mut all: Vec<(E, f32, bool)> = playing
        .iter()
        .map(|&(e, d)| (e, d, true))
        .chain(due.iter().map(|&(e, d)| (e, d, false)))
        .collect();
    // Ties go to what's already playing.
    all.sort_by(|a, b| a.1.total_cmp(&b.1).then(b.2.cmp(&a.2)));
    let (heard, cut) = all.split_at(all.len().min(cap));
    Voices {
        start: heard.iter().filter(|v| !v.2).map(|v| v.0).collect(),
        stop: cut.iter().filter(|v| v.2).map(|v| v.0).collect(),
    }
}

/// Which loops to start and stop: owners in `wanted` without a loop get one, and loops in
/// `playing`, as `(loop, owner)`, whose owner isn't wanted any more stop.
pub fn loop_changes<E: Copy + PartialEq>(wanted: &[E], playing: &[(E, E)]) -> (Vec<E>, Vec<E>) {
    let start = wanted.iter().filter(|w| !playing.iter().any(|(_, owner)| owner == *w)).copied().collect();
    let stop = playing.iter().filter(|(_, owner)| !wanted.contains(owner)).map(|(l, _)| *l).collect();
    (start, stop)
}

/// Where a Mystery Box is, as far as its sounds care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxPhase {
    Still,
    Spinning,
    Leaving,
    Away,
}

impl From<&BoxState> for BoxPhase {
    fn from(state: &BoxState) -> Self {
        match state {
            BoxState::Spinning { .. } => BoxPhase::Spinning,
            BoxState::Leaving { .. } => BoxPhase::Leaving,
            BoxState::Away { .. } => BoxPhase::Away,
            BoxState::Idle | BoxState::Offering { .. } | BoxState::Rearming { .. } => BoxPhase::Still,
        }
    }
}

/// The sound a Box makes going from `before` to `now`: the jingle as a spin starts, the laugh as
/// a Teddy Bear shows, and the whoosh-and-thud as it vanishes and again where it lands.
pub fn box_cue(before: BoxPhase, now: BoxPhase) -> Option<Cue> {
    match (before, now) {
        (before, now) if before == now => None,
        (_, BoxPhase::Spinning) => Some(Cue::BoxSpin),
        (_, BoxPhase::Leaving) => Some(Cue::BoxTeddy),
        (_, BoxPhase::Away) | (BoxPhase::Away, _) => Some(Cue::BoxMove),
        _ => None,
    }
}

/// The sound Pack-a-Punch makes going from `before` to `now`: its churn as a Weapon goes in, and
/// its ding once it's ready.
pub fn pap_cue(before: Option<Stage>, now: Option<Stage>) -> Option<Cue> {
    match (before, now) {
        (before, now) if before == now => None,
        (_, Some(Stage::Processing)) => Some(Cue::PapProcess),
        (_, Some(Stage::Ready)) => Some(Cue::PapReady),
        _ => None,
    }
}

/// A Perk machine's jingle.
pub fn jingle(perk: Perk) -> Cue {
    match perk {
        Perk::Juggernog => Cue::PerkJingleJuggernog,
        Perk::SpeedCola => Cue::PerkJingleSpeedCola,
        Perk::DoubleTap => Cue::PerkJingleDoubleTap,
        Perk::QuickRevive => Cue::PerkJingleQuickRevive,
    }
}

/// Seconds until a powered machine's next jingle, for a `roll` in `[0, 1)`.
pub fn next_jingle_secs(roll: f32) -> f32 {
    within(&PERK_JINGLE_SECS, roll)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::cues::Mix;

    const ALL_WEAPONS: [WeaponId; 28] = {
        let mut ids = [WeaponId::M1911; 28];
        let mut i = 0;
        while i < 28 {
            ids[i] = WEAPONS[i].id;
            i += 1;
        }
        ids
    };

    #[test]
    fn every_weapon_fires_its_class_gunshot() {
        assert_eq!(shot_cue(WeaponId::M1911, None).0, Cue::GunshotPistol);
        assert_eq!(shot_cue(WeaponId::Mp40, None).0, Cue::GunshotSmg);
        assert_eq!(shot_cue(WeaponId::M14, None).0, Cue::GunshotRifle);
        assert_eq!(shot_cue(WeaponId::TrenchGun, None).0, Cue::GunshotShotgun);
        assert_eq!(shot_cue(WeaponId::Rpk, None).0, Cue::GunshotLmg);
        assert_eq!(shot_cue(WeaponId::L96a1, None).0, Cue::GunshotSniper);
        assert_eq!(shot_cue(WeaponId::ChinaLake, None).0, Cue::GunshotLauncher);
        assert_eq!(shot_cue(WeaponId::RayGun, None).0, Cue::RayGun);
        assert_eq!(shot_cue(WeaponId::Thundergun, None).0, Cue::Thundergun);
    }

    #[test]
    fn a_weapon_with_its_own_clips_plays_them_and_one_without_falls_back_to_its_class() {
        let own = |id: WeaponId| id == WeaponId::M1911;
        assert_eq!(shot_sound(WeaponId::M1911, None, own), (Shot::Own(WeaponId::M1911), 1.0));
        assert_eq!(shot_sound(WeaponId::Cz75, None, own), (Shot::Cue(Cue::GunshotPistol), weapon_pitch(WeaponId::Cz75)));
        // The pitch offset only survives on the fallback.
        assert_ne!(weapon_pitch(WeaponId::Cz75), 1.0);
        assert_eq!(shot_sound(WeaponId::Cz75, None, |_| false), (Shot::Cue(Cue::GunshotPistol), weapon_pitch(WeaponId::Cz75)));
    }

    #[test]
    fn an_underbarrel_shot_ignores_the_weapons_own_clips() {
        let all = |_: WeaponId| true;
        assert_eq!(shot_sound(WeaponId::Commando, Some(Underbarrel::Masterkey), all), (Shot::Cue(Cue::GunshotShotgun), 1.0));
        assert_eq!(shot_sound(WeaponId::Commando, Some(Underbarrel::GrenadeLauncher), all), (Shot::Cue(Cue::GunshotLauncher), 1.0));
        assert_eq!(shot_sound(WeaponId::Commando, Some(Underbarrel::Flamethrower), all), (Shot::Cue(Cue::Flamethrower), 1.0));
    }

    #[test]
    fn an_own_clip_takes_its_class_gunshot_gain_and_voice_cap() {
        let shot = Shot::Own(WeaponId::Rpk);
        assert_eq!(shot_voice_cue(shot), Cue::GunshotLmg);
        assert_eq!(voice_cap(shot_voice_cue(shot)), Some(GUNSHOT_MAX_VOICES));
        assert_eq!(shot_voice_cue(Shot::Cue(Cue::RayGun)), Cue::RayGun);
        // A wonder weapon's own slot is uncapped, as its cue is.
        assert_eq!(voice_cap(shot_voice_cue(Shot::Own(WeaponId::RayGun))), None);
    }

    #[test]
    fn each_weapon_has_its_own_distinct_gunshot_slot() {
        let mut slots: Vec<_> = ALL_WEAPONS.iter().map(|&id| weapon_gunshot_slot(id)).collect();
        for (i, id) in ALL_WEAPONS.iter().enumerate() {
            assert_eq!(*id as usize, i);
            assert!(weapon_gunshot_slot(*id).starts_with("gunshot_"));
        }
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), ALL_WEAPONS.len());
        // And none is a cue's slot: they're optional.
        assert!(Cue::ALL.iter().all(|cue| !slots.contains(&cue.spec().name)));
    }

    #[test]
    fn the_underbarrels_borrow_the_shotgun_and_the_launcher() {
        assert_eq!(shot_cue(WeaponId::Commando, Some(Underbarrel::Masterkey)).0, Cue::GunshotShotgun);
        assert_eq!(shot_cue(WeaponId::Commando, Some(Underbarrel::GrenadeLauncher)).0, Cue::GunshotLauncher);
    }

    #[test]
    fn no_two_weapons_of_a_class_share_a_pitch_and_it_never_changes() {
        for a in ALL_WEAPONS {
            assert_eq!(weapon_pitch(a), weapon_pitch(a));
            assert!((0.8..=1.2).contains(&weapon_pitch(a)));
            for b in ALL_WEAPONS {
                if a != b && a.stats().class == b.stats().class {
                    assert_ne!(weapon_pitch(a), weapon_pitch(b), "{a:?} and {b:?}");
                }
            }
        }
    }

    #[test]
    fn a_teammates_hands_and_falls_make_their_sounds() {
        let plain = Pose::default();
        let doing = |action| Pose { action, ..plain };
        assert_eq!(pose_cue(&plain, &doing(Action::Reload)), Some(Cue::Reload));
        assert_eq!(pose_cue(&doing(Action::Reload), &doing(Action::Reload)), None, "once per reload");
        assert_eq!(pose_cue(&plain, &doing(Action::Knife)), Some(Cue::KnifeSwing));
        assert_eq!(pose_cue(&plain, &doing(Action::Drink)), Some(Cue::PerkDrink));
        assert_eq!(pose_cue(&doing(Action::Drink), &plain), None, "nothing as it ends");
        let down = Pose { last_stand: true, ..plain };
        assert_eq!(pose_cue(&plain, &down), Some(Cue::Down));
        assert_eq!(pose_cue(&down, &plain), Some(Cue::SelfRevive));
        assert_eq!(pose_cue(&down, &down), None);
    }

    #[test]
    fn footsteps_quicken_on_a_sprint_and_stop_when_still_or_airborne() {
        let grounded = Pose { airborne: false, ..Pose::default() };
        let (cue, walk) = movement_cue(&grounded, 3.0).unwrap();
        assert_eq!(cue, Cue::Footstep);
        let (cue, sprint) = movement_cue(&Pose { sprinting: true, ..grounded }, 6.0).unwrap();
        assert_eq!(cue, Cue::Footstep);
        assert!(sprint < walk);
        assert_eq!(movement_cue(&Pose { stance: Stance::Crouched, ..grounded }, 2.0), Some((Cue::Footstep, walk)));
        assert_eq!(movement_cue(&grounded, 0.0), None);
        assert_eq!(movement_cue(&Pose { airborne: true, ..grounded }, 3.0), None);
    }

    #[test]
    fn a_footstep_plays_its_floor_familys_slot_when_it_has_files_and_the_plain_one_otherwise() {
        let only_wood_and_tile = |family| matches!(family, FloorFamily::Wood | FloorFamily::Tile);
        assert_eq!(footstep_family(Some(FloorFamily::Wood), only_wood_and_tile), Some(FloorFamily::Wood));
        assert_eq!(footstep_family(Some(FloorFamily::Tile), only_wood_and_tile), Some(FloorFamily::Tile));
        assert_eq!(footstep_family(Some(FloorFamily::Metal), only_wood_and_tile), None);
        assert_eq!(footstep_family(None, only_wood_and_tile), None);
        assert_eq!(footstep_family(Some(FloorFamily::Wood), |_| false), None);
    }

    #[test]
    fn a_crawl_drags_on_the_footstep_cadence_prone_or_down() {
        let grounded = Pose { airborne: false, ..Pose::default() };
        let (_, walk) = movement_cue(&grounded, 3.0).unwrap();
        let prone = Pose { stance: Stance::Prone, ..grounded };
        let down = Pose { last_stand: true, ..grounded };
        assert_eq!(movement_cue(&prone, 1.0), Some((Cue::CrawlDrag, walk)));
        assert_eq!(movement_cue(&down, 1.0), Some((Cue::CrawlDrag, walk)));
        assert_eq!(movement_cue(&prone, 0.0), None, "a still crawler drags nothing");
        assert_eq!(movement_cue(&down, 0.0), None);
        // A Dive in flight is Prone and airborne: no drag until it lands.
        assert_eq!(movement_cue(&Pose { airborne: true, ..prone }, 6.0), None);
        assert_eq!(Cue::CrawlDrag.spec().mix, Mix::Spatial);
    }

    #[test]
    fn a_teammates_stance_changes_rustle_and_their_dive_lands_with_a_thud() {
        let at = |stance| Pose { stance, airborne: false, ..Pose::default() };
        let (standing, crouched, prone) = (at(Stance::Standing), at(Stance::Crouched), at(Stance::Prone));
        let cue = |before: Pose, now: Pose| stance_cue(before.feel(), now.feel());
        assert_eq!(cue(standing, crouched), Some(Cue::StanceRustle));
        assert_eq!(cue(crouched, standing), Some(Cue::StanceRustle), "both ways");
        assert_eq!(cue(standing, prone), Some(Cue::StanceRustle));
        assert_eq!(cue(prone, prone), None);
        // A Dive: Prone and airborne in flight, silent at the launch, a thud at the landing alone.
        let flying = Pose { airborne: true, ..prone };
        assert_eq!(cue(Pose { sprinting: true, ..standing }, flying), None, "the launch made a sound");
        assert_eq!(cue(flying, flying), None);
        assert_eq!(cue(flying, prone), Some(Cue::DiveLand));
        // A jump isn't a Dive.
        assert_eq!(cue(standing, Pose { airborne: true, ..standing }), None);
        // Going Down and getting back up have their own sounds.
        let down = Pose { last_stand: true, ..prone };
        assert_eq!(cue(standing, down), None);
        assert_eq!(cue(down, standing), None);
        assert_eq!(cue(flying, down), None, "a Down mid-Dive thudded");
    }

    #[test]
    fn a_teammate_riding_a_zipline_is_heard_on_the_cable_and_on_landing() {
        let plain = Pose::default();
        let riding = Pose { action: Action::Ride, ..plain };
        assert_eq!(pose_cue(&plain, &riding), Some(Cue::ZiplineRide));
        assert_eq!(pose_cue(&riding, &riding), None);
        assert_eq!(pose_cue(&riding, &plain), Some(Cue::ZiplineLand));
    }

    #[test]
    fn a_teammates_rustle_is_a_touch_louder_than_the_players_own() {
        assert!((Cue::StanceRustle.spec().gain * teammate_gain(Cue::StanceRustle) - TEAMMATE_STANCE_RUSTLE_GAIN).abs() < 1e-6);
        assert_eq!(teammate_gain(Cue::DiveLand), 1.0);
        assert_eq!(teammate_gain(Cue::CrawlDrag), 1.0);
    }

    #[test]
    fn only_gunshots_are_capped_and_the_oldest_voices_go_first() {
        assert_eq!(voice_cap(Cue::GunshotPistol), Some(GUNSHOT_MAX_VOICES));
        assert_eq!(voice_cap(Cue::Hit), None);
        let live = [('c', 7), ('a', 3), ('d', 9), ('b', 5)];
        assert_eq!(voices_to_cut(&live, 4), Vec::<char>::new());
        assert_eq!(voices_to_cut(&live, 3), vec!['a']);
        assert_eq!(voices_to_cut(&live, 2), vec!['a', 'b']);
    }

    #[test]
    fn idle_vocals_go_to_the_nearest_six() {
        let due: Vec<(u32, f32)> = (0..10).map(|i| (i, 10.0 - i as f32)).collect();
        let voices = choose_voices(&[], &due, IDLE_VOCAL_CAP);
        let mut start = voices.start.clone();
        start.sort_unstable();
        assert_eq!(start, vec![4, 5, 6, 7, 8, 9], "the six nearest");
        assert!(voices.stop.is_empty());
    }

    #[test]
    fn a_closer_zombie_takes_the_farthest_voice() {
        let playing: Vec<(u32, f32)> = (0..6).map(|i| (i, 5.0 + i as f32)).collect();
        let voices = choose_voices(&playing, &[(99, 1.0), (100, 50.0)], IDLE_VOCAL_CAP);
        assert_eq!(voices, Voices { start: vec![99], stop: vec![5] });
    }

    #[test]
    fn with_voices_free_every_due_zombie_is_heard() {
        let voices = choose_voices(&[(1, 30.0)], &[(2, 40.0), (3, 2.0)], IDLE_VOCAL_CAP);
        assert_eq!(voices.start, vec![3, 2]);
        assert!(voices.stop.is_empty());
    }

    #[test]
    fn never_more_than_the_cap_at_once() {
        let playing: Vec<(u32, f32)> = (0..6).map(|i| (i, i as f32)).collect();
        let due: Vec<(u32, f32)> = (6..20).map(|i| (i, i as f32 * 0.5)).collect();
        let voices = choose_voices(&playing, &due, IDLE_VOCAL_CAP);
        assert_eq!(playing.len() - voices.stop.len() + voices.start.len(), IDLE_VOCAL_CAP);
    }

    #[test]
    fn loops_follow_their_owners() {
        // Owners 1 and 2 want loops; 2 has one (loop 20), 3's loop (30) is no longer wanted.
        let (start, stop) = loop_changes(&[1, 2], &[(20, 2), (30, 3)]);
        assert_eq!(start, vec![1]);
        assert_eq!(stop, vec![30]);
    }

    #[test]
    fn the_box_jingles_laughs_and_moves() {
        use BoxPhase::*;
        assert_eq!(box_cue(Still, Spinning), Some(Cue::BoxSpin));
        assert_eq!(box_cue(Spinning, Spinning), None);
        assert_eq!(box_cue(Spinning, Still), None, "a Prize comes up quietly");
        assert_eq!(box_cue(Spinning, Leaving), Some(Cue::BoxTeddy));
        assert_eq!(box_cue(Leaving, Away), Some(Cue::BoxMove));
        assert_eq!(box_cue(Away, Still), Some(Cue::BoxMove), "and again where it lands");
    }

    #[test]
    fn pack_a_punch_churns_then_dings() {
        assert_eq!(pap_cue(None, Some(Stage::Processing)), Some(Cue::PapProcess));
        assert_eq!(pap_cue(Some(Stage::Processing), Some(Stage::Processing)), None);
        assert_eq!(pap_cue(Some(Stage::Processing), Some(Stage::Ready)), Some(Cue::PapReady));
        assert_eq!(pap_cue(Some(Stage::Ready), None), None);
    }

    #[test]
    fn perk_jingles_come_every_two_to_four_minutes() {
        assert_eq!(next_jingle_secs(0.0), 120.0);
        assert!((next_jingle_secs(0.999) - 240.0).abs() < 0.5);
        let cues: Vec<Cue> = Perk::ALL.iter().map(|&perk| jingle(perk)).collect();
        for (i, a) in cues.iter().enumerate() {
            assert!(cues[i + 1..].iter().all(|b| b != a), "each machine its own ditty");
        }
    }

    #[test]
    fn each_kind_of_zombie_has_its_own_idle_vocal() {
        let napalm = Zombie::special(crate::enemies::special::SpecialKind::Napalm, 100.0, bevy::math::Vec3::ZERO);
        assert!(idle_vocal(&napalm).is_none(), "the Napalm crackles on its own loop");
        let hound = Zombie::hellhound(100.0, bevy::math::Vec3::ZERO);
        assert_eq!(idle_vocal(&hound).map(|v| v.0), Some(Cue::HoundIdle));
        let nova = Zombie::nova_crawler(100.0, bevy::math::Vec3::ZERO);
        assert_eq!(idle_vocal(&nova).map(|v| v.0), Some(Cue::NovaVocal));
    }
}
