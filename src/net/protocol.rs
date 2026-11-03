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

/// Who a message is from or for, as the Host numbers them. The Host is `HOST`.
pub type Peer = u32;
pub const HOST: Peer = 0;

/// Which of a Run's places a Player holds, counted from the Host's, which is 0.
pub type Seat = u8;

/// What version of the wire format this game speaks. The Host refuses a Player on
/// another, and it is raised whenever the format changes (along with the ALPN).
pub const PROTOCOL: u32 = 1;
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
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    /// The two games don't speak the same `PROTOCOL`.
    Version,
}

impl Refusal {
    /// What to tell the Player who was refused.
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::Version => "Host is on a different version",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ToHost {
    /// The first thing a Player says.
    Hello {
        version: u32,
        /// What the Host gave them when they were here before, if they were: by it
        /// they are known for who they were, and take their own Seat again.
        back: Option<u64>,
    },
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
}

/// Something nobody else could guess, for a Player to be known again by. Never 0.
pub fn token() -> u64 {
    rand::rng().random_range(1..=u64::MAX)
}

/// What the Host answers a `Hello` with: a `Welcome` into `seat` with a new token, or
/// a refusal if the Player is on another version.
pub fn answer(hello: &ToHost, seat: Seat) -> ToPlayer {
    let ToHost::Hello { version, .. } = hello;
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
        };
        assert_eq!(decode::<ToHost>(&encode(&hello)).unwrap(), hello);
        let welcome = ToPlayer::Welcome { token: 9, seat: 2 };
        assert_eq!(decode::<ToPlayer>(&encode(&welcome)).unwrap(), welcome);
        assert!(decode::<ToPlayer>(&[0xff, 0xff, 0xff]).is_err());
    }

    #[test]
    fn a_token_is_never_nothing() {
        assert!((0..1000).all(|_| token() != 0));
    }
}
