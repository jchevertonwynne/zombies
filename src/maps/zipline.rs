//! Ziplines: a cable from a lookout down to a lower Zone, which a Player rides by pressing the Use
//! key at its top. A ride is a commitment: a straight line at a set pace that nothing steers,
//! stops or ends early, so it is part of the movement step and a client replaying its inputs rides
//! exactly as the Host does. Zombies never use one, and a line joins no Zones.

use bevy::prelude::*;

use crate::economy::buy::Focus;
use crate::economy::perk::Drinking;
use crate::maps::door::Door;
use crate::maps::map::MapId;
use crate::maps::zone::{ZoneId, ZoneSet};
use crate::players::last_stand::LastStand;
use crate::players::player::{LocalPlayer, Locomotion, Stance};
use crate::players::respawn::AwaitingRespawn;
use crate::state::GameState;
use crate::tuning::*;

/// A Zipline: a cable from a lookout in `zone` down to a lower Zone, `lands_in`. `from` is where a
/// Player's feet stand to grab it, `to` where their feet come down at the far end. The cable runs
/// `ZIPLINE_HANG` over the straight line between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zipline {
    pub zone: ZoneId,
    pub from: Vec3,
    pub to: Vec3,
    pub lands_in: ZoneId,
}

/// How far from a line's feet path a riding Player's feet can have strayed and still be on it. A
/// ride is kinematic, so this only has to forgive rounding.
const ON_LINE_SLACK: f32 = 1.0;

impl Zipline {
    /// How long the ride is, in metres.
    pub fn length(&self) -> f32 {
        self.from.distance(self.to)
    }

    /// The unit vector from `from` to `to`.
    pub fn direction(&self) -> Vec3 {
        (self.to - self.from).normalize_or_zero()
    }

    /// The way a rider faces on arrival: along the line, level.
    pub fn heading(&self) -> Vec3 {
        Vec3::new(self.to.x - self.from.x, 0.0, self.to.z - self.from.z).normalize_or_zero()
    }

    /// Whether feet at `feet` are near enough to the start to grab the handle.
    pub fn in_reach(&self, feet: Vec3) -> bool {
        feet.distance(self.from) <= ZIPLINE_GRAB_RADIUS
    }

    /// Where the feet are `travelled` metres along the ride.
    pub fn at(&self, travelled: f32) -> Vec3 {
        self.from + self.direction() * travelled.clamp(0.0, self.length())
    }

    /// The two ends of the cable: the feet line lifted by `ZIPLINE_HANG`.
    pub fn cable(&self) -> (Vec3, Vec3) {
        (self.from + Vec3::Y * ZIPLINE_HANG, self.to + Vec3::Y * ZIPLINE_HANG)
    }

    /// How far feet at `feet` are from the feet line, and how far along it the nearest point is.
    pub fn along(&self, feet: Vec3) -> (f32, f32) {
        let travelled = (feet - self.from).dot(self.direction()).clamp(0.0, self.length());
        (feet.distance(self.at(travelled)), travelled)
    }

    /// Whether feet at `feet` are on this line's path: how a riding Player's line is found again
    /// from where they are.
    pub fn carries(&self, feet: Vec3) -> bool {
        self.along(feet).0 <= ON_LINE_SLACK
    }
}

/// The Zones that are open: the start Zone and every Zone joined to an open one by a join that
/// never closes or a Door that is open. A Zipline only works once the Zone it lands in is here.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenZones(pub ZoneSet);

/// The open Zones of `map`, given its open Doors' joins.
pub fn open_zones(map: MapId, open_joins: impl IntoIterator<Item = (ZoneId, ZoneId)>) -> ZoneSet {
    let layout = map.layout();
    let joins: Vec<(ZoneId, ZoneId)> = layout.fixed_joins.iter().copied().chain(open_joins).collect();
    let mut open = ZoneSet::default();
    open.insert(layout.start_zone());
    loop {
        let before = open;
        for &(a, b) in &joins {
            if open.contains(a) {
                open.insert(b);
            }
            if open.contains(b) {
                open.insert(a);
            }
        }
        if open == before {
            return open;
        }
    }
}

/// The Ziplines on `map` that work now: those whose `lands_in` is open.
pub fn working(map: MapId, open: ZoneSet) -> Vec<Zipline> {
    map.layout().ziplines.iter().filter(|line| open.contains(line.lands_in)).copied().collect()
}

/// The line, among `lines`, a Player standing at `feet` is in reach of, nearest first.
pub fn grabbable(lines: &[Zipline], feet: Vec3) -> Option<&Zipline> {
    lines
        .iter()
        .filter(|line| line.in_reach(feet))
        .min_by(|a, b| feet.distance(a.from).total_cmp(&feet.distance(b.from)))
}

/// The line, among `lines`, a riding Player at `feet` is on.
pub fn ridden(lines: &[Zipline], feet: Vec3) -> Option<&Zipline> {
    lines.iter().filter(|line| line.carries(feet)).min_by(|a, b| a.along(feet).0.total_cmp(&b.along(feet).0))
}

/// How a ride goes on for `dt` seconds from `feet` at `speed`: where the feet are, how fast they
/// go, and whether they have arrived at `to`. The pace starts at `ZIPLINE_START_SPEED`, speeds up
/// at `ZIPLINE_ACCEL` to `ZIPLINE_TOP_SPEED` and stays there, whatever the slope.
pub fn advance(line: &Zipline, feet: Vec3, speed: f32, dt: f32) -> (Vec3, f32, bool) {
    let faster = (speed + ZIPLINE_ACCEL * dt).min(ZIPLINE_TOP_SPEED);
    let step = (speed + faster) / 2.0 * dt;
    let left = feet.distance(line.to);
    if left <= step {
        (line.to, faster, true)
    } else {
        (feet + line.direction() * step, faster, false)
    }
}

/// Seconds a whole ride on `line` takes.
#[cfg(test)]
pub fn ride_secs(line: &Zipline) -> f32 {
    let ramp = (ZIPLINE_TOP_SPEED - ZIPLINE_START_SPEED) / ZIPLINE_ACCEL;
    let ramp_length = (ZIPLINE_START_SPEED + ZIPLINE_TOP_SPEED) / 2.0 * ramp;
    if line.length() <= ramp_length {
        // Still speeding up when it arrives: solve s = v0 t + a t^2 / 2.
        let (v, a) = (ZIPLINE_START_SPEED, ZIPLINE_ACCEL);
        (-v + (v * v + 2.0 * a * line.length()).sqrt()) / a
    } else {
        ramp + (line.length() - ramp_length) / ZIPLINE_TOP_SPEED
    }
}

/// What the Prompt reads at a working Zipline's top.
pub const ZIPLINE_PROMPT: &str = "Press F to ride the Zipline";

/// Whether the local Player has a Zipline's Prompt showing: a working line's top is in reach, they
/// can grab it, and no Buyable has the Use key. The Use key then grabs.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZiplineFocus(pub bool);

pub struct ZiplinePlugin;

impl Plugin for ZiplinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenZones>().init_resource::<ZiplineFocus>().add_systems(
            Update,
            (update_open_zones, focus_zipline.after(crate::economy::buy::focus)).chain().run_if(in_state(GameState::Playing)),
        );
    }
}

/// Whether a Player in this state is offered a Zipline: up, on the ground, Standing or Crouched, not
/// drinking, and not already riding.
pub fn can_ride(locomotion: &Locomotion, last_stand: bool, drinking: bool) -> bool {
    !last_stand
        && !locomotion.down
        && !drinking
        && !locomotion.riding()
        && locomotion.grounded()
        && !locomotion.diving()
        && locomotion.stance != Stance::Prone
}

pub(crate) fn focus_zipline(
    map: Option<Res<MapId>>,
    open: Res<OpenZones>,
    buyable: Res<Focus>,
    player: Single<(&Transform, &Locomotion, Has<LastStand>, Has<AwaitingRespawn>, Has<Drinking>), With<LocalPlayer>>,
    mut focus: ResMut<ZiplineFocus>,
) {
    let (transform, locomotion, last_stand, awaiting, drinking) = *player;
    let offered = buyable.0.is_none()
        && !awaiting
        && can_ride(locomotion, last_stand, drinking)
        && map.is_some_and(|map| {
            let feet = transform.translation - Vec3::Y * locomotion.shape().half_height();
            grabbable(&working(*map, open.0), feet).is_some()
        });
    if focus.0 != offered {
        focus.0 = offered;
    }
}

/// Keeps `OpenZones` to how the Doors stand.
fn update_open_zones(doors: Query<&Door>, map: Option<Res<MapId>>, mut open: ResMut<OpenZones>) {
    let Some(map) = map else { return };
    if map.layout().ziplines.is_empty() {
        return;
    }
    let set = open_zones(*map, doors.iter().filter(|door| door.open).filter_map(|door| door.joins));
    if open.0 != set {
        open.0 = set;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Zipline {
        Zipline { zone: ZoneId::Lighthouse, from: Vec3::new(0.0, 20.0, 0.0), to: Vec3::new(30.0, 4.0, 40.0), lands_in: ZoneId::Shingle }
    }

    /// Rides `line` in `dt` steps from the start, returning seconds taken.
    fn ride(line: &Zipline, dt: f32) -> f32 {
        let (mut feet, mut speed, mut secs) = (line.from, ZIPLINE_START_SPEED, 0.0);
        loop {
            let (next, faster, arrived) = advance(line, feet, speed, dt);
            (feet, speed, secs) = (next, faster, secs + dt);
            if arrived {
                assert_eq!(feet, line.to);
                return secs;
            }
            assert!(secs < 60.0, "never arrives");
        }
    }

    #[test]
    fn a_ride_arrives_in_the_time_the_pace_gives() {
        let line = line();
        assert!(line.length() > 50.0);
        let ramp = (ZIPLINE_TOP_SPEED - ZIPLINE_START_SPEED) / ZIPLINE_ACCEL;
        let expected = ramp + (line.length() - (ZIPLINE_START_SPEED + ZIPLINE_TOP_SPEED) / 2.0 * ramp) / ZIPLINE_TOP_SPEED;
        assert!((ride_secs(&line) - expected).abs() < 1e-4);
        for dt in [1.0 / 60.0, 1.0 / 30.0, 0.1] {
            let secs = ride(&line, dt);
            assert!((secs - expected).abs() <= dt + 1e-3, "{secs} s at {dt}, expected {expected}");
        }
    }

    #[test]
    fn a_short_ride_is_still_speeding_up_when_it_arrives() {
        let short = Zipline { to: Vec3::new(0.0, 12.0, 8.0), ..line() };
        assert!(short.length() < 12.0);
        let secs = ride_secs(&short);
        assert!((ride(&short, 1.0 / 60.0) - secs).abs() < 0.03);
        assert!(secs < short.length() / ZIPLINE_START_SPEED);
    }

    #[test]
    fn the_pace_tops_out_and_slope_does_not_matter() {
        let line = line();
        let (_, speed, _) = advance(&line, line.from, ZIPLINE_TOP_SPEED, 1.0);
        assert_eq!(speed, ZIPLINE_TOP_SPEED);
        let (feet, speed, arrived) = advance(&line, line.from, ZIPLINE_START_SPEED, 0.5);
        assert!(!arrived);
        assert!((feet.distance(line.from) - (ZIPLINE_START_SPEED + speed) / 2.0 * 0.5).abs() < 1e-4);
        assert!((feet - line.from).normalize().dot(line.direction()) > 0.9999);
    }

    #[test]
    fn a_line_is_grabbed_only_within_reach_of_its_start() {
        let line = line();
        assert!(line.in_reach(line.from + Vec3::X * (ZIPLINE_GRAB_RADIUS - 0.05)));
        assert!(!line.in_reach(line.from + Vec3::X * (ZIPLINE_GRAB_RADIUS + 0.05)));
        assert!(!line.in_reach(line.to));
        let lines = [line];
        assert!(grabbable(&lines, line.from).is_some());
        assert!(grabbable(&lines, line.from + Vec3::Y * 3.0).is_none());
    }

    #[test]
    fn a_riding_player_is_found_on_their_line() {
        let line = line();
        let lines = [line];
        assert!(ridden(&lines, line.at(10.0)).is_some());
        assert!(ridden(&lines, line.at(10.0) + Vec3::Y * 3.0).is_none());
    }

    #[test]
    fn the_cable_hangs_over_the_feet_line() {
        let line = line();
        let (top, bottom) = line.cable();
        assert_eq!(top, line.from + Vec3::Y * ZIPLINE_HANG);
        assert_eq!(bottom, line.to + Vec3::Y * ZIPLINE_HANG);
    }

    #[test]
    fn open_zones_follow_open_joins_from_the_start() {
        let cove = MapId::Cove;
        assert_eq!(cove.layout().start_zone(), ZoneId::Strand);
        let none = open_zones(cove, []);
        assert!(none.contains(ZoneId::Strand) && !none.contains(ZoneId::Shingle));
        let some = open_zones(cove, [(ZoneId::Strand, ZoneId::Shingle), (ZoneId::Shingle, ZoneId::SeaCave)]);
        assert!(some.contains(ZoneId::Shingle) && some.contains(ZoneId::SeaCave) && !some.contains(ZoneId::Boathouse));
        // A join works from either end.
        assert!(open_zones(cove, [(ZoneId::Boathouse, ZoneId::Strand)]).contains(ZoneId::Boathouse));
    }

    #[test]
    fn a_line_works_once_the_zone_it_lands_in_is_open() {
        let lands = |open: ZoneSet| working(MapId::Cove, open).iter().map(|line| line.lands_in).collect::<Vec<_>>();
        assert_eq!(MapId::Cove.layout().ziplines.len(), 3);
        assert!(lands(open_zones(MapId::Cove, [])).is_empty());
        let mut some = open_zones(MapId::Cove, []);
        some.insert(ZoneId::Piazza);
        assert_eq!(lands(some), vec![ZoneId::Piazza]);
        let every: ZoneSet = ZoneId::ALL.into_iter().collect();
        assert_eq!(lands(every).len(), 3);
        assert!(working(MapId::Greybox, every).is_empty());
    }
}
