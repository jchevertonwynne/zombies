//! Power: off at the start of every Run until the Player throws the Power Switch in the Power
//! Room, then on for the rest of the Run. Power Doors, Jump Pads and the Teleporter read `Power`
//! to know. Perk Machines, Traps and Pack-a-Punch ask `Supply`, which reads `Power` on a Map with
//! a Power Switch and the Generators (see `generator`) on a Map with them.

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::maps::generator::Generators;
use crate::maps::zone::ZoneId;

use crate::maps::map::{MapId, QUARRY_X, STOREY, VILLAGE_X};

use crate::authority::HostSimulation;
use crate::economy::buy::{Buyable, Purchased};
use crate::layers::GameLayer;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;

/// Whether the Power is on. It never goes back off during a Run.
#[derive(Resource, Default)]
pub struct Power {
    on: bool,
}

impl Power {
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// The Power is on, as the Host says.
    pub fn switch_on(&mut self) {
        self.on = true;
    }
}

/// Whether something in `zone` that waits for Power works now. On a Map with Generators, that's
/// its Section being Powered, or always outside every Section; otherwise it's the Power being on.
pub fn powered(power_on: bool, generators: Option<&Generators>, zone: Option<ZoneId>) -> bool {
    match generators {
        Some(generators) if generators.on_map() => generators.powers(zone),
        _ => power_on,
    }
}

/// Whether Pack-a-Punch works now: with every Generator Held, on a Map with them; otherwise with
/// the Power on.
pub fn pack_a_punch_powered(power_on: bool, generators: Option<&Generators>) -> bool {
    match generators {
        Some(generators) if generators.on_map() => generators.all_held(),
        _ => power_on,
    }
}

/// Whether the Power has been on this Run, as Gauntlet's "Power on" reads it: the Power Switch
/// thrown, or any Generator ever Held, even if it's since been Lost.
pub fn power_has_been_on(power_on: bool, generators: Option<&Generators>) -> bool {
    power_on || generators.is_some_and(Generators::ever_held)
}

/// What Powers the Map's machines, whichever way the Map is Powered: the Power Switch's `Power`,
/// or its Generators. Every machine that waits for Power asks this.
#[derive(SystemParam)]
pub struct Supply<'w> {
    power: Option<Res<'w, Power>>,
    generators: Option<Res<'w, Generators>>,
}

impl Supply<'_> {
    fn power_on(&self) -> bool {
        self.power.as_ref().is_some_and(|power| power.is_on())
    }

    /// Whether a Perk Machine or Trap in `zone` that waits for Power works now.
    pub fn powers(&self, zone: Option<ZoneId>) -> bool {
        powered(self.power_on(), self.generators.as_deref(), zone)
    }

    /// Whether Pack-a-Punch works now.
    pub fn powers_pack_a_punch(&self) -> bool {
        pack_a_punch_powered(self.power_on(), self.generators.as_deref())
    }

    /// Whether the Power has been on this Run (see `power_has_been_on`).
    pub fn has_been_on(&self) -> bool {
        power_has_been_on(self.power_on(), self.generators.as_deref())
    }

    /// Whether the Map is Powered by Generators, not a Power Switch.
    pub fn on_generators(&self) -> bool {
        self.generators.as_ref().is_some_and(|generators| generators.on_map())
    }

    /// The refusal a machine in `zone` waiting for Power shows.
    pub fn refusal(&self, zone: Option<ZoneId>) -> String {
        let section = self.generators.as_ref().and_then(|generators| zone.and_then(|zone| generators.section_of(zone)));
        match section {
            Some(section) => format!("You must Claim the {} Generator first", section.name()),
            None if self.on_generators() => "You must Claim every Generator first".into(),
            None => "You must turn on the Power first".into(),
        }
    }
}

/// The Greybox's Power Switch: on the Power Room's east wall, at hand height over the control
/// platform it's thrown from.
pub(crate) const SWITCH_AT: Vec3 = Vec3::new(25.6, crate::maps::map::CONTROL_PLATFORM.height + 1.3, -20.0);
const SWITCH_FACING: Vec3 = Vec3::NEG_X;

/// Where the Power Switch is on `map`, and the way it faces. The Dam has none: its Power comes
/// from its Generators.
pub(crate) fn switch_spot(map: MapId) -> Option<(Vec3, Vec3)> {
    Some(match map {
        MapId::Greybox => (SWITCH_AT, SWITCH_FACING),
        // On the Offices' south wall, at hand height: the top of the climb but the Headframe.
        MapId::Quarry => (Vec3::new(QUARRY_X - 10.0, 2.0 * STOREY + 1.3, 16.4), Vec3::Z),
        // On the Mill's west wall, at the far end of the village from Pack-a-Punch.
        MapId::Village => (Vec3::new(VILLAGE_X - 71.6, 1.3, 19.0), Vec3::X),
        MapId::Dam => return None,
    })
}

/// The lever that turns the Power on. Used, not bought: it has a Prompt but no price.
#[derive(Component, Default)]
pub struct PowerSwitch {
    /// Seconds into the flip, once the Player has used it.
    flip: Option<f32>,
}

impl PowerSwitch {
    /// Throw the lever. Using it again does nothing.
    pub fn start(&mut self) {
        self.flip.get_or_insert(0.0);
    }

    /// Advance the flip. True on the frame it completes and the Power comes on.
    pub fn advance(&mut self, dt: f32) -> bool {
        let Some(elapsed) = &mut self.flip else {
            return false;
        };
        if *elapsed >= POWER_SWITCH_FLIP_SECS {
            return false;
        }
        *elapsed += dt;
        *elapsed >= POWER_SWITCH_FLIP_SECS
    }

    /// Whether the lever has been thrown, or is being.
    pub fn started(&self) -> bool {
        self.flip.is_some()
    }

    /// How far the lever has been thrown, 0 to 1.
    pub fn fraction(&self) -> f32 {
        self.flip
            .map_or(0.0, |elapsed| (elapsed / POWER_SWITCH_FLIP_SECS).min(1.0))
    }
}

/// The lever's handle, a child of the switch, which swings down as it's thrown.
#[derive(Component)]
struct Lever;

pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Power>()
            .add_systems(
                OnEnter(GameState::Playing),
                (
                    |mut power: ResMut<Power>| *power = Power::default(),
                    spawn_switch,
                ),
            )
            .add_systems(
                Update,
                // Every machine swings the lever; the Host's bought use starts it.
                (use_switch.in_set(HostSimulation), flip_switch).chain().run_if(world_runs),
            );
    }
}

fn spawn_switch(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    map: Res<MapId>,
) {
    let Some((at, facing)) = switch_spot(*map) else { return };
    let panel = Cuboid::new(0.8, 1.2, 0.3);
    commands
        .spawn((
            InRun,
            Name::new("Power Switch"),
            PowerSwitch::default(),
            Buyable {
                use_point: at + facing * 0.2,
                prompt: "Press F to turn on the Power".into(),
                price: None,
                range: USE_RANGE,
            },
            Mesh3d(meshes.add(panel)),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.32, 0.3))),
            Transform::from_translation(at).looking_to(facing, Vec3::Y),
            RigidBody::Static,
            Collider::from(panel),
            CollisionLayers::new(GameLayer::World, LayerMask::ALL),
        ))
        .with_child((
            Lever,
            Mesh3d(meshes.add(Cuboid::new(0.08, 0.5, 0.08))),
            MeshMaterial3d(materials.add(Color::srgb(0.7, 0.1, 0.1))),
            lever_transform(0.0),
        ));
}

/// Up when off, down once thrown. Local to the switch, whose −Z faces the Player.
fn lever_transform(fraction: f32) -> Transform {
    let angle = (1.0 - 2.0 * fraction) * 0.7;
    let pivot = Vec3::new(0.0, 0.0, -0.2);
    let rotation = Quat::from_rotation_x(-angle);
    Transform::from_translation(pivot + rotation * Vec3::new(0.0, 0.25, 0.0))
        .with_rotation(rotation)
}

/// Using the switch starts the flip. A used switch shows no Prompt.
fn use_switch(
    mut commands: Commands,
    mut purchases: MessageReader<Purchased>,
    mut switches: Query<&mut PowerSwitch>,
) {
    for &Purchased { buyable, .. } in purchases.read() {
        if let Ok(mut switch) = switches.get_mut(buyable) {
            switch.start();
            commands.entity(buyable).remove::<Buyable>();
        }
    }
}

fn flip_switch(
    mut switches: Query<(&mut PowerSwitch, &Children)>,
    mut levers: Query<&mut Transform, With<Lever>>,
    mut power: ResMut<Power>,
    time: Res<Time>,
) {
    for (mut switch, children) in &mut switches {
        if switch.advance(time.delta_secs()) {
            power.on = true;
        }
        for child in children {
            if let Ok(mut lever) = levers.get_mut(*child) {
                let target = lever_transform(switch.fraction());
                lever.set_if_neq(target);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::player::Stance;

    /// Whether a Player standing with their feet at `feet`, looking at the switch, can use it.
    fn in_use_range(feet: Vec3) -> bool {
        let eye = feet + Vec3::Y * Stance::Standing.eye_height();
        let use_point = SWITCH_AT + SWITCH_FACING * 0.2;
        crate::economy::buy::reach(eye, (use_point - eye).normalize(), use_point, USE_RANGE).is_some()
    }

    #[test]
    fn the_power_switch_is_thrown_from_up_on_the_control_platform() {
        let platform = crate::maps::map::CONTROL_PLATFORM;
        assert!(platform.min.x < SWITCH_AT.x && SWITCH_AT.x < platform.max.x + 0.3, "the switch isn't over the platform");
        // Up on it, in front of the switch.
        assert!(in_use_range(Vec3::new(SWITCH_AT.x - 0.6, platform.height, SWITCH_AT.z)));
        // On the floor at the foot of its edge, as near as the floor gets.
        let below = Vec3::new(platform.min.x - PLAYER_RADIUS, 0.0, SWITCH_AT.z);
        assert!(!in_use_range(below), "reached from the floor below");
        // At hand height over the platform, as it was over the floor before there was one.
        let hand = SWITCH_AT.y - platform.height;
        assert!((hand - 1.3).abs() < 0.01, "the switch hangs {hand:.2} m over the platform");
    }

    /// On a Map with a Power Switch, everything that waits for Power waits for the switch,
    /// wherever it stands.
    #[test]
    fn with_a_power_switch_everything_waits_for_the_power() {
        let generators = Generators::for_map(MapId::Greybox);
        for zone in [Some(crate::maps::zone::ZoneId::PowerRoom), None] {
            assert!(!powered(false, Some(&generators), zone));
            assert!(powered(true, Some(&generators), zone));
        }
        assert!(!powered(false, None, None), "an app without Generators has a Power Switch");
        assert!(!pack_a_punch_powered(false, Some(&generators)));
        assert!(pack_a_punch_powered(true, Some(&generators)));
        assert!(!power_has_been_on(false, Some(&generators)));
        assert!(power_has_been_on(true, None));
    }

    /// On a Map with Generators, a machine waits for its Section's, Pack-a-Punch for all of them,
    /// and `Power` itself counts for nothing.
    #[test]
    fn with_generators_each_machine_waits_for_its_section() {
        use crate::maps::generator::SectionId;
        use crate::maps::zone::ZoneId;
        let mut generators = Generators::for_map(MapId::Dam);
        let index = |section| MapId::Dam.layout().generators.iter().position(|generator| generator.section == section).unwrap();
        assert!(!powered(true, Some(&generators), Some(ZoneId::SwitchgearRoom)), "the Power counts for nothing");
        assert!(powered(false, Some(&generators), Some(ZoneId::CrestRoad)));
        generators.hold(index(SectionId::TurbineHall));
        assert!(powered(false, Some(&generators), Some(ZoneId::SwitchgearRoom)));
        assert!(!powered(false, Some(&generators), Some(ZoneId::TowerHead)));
        assert!(!pack_a_punch_powered(true, Some(&generators)));
        assert!(power_has_been_on(false, Some(&generators)), "once any Generator is Held");
        generators.hold(index(SectionId::IntakeTower));
        generators.hold(index(SectionId::SpillwayGallery));
        assert!(pack_a_punch_powered(false, Some(&generators)));
    }

    #[test]
    fn the_power_comes_on_once_the_flip_finishes() {
        let mut switch = PowerSwitch::default();
        switch.start();
        assert!(!switch.advance(POWER_SWITCH_FLIP_SECS * 0.5));
        assert!(switch.advance(POWER_SWITCH_FLIP_SECS * 0.6));
        // Only once.
        assert!(!switch.advance(1.0));
        assert_eq!(switch.fraction(), 1.0);
    }

    #[test]
    fn an_unused_switch_does_nothing() {
        let mut switch = PowerSwitch::default();
        assert!(!switch.advance(10.0));
        assert_eq!(switch.fraction(), 0.0);
    }

    #[test]
    fn using_the_switch_again_doesnt_restart_the_flip() {
        let mut switch = PowerSwitch::default();
        switch.start();
        switch.advance(POWER_SWITCH_FLIP_SECS * 0.5);
        switch.start();
        assert!(switch.advance(POWER_SWITCH_FLIP_SECS * 0.6));
    }
}
