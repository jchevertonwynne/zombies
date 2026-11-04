//! Gauntlet: Classic with a Boon Offer after every Round, and a Pact Offer in its place every
//! fifth Round. Between Runs, the Scrip each Run pays buys Unlocks and Upgrade Track Ranks in the
//! Armory, kept in `gauntlet.ron`. It's layered on top of Classic and
//! reaches it only through seams (ADR 0002): stat-modifier entries on the Player, messages Classic
//! already sends, Classic state it reads, and the `Freeze` and `IntermissionHold` it holds. Every
//! system here that touches a Run runs only when `Mode == Gauntlet`; the Armory and the save file
//! live in the menus.

mod armory;
mod boon;
mod pact;
pub mod progress;
mod screen;

use bevy::input::common_conditions::input_just_pressed;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::audio::PlayCue;
use crate::audio::cues::Cue;
use crate::authority::HostSimulation;
use crate::buy::Purchased;
use crate::explosive::Explosion;
use crate::last_stand::LastStand;
use crate::menu::{GameOverLines, ResetGauntletProgress};
use crate::mode::Mode;
use crate::modifier::{Modifiers, RunModifiers};
use crate::mystery_box::{MysteryBox, TeddyBear};
use crate::perk::{Perk, PerkMachine};
use crate::player::{LocalPlayer, Player, Seat};
use crate::points::BonusPoints;
use crate::power::Power;
use crate::power_up::{DropPowerUp, GivePowerUp, PowerUp, PowerUpTaken};
use crate::records::{RecordExtra, RecordExtras};
use crate::round::{IntermissionHold, Round, RoundEnded, RoundPhase};
use crate::save;
use crate::settings::SaveDir;
use crate::state::{Freeze, GameState, MainMenuScreen, Pause, input_runs};
use crate::tuning::*;
use crate::weapon::Loadout;
use crate::zombie::{DamageZombie, Dying, HitZone, KillDamage, Zombie, ZombieKilled};
use boon::{
    BLOOD_MONEY, BOONS, BOUNTY, BUTCHERS_BILL, BoonDef, HeldBoons, LUCKY_STRIKE, RunFacts,
    TEDDYS_TITHE, WAR_BONDS, blood_money_points, bounty_points, butchers_bill_drops, draw_offer,
    is_boon_source, lucky_strike_gift, teddys_tithe_points, war_bonds_points,
};
use pact::{
    HeldPacts, OVERKILL, PACTS, PactDef, SCORCHED_EARTH, draw_pact_offer, is_pact_round,
    is_pact_source, overkill_target, scorched_earth_blast,
};
use progress::{GauntletFile, rounds_completed, scrip_for_rounds};
use screen::{OfferCard, SkipButton};

/// Seconds left before a Player's Boon Offer opens, counted from the Round's last kill.
#[derive(Component, Default)]
struct PendingOffer(Option<f32>);

/// What the Player has bought this Run that Requirements ask about, from the Purchased messages
/// Classic sends.
#[derive(Resource, Default)]
struct Bought {
    from_box: bool,
    quick_revive: bool,
}

/// Power-ups Lucky Strike has given whose pickup hasn't come back yet, so a gift never rolls for
/// another.
#[derive(Resource, Default)]
struct Gifts(Vec<PowerUp>);

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
struct Offer {
    choices: Choices,
    /// The Round just completed.
    round: u32,
    /// When the cards last appeared, in real seconds, for the input lockout.
    shown_at: f32,
}

pub struct GauntletPlugin;

impl Plugin for GauntletPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldPacts>()
            .init_resource::<Bought>()
            .init_resource::<Gifts>()
            .init_resource::<UnlockAll>()
            .init_resource::<ActiveRanks>()
            .init_resource::<RunModifiers>()
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
                (reset, screen::spawn_held_column).chain().run_if(gauntlet),
            )
            .add_systems(
                Update,
                (
                    (
                        schedule_offer,
                        open_offer.run_if(input_runs),
                        track_purchases,
                        blood_money,
                        teddys_tithe,
                        (war_bonds, bounty, butchers_bill, lucky_strike),
                        (scorched_earth, overkill),
                    )
                        .in_set(HostSimulation),
                    apply_entries,
                    (record_picks, screen::refresh_held_column).run_if(picks_changed),
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
                screen::highlight_cards
                    .run_if(in_state(Pause::Frozen))
                    .run_if(gauntlet),
            )
            .add_observer(give_gauntlet_state)
            .add_observer(pick);
    }
}

fn gauntlet(mode: Res<Mode>) -> bool {
    *mode == Mode::Gauntlet
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
fn give_gauntlet_state(add: On<Add<Player>>, mut commands: Commands) {
    commands
        .entity(add.entity)
        .insert((HeldBoons::default(), PendingOffer::default()));
}

fn reset(mut commands: Commands) {
    commands.insert_resource(HeldPacts::default());
    commands.insert_resource(Bought::default());
    commands.insert_resource(Gifts::default());
}

/// The Round ended: the Offer is due after its delay, and the Intermission waits for it.
fn schedule_offer(
    mut ended: MessageReader<RoundEnded>,
    mut pending: Query<&mut PendingOffer, With<Player>>,
    mut hold: ResMut<IntermissionHold>,
) {
    if ended.read().count() > 0 {
        for mut pending in &mut pending {
            pending.0 = Some(BOON_OFFER_DELAY_SECS);
        }
        hold.0 = true;
    }
}

/// Opens the Offer once its delay is up, freezing the world. A Round that ended in Last Stand
/// waits for the Self-Revive. Every fifth Round it's a Pact Offer, unless no Pact is left to
/// offer. With no eligible Boon there's no Offer, and the Intermission starts.
fn open_offer(
    mut commands: Commands,
    mut player: Query<
        (Entity, Has<LastStand>, &Loadout, &HeldBoons, &mut PendingOffer),
        With<LocalPlayer>,
    >,
    pacts: Res<HeldPacts>,
    bought: Res<Bought>,
    (unlock_all, progress): (Res<UnlockAll>, Res<GauntletFile>),
    power: Option<Res<Power>>,
    round: Res<Round>,
    time: Res<Time>,
    mut hold: ResMut<IntermissionHold>,
    mut freeze: ResMut<Freeze>,
    mut cues: MessageWriter<PlayCue>,
) {
    let Ok((player, last_stand, loadout, held, mut pending)) = player.single_mut() else {
        return;
    };
    let Some(left) = pending.0.as_mut() else {
        return;
    };
    *left -= time.delta_secs();
    if *left > 0.0 || last_stand {
        return;
    }
    pending.0 = None;
    let run = RunFacts {
        power_on: power.is_some_and(|power| power.is_on()),
        held_pacts: pacts.ids(),
        used_box: bought.from_box,
        bought_quick_revive: bought.quick_revive,
        holds_upgraded: loadout.weapons().any(|weapon| weapon.upgraded),
        holds_ammo_mod: loadout.weapons().any(|weapon| !weapon.ammo_mods.is_empty()),
        unlocked: progress.unlocked.clone(),
        unlock_all: unlock_all.0,
    };
    let roll = |total| rand::random_range(0.0..total);
    let pact_choices = if is_pact_round(round.number) {
        draw_pact_offer(PACTS, &pacts, &run, roll)
    } else {
        Vec::new()
    };
    let choices = if pact_choices.is_empty() {
        Choices::Boons(draw_offer(BOONS, held, &run, roll))
    } else {
        Choices::Pacts(pact_choices)
    };
    if choices.is_empty() {
        hold.0 = false;
        return;
    }
    let cue = match choices {
        Choices::Boons(_) => Cue::BoonOffer,
        Choices::Pacts(_) => Cue::PactOffer,
    };
    cues.write(PlayCue { cue, at: None });
    commands.entity(player).insert(Offer {
        choices,
        round: round.number,
        shown_at: 0.0,
    });
    freeze.0 = true;
}

/// A click on a card, once the input lockout is over, takes that Boon or Pact; on a Pact Offer's
/// Skip button, it takes nothing. The world runs again and the Intermission starts.
fn pick(
    activate: On<Activate>,
    mut commands: Commands,
    cards: Query<&OfferCard>,
    skips: Query<(), With<SkipButton>>,
    mut player: Query<(Entity, &Seat, &Offer, &mut HeldBoons), With<LocalPlayer>>,
    real: Res<Time<Real>>,
    mut pacts: ResMut<HeldPacts>,
    mut freeze: ResMut<Freeze>,
    mut hold: ResMut<IntermissionHold>,
    mut cues: MessageWriter<PlayCue>,
) {
    let Ok((player, &seat, offer, mut held)) = player.single_mut() else {
        return;
    };
    let card = cards.get(activate.entity).ok();
    let skip = skips.contains(activate.entity) && matches!(offer.choices, Choices::Pacts(_));
    if card.is_none() && !skip {
        return;
    }
    if real.elapsed_secs() - offer.shown_at < BOON_OFFER_INPUT_LOCKOUT_SECS {
        return;
    }
    if let Some(OfferCard(index)) = card {
        match &offer.choices {
            Choices::Boons(boons) => {
                let Some(&boon) = boons.get(*index) else {
                    return;
                };
                held.add(boon);
            }
            Choices::Pacts(choices) => {
                let Some(&pact) = choices.get(*index) else {
                    return;
                };
                pacts.add(pact, seat);
            }
        }
    }
    cues.write(PlayCue {
        cue: Cue::OfferPick,
        at: None,
    });
    commands.entity(player).remove::<Offer>();
    freeze.0 = false;
    hold.0 = false;
}

/// Keeps the Player's Boon and Pact entries in step with the Boons and Pacts held. A Down takes
/// only Perk entries, so these survive it, and a Pact's Drawback lasts the Run.
fn apply_entries(
    pacts: Res<HeldPacts>,
    mut players: Query<(&Seat, &HeldBoons, &mut Modifiers), With<Player>>,
) {
    let ours = |source| is_boon_source(source) || is_pact_source(source);
    for (&seat, held, mut modifiers) in &mut players {
        let mut wanted = held.modifiers();
        wanted.extend(pacts.modifiers(seat));
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

/// Scorched Earth: every Zombie killed by a hit explodes, never hurting the Player.
fn scorched_earth(
    mut kills: MessageReader<KillDamage>,
    pacts: Res<HeldPacts>,
    mut explosions: MessageWriter<Explosion>,
) {
    let held = pacts.holds(SCORCHED_EARTH.id);
    for kill in kills.read() {
        if held {
            explosions.write(scorched_earth_blast(kill.at, kill.max_health, kill.player));
        }
    }
}

/// Overkill: a kill's damage beyond what it took carries to the nearest Zombie, by the same
/// attack.
fn overkill(
    mut kills: MessageReader<KillDamage>,
    pacts: Res<HeldPacts>,
    zombies: Query<(Entity, &GlobalTransform), (With<Zombie>, Without<Dying>)>,
    mut damage: MessageWriter<DamageZombie>,
) {
    let held = pacts.holds(OVERKILL.id);
    for kill in kills.read() {
        if !held {
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

fn blood_money(
    mut kills: MessageReader<ZombieKilled>,
    mut bonus: MessageWriter<BonusPoints>,
    player: Single<(Entity, &HeldBoons), With<LocalPlayer>>,
) {
    let (player, held) = *player;
    let stacks = held.stacks(BLOOD_MONEY);
    for kill in kills.read() {
        let points = blood_money_points(kill.pay, stacks);
        if points > 0 {
            bonus.write(BonusPoints {
                player,
                amount: points,
            });
        }
    }
}

/// Teddy's Tithe: a Teddy Bear pays Points on top of its refund.
fn teddys_tithe(
    mut teddies: MessageReader<TeddyBear>,
    mut bonus: MessageWriter<BonusPoints>,
    player: Single<(Entity, &HeldBoons), With<LocalPlayer>>,
) {
    let (player, held) = *player;
    let points = teddys_tithe_points(held.stacks(TEDDYS_TITHE));
    for _ in teddies.read() {
        if points > 0 {
            bonus.write(BonusPoints {
                player,
                amount: points,
            });
        }
    }
}

/// Notes the purchases Requirements ask about: anything from a Mystery Box, and Quick Revive.
fn track_purchases(
    mut purchases: MessageReader<Purchased>,
    boxes: Query<(), With<MysteryBox>>,
    machines: Query<&PerkMachine>,
    mut bought: ResMut<Bought>,
) {
    for &Purchased { buyable, .. } in purchases.read() {
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

/// War Bonds: each Round's end pays Points by its number.
fn war_bonds(
    mut ended: MessageReader<RoundEnded>,
    round: Res<Round>,
    mut bonus: MessageWriter<BonusPoints>,
    player: Single<(Entity, &HeldBoons), With<LocalPlayer>>,
) {
    let (player, held) = *player;
    let stacks = held.stacks(WAR_BONDS);
    for _ in ended.read() {
        let points = war_bonds_points(round.number, stacks);
        if points > 0 {
            bonus.write(BonusPoints {
                player,
                amount: points,
            });
        }
    }
}

/// Bounty: a Trap's kill pays Points.
fn bounty(
    mut kills: MessageReader<ZombieKilled>,
    mut bonus: MessageWriter<BonusPoints>,
    player: Single<(Entity, &HeldBoons), With<LocalPlayer>>,
) {
    let (player, held) = *player;
    let stacks = held.stacks(BOUNTY);
    for kill in kills.read() {
        let points = bounty_points(kill.attack, stacks);
        if points > 0 {
            bonus.write(BonusPoints {
                player,
                amount: points,
            });
        }
    }
}

/// Butcher's Bill: a Knife kill may drop a Power-up where it fell, outside the per-Round max.
fn butchers_bill(
    mut kills: MessageReader<ZombieKilled>,
    held: Single<&HeldBoons, With<LocalPlayer>>,
    mut drops: MessageWriter<DropPowerUp>,
) {
    let stacks = held.stacks(BUTCHERS_BILL);
    for kill in kills.read() {
        if stacks > 0 && butchers_bill_drops(kill.attack, stacks, rand::random()) {
            drops.write(DropPowerUp {
                at: kill.at,
                power_up: None,
            });
        }
    }
}

/// Lucky Strike: picking up a Power-up may also give a different one. The gift's own pickup
/// doesn't roll again.
fn lucky_strike(
    mut taken: MessageReader<PowerUpTaken>,
    held: Single<&HeldBoons, With<LocalPlayer>>,
    mut gifts: ResMut<Gifts>,
    mut give: MessageWriter<GivePowerUp>,
) {
    let stacks = held.stacks(LUCKY_STRIKE);
    for PowerUpTaken(power_up) in taken.read() {
        if let Some(i) = gifts.0.iter().position(|gift| gift == power_up) {
            gifts.0.remove(i);
            continue;
        }
        if stacks == 0 {
            continue;
        }
        if let Some(gift) = lucky_strike_gift(*power_up, stacks, rand::random(), rand::random()) {
            gifts.0.push(gift);
            give.write(GivePowerUp(gift));
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
/// included, through Classic's `RunModifiers`. A Classic Run gets none.
fn set_run_modifiers(
    mode: Res<Mode>,
    file: Res<GauntletFile>,
    mut run: ResMut<RunModifiers>,
    mut ranks: ResMut<ActiveRanks>,
) {
    if *mode == Mode::Gauntlet {
        run.0 = file.track_entries();
        ranks.0 = file.active_ranks();
    } else {
        run.0.clear();
        ranks.0 = 0;
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
        .push(format!("Scrip earned {}", crate::hud::with_commas(scrip)));
}
