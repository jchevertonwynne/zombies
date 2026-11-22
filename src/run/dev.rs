//! Developer mode: a layer on Classic for testing, silent until a Cheat is used (ADR 0010).
//!
//! It reaches Classic only through neutral seams (`BonusPoints`, `GivePowerUp`, `GiveWeapon`,
//! `GivePerk`, the Gauntlet layer's `GrantBoon` and `GrantPact`, `TaintRun`, the `Invincible`
//! marker and stat entries for `Stat::Price` and `Stat::AmmoSpent`), and never writes a component
//! a layer rewrites. Solo it is always allowed. In Co-op
//! the Host sets it in the Room to Off, On (every Player) or Host only, and can change it during
//! the Run. Every Cheat is a request the Host judges: one the setting forbids is dropped without a
//! word, and one that goes through taints the Run (`TaintRun`), which the Host tells to everyone.
//!
//! The Cheat Panel that sends the requests is in `ui::cheat_panel`; the wire is in `net::world`.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub mod oneshots;

use crate::authority::HostSimulation;
use crate::economy::perk::{GivePerk, Perk};
use crate::economy::points::BonusPoints;
use crate::economy::power_up::{GivePowerUp, PowerUp};
use crate::modifier::{Effect, Modifier, Modifiers, Source, Stat};
use crate::players::last_stand::LastStand;
use crate::players::player::{Invincible, LocalPlayer, Noclip, Player, Seat};
use crate::run::gauntlet::{GrantBoon, GrantPact, boon_catalog, pact_catalog};
use crate::run::mode::Mode;
use crate::weapons::arsenal::WeaponId;
use crate::weapons::weapon::GiveWeapon;
use crate::players::respawn::Disconnected;
use crate::run::party::Party;
use crate::state::{GameState, RunType};
use crate::storage::records::TaintRun;
use crate::weapons::equipment::Equipment;
use crate::weapons::weapon::Loadout;

use oneshots::{RoundJump, Spawn, Spawnable, WorldCheats};

/// The Points the Points Cheat gives.
pub const CHEAT_POINTS: u64 = 10_000;

/// Who may use Cheats in a Run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevMode {
    /// Nobody: the Cheat Panel does not open.
    #[default]
    Off,
    /// Every Player.
    On,
    /// The Host alone: a client's panel opens with every button disabled.
    HostOnly,
}

impl DevMode {
    /// The next choice round the Room's button: Off, On, Host only.
    pub fn next(self) -> Self {
        match self {
            DevMode::Off => DevMode::On,
            DevMode::On => DevMode::HostOnly,
            DevMode::HostOnly => DevMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DevMode::Off => "Off",
            DevMode::On => "On",
            DevMode::HostOnly => "Host only",
        }
    }

    /// Whether a Player may use Cheats: the Host (Seat 0) or anyone, as it is set.
    pub fn allows(self, host: bool) -> bool {
        match self {
            DevMode::Off => false,
            DevMode::On => true,
            DevMode::HostOnly => host,
        }
    }
}

/// One Cheat, as a Player asks for it. A toggle would stay on for the Player who set it; a
/// one-shot acts at once.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cheat {
    /// One-shot: `CHEAT_POINTS` Points.
    Points,
    /// Toggle: nothing hurts or Downs the Player.
    Invincible,
    /// Toggle: every purchase is free; kills still pay.
    FreePurchases,
    /// Toggle: Magazines never empty and Reserve Ammo never drops, Equipment included.
    InfiniteAmmo,
    /// Toggle: free flight, no gravity and no map collision. Predicted on the Player's own machine
    /// (a `Step` bit the Host replays), so the Host's word is only that it taints the Run.
    Noclip,
    /// One-shot: begin another Round now, forwards or back, the Horde in play removed.
    RoundJump(RoundJump),
    /// One-shot: every Zombie, Hellhound and Boss in play dies, Unpaid.
    KillAll,
    /// One-shot: a Zombie of a kind comes up about ten metres ahead of the Player.
    Spawn(Spawn),
    /// One-shot: every Door opens, free.
    OpenAllDoors,
    /// One-shot: the Power comes on and every Generator is Claimed.
    PowerOn,
    /// One-shot: the Tactical slot is filled with Monkey Bombs, as the Mystery Box would.
    MonkeyBombs,
    /// One-shot: the Held Generator nearest the Player is Lost, as a Sapper would take it.
    LoseGenerator,
    /// One-shot: the Lever-driven bridge nearest the Player swings now: no price, warning or cooldown.
    SwingBridge,
    /// One-shot: the Tide turns to its next phase now, with no bells.
    TurnTide,
    /// One-shot, Gauntlet only: flips whether every Boon and Pact counts as Unlocked.
    UnlockAll,
    /// One-shot grant: the Power-up, applied at once.
    PowerUp(PowerUp),
    /// One-shot grant: the Weapon, Pack-a-Punched or not, in place of the one in hand if full.
    Weapon(WeaponId, bool),
    /// One-shot grant: the Perk, held at once.
    Perk(Perk),
    /// One-shot grant, Gauntlet only: a Boon, by its place in `boon_catalog` (one more Stack if held).
    Boon(u16),
    /// One-shot grant, Gauntlet only: a Pact, by its place in `pact_catalog`.
    Pact(u8),
}

/// The grants the Cheat Panel groups under one tab each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GrantKind {
    #[default]
    PowerUps,
    Weapons,
    PackedWeapons,
    Perks,
    Boons,
    Pacts,
    /// Not grants: the odd world one-shots, in the same grid.
    Debug,
}

impl GrantKind {
    pub const ALL: [GrantKind; 7] = [
        GrantKind::PowerUps,
        GrantKind::Weapons,
        GrantKind::PackedWeapons,
        GrantKind::Perks,
        GrantKind::Boons,
        GrantKind::Pacts,
        GrantKind::Debug,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GrantKind::PowerUps => "Power-ups",
            GrantKind::Weapons => "Weapons",
            GrantKind::PackedWeapons => "Pack-a-Punched",
            GrantKind::Perks => "Perks",
            GrantKind::Boons => "Boons",
            GrantKind::Pacts => "Pacts",
            GrantKind::Debug => "Debug",
        }
    }

    /// Every Cheat of this kind.
    pub fn cheats(self) -> Vec<Cheat> {
        match self {
            GrantKind::PowerUps => PowerUp::ALL.map(Cheat::PowerUp).to_vec(),
            GrantKind::Weapons => WeaponId::ALL.map(|id| Cheat::Weapon(id, false)).to_vec(),
            GrantKind::PackedWeapons => WeaponId::ALL.map(|id| Cheat::Weapon(id, true)).to_vec(),
            GrantKind::Perks => Perk::ALL.map(Cheat::Perk).to_vec(),
            GrantKind::Boons => (0..boon_catalog().count() as u16).map(Cheat::Boon).collect(),
            GrantKind::Pacts => (0..pact_catalog().len() as u8).map(Cheat::Pact).collect(),
            GrantKind::Debug => Cheat::DEBUG.to_vec(),
        }
    }
}

impl Cheat {
    pub const ALL: [Cheat; 5] = [Cheat::Points, Cheat::Invincible, Cheat::FreePurchases, Cheat::InfiniteAmmo, Cheat::Noclip];

    /// The one-shots on the Round, Doors and Power that the panel lists after the rest.
    pub const WORLD: [Cheat; 8] = [
        Cheat::RoundJump(RoundJump::Back5),
        Cheat::RoundJump(RoundJump::Back),
        Cheat::RoundJump(RoundJump::Forward),
        Cheat::RoundJump(RoundJump::Forward5),
        Cheat::RoundJump(RoundJump::NextTenth),
        Cheat::KillAll,
        Cheat::OpenAllDoors,
        Cheat::PowerOn,
    ];

    /// The one-shots that were debug hotkeys, on the panel's Debug tab.
    pub const DEBUG: [Cheat; 5] = [Cheat::MonkeyBombs, Cheat::LoseGenerator, Cheat::SwingBridge, Cheat::TurnTide, Cheat::UnlockAll];

    /// One Spawn button per kind: the panel puts the Elite tick on at the press.
    pub fn spawns() -> impl Iterator<Item = Cheat> {
        Spawnable::ALL.into_iter().map(|kind| Cheat::Spawn(Spawn { kind, elite: false }))
    }

    pub fn label(self) -> &'static str {
        match self {
            Cheat::Points => "+10,000 Points",
            Cheat::Invincible => "Invincibility",
            Cheat::FreePurchases => "Free purchases",
            Cheat::InfiniteAmmo => "Infinite ammo",
            Cheat::Noclip => "Noclip",
            Cheat::RoundJump(jump) => jump.label(),
            Cheat::KillAll => "Kill all",
            Cheat::Spawn(spawn) => spawn.label(),
            Cheat::OpenAllDoors => "Open all Doors",
            Cheat::PowerOn => "Power on",
            Cheat::MonkeyBombs => "Monkey Bombs",
            Cheat::LoseGenerator => "Lose nearest Generator",
            Cheat::SwingBridge => "Swing nearest bridge",
            Cheat::TurnTide => "Turn the Tide",
            Cheat::UnlockAll => "Unlock all (toggle)",
            Cheat::PowerUp(power_up) => power_up.name(),
            Cheat::Weapon(id, false) => id.stats().name,
            Cheat::Weapon(id, true) => id.upgraded().name,
            Cheat::Perk(perk) => perk.name(),
            Cheat::Boon(index) => boon_catalog().nth(index.into()).map_or("?", |boon| boon.name),
            Cheat::Pact(index) => pact_catalog().get(usize::from(index)).map_or("?", |pact| pact.name()),
        }
    }

    /// Whether the Run's Mode has anything for it to act on: a Boon or Pact needs the Gauntlet,
    /// and a place in the catalogue.
    pub fn available(self, mode: Mode) -> bool {
        match self {
            Cheat::Boon(index) => mode == Mode::Gauntlet && usize::from(index) < boon_catalog().count(),
            Cheat::Pact(index) => mode == Mode::Gauntlet && usize::from(index) < pact_catalog().len(),
            Cheat::UnlockAll => mode == Mode::Gauntlet,
            _ => true,
        }
    }

    /// Whether it stays on for the Player until they switch it off, as against acting at once.
    pub fn is_toggle(self) -> bool {
        matches!(self, Cheat::Invincible | Cheat::FreePurchases | Cheat::InfiniteAmmo | Cheat::Noclip)
    }
}

/// The writers for the grants, so `apply_cheats` stays short.
#[derive(SystemParam)]
struct Grants<'w> {
    power_ups: MessageWriter<'w, GivePowerUp>,
    weapons: MessageWriter<'w, GiveWeapon>,
    perks: MessageWriter<'w, GivePerk>,
    boons: MessageWriter<'w, GrantBoon>,
    pacts: MessageWriter<'w, GrantPact>,
}

impl Grants<'_> {
    /// Sends the grant `cheat` is, if it is one.
    fn give(&mut self, player: Entity, cheat: Cheat) {
        match cheat {
            Cheat::PowerUp(power_up) => {
                self.power_ups.write(GivePowerUp(power_up, Some(player)));
            }
            Cheat::Weapon(id, upgraded) => {
                self.weapons.write(GiveWeapon { player, id, upgraded });
            }
            Cheat::Perk(perk) => {
                self.perks.write(GivePerk { player, perk });
            }
            Cheat::Boon(index) => {
                if let Some(boon) = boon_catalog().nth(index.into()) {
                    self.boons.write(GrantBoon { player, boon });
                }
            }
            Cheat::Pact(index) => {
                if let Some(pact) = pact_catalog().get(usize::from(index)) {
                    self.pacts.write(GrantPact { player, pact });
                }
            }
            _ => {}
        }
    }

}

/// The source of the stat entries the toggles put on a Player.
pub const CHEAT_SOURCE: Source = Source("Cheats");

/// The Run's Developer mode setting, as this machine has it. Solo it is `On`; in Co-op it is the
/// Room's, and the Host's word changes it.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct DevAccess(pub DevMode);

/// What a Player has switched on or done with the Cheats, kept on their Player (so by Seat, and
/// through a Rejoin) and gone when the Host sets Developer mode to Off. A Player who has used none
/// has no `Cheats`. The Host's; a client has the toggles of its own Player as the Host tells it, so
/// it can resolve the stats that are its own (ammo) and show the panel.
#[derive(Component, Default, Debug, Clone, PartialEq, Eq)]
pub struct Cheats {
    /// How many Points the Points Cheat has given them.
    pub points_given: u64,
    pub invincible: bool,
    pub free_purchases: bool,
    pub infinite_ammo: bool,
    pub noclip: bool,
}

impl Cheats {
    const INVINCIBLE: u8 = 1;
    const FREE_PURCHASES: u8 = 2;
    const INFINITE_AMMO: u8 = 4;
    const NOCLIP: u8 = 8;

    /// Whether this toggle is on. A one-shot never is.
    pub fn on(&self, cheat: Cheat) -> bool {
        match cheat {
            Cheat::Invincible => self.invincible,
            Cheat::FreePurchases => self.free_purchases,
            Cheat::InfiniteAmmo => self.infinite_ammo,
            Cheat::Noclip => self.noclip,
            _ => false,
        }
    }

    fn flip(&mut self, cheat: Cheat) {
        match cheat {
            Cheat::Invincible => self.invincible = !self.invincible,
            Cheat::FreePurchases => self.free_purchases = !self.free_purchases,
            Cheat::InfiniteAmmo => self.infinite_ammo = !self.infinite_ammo,
            Cheat::Noclip => self.noclip = !self.noclip,
            _ => {}
        }
    }

    /// The toggles as the Host tells them to their Player.
    pub fn toggles(cheats: Option<&Cheats>) -> u8 {
        cheats.map_or(0, |cheats| {
            (u8::from(cheats.invincible) * Self::INVINCIBLE)
                | (u8::from(cheats.free_purchases) * Self::FREE_PURCHASES)
                | (u8::from(cheats.infinite_ammo) * Self::INFINITE_AMMO)
                | (u8::from(cheats.noclip) * Self::NOCLIP)
        })
    }

    /// The toggles a Host told, as the Player's `Cheats`; none on means none held.
    pub fn from_toggles(toggles: u8) -> Option<Cheats> {
        (toggles != 0).then_some(Cheats {
            points_given: 0,
            invincible: toggles & Self::INVINCIBLE != 0,
            free_purchases: toggles & Self::FREE_PURCHASES != 0,
            infinite_ammo: toggles & Self::INFINITE_AMMO != 0,
            noclip: toggles & Self::NOCLIP != 0,
        })
    }

    /// The stat entries the toggles put on the Player.
    fn entries(&self) -> Vec<Modifier> {
        let entry = |stat, effect| Modifier { stat, source: CHEAT_SOURCE, effect };
        let mut entries = Vec::new();
        if self.free_purchases {
            entries.push(entry(Stat::Price, Effect::Mult(0.0)));
        }
        if self.infinite_ammo {
            entries.push(entry(Stat::AmmoSpent, Effect::Mult(0.0)));
        }
        entries
    }
}

/// A Player asks for a Cheat. The Host (or solo) judges it; a client's is sent to the Host.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheatRequest {
    pub player: Entity,
    pub cheat: Cheat,
}

/// The Host sets Developer mode during a Co-op Run.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetDevMode(pub DevMode);

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        oneshots::add_messages(app);
        app.init_resource::<DevAccess>()
            .add_message::<CheatRequest>()
            .add_message::<SetDevMode>()
            .add_systems(OnEnter(GameState::Playing), start_access.after(crate::players::player::SpawnPlayers))
            .add_systems(Update, follow_noclip.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                (set_dev_mode, apply_cheats)
                    .chain()
                    .in_set(HostSimulation)
                    .run_if(in_state(GameState::Playing)),
            )
            // On every machine: each resolves the Cheats' effects that are its own.
            .add_systems(Update, (sync_cheat_effects, top_up_ammo).chain().run_if(in_state(GameState::Playing)));
    }
}

/// A Run starts with the Room's setting, or `On` solo.
fn start_access(mut access: ResMut<DevAccess>, party: Option<Res<Party>>) {
    access.0 = party.map_or(DevMode::On, |party| party.dev());
}

/// The Host changes the setting mid-Run. Off takes every Player's Cheats away; the Run stays
/// Tainted. Solo has no choice to make.
fn set_dev_mode(
    mut commands: Commands,
    mut sets: MessageReader<SetDevMode>,
    run_type: Res<RunType>,
    mut access: ResMut<DevAccess>,
    party: Option<ResMut<Party>>,
    cheats: Query<Entity, With<Cheats>>,
) {
    let Some(&SetDevMode(dev)) = sets.read().last() else { return };
    if !run_type.is_coop() || access.0 == dev {
        return;
    }
    access.0 = dev;
    if let Some(mut party) = party {
        party.set_dev(dev);
    }
    if dev == DevMode::Off {
        for player in &cheats {
            commands.entity(player).remove::<Cheats>();
        }
    }
}

/// Judges each request: from a Player the setting allows it does the Cheat and taints the Run;
/// otherwise it is dropped silently. A toggle's request flips it.
fn apply_cheats(
    mut commands: Commands,
    mut requests: MessageReader<CheatRequest>,
    access: Res<DevAccess>,
    players: Query<(&Seat, Has<Disconnected>, Has<LastStand>), With<Player>>,
    mode: Option<Res<Mode>>,
    mut cheats: Query<&mut Cheats>,
    mut bonus: MessageWriter<BonusPoints>,
    mut grants: Grants,
    mut taint: MessageWriter<TaintRun>,
    mut world: WorldCheats,
) {
    let mode = mode.map_or(Mode::Classic, |mode| *mode);
    // Cheats a Player has just got their first of, which the world doesn't have yet.
    let mut fresh: Vec<(Entity, Cheats)> = Vec::new();
    for &CheatRequest { player, cheat } in requests.read() {
        let Ok((seat, away, down)) = players.get(player) else { continue };
        if away || !access.0.allows(seat.0 == 0) || !cheat.available(mode) {
            continue;
        }
        // A Player in Last Stand has no hands to take a Weapon or a Perk into.
        if down && matches!(cheat, Cheat::Weapon(..) | Cheat::Perk(_) | Cheat::MonkeyBombs) {
            continue;
        }
        // The one-shots on the Round, Zombies, Doors and Power: the Run is Tainted only when one
        // was really done.
        if let Some(done) = world.apply(player, cheat) {
            if done {
                taint.write(TaintRun);
            }
            continue;
        }
        taint.write(TaintRun);
        if matches!(cheat, Cheat::PowerUp(_) | Cheat::Weapon(..) | Cheat::Perk(_) | Cheat::Boon(_) | Cheat::Pact(_)) {
            grants.give(player, cheat);
            continue;
        }
        if cheat == Cheat::Points {
            bonus.write(BonusPoints { player, amount: CHEAT_POINTS });
        }
        let held = match cheats.get_mut(player) {
            Ok(held) => held.into_inner(),
            Err(_) => match fresh.iter().position(|(who, _)| *who == player) {
                Some(at) => &mut fresh[at].1,
                None => {
                    fresh.push((player, Cheats::default()));
                    &mut fresh.last_mut().expect("just pushed").1
                }
            },
        };
        match cheat {
            Cheat::Points => held.points_given += CHEAT_POINTS,
            toggle => held.flip(toggle),
        }
    }
    for (player, held) in fresh {
        commands.entity(player).insert(held);
    }
}

/// On the Player's own machine (solo, the Host's or a client's), a Noclip request flips flight on
/// the `LocalPlayer` at once, so the step is predicted, and the Host judges the same request.
/// Flight ends when Developer mode stops allowing it. When the Host's word on the toggle changes
/// (a Rejoin tells it again), flight follows it.
fn follow_noclip(
    mut commands: Commands,
    mut requests: MessageReader<CheatRequest>,
    access: Res<DevAccess>,
    mut told: Local<bool>,
    local: Query<(Entity, &Seat, Option<&Cheats>, Has<Noclip>), With<LocalPlayer>>,
) {
    let Ok((player, seat, cheats, mut flying)) = local.single() else { return };
    let allowed = access.0.allows(seat.0 == 0);
    let hosts = cheats.is_some_and(|cheats| cheats.noclip);
    if hosts != *told {
        *told = hosts;
        flying = hosts;
    }
    for request in requests.read() {
        if allowed && request.player == player && request.cheat == Cheat::Noclip {
            flying = !flying;
        }
    }
    if !allowed {
        flying = false;
    }
    match (flying, local.single().is_ok_and(|(.., had)| had)) {
        (true, false) => {
            commands.entity(player).insert(Noclip);
        }
        (false, true) => {
            commands.entity(player).remove::<Noclip>();
        }
        _ => {}
    }
}

/// Keeps what the toggles do to a Player as their `Cheats` say, on this machine: the `Invincible`
/// marker, and the stat entries (free purchases, free ammo). Without `Cheats` they are all off.
/// Each machine does it for the Players it has the `Cheats` of: the Host for everyone (it judges
/// harm and Price), a client for its own Player (it resolves its ammo, and its prompts' prices).
fn sync_cheat_effects(
    mut commands: Commands,
    mut players: Query<(Entity, Option<&Cheats>, &mut Modifiers, Has<Invincible>), With<Player>>,
) {
    for (player, cheats, mut modifiers, invincible) in &mut players {
        let wanted = cheats.map(Cheats::entries).unwrap_or_default();
        let current: Vec<Modifier> = modifiers.0.iter().filter(|entry| entry.source == CHEAT_SOURCE).copied().collect();
        if current != wanted {
            modifiers.0.retain(|entry| entry.source != CHEAT_SOURCE);
            modifiers.0.extend(wanted);
        }
        let want_invincible = cheats.is_some_and(|cheats| cheats.invincible);
        if want_invincible && !invincible {
            commands.entity(player).insert(Invincible);
        } else if !want_invincible && invincible {
            commands.entity(player).remove::<Invincible>();
        }
    }
}

/// Switching Infinite ammo on fills this machine's Player's Weapons and Equipment, which are its
/// own to resolve, so no Magazine starts the toggle empty.
fn top_up_ammo(
    mut was_on: Local<bool>,
    mut local: Query<(Option<&Cheats>, &Modifiers, &mut Loadout, &mut Equipment), With<LocalPlayer>>,
) {
    let Ok((cheats, modifiers, mut loadout, mut equipment)) = local.single_mut() else { return };
    let on = cheats.is_some_and(|cheats| cheats.infinite_ammo);
    if on && !*was_on {
        for slot in 0..loadout.slot_count() {
            if loadout.slot(slot).is_some() {
                loadout.refill(slot, &modifiers.0);
            }
        }
        equipment.max_ammo();
    }
    *was_on = on;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::points::Points;
    use crate::storage::records::RunTainted;

    fn app(access: DevMode, run_type: RunType) -> App {
        let mut app = App::new();
        oneshots::add_messages(&mut app);
        app.add_plugins(MinimalPlugins)
            .add_message::<BonusPoints>()
            .add_message::<GivePowerUp>()
            .add_message::<GiveWeapon>()
            .add_message::<GivePerk>()
            .add_message::<GrantBoon>()
            .add_message::<GrantPact>()
            .add_systems(Update, pay.after(apply_cheats))
            .init_resource::<RunTainted>()
            .add_message::<TaintRun>()
            .add_message::<CheatRequest>()
            .add_message::<SetDevMode>()
            .insert_resource(run_type)
            .insert_resource(DevAccess(access))
            .add_systems(Update, (set_dev_mode, apply_cheats, sync_cheat_effects).chain())
            .add_systems(Update, |mut asked: MessageReader<TaintRun>, mut tainted: ResMut<RunTainted>| {
                if asked.read().count() > 0 {
                    tainted.0 = true;
                }
            });
        app
    }

    /// Stands in for Points' own payout of `BonusPoints`.
    fn pay(mut bonus: MessageReader<BonusPoints>, mut points: Query<&mut Points>) {
        for paid in bonus.read() {
            points.get_mut(paid.player).unwrap().balance += paid.amount;
        }
    }

    fn player(app: &mut App, seat: u8) -> Entity {
        app.world_mut().spawn((Player, Seat(seat), Points::default(), Modifiers::default())).id()
    }

    fn balance(app: &App, player: Entity) -> u64 {
        app.world().get::<Points>(player).unwrap().balance
    }

    #[test]
    fn the_settings_cycle_and_say_who_they_allow() {
        assert_eq!(DevMode::Off.next().next().next(), DevMode::Off);
        assert!(!DevMode::Off.allows(true));
        assert!(DevMode::On.allows(false));
        assert!(DevMode::HostOnly.allows(true));
        assert!(!DevMode::HostOnly.allows(false));
    }

    #[test]
    fn a_cheat_gives_points_taints_the_run_and_is_remembered() {
        let mut app = app(DevMode::On, RunType::Solo);
        let host = player(&mut app, 0);
        app.update();
        assert!(!app.world().resource::<RunTainted>().0, "no Cheat, no taint");
        app.world_mut().write_message(CheatRequest { player: host, cheat: Cheat::Points });
        app.update();
        app.update();
        assert_eq!(balance(&app, host), CHEAT_POINTS);
        assert!(app.world().resource::<RunTainted>().0);
        assert_eq!(app.world().get::<Cheats>(host).unwrap().points_given, CHEAT_POINTS);
    }

    #[test]
    fn a_forbidden_request_is_dropped_without_a_taint() {
        for (access, seat) in [(DevMode::Off, 0), (DevMode::Off, 1), (DevMode::HostOnly, 1)] {
            let mut app = app(access, RunType::Coop);
            let asker = player(&mut app, seat);
            app.world_mut().write_message(CheatRequest { player: asker, cheat: Cheat::Points });
            app.update();
            app.update();
            assert_eq!(balance(&app, asker), 0, "{access:?} seat {seat}");
            assert!(!app.world().resource::<RunTainted>().0, "{access:?} seat {seat}");
            assert!(app.world().get::<Cheats>(asker).is_none());
        }
    }

    #[test]
    fn host_only_lets_the_host_through_and_on_lets_a_client() {
        for (access, seat) in [(DevMode::HostOnly, 0), (DevMode::On, 1)] {
            let mut app = app(access, RunType::Coop);
            let asker = player(&mut app, seat);
            app.world_mut().write_message(CheatRequest { player: asker, cheat: Cheat::Points });
            app.update();
            app.update();
            assert_eq!(balance(&app, asker), CHEAT_POINTS, "{access:?} seat {seat}");
        }
    }

    #[test]
    fn off_clears_every_players_cheats_and_the_run_stays_tainted() {
        let mut app = app(DevMode::On, RunType::Coop);
        let (host, guest) = (player(&mut app, 0), player(&mut app, 1));
        for asker in [host, guest] {
            app.world_mut().write_message(CheatRequest { player: asker, cheat: Cheat::Points });
        }
        app.update();
        app.update();
        assert!(app.world().get::<Cheats>(guest).is_some());
        app.world_mut().write_message(SetDevMode(DevMode::Off));
        app.update();
        assert_eq!(app.world().resource::<DevAccess>().0, DevMode::Off);
        for asker in [host, guest] {
            assert!(app.world().get::<Cheats>(asker).is_none());
        }
        assert!(app.world().resource::<RunTainted>().0);
    }

    fn ask_for(app: &mut App, player: Entity, cheat: Cheat) {
        app.world_mut().write_message(CheatRequest { player, cheat });
        app.update();
        app.update();
    }

    fn entries_of(app: &App, player: Entity) -> Vec<Modifier> {
        app.world().get::<Modifiers>(player).unwrap().0.iter().filter(|entry| entry.source == CHEAT_SOURCE).copied().collect()
    }

    #[test]
    fn a_toggle_flips_with_each_request_and_the_stat_entries_and_marker_follow() {
        let mut app = app(DevMode::On, RunType::Solo);
        let host = player(&mut app, 0);
        ask_for(&mut app, host, Cheat::Invincible);
        ask_for(&mut app, host, Cheat::FreePurchases);
        ask_for(&mut app, host, Cheat::InfiniteAmmo);
        assert!(app.world().resource::<RunTainted>().0, "a toggle taints the Run too");
        assert!(app.world().get::<Invincible>(host).is_some());
        let entries = entries_of(&app, host);
        assert!(entries.iter().any(|entry| entry.stat == Stat::Price && entry.effect == Effect::Mult(0.0)));
        assert!(entries.iter().any(|entry| entry.stat == Stat::AmmoSpent && entry.effect == Effect::Mult(0.0)));
        assert_eq!(Cheats::toggles(app.world().get::<Cheats>(host)), 7);
        assert_eq!(balance(&app, host), 0, "a toggle gives no Points");

        ask_for(&mut app, host, Cheat::Invincible);
        ask_for(&mut app, host, Cheat::FreePurchases);
        assert!(app.world().get::<Invincible>(host).is_none());
        assert_eq!(entries_of(&app, host).len(), 1, "only the ammo entry is left");
        assert_eq!(Cheats::toggles(app.world().get::<Cheats>(host)), 4);
    }

    #[test]
    fn two_toggles_in_one_frame_are_both_kept() {
        let mut app = app(DevMode::On, RunType::Solo);
        let host = player(&mut app, 0);
        app.world_mut().write_message(CheatRequest { player: host, cheat: Cheat::Invincible });
        app.world_mut().write_message(CheatRequest { player: host, cheat: Cheat::InfiniteAmmo });
        app.update();
        app.update();
        assert_eq!(Cheats::toggles(app.world().get::<Cheats>(host)), 5);
    }

    #[test]
    fn the_stat_entries_ride_on_a_stat_other_layers_do_not_rewrite_and_survive_theirs() {
        use crate::modifier::resolve;
        let mut app = app(DevMode::On, RunType::Solo);
        let host = player(&mut app, 0);
        ask_for(&mut app, host, Cheat::FreePurchases);
        // A layer's own entries come and go; Gold Rush and Inflation can't make a price above 0.
        let pact = Modifier { stat: Stat::Price, source: Source("Gold Rush"), effect: Effect::Mult(1.25) };
        app.world_mut().get_mut::<Modifiers>(host).unwrap().0.push(pact);
        app.update();
        let entries = &app.world().get::<Modifiers>(host).unwrap().0;
        assert_eq!(resolve(Stat::Price, 1000.0, entries), 0.0);
        assert_eq!(crate::economy::buy::charged(Some(1000), entries), Some(0));
    }

    #[test]
    fn off_takes_the_toggles_and_their_effects_away() {
        let mut app = app(DevMode::On, RunType::Coop);
        let guest = player(&mut app, 1);
        ask_for(&mut app, guest, Cheat::Invincible);
        ask_for(&mut app, guest, Cheat::FreePurchases);
        app.world_mut().write_message(SetDevMode(DevMode::Off));
        app.update();
        app.update();
        assert!(app.world().get::<Cheats>(guest).is_none());
        assert!(app.world().get::<Invincible>(guest).is_none());
        assert!(entries_of(&app, guest).is_empty());
    }

    #[test]
    fn toggles_survive_the_wire_form() {
        let held = Cheats { invincible: true, infinite_ammo: true, ..default() };
        let told = Cheats::from_toggles(Cheats::toggles(Some(&held)));
        assert_eq!(told, Some(held));
        assert_eq!(Cheats::from_toggles(0), None);
        assert_eq!(Cheats::toggles(None), 0);
    }

    #[test]
    fn every_grant_has_a_name_and_the_gauntlet_ones_wait_for_the_gauntlet() {
        for kind in GrantKind::ALL {
            let cheats = kind.cheats();
            assert!(!cheats.is_empty(), "{kind:?}");
            for cheat in cheats {
                assert!(!cheat.label().is_empty() && cheat.label() != "?", "{kind:?} {cheat:?}");
                let gauntlet_only = matches!(kind, GrantKind::Boons | GrantKind::Pacts) || cheat == Cheat::UnlockAll;
                assert!(cheat.available(Mode::Gauntlet));
                assert_eq!(cheat.available(Mode::Classic), !gauntlet_only, "{cheat:?}");
            }
        }
        assert!(!Cheat::Boon(u16::MAX).available(Mode::Gauntlet), "no such Boon");
        assert!(!Cheat::Pact(u8::MAX).available(Mode::Gauntlet), "no such Pact");
    }

    #[test]
    fn a_weapon_in_last_stand_is_dropped_but_a_power_up_is_not() {
        let mut app = app(DevMode::On, RunType::Solo);
        let host = player(&mut app, 0);
        app.world_mut().entity_mut(host).insert(LastStand::begin(&mut crate::weapons::weapon::Loadout::new(WeaponId::M1911)));
        app.world_mut().write_message(CheatRequest { player: host, cheat: Cheat::Weapon(WeaponId::RayGun, false) });
        app.update();
        app.update();
        assert!(!app.world().resource::<RunTainted>().0);
        app.world_mut().write_message(CheatRequest { player: host, cheat: Cheat::PowerUp(PowerUp::MaxAmmo) });
        app.update();
        app.update();
        assert!(app.world().resource::<RunTainted>().0);
    }

    #[test]
    fn solo_has_no_setting_to_change() {
        let mut app = app(DevMode::On, RunType::Solo);
        app.world_mut().write_message(SetDevMode(DevMode::Off));
        app.update();
        assert_eq!(app.world().resource::<DevAccess>().0, DevMode::On);
    }
}
