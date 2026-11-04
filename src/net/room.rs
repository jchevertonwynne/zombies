//! The Room: where a Host and its Players gather before a Run.
//!
//! `Room` is the Host's book of who sits where. `Hosting` and `Joining` are the two ends
//! of a `Link` that keep it: the Host takes Players in and tells everyone what the Room
//! looks like whenever it changes, and a joiner says Hello, then Ready and its Armory.

use super::link::{Event, Link};
use super::protocol::{
    HOST, PROTOCOL, Peer, Refusal, RoomView, Seat, SeatView, ToHost, ToPlayer, Unlocks, answer,
    clean_name, decode, encode,
};
use crate::map::MapId;
use crate::mode::Mode;
use crate::tuning::*;

/// Someone in a Seat.
struct Member {
    peer: Peer,
    name: String,
    ready: bool,
    /// What they brought from their Armory.
    unlocks: Unlocks,
}

/// The Host's book of the Room: its settings and who sits in each Seat.
pub struct Room {
    code: String,
    mode: Mode,
    map: MapId,
    heat: u8,
    /// Whether the lobby lists the Room, or it is found only by its code.
    listed: bool,
    /// Empty if the Room has none.
    password: String,
    /// The Run has begun, and no one new is taken.
    started: bool,
    seats: [Option<Member>; ROOM_SEATS],
}

impl Room {
    /// A Room with its Host in the first Seat. The Host is never "not Ready".
    pub fn new(name: &str, unlocks: Unlocks, mode: Mode) -> Self {
        let host = Member {
            peer: HOST,
            name: clean_name(name),
            ready: true,
            unlocks,
        };
        let mut seats: [Option<Member>; ROOM_SEATS] = Default::default();
        seats[0] = Some(host);
        Room {
            code: String::new(),
            mode,
            map: MapId::default(),
            heat: 0,
            listed: true,
            password: String::new(),
            started: false,
            seats,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn map(&self) -> MapId {
        self.map
    }

    pub fn heat(&self) -> u8 {
        self.heat
    }

    pub fn is_listed(&self) -> bool {
        self.listed
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn has_password(&self) -> bool {
        !self.password.is_empty()
    }

    pub fn host_name(&self) -> &str {
        self.seats[0].as_ref().map_or("", |host| &host.name)
    }

    /// How many Seats are taken.
    pub fn players(&self) -> usize {
        self.seats.iter().flatten().count()
    }

    /// What the Armory of whoever sits in `seat` holds.
    pub fn unlocks(&self, seat: Seat) -> Option<&Unlocks> {
        self.seats.get(seat as usize)?.as_ref().map(|m| &m.unlocks)
    }

    pub fn seat_of(&self, peer: Peer) -> Option<usize> {
        self.seats
            .iter()
            .position(|seat| seat.as_ref().is_some_and(|m| m.peer == peer))
    }

    /// The Room has told the Players it has a code.
    pub fn set_code(&mut self, code: &str) {
        self.code = code.to_string();
    }

    /// The Run has begun (or the party is back for the next).
    pub fn set_started(&mut self, started: bool) {
        self.started = started;
    }

    /// Whether the Host may Start: enough Players, and every joiner Ready.
    pub fn can_start(&self) -> bool {
        self.players() >= ROOM_MIN_PLAYERS
            && !self.started
            && self.seats.iter().flatten().all(|member| member.ready)
    }

    /// The Players with a Seat and a Name, in Seat order.
    pub fn seated(&self) -> Vec<(Seat, String)> {
        (0..ROOM_SEATS)
            .filter_map(|at| Some((at as Seat, self.seats[at].as_ref()?.name.clone())))
            .collect()
    }

    /// Who is away from the Host and in which Seat, in Seat order.
    pub fn remotes(&self) -> Vec<(Peer, Seat)> {
        (0..ROOM_SEATS)
            .filter_map(|at| {
                let member = self.seats[at].as_ref()?;
                (member.peer != HOST).then_some((member.peer, at as Seat))
            })
            .collect()
    }

    /// The party is back from a Run: the Room takes new Players again, and no one is Ready.
    pub fn return_to_room(&mut self) {
        self.started = false;
        self.unready();
    }

    /// Everyone's Ready is taken back: the Room is not what they agreed to.
    fn unready(&mut self) {
        for member in self.seats.iter_mut().flatten().skip(1) {
            member.ready = false;
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.unready();
        }
    }

    pub fn set_map(&mut self, map: MapId) {
        if self.map != map {
            self.map = map;
            self.unready();
        }
    }

    #[allow(dead_code, reason = "Heat does not exist yet")]
    pub fn set_heat(&mut self, heat: u8) {
        if self.heat != heat {
            self.heat = heat;
            self.unready();
        }
    }

    pub fn set_listed(&mut self, listed: bool) {
        if self.listed != listed {
            self.listed = listed;
            self.unready();
        }
    }

    pub fn set_password(&mut self, password: &str) {
        let password: String = password.chars().take(ROOM_PASSWORD_MAX_CHARS).collect();
        if self.password != password {
            self.password = password;
            self.unready();
        }
    }

    /// Takes the Host's own Armory, which the Host's Heat and Offers go by.
    pub fn set_host_unlocks(&mut self, unlocks: Unlocks) {
        if let Some(host) = self.seats[0].as_mut() {
            host.unlocks = unlocks;
        }
    }

    pub fn set_host_name(&mut self, name: &str) {
        if let Some(host) = self.seats[0].as_mut() {
            host.name = clean_name(name);
        }
    }

    /// Answers a Player's first message: a Welcome into the lowest free Seat, or the reason
    /// they are refused. Checked in the order a Player can fix them: version, Run, password,
    /// then room.
    pub fn admit(&mut self, peer: Peer, hello: &ToHost) -> ToPlayer {
        let ToHost::Hello {
            name,
            password,
            unlocks,
            ..
        } = hello
        else {
            return ToPlayer::Refused(Refusal::Version);
        };
        let welcome = answer(hello, 0);
        if matches!(welcome, ToPlayer::Refused(_)) {
            return welcome;
        }
        if self.started {
            return ToPlayer::Refused(Refusal::Started);
        }
        if self.has_password() && *password != self.password {
            return ToPlayer::Refused(Refusal::WrongPassword);
        }
        let Some(free) = self.seats.iter().position(Option::is_none) else {
            return ToPlayer::Refused(Refusal::Full);
        };
        self.seats[free] = Some(Member {
            peer,
            name: clean_name(name),
            ready: false,
            unlocks: unlocks.clone(),
        });
        match welcome {
            ToPlayer::Welcome { token, .. } => ToPlayer::Welcome {
                token,
                seat: free as Seat,
            },
            other => other,
        }
    }

    /// A joiner is Ready or not. The Host is always Ready.
    pub fn set_ready(&mut self, peer: Peer, ready: bool) {
        if let Some(member) = self.seat_of(peer).and_then(|at| self.seats[at].as_mut())
            && peer != HOST
        {
            member.ready = ready;
        }
    }

    /// A joiner's Armory has changed: they are no longer Ready.
    pub fn set_unlocks(&mut self, peer: Peer, unlocks: Unlocks) {
        if let Some(member) = self.seat_of(peer).and_then(|at| self.seats[at].as_mut()) {
            if member.unlocks != unlocks {
                member.unlocks = unlocks;
                member.ready = peer == HOST;
            }
        }
    }

    /// Frees whoever's Seat this was, if it was anyone's.
    pub fn remove(&mut self, peer: Peer) -> bool {
        match self.seat_of(peer) {
            Some(at) if peer != HOST => {
                self.seats[at] = None;
                true
            }
            _ => false,
        }
    }

    /// The peer in a Seat that the Host may kick: anyone but the Host.
    pub fn kickable(&self, seat: Seat) -> Option<Peer> {
        match self.seats.get(seat as usize)? {
            Some(member) if seat != 0 => Some(member.peer),
            _ => None,
        }
    }

    pub fn view(&self) -> RoomView {
        RoomView {
            code: self.code.clone(),
            mode: self.mode,
            map: self.map,
            heat: self.heat,
            locked: self.has_password(),
            seats: std::array::from_fn(|at| {
                self.seats[at].as_ref().map(|member| SeatView {
                    name: member.name.clone(),
                    ready: member.ready,
                })
            }),
        }
    }
}

/// The Host's end of a Room.
pub struct Hosting<L: Link> {
    link: L,
    pub room: Room,
    /// The Room as the Players were last told it.
    told: Option<RoomView>,
    /// The Host has left, and the Players have been told.
    closed: bool,
}

impl<L: Link> Hosting<L> {
    pub fn new(link: L, room: Room) -> Self {
        Hosting {
            link,
            room,
            told: None,
            closed: false,
        }
    }

    /// What Players dial to reach this Room, once it is known.
    pub fn address(&self) -> Option<String> {
        self.link.address()
    }

    /// Takes in whatever has arrived, then tells every Player of any change to the Room.
    /// Returns what went wrong with the link itself, if anything.
    pub fn poll(&mut self) -> Option<String> {
        let mut failed = None;
        while let Some(event) = self.link.poll() {
            match event {
                Event::Message(peer, bytes) => self.hear(peer, &bytes),
                Event::Left(peer) | Event::Lost(peer) => {
                    self.room.remove(peer);
                }
                Event::Failed(why) => failed = Some(why),
                Event::Joined(_) | Event::Datagram(..) => {}
            }
        }
        self.tell();
        failed
    }

    fn hear(&mut self, peer: Peer, bytes: &[u8]) {
        let Ok(message) = decode::<ToHost>(bytes) else {
            return;
        };
        match message {
            ToHost::Hello { .. } if self.room.seat_of(peer).is_some() => {}
            ToHost::Hello { .. } => {
                let answer = self.room.admit(peer, &message);
                let refused = matches!(answer, ToPlayer::Refused(_));
                self.link.send(peer, encode(&answer));
                if refused {
                    self.link.close(peer);
                } else {
                    // A new Player is sent the Room, even if it looks as it last did.
                    self.told = None;
                }
            }
            ToHost::Ready(ready) => self.room.set_ready(peer, ready),
            ToHost::Unlocks(unlocks) => self.room.set_unlocks(peer, unlocks),
            ToHost::LeftRun => {}
        }
    }

    /// Sends every Player the Room, if it differs from what they were last sent. A Player
    /// who has just been welcomed has not been sent it, and so is.
    fn tell(&mut self) {
        let view = self.room.view();
        if self.told.as_ref() == Some(&view) {
            return;
        }
        for member in self.room.seats.iter().flatten() {
            if member.peer != HOST {
                self.link
                    .send(member.peer, encode(&ToPlayer::Room(view.clone())));
            }
        }
        self.told = Some(view);
    }

    /// Changes the Room, and tells the Players if it did.
    pub fn edit(&mut self, change: impl FnOnce(&mut Room)) {
        change(&mut self.room);
        self.tell();
    }

    /// Sends the Player in `seat` away.
    pub fn kick(&mut self, seat: Seat) {
        if let Some(peer) = self.room.kickable(seat) {
            self.link.send(peer, encode(&ToPlayer::Kicked));
            self.link.close(peer);
            self.room.remove(peer);
            self.tell();
        }
    }

    /// Starts the Run if the Room may: every Player is told, and the Room is kept as it stood.
    /// Returns the link, the Room, and who is in which Seat, ready for the Run to use.
    pub fn start(mut self) -> Result<(L, Room, Vec<(Peer, Seat)>), Self> {
        if !self.room.can_start() {
            return Err(self);
        }
        self.room.set_started(true);
        let roster = self.room.remotes();
        let view = self.room.view();
        for &(peer, _) in &roster {
            self.link.send(peer, encode(&ToPlayer::Start(view.clone())));
        }
        Ok((self.link, self.room, roster))
    }

    /// The party is back from a Run: the Room is as it was, with no one Ready, and every Player
    /// still in it is told to come back.
    pub fn resume(link: L, mut room: Room) -> Self {
        room.return_to_room();
        let mut hosting = Hosting::new(link, room);
        for (peer, _) in hosting.room.remotes() {
            hosting.link.send(peer, encode(&ToPlayer::BackToRoom));
        }
        hosting.tell();
        hosting
    }

    /// The Host leaves: the Room closes, and every Player is told.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        let peers: Vec<Peer> = self
            .room
            .seats
            .iter()
            .flatten()
            .map(|member| member.peer)
            .filter(|&peer| peer != HOST)
            .collect();
        for peer in peers {
            self.link.send(peer, encode(&ToPlayer::Closed));
            self.link.close(peer);
        }
    }
}

/// Where a joiner is in getting into a Room.
#[derive(Clone, Debug, PartialEq)]
pub enum Joined {
    /// Reaching the Host.
    Dialling,
    /// Hello is said, and the Host has not answered.
    Waiting,
    /// In a Seat.
    In { seat: Seat, view: Option<RoomView> },
    Refused(Refusal),
    Kicked,
    /// The Host has started the Run: the Seat, and the Room as it stood.
    Started { seat: Seat, view: RoomView },
    /// "Host closed the room".
    Closed,
    /// The way to the Host is gone: why.
    Lost(String),
}

impl Joined {
    /// Whether this is the end of the joiner's time in the Room.
    pub fn is_over(&self) -> bool {
        !matches!(
            self,
            Joined::Dialling | Joined::Waiting | Joined::In { .. } | Joined::Started { .. }
        )
    }
}

/// A joiner's end of a Room.
pub struct Joining<L: Link> {
    link: L,
    hello: ToHost,
    pub state: Joined,
    /// What the Host was last told this Player's Armory held.
    sent: Unlocks,
}

impl<L: Link> Joining<L> {
    pub fn new(link: L, name: &str, password: &str, unlocks: Unlocks) -> Self {
        Joining {
            link,
            hello: ToHost::Hello {
                version: PROTOCOL,
                back: None,
                name: clean_name(name),
                password: password.to_string(),
                unlocks: unlocks.clone(),
            },
            state: Joined::Dialling,
            sent: unlocks,
        }
    }

    /// Takes in whatever the Host has said.
    pub fn poll(&mut self) {
        // Once the Run has started, what comes next is the Run's to read.
        while !matches!(self.state, Joined::Started { .. }) {
            let Some(event) = self.link.poll() else { break };
            if self.state.is_over() {
                continue;
            }
            match event {
                Event::Joined(HOST) => {
                    self.link.send(HOST, encode(&self.hello));
                    self.state = Joined::Waiting;
                }
                Event::Message(HOST, bytes) => self.hear(&bytes),
                Event::Left(_) | Event::Lost(_) => {
                    self.state = Joined::Lost("Lost the connection to the Host".into())
                }
                Event::Failed(why) => self.state = Joined::Lost(why),
                _ => {}
            }
        }
    }

    fn hear(&mut self, bytes: &[u8]) {
        let Ok(message) = decode::<ToPlayer>(bytes) else {
            return;
        };
        match (message, &mut self.state) {
            (ToPlayer::Welcome { seat, .. }, state) => *state = Joined::In { seat, view: None },
            (ToPlayer::Refused(why), state) => *state = Joined::Refused(why),
            (ToPlayer::Room(room), Joined::In { view, .. }) => *view = Some(room),
            (ToPlayer::Kicked, state) => *state = Joined::Kicked,
            (ToPlayer::Closed, state) => *state = Joined::Closed,
            (ToPlayer::Start(view), state) => {
                if let Joined::In { seat, .. } = *state {
                    *state = Joined::Started { seat, view };
                }
            }
            (ToPlayer::Room(_) | ToPlayer::RunOver | ToPlayer::BackToRoom, _) => {}
        }
    }

    pub fn ready(&mut self, ready: bool) {
        if matches!(self.state, Joined::In { .. }) {
            self.link.send(HOST, encode(&ToHost::Ready(ready)));
        }
    }

    /// Tells the Host the Armory has changed since it was last told, if it has.
    pub fn armory(&mut self, unlocks: &Unlocks) {
        if matches!(self.state, Joined::In { .. }) && self.sent != *unlocks {
            self.sent = unlocks.clone();
            self.link.send(HOST, encode(&ToHost::Unlocks(unlocks.clone())));
        }
    }

    /// The party is back from a Run in `seat`: the Host sends the Room again.
    pub fn resume(link: L, seat: Seat, unlocks: Unlocks) -> Self {
        let mut joining = Joining::new(link, "", "", unlocks);
        joining.state = Joined::In { seat, view: None };
        joining
    }

    /// The link, once the Run has started and it is the Run's.
    pub fn into_link(self) -> L {
        self.link
    }

    /// The Player leaves the Room.
    pub fn leave(&mut self) {
        self.link.close(HOST);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::link::{Hub, Memory};
    use crate::net::protocol::TrackState;

    fn armory(ranks: u32) -> Unlocks {
        Unlocks {
            unlocked: vec!["boon:x".into()],
            tracks: vec![TrackState {
                id: "move_speed".into(),
                ranks,
                on: true,
            }],
        }
    }

    /// A Host with its Room, and a hub to join it on.
    fn open() -> (Hub, Hosting<Memory>) {
        let hub = Hub::new(1, 0.0);
        let room = Room::new("Hostess", Unlocks::default(), Mode::Classic);
        let hosting = Hosting::new(hub.host(), room);
        (hub, hosting)
    }

    fn join(hub: &Hub, name: &str, password: &str) -> Joining<Memory> {
        Joining::new(hub.join(), name, password, armory(1))
    }

    /// Lets the network run until everything said has been heard.
    fn settle<L: Link, M: Link>(
        hub: &Hub,
        hosting: &mut Hosting<L>,
        joiners: &mut [&mut Joining<M>],
    ) {
        for _ in 0..12 {
            hub.step();
            hosting.poll();
            for joiner in joiners.iter_mut() {
                joiner.poll();
            }
        }
    }

    fn names(view: &RoomView) -> Vec<Option<(&str, bool)>> {
        view.seats
            .iter()
            .map(|seat| seat.as_ref().map(|s| (s.name.as_str(), s.ready)))
            .collect()
    }

    fn view_of<L: Link>(joiner: &Joining<L>) -> RoomView {
        match &joiner.state {
            Joined::In { view: Some(view), .. } => view.clone(),
            other => panic!("not in a Room: {other:?}"),
        }
    }

    #[test]
    fn players_take_seats_in_join_order_and_see_the_room() {
        let (hub, mut host) = open();
        host.edit(|room| room.set_code("ABCDE"));
        let mut ada = join(&hub, "Ada", "");
        let mut bo = join(&hub, "Bo", "");
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        assert!(matches!(ada.state, Joined::In { seat: 1, .. }));
        assert!(matches!(bo.state, Joined::In { seat: 2, .. }));
        let view = view_of(&bo);
        assert_eq!(view.code, "ABCDE");
        assert_eq!(
            names(&view),
            [
                Some(("Hostess", true)),
                Some(("Ada", false)),
                Some(("Bo", false)),
                None
            ]
        );
        assert_eq!(view_of(&ada), view);
    }

    #[test]
    fn a_name_is_cut_to_twelve_characters_on_joining() {
        let (hub, mut host) = open();
        let mut long = join(&hub, "Bartholomew The Third", "");
        settle(&hub, &mut host, &mut [&mut long]);
        assert_eq!(view_of(&long).seats[1].as_ref().unwrap().name, "Bartholomew");
    }

    #[test]
    fn a_full_room_refuses_a_fifth_player() {
        let (hub, mut host) = open();
        let mut joiners: Vec<_> = (0..3).map(|n| join(&hub, &format!("P{n}"), "")).collect();
        let mut refs: Vec<&mut Joining<_>> = joiners.iter_mut().collect();
        settle(&hub, &mut host, &mut refs);
        let mut late = join(&hub, "Late", "");
        settle(&hub, &mut host, &mut [&mut late]);
        assert_eq!(late.state, Joined::Refused(Refusal::Full));
        assert_eq!(host.room.players(), 4);
    }

    #[test]
    fn a_wrong_or_missing_password_is_refused_and_the_right_one_is_not() {
        let (hub, mut host) = open();
        host.edit(|room| room.set_password("hunter2"));
        let (mut none, mut wrong, mut right) = (
            join(&hub, "None", ""),
            join(&hub, "Wrong", "hunter3"),
            join(&hub, "Right", "hunter2"),
        );
        settle(&hub, &mut host, &mut [&mut none, &mut wrong, &mut right]);
        assert_eq!(none.state, Joined::Refused(Refusal::WrongPassword));
        assert_eq!(wrong.state, Joined::Refused(Refusal::WrongPassword));
        assert!(matches!(right.state, Joined::In { seat: 1, .. }));
        assert!(view_of(&right).locked);
    }

    #[test]
    fn nobody_joins_once_the_run_has_started() {
        let (hub, mut host) = open();
        host.edit(|room| room.set_started(true));
        let mut late = join(&hub, "Late", "");
        settle(&hub, &mut host, &mut [&mut late]);
        assert_eq!(late.state, Joined::Refused(Refusal::Started));
    }

    #[test]
    fn a_different_version_is_refused() {
        let (hub, mut host) = open();
        let mut old = join(&hub, "Old", "");
        old.hello = ToHost::Hello {
            version: PROTOCOL + 1,
            back: None,
            name: "Old".into(),
            password: String::new(),
            unlocks: Unlocks::default(),
        };
        settle(&hub, &mut host, &mut [&mut old]);
        assert_eq!(old.state, Joined::Refused(Refusal::Version));
        assert_eq!(host.room.players(), 1);
    }

    #[test]
    fn ready_is_toggled_by_the_joiner() {
        let (hub, mut host) = open();
        let mut ada = join(&hub, "Ada", "");
        settle(&hub, &mut host, &mut [&mut ada]);
        ada.ready(true);
        settle(&hub, &mut host, &mut [&mut ada]);
        assert_eq!(view_of(&ada).seats[1].as_ref().unwrap().ready, true);
        ada.ready(false);
        settle(&hub, &mut host, &mut [&mut ada]);
        assert_eq!(view_of(&ada).seats[1].as_ref().unwrap().ready, false);
    }

    #[test]
    fn every_change_to_the_room_clears_ready() {
        type Change = fn(&mut Room);
        let changes: [(&str, Change); 4] = [
            ("mode", |room| room.set_mode(Mode::Gauntlet)),
            ("listing", |room| room.set_listed(false)),
            ("password", |room| room.set_password("x")),
            ("heat", |room| room.set_heat(3)),
        ];
        for (what, change) in changes {
            let (hub, mut host) = open();
            let mut ada = join(&hub, "Ada", "");
            settle(&hub, &mut host, &mut [&mut ada]);
            ada.ready(true);
            settle(&hub, &mut host, &mut [&mut ada]);
            assert!(view_of(&ada).seats[1].as_ref().unwrap().ready, "{what}");
            host.edit(change);
            settle(&hub, &mut host, &mut [&mut ada]);
            assert!(!view_of(&ada).seats[1].as_ref().unwrap().ready, "{what}");
        }
    }

    #[test]
    fn setting_a_setting_to_what_it_is_clears_nothing() {
        let (hub, mut host) = open();
        let mut ada = join(&hub, "Ada", "");
        settle(&hub, &mut host, &mut [&mut ada]);
        ada.ready(true);
        settle(&hub, &mut host, &mut [&mut ada]);
        host.edit(|room| {
            room.set_mode(Mode::Classic);
            room.set_map(MapId::Greybox);
            room.set_listed(true);
            room.set_password("");
        });
        settle(&hub, &mut host, &mut [&mut ada]);
        assert!(view_of(&ada).seats[1].as_ref().unwrap().ready);
    }

    #[test]
    fn a_changed_armory_is_resent_and_clears_that_players_ready_alone() {
        let (hub, mut host) = open();
        let (mut ada, mut bo) = (join(&hub, "Ada", ""), join(&hub, "Bo", ""));
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        ada.ready(true);
        bo.ready(true);
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        assert_eq!(host.room.unlocks(1), Some(&armory(1)));

        ada.armory(&armory(1));
        ada.armory(&armory(2));
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        assert_eq!(host.room.unlocks(1), Some(&armory(2)));
        let view = view_of(&bo);
        assert_eq!(view.seats[1].as_ref().unwrap().ready, false);
        assert_eq!(view.seats[2].as_ref().unwrap().ready, true);
    }

    #[test]
    fn the_host_kicks_a_player_and_cannot_kick_themselves() {
        let (hub, mut host) = open();
        let (mut ada, mut bo) = (join(&hub, "Ada", ""), join(&hub, "Bo", ""));
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        host.kick(0);
        host.kick(1);
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        assert_eq!(ada.state, Joined::Kicked);
        assert!(matches!(bo.state, Joined::In { seat: 2, .. }));
        assert_eq!(
            names(&view_of(&bo)),
            [Some(("Hostess", true)), None, Some(("Bo", false)), None]
        );
        // The freed Seat is the next one taken.
        let mut cy = join(&hub, "Cy", "");
        settle(&hub, &mut host, &mut [&mut bo, &mut cy]);
        assert!(matches!(cy.state, Joined::In { seat: 1, .. }));
    }

    #[test]
    fn a_player_who_leaves_frees_their_seat() {
        let (hub, mut host) = open();
        let mut ada = join(&hub, "Ada", "");
        settle(&hub, &mut host, &mut [&mut ada]);
        hub.cut(ada.link.peer());
        settle(&hub, &mut host, &mut [&mut ada]);
        assert_eq!(host.room.players(), 1);
    }

    #[test]
    fn the_host_closing_the_room_tells_everyone() {
        let (hub, mut host) = open();
        let (mut ada, mut bo) = (join(&hub, "Ada", ""), join(&hub, "Bo", ""));
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        host.close();
        settle(&hub, &mut host, &mut [&mut ada, &mut bo]);
        assert_eq!(ada.state, Joined::Closed);
        assert_eq!(bo.state, Joined::Closed);
    }
}
