//! Gauntlet: Classic with a Boon Offer after every Round, a Pact Offer in its place after Rounds 5,
//! 15 and 25, and a Run of three Acts each ending in a Boss Round, the last one's clearing a Win
//! (see `arc`). Between Runs, the Scrip each Run pays buys Unlocks and Upgrade Track Ranks in the
//! Armory, kept in `gauntlet.ron`. It's layered on top of Classic and
//! reaches it only through seams (ADR 0002): stat-modifier entries on the Player, messages Classic
//! already sends, Classic state it reads, and the `Freeze` and `IntermissionHold` it holds. Every
//! system here that touches a Run runs only when `Mode == Gauntlet`; the Armory and the save file
//! live in the menus.

pub(crate) mod arc;
mod armory;
pub mod boon;
pub(crate) mod commendations;
pub(crate) mod duos;
pub(crate) mod elite;
pub mod heat;
pub mod kits;
mod pact;
pub(crate) mod procs;
pub mod progress;
mod screen;
pub(crate) mod sets;
mod tools;
#[cfg(test)]
pub(crate) use tools::reroll_price_from;
pub(crate) mod variety;
pub(crate) mod win;
#[cfg(test)]
mod coop_tests;
#[cfg(test)]
mod sets_tests;
#[cfg(test)]
mod snapshot_tests;
#[cfg(test)]
mod duos_tests;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use bevy::ui_widgets::Activate;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::{Authority, HostSimulation};
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
use crate::economy::points::{BonusPoints, Points};
use crate::economy::power_up::{DropPowerUp, GivePowerUp, PowerUp, PowerUpTaken};
use crate::storage::records::{RecordExtra, RecordExtras, RunHeatRecord, RunKit};
use crate::run::round::{IntermissionHold, ReservedRounds, Round, RoundEnded, RoundPhase};
use crate::storage::save;
use crate::storage::settings::SaveDir;
use crate::state::{Choosing, Freeze, GameState, MainMenuScreen, Pause, RunType, world_runs};
use crate::tuning::*;
use crate::weapons::ammo_mod::TurnedAllyCap;
use crate::weapons::weapon::{Loadout, StartingLoadout};
use crate::enemies::zombie::{DamageZombie, Dying, HitZone, KillDamage, Zombie, ZombieKilled};
use boon::{
    BLOOD_MONEY, BOONS, BOUNTY, BUTCHERS_BILL, BoonDef, LUCKY_STRIKE,
    TEDDYS_TITHE, WAR_BONDS, blood_money_points, bounty_points, butchers_bill_drops, draw_offer,
    is_boon_source, lucky_strike_gift, teddys_tithe_points, war_bonds_points,
};
use arc::{RunArc, is_arc_source, is_boss_round, is_pact_round};
use pact::{
    OVERKILL, PACTS, PactDef, SCORCHED_EARTH, draw_pact_offer,
    is_pact_source, overkill_target, scorched_earth_blast,
};
use progress::{GauntletFile, rounds_completed, scrip_for_rounds};
pub use boon::HeldBoons;
pub use elite::{Elite, EliteTrait};
pub use arc::{Boss, BossView, BurstView, boss_of};
pub use pact::HeldPacts;
use screen::{OfferCard, SkipButton};
use heat::RunHeat;
use kits::{Kit, PlayerKit, chosen_kit};
pub(crate) use tools::Banishes;
pub(crate) use tools::{BanishCard, RerollOffer, ToolUsed};
use variety::{Boost, OwedBoost};
pub use variety::{PickRound, RoundVariety, VoteRound};

/// The Offers a Player is waiting for.
#[derive(Component, Default)]
pub(crate) struct PendingOffer {
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
pub(crate) struct OfferWait(f32);

/// A Player picked a card of their open Offer, or `None` to skip a Pact Offer. The screen sends it
/// for the `LocalPlayer`; the netcode will for a remote one.
#[derive(Message, Debug, Clone, Copy)]
pub struct PickOffer {
    pub player: Entity,
    pub card: Option<usize>,
}

/// Request to give a Player a Boon (one more Stack if held), with no Offer. Gauntlet only; a Run of
/// any other Mode ignores it. Developer mode sends it.
#[derive(Message, Clone, Copy)]
pub struct GrantBoon {
    pub player: Entity,
    pub boon: &'static boon::BoonDef,
}

/// Request to give a Player a Pact, with no Offer: they hold its upside, as if they had picked it.
/// One already held stays held. Gauntlet only. Developer mode sends it.
#[derive(Message, Clone, Copy)]
pub struct GrantPact {
    pub player: Entity,
    pub pact: &'static pact::PactDef,
}

/// Every Boon and Duo, in a fixed order, so a Cheat can name one by its place in it.
pub fn boon_catalog() -> impl Iterator<Item = &'static boon::BoonDef> {
    boon::BOONS.iter().chain(duos::DUOS)
}

/// Every Pact, in a fixed order.
pub fn pact_catalog() -> &'static [pact::PactDef] {
    PACTS
}

/// Debug: every Boon and Pact counts as Unlocked, toggled by the Cheat Panel during a Gauntlet Run. It
/// changes nothing in `gauntlet.ron`.
#[derive(Resource, Default)]
pub(crate) struct UnlockAll(bool);

/// The Upgrade Track Ranks switched on as this Run began (0 to 25), for its Run Record.
#[derive(Resource, Default)]
pub(crate) struct ActiveRanks(u32);

/// The Kit this Run started with (Standard in Classic), fixed as the Run begins.
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct ChosenKit(Kit);

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
    /// Rerolls already made in this Offer, which set the next one's price.
    rerolls: u32,
    /// What the last Round's variety adds to it: a higher Rarity, a Family's card.
    boost: Boost,
    /// Cards that may be picked from it (more than one with a Boosted Offer's extra pick), and
    /// how many have been.
    picks: u32,
    picked: u32,
}

impl Offer {
    /// An Offer of `choices` after Round `round`, with one pick.
    fn new(choices: Choices, round: u32) -> Self {
        Self { choices, round, shown_at: 0.0, rerolls: 0, boost: Boost::default(), picks: 1, picked: 0 }
    }
}

/// An open Offer as it goes over the network: the Round it follows, whether its cards are Pacts,
/// the stable IDs of its cards in order, the Rerolls made and Banish charges left (so the
/// Reroll price and Banish buttons read right on a client) and the picks allowed and taken.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OfferWire {
    pub round: u32,
    pub pacts: bool,
    pub cards: Vec<String>,
    pub rerolls: u8,
    pub charges: u8,
    pub picks: u8,
    pub picked: u8,
}

impl Offer {
    /// This Offer as the Host tells its Player, with the Player's Banish `charges`.
    pub fn wire(&self, charges: u32) -> OfferWire {
        let (pacts, cards) = match &self.choices {
            Choices::Boons(boons) => (false, boons.iter().map(|boon| boon.id.to_string()).collect()),
            Choices::Pacts(pacts) => (true, pacts.iter().map(|pact| pact.id().to_string()).collect()),
        };
        let small = |count: u32| u8::try_from(count).unwrap_or(u8::MAX);
        OfferWire {
            round: self.round,
            pacts,
            cards,
            rerolls: small(self.rerolls),
            charges: small(charges),
            picks: small(self.picks),
            picked: small(self.picked),
        }
    }

    /// The Offer the Host told, or nothing if a card is one this game doesn't know.
    pub fn from_wire(wire: &OfferWire) -> Option<Self> {
        let choices = if wire.pacts {
            Choices::Pacts(wire.cards.iter().map(|id| PACTS.iter().find(|pact| pact.id() == id)).collect::<Option<_>>()?)
        } else {
            Choices::Boons(wire.cards.iter().map(|id| boon::find(id)).collect::<Option<_>>()?)
        };
        let mut offer = Self::new(choices, wire.round);
        offer.rerolls = u32::from(wire.rerolls);
        offer.picks = u32::from(wire.picks).max(1);
        offer.picked = u32::from(wire.picked);
        Some(offer)
    }

    /// Rerolls already made in this Offer.
    pub fn rerolls(&self) -> u32 {
        self.rerolls
    }

    /// The cards' stable IDs, in order.
    pub fn card_ids(&self) -> Vec<String> {
        self.wire(0).cards
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

/// What the Host needs to run Reroll and Banish for a test of the netcode: the Gauntlet state on
/// each Player, and the tools themselves.
#[cfg(test)]
pub(crate) fn host_offer_tools(app: &mut App) {
    app.init_resource::<UnlockAll>()
        .init_resource::<RunHeat>()
        .add_message::<crate::economy::points::SpendPoints>()
        .add_message::<RerollOffer>()
        .add_message::<BanishCard>()
        .add_message::<ToolUsed>()
        .add_message::<PlayCue>()
        .add_observer(give_gauntlet_state)
        .add_systems(Update, (tools::apply_reroll, tools::apply_banish).chain().in_set(HostSimulation))
        .add_systems(Update, crate::economy::points::spend);
}

pub struct GauntletPlugin;

impl Plugin for GauntletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldPacts>()
            .init_resource::<Gifts>()
            .init_resource::<OfferWait>()
            .add_message::<PickOffer>()
            .add_message::<ToggleUnlockAll>()
            .add_message::<GrantBoon>()
            .add_message::<GrantPact>()
            .add_message::<RerollOffer>()
            .add_message::<BanishCard>()
            .add_message::<ToolUsed>()
            .add_message::<commendations::CommendationEarned>()
            .init_resource::<commendations::RunLog>()
            .init_resource::<commendations::RunCommendations>()
            .init_resource::<ChosenKit>()
            .init_resource::<RunHeat>()
            .init_resource::<elite::EliteRules>()
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
            .init_resource::<RunArc>()
            .init_resource::<win::WinWait>()
            .init_resource::<win::WinClock>()
            .add_message::<win::WinChoice>()
            .init_resource::<RoundVariety>()
            .init_resource::<crate::audio::RoundStartCue>()
            .add_message::<PickRound>()
            .add_message::<VoteRound>()
            .init_resource::<arc::BossPlan>()
            .add_message::<arc::SummonBoss>()
            .init_resource::<arc::BossView>()
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
            .add_observer(kits::pick_kit)
            .add_systems(
                OnEnter(GameState::Playing),
                (
                    reset,
                    commendations::begin_run,
                    screen::spawn_held_column,
                    sets::spawn_set_banner,
                    duos::spawn_duo_banner,
                    commendations::spawn_banner,
                    arc::spawn_boss_hud,
                    variety::spawn_hud,
                )
                    .chain()
                    .run_if(gauntlet),
            )
            .add_systems(
                Update,
                (
                    (
                        (
                            variety::begin_round,
                            variety::track_challenge,
                            variety::end_round,
                            tools::grant_banishes,
                            schedule_offer,
                            open_offer.run_if(world_runs),
                            (withdraw_dropped, withdraw_offer).chain(),
                            (tools::apply_reroll, tools::apply_banish),
                            apply_pick,
                            apply_grants,
                            variety::light_choice,
                            variety::apply_pick,
                            variety::count_votes,
                            variety::settle_votes,
                            hold_intermission,
                        )
                            .chain(),
                        track_purchases,
                        kits::spend_first_buys,
                        blood_money,
                        teddys_tithe,
                        (war_bonds, bounty, butchers_bill, lucky_strike),
                        (scorched_earth, overkill),
                        (
                            elite::mark_elites,
                            elite::mark_requested_elites,
                            elite::soak_armour,
                            elite::regenerate,
                            elite::rally,
                            elite::elite_deaths,
                            elite::burn_fuses,
                            elite::elite_points,
                            elite::strip_turned_elites,
                        ),
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
                    variety::record_counts.after(record_picks),
                    (variety::update_hud, variety::show_choice, variety::light_panels, variety::show_votes).chain(),
                    variety::pick_keys,
                    variety::client_rounds.run_if(in_state(Authority::Client)),
                    (procs::draw_procs, screen::update_proc_bars, screen::update_jackpot_bar),
                    (elite::tint_elites, elite::draw_rally_rings, elite::puppet_fuses.run_if(in_state(Authority::Client))),
                    (sets::show_set_banner, sets::draw_set_flashes, sets::draw_frost_patches),
                    (sets::fade_patches, duos::fly_shell_views, procs::thaw_puppets).run_if(in_state(Authority::Client)),
                    (duos::announce_duos, duos::show_duo_banner).chain(),
                    commendations::show_banner,
                    record_run_meta,
                )
                    .run_if(in_state(GameState::Playing))
                    .run_if(gauntlet),
            )
            .add_systems(
                Update,
                (
                    (
                        arc::extend_reserved,
                        arc::spawn_boss.run_if(world_runs),
                        arc::summon_boss,
                        arc::tag_boss,
                        arc::bloater_bursts,
                        arc::grow_bursts,
                        arc::pack_howls,
                        arc::pack_dies_with_alpha,
                        arc::boss_deaths,
                        // Before the Offer is due, so a Win holds it back.
                        arc::clear_boss_rounds.before(schedule_offer),
                    )
                        .chain()
                        .in_set(HostSimulation),
                    // A Co-op Win holds the Offers back until every Player has chosen.
                    (win::await_win_choices.after(arc::clear_boss_rounds), win::settle_win_choices)
                        .chain()
                        .before(open_offer)
                        .before(hold_intermission)
                        .in_set(HostSimulation),
                    (win::open_win_screen, win::close_win_screen)
                        .chain()
                        .after(arc::clear_boss_rounds)
                        .after(arc::infer_win)
                        .run_if(coop),
                    (
                        arc::plan_boss,
                        arc::count_boss_warning.run_if(world_runs).before(arc::spawn_boss),
                        arc::fill_boss_view.in_set(HostSimulation),
                        arc::update_boss_hud,
                    )
                        .chain(),
                    arc::draw_burst_views.run_if(in_state(Authority::Client)),
                )
                    .run_if(in_state(GameState::Playing))
                    .run_if(gauntlet),
            )
            // Each machine logs and pays its own Player's Commendations, so these are not
            // `HostSimulation`; a client infers its Win from the Round first.
            .add_systems(
                Update,
                (
                    arc::infer_win.run_if(in_state(Authority::Client)),
                    (commendations::log_run, commendations::earn_commendations).chain(),
                )
                    .chain()
                    .after(arc::clear_boss_rounds)
                    .run_if(in_state(GameState::Playing))
                    .run_if(gauntlet),
            )
            .add_systems(OnEnter(Pause::Frozen), arc::spawn_win_screen.run_if(gauntlet))
            .add_observer(arc::click_win)
            .add_systems(
                Update,
                toggle_unlock_all.in_set(HostSimulation).run_if(gauntlet),
            )
            .add_systems(OnEnter(Pause::Frozen), screen::spawn_offer.run_if(gauntlet))
            .add_systems(
                Update,
                (
                    screen::show_coop_offer
                        .run_if(in_state(GameState::Playing))
                        .run_if(coop),
                    screen::highlight_cards,
                    (screen::refresh_offer, screen::update_tools, screen::close_offer_screen).chain().run_if(in_state(GameState::Playing)),
                )
                    .run_if(gauntlet),
            )
            .add_systems(PostUpdate, variety::draw_fog.run_if(in_state(GameState::Playing)).run_if(gauntlet))
            .add_observer(variety::click_panel)
            .add_observer(give_gauntlet_state)
            .add_observer(click_tool)
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
pub(crate) fn give_gauntlet_state(
    add: On<Add<Player>>,
    mut commands: Commands,
    progress: Option<Res<GauntletFile>>,
    (chosen, heat): (Option<Res<ChosenKit>>, Option<Res<RunHeat>>),
    locals: Query<(), With<LocalPlayer>>,
    seats: Query<&Seat>,
    party: Option<Res<crate::run::party::Party>>,
    mode: Option<Res<Mode>>,
) {
    // Their own Kit helps only this machine's Player; the Host builds each remote Player's Tracks
    // from what their machine brought to the Room.
    let local = locals.contains(add.entity);
    let (tracks, kit) = if local {
        (progress.as_ref().map(|file| file.tracks_on_the_wire()), chosen.map(|chosen| chosen.0))
    } else {
        let seat = seats.get(add.entity).ok();
        let brought = party.as_ref().and_then(|party| party.room()).zip(seat).and_then(|(room, seat)| room.unlocks(seat.0));
        (brought.map(|brought| brought.tracks.clone()), brought.and_then(|brought| Kit::from_id(&brought.kit)))
    };
    let tracks = tracks.unwrap_or_default();
    // Classic has no Kits, whatever a machine brought.
    let kit = kit.filter(|_| mode.is_none_or(|mode| *mode == Mode::Gauntlet)).unwrap_or_default();
    let track_banishes = progress::track_banishes_of(&tracks);
    let entries = progress::track_entries_of(&tracks);
    // The local save's Unlocks stand in for every Player's until the Room's `Hello` fills a
    // remote one's.
    let unlocked = progress.map_or_else(Vec::new, |file| file.unlocked.clone());
    // The local Player's entries came with their body, from the Run's modifiers; a remote
    // Player's, and their starting Points, come from their own Tracks.
    if !local && !entries.is_empty() {
        commands.entity(add.entity).insert((Modifiers(entries.clone()), Points::starting(&entries)));
    }
    // A remote Player's body starts, and Respawns, with their own Kit's Loadout, so the Host's copy
    // of their Weapons is the one their machine gave them. The local body got theirs from the Run.
    if !local {
        let start = kit.starting_loadout();
        commands.entity(add.entity).insert((start, start.loadout(), start.knife(), start.equipment()));
    }
    commands.entity(add.entity).insert((
        HeldBoons::default(),
        PendingOffer::default(),
        OwedBoost::default(),
        PlayerUnlocks(unlocked),
        Bought::default(),
        // Tight Offers' base is the Run's, so every Player starts with it.
        Banishes::starting_from(heat.map_or(GAUNTLET_BANISHES_START, |heat| heat.starting_banishes()), track_banishes),
        PlayerKit::new(kit),
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
    commands.insert_resource(RunArc::default());
    commands.insert_resource(win::WinWait::default());
    commands.insert_resource(win::WinClock::default());
    commands.insert_resource(arc::BossPlan::default());
    commands.insert_resource(arc::BossView::default());
    commands.insert_resource(RoundVariety::default());
    commands.insert_resource(crate::audio::RoundStartCue::default());
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
/// from their own Unlocks and Boons. After a Pact Round it's a Pact Offer, unless no Pact is left
/// to offer, and never one a teammate holds or has on their Offer; after a Boss Round it's a Boss
/// Offer, unless there's no Legendary to offer. With no eligible Boon there's no Offer. An Offer
/// still open from an earlier Round holds the next one back, and so does the Win screen (in Co-op,
/// every Player's, up to `GAUNTLET_WIN_COOP_TIMEOUT_SECS`).
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
        &Banishes,
        &mut PlayerKit,
        &mut PendingOffer,
        &mut OwedBoost,
    )>,
    pacts: Res<HeldPacts>,
    sources: tools::RunSources,
    run_type: Res<RunType>,
    time: Res<Time>,
    arc: Res<RunArc>,
    win_wait: Res<win::WinWait>,
    mut cues: MessageWriter<PlayCue>,
) {
    // In Co-op the Host's own screen is one of the choices the wait is for.
    if (arc.choosing && !run_type.is_coop()) || win_wait.holds() {
        return;
    }
    // Pacts on a Player's open Offer are spoken for, so two Offers drawn together never share one.
    let mut claimed: Vec<&'static str> = players
        .iter()
        .filter_map(|(_, _, _, offer, ..)| match &offer?.choices {
            Choices::Pacts(pacts) => Some(pacts.iter().map(|pact| pact.id())),
            Choices::Boons(_) => None,
        })
        .flatten()
        .collect();
    for (player, last_stand, local, offer, loadout, held, unlocks, bought, banishes, mut kit, mut pending, mut owed) in &mut players {
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
        let run = sources.facts(bought, loadout, held, unlocks, &banishes.banished, kit.kit.family());
        let roll = |total| rand::random_range(0.0..total);
        let pact_choices = if is_pact_round(round) {
            draw_pact_offer(PACTS, &pacts, &claimed, &run, roll)
        } else {
            Vec::new()
        };
        let boss_choices = if is_boss_round(round) {
            arc::draw_boss_offer(BOONS, duos::DUOS, held, &run, roll)
        } else {
            Vec::new()
        };
        let choices = if !boss_choices.is_empty() {
            Choices::Boons(boss_choices)
        } else if pact_choices.is_empty() {
            Choices::Boons(draw_offer(BOONS, held, &run, roll))
        } else {
            claimed.extend(pact_choices.iter().map(|pact| pact.id()));
            Choices::Pacts(pact_choices)
        };
        if choices.is_empty() {
            continue;
        }
        // The last Round's reward lands on the first Boon or Boss Offer: a Pact Offer carries it.
        let mut boost = Boost::default();
        let choices = match choices {
            Choices::Boons(cards) => {
                // The Round Choice's reward is every Player's. Looked at, not taken, until a boosted
                // draw has really made the Offer.
                let mut earned = owed.0.clone();
                // The Blade Kit's first Boon Offer holds a Blade card.
                let kit_boost = kits::first_offer_boost(kit.kit, !kit.offered);
                let first_offer = !kit.offered;
                let kit_adds_nothing = kit_boost.is_none();
                earned.merge(kit_boost);
                if earned.is_none() {
                    kit.offered = true;
                    Choices::Boons(cards)
                } else {
                    let boosted = variety::draw_cards_boosted(round, held, &run, &earned, roll);
                    if boosted.is_empty() {
                        // Nothing to draw boosted: the plain Offer stands, and what was earned
                        // waits for the next one.
                        kit.offered |= first_offer && kit_adds_nothing;
                        Choices::Boons(cards)
                    } else {
                        owed.0 = Boost::default();
                        kit.offered |= first_offer;
                        boost = earned;
                        Choices::Boons(boosted)
                    }
                }
            }
            pacts => pacts,
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
        let mut offer = Offer::new(choices, round);
        offer.picks = 1 + boost.extra_picks;
        offer.boost = boost;
        entity.insert(offer);
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

/// A click on the Reroll button asks for a Reroll; on a Banish button, the first click arms it
/// and the second asks for the Banish. Both wait out the input lockout.
fn click_tool(
    activate: On<Activate>,
    mut banishes: Query<&mut screen::BanishButton>,
    rerolls: Query<(), With<screen::RerollButton>>,
    player: Query<(Entity, &Offer), With<LocalPlayer>>,
    real: Res<Time<Real>>,
    mut reroll_requests: MessageWriter<RerollOffer>,
    mut banish_requests: MessageWriter<BanishCard>,
) {
    let Ok((player, offer)) = player.single() else {
        return;
    };
    if real.elapsed_secs() - offer.shown_at < BOON_OFFER_INPUT_LOCKOUT_SECS {
        return;
    }
    if rerolls.contains(activate.entity) {
        reroll_requests.write(RerollOffer { player });
        return;
    }
    let Ok(clicked) = banishes.get(activate.entity).map(|button| (button.card, button.armed)) else {
        return;
    };
    match clicked {
        (card, true) => {
            banish_requests.write(BanishCard { player, card });
        }
        (card, false) => {
            for mut button in &mut banishes {
                button.armed = button.card == card;
            }
        }
    }
}

/// A pick takes that Boon or Pact for its Player, or nothing on a skip, and closes their Offer.
/// A Pact goes to the picker, who holds its upside.
pub(crate) fn apply_pick(
    mut commands: Commands,
    mut picks: MessageReader<PickOffer>,
    mut players: Query<(&Seat, &mut Offer, &mut HeldBoons, Has<LocalPlayer>)>,
    mut pacts: ResMut<HeldPacts>,
    mut cues: MessageWriter<PlayCue>,
) {
    for &PickOffer { player, card } in picks.read() {
        let Ok((&seat, mut offer, mut held, local)) = players.get_mut(player) else {
            continue;
        };
        let mut more = false;
        match (card, &offer.choices) {
            (Some(index), Choices::Boons(boons)) => {
                let Some(&boon) = boons.get(index) else {
                    continue;
                };
                held.add(boon);
                // An extra pick leaves the other cards open to take another from.
                more = offer.picked + 1 < offer.picks && boons.len() > 1;
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
        if more {
            offer.picked += 1;
            if let (Some(index), Choices::Boons(boons)) = (card, &mut offer.choices) {
                boons.remove(index);
            }
            continue;
        }
        commands.entity(player).remove::<(Offer, Choosing)>();
    }
}

/// Boons and Pacts asked for with no Offer are taken as a pick would take them.
pub(crate) fn apply_grants(
    mut boons: MessageReader<GrantBoon>,
    mut pacts_asked: MessageReader<GrantPact>,
    mut players: Query<(&Seat, &mut HeldBoons), With<Player>>,
    mut pacts: ResMut<HeldPacts>,
) {
    for &GrantBoon { player, boon } in boons.read() {
        if let Ok((_, mut held)) = players.get_mut(player) {
            held.add(boon);
        }
    }
    for &GrantPact { player, pact } in pacts_asked.read() {
        if let Ok((&seat, _)) = players.get(player) {
            pacts.add(pact, seat);
        }
    }
}

/// Keeps the Intermission from starting while Offers are due or open. Solo that is until the pick,
/// with the world frozen for it; in Co-op the world runs and the wait ends after
/// `BOON_OFFER_COOP_TIMEOUT_SECS`, leaving any Offer still open. A Co-op Win holds it first, while
/// any Player is still to choose (`win::WinWait`), and the Offers' wait starts once that is over.
fn hold_intermission(
    players: Query<(&PendingOffer, Has<Offer>), With<Player>>,
    run_type: Res<RunType>,
    arc: Res<RunArc>,
    win_wait: Res<win::WinWait>,
    variety: Res<RoundVariety>,
    time: Res<Time>,
    mut wait: ResMut<OfferWait>,
    mut hold: ResMut<IntermissionHold>,
    mut freeze: ResMut<Freeze>,
) {
    // The Round Choice is the last step of the Intermission, and the Win screen goes before it.
    let choosing = variety.pending.is_some() && !arc.choosing;
    let busy = choosing
        || players
            .iter()
            .any(|(pending, open)| pending.left.is_some() || open);
    let (held, frozen) = if run_type.is_coop() && win_wait.holds() {
        wait.0 = BOON_OFFER_COOP_TIMEOUT_SECS;
        (true, false)
    } else if run_type.is_coop() {
        wait.0 = (wait.0 - time.delta_secs()).max(0.0);
        // The vote has its own clock, so the Intermission waits for it however the Offers went.
        let offers = players.iter().any(|(pending, open)| pending.left.is_some() || open);
        ((offers && wait.0 > 0.0) || choosing, false)
    } else {
        // The Win screen holds the world frozen too, until the Player chooses.
        (busy || arc.choosing, arc.choosing || variety.lit || players.iter().any(|(_, open)| open))
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
    round: Option<Res<Round>>,
    (variety, heat): (Res<RoundVariety>, Res<RunHeat>),
    mut players: Query<(&Seat, &HeldBoons, &sets::SetBonuses, &Loadout, &PlayerKit, &mut Modifiers), With<Player>>,
) {
    let ours = |source| {
        is_boon_source(source)
            || kits::is_kit_source(source)
            || is_pact_source(source)
            || sets::is_set_source(source)
            || is_arc_source(source)
            || variety::is_twist_source(source)
            || heat::is_heat_source(source)
    };
    let arc_entries = round.map_or_else(Vec::new, |round| {
        let mut entries = arc::arc_entries_with(round.number, &round.phase, heat.boss_horde_factor());
        entries.extend(variety.entries(round.number, &round.phase));
        entries
    });
    for (&seat, held, set_bonuses, loadout, kit, mut modifiers) in &mut players {
        let low = sets::magazine_low(loadout.gun_ammo().0, loadout.magazine_size(&modifiers.0));
        let mut wanted = held.modifiers();
        wanted.extend(pacts.modifiers(seat));
        wanted.extend(sets::set_entries(set_bonuses, low));
        wanted.extend(kit.kit.entries(kit.spent));
        wanted.extend(arc_entries.iter().copied());
        wanted.extend(heat.entries());
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

/// Request to flip `UnlockAll`. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleUnlockAll;

fn toggle_unlock_all(mut asked: MessageReader<ToggleUnlockAll>, mut unlock_all: ResMut<UnlockAll>) {
    for _ in asked.read() {
        unlock_all.0 = !unlock_all.0;
    }
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

/// The Run Record's Kit: the one the Run started with.
fn record_run_meta(chosen: Res<ChosenKit>, mut kit: ResMut<RunKit>) {
    let id = Some(chosen.0.id().to_string());
    if kit.0 != id {
        kit.0 = id;
    }
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
/// included, through Classic's `RunModifiers`, the Kit picked sets the Player's `StartingLoadout`,
/// the Run caps Turned allies at
/// `GAUNTLET_TURNED_ALLY_CAP`, and its Boss Rounds are kept clear of Classic's own schedules. A
/// Classic Run gets no entries, no cap and no reserved Rounds.
pub(crate) fn set_run_modifiers(
    mode: Res<Mode>,
    file: Res<GauntletFile>,
    party: Option<Res<crate::run::party::Party>>,
    mut run: ResMut<RunModifiers>,
    mut ranks: ResMut<ActiveRanks>,
    mut cap: ResMut<TurnedAllyCap>,
    mut reserved: ResMut<ReservedRounds>,
    (mut start, mut chosen): (ResMut<StartingLoadout>, ResMut<ChosenKit>),
    (mut heat, mut elites, mut record): (ResMut<RunHeat>, ResMut<elite::EliteRules>, ResMut<RunHeatRecord>),
) {
    if *mode == Mode::Gauntlet {
        run.0 = file.track_entries();
        ranks.0 = file.active_ranks();
        chosen.0 = chosen_kit(&file);
        *start = chosen.0.starting_loadout();
        *cap = TurnedAllyCap(Some(GAUNTLET_TURNED_ALLY_CAP));
        reserved.0 = arc::boss_rounds_through(arc::win_round()).collect();
        // In a Room every machine plays the Host's Degrees, not its own save's.
        *heat = match &party {
            Some(party) => RunHeat::from_room(party.heat()),
            None => file.run_heat(),
        };
    } else {
        *heat = RunHeat::default();
        run.0.clear();
        ranks.0 = 0;
        chosen.0 = Kit::Standard;
        *start = StartingLoadout::default();
        *cap = TurnedAllyCap(None);
        reserved.0.clear();
    }
    *elites = heat.elite_rules();
    *record = RunHeatRecord {
        heat: Some(heat.total()).filter(|&total| total > 0),
        conditions: heat.record_conditions(),
    };
}

/// The Run ended, at Game Over or on quit: each completed Round pays its number in Scrip, and each
/// cleared Boss Round its Act's bonus, saved straight away (the app may be exiting) and shown on
/// the Game Over screen, under the Win if the Run reached it.
fn pay_scrip(
    round: Res<Round>,
    arc: Res<RunArc>,
    (variety, heat): (Res<RoundVariety>, Res<RunHeat>),
    (log, earned, chosen): (Res<commendations::RunLog>, Res<commendations::RunCommendations>, Res<ChosenKit>),
    mut file: ResMut<GauntletFile>,
    dir: Res<SaveDir>,
    mut lines: ResMut<GameOverLines>,
    tainted: Res<crate::storage::records::RunTainted>,
) {
    // A Tainted Run pays nothing and says nothing of it.
    if tainted.0 {
        return;
    }
    let in_intermission = matches!(round.phase, RoundPhase::Intermission(_));
    // The Run's total (Rounds plus Act bonuses) is multiplied by Heat; Commendation Scrip never is.
    let total = scrip_for_rounds(rounds_completed(round.number, in_intermission)).saturating_add(arc.act_scrip);
    let scrip = heat::scaled_scrip(total, heat.total());
    file.pay(scrip);
    file.elites_killed = file.elites_killed.max(log.elites_total());
    store_progress(&file, &dir);
    if arc.won {
        lines.0.push("RUN WON".to_string());
    }
    lines.0.push(format!("Kit: {}", chosen.0.name()));
    if heat.total() > 0 {
        lines.0.push(heat.banner());
        lines.0.push(heat::conditions_line(&heat));
    }
    if let Some(line) = commendations::game_over_line(&earned) {
        lines.0.push(line);
    }
    if variety.challenges_met > 0 {
        lines.0.push(format!("Challenges met {}", variety.challenges_met));
    }
    if variety.twists_survived > 0 {
        lines.0.push(format!("Twists survived {}", variety.twists_survived));
    }
    lines
        .0
        .push(format!("Scrip earned {}", crate::ui::hud::with_commas(scrip)));
}

/// The ids of the first `count` Boons, for a test to offer.
#[cfg(test)]
pub fn sample_boon_ids(count: usize) -> Vec<String> {
    BOONS.iter().take(count).map(|boon| boon.id.to_string()).collect()
}

#[cfg(test)]
mod taint_tests {
    use super::*;
    use crate::storage::records::RunTainted;

    fn ended(tainted: bool) -> App {
        let mut app = App::new();
        app.init_resource::<RunArc>()
            .init_resource::<RoundVariety>()
            .init_resource::<heat::RunHeat>()
            .insert_resource(commendations::RunLog::new(0))
            .init_resource::<commendations::RunCommendations>()
            .init_resource::<ChosenKit>()
            .init_resource::<GauntletFile>()
            .insert_resource(SaveDir(None))
            .init_resource::<GameOverLines>()
            .insert_resource(RunTainted(tainted))
            .insert_resource(Round::in_progress(6))
            .add_systems(Update, pay_scrip);
        app.update();
        app
    }

    #[test]
    fn a_normal_run_pays_scrip() {
        let app = ended(false);
        assert!(app.world().resource::<GauntletFile>().scrip > 0);
        assert!(!app.world().resource::<GameOverLines>().0.is_empty());
    }

    #[test]
    fn a_tainted_run_pays_no_scrip() {
        let app = ended(true);
        let file = app.world().resource::<GauntletFile>();
        assert_eq!(file.scrip, 0);
        assert_eq!(file.elites_killed, 0);
        assert!(app.world().resource::<GameOverLines>().0.is_empty());
    }
}
