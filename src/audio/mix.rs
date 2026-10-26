//! The mix rules: which cue an event plays and at what pitch, which idle vocals get a voice,
//! which loops start and stop, and when a Perk machine sings. Plain functions, tested without an
//! `App`.

use std::ops::RangeInclusive;

use crate::arsenal::{WeaponClass, WeaponId};
use crate::attachment::Underbarrel;
use crate::mystery_box::BoxState;
use crate::pack_a_punch::Stage;
use crate::perk::Perk;
use crate::tuning::*;
use crate::zombie::{SpeedTier, Zombie};

use super::cues::Cue;

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
        let napalm = Zombie::special(crate::special::SpecialKind::Napalm, 100.0, bevy::math::Vec3::ZERO);
        assert!(idle_vocal(&napalm).is_none(), "the Napalm crackles on its own loop");
        let hound = Zombie::hellhound(100.0, bevy::math::Vec3::ZERO);
        assert_eq!(idle_vocal(&hound).map(|v| v.0), Some(Cue::HoundIdle));
        let nova = Zombie::nova_crawler(100.0, bevy::math::Vec3::ZERO);
        assert_eq!(idle_vocal(&nova).map(|v| v.0), Some(Cue::NovaVocal));
    }
}
