//! Power: off at the start of every Run until the Player throws the Power Switch in the Power
//! Room, then on for the rest of the Run. Perk Machines, Power Doors, Traps, and later the
//! Teleporter, read `Power` to know.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::buy::{Buyable, Purchased};
use crate::layers::GameLayer;
use crate::state::{GameState, InRun, Pause};
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
}

/// The Power Switch's spot on the Power Room's east wall, and the way it faces.
const SWITCH_AT: Vec3 = Vec3::new(25.6, 1.3, -20.0);
const SWITCH_FACING: Vec3 = Vec3::NEG_X;

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
                (use_switch, flip_switch)
                    .chain()
                    .run_if(in_state(Pause::Running)),
            );
    }
}

fn spawn_switch(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let panel = Cuboid::new(0.8, 1.2, 0.3);
    commands
        .spawn((
            InRun,
            Name::new("Power Switch"),
            PowerSwitch::default(),
            Buyable {
                use_point: SWITCH_AT + SWITCH_FACING * 0.2,
                prompt: "Press F to turn on the Power".into(),
                price: None,
                range: USE_RANGE,
            },
            Mesh3d(meshes.add(panel)),
            MeshMaterial3d(materials.add(Color::srgb(0.3, 0.32, 0.3))),
            Transform::from_translation(SWITCH_AT).looking_to(SWITCH_FACING, Vec3::Y),
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
    for Purchased(entity) in purchases.read() {
        if let Ok(mut switch) = switches.get_mut(*entity) {
            switch.start();
            commands.entity(*entity).remove::<Buyable>();
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
