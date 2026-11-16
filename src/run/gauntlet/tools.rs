//! Offer tools: Reroll and Banish on Boon and Boss Offers. A Reroll swaps all the cards for Points,
//! its price doubling within the Offer. A Banish spends a charge to remove one card's Boon from
//! the Offer and from the pool for the rest of the Run, and its slot redraws at once. Pact Offers
//! take neither.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::arc::{draw_boss_cards, is_boss_round};
use super::boon::{BOONS, BoonDef, Family, HeldBoons, Rarity, RunFacts};
use super::duos::DUOS;
use super::heat::RunHeat;
use super::kits::PlayerKit;
use super::pact::HeldPacts;
use super::variety::{Boost, draw_boosted, draw_cards_boosted, guarantee_families};
use super::{Bought, Choices, Offer, PlayerUnlocks, UnlockAll};
use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::economy::points::{Points, SpendPoints};
use crate::maps::power::Supply;
use crate::players::player::{LocalPlayer, Player};
use crate::run::round::{Round, RoundEnded};
use crate::state::Choosing;
use crate::tuning::*;
use crate::weapons::weapon::Loadout;

/// A Player's Banish charges, and the Boons they have Banished this Run.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Banishes {
    pub charges: u32,
    /// The stable IDs of the Banished Boons, never offered to this Player again.
    pub banished: Vec<&'static str>,
}

impl Default for Banishes {
    fn default() -> Self {
        Self::starting(0)
    }
}

impl Banishes {
    /// A Run's starting charges: the usual one, plus `track` from the Banish Track's Ranks.
    pub fn starting(track: u32) -> Self {
        Self::starting_from(GAUNTLET_BANISHES_START, track)
    }

    /// A Run's starting charges from a `base` (Tight Offers lowers it) plus `track`.
    pub fn starting_from(base: u32, track: u32) -> Self {
        Self { charges: base.saturating_add(track), banished: Vec::new() }
    }
}

/// A Player's Reroll or Banish went through (it was affordable and drew something).
#[derive(Message, Debug, Clone, Copy)]
pub struct ToolUsed {
    pub player: Entity,
}

/// The Player asks to Reroll their open Boon or Boss Offer.
#[derive(Message, Debug, Clone, Copy)]
pub struct RerollOffer {
    pub player: Entity,
}

/// The Player asks to Banish the card at `card` of their open Boon or Boss Offer.
#[derive(Message, Debug, Clone, Copy)]
pub struct BanishCard {
    pub player: Entity,
    pub card: usize,
}

/// A Reroll's price when `rerolls` have already been made in this Offer. It doubles each time and
/// starts over at the next Offer.
#[cfg(test)]
pub fn reroll_price(rerolls: u32) -> u64 {
    reroll_price_from(GAUNTLET_REROLL_BASE_PRICE, rerolls)
}

/// `reroll_price` from a `base` price (Tight Offers raises it). `Price` never scales it.
pub fn reroll_price_from(base: u64, rerolls: u32) -> u64 {
    base.saturating_mul(2u64.saturating_pow(rerolls))
}

/// Whether `cards` are a Boss Offer's: on a Boss Round, and all Legendary.
fn is_boss_offer(round: u32, cards: &[&'static BoonDef]) -> bool {
    is_boss_round(round) && cards.iter().all(|card| card.rarity == Rarity::Legendary)
}

/// `cards` with the one at `index` Banished: its slot redraws from what `run` and `held` allow,
/// never repeating a card still on the Offer, and a Boss Offer keeps a Duo if it can. If nothing
/// is left to draw, the slot is simply gone.
pub fn banish(
    cards: &[&'static BoonDef],
    index: usize,
    round: u32,
    held: &HeldBoons,
    run: &RunFacts,
    boost: &Boost,
    mut roll: impl FnMut(f64) -> f64,
) -> Vec<&'static BoonDef> {
    let Some(&gone) = cards.get(index) else {
        return cards.to_vec();
    };
    let mut remaining = cards.to_vec();
    remaining.remove(index);
    // The Banished Boon and the cards still showing are out of this draw.
    let mut run = run.clone();
    run.banished.push(gone.id);
    run.banished.extend(remaining.iter().map(|card| card.id));
    let fresh = if is_boss_offer(round, cards) {
        let guarantee = !remaining.iter().any(|card| card.duo.is_some());
        draw_boss_cards(BOONS, DUOS, held, &run, 1, guarantee, &mut roll)
    } else {
        // The redrawn slot keeps the Offer's Rarity bump.
        let slot = Boost { families: Vec::new(), ..boost.clone() };
        draw_boosted(BOONS, held, &run, 1, &slot, &mut roll)
    };
    match fresh.first() {
        Some(&card) => {
            remaining.insert(index, card);
            // A Family's guaranteed card may have been the one Banished.
            guarantee_families(remaining, boost, is_boss_offer(round, cards), BOONS, held, &run, roll)
        }
        None => remaining,
    }
}

/// Everything an Offer's draw reads from the Run.
#[derive(SystemParam)]
pub struct RunSources<'w> {
    pacts: Res<'w, HeldPacts>,
    unlock_all: Res<'w, UnlockAll>,
    supply: Supply<'w>,
}

impl RunSources<'_> {
    /// The Run as one Player's Offers see it.
    pub fn facts(&self, bought: &Bought, loadout: &Loadout, held: &HeldBoons, unlocks: &PlayerUnlocks, banished: &[&'static str], kit: Option<Family>) -> RunFacts {
        RunFacts {
            power_on: self.supply.has_been_on(),
            held_pacts: self.pacts.ids(),
            used_box: bought.from_box,
            bought_quick_revive: bought.quick_revive,
            holds_upgraded: loadout.weapons().any(|weapon| weapon.upgraded),
            holds_ammo_mod: loadout.weapons().any(|weapon| !weapon.ammo_mods.is_empty()),
            holds_proc_boon: held.0.iter().any(|(boon, _)| boon.proc.is_some()),
            unlocked: unlocks.0.clone(),
            unlock_all: self.unlock_all.0,
            kit,
            banished: banished.to_vec(),
        }
    }
}

/// A cleared Boss Round gives every Player a Banish charge, and its Offer is the first to use it.
pub fn grant_banishes(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    mut players: Query<&mut Banishes, With<Player>>,
) {
    if ended.read().count() == 0 || !is_boss_round(round.number) {
        return;
    }
    for mut banishes in &mut players {
        banishes.charges = banishes.charges.saturating_add(GAUNTLET_BANISHES_PER_BOSS);
    }
}

/// A Reroll swaps every card of the open Boon or Boss Offer for a fresh draw, for Points through
/// `SpendPoints`. Refused if the Player can't afford it; a Pact Offer can't be Rerolled.
pub fn apply_reroll(
    mut requests: MessageReader<RerollOffer>,
    mut players: Query<(&mut Offer, &HeldBoons, &PlayerUnlocks, &Bought, &Loadout, &Banishes, &Points, &PlayerKit, Has<LocalPlayer>)>,
    sources: RunSources,
    real: Res<Time<Real>>,
    mut spends: MessageWriter<SpendPoints>,
    mut cues: MessageWriter<PlayCue>,
    mut used: MessageWriter<ToolUsed>,
    heat: Res<RunHeat>,
) {
    // Points already asked for this frame, so two Rerolls can't be paid for with one balance.
    let mut committed: Vec<(Entity, u64)> = Vec::new();
    for &RerollOffer { player } in requests.read() {
        let Ok((mut offer, held, unlocks, bought, loadout, banishes, points, kit, local)) = players.get_mut(player) else {
            continue;
        };
        // After an extra pick the Offer has already paid out; it can't be Rerolled back to full.
        if !matches!(offer.choices, Choices::Boons(_)) || offer.picked > 0 {
            continue;
        }
        let price = reroll_price_from(heat.reroll_base_price(), offer.rerolls);
        let owed = committed.iter().filter(|(who, _)| *who == player).map(|(_, owed)| owed).sum::<u64>();
        if points.balance.saturating_sub(owed) < price {
            continue;
        }
        let run = sources.facts(bought, loadout, held, unlocks, &banishes.banished, kit.kit.family());
        let cards = draw_cards_boosted(offer.round, held, &run, &offer.boost, |total| rand::random_range(0.0..total));
        if cards.is_empty() {
            continue;
        }
        committed.push((player, price));
        spends.write(SpendPoints { player, amount: price });
        used.write(ToolUsed { player });
        offer.choices = Choices::Boons(cards);
        offer.rerolls += 1;
        offer.shown_at = real.elapsed_secs();
        if local {
            cues.write(PlayCue::new(Cue::BoonOffer, None));
        }
    }
}

/// A Banish spends a charge to remove one card's Boon from the Offer and the Run's pool, and its
/// slot redraws. With nothing left to offer the Offer closes. A Pact Offer can't Banish.
pub fn apply_banish(
    mut commands: Commands,
    mut requests: MessageReader<BanishCard>,
    mut players: Query<(&mut Offer, &HeldBoons, &PlayerUnlocks, &Bought, &Loadout, &mut Banishes, &PlayerKit, Has<LocalPlayer>)>,
    sources: RunSources,
    real: Res<Time<Real>>,
    mut cues: MessageWriter<PlayCue>,
    mut used: MessageWriter<ToolUsed>,
) {
    for &BanishCard { player, card } in requests.read() {
        let Ok((mut offer, held, unlocks, bought, loadout, mut banishes, kit, local)) = players.get_mut(player) else {
            continue;
        };
        let Choices::Boons(cards) = &offer.choices else {
            continue;
        };
        // After an extra pick the Offer has already paid out; it can't be Banished back to full.
        if offer.picked > 0 {
            continue;
        }
        let Some(&gone) = cards.get(card) else {
            continue;
        };
        if banishes.charges == 0 {
            continue;
        }
        let run = sources.facts(bought, loadout, held, unlocks, &banishes.banished, kit.kit.family());
        let cards = banish(cards, card, offer.round, held, &run, &offer.boost, |total| rand::random_range(0.0..total));
        banishes.charges -= 1;
        used.write(ToolUsed { player });
        if !banishes.banished.contains(&gone.id) {
            banishes.banished.push(gone.id);
        }
        if local {
            cues.write(PlayCue::new(Cue::OfferPick, None));
        }
        if cards.is_empty() {
            commands.entity(player).remove::<(Offer, Choosing)>();
        } else {
            offer.choices = Choices::Boons(cards);
            offer.shown_at = real.elapsed_secs();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::gauntlet::boon::{HeldBoons, draw_offer, find};

    fn everything() -> RunFacts {
        RunFacts { unlock_all: true, ..default() }
    }

    fn seeded(seed: u64) -> impl FnMut(f64) -> f64 {
        let mut n = seed;
        move |total| {
            n = n.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (n >> 11) as f64 / (1u64 << 53) as f64 * total
        }
    }

    fn holding(ids: &[&str]) -> HeldBoons {
        let mut held = HeldBoons::default();
        for id in ids {
            held.add(find(id).unwrap());
        }
        held
    }

    #[test]
    fn a_rerolls_price_doubles_within_the_offer() {
        assert_eq!(reroll_price(0), 1000);
        assert_eq!(reroll_price(1), 2000);
        assert_eq!(reroll_price(2), 4000);
        assert_eq!(reroll_price(5), 32_000);
        assert_eq!(reroll_price(200), u64::MAX, "never wraps");
    }

    #[test]
    fn a_banished_boon_is_never_drawn_again() {
        let mut run = everything();
        let all = draw_offer(BOONS, &HeldBoons::default(), &run, seeded(1));
        run.banished.push(all[0].id);
        for seed in 0..50 {
            let drawn = draw_offer(BOONS, &HeldBoons::default(), &run, seeded(seed));
            assert!(drawn.iter().all(|boon| boon.id != all[0].id));
        }
    }

    #[test]
    fn a_banish_redraws_its_slot_without_repeating_a_card() {
        let run = everything();
        for seed in 0..50 {
            let cards = draw_offer(BOONS, &HeldBoons::default(), &run, seeded(seed));
            let after = banish(&cards, 1, 4, &HeldBoons::default(), &run, &Boost::default(), seeded(seed + 100));
            assert_eq!(after.len(), BOON_OFFER_SIZE);
            assert_eq!((after[0].id, after[2].id), (cards[0].id, cards[2].id), "the other slots stay");
            assert_ne!(after[1].id, cards[1].id, "the Banished Boon is gone");
            let mut ids: Vec<_> = after.iter().map(|boon| boon.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), 3, "no card twice");
        }
    }

    #[test]
    fn a_banish_with_nothing_left_to_draw_shrinks_the_offer() {
        // Only the three Boons on the Offer are Unlocked: banishing one leaves two cards.
        let mut run = RunFacts::default();
        let cards: Vec<_> = BOONS.iter().filter(|boon| boon.starts_unlocked).take(3).collect();
        run.banished = BOONS.iter().filter(|boon| !cards.iter().any(|card| card.id == boon.id)).map(|boon| boon.id).collect();
        let after = banish(&cards.to_vec(), 0, 4, &HeldBoons::default(), &run, &Boost::default(), seeded(3));
        assert_eq!(after.len(), 2);
        assert!(after.iter().all(|boon| boon.id != cards[0].id));
    }

    #[test]
    fn a_boss_offer_banish_stays_legendary_and_keeps_a_duo() {
        let held = holding(&["marksmans_eye", "trench_raider"]);
        let run = everything();
        for seed in 0..30 {
            let cards = draw_cards_boosted(10, &held, &run, &Boost::default(), seeded(seed));
            assert!(cards.iter().all(|boon| boon.rarity == Rarity::Legendary));
            // Banishing the guaranteed Duo: the guarantee skips it (it was the only one).
            let duo = cards.iter().position(|boon| boon.duo.is_some()).expect("a guaranteed Duo");
            let after = banish(&cards, duo, 10, &held, &run, &Boost::default(), seeded(seed + 7));
            assert!(after.iter().all(|boon| boon.rarity == Rarity::Legendary));
            assert!(after.iter().all(|boon| boon.id != cards[duo].id), "a Banished Duo is out");
            // Banishing another card keeps the Duo and redraws a Legendary.
            let other = (duo + 1) % cards.len();
            let kept = banish(&cards, other, 10, &held, &run, &Boost::default(), seeded(seed + 9));
            assert!(kept.iter().any(|boon| boon.id == cards[duo].id));
            assert!(kept.iter().all(|boon| boon.rarity == Rarity::Legendary));
        }
    }

    #[test]
    fn a_banished_recipe_boon_still_counts_toward_its_duo() {
        let held = holding(&["marksmans_eye", "trench_raider"]);
        let mut run = everything();
        run.banished = vec!["marksmans_eye"];
        let duos = super::super::duos::duo_pool(DUOS, &held, &run);
        assert!(duos.iter().any(|duo| duo.id == "gun_and_blade"));
        run.banished.push("gun_and_blade");
        assert!(super::super::duos::duo_pool(DUOS, &held, &run).iter().all(|duo| duo.id != "gun_and_blade"));
    }

    #[test]
    fn a_reroll_of_a_boss_offer_keeps_its_duo_slot() {
        let held = holding(&["marksmans_eye", "trench_raider"]);
        for seed in 0..30 {
            let cards = draw_cards_boosted(20, &held, &everything(), &Boost::default(), seeded(seed));
            assert_eq!(cards[0].id, "gun_and_blade");
        }
    }
}
