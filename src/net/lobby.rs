//! The game's side of the lobby at play.jchevertonwynne.uk: the list of Rooms being
//! hosted, shared with Block Racers and kept by a small server there.
//!
//! A Host puts its Room on the list and says every few seconds that it is still there; a
//! Player reads the list, or finds a Room by its code, and dials the Host it picks.
//! Nothing of a Run goes by way of the lobby. Asking it anything waits on the network,
//! so it is done on `transport::runtime` and the answers are picked up here a frame or
//! more later. What is asked sits behind `LobbyApi`, so tests use a stand-in.
//!
//! The request and answer types are copied from Block Racers' `lobby-api` crate (0.1.0),
//! which is serde-only.

use std::future::Future;
use std::pin::Pin;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::protocol::PROTOCOL;
use super::transport::runtime;
use crate::map::MapId;
use crate::mode::Mode;
use crate::tuning::*;

/// Where the lobby is, unless `ZOMBIES_LOBBY` says otherwise.
const LOBBY: &str = "https://play.jchevertonwynne.uk";
/// The game key Rooms are listed under, which keeps them apart from Block Racers' sessions.
pub const GAME: &str = "zombies";
/// The most a Room holds, as the lobby is told.
pub const MAX_PLAYERS: u8 = ROOM_SEATS as u8;
/// The longest a name may be, in bytes: the Room's, the Host's, the status line.
pub const MAX_NAME: usize = 32;

/// A Host putting its Room on the list.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Register {
    pub game: String,
    pub protocol: u32,
    pub name: String,
    /// The name of the Player hosting.
    pub host: String,
    /// How to dial the Host.
    pub endpoint: String,
    pub locked: bool,
    pub max: u8,
    pub status: Status,
    /// Kept off the list, to be found only by its code.
    pub unlisted: bool,
}

/// What changes about a Room while it is listed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub players: u8,
    /// Map, Mode and Heat; see `status_line`.
    pub circuit: String,
    /// The Run is on, and the Room takes no one new.
    pub racing: bool,
}

/// The lobby's answer to `Register`. The `token` is what lets the Host, and nobody else,
/// change the Room's listing or take it down.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Registered {
    pub id: String,
    pub token: String,
    /// What a Player may type to find the Room.
    #[serde(default)]
    pub code: String,
}

/// A Room as the list shows it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Listed {
    pub id: String,
    pub name: String,
    pub host: String,
    pub endpoint: String,
    pub locked: bool,
    pub max: u8,
    pub status: Status,
}

const SEPARATOR: &str = " · ";

/// The status line a Room lists: Map, Mode and, once there is any, Heat.
pub fn status_line(map: MapId, mode: Mode, heat: u8) -> String {
    let mut line = format!("{}{SEPARATOR}{mode:?}", map.name());
    if heat > 0 {
        line.push_str(&format!("{SEPARATOR}Heat {heat}"));
    }
    line
}

/// What a listed Room's status line says, split into Map, Mode and Heat.
pub fn read_status_line(line: &str) -> (String, String, u8) {
    let mut parts = line.split(SEPARATOR);
    let map = parts.next().unwrap_or_default().to_string();
    let mode = parts.next().unwrap_or_default().to_string();
    let heat = parts
        .next()
        .and_then(|heat| heat.strip_prefix("Heat "))
        .and_then(|heat| heat.parse().ok())
        .unwrap_or(0);
    (map, mode, heat)
}

/// `text` cut to at most `bytes` bytes without splitting a character, as the lobby counts
/// names in bytes.
pub fn fit(text: &str, bytes: usize) -> String {
    let mut fitted = String::new();
    for c in text.chars() {
        if fitted.len() + c.len_utf8() > bytes {
            break;
        }
        fitted.push(c);
    }
    fitted
}

#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum LobbyError {
    #[error("the lobby couldn't be reached: {0}")]
    Unreached(String),
    #[error("the lobby said {0}")]
    Refused(u16),
}

pub type Reply<T> = Pin<Box<dyn Future<Output = Result<T, LobbyError>> + Send>>;

/// What the game asks of the lobby.
pub trait LobbyApi: Send + Sync + 'static {
    /// The Rooms on the list.
    fn list(&self) -> Reply<Vec<Listed>>;
    /// The Room a code is for, if it is for any.
    fn find(&self, code: &str) -> Reply<Option<Listed>>;
    fn register(&self, ask: Register) -> Reply<Registered>;
    /// Says the Room is still there, and what it is like now.
    fn beat(&self, listed: &Registered, status: Status) -> Reply<()>;
    /// Takes the Room off the list.
    fn close(&self, listed: &Registered) -> Reply<()>;
}

/// The lobby over HTTPS.
pub struct Http {
    url: String,
    client: reqwest::Client,
}

impl Http {
    pub fn new() -> Self {
        // The runtime's making is also what readies the client for HTTPS.
        runtime();
        Http {
            url: std::env::var("ZOMBIES_LOBBY").unwrap_or_else(|_| LOBBY.to_string()),
            client: reqwest::Client::new(),
        }
    }
}

fn unreached(error: reqwest::Error) -> LobbyError {
    LobbyError::Unreached(error.to_string())
}

async fn answered(
    sent: Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response, LobbyError> {
    let answer = sent.map_err(unreached)?;
    if answer.status().is_success() {
        Ok(answer)
    } else {
        Err(LobbyError::Refused(answer.status().as_u16()))
    }
}

impl LobbyApi for Http {
    fn list(&self) -> Reply<Vec<Listed>> {
        let request = self.client.get(format!(
            "{}/sessions?protocol={PROTOCOL}&game={GAME}",
            self.url
        ));
        Box::pin(async move {
            answered(request.send().await)
                .await?
                .json()
                .await
                .map_err(unreached)
        })
    }

    fn find(&self, code: &str) -> Reply<Option<Listed>> {
        let request = self.client.get(format!(
            "{}/codes/{}?protocol={PROTOCOL}&game={GAME}",
            self.url,
            code.trim()
        ));
        Box::pin(async move {
            match answered(request.send().await).await {
                Ok(answer) => Ok(Some(answer.json().await.map_err(unreached)?)),
                Err(LobbyError::Refused(404)) => Ok(None),
                Err(error) => Err(error),
            }
        })
    }

    fn register(&self, ask: Register) -> Reply<Registered> {
        let request = self.client.post(format!("{}/sessions", self.url)).json(&ask);
        Box::pin(async move {
            answered(request.send().await)
                .await?
                .json()
                .await
                .map_err(unreached)
        })
    }

    fn beat(&self, listed: &Registered, status: Status) -> Reply<()> {
        let request = self
            .client
            .put(format!("{}/sessions/{}", self.url, listed.id))
            .bearer_auth(&listed.token)
            .json(&status);
        Box::pin(async move { answered(request.send().await).await.map(drop) })
    }

    fn close(&self, listed: &Registered) -> Reply<()> {
        let request = self
            .client
            .delete(format!("{}/sessions/{}", self.url, listed.id))
            .bearer_auth(&listed.token);
        Box::pin(async move { answered(request.send().await).await.map(drop) })
    }
}

/// What a Host wants the lobby to say of its Room.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub host: String,
    pub endpoint: String,
    pub locked: bool,
    pub unlisted: bool,
    pub status: Status,
}

impl Listing {
    /// What can't be changed by a beat, and so needs the Room listed afresh when it does.
    fn fixed(&self) -> (String, String, bool, bool) {
        (
            self.host.clone(),
            self.endpoint.clone(),
            self.locked,
            self.unlisted,
        )
    }
}

enum Heard {
    List(Vec<Listed>),
    Found(Option<Listed>),
    Registered(Registered),
    /// A beat was answered.
    Beat,
    /// The lobby no longer has the Room: it is to be listed again.
    Forgotten,
    Failed(String),
}

#[derive(Resource)]
pub struct LobbyClient {
    api: Arc<dyn LobbyApi>,
    tell: Sender<Heard>,
    heard: Mutex<Receiver<Heard>>,
    /// The Rooms on the list when it was last read.
    pub sessions: Vec<Listed>,
    /// What a code turned out to be for, once the lobby has said: a Room, or none.
    pub found: Option<Option<Listed>>,
    /// What went wrong the last time the lobby was asked something, if anything did.
    pub trouble: Option<String>,
    /// Counts up whenever the list or a found Room changes, for whatever draws them.
    pub revision: u32,
    /// This game's own Room on the list, and whether it is being put there now.
    listed: Option<Registered>,
    listing: bool,
    told: Option<(String, String, bool, bool)>,
    last_status: Option<Status>,
    /// Seconds until the lobby is next told the Room is still there, or asked again.
    beat: f32,
}

impl Default for LobbyClient {
    fn default() -> Self {
        Self::new(Arc::new(Http::new()))
    }
}

impl LobbyClient {
    pub fn new(api: Arc<dyn LobbyApi>) -> Self {
        let (tell, heard) = channel();
        LobbyClient {
            api,
            tell,
            heard: Mutex::new(heard),
            sessions: Vec::new(),
            found: None,
            trouble: None,
            revision: 0,
            listed: None,
            listing: false,
            told: None,
            last_status: None,
            beat: 0.0,
        }
    }

    /// Does `ask` on the network's threads and keeps what it comes back with.
    fn ask<T: Send + 'static>(
        &self,
        ask: Reply<T>,
        heard: impl FnOnce(T) -> Heard + Send + 'static,
    ) {
        let tell = self.tell.clone();
        runtime().spawn(async move {
            let _ = tell.send(match ask.await {
                Ok(answer) => heard(answer),
                Err(LobbyError::Refused(404)) => Heard::Forgotten,
                Err(error) => Heard::Failed(error.to_string()),
            });
        });
    }

    /// Reads the list afresh; `sessions` has it once the lobby has answered.
    pub fn list(&self) {
        self.ask(self.api.list(), Heard::List);
    }

    /// Asks which Room a code is for; `found` has the answer once there is one.
    pub fn find(&mut self, code: &str) {
        self.found = None;
        self.ask(self.api.find(code), Heard::Found);
    }

    /// The code the lobby gave this game's Room, once it has.
    pub fn code(&self) -> Option<&str> {
        self.listed
            .as_ref()
            .map(|listed| listed.code.as_str())
            .filter(|code| !code.is_empty())
    }

    /// Whether this game's Room is on the lobby's list now.
    pub fn is_listed(&self) -> bool {
        self.listed.is_some()
    }

    /// Takes this game's Room off the list.
    pub fn close(&mut self) {
        self.told = None;
        self.last_status = None;
        if let Some(listed) = self.listed.take() {
            let closing = self.api.close(&listed);
            runtime().spawn(async move { drop(closing.await) });
        }
    }

    /// The game is closing: the Room comes off the list first, and is not left there for
    /// the lobby to give up on. Not waited on for long.
    pub fn farewell(&mut self) {
        self.told = None;
        if let Some(listed) = self.listed.take() {
            let closing = self.api.close(&listed);
            let _ = runtime().block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(2), closing).await
            });
        }
    }

    /// Picks up the lobby's answers, and keeps `listing`, if there is one, on the list:
    /// registered when it can be, beaten every `LOBBY_BEAT_SECS` and whenever it changes,
    /// and taken off when there is none. Returns whether the list, a found Room, the
    /// trouble or this Room's code changed.
    pub fn tick(&mut self, seconds: f32, listing: Option<&Listing>) -> bool {
        let mut changed = false;
        let heard: Vec<Heard> = self
            .heard
            .lock()
            .map(|heard| heard.try_iter().collect())
            .unwrap_or_default();
        for heard in heard {
            changed = true;
            match heard {
                Heard::List(sessions) => {
                    (self.sessions, self.trouble, self.revision) =
                        (sessions, None, self.revision + 1)
                }
                Heard::Found(found) => {
                    (self.found, self.trouble, self.revision) =
                        (Some(found), None, self.revision + 1)
                }
                Heard::Registered(listed) => {
                    (self.listed, self.listing, self.beat, self.trouble) =
                        (Some(listed), false, LOBBY_BEAT_SECS, None)
                }
                Heard::Beat => changed = false,
                Heard::Forgotten => {
                    (self.listed, self.last_status) = (None, None);
                    self.beat = 0.0;
                }
                Heard::Failed(why) => {
                    warn!("{why}");
                    (self.trouble, self.listing, self.revision) =
                        (Some(why), false, self.revision + 1);
                }
            }
        }
        let Some(listing) = listing else {
            if self.listed.is_some() {
                self.close();
                changed = true;
            }
            return changed;
        };
        self.beat -= seconds;
        // A Room the Host has changed past what a beat can say is listed afresh.
        if self.listed.is_some() && self.told.as_ref() != Some(&listing.fixed()) {
            self.close();
            self.beat = 0.0;
            changed = true;
        }
        match self.listed.clone() {
            Some(listed) => {
                let changed_status = self.last_status.as_ref() != Some(&listing.status);
                if self.beat <= 0.0 || changed_status {
                    self.beat = LOBBY_BEAT_SECS;
                    self.last_status = Some(listing.status.clone());
                    self.ask(self.api.beat(&listed, listing.status.clone()), |()| Heard::Beat);
                }
            }
            None if !self.listing && self.beat <= 0.0 => {
                // If the lobby can't be reached it is tried again, but not every frame.
                self.beat = LOBBY_RETRY_SECS;
                self.listing = true;
                self.told = Some(listing.fixed());
                self.last_status = Some(listing.status.clone());
                let name = fit(&listing.host, MAX_NAME);
                self.ask(
                    self.api.register(Register {
                        game: GAME.into(),
                        protocol: PROTOCOL,
                        name: name.clone(),
                        host: name,
                        endpoint: listing.endpoint.clone(),
                        locked: listing.locked,
                        max: MAX_PLAYERS,
                        status: listing.status.clone(),
                        unlisted: listing.unlisted,
                    }),
                    Heard::Registered,
                );
            }
            None => {}
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A lobby in memory that remembers what it was asked.
    #[derive(Default)]
    struct Fake {
        registered: Mutex<Vec<Register>>,
        beats: Mutex<Vec<Status>>,
        closed: AtomicU32,
        rooms: Mutex<Vec<(String, Listed)>>,
        /// The lobby is down: everything fails.
        down: Mutex<bool>,
        /// The next beat is answered "not found".
        forget: Mutex<bool>,
    }

    impl Fake {
        fn room(code: &str, host: &str, locked: bool) -> (String, Listed) {
            (
                code.to_string(),
                Listed {
                    id: code.to_lowercase(),
                    name: host.into(),
                    host: host.into(),
                    endpoint: format!("endpoint-{code}"),
                    locked,
                    max: 4,
                    status: Status {
                        players: 2,
                        circuit: status_line(MapId::Greybox, Mode::Gauntlet, 0),
                        racing: false,
                    },
                },
            )
        }

        fn is_down(&self) -> bool {
            *self.down.lock().unwrap()
        }
    }

    fn done<T: Send + 'static>(result: Result<T, LobbyError>) -> Reply<T> {
        Box::pin(async move { result })
    }

    impl LobbyApi for Arc<Fake> {
        fn list(&self) -> Reply<Vec<Listed>> {
            if self.is_down() {
                return done(Err(LobbyError::Unreached("down".into())));
            }
            done(Ok(self
                .rooms
                .lock()
                .unwrap()
                .iter()
                .map(|(_, room)| room.clone())
                .collect()))
        }

        fn find(&self, code: &str) -> Reply<Option<Listed>> {
            done(Ok(self
                .rooms
                .lock()
                .unwrap()
                .iter()
                .find(|(c, _)| c.eq_ignore_ascii_case(code.trim()))
                .map(|(_, room)| room.clone())))
        }

        fn register(&self, ask: Register) -> Reply<Registered> {
            if self.is_down() {
                return done(Err(LobbyError::Unreached("down".into())));
            }
            let mut registered = self.registered.lock().unwrap();
            registered.push(ask);
            let n = registered.len();
            done(Ok(Registered {
                id: format!("id{n}"),
                token: "secret".into(),
                code: format!("CODE{n}"),
            }))
        }

        fn beat(&self, _: &Registered, status: Status) -> Reply<()> {
            if std::mem::take(&mut *self.forget.lock().unwrap()) {
                return done(Err(LobbyError::Refused(404)));
            }
            self.beats.lock().unwrap().push(status);
            done(Ok(()))
        }

        fn close(&self, _: &Registered) -> Reply<()> {
            self.closed.fetch_add(1, Ordering::SeqCst);
            done(Ok(()))
        }
    }

    fn listing(players: u8) -> Listing {
        Listing {
            host: "Hostess".into(),
            endpoint: "me".into(),
            locked: false,
            unlisted: false,
            status: Status {
                players,
                circuit: status_line(MapId::Greybox, Mode::Classic, 0),
                racing: false,
            },
        }
    }

    /// Ticks until `done`, giving the network's threads time to answer.
    fn until(client: &mut LobbyClient, listing: Option<&Listing>, done: impl Fn(&LobbyClient) -> bool) {
        for _ in 0..400 {
            client.tick(0.0, listing);
            if done(client) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the lobby never answered");
    }

    fn client() -> (Arc<Fake>, LobbyClient) {
        let fake = Arc::new(Fake::default());
        (fake.clone(), LobbyClient::new(Arc::new(fake)))
    }

    #[test]
    fn a_room_registers_under_the_zombies_key_and_is_given_its_code() {
        let (fake, mut lobby) = client();
        let room = listing(1);
        until(&mut lobby, Some(&room), |l| l.code().is_some());
        assert_eq!(lobby.code(), Some("CODE1"));
        let registered = fake.registered.lock().unwrap();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].game, "zombies");
        assert_eq!(registered[0].max, 4);
        assert_eq!(registered[0].host, "Hostess");
        assert_eq!(registered[0].status.circuit, "Greybox · Classic");
        assert!(!registered[0].unlisted);
    }

    #[test]
    fn a_room_is_beaten_every_ten_seconds_and_not_before() {
        let (fake, mut lobby) = client();
        let room = listing(1);
        until(&mut lobby, Some(&room), |l| l.code().is_some());
        for _ in 0..9 {
            lobby.tick(1.0, Some(&room));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(fake.beats.lock().unwrap().is_empty());
        lobby.tick(1.0, Some(&room));
        until(&mut lobby, Some(&room), |_| !fake.beats.lock().unwrap().is_empty());
        assert_eq!(fake.beats.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_change_of_players_is_told_at_once() {
        let (fake, mut lobby) = client();
        until(&mut lobby, Some(&listing(1)), |l| l.code().is_some());
        until(&mut lobby, Some(&listing(2)), |_| !fake.beats.lock().unwrap().is_empty());
        assert_eq!(fake.beats.lock().unwrap()[0].players, 2);
        assert_eq!(fake.registered.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_change_a_beat_cannot_carry_lists_the_room_afresh() {
        let (fake, mut lobby) = client();
        until(&mut lobby, Some(&listing(1)), |l| l.code().is_some());
        let locked = Listing {
            locked: true,
            ..listing(1)
        };
        until(&mut lobby, Some(&locked), |l| l.code() == Some("CODE2"));
        assert_eq!(fake.closed.load(Ordering::SeqCst), 1);
        assert!(fake.registered.lock().unwrap()[1].locked);
    }

    #[test]
    fn no_listing_takes_the_room_off() {
        let (fake, mut lobby) = client();
        until(&mut lobby, Some(&listing(1)), |l| l.code().is_some());
        lobby.tick(0.0, None);
        assert!(!lobby.is_listed());
        until(&mut lobby, None, |_| fake.closed.load(Ordering::SeqCst) == 1);
        // And nothing registers while there is nothing to list.
        std::thread::sleep(std::time::Duration::from_millis(50));
        lobby.tick(5.0, None);
        assert_eq!(fake.registered.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_lobby_that_forgot_the_room_is_told_of_it_again() {
        let (fake, mut lobby) = client();
        until(&mut lobby, Some(&listing(1)), |l| l.code().is_some());
        *fake.forget.lock().unwrap() = true;
        lobby.tick(LOBBY_BEAT_SECS, Some(&listing(1)));
        until(&mut lobby, Some(&listing(1)), |l| l.code() == Some("CODE2"));
        assert_eq!(fake.registered.lock().unwrap().len(), 2);
    }

    #[test]
    fn an_unreachable_lobby_is_retried_and_does_not_stop_the_game() {
        let (fake, mut lobby) = client();
        *fake.down.lock().unwrap() = true;
        until(&mut lobby, Some(&listing(1)), |l| l.trouble.is_some());
        assert!(lobby.code().is_none());
        *fake.down.lock().unwrap() = false;
        // Not asked again at once, but after the retry wait.
        lobby.tick(LOBBY_RETRY_SECS, Some(&listing(1)));
        until(&mut lobby, Some(&listing(1)), |l| l.code().is_some());
        assert!(lobby.trouble.is_none());
    }

    #[test]
    fn the_list_and_a_code_find_rooms() {
        let (fake, mut lobby) = client();
        *fake.rooms.lock().unwrap() = vec![
            Fake::room("ABCDE", "Ada", false),
            Fake::room("FGHJK", "Bo", true),
        ];
        lobby.list();
        until(&mut lobby, None, |l| !l.sessions.is_empty());
        assert_eq!(lobby.sessions.len(), 2);
        assert!(lobby.sessions[1].locked);

        lobby.find(" fghjk ");
        until(&mut lobby, None, |l| l.found.is_some());
        assert_eq!(lobby.found.as_ref().unwrap().as_ref().unwrap().host, "Bo");
        lobby.find("ZZZZZ");
        until(&mut lobby, None, |l| l.found.is_some());
        assert_eq!(lobby.found, Some(None));
    }

    #[test]
    fn the_status_line_round_trips_and_names_fit_the_lobby() {
        let line = status_line(MapId::Greybox, Mode::Gauntlet, 7);
        assert_eq!(line, "Greybox · Gauntlet · Heat 7");
        assert_eq!(read_status_line(&line), ("Greybox".into(), "Gauntlet".into(), 7));
        assert_eq!(read_status_line("Greybox · Classic").2, 0);
        assert!(line.len() <= MAX_NAME);
        let wide = "界".repeat(12);
        assert!(fit(&wide, MAX_NAME).len() <= MAX_NAME);
        assert_eq!(fit("Ada", MAX_NAME), "Ada");
    }
}
