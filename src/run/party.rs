//! A Co-op Run's party: the Players the Room started it with, and how the Room starts the Run and
//! takes the party back.
//!
//! The Host's Start hands the Room's `Link` to the Run (`play`), keeping the Room itself in the
//! `Party` for afterwards. A joiner hears Start, and does the same with its end. Each machine then
//! enters the Run with a Player in every Seat, its own the `LocalPlayer`. After Game Over, "Back to
//! the room" gives the `Link` back to a Room (the Host's re-opened as it was, a joiner's waiting for
//! it), and "Leave" lets the other end know and goes to the Co-op screen.

use bevy::prelude::*;

use crate::authority::Authority;
use crate::ui::coop::{CoopSession, Notice, armory_of};
use crate::run::gauntlet::progress::GauntletFile;
use crate::run::leave_run::PlayerLeft;
use crate::maps::map::MapId;
use crate::run::dev::DevMode;
use crate::run::mode::Mode;
use crate::net::link::Link;
use crate::net::away::{AwayPlugin, Rejoin};
use crate::net::play::{self, Held, HostSaid, PlayPlugin, Roster, Scoreboard, Wire};
use crate::net::protocol::{HOST, Peer, RoomView, Score, ToHost, ToPlayer, encode};
use crate::net::room::{Hosting, Joining, Room};
use crate::run::gauntlet::PlayerUnlocks;
use crate::run::gauntlet::arc::RunArc;
use crate::run::gauntlet::win::WinChoice;
use crate::storage::records::RunWon;
use crate::players::player::{Player, PlayerName, Seat};
use crate::economy::points::Points;
use crate::run::stats::RunStats;
use crate::state::{GameState, MainMenuScreen};

/// How far a Run has got in starting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// The Authority is yet to be set.
    Gathering,
    /// The Authority is set, and the Run is yet to begin.
    Starting,
    Running,
}

/// The Players a Co-op Run started with, from the Room's start until the party is back in it or
/// has left. Solo has none.
#[derive(Resource)]
pub struct Party {
    seats: Vec<(Seat, String)>,
    /// The Seat of this machine's Player.
    pub local: Seat,
    mode: Mode,
    map: MapId,
    /// The Host's Degrees, which every machine's Run takes in place of its own save's.
    heat: [u8; 8],
    /// Who may use Cheats: the Room's setting when the Run began, and as the Host has set it since.
    dev: DevMode,
    stage: Stage,
    /// The Host's Room, kept for the party's return.
    room: Option<Room>,
    /// This machine's Player has asked to go back to the Room.
    back: bool,
    /// Game Over has been reached.
    over: bool,
    /// The other end has already been told, or has said it has gone.
    parted: bool,
    /// This joiner ended the Run at the Win and gave up their Seat while the rest play on, so
    /// there is no Room to go back to.
    ended_alone: bool,
}

impl Party {
    /// Who is in each Seat, and their Name.
    pub fn seats(&self) -> &[(Seat, String)] {
        &self.seats
    }

    /// The Host's Room, if this machine is the Host.
    pub(crate) fn room(&self) -> Option<&Room> {
        self.room.as_ref()
    }

    /// The Host's Degrees, one per Condition.
    pub fn heat(&self) -> [u8; 8] {
        self.heat
    }

    /// Who may use Cheats.
    pub fn dev(&self) -> DevMode {
        self.dev
    }

    /// The Host sets who may use Cheats during the Run: the Room it comes back to keeps it.
    pub fn set_dev(&mut self, dev: DevMode) {
        self.dev = dev;
        if let Some(room) = self.room.as_mut() {
            room.set_dev(dev);
        }
    }

    /// Whether there is a Room for this machine's Player to go back to: not for a joiner who ended
    /// the Run at the Win on their own.
    pub fn can_go_back(&self) -> bool {
        !self.ended_alone
    }

    /// This machine's Player wants back in the Room.
    pub fn back_to_room(&mut self) {
        self.back = true;
    }

    fn new(seats: Vec<(Seat, String)>, local: Seat, mode: Mode, map: MapId, heat: [u8; 8], dev: DevMode, room: Option<Room>) -> Self {
        Party { seats, local, mode, map, heat, dev, stage: Stage::Gathering, room, back: false, over: false, parted: false, ended_alone: false }
    }

}

/// The Peers a Host has Players on now, or nothing for a joiner. A Player whose way has broken and
/// who is dialling back is found by the Run's roster, not the Room's.
fn live_peers(world: &World, party: &Party) -> Option<Vec<Peer>> {
    party.room.as_ref()?;
    Some(world.get_resource::<Roster>().map_or_else(Vec::new, |roster| roster.0.iter().map(|&(peer, _)| peer).collect()))
}

/// Starts the Run from the Host's Room, if it may start: every machine is told. The Room is given
/// back if it may not.
pub fn start_hosted(
    world: &mut World,
    hosting: Hosting<Box<dyn Link>>,
) -> Result<(), Box<Hosting<Box<dyn Link>>>> {
    let (link, room, roster) = hosting.start().map_err(Box::new)?;
    let seats = room
        .seated()
        .into_iter()
        .map(|(seat, name)| (Seat(seat), name))
        .collect();
    let room_tokens = room.tokens();
    world.insert_resource(Party::new(seats, Seat(0), room.mode(), room.map(), room.degrees(), room.dev(), Some(room)));
    play::begin_host(world, link, roster);
    world.insert_resource(Held { tokens: room_tokens, away: Vec::new() });
    Ok(())
}

/// Starts the Run a Host has said to start: the Player's own Seat and the Room as it stood.
pub fn start_joined(world: &mut World, joining: Joining<Box<dyn Link>>, seat: u8, view: RoomView) {
    let seats = view
        .seats
        .iter()
        .enumerate()
        .filter_map(|(at, held)| Some((Seat(at as u8), held.as_ref()?.name.clone())))
        .collect();
    world.insert_resource(Party::new(seats, Seat(seat), view.mode, view.map, view.heat, view.dev, None));
    let (token, hello) = joining.rejoin();
    play::begin_client(world, joining.into_link(), Rejoin::new(token, hello));
}

fn launching(party: Option<Res<Party>>) -> bool {
    party.is_some_and(|party| party.stage != Stage::Running)
}

/// Starts the Run over two frames: the Authority first, so that what is built as the Run begins
/// is built for it.
fn launch(
    mut party: ResMut<Party>,
    authority: Res<State<Authority>>,
    mut next_authority: ResMut<NextState<Authority>>,
    mut next: ResMut<NextState<GameState>>,
    (mut mode, mut map): (ResMut<Mode>, ResMut<MapId>),
) {
    match party.stage {
        Stage::Gathering => {
            let wanted = if party.room.is_some() { Authority::Host } else { Authority::Client };
            if *authority.get() != wanted {
                next_authority.set(wanted);
            }
            party.stage = Stage::Starting;
        }
        Stage::Starting => {
            *mode = party.mode;
            *map = party.map;
            next.set(GameState::Playing);
            party.stage = Stage::Running;
        }
        Stage::Running => {}
    }
}

/// Game Over: the Host tells every Player how each did and that the Run is over, and whether it
/// reached its Win (so the Host's End the Run is a Win for everyone), and keeps the scoreboard
/// itself.
fn run_over(
    party: Option<ResMut<Party>>,
    roster: Option<Res<Roster>>,
    wire: Option<ResMut<Wire>>,
    players: Query<(&Seat, &PlayerName, &RunStats, &Points), With<Player>>,
    mut scoreboard: ResMut<Scoreboard>,
    won: Option<Res<RunWon>>,
) {
    let Some(mut party) = party else { return };
    party.over = true;
    if let (Some(roster), Some(mut wire)) = (roster, wire) {
        let mut rows: Vec<Score> = players
            .iter()
            .map(|(seat, name, stats, points)| Score {
                name: name.0.clone(),
                seat: seat.0,
                kills: stats.kills,
                downs: stats.downs,
                revives: stats.revives,
                points_earned: points.earned,
            })
            .collect();
        rows.sort_by_key(|row| row.seat);
        for &(peer, _) in &roster.0 {
            wire.0.send(peer, encode(&ToPlayer::Scoreboard(rows.clone())));
            wire.0.send(peer, encode(&ToPlayer::RunOver { won: won.as_ref().is_some_and(|won| won.0) }));
        }
        scoreboard.0 = rows;
    }
}

/// A remote Player's Offers are drawn from the Unlocks they brought to the Room, not the Host's.
fn remote_unlocks(
    party: Option<Res<Party>>,
    mut players: Query<(&Seat, &mut PlayerUnlocks), Added<PlayerUnlocks>>,
) {
    let Some(room) = party.as_ref().and_then(|party| party.room.as_ref()) else { return };
    for (seat, mut unlocks) in &mut players {
        if seat.0 != 0
            && let Some(brought) = room.unlocks(seat.0)
        {
            unlocks.0 = brought.unlocked.clone();
        }
    }
}

/// What the Host said, as a client hears it. A Run over that reached its Win is this Player's Win
/// too, saved with their Record.
fn hear_host(
    mut said: MessageReader<HostSaid>,
    party: Option<ResMut<Party>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut notice: ResMut<Notice>,
    (mut run_won, mut arc): (Option<ResMut<RunWon>>, Option<ResMut<RunArc>>),
) {
    let Some(mut party) = party else {
        said.clear();
        return;
    };
    for said in said.read() {
        match said {
            &HostSaid::RunOver { won } => {
                if won {
                    if let Some(run_won) = run_won.as_mut() {
                        run_won.0 = true;
                    }
                    if let Some(arc) = arc.as_mut() {
                        arc.won = true;
                    }
                }
                if *state.get() == GameState::Playing {
                    next.set(GameState::GameOver);
                }
            }
            HostSaid::Closed => {
                party.parted = true;
                notice.0 = if party.over { "Host closed the room" } else { "Host ended the Run" }.into();
                next.set(GameState::MainMenu);
            }
            HostSaid::BackToRoom => party.back = true,
            HostSaid::Refused(why) => {
                // The Host holds no Seat for this Player: the Run is over for them.
                notice.0 = why.reason().into();
                next.set(GameState::MainMenu);
            }
        }
    }
}

/// A joiner who ends the Run at the Win has no Room to go back to: the rest play on without them.
fn end_alone(mut chosen: MessageReader<WinChoice>, party: Option<ResMut<Party>>) {
    let Some(mut party) = party else {
        chosen.clear();
        return;
    };
    if party.room.is_none() && chosen.read().any(|choice| choice.end) {
        party.ended_alone = true;
    }
}

/// A joiner who left the Run gives up their Seat in the Room the Host keeps.
fn free_seat(mut left: MessageReader<PlayerLeft>, party: Option<ResMut<Party>>) {
    let Some(mut party) = party else {
        left.clear();
        return;
    };
    for &PlayerLeft { seat } in left.read() {
        if let Some(room) = party.room.as_mut()
            && let Some(peer) = room.kickable(seat.0)
        {
            room.remove(peer);
        }
    }
}

fn wants_back(party: Option<Res<Party>>) -> bool {
    party.is_some_and(|party| party.back)
}

/// The party goes back to the Room it left: the Host's as it was, a joiner's in its Seat, waiting
/// for the Host to say how it stands.
fn back_to_room(world: &mut World) {
    let Some(party) = world.remove_resource::<Party>() else { return };
    // What each Player comes back by now, and who is still with the Host.
    let held = world.remove_resource::<Held>();
    let live = world.get_resource::<Roster>().map(|roster| roster.0.clone()).unwrap_or_default();
    let token = world.get_resource::<Rejoin>().map_or(0, |rejoin| rejoin.token);
    let Some(link) = play::end_run(world) else { return };
    let session = match party.room {
        Some(mut room) => {
            // Those who came back by a new link are the Room's by it; those still away have lost
            // their Seat in it.
            for &(seat, token) in held.iter().flat_map(|held| held.tokens.iter()) {
                if let Some(&(peer, _)) = live.iter().find(|&&(_, on)| on == seat) {
                    room.rebind(seat, peer, token);
                }
            }
            for seat in held.iter().flat_map(|held| held.away.iter()) {
                if let Some(stale) = room.kickable(*seat) {
                    room.remove(stale);
                }
            }
            CoopSession::Host(Box::new(Hosting::resume(link, room)))
        }
        None => {
            let unlocks = armory_of(world.resource::<GauntletFile>());
            CoopSession::Join(Box::new(Joining::resume(link, party.local.0, token, unlocks)))
        }
    };
    world.insert_resource(session);
    world.resource_mut::<NextState<MainMenuScreen>>().set(MainMenuScreen::Room);
    world.resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
    set_authority_host(world);
}

fn set_authority_host(world: &mut World) {
    if *world.resource::<State<Authority>>().get() != Authority::Host {
        world.resource_mut::<NextState<Authority>>().set(Authority::Host);
    }
}

/// Tells the other end this machine's Player has gone: a Host's Players that the Room is closed, a
/// joiner's Host that they have left for good.
fn goodbye(link: &mut dyn Link, remotes: Option<Vec<Peer>>) {
    match remotes {
        Some(peers) => {
            for peer in peers {
                link.send(peer, encode(&ToPlayer::Closed));
                link.close(peer);
            }
        }
        None => {
            link.send(HOST, encode(&ToHost::LeftRun));
            link.close(HOST);
        }
    }
}

/// The Main Menu is reached from a Run without the party going back to its Room: the Run is left,
/// and the Co-op screen is where the Player goes.
fn leave_party(world: &mut World) {
    let Some(party) = world.remove_resource::<Party>() else { return };
    let peers = live_peers(world, &party);
    if let Some(mut link) = play::end_run(world)
        && !party.parted
    {
        goodbye(&mut link, peers);
    }
    world.resource_mut::<NextState<MainMenuScreen>>().set(MainMenuScreen::Coop);
    set_authority_host(world);
}

/// Closing the window mid-Run is leaving it.
fn leave_on_exit(world: &mut World) {
    let Some(mut party) = world.remove_resource::<Party>() else { return };
    if !party.parted {
        let peers = live_peers(world, &party);
        if let Some(mut wire) = world.get_resource_mut::<Wire>() {
            goodbye(&mut wire.0, peers);
        }
        party.parted = true;
    }
    world.insert_resource(party);
}

pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((PlayPlugin, AwayPlugin))
            .add_systems(OnEnter(GameState::GameOver), run_over)
            .add_systems(OnEnter(GameState::MainMenu), leave_party)
            .add_systems(
                Update,
                (
                    launch.run_if(launching),
                    (hear_host, free_seat, remote_unlocks, end_alone),
                    back_to_room.run_if(wants_back),
                )
                    .chain(),
            )
            .add_systems(Last, leave_on_exit.run_if(on_message::<AppExit>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use avian3d::prelude::*;
    use bevy::state::app::StatesPlugin;
    use bevy::time::TimeUpdateStrategy;

    use crate::authority::AuthorityPlugin;
    use crate::ui::coop::{close_unused_session, pump_session};
    use crate::ui::menu::BackTo;
    use crate::modifier::RunModifiers;
    use crate::net::link::{Hub, Memory};
    use crate::net::protocol::Unlocks;
    use crate::net::room::Joined;
    use crate::players::player::{LocalPlayer, Player, SpawnPlayers, spawn_player};
    use crate::storage::settings::Settings;
    use crate::state::{RunType, StatePlugin};
    use crate::tuning::*;

    /// One machine in the Main Menu's Room, holding the session it was given.
    fn machine(session: CoopSession) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            PhysicsPlugins::default(),
            StatesPlugin,
            bevy::input::InputPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .add_plugins((StatePlugin, AuthorityPlugin, crate::run::leave_run::LeaveRunPlugin, PartyPlugin))
        .add_message::<crate::players::player::PlayerHit>()
        .add_message::<crate::enemies::special::SpecialCue>()
        .add_message::<crate::enemies::nova::NovaCrawlerKilled>()
        .init_resource::<crate::economy::power_up::ActivePowerUps>()
        .insert_resource(crate::enemies::nav::NavGraph::new(crate::maps::map::MapId::Greybox))
        .insert_resource(crate::maps::barrier::Barriers::default())
        .add_plugins((crate::enemies::zombie::ZombiePlugin, crate::enemies::hellhound::HellhoundPlugin))
        .add_systems(Startup, (crate::enemies::special::load_assets, crate::enemies::sapper::load_assets, crate::enemies::nova::load_assets))
        .insert_resource(session)
        .init_resource::<Notice>()
        .init_resource::<crate::ui::coop::JoinTarget>()
        .init_resource::<BackTo>()
        .init_resource::<GauntletFile>()
        .init_resource::<RunModifiers>()
        .init_resource::<crate::run::gauntlet::ActiveRanks>()
        .init_resource::<crate::run::gauntlet::ChosenKit>()
        .init_resource::<crate::run::gauntlet::heat::RunHeat>()
        .init_resource::<crate::run::gauntlet::elite::EliteRules>()
        .init_resource::<crate::storage::records::RunHeatRecord>()
        .init_resource::<crate::weapons::ammo_mod::TurnedAllyCap>()
        .init_resource::<crate::run::round::ReservedRounds>()
        .add_observer(crate::run::gauntlet::give_gauntlet_state)
        .add_systems(OnExit(GameState::MainMenu), crate::run::gauntlet::set_run_modifiers)
        .init_resource::<crate::weapons::weapon::StartingLoadout>()
        .init_resource::<Mode>()
        .init_resource::<MapId>()
        .insert_resource(Settings::default())
        .add_systems(OnEnter(GameState::Playing), spawn_player.in_set(SpawnPlayers))
        .add_systems(Update, (pump_session, close_unused_session).chain())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / f64::from(FIXED_HZ),
        )));
        app.finish();
        app.cleanup();
        app.world_mut().resource_mut::<NextState<MainMenuScreen>>().set(MainMenuScreen::Room);
        app.update();
        app
    }

    /// Lets the Room's network run until everything said has been heard.
    fn settle(hub: &Hub, hosting: &mut Hosting<Box<dyn Link>>, joining: &mut [Joining<Box<dyn Link>>]) {
        for _ in 0..12 {
            hub.step();
            hosting.poll();
            joining.iter_mut().for_each(Joining::poll);
        }
    }

    /// A Host's machine and `joiners` others, in one Room that everyone is Ready in.
    fn room(joiners: usize) -> (Hub, Vec<App>) {
        room_with(joiners, Mode::Classic, [0; 8])
    }

    /// `room`, in `mode` with the Host's `heat` set.
    fn room_with(joiners: usize, mode: Mode, heat: [u8; 8]) -> (Hub, Vec<App>) {
        room_bringing(vec![Unlocks::default(); joiners], mode, heat)
    }

    /// `room_with`, one joiner for each Armory in `armories`.
    fn room_bringing(armories: Vec<Unlocks>, mode: Mode, heat: [u8; 8]) -> (Hub, Vec<App>) {
        let hub = Hub::new(1, 0.0);
        let mut room = Room::new("Hostess", Unlocks::default(), mode);
        room.set_heat(heat);
        let mut hosting: Hosting<Box<dyn Link>> = Hosting::new(Box::new(hub.host()), room);
        let mut joining: Vec<Joining<Box<dyn Link>>> = armories
            .into_iter()
            .enumerate()
            .map(|(n, armory)| Joining::new(Box::new(hub.join()) as Box<dyn Link>, &format!("P{n}"), "", armory))
            .collect();
        settle(&hub, &mut hosting, &mut joining);
        joining.iter_mut().for_each(|joining| joining.ready(true));
        settle(&hub, &mut hosting, &mut joining);
        let mut apps = vec![machine(CoopSession::Host(Box::new(hosting)))];
        apps.extend(joining.into_iter().map(|joining| machine(CoopSession::Join(Box::new(joining)))));
        (hub, apps)
    }

    fn frames(hub: &Hub, apps: &mut [App], frames: usize) {
        for _ in 0..frames {
            hub.step();
            apps.iter_mut().for_each(App::update);
        }
    }

    /// The Host presses Start, if it may.
    fn start(host: &mut App) -> bool {
        let world = host.world_mut();
        let session = std::mem::take(&mut *world.resource_mut::<CoopSession>());
        let CoopSession::Host(hosting) = session else { panic!("not a Host") };
        match start_hosted(world, *hosting) {
            Ok(()) => true,
            Err(hosting) => {
                *world.resource_mut::<CoopSession>() = CoopSession::Host(hosting);
                false
            }
        }
    }

    /// The Seats of the Players on a machine, and which of them is its own.
    fn seats_and_local(app: &mut App) -> (Vec<u8>, u8) {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&crate::players::player::Seat, Has<LocalPlayer>), With<Player>>();
        let mut seats: Vec<u8> = query.iter(world).map(|(seat, _)| seat.0).collect();
        seats.sort_unstable();
        let local: Vec<u8> = query.iter(world).filter(|(_, local)| *local).map(|(seat, _)| seat.0).collect();
        assert_eq!(local.len(), 1, "one LocalPlayer");
        (seats, local[0])
    }

    fn state(app: &App) -> GameState {
        *app.world().resource::<State<GameState>>().get()
    }

    fn authority(app: &App) -> Authority {
        *app.world().resource::<State<Authority>>().get()
    }

    fn screen(app: &App) -> MainMenuScreen {
        *app.world().resource::<State<MainMenuScreen>>().get()
    }

    /// Everyone is in a Run, each in their own Seat, as the Authority they should be.
    fn assert_in_run(apps: &mut [App]) {
        let seats: Vec<u8> = (0..apps.len() as u8).collect();
        for (at, app) in apps.iter_mut().enumerate() {
            assert_eq!(state(app), GameState::Playing, "machine {at}");
            let wanted = if at == 0 { Authority::Host } else { Authority::Client };
            assert_eq!(authority(app), wanted, "machine {at}");
            assert_eq!(*app.world().resource::<RunType>(), RunType::Coop, "machine {at}");
            assert_eq!(seats_and_local(app), (seats.clone(), at as u8), "machine {at}");
        }
    }

    #[test]
    fn a_lone_host_cannot_start() {
        let (_hub, mut apps) = room(0);
        assert!(!start(&mut apps[0]));
        assert!(matches!(apps[0].world().resource::<CoopSession>(), CoopSession::Host(_)));
        assert!(apps[0].world().get_resource::<Party>().is_none());
    }

    #[test]
    fn a_joiner_who_is_not_ready_holds_the_start_back() {
        let hub = Hub::new(1, 0.0);
        let room = Room::new("Hostess", Unlocks::default(), Mode::Classic);
        let mut hosting: Hosting<Box<dyn Link>> = Hosting::new(Box::new(hub.host()), room);
        let mut joining = vec![Joining::new(Box::new(hub.join()) as Box<dyn Link>, "Ada", "", Unlocks::default())];
        settle(&hub, &mut hosting, &mut joining);
        assert_eq!(hosting.room.players(), 2);
        assert!(!hosting.room.can_start());
        joining[0].ready(true);
        settle(&hub, &mut hosting, &mut joining);
        assert!(hosting.room.can_start());
    }

    #[test]
    fn start_puts_every_machine_in_the_run_in_its_seat_and_back_to_the_room_returns_them() {
        for joiners in [1, 3] {
            let (hub, mut apps) = room(joiners);
            assert!(start(&mut apps[0]));
            frames(&hub, &mut apps, 12);
            assert_in_run(&mut apps);

            // Every Player is down: the Host's rule ends the Run, and each machine reaches Game Over
            // with the scoreboard.
            let world = apps[0].world_mut();
            let mut players = world.query_filtered::<Entity, With<Player>>();
            let all: Vec<Entity> = players.iter(world).collect();
            for player in all {
                world.entity_mut(player).insert(crate::players::last_stand::LastStand::shown(10.0, true));
            }
            bevy::ecs::system::RunSystemOnce::run_system_once(world, crate::players::respawn::game_over_when_none_up).unwrap();
            frames(&hub, &mut apps, 12);
            for app in &apps {
                assert_eq!(state(app), GameState::GameOver);
                let board = &app.world().resource::<play::Scoreboard>().0;
                let seats: Vec<u8> = board.iter().map(|row| row.seat).collect();
                assert_eq!(seats, (0..=joiners as u8).collect::<Vec<_>>());
            }

            // The Host goes back to the Room, and the rest are brought with it.
            apps[0].world_mut().resource_mut::<Party>().back_to_room();
            frames(&hub, &mut apps, 20);
            for (at, app) in apps.iter_mut().enumerate() {
                assert_eq!(state(app), GameState::MainMenu, "machine {at}");
                assert_eq!(authority(app), Authority::Host, "machine {at}");
                assert_eq!(screen(app), MainMenuScreen::Room, "machine {at}");
                assert!(app.world().get_resource::<Party>().is_none());
                assert!(app.world().get_resource::<Wire>().is_none());
                match app.world().resource::<CoopSession>() {
                    CoopSession::Host(hosting) => {
                        assert_eq!(at, 0);
                        assert!(!hosting.room.is_started(), "the Room takes Players again");
                        assert_eq!(hosting.room.players(), joiners + 1);
                    }
                    CoopSession::Join(joining) => match &joining.state {
                        Joined::In { seat, view: Some(view) } => {
                            assert_eq!(usize::from(*seat), at, "the same Seat");
                            assert!(!view.seats[at].as_ref().unwrap().ready, "no one is Ready");
                        }
                        other => panic!("machine {at} is {other:?}"),
                    },
                    CoopSession::Off => panic!("machine {at} has no Room"),
                }
            }

            // Ready again, and the same Room starts another Run.
            for app in &mut apps[1..] {
                let CoopSession::Join(joining) = &mut *app.world_mut().resource_mut::<CoopSession>() else {
                    panic!("not a joiner")
                };
                joining.ready(true);
            }
            frames(&hub, &mut apps, 6);
            assert!(start(&mut apps[0]));
            frames(&hub, &mut apps, 12);
            assert_in_run(&mut apps);
        }
    }

    #[test]
    fn a_joiner_who_leaves_loses_their_player_and_the_others_carry_on() {
        let (hub, mut apps) = room(2);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        // Leaving the Run, as the Pause menu's confirm does: the Main Menu is reached.
        apps[2].world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
        frames(&hub, &mut apps, 12);
        assert_eq!(state(&apps[2]), GameState::MainMenu);
        assert_eq!(screen(&apps[2]), MainMenuScreen::Coop);
        assert!(matches!(apps[2].world().resource::<CoopSession>(), CoopSession::Off));
        assert_eq!(seats_and_local(&mut apps[0]).0, [0, 1]);
        assert_eq!(state(&apps[0]), GameState::Playing);
        assert_eq!(state(&apps[1]), GameState::Playing);
    }

    #[test]
    fn the_host_ending_the_run_sends_every_player_to_the_co_op_screen() {
        let (hub, mut apps) = room(1);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        apps[0].world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
        frames(&hub, &mut apps, 12);
        for app in &apps {
            assert_eq!(state(app), GameState::MainMenu);
            assert_eq!(screen(app), MainMenuScreen::Coop);
        }
        assert_eq!(apps[1].world().resource::<Notice>().0, "Host ended the Run");
    }

    #[test]
    fn a_player_who_comes_after_the_start_is_refused() {
        let (hub, mut apps) = room(1);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 6);
        let mut late: Joining<Memory> = Joining::new(hub.join(), "Late", "", Unlocks::default());
        for _ in 0..12 {
            hub.step();
            apps[0].update();
            late.poll();
        }
        assert_eq!(late.state, Joined::Refused(crate::net::protocol::Refusal::Started));
    }

    #[test]
    fn a_client_plays_the_hosts_heat_and_ignores_its_own_save() {
        use crate::run::gauntlet::heat::{Condition, RunHeat};
        use crate::run::gauntlet::reroll_price_from;
        use crate::storage::records::RunHeatRecord;

        let mut hosts = [0u8; 8];
        hosts[Condition::TightOffers as usize] = 2;
        hosts[Condition::Inflation as usize] = 1;
        let (hub, mut apps) = room_with(2, Mode::Gauntlet, hosts);
        // A client's own save has another Heat set; in a Room it must not count.
        let mut own = crate::run::gauntlet::progress::GauntletFile { has_won: true, ..Default::default() };
        own.cycle_heat(Condition::EliteSurge).unwrap();
        apps[1].world_mut().insert_resource(own);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        assert_in_run(&mut apps);

        let host = *apps[0].world().resource::<RunHeat>();
        assert_eq!(host.degree(Condition::TightOffers), 2);
        for (at, app) in apps.iter().enumerate() {
            let heat = *app.world().resource::<RunHeat>();
            assert_eq!(heat, host, "machine {at} plays the Host's Degrees");
            assert_eq!(heat.degree(Condition::EliteSurge), 0, "machine {at} ignores its own save");
            assert_eq!(
                reroll_price_from(heat.reroll_base_price(), 1),
                reroll_price_from(host.reroll_base_price(), 1),
                "machine {at}'s Reroll price"
            );
            let record = app.world().resource::<RunHeatRecord>();
            let host_record = apps[0].world().resource::<RunHeatRecord>();
            assert_eq!(record.heat, host_record.heat);
            assert_eq!(record.heat, Some(3));
            assert_eq!(record.conditions.len(), host_record.conditions.len());
        }
    }

    #[test]
    fn a_client_without_a_win_plays_the_hosts_heat() {
        use crate::run::gauntlet::heat::{Condition, RunHeat};
        use crate::storage::records::RunHeatRecord;

        let mut hosts = [0u8; 8];
        hosts[Condition::TightOffers as usize] = 2;
        hosts[Condition::Inflation as usize] = 1;
        let (hub, mut apps) = room_with(2, Mode::Gauntlet, hosts);
        let own = crate::run::gauntlet::progress::GauntletFile::default();
        assert!(!own.has_won);
        apps[1].world_mut().insert_resource(own);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        assert_in_run(&mut apps);

        let host = *apps[0].world().resource::<RunHeat>();
        assert_eq!(host.degree(Condition::TightOffers), 2);
        let host_record = apps[0].world().resource::<RunHeatRecord>().clone();
        assert_eq!(host_record.heat, Some(3));
        let client = apps[1].world();
        assert_eq!(*client.resource::<RunHeat>(), host, "the client plays the Host's Degrees");
        let record = client.resource::<RunHeatRecord>();
        assert_eq!(record.heat, host_record.heat);
        assert_eq!(record.conditions.len(), host_record.conditions.len());
    }

    #[test]
    fn a_client_in_a_room_cannot_change_heat_with_or_without_a_win() {
        use crate::run::gauntlet::armory::{ArmoryButton, ArmoryNote, ArmoryTab, armory_click};
        use crate::run::gauntlet::heat::Condition;
        use crate::run::gauntlet::progress::GauntletFile;

        for has_won in [false, true] {
            let (_hub, mut apps) = room_with(1, Mode::Gauntlet, [0; 8]);
            // The client's session in a bare App, so only the Armory's observer hears the click.
            let session = apps[1].world_mut().remove_resource::<CoopSession>().expect("a client session");
            assert!(matches!(session, CoopSession::Join(_)));
            let mut app = App::new();
            app.insert_resource(session);
            let before = GauntletFile { has_won, ..Default::default() };
            app.insert_resource(before.clone());
            app.init_resource::<ArmoryTab>();
            app.init_resource::<ArmoryNote>();
            app.add_observer(armory_click);
            let button = app.world_mut().spawn(ArmoryButton::Heat(Condition::Inflation)).id();
            app.world_mut().trigger(bevy::ui_widgets::Activate { entity: button });
            let after = app.world().resource::<GauntletFile>();
            assert_eq!(after.run_heat(), before.run_heat(), "has_won {has_won}: Heat unchanged");
        }
    }

    /// A Player's Banish charges and Track entries, on the machine that holds their body.
    fn banish_and_speed(app: &mut App, seat: u8) -> (u32, usize) {
        use crate::modifier::Modifiers;
        use crate::run::gauntlet::Banishes;
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&crate::players::player::Seat, &Banishes, &Modifiers), With<Player>>();
        let (_, banishes, modifiers) = query.iter(world).find(|(held, ..)| held.0 == seat).expect("a body in the Seat");
        (banishes.charges, modifiers.0.len())
    }

    fn tracks(banish: u32, speed: u32) -> Unlocks {
        use crate::net::protocol::TrackState;
        let track = |id: &str, ranks| TrackState { id: id.into(), ranks, on: true };
        Unlocks { unlocked: Vec::new(), tracks: vec![track("banish", banish), track("move_speed", speed)], kit: String::new() }
    }

    /// The Weapon and Bowie Knife of the Player in `seat`, on the machine that holds their body.
    fn arms_of(app: &mut App, seat: u8) -> (crate::weapons::arsenal::WeaponId, bool) {
        use crate::weapons::weapon::{Knife, Loadout};
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&crate::players::player::Seat, &Loadout, &Knife), With<Player>>();
        let (_, loadout, knife) = query.iter(world).find(|(held, ..)| held.0 == seat).expect("a body in the Seat");
        (loadout.held().id, knife.bowie)
    }

    fn kit_on(app: &mut App, seat: u8) -> crate::run::gauntlet::kits::Kit {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(&crate::players::player::Seat, &crate::run::gauntlet::kits::PlayerKit), With<Player>>();
        query.iter(world).find(|(held, _)| held.0 == seat).expect("a body in the Seat").1.kit
    }

    #[test]
    fn a_remote_lead_kit_player_holds_the_m14_on_both_machines() {
        use crate::run::gauntlet::kits::Kit;
        use crate::run::gauntlet::progress::GauntletFile;
        use crate::weapons::arsenal::WeaponId;
        let save = |kit: Kit| GauntletFile {
            commendations: kit.commendation().map(String::from).into_iter().collect(),
            kit: Some(kit.id().into()),
            ..default()
        };
        let brought = |kit: Kit| Unlocks { kit: kit.id().into(), ..Unlocks::default() };
        let (hub, mut apps) = room_bringing(vec![brought(Kit::Lead), brought(Kit::Blade)], Mode::Gauntlet, [0; 8]);
        apps[1].world_mut().insert_resource(save(Kit::Lead));
        apps[2].world_mut().insert_resource(save(Kit::Blade));
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        assert_in_run(&mut apps);
        // The Host's copy of each remote Player matches what their machine gave them.
        assert_eq!(arms_of(&mut apps[0], 1), (WeaponId::M14, false), "the Lead Kit on the Host");
        assert_eq!(arms_of(&mut apps[0], 2), (crate::tuning::STARTING_WEAPON, true), "the Blade Kit on the Host");
        assert_eq!(arms_of(&mut apps[1], 1), (WeaponId::M14, false), "the Lead Kit on its own machine");
        assert_eq!(arms_of(&mut apps[2], 2), (crate::tuning::STARTING_WEAPON, true));
        assert_eq!(arms_of(&mut apps[0], 0), (crate::tuning::STARTING_WEAPON, false), "the Host's Standard Kit");
        // And the Host runs each remote Player's Kit rules.
        assert_eq!(kit_on(&mut apps[0], 1), Kit::Lead);
        assert_eq!(kit_on(&mut apps[0], 2), Kit::Blade);
    }

    #[test]
    fn classic_gives_every_remote_player_the_usual_loadout() {
        use crate::run::gauntlet::kits::Kit;
        let lead = Unlocks { kit: Kit::Lead.id().into(), ..Unlocks::default() };
        let (hub, mut apps) = room_bringing(vec![lead], Mode::Classic, [0; 8]);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        assert_in_run(&mut apps);
        assert_eq!(arms_of(&mut apps[0], 1), (crate::tuning::STARTING_WEAPON, false));
        assert_eq!(kit_on(&mut apps[0], 1), Kit::Standard);
    }

    #[test]
    fn the_host_applies_each_remote_players_own_tracks() {
        use crate::run::gauntlet::progress::{GauntletFile, Track, TrackSave};
        let mut host_save = GauntletFile::default();
        host_save.tracks.push(TrackSave { id: Track::Banish.id().into(), ranks: 1, on: true });
        // Seat 1 brings two Banish Ranks and Move speed; Seat 2 brings none of either.
        let (hub, mut apps) = room_bringing(vec![tracks(2, 3), tracks(0, 0)], Mode::Gauntlet, [0; 8]);
        for (at, armory) in [(1, tracks(2, 3)), (2, tracks(0, 0))] {
            // A client plays its own save: give each the Tracks it brought.
            let mut file = GauntletFile::default();
            for track in &armory.tracks {
                file.tracks.push(TrackSave { id: track.id.clone(), ranks: track.ranks, on: track.on });
            }
            apps[at].world_mut().insert_resource(file);
        }
        apps[0].world_mut().insert_resource(host_save);
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        assert_in_run(&mut apps);

        let per_rank = TRACK_BANISHES_PER_RANK;
        let base = GAUNTLET_BANISHES_START;
        // The Host holds every Player's body: each has their own Banish charges and entries.
        assert_eq!(banish_and_speed(&mut apps[0], 0), (base + per_rank, 0));
        assert_eq!(banish_and_speed(&mut apps[0], 1), (base + 2 * per_rank, 1), "the remote Player's own Tracks");
        assert_eq!(banish_and_speed(&mut apps[0], 2), (base, 0), "a Player with no Ranks");
        // And each client's own Player matches what the Host holds for them.
        assert_eq!(banish_and_speed(&mut apps[1], 1), (base + 2 * per_rank, 1));
        assert_eq!(banish_and_speed(&mut apps[2], 2), (base, 0));
    }

    /// A Gauntlet Room of the Host and `joiners` others, each machine with the Win's systems: the
    /// arc's clearing of the last Act, a client's inferring it, and Co-op's Win screen and wait.
    /// The Run starts in the Round whose clearing is the Win.
    fn win_room(joiners: usize) -> (Hub, Vec<App>) {
        use crate::authority::HostSimulation;
        use crate::run::gauntlet::{arc, win};
        let (hub, mut apps) = room_with(joiners, Mode::Gauntlet, [0; 8]);
        for app in &mut apps {
            app.insert_resource(crate::run::round::Round::in_progress(arc::win_round()))
                .init_resource::<RunArc>()
                .init_resource::<RunWon>()
                .init_resource::<win::WinWait>()
                .init_resource::<win::WinClock>()
                .add_message::<WinChoice>()
                .add_message::<crate::audio::PlayCue>()
                .add_message::<crate::run::round::RoundEnded>()
                .add_message::<crate::storage::records::SaveRecord>()
                .add_observer(arc::click_win)
                .add_systems(
                    Update,
                    (
                        arc::clear_boss_rounds.in_set(HostSimulation),
                        arc::infer_win.run_if(in_state(Authority::Client)),
                        (win::await_win_choices, win::settle_win_choices).chain().in_set(HostSimulation),
                        (win::open_win_screen, win::close_win_screen).chain(),
                    )
                        .chain()
                        .run_if(in_state(GameState::Playing)),
                );
        }
        assert!(start(&mut apps[0]));
        frames(&hub, &mut apps, 12);
        assert_in_run(&mut apps);
        // The Map's machines, so a client has a world to hear the Host's words in.
        for app in &mut apps {
            bevy::ecs::system::RunSystemOnce::run_system_once(app.world_mut(), crate::maps::map::spawn_map).unwrap();
        }
        frames(&hub, &mut apps, 6);
        (hub, apps)
    }

    /// The Host clears the last Act's Boss Round: the Win, on every machine.
    fn reach_the_win(hub: &Hub, apps: &mut [App]) {
        use crate::run::round::{Round, RoundEnded, RoundPhase};
        let world = apps[0].world_mut();
        world.write_message(RoundEnded);
        world.resource_mut::<Round>().phase = RoundPhase::Intermission(INTERMISSION_SECS);
        frames(hub, apps, 12);
    }

    fn win_screens(app: &mut App) -> usize {
        let world = app.world_mut();
        world.query_filtered::<(), With<crate::run::gauntlet::arc::WinScreen>>().iter(world).count()
    }

    /// The Player on `app` presses a Win screen button.
    fn press<B: Component>(app: &mut App) {
        let world = app.world_mut();
        let button = world.query_filtered::<Entity, With<B>>().single(world).expect("the Win screen's button");
        world.trigger(bevy::ui_widgets::Activate { entity: button });
    }

    fn waiting_on(app: &App) -> Vec<u8> {
        let mut seats: Vec<u8> =
            app.world().resource::<crate::run::gauntlet::win::WinWait>().waiting().iter().map(|seat| seat.0).collect();
        seats.sort_unstable();
        seats
    }

    #[test]
    fn every_player_gets_the_win_screen_at_the_win_with_no_freeze() {
        let (hub, mut apps) = win_room(1);
        reach_the_win(&hub, &mut apps);
        for (at, app) in apps.iter_mut().enumerate() {
            assert_eq!(win_screens(app), 1, "machine {at}");
            assert!(app.world().resource::<RunArc>().choosing, "machine {at}");
            assert!(app.world().resource::<RunWon>().0, "machine {at}");
            let pause = *app.world().resource::<State<crate::state::Pause>>().get();
            assert_ne!(pause, crate::state::Pause::Frozen, "machine {at}");
        }
        assert_eq!(waiting_on(&apps[0]), [0, 1]);
    }

    #[test]
    fn a_client_who_ends_the_run_at_the_win_keeps_it_and_gives_up_their_seat() {
        use crate::run::gauntlet::arc::EndRunButton;
        let (hub, mut apps) = win_room(2);
        reach_the_win(&hub, &mut apps);
        press::<EndRunButton>(&mut apps[1]);
        frames(&hub, &mut apps, 12);

        // Their Win is theirs to save, at Game Over, with no Room to go back to.
        assert_eq!(state(&apps[1]), GameState::GameOver);
        assert!(apps[1].world().resource::<RunWon>().0);
        assert!(!apps[1].world().resource::<Party>().can_go_back());
        // The rest play on without them, and the Host waits only for those still here.
        for at in [0, 2] {
            assert_eq!(state(&apps[at]), GameState::Playing, "machine {at}");
            assert_eq!(seats_and_local(&mut apps[at]).0, [0, 2], "machine {at}");
        }
        assert_eq!(waiting_on(&apps[0]), [0, 2]);
        // Their Seat is given up for good: not held for Rejoin, and gone from the Room.
        let host = apps[0].world();
        assert!(host.resource::<Roster>().0.iter().all(|&(_, seat)| seat != 1));
        assert!(host.resource::<Held>().away.is_empty());
        assert_eq!(host.resource::<Party>().room().expect("the Host's Room").players(), 2);
    }

    #[test]
    fn a_clients_keep_going_reaches_the_host_and_they_play_on() {
        use crate::run::gauntlet::arc::KeepGoingButton;
        let (hub, mut apps) = win_room(1);
        reach_the_win(&hub, &mut apps);
        press::<KeepGoingButton>(&mut apps[1]);
        frames(&hub, &mut apps, 12);
        assert_eq!(win_screens(&mut apps[1]), 0);
        assert_eq!(state(&apps[1]), GameState::Playing);
        assert_eq!(waiting_on(&apps[0]), [0], "the Host is still to choose");
        assert_eq!(seats_and_local(&mut apps[0]).0, [0, 1]);
        press::<KeepGoingButton>(&mut apps[0]);
        frames(&hub, &mut apps, 2);
        assert!(waiting_on(&apps[0]).is_empty());
        for app in &apps {
            assert!(app.world().resource::<RunWon>().0, "the Run stays a Win");
        }
    }

    #[test]
    fn the_hosts_end_the_run_ends_it_for_everyone_as_a_win() {
        use crate::run::gauntlet::arc::EndRunButton;
        let (hub, mut apps) = win_room(2);
        reach_the_win(&hub, &mut apps);
        // A client that has lost track of the Win (back from a dropped link) has it from the Host.
        apps[2].world_mut().resource_mut::<RunWon>().0 = false;
        apps[2].world_mut().resource_mut::<RunArc>().won = false;
        press::<EndRunButton>(&mut apps[0]);
        frames(&hub, &mut apps, 12);
        for (at, app) in apps.iter_mut().enumerate() {
            assert_eq!(state(app), GameState::GameOver, "machine {at}");
            assert!(app.world().resource::<RunWon>().0, "machine {at} keeps the Win");
            assert!(app.world().resource::<RunArc>().won, "machine {at}");
            assert_eq!(win_screens(app), 0, "machine {at}");
            assert!(app.world().resource::<Party>().can_go_back(), "machine {at}");
        }
        assert_ne!(apps[1].world().resource::<Notice>().0, "Host ended the Run");
    }

    #[test]
    fn no_choice_at_the_win_within_thirty_seconds_is_keep_going_on_every_machine() {
        let (hub, mut apps) = win_room(1);
        reach_the_win(&hub, &mut apps);
        let secs = |secs: f32| (secs * FIXED_HZ as f32) as usize;
        frames(&hub, &mut apps, secs(GAUNTLET_WIN_COOP_TIMEOUT_SECS - 2.0));
        assert_eq!(waiting_on(&apps[0]), [0, 1], "still inside the 30 s");
        for app in &mut apps {
            assert_eq!(win_screens(app), 1);
        }
        frames(&hub, &mut apps, secs(3.0));
        assert!(waiting_on(&apps[0]).is_empty(), "the Host waits no longer");
        for (at, app) in apps.iter_mut().enumerate() {
            assert_eq!(win_screens(app), 0, "machine {at}'s screen closed");
            assert!(!app.world().resource::<RunArc>().choosing, "machine {at}");
            assert_eq!(state(app), GameState::Playing, "machine {at} keeps going");
            assert!(app.world().resource::<RunWon>().0, "machine {at}'s Run stays a Win");
        }
        assert_eq!(seats_and_local(&mut apps[0]).0, [0, 1]);
    }
}
