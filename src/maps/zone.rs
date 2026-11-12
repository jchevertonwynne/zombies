//! Zones: which one the Player is in, and which are Active.

use bevy::prelude::*;

use crate::debug::DebugView;
use crate::maps::door::Door;
use crate::maps::map::{MapId, QUARRY_X, STOREY};
use crate::players::player::{LocalPlayer, Player};
use crate::players::respawn::AwaitingRespawn;
use crate::state::GameState;
use crate::tuning::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoneId {
    StartRoom,
    WestHall,
    Balcony,
    EastHall,
    DiningRoom,
    PowerRoom,
    Yard,
    /// The corridor south from the Dining Room to the Workshop.
    Passage,
    Workshop,
    /// Between the Workshop and the Generator Room, at the middle of the south wing.
    Storeroom,
    GeneratorRoom,
    /// The corridor south from the Power Room to the Generator Room.
    Tunnel,
    /// The Quarry's start: a brick pump house at the foot of the pit.
    PumpHouse,
    /// The open pit floor, with the sump pond and the Risers.
    QuarryFloor,
    /// The first terrace's open sidings, up the ramp from the pit floor.
    RailYard,
    /// The shed on the first terrace, round the stone crusher.
    CrusherShed,
    /// The engine house on the second terrace, round its winding drum.
    WindingHouse,
    /// The mine offices on the second terrace.
    Offices,
    /// The pithead tower: a yard at the second terrace's level and the deck above it.
    Headframe,
}

impl ZoneId {
    pub const ALL: [ZoneId; 19] = [
        ZoneId::StartRoom,
        ZoneId::WestHall,
        ZoneId::Balcony,
        ZoneId::EastHall,
        ZoneId::DiningRoom,
        ZoneId::PowerRoom,
        ZoneId::Yard,
        ZoneId::Passage,
        ZoneId::Workshop,
        ZoneId::Storeroom,
        ZoneId::GeneratorRoom,
        ZoneId::Tunnel,
        ZoneId::PumpHouse,
        ZoneId::QuarryFloor,
        ZoneId::RailYard,
        ZoneId::CrusherShed,
        ZoneId::WindingHouse,
        ZoneId::Offices,
        ZoneId::Headframe,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ZoneId::StartRoom => "Start Room",
            ZoneId::WestHall => "West Hall",
            ZoneId::Balcony => "Balcony",
            ZoneId::EastHall => "East Hall",
            ZoneId::DiningRoom => "Dining Room",
            ZoneId::PowerRoom => "Power Room",
            ZoneId::Yard => "Yard",
            ZoneId::Passage => "Passage",
            ZoneId::Workshop => "Workshop",
            ZoneId::Storeroom => "Storeroom",
            ZoneId::GeneratorRoom => "Generator Room",
            ZoneId::Tunnel => "Tunnel",
            ZoneId::PumpHouse => "Pump House",
            ZoneId::QuarryFloor => "Quarry Floor",
            ZoneId::RailYard => "Rail Yard",
            ZoneId::CrusherShed => "Crusher Shed",
            ZoneId::WindingHouse => "Winding House",
            ZoneId::Offices => "Offices",
            ZoneId::Headframe => "Headframe",
        }
    }

    /// The Map the Zone is part of.
    pub fn map(self) -> MapId {
        use ZoneId::*;
        match self {
            StartRoom | WestHall | Balcony | EastHall | DiningRoom | PowerRoom | Yard | Passage | Workshop | Storeroom | GeneratorRoom | Tunnel => MapId::Greybox,
            PumpHouse | QuarryFloor | RailYard | CrusherShed | WindingHouse | Offices | Headframe => MapId::Quarry,
        }
    }

    /// The height of the Zone's floor: the ground, a storey up for the Balcony, and a terrace's
    /// height in the Quarry.
    pub fn floor(self) -> f32 {
        use ZoneId::*;
        match self {
            StartRoom | WestHall | EastHall | DiningRoom | PowerRoom | Yard | Passage | Workshop | Storeroom | GeneratorRoom | Tunnel => 0.0,
            PumpHouse | QuarryFloor => 0.0,
            Balcony | RailYard | CrusherShed => STOREY,
            WindingHouse | Offices | Headframe => 2.0 * STOREY,
        }
    }

    /// The Zone's volume. Floors sit a little inside it, so a Player standing on one is inside.
    /// West Hall is two storeys tall because its ramp climbs to the Balcony, and a Zone with a
    /// sunken floor (the Yard's garden, the Tunnel's dip) reaches down into it. The Quarry's Zones
    /// stand at its own patch of the world, `QUARRY_X` along x, and a Zone a ramp or stairs climb
    /// out of is tall enough to hold a Player on them.
    pub fn volume(self) -> (Vec3, Vec3) {
        const LOW: f32 = -0.25;
        const UPPER: f32 = 3.75;
        const TOP: f32 = 8.0;
        const Q: f32 = QUARRY_X;
        let (x0, x1, y0, y1, z0, z1) = match self {
            ZoneId::StartRoom => (-14.0, 14.0, LOW, UPPER, 14.0, 26.0),
            ZoneId::WestHall => (-26.0, -14.0, LOW, TOP, -14.0, 26.0),
            ZoneId::Balcony => (-14.0, -10.0, UPPER, TOP, -14.0, 10.0),
            ZoneId::EastHall => (14.0, 26.0, LOW, UPPER, -14.0, 26.0),
            ZoneId::DiningRoom => (-26.0, 0.0, LOW, UPPER, -26.0, -14.0),
            ZoneId::PowerRoom => (0.0, 26.0, LOW, UPPER, -26.0, -14.0),
            ZoneId::Yard => (-14.0, 14.0, LOW, UPPER, -14.0, 14.0),
            ZoneId::Passage => (-13.5, -10.5, LOW, UPPER, -38.0, -26.0),
            ZoneId::Workshop => (-34.0, -4.0, LOW, UPPER, -50.0, -38.0),
            ZoneId::Storeroom => (-4.0, 4.0, LOW, UPPER, -50.0, -38.0),
            ZoneId::GeneratorRoom => (4.0, 34.0, LOW, UPPER, -50.0, -38.0),
            ZoneId::Tunnel => (12.5, 15.5, LOW, UPPER, -38.0, -26.0),
            ZoneId::PumpHouse => (Q - 8.0, Q + 8.0, LOW, UPPER, -30.0, -20.0),
            ZoneId::QuarryFloor => (Q - 20.0, Q + 20.0, LOW, TOP - 0.25, -20.0, 2.0),
            ZoneId::RailYard => (Q, Q + 20.0, UPPER, 3.0 * STOREY - 0.25, 2.0, 16.0),
            ZoneId::CrusherShed => (Q - 20.0, Q, UPPER, TOP - 0.25, 2.0, 16.0),
            ZoneId::WindingHouse => (Q, Q + 20.0, TOP - 0.25, 3.0 * STOREY - 0.25, 16.0, 28.0),
            ZoneId::Offices => (Q - 20.0, Q, TOP - 0.25, 3.0 * STOREY - 0.25, 16.0, 28.0),
            ZoneId::Headframe => (Q - 6.0, Q + 6.0, TOP - 0.25, 4.0 * STOREY, 28.0, 40.0),
        };
        let sunk = self.map().layout().sunken.iter().filter(|sunken| sunken.zone == self).map(|sunken| sunken.depth).fold(0.0, f32::max);
        (Vec3::new(x0, y0 - sunk, z0), Vec3::new(x1, y1, z1))
    }

    fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// A set of Zones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZoneSet(u32);

impl ZoneSet {
    pub fn insert(&mut self, zone: ZoneId) {
        self.0 |= zone.bit();
    }

    /// Every Zone in either set.
    pub fn union(self, other: ZoneSet) -> ZoneSet {
        ZoneSet(self.0 | other.0)
    }

    pub fn contains(self, zone: ZoneId) -> bool {
        self.0 & zone.bit() != 0
    }

    pub fn iter(self) -> impl Iterator<Item = ZoneId> {
        ZoneId::ALL.into_iter().filter(move |&zone| self.contains(zone))
    }
}

impl FromIterator<ZoneId> for ZoneSet {
    fn from_iter<I: IntoIterator<Item = ZoneId>>(iter: I) -> Self {
        let mut set = ZoneSet::default();
        for zone in iter {
            set.insert(zone);
        }
        set
    }
}

/// The Zone the Player is in after moving. They change Zone only once their whole body
/// (`min`..`max`) is inside the new one; until then they stay in the old Zone.
pub fn track_zone(current: ZoneId, min: Vec3, max: Vec3) -> ZoneId {
    let fully_inside = |zone: ZoneId| {
        let (lo, hi) = zone.volume();
        min.cmpge(lo).all() && max.cmple(hi).all()
    };
    if fully_inside(current) {
        return current;
    }
    ZoneId::ALL.into_iter().find(|&zone| fully_inside(zone)).unwrap_or(current)
}

/// The Active Zones: the Player's Zone, plus every Zone one open join away from it.
/// `open_joins` lists the joins that are passable now (open Doors, the ramp and the drop).
pub fn active_zones(player: ZoneId, open_joins: impl IntoIterator<Item = (ZoneId, ZoneId)>) -> ZoneSet {
    let mut active = ZoneSet::default();
    active.insert(player);
    for (a, b) in open_joins {
        if a == player {
            active.insert(b);
        } else if b == player {
            active.insert(a);
        }
    }
    active
}

/// The Zone a Player is standing in.
#[derive(Component)]
pub struct PlayerZone(pub ZoneId);

impl PlayerZone {
    /// Where every Player on `map` starts.
    pub fn start(map: MapId) -> Self {
        Self(map.layout().start_zone())
    }
}

/// Zones whose Spawn Points send in Zombies: every Player's Zone and its open neighbours.
#[derive(Resource)]
pub struct ActiveZones(pub ZoneSet);

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ZoneGizmos;

#[derive(Component)]
struct ZoneDebugText;

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<ZoneGizmos>()
            .insert_resource(ActiveZones(ZoneSet::default()))
            .add_systems(OnEnter(GameState::Playing), spawn_debug_text)
            .add_systems(
                Update,
                (
                    (track_player, update_active_zones).chain(),
                    (draw_zones, debug_text).run_if(|view: Res<DebugView>| view.0),
                    show_debug_text,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn track_player(mut players: Query<(&Transform, &mut PlayerZone), With<Player>>) {
    let half = Vec3::new(PLAYER_RADIUS, PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0, PLAYER_RADIUS);
    for (transform, mut zone) in &mut players {
        let at = transform.translation;
        let tracked = track_zone(zone.0, at - half, at + half);
        if tracked != zone.0 {
            zone.0 = tracked;
        }
    }
}

pub(crate) fn update_active_zones(
    players: Query<&PlayerZone, (With<Player>, Without<AwaitingRespawn>)>,
    doors: Query<&Door>,
    map: Res<MapId>,
    mut active: ResMut<ActiveZones>,
) {
    let joins: Vec<_> = map
        .layout()
        .fixed_joins
        .iter()
        .copied()
        .chain(doors.iter().filter(|door| door.open).filter_map(|door| door.joins))
        .collect();
    let set = players
        .iter()
        .fold(ZoneSet::default(), |set, zone| set.union(active_zones(zone.0, joins.iter().copied())));
    if set != active.0 {
        active.0 = set;
    }
}

fn draw_zones(mut gizmos: Gizmos<ZoneGizmos>, zone: Single<&PlayerZone, With<LocalPlayer>>, active: Res<ActiveZones>, map: Res<MapId>) {
    for &id in map.layout().zones {
        let (lo, hi) = id.volume();
        let colour = if id == zone.0 {
            Color::WHITE
        } else if active.0.contains(id) {
            Color::srgb(0.2, 1.0, 0.3)
        } else {
            Color::srgb(0.6, 0.2, 0.2)
        };
        let centre = (lo + hi) / 2.0;
        gizmos.cube(Transform::from_translation(centre).with_scale(hi - lo), colour);
    }
}

fn spawn_debug_text(mut commands: Commands) {
    commands.spawn((
        crate::state::InRun,
        ZoneDebugText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            left: px(16.0),
            top: px(12.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

fn debug_text(
    zone: Single<&PlayerZone, With<LocalPlayer>>,
    active: Res<ActiveZones>,
    mut text: Single<&mut Text, With<ZoneDebugText>>,
) {
    let names: Vec<_> = active.0.iter().map(ZoneId::name).collect();
    text.0 = format!("Zone: {}\nActive: {}", zone.0.name(), names.join(", "));
}

fn show_debug_text(view: Res<DebugView>, mut visibility: Single<&mut Visibility, With<ZoneDebugText>>) {
    let wanted = if view.0 { Visibility::Inherited } else { Visibility::Hidden };
    visibility.set_if_neq(wanted);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ZoneId::*;

    /// The Greybox's joins that never close.
    const JOINS: &[(ZoneId, ZoneId)] = MapId::Greybox.layout().fixed_joins;

    /// A Player-sized box with its feet at `(x, y, z)`.
    fn body(x: f32, y: f32, z: f32) -> (Vec3, Vec3) {
        (Vec3::new(x - 0.35, y, z - 0.35), Vec3::new(x + 0.35, y + 1.8, z + 0.35))
    }

    fn track(current: ZoneId, feet: (f32, f32, f32)) -> ZoneId {
        let (min, max) = body(feet.0, feet.1, feet.2);
        track_zone(current, min, max)
    }

    #[test]
    fn zone_volumes_do_not_overlap() {
        for a in ZoneId::ALL {
            for b in ZoneId::ALL.into_iter().filter(|&b| b != a) {
                let ((alo, ahi), (blo, bhi)) = (a.volume(), b.volume());
                let overlap = alo.cmplt(bhi).all() && blo.cmplt(ahi).all();
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn standing_in_a_room_puts_the_player_in_it() {
        assert_eq!(track(StartRoom, (0.0, 0.0, 20.0)), StartRoom);
        assert_eq!(track(StartRoom, (-20.0, 0.0, 0.0)), WestHall);
        assert_eq!(track(StartRoom, (0.0, 0.0, 0.0)), Yard);
        assert_eq!(track(StartRoom, (-12.0, 4.0, 0.0)), Balcony);
        assert_eq!(track(StartRoom, (-10.0, 0.0, -20.0)), DiningRoom);
        assert_eq!(track(StartRoom, (10.0, 0.0, -20.0)), PowerRoom);
        assert_eq!(track(StartRoom, (20.0, 0.0, 0.0)), EastHall);
    }

    #[test]
    fn a_player_straddling_a_doorway_stays_in_the_old_zone() {
        // The Start–Yard Door is in the wall at z = 14.
        assert_eq!(track(StartRoom, (0.0, 0.0, 14.0)), StartRoom);
        assert_eq!(track(Yard, (0.0, 0.0, 14.0)), Yard);
    }

    #[test]
    fn a_player_changes_zone_once_fully_inside_the_new_one() {
        assert_eq!(track(StartRoom, (0.0, 0.0, 13.7)), StartRoom);
        assert_eq!(track(StartRoom, (0.0, 0.0, 13.6)), Yard);
    }

    #[test]
    fn falling_from_the_balcony_moves_the_player_into_the_yard_only_once_below_it() {
        assert_eq!(track(Balcony, (-11.0, 3.0, 4.0)), Balcony);
        assert_eq!(track(Balcony, (-9.0, 1.0, 4.0)), Yard);
    }

    #[test]
    fn climbing_the_ramp_keeps_the_player_in_west_hall() {
        assert_eq!(track(WestHall, (-15.7, 2.0, 4.0)), WestHall);
        assert_eq!(track(WestHall, (-15.7, 4.0, -2.0)), WestHall);
    }

    #[test]
    fn the_start_room_alone_is_active_with_every_door_shut() {
        let active = active_zones(StartRoom, JOINS.iter().copied());
        assert_eq!(active.iter().collect::<Vec<_>>(), vec![StartRoom]);
    }

    #[test]
    fn open_doors_activate_the_zone_on_the_other_side() {
        let active = active_zones(StartRoom, JOINS.iter().copied().chain([(StartRoom, WestHall)]));
        assert!(active.contains(WestHall));
        assert!(!active.contains(Balcony), "only one join away counts");
    }

    #[test]
    fn west_hall_activates_the_balcony_but_not_the_yard() {
        let active = active_zones(WestHall, JOINS.iter().copied());
        assert!(active.contains(Balcony));
        assert!(!active.contains(Yard));
    }

    #[test]
    fn the_balcony_activates_west_hall_and_the_yard() {
        let active = active_zones(Balcony, JOINS.iter().copied());
        assert_eq!(active, ZoneSet::from_iter([Balcony, WestHall, Yard]));
    }

    #[test]
    fn the_drop_counts_both_ways_for_activation() {
        assert!(active_zones(Yard, JOINS.iter().copied()).contains(Balcony));
    }

    #[test]
    fn a_far_open_door_activates_nothing() {
        let active = active_zones(StartRoom, JOINS.iter().copied().chain([(DiningRoom, PowerRoom)]));
        assert_eq!(active, ZoneSet::from_iter([StartRoom]));
    }
}
