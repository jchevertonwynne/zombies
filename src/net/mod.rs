//! Co-op's network layer: how a Host and its Players talk. Solo builds none of it.
//!
//! The Room (`room`) gathers a Host and its Players; when the Host starts the Run, the same `Link`
//! goes to `play`, which carries the Run (see `party`).

#![allow(dead_code, reason = "the Co-op systems that use the network layer are built next")]

pub mod away;
pub mod fx;
pub mod hits;
pub mod link;
pub mod lobby;
pub mod play;
pub mod protocol;
pub mod remote;
pub mod room;
pub mod state;
pub mod transport;
pub mod world;
pub mod zombies;

#[cfg(test)]
mod movement;

#[cfg(test)]
mod tests {
    use super::link::{Event, Hub, Link};
    use super::protocol::{
        DATAGRAM, HOST, PROTOCOL, Refusal, ToHost, ToPlayer, answer, decode, encode,
    };

    /// Lets the network run until a link has something to say.
    fn next(hub: &Hub, link: &mut impl Link) -> Event {
        for _ in 0..100 {
            if let Some(event) = link.poll() {
                return event;
            }
            hub.step();
        }
        panic!("nothing arrived");
    }

    /// A Player joins and says `Hello`; returns what the Host answered.
    fn join_with(version: u32) -> ToPlayer {
        let hub = Hub::new(2, 0.0);
        let (mut host, mut player) = (hub.host(), hub.join());
        assert_eq!(next(&hub, &mut host), Event::Joined(player.peer()));
        assert_eq!(next(&hub, &mut player), Event::Joined(HOST));
        player.send(
            HOST,
            encode(&ToHost::Hello {
                version,
                back: None,
                name: "Ada".into(),
                password: String::new(),
                unlocks: Default::default(),
            }),
        );
        let Event::Message(peer, bytes) = next(&hub, &mut host) else {
            panic!("expected the Hello");
        };
        host.send(peer, encode(&answer(&decode(&bytes).unwrap(), 1)));
        let Event::Message(from, bytes) = next(&hub, &mut player) else {
            panic!("expected an answer");
        };
        assert_eq!(from, HOST);
        decode(&bytes).unwrap()
    }

    #[test]
    fn a_player_joins_and_exchanges_a_message_and_a_datagram() {
        let ToPlayer::Welcome { token, seat } = join_with(PROTOCOL) else {
            panic!("expected a Welcome");
        };
        assert_ne!(token, 0);
        assert_eq!(seat, 1);

        let hub = Hub::new(1, 0.0);
        let (mut host, mut player) = (hub.host(), hub.join());
        player.datagram(HOST, vec![7; DATAGRAM]);
        host.datagram(player.peer(), vec![8; 60]);
        assert_eq!(next(&hub, &mut host), Event::Joined(player.peer()));
        assert_eq!(
            next(&hub, &mut host),
            Event::Datagram(player.peer(), vec![7; DATAGRAM])
        );
        assert_eq!(next(&hub, &mut player), Event::Joined(HOST));
        assert_eq!(next(&hub, &mut player), Event::Datagram(HOST, vec![8; 60]));
    }

    #[test]
    fn a_player_on_another_version_is_refused_with_its_reason() {
        let ToPlayer::Refused(why) = join_with(PROTOCOL + 1) else {
            panic!("expected a refusal");
        };
        assert_eq!(why, Refusal::Version);
        assert_eq!(why.reason(), "Host is on a different version");
    }
}
