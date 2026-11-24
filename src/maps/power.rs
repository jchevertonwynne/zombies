//! Power: off at the start of every Run until the Player throws the Power Switch in the Power
//! Room, then on for the rest of the Run. Power Doors, Jump Pads and the Teleporter read `Power`
//! to know. Perk Machines, Traps and Pack-a-Punch ask `Supply`, which reads `Power` on a Map with
//! a Power Switch and the Generators (see `generator`) on a Map with them.

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::maps::generator::Generators;
use crate::maps::machine_look;
use crate::maps::map::textures::{Shape, Textured, box_uv_mesh};
use crate::maps::zone::ZoneId;

use crate::maps::map::{MapId, STOREY};

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

/// Turn the Power on now, as if the Power Switch had been thrown, and Claim every Generator the
/// Map has. The Host judges it.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchPowerOn;

/// What Powers the Map's machines right now, from the Power Switch's `Power` and the Generators,
/// as `Supply` reads them. On a Map with Generators they decide everything and `Power` counts for
/// nothing; otherwise it's the Power being on.
#[derive(Clone, Copy)]
pub struct Powering<'a> {
    power_on: bool,
    generators: Option<&'a Generators>,
}

impl<'a> Powering<'a> {
    pub fn new(power_on: bool, generators: Option<&'a Generators>) -> Self {
        Self { power_on, generators }
    }

    /// The Generators, on a Map Powered by them.
    fn on_map(self) -> Option<&'a Generators> {
        self.generators.filter(|generators| generators.on_map())
    }

    /// Whether the Map is Powered by Generators, not a Power Switch.
    pub fn on_generators(self) -> bool {
        self.on_map().is_some()
    }

    /// Whether something in `zone` that waits for Power works now: its Section being Powered, or
    /// always outside every Section.
    pub fn powers(self, zone: Option<ZoneId>) -> bool {
        self.on_map().map_or(self.power_on, |generators| generators.powers(zone))
    }

    /// Whether Pack-a-Punch works now: with every Generator Held.
    pub fn powers_pack_a_punch(self) -> bool {
        self.on_map().map_or(self.power_on, Generators::all_held)
    }

    /// Whether something running in `zone` loses its Power now: only when its Section's Generator
    /// is lost, since a Power Switch's Power never goes back off.
    pub fn cut_off(self, zone: Option<ZoneId>) -> bool {
        self.on_generators() && !self.powers(zone)
    }

    /// Whether the Power has been on this Run, as Gauntlet's "Power on" reads it: the Power Switch
    /// thrown, or any Generator ever Held, even if it's since been Lost.
    pub fn has_been_on(self) -> bool {
        self.power_on || self.generators.is_some_and(Generators::ever_held)
    }

    /// The refusal a machine in `zone` waiting for Power shows.
    pub fn refusal(self, zone: Option<ZoneId>) -> String {
        let section = self.generators.and_then(|generators| zone.and_then(|zone| generators.section_of(zone)));
        match section {
            Some(section) => format!("You must Claim the {} Generator first", section.name()),
            None if self.on_generators() => "You must Claim every Generator first".into(),
            None => "You must turn on the Power first".into(),
        }
    }
}

/// What Powers the Map's machines, whichever way the Map is Powered: the Power Switch's `Power`,
/// or its Generators. Every machine that waits for Power asks this (see `Powering`).
#[derive(SystemParam)]
pub struct Supply<'w> {
    power: Option<Res<'w, Power>>,
    generators: Option<Res<'w, Generators>>,
}

impl Supply<'_> {
    /// What Powers the Map's machines right now.
    pub fn now(&self) -> Powering<'_> {
        Powering::new(self.power.as_ref().is_some_and(|power| power.is_on()), self.generators.as_deref())
    }

    /// Whether a Perk Machine or Trap in `zone` that waits for Power works now.
    pub fn powers(&self, zone: Option<ZoneId>) -> bool {
        self.now().powers(zone)
    }

    /// Whether Pack-a-Punch works now.
    pub fn powers_pack_a_punch(&self) -> bool {
        self.now().powers_pack_a_punch()
    }

    /// Whether something running in `zone` loses its Power now (see `Powering::cut_off`).
    pub fn cut_off(&self, zone: Option<ZoneId>) -> bool {
        self.now().cut_off(zone)
    }

    /// Whether the Power has been on this Run (see `Powering::has_been_on`).
    pub fn has_been_on(&self) -> bool {
        self.now().has_been_on()
    }

    /// The refusal a machine in `zone` waiting for Power shows.
    pub fn refusal(&self, zone: Option<ZoneId>) -> String {
        self.now().refusal(zone)
    }
}

/// The Greybox's Power Switch: on the Power Room's east wall, at hand height over the control
/// platform it's thrown from.
pub(crate) const SWITCH_AT: Vec3 = Vec3::new(25.6, crate::maps::map::CONTROL_PLATFORM.height + 1.3, -20.0);
const SWITCH_FACING: Vec3 = Vec3::NEG_X;

/// Where the Power Switch is on `map`, and the way it faces. A Map with Generators has none.
pub(crate) fn switch_spot(map: MapId) -> Option<(Vec3, Vec3)> {
    if !map.layout().has_power_switch() {
        return None;
    }
    Some(match map {
        MapId::Greybox => (SWITCH_AT, SWITCH_FACING),
        // On the Offices' south wall, at hand height: the top of the climb but the Headframe.
        MapId::Quarry => (Vec3::new(-10.0, 2.0 * STOREY + 1.3, 16.4), Vec3::Z),
        // On the Mill's west wall, at the far end of the village from Pack-a-Punch.
        MapId::Village => (Vec3::new(-71.6, 1.3, 19.0), Vec3::X),
        // It has Generators, so it never gets here.
        MapId::Dam => return None,
        // On the Lighthouse's south wall, at hand height, at the foot of its stairs.
        MapId::Cove => (Vec3::new(-48.0, 3.0 * STOREY + 1.3, 44.4), Vec3::Z),
        // On the Pumping Station's seaward wall, at hand height, by the beam engine.
        MapId::Polder => (Vec3::new(62.0, STOREY + 1.3, 59.6), Vec3::NEG_Z),
        // On the Boiler House's north wall, at hand height, on the boiler front.
        MapId::Sanatorium => (Vec3::new(-66.0, 1.3, 13.6), Vec3::NEG_Z),
        // On the Pump Room's east wall, at hand height, on the pump engine's control panel.
        MapId::Archive => (Vec3::new(35.6, 1.3, 12.0), Vec3::NEG_X),
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
            .add_message::<SwitchPowerOn>()
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
                (use_switch.in_set(HostSimulation), switch_power_on.in_set(HostSimulation), flip_switch).chain().run_if(world_runs),
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
    let steel = machine_look::STEEL;
    let switch = commands
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
            Mesh3d(meshes.add(box_uv_mesh(panel.size(), Vec3::ZERO, Quat::IDENTITY, steel.tile()))),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.32, 0.3))),
            Textured { material: steel, shape: Shape::Baked },
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
        ))
        .id();
    machine_look::dress(&mut commands, &mut meshes, &mut materials, switch, &machine_look::power_switch(), Color::WHITE, LinearRgba::BLACK);
    // The cue: a lamp above the lever, red until the Power is on and green after, with a small
    // unshadowed light in the same colour. Its face is the switch's -Z.
    let (red, green) = (Color::srgb(1.0, 0.12, 0.08), Color::srgb(0.2, 1.0, 0.3));
    let lamp = materials.add(StandardMaterial { base_color: red, emissive: LinearRgba::from(red) * 5.0, ..default() });
    commands.entity(switch).with_child((
        machine_look::PowerGlow {
            material: lamp.clone(),
            off: LinearRgba::from(red) * 5.0,
            on: LinearRgba::from(green) * 5.0,
            light: Some((red, green)),
        },
        Mesh3d(meshes.add(Cuboid::new(0.14, 0.14, 0.04))),
        MeshMaterial3d(lamp),
        Transform::from_xyz(0.0, 0.33, -0.16),
        PointLight { color: red, intensity: 12_000.0, range: 3.5, shadow_maps_enabled: false, ..default() },
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

/// A `SwitchPowerOn` puts the Power on at once, throws the Power Switch for the show, and holds
/// every Generator.
fn switch_power_on(
    mut commands: Commands,
    mut asked: MessageReader<SwitchPowerOn>,
    mut power: ResMut<Power>,
    generators: Option<ResMut<Generators>>,
    mut switches: Query<(Entity, &mut PowerSwitch)>,
) {
    if asked.read().count() == 0 {
        return;
    }
    power.switch_on();
    for (entity, mut switch) in &mut switches {
        switch.start();
        commands.entity(entity).remove::<Buyable>();
    }
    if let Some(mut generators) = generators {
        for index in 0..generators.states().len() {
            generators.hold(index);
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
        let (off, on) = (Powering::new(false, Some(&generators)), Powering::new(true, Some(&generators)));
        for zone in [Some(crate::maps::zone::ZoneId::PowerRoom), None] {
            assert!(!off.powers(zone));
            assert!(on.powers(zone));
            assert!(!off.cut_off(zone), "a Power Switch's Power never goes back off");
        }
        assert!(!Powering::new(false, None).powers(None), "an app without Generators has a Power Switch");
        assert!(!off.powers_pack_a_punch());
        assert!(on.powers_pack_a_punch());
        assert!(!off.has_been_on());
        assert!(Powering::new(true, None).has_been_on());
    }

    /// On a Map with Generators, a machine waits for its Section's, Pack-a-Punch for all of them,
    /// and `Power` itself counts for nothing.
    #[test]
    fn with_generators_each_machine_waits_for_its_section() {
        use crate::maps::generator::SectionId;
        use crate::maps::zone::ZoneId;
        let mut generators = Generators::for_map(MapId::Dam);
        let index = |section| MapId::Dam.layout().generators.iter().position(|generator| generator.section == section).unwrap();
        let powered = |power_on, generators: &Generators, zone| Powering::new(power_on, Some(generators)).powers(Some(zone));
        assert!(!powered(true, &generators, ZoneId::SwitchgearRoom), "the Power counts for nothing");
        assert!(Powering::new(true, Some(&generators)).cut_off(Some(ZoneId::SwitchgearRoom)));
        assert!(powered(false, &generators, ZoneId::CrestRoad));
        generators.hold(index(SectionId::TurbineHall));
        assert!(powered(false, &generators, ZoneId::SwitchgearRoom));
        assert!(!powered(false, &generators, ZoneId::TowerHead));
        assert!(!Powering::new(true, Some(&generators)).powers_pack_a_punch());
        assert!(Powering::new(false, Some(&generators)).has_been_on(), "once any Generator is Held");
        generators.hold(index(SectionId::IntakeTower));
        generators.hold(index(SectionId::SpillwayGallery));
        assert!(Powering::new(false, Some(&generators)).powers_pack_a_punch());
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
