//! Gauntlet: Classic with a Boon Offer after every Round, and a Pact Offer in its place every
//! fifth Round. Between Runs, the Scrip each Run pays buys Unlocks and Upgrade Track Ranks in the
//! Armory, kept in `gauntlet.ron`. It's layered on top of Classic and
//! reaches it only through seams (ADR 0002): stat-modifier entries on the Player, messages Classic
//! already sends, Classic state it reads, and the `Freeze` and `IntermissionHold` it holds. Every
//! system here that touches a Run runs only when `Mode == Gauntlet`; the Armory and the save file
//! live in the menus.

mod armory;
mod boon;
mod duos;
mod pact;
mod procs;
pub mod progress;
mod screen;
mod sets;
#[cfg(test)]
mod coop_tests;
#[cfg(test)]
mod sets_tests;
#[cfg(test)]
mod duos_tests;

use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::economy::buy::Purchased;
use crate::weapons::explosive::Explosion;
use crate::players::last_stand::LastStand;
use crate::ui::menu::{GameOverLines, ResetGauntletProgress};
use crate::run::mode::Mode;
use crate::modifier::{Modifiers, RunModifiers};
use crate::weapons::mystery_box::{MysteryBox, TeddyBear};
use crate::economy::perk::{Perk, PerkMachine};
use crate::players::player::{LocalPlayer, Player, Seat};
use crate::players::respawn::{BledOut, Disconnected, SeatDropped};
use crate::economy::points::BonusPoints;
use crate::maps::power::Supply;
use crate::economy::power_up::{DropPowerUp, GivePowerUp, PowerUp, PowerUpTaken};
use crate::storage::records::{RecordExtra, RecordExtras};
use crate::run::round::{IntermissionHold, Round, RoundEnded, RoundPhase};
use crate::storage::save;
use crate::storage::settings::SaveDir;
use crate::state::{Choosing, Freeze, GameState, MainMenuScreen, Pause, RunType, world_runs};
use crate::tuning::*;
use crate::weapons::ammo_mod::TurnedAllyCap;
use crate::weapons::weapon::Loadout;
use crate::enemies::zombie::{DamageZombie, Dying, HitZone, KillDamage, Zombie, ZombieKilled};
use boon::{
    BLOOD_MONEY, BOONS, BOUNTY, BUTCHERS_BILL, BoonDef, LUCKY_STRIKE, RunFacts,
    TEDDYS_TITHE, WAR_BONDS, blood_money_points, bounty_points, butchers_bill_drops, draw_offer,
    is_boon_source, lucky_strike_gift, teddys_tithe_points, war_bonds_points,
};
use pact::{
    OVERKILL, PACTS, PactDef, SCORCHED_EARTH, draw_pact_offer, is_pact_round,
    is_pact_source, overkill_target, scorched_earth_blast,
};
use progress::{GauntletFile, rounds_completed, scrip_for_rounds};
pub use boon::HeldBoons;
pub use pact::HeldPacts;
use screen::{OfferCard, SkipButton};

/// The Offers a Player is waiting for.
#[derive(Component, Default)]
struct PendingOffer {
    /// Seconds left before the next one opens, counted from the Round's last kill. `None` once
    /// there's nothing owed.
    left: Option<f32>,
    /// The Rounds whose Offers the Player hasn't been shown yet, oldest first. A Round's Offer
    /// waits behind one still open, and one withdrawn by a Bleed Out goes back to the front.
    owed: Vec<u32>,
}

/// The Boons and Pacts this Player bought in the Armory, which their Offers are drawn from. On
/// the Host it's filled from the local save for every Player; once the netcode lands, a remote
/// Player's is filled from the `Unlocks` in their `Hello` instead.
#[derive(Component, Default, Clone, Debug, PartialEq, Eq)]
pub struct PlayerUnlocks(pub Vec<String>);

/// What the Player has bought this Run that Requirements ask about, from the Purchased messages
/// Classic sends.
#[derive(Component, Default)]
struct Bought {
    from_box: bool,
    quick_revive: bool,
}

/// Power-ups Lucky Strike has given, with the holder they were given to, whose pickup hasn't come
/// back yet, so a gift never rolls for another.
#[derive(Resource, Default)]
struct Gifts(Vec<(Entity, PowerUp)>);

/// Seconds left of the Co-op Intermission's wait for every Player's pick.
#[derive(Resource, Default)]
struct OfferWait(f32);

/// A Player picked a card of their open Offer, or `None` to skip a Pact Offer. The screen sends it
/// for the `LocalPlayer`; the netcode will for a remote one.
#[derive(Message, Debug, Clone, Copy)]
pub struct PickOffer {
    pub player: Entity,
    pub card: Option<usize>,
}

/// Debug: every Boon and Pact counts as Unlocked, toggled with F9 during a Gauntlet Run. It
/// changes nothing in `gauntlet.ron`.
#[derive(Resource, Default)]
struct UnlockAll(bool);

/// The Upgrade Track Ranks switched on as this Run began (0 to 25), for its Run Record.
#[derive(Resource, Default)]
struct ActiveRanks(u32);

/// The Run Record line holding the active Rank count.
const RANKS_RECORD_ID: &str = "upgrade_ranks";

/// What an Offer's cards hold.
enum Choices {
    /// A Boon Offer: one must be picked.
    Boons(Vec<&'static BoonDef>),
    /// A Pact Offer: one may be picked, or the Offer skipped.
    Pacts(Vec<&'static PactDef>),
}

impl Choices {
    fn is_empty(&self) -> bool {
        match self {
            Choices::Boons(boons) => boons.is_empty(),
            Choices::Pacts(pacts) => pacts.is_empty(),
        }
    }
}

/// The Boon or Pact Offer a Player is choosing from. Quitting the Run loses it.
#[derive(Component)]
pub struct Offer {
    choices: Choices,
    /// The Round just completed.
    round: u32,
    /// When the cards last appeared, in real seconds, for the input lockout.
    shown_at: f32,
}

/// An open Offer as it goes over the network: the Round it follows, whether its cards are Pacts,
/// and the stable IDs of its cards in order.
pub type OfferWire = (u32, bool, Vec<String>);

impl Offer {
    /// This Offer as the Host tells its Player.
    pub fn wire(&self) -> OfferWire {
        match &self.choices {
            Choices::Boons(boons) => (self.round, false, boons.iter().map(|boon| boon.id.to_string()).collect()),
            Choices::Pacts(pacts) => (self.round, true, pacts.iter().map(|pact| pact.id().to_string()).collect()),
        }
    }

    /// The Offer the Host told, or nothing if a card is one this game doesn't know.
    pub fn from_wire((round, pacts, ids): &OfferWire) -> Option<Self> {
        let choices = if *pacts {
            Choices::Pacts(ids.iter().map(|id| PACTS.iter().find(|pact| pact.id() == id)).collect::<Option<_>>()?)
        } else {
            Choices::Boons(ids.iter().map(|id| boon::find(id)).collect::<Option<_>>()?)
        };
        Some(Self { choices, round: *round, shown_at: 0.0 })
    }
}

/// The Boons a Player holds as the Host tells them: stable ID and Stacks.
pub fn boons_wire(held: &HeldBoons) -> Vec<(String, u32)> {
    held.0.iter().map(|&(boon, stacks)| (boon.id.to_string(), stacks)).collect()
}

/// Takes the Boons the Host says a Player holds, leaving out any this game doesn't know.
pub fn show_boons(held: &mut HeldBoons, told: &[(String, u32)]) {
    held.0 = told
        .iter()
        .filter_map(|(id, stacks)| Some((boon::find(id)?, *stacks)))
        .collect();
}

/// The Pacts held this Run as the Host tells them: stable ID and holder's Seat.
pub fn pacts_wire(held: &HeldPacts) -> Vec<(String, u8)> {
    held.0.iter().map(|held| (held.pact.id().to_string(), held.holder.0)).collect()
}

/// Takes the Pacts the Host says are held, leaving out any this game doesn't know.
pub fn show_pacts(held: &mut HeldPacts, told: &[(String, u8)]) {
    held.0 = told
        .iter()
        .filter_map(|(id, seat)| {
            Some(pact::HeldPact { pact: PACTS.iter().find(|pact| pact.id() == id)?, holder: Seat(*seat) })
        })
        .collect();
}

pub struct GauntletPlugin;

impl Plugin for GauntletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldPacts>()
            .init_resource::<Gifts>()
            .init_resource::<OfferWait>()
            .add_message::<PickOffer>()
            .add_message::<procs::ProcFired>()
            .add_message::<sets::SetTierReached>()
            .add_message::<duos::DuoTaken>()
            .init_resource::<sets::SetFlashes>()
            .init_resource::<procs::Thinning>()
            .init_resource::<procs::ProcFlashes>()
            .init_resource::<procs::Landing>()
            .init_resource::<duos::BallLightning>()
            .init_resource::<UnlockAll>()
            .init_resource::<ActiveRanks>()
            .init_resource::<RunModifiers>()
            .init_resource::<TurnedAllyCap>()
            .init_resource::<armory::ArmoryTab>()
            .init_resource::<armory::ArmoryNote>()
            .add_systems(Startup, load_progress)
            // Both run before the new Run's Player and Points are set up, after the Mode is picked.
            .add_systems(OnExit(GameState::MainMenu), set_run_modifiers)
            .add_systems(OnExit(GameState::GameOver), set_run_modifiers)
            .add_systems(OnExit(GameState::Playing), pay_scrip.run_if(gauntlet))
            // Closing the window mid-Run is quitting too.
            .add_systems(
                Last,
                pay_scrip
                    .run_if(on_message::<AppExit>)
                    .run_if(in_state(GameState::Playing))
                    .run_if(gauntlet),
            )
            .add_systems(
                Update,
                (
                    reset_progress.run_if(on_message::<ResetGauntletProgress>),
                    save_progress.run_if(resource_changed::<GauntletFile>),
                )
                    .chain(),
            )
            .add_systems(OnEnter(MainMenuScreen::Armory), armory::spawn_armory)
            .add_systems(
                Update,
                armory::refresh_armory
                    .run_if(in_state(MainMenuScreen::Armory))
                    .run_if(armory::armory_changed),
            )
            .add_observer(armory::armory_click)
            .add_systems(
                OnEnter(GameState::Playing),
                (reset, screen::spawn_held_column, sets::spawn_set_banner, duos::spawn_duo_banner).chain().run_if(gauntlet),
            )
            .add_systems(
                Update,
                (
                    (
                        (
                            schedule_offer,
                            open_offer.run_if(world_runs),
                            (withdraw_dropped, withdraw_offer).chain(),
                            apply_pick,
                            hold_intermission,
                        )
                            .chain(),
                        track_purchases,
                        blood_money,
                        teddys_tithe,
                        (war_bonds, bounty, butchers_bill, lucky_strike),
                        (scorched_earth, overkill),
                        (procs::fire_procs, procs::land_lethals, procs::cast_procs, procs::thaw).chain(),
                        (
                            duos::gun_and_blade,
                            duos::life_insurance,
                            duos::martyrs,
                            duos::thermal_shock,
                            duos::galvanised_dead,
                            duos::conscripts,
                            duos::live_fuse,
                            duos::supply_drop,
                            duos::fly_supply_shells,
                            // After the Blast's kills land, so its chains skip the Zombies it killed.
                            duos::ball_lightning.after(crate::enemies::zombie::apply_damage),
                        ),
                        (
                            (sets::blade_4, sets::ember_4, sets::gold_4, sets::grit_4, sets::ricochet),
                            (sets::cleave, sets::chill_patches).chain(),
                            (sets::storm_aura, sets::scorched, sets::jackpot, sets::bodyguard),
                        )
                            .after(sets::reach_set_bonuses),
                    )
                        .in_set(HostSimulation),
                    sets::reach_set_bonuses,
                    (apply_entries, sets::lend_ammo_mods),
                    (record_picks, screen::refresh_held_column).run_if(picks_changed).after(sets::reach_set_bonuses),
                    (procs::draw_procs, screen::update_proc_bars, screen::update_jackpot_bar),
                    (sets::show_set_banner, sets::draw_set_flashes, sets::draw_frost_patches),
                    (duos::announce_duos, duos::show_duo_banner).chain(),
                )
                    .run_if(in_state(GameState::Playing))
                    .run_if(gauntlet),
            )
            .add_systems(
                Update,
                toggle_unlock_all
                    .run_if(input_just_pressed(KeyCode::F9))
                    .run_if(gauntlet),
            )
            .add_systems(OnEnter(Pause::Frozen), screen::spawn_offer.run_if(gauntlet))
            .add_systems(
                Update,
                (
                    screen::show_coop_offer
                        .run_if(in_state(GameState::Playing))
                        .run_if(coop),
                    screen::highlight_cards,
                )
                    .run_if(gauntlet),
            )
            .add_observer(give_gauntlet_state)
            .add_observer(click_card);
    }
}

fn gauntlet(mode: Res<Mode>) -> bool {
    *mode == Mode::Gauntlet
}

fn coop(run_type: Res<RunType>) -> bool {
    run_type.is_coop()
}

/// A Boon or a Pact was just taken (or the Run began), or the Round moved on, since a Drawback
/// can quote the Round's numbers.
fn picks_changed(
    held: Query<Ref<HeldBoons>, With<LocalPlayer>>,
    pacts: Res<HeldPacts>,
    round: Option<Res<Round>>,
    mut seen: Local<u32>,
) -> bool {
    let number = round.map_or(0, |round| round.number);
    let moved = std::mem::replace(&mut *seen, number) != number;
    held.single().is_ok_and(|held| held.is_changed()) || pacts.is_changed() || moved
}

/// Every new Player starts with no Boons and no Offer coming. Gauntlet's state sits on the Player
/// from here, so Classic never has to know it.
fn give_gauntlet_state(
    add: On<Add<Player>>,
    mut commands: Commands,
    progress: Option<Res<GauntletFile>>,
) {
    // The local save's Unlocks stand in for every Player's until the Room's `Hello` fills a
    // remote one's.
    let unlocked = progress.map_or_else(Vec::new, |file| file.unlocked.clone());
    commands.entity(add.entity).insert((
        HeldBoons::default(),
        PendingOffer::default(),
        PlayerUnlocks(unlocked),
        Bought::default(),
        procs::ProcCooldowns::default(),
        sets::SetBonuses::default(),
        sets::Jackpot::default(),
        sets::Capstones::default(),
    ));
}

fn reset(mut commands: Commands) {
    commands.insert_resource(HeldPacts::default());
    commands.insert_resource(Gifts::default());
    commands.insert_resource(OfferWait::default());
    commands.insert_resource(procs::Thinning::default());
    commands.insert_resource(procs::ProcFlashes::default());
    commands.insert_resource(procs::Landing::default());
    commands.insert_resource(duos::BallLightning::default());
    commands.insert_resource(sets::SetFlashes::default());
}

/// The Round ended: every Player's Offer is due after its delay, and the Intermission waits for
/// them (in Co-op, for at most `BOON_OFFER_COOP_TIMEOUT_SECS`).
fn schedule_offer(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    mut pending: Query<(&mut PendingOffer, Has<Disconnected>), With<Player>>,
    mut hold: ResMut<IntermissionHold>,
    mut wait: ResMut<OfferWait>,
) {
    if ended.read().count() > 0 {
        for (mut pending, disconnected) in &mut pending {
            pending.owed.push(round.number);
            // One whose connection has dropped is owed it, and is shown it in their first
            // Intermission back.
            pending.left = (!disconnected).then_some(BOON_OFFER_DELAY_SECS);
        }
        hold.0 = true;
        wait.0 = BOON_OFFER_COOP_TIMEOUT_SECS;
    }
}

/// Opens each Player's Offer once its delay is up. Solo that freezes the world, and a Round that
/// ended in Last Stand waits for the Self-Revive; in Co-op it opens whatever the Player is doing,
/// from their own Unlocks and Boons. Every fifth Round it's a Pact Offer, unless no Pact is left
/// to offer, and never one a teammate holds or has on their Offer. With no eligible Boon there's no
/// Offer. An Offer still open from an earlier Round holds the next one back.
fn open_offer(
    mut commands: Commands,
    mut players: Query<(
        Entity,
        Has<LastStand>,
        Has<LocalPlayer>,
        Option<&Offer>,
        &Loadout,
        &HeldBoons,
        &PlayerUnlocks,
        &Bought,
        &mut PendingOffer,
    )>,
    pacts: Res<HeldPacts>,
    unlock_all: Res<UnlockAll>,
    supply: Supply,
    run_type: Res<RunType>,
    time: Res<Time>,
    mut cues: MessageWriter<PlayCue>,
) {
    // Pacts on a Player's open Offer are spoken for, so two Offers drawn together never share one.
    let mut claimed: Vec<&'static str> = players
        .iter()
        .filter_map(|(_, _, _, offer, ..)| match &offer?.choices {
            Choices::Pacts(pacts) => Some(pacts.iter().map(|pact| pact.id())),
            Choices::Boons(_) => None,
        })
        .flatten()
        .collect();
    for (player, last_stand, local, offer, loadout, held, unlocks, bought, mut pending) in &mut players {
        let Some(left) = pending.left.as_mut() else {
            continue;
        };
        *left -= time.delta_secs();
        if *left > 0.0 || (last_stand && !run_type.is_coop()) || offer.is_some() {
            continue;
        }
        if pending.owed.is_empty() {
            pending.left = None;
            continue;
        }
        let round = pending.owed.remove(0);
        let run = RunFacts {
            power_on: supply.has_been_on(),
            held_pacts: pacts.ids(),
            used_box: bought.from_box,
            bought_quick_revive: bought.quick_revive,
            holds_upgraded: loadout.weapons().any(|weapon| weapon.upgraded),
            holds_ammo_mod: loadout.weapons().any(|weapon| !weapon.ammo_mods.is_empty()),
            holds_proc_boon: held.0.iter().any(|(boon, _)| boon.proc.is_some()),
            unlocked: unlocks.0.clone(),
            unlock_all: unlock_all.0,
            kit: None,
        };
        let roll = |total| rand::random_range(0.0..total);
        let pact_choices = if is_pact_round(round) {
            draw_pact_offer(PACTS, &pacts, &claimed, &run, roll)
        } else {
            Vec::new()
        };
        let choices = if pact_choices.is_empty() {
            Choices::Boons(draw_offer(BOONS, held, &run, roll))
        } else {
            claimed.extend(pact_choices.iter().map(|pact| pact.id()));
            Choices::Pacts(pact_choices)
        };
        if choices.is_empty() {
            continue;
        }
        if local {
            let cue = match choices {
                Choices::Boons(_) => Cue::BoonOffer,
                Choices::Pacts(_) => Cue::PactOffer,
            };
            cues.write(PlayCue::new(cue, None));
        }
        let mut entity = commands.entity(player);
        entity.insert(Offer {
            choices,
            round,
            shown_at: 0.0,
        });
        if run_type.is_coop() {
            entity.insert(Choosing);
        }
    }
}

/// A Player who Bleeds Out loses the Offer on their screen, and it goes back to the front of what
/// they're owed, to be shown in the next Intermission.
fn withdraw_offer(
    mut commands: Commands,
    mut bled_out: MessageReader<BledOut>,
    mut players: Query<(&Offer, &mut PendingOffer)>,
) {
    for &BledOut { player } in bled_out.read() {
        if let Ok((offer, mut pending)) = players.get_mut(player) {
            pending.owed.insert(0, offer.round);
            // Not before the next Intermission.
            pending.left = None;
            commands.entity(player).remove::<(Offer, Choosing)>();
        }
    }
}

/// A Player whose connection drops loses the Offer on their screen like a Bleed Out does, and it
/// is owed to them for their first Intermission back; nothing is due while they are away.
fn withdraw_dropped(
    mut commands: Commands,
    mut dropped: MessageReader<SeatDropped>,
    mut players: Query<(Entity, &Seat, Option<&Offer>, &mut PendingOffer)>,
) {
    for &SeatDropped { seat } in dropped.read() {
        for (player, held, offer, mut pending) in &mut players {
            if *held != seat {
                continue;
            }
            if let Some(offer) = offer {
                pending.owed.insert(0, offer.round);
                commands.entity(player).remove::<(Offer, Choosing)>();
            }
            pending.left = None;
        }
    }
}

/// A click on a card, once the input lockout is over, picks that card for the `LocalPlayer`; on a
/// Pact Offer's Skip button, it skips.
fn click_card(
    activate: On<Activate>,
    cards: Query<&OfferCard>,
    skips: Query<(), With<SkipButton>>,
    player: Query<(Entity, &Offer), With<LocalPlayer>>,
    real: Res<Time<Real>>,
    mut picks: MessageWriter<PickOffer>,
) {
    let Ok((player, offer)) = player.single() else {
        return;
    };
    let card = cards.get(activate.entity).ok().map(|&OfferCard(index)| index);
    let skip = skips.contains(activate.entity) && matches!(offer.choices, Choices::Pacts(_));
    if card.is_none() && !skip {
        return;
    }
    if real.elapsed_secs() - offer.shown_at < BOON_OFFER_INPUT_LOCKOUT_SECS {
        return;
    }
    picks.write(PickOffer { player, card });
}

/// A pick takes that Boon or Pact for its Player, or nothing on a skip, and closes their Offer.
/// A Pact goes to the picker, who holds its upside.
pub(crate) fn apply_pick(
    mut commands: Commands,
    mut picks: MessageReader<PickOffer>,
    mut players: Query<(&Seat, &Offer, &mut HeldBoons, Has<LocalPlayer>)>,
    mut pacts: ResMut<HeldPacts>,
    mut cues: MessageWriter<PlayCue>,
) {
    for &PickOffer { player, card } in picks.read() {
        let Ok((&seat, offer, mut held, local)) = players.get_mut(player) else {
            continue;
        };
        match (card, &offer.choices) {
            (Some(index), Choices::Boons(boons)) => {
                let Some(&boon) = boons.get(index) else {
                    continue;
                };
                held.add(boon);
            }
            (Some(index), Choices::Pacts(choices)) => {
                let Some(&pact) = choices.get(index) else {
                    continue;
                };
                pacts.add(pact, seat);
            }
            // Only a Pact Offer can be skipped.
            (None, Choices::Boons(_)) => continue,
            (None, Choices::Pacts(_)) => {}
        }
        if local {
            cues.write(PlayCue::new(Cue::OfferPick, None));
        }
        commands.entity(player).remove::<(Offer, Choosing)>();
    }
}

/// Keeps the Intermission from starting while Offers are due or open. Solo that is until the pick,
/// with the world frozen for it; in Co-op the world runs and the wait ends after
/// `BOON_OFFER_COOP_TIMEOUT_SECS`, leaving any Offer still open.
fn hold_intermission(
    players: Query<(&PendingOffer, Has<Offer>), With<Player>>,
    run_type: Res<RunType>,
    time: Res<Time>,
    mut wait: ResMut<OfferWait>,
    mut hold: ResMut<IntermissionHold>,
    mut freeze: ResMut<Freeze>,
) {
    let busy = players
        .iter()
        .any(|(pending, open)| pending.left.is_some() || open);
    let (held, frozen) = if run_type.is_coop() {
        wait.0 = (wait.0 - time.delta_secs()).max(0.0);
        (busy && wait.0 > 0.0, false)
    } else {
        (busy, players.iter().any(|(_, open)| open))
    };
    if hold.0 != held {
        hold.0 = held;
    }
    if freeze.0 != frozen {
        freeze.0 = frozen;
    }
}

/// Keeps the Player's Boon, Pact and Set Bonus entries in step with the Boons and Pacts held and
/// the tiers reached. A Down takes only Perk entries, so these survive it, and a Pact's Drawback
/// lasts the Run.
fn apply_entries(
    pacts: Res<HeldPacts>,
    mut players: Query<(&Seat, &HeldBoons, &sets::SetBonuses, &Loadout, &mut Modifiers), With<Player>>,
) {
    let ours = |source| is_boon_source(source) || is_pact_source(source) || sets::is_set_source(source);
    for (&seat, held, set_bonuses, loadout, mut modifiers) in &mut players {
        let low = sets::magazine_low(loadout.gun_ammo().0, loadout.magazine_size(&modifiers.0));
        let mut wanted = held.modifiers();
        wanted.extend(pacts.modifiers(seat));
        wanted.extend(sets::set_entries(set_bonuses, low));
        let current: Vec<_> = modifiers
            .0
            .iter()
            .filter(|entry| ours(entry.source))
            .copied()
            .collect();
        if current != wanted {
            modifiers.0.retain(|entry| !ours(entry.source));
            modifiers.0.extend(wanted);
        }
    }
}

/// Scorched Earth: every Zombie killed by its holder's hit explodes, never hurting any Player.
fn scorched_earth(
    mut kills: MessageReader<KillDamage>,
    pacts: Res<HeldPacts>,
    seats: Query<&Seat, With<Player>>,
    mut explosions: MessageWriter<Explosion>,
) {
    let holder = pacts.holder(SCORCHED_EARTH.id);
    for kill in kills.read() {
        if holder.is_some() && holder == seat_of(kill.player, &seats) {
            explosions.write(scorched_earth_blast(kill.at, kill.max_health, kill.player));
        }
    }
}

/// Overkill: a kill's damage beyond what it took, by its holder's hit, carries to the nearest
/// Zombie, by the same attack.
fn overkill(
    mut kills: MessageReader<KillDamage>,
    pacts: Res<HeldPacts>,
    seats: Query<&Seat, With<Player>>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
) {
    let holder = pacts.holder(OVERKILL.id);
    for kill in kills.read() {
        if holder.is_none() || holder != seat_of(kill.player, &seats) {
            continue;
        }
        let others = zombies
            .iter()
            .filter(|(zombie, _)| *zombie != kill.zombie)
            .map(|(zombie, transform)| (zombie, transform.translation()));
        if let Some(zombie) = overkill_target(kill.at, kill.overflow, others) {
            damage.write(DamageZombie {
                player: kill.player,
                zombie,
                damage: kill.overflow,
                zone: HitZone::Torso,
                attack: kill.attack,
            });
        }
    }
}

/// The Seat of the Player a kill or pickup is credited to, if there is one.
fn seat_of(player: Option<Entity>, seats: &Query<&Seat, With<Player>>) -> Option<Seat> {
    seats.get(player?).ok().copied()
}

/// Blood Money: a Zombie's kill pays its killer's Blood Money on top.
fn blood_money(
    mut kills: MessageReader<ZombieKilled>,
    mut bonus: MessageWriter<BonusPoints>,
    players: Query<&HeldBoons, With<Player>>,
) {
    for kill in kills.read() {
        let Some(player) = kill.player else {
            continue;
        };
        let Ok(held) = players.get(player) else {
            continue;
        };
        let points = blood_money_points(kill.pay, held.stacks(BLOOD_MONEY));
        let points = duos::gold_standard_points(points, kill.headshot, held.stacks(duos::GOLD_STANDARD) > 0);
        if points > 0 {
            bonus.write(BonusPoints {
                player,
                amount: points,
            });
        }
    }
}

/// Teddy's Tithe: a Teddy Bear pays its spinner Points on top of their refund.
fn teddys_tithe(
    mut teddies: MessageReader<TeddyBear>,
    mut bonus: MessageWriter<BonusPoints>,
    players: Query<&HeldBoons, With<Player>>,
) {
    for &TeddyBear { spinner } in teddies.read() {
        let Ok(held) = players.get(spinner) else {
            continue;
        };
        let points = teddys_tithe_points(held.stacks(TEDDYS_TITHE));
        if points > 0 {
            bonus.write(BonusPoints {
                player: spinner,
                amount: points,
            });
        }
    }
}

/// Notes the purchases Requirements ask about, for the Player who made them: anything from a
/// Mystery Box, and Quick Revive.
fn track_purchases(
    mut purchases: MessageReader<Purchased>,
    boxes: Query<(), With<MysteryBox>>,
    machines: Query<&PerkMachine>,
    mut players: Query<&mut Bought, With<Player>>,
) {
    for &Purchased { player, buyable } in purchases.read() {
        let Ok(mut bought) = players.get_mut(player) else {
            continue;
        };
        if boxes.contains(buyable) {
            bought.from_box = true;
        }
        if machines
            .get(buyable)
            .is_ok_and(|machine| machine.perk == Perk::QuickRevive)
        {
            bought.quick_revive = true;
        }
    }
}

/// War Bonds: each Round's end pays every holder Points by its number.
fn war_bonds(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    mut bonus: MessageWriter<BonusPoints>,
    players: Query<(Entity, &HeldBoons), With<Player>>,
) {
    for _ in ended.read() {
        for (player, held) in &players {
            let points = war_bonds_points(round.number, held.stacks(WAR_BONDS));
            if points > 0 {
                bonus.write(BonusPoints {
                    player,
                    amount: points,
                });
            }
        }
    }
}

/// Bounty: a Trap's kill pays the Player whose Trap it was.
fn bounty(
    mut kills: MessageReader<ZombieKilled>,
    mut bonus: MessageWriter<BonusPoints>,
    players: Query<&HeldBoons, With<Player>>,
) {
    for kill in kills.read() {
        let Some(player) = kill.player else {
            continue;
        };
        let Ok(held) = players.get(player) else {
            continue;
        };
        let points = bounty_points(kill.attack, held.stacks(BOUNTY));
        if points > 0 {
            bonus.write(BonusPoints {
                player,
                amount: points,
            });
        }
    }
}

/// Butcher's Bill: a Knife kill may drop a Power-up where it fell, outside the per-Round max, for
/// the killer's Stacks alone.
fn butchers_bill(
    mut kills: MessageReader<ZombieKilled>,
    players: Query<&HeldBoons, With<Player>>,
    mut drops: MessageWriter<DropPowerUp>,
) {
    for kill in kills.read() {
        let Some(held) = kill.player.and_then(|player| players.get(player).ok()) else {
            continue;
        };
        let stacks = held.stacks(BUTCHERS_BILL);
        if stacks > 0 && butchers_bill_drops(kill.attack, stacks, rand::random()) {
            // Cut of the Take makes each drop more than one.
            for _ in 0..duos::butchers_bill_drops_for(held) {
                drops.write(DropPowerUp {
                    at: kill.at,
                    power_up: None,
                });
            }
        }
    }
}

/// Lucky Strike: a holder picking up a Power-up may also be given a different one. The gift's own
/// pickup doesn't roll again.
fn lucky_strike(
    mut taken: MessageReader<PowerUpTaken>,
    players: Query<&HeldBoons, With<Player>>,
    mut gifts: ResMut<Gifts>,
    mut give: MessageWriter<GivePowerUp>,
) {
    for &PowerUpTaken(power_up, taker) in taken.read() {
        let Some(taker) = taker else {
            continue;
        };
        if let Some(i) = gifts.0.iter().position(|&gift| gift == (taker, power_up)) {
            gifts.0.remove(i);
            continue;
        }
        let Ok(held) = players.get(taker) else {
            continue;
        };
        let stacks = held.stacks(LUCKY_STRIKE);
        if stacks == 0 {
            continue;
        }
        if let Some(gift) = lucky_strike_gift(power_up, stacks, rand::random(), rand::random()) {
            gifts.0.push((taker, gift));
            give.write(GivePowerUp(gift, Some(taker)));
        }
    }
}

fn toggle_unlock_all(mut unlock_all: ResMut<UnlockAll>) {
    unlock_all.0 = !unlock_all.0;
}

/// The held Boons go on the Run Record in pick order, with their Stacks, then the Pacts, then the
/// active Rank count.
fn record_picks(
    held: Single<&HeldBoons, With<LocalPlayer>>,
    pacts: Res<HeldPacts>,
    ranks: Res<ActiveRanks>,
    mut extras: ResMut<RecordExtras>,
) {
    let boons = held.0.iter().map(|&(boon, stacks)| RecordExtra {
        id: boon.id.to_string(),
        count: stacks,
    });
    let pacts = pacts.0.iter().map(|held| RecordExtra {
        id: held.pact.id().to_string(),
        count: 1,
    });
    let ranks = RecordExtra {
        id: RANKS_RECORD_ID.to_string(),
        count: ranks.0,
    };
    extras.0 = boons.chain(pacts).chain([ranks]).collect();
}

fn load_progress(mut commands: Commands, dir: Res<SaveDir>) {
    let file = match &dir.0 {
        Some(dir) => save::load::<GauntletFile>(dir),
        None => GauntletFile::default(),
    };
    commands.insert_resource(file);
}

fn save_progress(file: Res<GauntletFile>, dir: Res<SaveDir>) {
    // A freshly loaded file is already on disk (or defaults); only changes get written.
    if !file.is_added() {
        store_progress(&file, &dir);
    }
}

fn store_progress(file: &GauntletFile, dir: &SaveDir) {
    if let Some(dir) = &dir.0
        && let Err(err) = save::store(dir, file)
    {
        warn!("Couldn't save Gauntlet progress: {err}");
    }
}

/// Settings' "Reset Gauntlet Progress": Scrip, Unlocks, Ranks and toggles go back to a first
/// launch's. Records are left alone.
fn reset_progress(mut file: ResMut<GauntletFile>) {
    *file = GauntletFile::default();
}

/// The Upgrade Tracks switched on give a Gauntlet Run's Player its entries, starting Points
/// included, through Classic's `RunModifiers`, and the Run caps Turned allies at
/// `GAUNTLET_TURNED_ALLY_CAP`. A Classic Run gets no entries and no cap.
fn set_run_modifiers(
    mode: Res<Mode>,
    file: Res<GauntletFile>,
    mut run: ResMut<RunModifiers>,
    mut ranks: ResMut<ActiveRanks>,
    mut cap: ResMut<TurnedAllyCap>,
) {
    if *mode == Mode::Gauntlet {
        run.0 = file.track_entries();
        ranks.0 = file.active_ranks();
        *cap = TurnedAllyCap(Some(GAUNTLET_TURNED_ALLY_CAP));
    } else {
        run.0.clear();
        ranks.0 = 0;
        *cap = TurnedAllyCap(None);
    }
}

/// The Run ended, at Game Over or on quit: each completed Round pays its number in Scrip, saved
/// straight away (the app may be exiting) and shown on the Game Over screen.
fn pay_scrip(
    round: Res<Round>,
    mut file: ResMut<GauntletFile>,
    dir: Res<SaveDir>,
    mut lines: ResMut<GameOverLines>,
) {
    let in_intermission = matches!(round.phase, RoundPhase::Intermission(_));
    let scrip = scrip_for_rounds(rounds_completed(round.number, in_intermission));
    file.pay(scrip);
    store_progress(&file, &dir);
    lines
        .0
        .push(format!("Scrip earned {}", crate::ui::hud::with_commas(scrip)));
}

/// The ids of the first `count` Boons, for a test to offer.
#[cfg(test)]
pub fn sample_boon_ids(count: usize) -> Vec<String> {
    BOONS.iter().take(count).map(|boon| boon.id.to_string()).collect()
}
