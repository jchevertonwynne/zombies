//! Developer mode: a layer on Classic for testing, silent until a Cheat is used (ADR 0010).
//!
//! It reaches Classic only through neutral messages (`BonusPoints` for the one Cheat so far, and
//! `TaintRun`), and never writes a component a layer rewrites. Solo it is always allowed. In Co-op
//! the Host sets it in the Room to Off, On (every Player) or Host only, and can change it during
//! the Run. Every Cheat is a request the Host judges: one the setting forbids is dropped without a
//! word, and one that goes through taints the Run (`TaintRun`), which the Host tells to everyone.
//!
//! The Cheat Panel that sends the requests is in `ui::cheat_panel`; the wire is in `net::world`.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::authority::HostSimulation;
use crate::economy::points::BonusPoints;
use crate::players::player::{Player, Seat};
use crate::players::respawn::Disconnected;
use crate::run::party::Party;
use crate::state::{GameState, RunType};
use crate::storage::records::TaintRun;

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
}

impl Cheat {
    pub const ALL: [Cheat; 1] = [Cheat::Points];

    pub fn label(self) -> &'static str {
        match self {
            Cheat::Points => "+10,000 Points",
        }
    }
}

/// The Run's Developer mode setting, as this machine has it. Solo it is `On`; in Co-op it is the
/// Room's, and the Host's word changes it.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct DevAccess(pub DevMode);

/// What a Player has switched on or done with the Cheats, kept on their Player (so by Seat, and
/// through a Rejoin) and gone when the Host sets Developer mode to Off. A Player who has used none
/// has no `Cheats`. The Host's.
#[derive(Component, Default, Debug, Clone, PartialEq, Eq)]
pub struct Cheats {
    /// How many Points the Points Cheat has given them.
    pub points_given: u64,
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
        app.init_resource::<DevAccess>()
            .add_message::<CheatRequest>()
            .add_message::<SetDevMode>()
            .add_systems(OnEnter(GameState::Playing), start_access.after(crate::players::player::SpawnPlayers))
            .add_systems(
                Update,
                (set_dev_mode, apply_cheats)
                    .chain()
                    .in_set(HostSimulation)
                    .run_if(in_state(GameState::Playing)),
            );
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
/// otherwise it is dropped silently.
fn apply_cheats(
    mut commands: Commands,
    mut requests: MessageReader<CheatRequest>,
    access: Res<DevAccess>,
    players: Query<(&Seat, Has<Disconnected>), With<Player>>,
    mut cheats: Query<&mut Cheats>,
    mut bonus: MessageWriter<BonusPoints>,
    mut taint: MessageWriter<TaintRun>,
) {
    for &CheatRequest { player, cheat } in requests.read() {
        let Ok((seat, away)) = players.get(player) else { continue };
        if away || !access.0.allows(seat.0 == 0) {
            continue;
        }
        taint.write(TaintRun);
        match cheat {
            Cheat::Points => {
                bonus.write(BonusPoints { player, amount: CHEAT_POINTS });
                match cheats.get_mut(player) {
                    Ok(mut held) => held.points_given += CHEAT_POINTS,
                    Err(_) => {
                        commands.entity(player).insert(Cheats { points_given: CHEAT_POINTS });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::points::Points;
    use crate::storage::records::RunTainted;

    fn app(access: DevMode, run_type: RunType) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<BonusPoints>()
            .add_systems(Update, pay.after(apply_cheats))
            .init_resource::<RunTainted>()
            .add_message::<TaintRun>()
            .add_message::<CheatRequest>()
            .add_message::<SetDevMode>()
            .insert_resource(run_type)
            .insert_resource(DevAccess(access))
            .add_systems(Update, (set_dev_mode, apply_cheats).chain())
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
        app.world_mut().spawn((Player, Seat(seat), Points::default())).id()
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

    #[test]
    fn solo_has_no_setting_to_change() {
        let mut app = app(DevMode::On, RunType::Solo);
        app.world_mut().write_message(SetDevMode(DevMode::Off));
        app.update();
        assert_eq!(app.world().resource::<DevAccess>().0, DevMode::On);
    }
}
