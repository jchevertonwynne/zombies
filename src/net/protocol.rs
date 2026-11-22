//! What a Host and its Players say to each other.
//!
//! Two kinds of message. Those that must arrive, and in order, go as `ToHost` and
//! `ToPlayer`: joining, who sits where. Those that are out of date a moment later
//! go as datagrams, and a lost one is simply made up for by the next. Which type a
//! message is read as follows from the way it came and which end reads it, not from
//! anything written on it.

use rand::RngExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::hits::{Burst, Hit, Launched, Throw};
use super::world::{Request, WorldEvent};
use crate::maps::map::MapId;
use crate::run::dev::DevMode;
use crate::run::mode::Mode;
use crate::tuning::{PLAYER_NAME_MAX_CHARS, ROOM_SEATS};

/// Who a message is from or for, as the Host numbers them. The Host is `HOST`.
pub type Peer = u32;
pub const HOST: Peer = 0;

/// Which of a Run's places a Player holds, counted from the Host's, which is 0.
pub type Seat = u8;

/// What version of the wire format this game speaks. The Host refuses a Player on
/// another, and it is raised whenever the format changes (along with the ALPN).
pub const PROTOCOL: u32 = 15;
/// The most a datagram may be, in bytes: what the real link is tried with, and under
/// what it carries. One bigger is not sent at all.
pub const DATAGRAM: usize = 1000;

#[derive(thiserror::Error, Debug)]
pub enum WireError {
    #[error("a message that could not be read: {0}")]
    Unreadable(#[from] postcard::Error),
}

pub fn encode<T: Serialize>(message: &T) -> Vec<u8> {
    // Nothing sent has a map with keys that can't be written or a length not known.
    postcard::to_stdvec(message).expect("a message that can be written")
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, WireError> {
    Ok(postcard::from_bytes(bytes)?)
}

/// Why a Host won't have a Player.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The two games don't speak the same `PROTOCOL`.
    Version,
    /// Every Seat in the Room is taken.
    Full,
    /// The Room has a password and it wasn't the one given.
    WrongPassword,
    /// The Run has started, and the Room takes no one new.
    Started,
    /// The Host holds no Seat for the token given, or for a Player who didn't give one.
    NoSeat,
}

impl Refusal {
    /// What to tell the Player who was refused.
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::Version => "Host is on a different version",
            Refusal::Full => "Room is full",
            Refusal::WrongPassword => "Wrong password",
            Refusal::Started => "Run already started",
            Refusal::NoSeat => "No Seat held for you",
        }
    }
}

/// What a Player brings from their own Armory: the Boons and Pacts they have Unlocked, and
/// their Upgrade Tracks' Ranks and toggles. The Host draws that Player's Offers from it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Unlocks {
    /// Stable IDs of the Boons and Pacts bought in the Armory.
    pub unlocked: Vec<String>,
    pub tracks: Vec<TrackState>,
    /// The stable ID of the Kit they start the Run with; empty is Standard.
    pub kit: String,
}

/// One Upgrade Track's Ranks and whether it is switched on.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TrackState {
    /// The Track's stable ID.
    pub id: String,
    pub ranks: u32,
    pub on: bool,
}

/// The Room as every Player sees it, sent to all of them whenever it changes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoomView {
    /// What others type to find the Room. Empty until the lobby has given one.
    pub code: String,
    pub mode: Mode,
    pub map: MapId,
    /// The Host's Degrees, one per Condition in `Condition::ALL` order. Every machine's Run
    /// takes these, not its own save's.
    pub heat: [u8; 8],
    /// Who may use Cheats in the Run.
    pub dev: DevMode,
    /// Whether the Room has a password.
    pub locked: bool,
    /// Who sits in each Seat, the Host's first.
    pub seats: [Option<SeatView>; ROOM_SEATS],
}

impl RoomView {
    /// The Host's Heat: every Degree is worth 1. This is what the lobby lists.
    pub fn heat_total(&self) -> u8 {
        self.heat.iter().sum()
    }
}

/// One Seat's row in the Room.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SeatView {
    pub name: String,
    pub ready: bool,
}

/// One Player's end-of-Run line: what the Host counted for them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Score {
    pub name: String,
    pub seat: Seat,
    pub kills: u32,
    pub downs: u32,
    pub revives: u32,
    /// The Points they earned over the Run, not the balance they hold.
    pub points_earned: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ToHost {
    /// The first thing a Player says.
    Hello {
        version: u32,
        /// What the Host gave them when they were here before, if they were: by it
        /// they are known for who they were, and take their own Seat again.
        back: Option<u64>,
        /// Their Name, which the Host cuts to `PLAYER_NAME_MAX_CHARS`.
        name: String,
        /// The Room's password, or nothing if it has none.
        password: String,
        unlocks: Unlocks,
    },
    /// Ready or not. Taken back whenever the Room changes.
    Ready(bool),
    /// Their Armory has changed. Taken as no longer Ready.
    Unlocks(Unlocks),
    /// They have left the Run for good, and give up their Seat.
    LeftRun,
    /// Their bullet or Knife landed on a Zombie.
    Hit(Hit),
    /// They threw, planted or fired something the Host is to simulate.
    Launch(Throw),
    /// They used something in the world, or switched Weapon.
    Request(Request),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ToPlayer {
    Welcome {
        /// What to say on coming back, should the way here break (`ToHost::Hello`).
        /// Never 0.
        token: u64,
        /// The Seat the Player has for the whole Run.
        seat: Seat,
    },
    Refused(Refusal),
    /// The Room as it now is.
    Room(RoomView),
    /// The Host has sent this Player away.
    Kicked,
    /// The Host has left, and the Room with them.
    Closed,
    /// The Host has started the Run, with the Room as it stood.
    Start(RoomView),
    /// Every Player is down: the Run is over.
    RunOver,
    /// How each Player did, said just before `RunOver`.
    Scoreboard(Vec<Score>),
    /// The Host is back in the Room, and the party with it.
    BackToRoom,
    /// Something was thrown, planted or fired by someone in the Run.
    Launched(Launched),
    /// Something went off.
    Burst(Burst),
    /// What the Host drew this frame that a client cannot make for itself: flashes, rings, arcs.
    Fx(Vec<super::fx::FxWire>),
    /// Something changed in the world, or in what a Player holds.
    Event(WorldEvent),
}

/// A Name as the Room has it: no control characters, at most `PLAYER_NAME_MAX_CHARS`
/// characters, and never empty.
pub fn clean_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(PLAYER_NAME_MAX_CHARS)
        .collect();
    let name = name.trim().to_string();
    if name.is_empty() {
        "Player".to_string()
    } else {
        name
    }
}

/// Something nobody else could guess, for a Player to be known again by. Never 0.
pub fn token() -> u64 {
    rand::rng().random_range(1..=u64::MAX)
}

/// What the Host answers a `Hello` with: a `Welcome` into `seat` with a new token, or
/// a refusal if the Player is on another version.
pub fn answer(hello: &ToHost, seat: Seat) -> ToPlayer {
    let ToHost::Hello { version, .. } = hello else {
        return ToPlayer::Refused(Refusal::Version);
    };
    if *version != PROTOCOL {
        return ToPlayer::Refused(Refusal::Version);
    }
    ToPlayer::Welcome {
        token: token(),
        seat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_survive_the_wire() {
        let hello = ToHost::Hello {
            version: PROTOCOL,
            back: Some(7),
            name: "Ada".into(),
            password: String::new(),
            unlocks: Unlocks::default(),
        };
        assert_eq!(decode::<ToHost>(&encode(&hello)).unwrap(), hello);
        let welcome = ToPlayer::Welcome { token: 9, seat: 2 };
        assert_eq!(decode::<ToPlayer>(&encode(&welcome)).unwrap(), welcome);
        assert!(decode::<ToPlayer>(&[0xff, 0xff, 0xff]).is_err());
    }

    #[test]
    fn a_name_is_cut_to_twelve_characters_and_never_empty() {
        assert_eq!(clean_name("  Ada  "), "Ada");
        assert_eq!(clean_name("abcdefghijklmnop"), "abcdefghijkl");
        assert_eq!(clean_name("é".repeat(20).as_str()).chars().count(), 12);
        assert_eq!(clean_name("a\nb"), "ab");
        assert_eq!(clean_name("   "), "Player");
    }

    #[test]
    fn each_refusal_has_its_reason() {
        assert_eq!(Refusal::Full.reason(), "Room is full");
        assert_eq!(Refusal::WrongPassword.reason(), "Wrong password");
        assert_eq!(Refusal::Started.reason(), "Run already started");
    }

    #[test]
    fn a_token_is_never_nothing() {
        assert!((0..1000).all(|_| token() != 0));
    }
}
