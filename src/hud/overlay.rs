//! The effect overlay layer: one full-screen layer under the rest of the HUD, with a pane for
//! each [`Overlay`]. Anything can show an effect by writing to [`Overlays`]: `hold` it every frame
//! while it lasts (low Health, standing in Napalm heat), or `pulse` it once and let it fade
//! (Shellshock, Teleporter aftereffects).

use bevy::prelude::*;

use crate::state::{GameState, InRun, Pause};

/// How long a pulsed overlay takes to fade out at the end of its time.
const OVERLAY_FADE_SECS: f32 = 1.0;

/// The effects the overlay layer can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// Redness at the screen's edges that deepens as Health falls.
    LowHealth,
    /// The Player is in Last Stand.
    LastStand,
    /// A pale haze from a nearby blast.
    Shellshock,
    /// A flickering orange tint from a Napalm Zombie's heat.
    NapalmHeat,
    /// A shimmering violet wash after a Teleporter jump.
    TeleporterDaze,
}

impl Overlay {
    pub const ALL: [Overlay; 5] = [
        Overlay::LowHealth,
        Overlay::LastStand,
        Overlay::Shellshock,
        Overlay::NapalmHeat,
        Overlay::TeleporterDaze,
    ];

    fn index(self) -> usize {
        self as usize
    }

    /// Whether its look moves with time, so it's redrawn every frame it shows.
    fn animated(self) -> bool {
        matches!(self, Overlay::NapalmHeat | Overlay::TeleporterDaze)
    }

    /// Its look at `strength` (0–1), `elapsed` seconds into the Run: a tint from the centre out to
    /// the edges.
    fn look(self, strength: f32, elapsed: f32) -> (Color, Color) {
        let a = strength;
        match self {
            Overlay::LowHealth => (Color::srgba(0.6, 0.0, 0.0, 0.2 * a), Color::srgba(0.55, 0.0, 0.0, 0.85 * a)),
            Overlay::LastStand => (Color::srgba(0.15, 0.0, 0.0, 0.35 * a), Color::srgba(0.3, 0.0, 0.0, 0.9 * a)),
            Overlay::Shellshock => (Color::srgba(0.85, 0.85, 0.85, 0.5 * a), Color::srgba(0.9, 0.9, 0.9, 0.8 * a)),
            Overlay::NapalmHeat => {
                let flicker = 0.85 + 0.15 * (elapsed * 9.0).sin();
                (
                    Color::srgba(1.0, 0.45, 0.05, 0.12 * a * flicker),
                    Color::srgba(1.0, 0.3, 0.0, 0.6 * a * flicker),
                )
            }
            Overlay::TeleporterDaze => {
                let shimmer = 0.75 + 0.25 * (elapsed * 4.0).sin();
                (
                    Color::srgba(0.6, 0.7, 1.0, 0.25 * a),
                    Color::srgba(0.5, 0.3, 1.0, 0.7 * a * shimmer),
                )
            }
        }
    }
}

/// A pulsed overlay: its strength, and its seconds left of the seconds it was given.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Pulse {
    strength: f32,
    left: f32,
    secs: f32,
}

impl Pulse {
    /// Full strength, fading out over its last `OVERLAY_FADE_SECS` (or all of it, if shorter).
    fn strength(&self) -> f32 {
        let fade = self.secs.min(OVERLAY_FADE_SECS);
        if self.left <= 0.0 || fade <= 0.0 {
            return 0.0;
        }
        self.strength * (self.left / fade).min(1.0)
    }
}

/// How strongly each effect overlay shows. Sources write to it; the HUD draws it.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Overlays {
    held: [f32; Overlay::ALL.len()],
    pulses: [Pulse; Overlay::ALL.len()],
}

impl Overlays {
    /// Shows an overlay at `strength` (0–1) this frame. For an effect that lasts while something
    /// is true, called every frame; the strongest hold of the frame wins.
    pub fn hold(&mut self, overlay: Overlay, strength: f32) {
        let held = &mut self.held[overlay.index()];
        *held = held.max(strength.clamp(0.0, 1.0));
    }

    /// Shows an overlay at `strength` (0–1) for `secs`, fading out at the end. A pulse weaker than
    /// what the last one shows now leaves that one be.
    pub fn pulse(&mut self, overlay: Overlay, strength: f32, secs: f32) {
        let pulse = &mut self.pulses[overlay.index()];
        let strength = strength.clamp(0.0, 1.0);
        if strength >= pulse.strength() {
            *pulse = Pulse {
                strength,
                left: secs,
                secs,
            };
        }
    }

    /// How strongly an overlay shows, 0–1.
    pub fn strength(&self, overlay: Overlay) -> f32 {
        let i = overlay.index();
        self.held[i].max(self.pulses[i].strength())
    }

    /// Runs every pulse down by `dt`.
    pub fn tick(&mut self, dt: f32) {
        for pulse in &mut self.pulses {
            pulse.left = (pulse.left - dt).max(0.0);
        }
    }

    /// Lets go of this frame's holds.
    pub fn end_frame(&mut self) {
        self.held = Default::default();
    }
}

/// One overlay's full-screen pane, and the strength it was last drawn at.
#[derive(Component)]
struct OverlayPane {
    overlay: Overlay,
    drawn: f32,
}

pub struct OverlayPlugin;

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Overlays>()
            .add_systems(OnEnter(GameState::Playing), (reset, spawn_layer))
            .add_systems(Update, tick.run_if(in_state(Pause::Running)))
            .add_systems(
                PostUpdate,
                (draw, end_frame.run_if(in_state(Pause::Running)))
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn reset(mut overlays: ResMut<Overlays>) {
    *overlays = Overlays::default();
}

fn spawn_layer(mut commands: Commands) {
    // Under everything else on the HUD.
    commands
        .spawn((InRun, super::full_screen(), GlobalZIndex(-1)))
        .with_children(|layer| {
            for overlay in Overlay::ALL {
                layer.spawn((OverlayPane { overlay, drawn: 0.0 }, super::full_screen(), Visibility::Hidden));
            }
        });
}

fn tick(mut overlays: ResMut<Overlays>, time: Res<Time>) {
    overlays.tick(time.delta_secs());
}

fn draw(
    mut commands: Commands,
    overlays: Res<Overlays>,
    mut panes: Query<(Entity, &mut OverlayPane, &mut Visibility)>,
    time: Res<Time>,
) {
    for (entity, mut pane, mut visibility) in &mut panes {
        let strength = overlays.strength(pane.overlay);
        if strength <= 0.0 {
            visibility.set_if_neq(Visibility::Hidden);
            pane.drawn = 0.0;
            continue;
        }
        visibility.set_if_neq(Visibility::Inherited);
        if strength == pane.drawn && !pane.overlay.animated() {
            continue;
        }
        pane.drawn = strength;
        let (centre, edge) = pane.overlay.look(strength, time.elapsed_secs());
        commands.entity(entity).insert(BackgroundGradient::from(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![ColorStop::percent(centre, 0.0), ColorStop::percent(edge, 100.0)],
        )));
    }
}

fn end_frame(mut overlays: ResMut<Overlays>) {
    overlays.end_frame();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_shows_by_default() {
        let overlays = Overlays::default();
        for overlay in Overlay::ALL {
            assert_eq!(overlays.strength(overlay), 0.0);
        }
    }

    #[test]
    fn a_hold_lasts_one_frame_and_the_strongest_wins() {
        let mut overlays = Overlays::default();
        overlays.hold(Overlay::NapalmHeat, 0.3);
        overlays.hold(Overlay::NapalmHeat, 0.6);
        overlays.hold(Overlay::NapalmHeat, 0.4);
        assert_eq!(overlays.strength(Overlay::NapalmHeat), 0.6);
        assert_eq!(overlays.strength(Overlay::Shellshock), 0.0);
        overlays.end_frame();
        assert_eq!(overlays.strength(Overlay::NapalmHeat), 0.0);
    }

    #[test]
    fn strengths_are_kept_between_zero_and_one() {
        let mut overlays = Overlays::default();
        overlays.hold(Overlay::LowHealth, 3.0);
        overlays.pulse(Overlay::Shellshock, -1.0, 2.0);
        assert_eq!(overlays.strength(Overlay::LowHealth), 1.0);
        assert_eq!(overlays.strength(Overlay::Shellshock), 0.0);
    }

    #[test]
    fn a_pulse_holds_then_fades_over_its_last_second() {
        let mut overlays = Overlays::default();
        overlays.pulse(Overlay::Shellshock, 0.8, 3.0);
        overlays.tick(1.5);
        assert_eq!(overlays.strength(Overlay::Shellshock), 0.8);
        overlays.tick(1.0);
        assert!((overlays.strength(Overlay::Shellshock) - 0.4).abs() < 1e-5);
        overlays.tick(0.5);
        assert_eq!(overlays.strength(Overlay::Shellshock), 0.0);
    }

    #[test]
    fn a_short_pulse_fades_from_the_start() {
        let mut overlays = Overlays::default();
        overlays.pulse(Overlay::TeleporterDaze, 1.0, 0.5);
        overlays.tick(0.25);
        assert!((overlays.strength(Overlay::TeleporterDaze) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn a_pulse_survives_the_end_of_the_frame() {
        let mut overlays = Overlays::default();
        overlays.pulse(Overlay::Shellshock, 0.5, 2.0);
        overlays.end_frame();
        assert_eq!(overlays.strength(Overlay::Shellshock), 0.5);
    }

    #[test]
    fn a_weaker_pulse_leaves_a_stronger_one_be() {
        let mut overlays = Overlays::default();
        overlays.pulse(Overlay::Shellshock, 0.9, 4.0);
        overlays.pulse(Overlay::Shellshock, 0.3, 10.0);
        overlays.tick(3.5);
        assert!((overlays.strength(Overlay::Shellshock) - 0.45).abs() < 1e-5);
        // Once it has faded below a new pulse, the new one takes over.
        overlays.pulse(Overlay::Shellshock, 0.6, 2.0);
        assert_eq!(overlays.strength(Overlay::Shellshock), 0.6);
    }

    #[test]
    fn a_hold_and_a_pulse_show_the_stronger() {
        let mut overlays = Overlays::default();
        overlays.pulse(Overlay::NapalmHeat, 0.7, 2.0);
        overlays.hold(Overlay::NapalmHeat, 0.4);
        assert_eq!(overlays.strength(Overlay::NapalmHeat), 0.7);
        overlays.hold(Overlay::NapalmHeat, 0.9);
        assert_eq!(overlays.strength(Overlay::NapalmHeat), 0.9);
    }
}
