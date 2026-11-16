//! The Boss's health bar on a client. Everything else about a Boss is inferred (its name, phase
//! marks and warning from the Round) or already on the wire (its size and helmet in the snapshot,
//! its howl and burst ring as cues and effects); only its health is told, as a byte.

use bevy::prelude::*;

use super::{Heard, Mirror, SeatReturned, Tell, WorldEvent};
use crate::run::gauntlet::{BossView, boss_of};
use crate::run::round::Round;

/// The most often a Boss's health is told, in a second.
pub const BOSS_HEALTH_TELL_PER_SEC: f32 = 10.0;

/// A Boss's health as `WorldEvent::BossHealth` says it: 1 to 255 in 255ths while it lives (never 0,
/// so a sliver of health still shows), and 0 for no Boss.
pub fn boss_health_byte(view: &BossView) -> u8 {
    if view.kind.is_none() {
        return 0;
    }
    (view.share.clamp(0.0, 1.0) * 255.0).ceil().clamp(1.0, 255.0) as u8
}

/// Tells everyone the Boss's health when its byte changes, at most `BOSS_HEALTH_TELL_PER_SEC` times
/// a second.
pub fn tell_boss_health(mut tell: Tell, mut mirror: ResMut<Mirror>, view: Option<Res<BossView>>, time: Res<Time>) {
    let Some(view) = view else { return };
    let now = boss_health_byte(&view);
    let told_at = time.elapsed_secs();
    if mirror.boss_health == now || mirror.boss_told.is_some_and(|at| told_at - at < 1.0 / BOSS_HEALTH_TELL_PER_SEC) {
        return;
    }
    mirror.boss_health = now;
    mirror.boss_told = Some(told_at);
    tell.all(&WorldEvent::BossHealth(now));
}

/// Tells a Player who has come back the Boss's health, if there is a Boss.
pub fn catch_up_boss(mut tell: Tell, mut returned: MessageReader<SeatReturned>, mirror: Res<Mirror>) {
    for &SeatReturned { seat } in returned.read() {
        let Some(&(peer, _)) = tell.roster.0.iter().find(|&&(_, held)| held == seat.0) else { continue };
        if mirror.boss_health != 0 {
            tell.to(peer, &WorldEvent::BossHealth(mirror.boss_health));
        }
    }
}

/// Fills the Boss HUD's view from what the Host told: which Boss it is comes from the Round.
pub fn hear_boss_health(mut heard: MessageReader<Heard>, view: Option<ResMut<BossView>>, round: Option<Res<Round>>) {
    let Some(mut view) = view else { return };
    for Heard(event) in heard.read() {
        let WorldEvent::BossHealth(byte) = *event else { continue };
        let kind = if byte == 0 { None } else { round.as_ref().and_then(|round| boss_of(round.number)) };
        view.set_if_neq(BossView { kind, share: f32::from(byte) / 255.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::Run;
    use super::*;
    use crate::run::gauntlet::arc::BossKind;
    use crate::run::round::Round;

    fn view(kind: Option<BossKind>, share: f32) -> BossView {
        BossView { kind, share }
    }

    #[test]
    fn the_boss_health_byte_is_never_zero_while_the_boss_lives() {
        assert_eq!(boss_health_byte(&view(None, 0.7)), 0);
        assert_eq!(boss_health_byte(&view(Some(BossKind::Bloater), 1.0)), 255);
        assert_eq!(boss_health_byte(&view(Some(BossKind::Bloater), 0.0)), 1);
        assert_eq!(boss_health_byte(&view(Some(BossKind::Warden), 0.5)), 128);
    }

    fn run(clients: u8) -> Run {
        Run::with(clients, &|app| {
            app.init_resource::<BossView>();
        })
    }

    fn boss_round(run: &mut Run, clients: usize) {
        for on in 0..=clients {
            run.world(on).insert_resource(Round::shown(20, None));
        }
    }

    #[test]
    fn the_boss_health_reaches_every_client_and_goes_when_the_boss_does() {
        let mut run = run(2);
        boss_round(&mut run, 2);
        *run.world(0).resource_mut::<BossView>() = view(Some(BossKind::PackAlpha), 0.5);
        run.step(10);
        for on in 1..3 {
            let seen = *run.world(on).resource::<BossView>();
            assert_eq!(seen.kind, Some(BossKind::PackAlpha), "game {on} infers which Boss from the Round");
            assert!((seen.share - 0.5).abs() < 0.01, "game {on}: {}", seen.share);
        }
        *run.world(0).resource_mut::<BossView>() = BossView::default();
        run.step(10);
        for on in 1..3 {
            assert_eq!(run.world(on).resource::<BossView>().kind, None, "game {on}");
        }
    }

    #[test]
    fn the_boss_health_is_told_at_most_ten_times_a_second() {
        let mut run = run(1);
        boss_round(&mut run, 1);
        *run.world(0).resource_mut::<BossView>() = view(Some(BossKind::Bloater), 1.0);
        let mut told = 0;
        let mut last = 255u8;
        // A second of health falling every tick.
        for tick in 0..crate::tuning::FIXED_HZ as usize {
            run.world(0).resource_mut::<BossView>().share = 1.0 - tick as f32 / 100.0;
            run.step(1);
            let byte = run.world(0).resource::<Mirror>().boss_health;
            if byte != last {
                told += 1;
                last = byte;
            }
        }
        assert!((5..=11).contains(&told), "{told} tellings in a second");
    }
}
