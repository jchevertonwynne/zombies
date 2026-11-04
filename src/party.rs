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
use crate::coop::{CoopSession, Notice, armory_of};
use crate::gauntlet::progress::GauntletFile;
use crate::leave_run::PlayerLeft;
use crate::map::MapId;
use crate::mode::Mode;
use crate::net::link::Link;
use crate::net::play::{self, HostSaid, PlayPlugin, Roster, Wire};
use crate::net::protocol::{HOST, Peer, RoomView, ToHost, ToPlayer, encode};
use crate::net::room::{Hosting, Joining, Room};
use crate::player::Seat;
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
    stage: Stage,
    /// The Host's Room, kept for the party's return.
    room: Option<Room>,
    /// This machine's Player has asked to go back to the Room.
    back: bool,
    /// Game Over has been reached.
    over: bool,
    /// The other end has already been told, or has said it has gone.
    parted: bool,
}

impl Party {
    /// Who is in each Seat, and their Name.
    pub fn seats(&self) -> &[(Seat, String)] {
        &self.seats
    }

    /// This machine's Player wants back in the Room.
    pub fn back_to_room(&mut self) {
        self.back = true;
    }

    fn new(seats: Vec<(Seat, String)>, local: Seat, mode: Mode, map: MapId, room: Option<Room>) -> Self {
        Party { seats, local, mode, map, stage: Stage::Gathering, room, back: false, over: false, parted: false }
    }

    /// The Peers the Host's Room still has Players for.
    fn peers(&self) -> Option<Vec<Peer>> {
        self.room.as_ref().map(|room| room.remotes().iter().map(|&(peer, _)| peer).collect())
    }
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
    world.insert_resource(Party::new(seats, Seat(0), room.mode(), room.map(), Some(room)));
    play::begin_host(world, link, roster);
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
    world.insert_resource(Party::new(seats, Seat(seat), view.mode, view.map, None));
    play::begin_client(world, joining.into_link());
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

/// Game Over: the Host tells every Player, and the Run is over for each of them.
fn run_over(party: Option<ResMut<Party>>, roster: Option<Res<Roster>>, wire: Option<ResMut<Wire>>) {
    let Some(mut party) = party else { return };
    party.over = true;
    if let (Some(roster), Some(mut wire)) = (roster, wire) {
        for &(peer, _) in &roster.0 {
            wire.0.send(peer, encode(&ToPlayer::RunOver));
        }
    }
}

/// What the Host said, as a client hears it.
fn hear_host(
    mut said: MessageReader<HostSaid>,
    party: Option<ResMut<Party>>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut notice: ResMut<Notice>,
) {
    let Some(mut party) = party else {
        said.clear();
        return;
    };
    for said in said.read() {
        match said {
            HostSaid::RunOver => {
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
        }
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
    let Some(link) = play::end_run(world) else { return };
    let session = match party.room {
        Some(room) => CoopSession::Host(Box::new(Hosting::resume(link, room))),
        None => {
            let unlocks = armory_of(world.resource::<GauntletFile>());
            CoopSession::Join(Box::new(Joining::resume(link, party.local.0, unlocks)))
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
    if let Some(mut link) = play::end_run(world)
        && !party.parted
    {
        goodbye(&mut link, party.peers());
    }
    world.resource_mut::<NextState<MainMenuScreen>>().set(MainMenuScreen::Coop);
    set_authority_host(world);
}

/// Closing the window mid-Run is leaving it.
fn leave_on_exit(world: &mut World) {
    let Some(mut party) = world.remove_resource::<Party>() else { return };
    if !party.parted {
        let peers = party.peers();
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
        app.add_plugins(PlayPlugin)
            .add_systems(OnEnter(GameState::GameOver), run_over)
            .add_systems(OnEnter(GameState::MainMenu), leave_party)
            .add_systems(
                Update,
                (
                    launch.run_if(launching),
                    (hear_host, free_seat),
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
    use crate::coop::{close_unused_session, pump_session};
    use crate::menu::BackTo;
    use crate::modifier::RunModifiers;
    use crate::net::link::{Hub, Memory};
    use crate::net::protocol::Unlocks;
    use crate::net::room::Joined;
    use crate::player::{LocalPlayer, Player, SpawnPlayers, spawn_player};
    use crate::settings::Settings;
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
        .add_plugins((StatePlugin, AuthorityPlugin, crate::leave_run::LeaveRunPlugin, PartyPlugin))
        .insert_resource(session)
        .init_resource::<Notice>()
        .init_resource::<crate::coop::JoinTarget>()
        .init_resource::<BackTo>()
        .init_resource::<GauntletFile>()
        .init_resource::<RunModifiers>()
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
        let hub = Hub::new(1, 0.0);
        let room = Room::new("Hostess", Unlocks::default(), Mode::Classic);
        let mut hosting: Hosting<Box<dyn Link>> = Hosting::new(Box::new(hub.host()), room);
        let mut joining: Vec<Joining<Box<dyn Link>>> = (0..joiners)
            .map(|n| Joining::new(Box::new(hub.join()) as Box<dyn Link>, &format!("P{n}"), "", Unlocks::default()))
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
        let mut query = world.query_filtered::<(&crate::player::Seat, Has<LocalPlayer>), With<Player>>();
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

            // Every Player is down: each machine reaches Game Over.
            apps[0].world_mut().resource_mut::<NextState<GameState>>().set(GameState::GameOver);
            frames(&hub, &mut apps, 12);
            for app in &apps {
                assert_eq!(state(app), GameState::GameOver);
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
}
