//! Every Gauntlet system's worst case fits the Co-op snapshot (issue #390). A snapshot tells of at
//! most `SNAPSHOT_MAX_CREATURES` Zombies and Hellhounds between them and `SNAPSHOT_MAX_SPECIALS`
//! Special Zombies, and `net::state`'s tests hold that a snapshot that full fits one datagram.
//! Whatever is alive past those caps vanishes for clients, so each system that adds to what can be
//! alive at once is counted here at its worst.
//!
//! The Horde is held under the Alive Cap, which counts every Zombie and Hellhound alive whatever
//! sent it (the Horde, the special slot, the Sapper, a Generator Surge), so the special slot's and
//! the Sapper's Zombies and Classic's mixed hounds are inside it. Only `SpawnZombie` requests come
//! outside it: a Splitter Elite's Walkers and a Boss with its pack.

use super::arc::{BossKind, pack_to_call};
use super::elite::{EliteRules, becomes_elite_under, split_requests};
use super::heat::{Condition, RunHeat};
use super::pact::PACTS;
use super::variety::{Twist, twist_entries};
use crate::enemies::zombie::ZombieKind;
use crate::modifier::{Modifier, Source, Stat, resolve_count};
use crate::tuning::*;

/// Every Alive Cap entry a Run can hold at once: each Pact's (Overkill's), Swarm's while its Round
/// is on, and every Heat Condition's at its top Degree.
fn every_alive_cap_entry() -> Vec<Modifier> {
    let pacts = PACTS.iter().flat_map(|pact| {
        pact.entries.iter().map(|&(stat, effect)| Modifier { stat, source: Source(pact.id()), effect })
    });
    let mut heat = RunHeat::default();
    for condition in Condition::ALL {
        heat.0[condition as usize] = condition.max_degree();
    }
    pacts
        .chain(twist_entries(Twist::Swarm, true))
        .chain(heat.entries())
        .filter(|entry| entry.stat == Stat::AliveCap)
        .collect()
}

/// The biggest the Alive Cap gets: its top end with every entry that raises it.
fn biggest_alive_cap() -> u32 {
    resolve_count(Stat::AliveCap, ALIVE_CAP_MAX.max(ALIVE_CAP_COOP), &every_alive_cap_entry())
}

/// The most Elites alive at once, under any Degree of Elite Surge.
fn most_elites_alive() -> u32 {
    let surge = GAUNTLET_HEAT_ELITE_SURGE_MAX_ALIVE.iter().copied().max().unwrap_or(0);
    EliteRules::default().max_alive.max(surge)
}

/// The Walkers one Splitter splits into.
fn walkers_per_split() -> u32 {
    split_requests(10, 1000.0, bevy::math::Vec3::ZERO).len() as u32
}

/// How far Splitters can take what's alive past the Alive Cap: each dying Elite takes one off and
/// puts its Walkers on, outside the cap, but a new Elite only comes from the Horde while there is
/// room under it. So the alive count stays within the cap plus (Walkers − 1) for each Elite that
/// can be alive at once (see `splitters_never_pass_the_cap_by_more_than_their_overflow`).
fn splitter_overflow() -> u32 {
    walkers_per_split().saturating_sub(1) * most_elites_alive()
}

/// The most a Boss Round's Boss brings outside the Horde: itself and, for the Pack Alpha, its pack.
fn boss_extras(kind: BossKind) -> u32 {
    match kind {
        BossKind::PackAlpha => 1 + GAUNTLET_PACK_ALPHA_MAX_HOUNDS,
        BossKind::Bloater | BossKind::Warden => 1,
    }
}

#[test]
fn swarm_and_overkill_together_fit() {
    assert_eq!(biggest_alive_cap(), ALIVE_CAP_MAX + PACT_OVERKILL_ALIVE_CAP_BONUS + GAUNTLET_SWARM_ALIVE_CAP_BONUS);
    assert!(biggest_alive_cap() as usize <= SNAPSHOT_MAX_CREATURES, "{}", biggest_alive_cap());
}

#[test]
fn elites_splitting_at_the_biggest_alive_cap_fit() {
    let worst = biggest_alive_cap() + splitter_overflow();
    assert!(worst as usize <= SNAPSHOT_MAX_CREATURES, "{worst}");
}

#[test]
fn each_boss_round_at_its_worst_fits() {
    for kind in [BossKind::Bloater, BossKind::PackAlpha, BossKind::Warden] {
        let worst = biggest_alive_cap() + splitter_overflow() + boss_extras(kind);
        assert!(worst as usize <= SNAPSHOT_MAX_CREATURES, "{kind:?}: {worst}");
    }
}

/// The Pack Alpha never calls more than its pack's most, howl after howl.
#[test]
fn the_pack_never_passes_its_most() {
    let mut alive = 0;
    for _ in 0..10 {
        alive += pack_to_call(alive);
        assert!(alive <= GAUNTLET_PACK_ALPHA_MAX_HOUNDS, "{alive}");
    }
    assert_eq!(alive, GAUNTLET_PACK_ALPHA_MAX_HOUNDS);
}

/// The Special Zombies alive at once: the special slot's one (never another while it lives), the
/// Sapper, a Boss (the Bloater is a Napalm and the Warden a Sapper) and the most Nova Crawlers.
#[test]
fn the_most_special_zombies_fit() {
    let worst = 1 + 1 + 1 + NOVA_CRAWLER_MAX_ALIVE as usize;
    assert!(worst <= SNAPSHOT_MAX_SPECIALS, "{worst}");
}

/// A model of the Horde at the Alive Cap with Splitters, played adversarially: the Horde fills
/// every gap with an Elite whenever the rules allow one, and Elites die whenever they can. What's
/// alive never passes the cap by more than `splitter_overflow`.
#[test]
fn splitters_never_pass_the_cap_by_more_than_their_overflow() {
    let rules = EliteRules { max_alive: most_elites_alive(), ..EliteRules::default() };
    let cap = biggest_alive_cap();
    let (mut others, mut elites) = (0u32, 0u32);
    let mut most = 0;
    for step in 0..10_000u32 {
        // The Horde spawns while there's room, an Elite whenever one may be.
        while others + elites < cap {
            if becomes_elite_under(100, ZombieKind::Normal, false, elites, 0.0, &rules) {
                elites += 1;
            } else {
                others += 1;
            }
        }
        // Then some die: every Elite at once on some steps, one Walker on others.
        if step % 3 == 0 {
            others += elites * walkers_per_split();
            elites = 0;
        } else {
            others = others.saturating_sub(1 + step % 4);
        }
        most = most.max(others + elites);
    }
    assert_eq!(most, cap + splitter_overflow());
}
