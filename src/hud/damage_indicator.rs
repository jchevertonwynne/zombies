//! The damage indicator: a red arc on a ring around the crosshair, pointing toward where each hit
//! on the local Player came from, fading out over `DAMAGE_INDICATOR_SECS`.

use bevy::prelude::*;

use crate::last_stand::LastStand;
use crate::player::{LocalPlayer, PlayerHit};
use crate::respawn::AwaitingRespawn;
use crate::state::{GameState, InRun, world_runs};
use crate::tuning::*;

/// The indicator's red, at full strength.
const INDICATOR_RED: Color = Color::srgba(0.75, 0.0, 0.0, 0.85);

/// One hit's indicator: where it came from, and its seconds left.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Indicator {
    from: Vec3,
    left: f32,
}

/// The damage indicators showing for the local Player.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct DamageIndicators {
    shown: Vec<Indicator>,
}

impl DamageIndicators {
    /// A hit on the Player standing `at`, from a source at `from`.
    /// One within `DAMAGE_INDICATOR_MERGE_DEGREES` of a showing indicator refreshes that one;
    /// past `DAMAGE_INDICATOR_MAX`, it replaces the oldest. A source on top of the Player shows
    /// nothing.
    pub fn hit(&mut self, at: Vec3, from: Vec3) {
        // Any facing will do: only the angle between two sources counts.
        let Some(new) = bearing(at, Vec3::NEG_Z, from) else {
            return;
        };
        let fresh = Indicator {
            from,
            left: DAMAGE_INDICATOR_SECS,
        };
        let merge = DAMAGE_INDICATOR_MERGE_DEGREES.to_radians();
        let near = self.shown.iter_mut().find(|indicator| {
            bearing(at, Vec3::NEG_Z, indicator.from).is_some_and(|old| angle_apart(old, new) < merge)
        });
        if let Some(near) = near {
            *near = fresh;
            return;
        }
        if self.shown.len() >= DAMAGE_INDICATOR_MAX
            && let Some(oldest) = self.shown.iter_mut().min_by(|a, b| a.left.total_cmp(&b.left))
        {
            *oldest = fresh;
            return;
        }
        self.shown.push(fresh);
    }

    /// Runs every indicator down by `dt`, letting go of those that have faded out.
    pub fn tick(&mut self, dt: f32) {
        for indicator in &mut self.shown {
            indicator.left -= dt;
        }
        self.shown.retain(|indicator| indicator.left > 0.0);
    }

    /// Each indicator showing to the Player standing `at`, facing `forward`: its bearing in
    /// radians clockwise from straight ahead, and its strength (0–1).
    pub fn shown(&self, at: Vec3, forward: Vec3) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.shown.iter().filter_map(move |indicator| Some((bearing(at, forward, indicator.from)?, strength(indicator.left))))
    }
}

/// Full strength, fading out over the last `DAMAGE_INDICATOR_FADE_SECS`.
fn strength(left: f32) -> f32 {
    (left / DAMAGE_INDICATOR_FADE_SECS).clamp(0.0, 1.0)
}

/// How far apart two bearings are, the short way round: 0..=π.
fn angle_apart(a: f32, b: f32) -> f32 {
    let apart = (a - b).rem_euclid(std::f32::consts::TAU);
    apart.min(std::f32::consts::TAU - apart)
}

/// The bearing of `from` for a Player standing `at`, facing `forward`: radians clockwise (seen
/// from above) from straight ahead, in −π..=π. None for a source on top of them.
fn bearing(at: Vec3, forward: Vec3, from: Vec3) -> Option<f32> {
    let to = Vec2::new(from.x - at.x, from.z - at.z);
    let ahead = Vec2::new(forward.x, forward.z).try_normalize()?;
    if to.length() < DAMAGE_INDICATOR_MIN_DISTANCE {
        return None;
    }
    let right = Vec2::new(-ahead.y, ahead.x);
    Some(to.dot(right).atan2(to.dot(ahead)))
}

/// One of the `DAMAGE_INDICATOR_MAX` rings around the crosshair, each showing one indicator's arc.
#[derive(Component)]
struct IndicatorPane(usize);

pub struct DamageIndicatorPlugin;

impl Plugin for DamageIndicatorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DamageIndicators>()
            .add_systems(OnEnter(GameState::Playing), (reset, spawn_layer))
            .add_systems(Update, (tick, note_hits).chain().run_if(world_runs))
            .add_systems(PostUpdate, draw.run_if(in_state(GameState::Playing)));
    }
}

fn reset(mut indicators: ResMut<DamageIndicators>) {
    *indicators = DamageIndicators::default();
}

fn spawn_layer(mut commands: Commands) {
    let diameter = 2.0 * DAMAGE_INDICATOR_RADIUS_PX;
    commands
        .spawn((
            InRun,
            Node {
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..super::full_screen()
            },
        ))
        .with_children(|layer| {
            for i in 0..DAMAGE_INDICATOR_MAX {
                layer.spawn((
                    IndicatorPane(i),
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(diameter),
                        height: px(diameter),
                        border: UiRect::all(px(DAMAGE_INDICATOR_THICKNESS_PX)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    Visibility::Hidden,
                ));
            }
        });
}

fn tick(mut indicators: ResMut<DamageIndicators>, time: Res<Time>) {
    indicators.tick(time.delta_secs());
}

/// Each hit with a source on the local Player, while they can be hurt, shows an indicator.
fn note_hits(
    mut hits: MessageReader<PlayerHit>,
    local: Query<(Entity, &Transform, Has<LastStand>, Has<AwaitingRespawn>), With<LocalPlayer>>,
    mut indicators: ResMut<DamageIndicators>,
) {
    let Ok((player, transform, last_stand, awaiting)) = local.single() else {
        hits.clear();
        return;
    };
    for hit in hits.read() {
        if let Some(from) = hit.from
            && hit.player == player
            && !last_stand
            && !awaiting
        {
            indicators.hit(transform.translation, from);
        }
    }
}

/// Draws each indicator as an arc of its ring, centred on its bearing from where the local Player
/// faces now.
fn draw(
    mut commands: Commands,
    indicators: Res<DamageIndicators>,
    local: Query<&Transform, With<LocalPlayer>>,
    mut panes: Query<(Entity, &IndicatorPane, &mut Visibility)>,
) {
    let shown: Vec<_> = match local.single() {
        Ok(transform) => indicators.shown(transform.translation, *transform.forward()).collect(),
        Err(_) => Vec::new(),
    };
    let half = DAMAGE_INDICATOR_ARC_DEGREES.to_radians() / 2.0;
    for (entity, pane, mut visibility) in &mut panes {
        let Some(&(bearing, strength)) = shown.get(pane.0) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let red = INDICATOR_RED.with_alpha(INDICATOR_RED.alpha() * strength);
        let clear = red.with_alpha(0.0);
        // A conic gradient turns clockwise from the top, as the bearing does.
        let mut arc = ConicGradient::new(
            UiPosition::CENTER,
            vec![
                AngularColorStop::new(clear, 0.0),
                AngularColorStop::new(red, half),
                AngularColorStop::new(clear, 2.0 * half),
                AngularColorStop::new(clear, std::f32::consts::TAU),
            ],
        );
        arc.start = bearing - half;
        commands.entity(entity).insert(BorderGradient::from(arc));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bearings showing to a Player at the origin facing −Z, in degrees.
    fn bearings(indicators: &DamageIndicators) -> Vec<f32> {
        indicators.shown(Vec3::ZERO, Vec3::NEG_Z).map(|(bearing, _)| bearing.to_degrees()).collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_hit_from_the_right_shows_to_the_right() {
        let mut indicators = DamageIndicators::default();
        indicators.hit(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
        let shown = bearings(&indicators);
        assert_eq!(shown.len(), 1);
        assert!(close(shown[0], 90.0), "{shown:?}");
    }

    #[test]
    fn it_keeps_pointing_at_the_source_as_the_player_turns() {
        let mut indicators = DamageIndicators::default();
        indicators.hit(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
        let facing_it: Vec<_> = indicators.shown(Vec3::ZERO, Vec3::X).map(|(bearing, _)| bearing).collect();
        assert!(close(facing_it[0], 0.0), "{facing_it:?}");
        let back_to_it: Vec<_> = indicators.shown(Vec3::ZERO, Vec3::NEG_X).map(|(bearing, _)| bearing.abs()).collect();
        assert!(close(back_to_it[0].to_degrees(), 180.0), "{back_to_it:?}");
    }

    #[test]
    fn it_holds_then_fades_out() {
        let mut indicators = DamageIndicators::default();
        indicators.hit(Vec3::ZERO, Vec3::Z);
        let strength = |indicators: &DamageIndicators| -> Vec<f32> {
            indicators.shown(Vec3::ZERO, Vec3::NEG_Z).map(|(_, strength)| strength).collect()
        };
        // 1.6 s in all, fading over the last 1.0.
        indicators.tick(0.5);
        assert_eq!(strength(&indicators), [1.0]);
        indicators.tick(0.6);
        assert!(close(strength(&indicators)[0], 0.5), "{:?}", strength(&indicators));
        indicators.tick(0.6);
        assert!(strength(&indicators).is_empty());
    }

    /// A source 3 m from the origin, `degrees` clockwise from −Z.
    fn around(degrees: f32) -> Vec3 {
        let r = degrees.to_radians();
        Vec3::new(r.sin(), 0.0, -r.cos()) * 3.0
    }

    #[test]
    fn a_hit_near_a_showing_indicator_refreshes_it() {
        let mut indicators = DamageIndicators::default();
        indicators.hit(Vec3::ZERO, around(90.0));
        indicators.tick(1.0);
        indicators.hit(Vec3::ZERO, around(110.0));
        let shown: Vec<_> = indicators.shown(Vec3::ZERO, Vec3::NEG_Z).collect();
        assert_eq!(shown.len(), 1, "{shown:?}");
        assert!(close(shown[0].0.to_degrees(), 110.0));
        assert_eq!(shown[0].1, 1.0);
    }

    #[test]
    fn hits_from_apart_each_show() {
        let mut indicators = DamageIndicators::default();
        indicators.hit(Vec3::ZERO, around(0.0));
        indicators.hit(Vec3::ZERO, around(90.0));
        indicators.hit(Vec3::ZERO, around(-150.0));
        let mut shown = bearings(&indicators);
        shown.sort_by(f32::total_cmp);
        assert_eq!(shown.len(), 3);
        assert!(close(shown[0], -150.0) && close(shown[1], 0.0) && close(shown[2], 90.0), "{shown:?}");
    }

    #[test]
    fn past_the_cap_a_new_hit_replaces_the_oldest() {
        let mut indicators = DamageIndicators::default();
        // Six hits 50 degrees apart, the first the oldest, then one well clear of them all.
        for i in 0..6 {
            indicators.hit(Vec3::ZERO, around(i as f32 * 50.0));
            indicators.tick(0.1);
        }
        indicators.hit(Vec3::ZERO, around(-55.0));
        let shown = bearings(&indicators);
        assert_eq!(shown.len(), 6, "{shown:?}");
        assert!(!shown.iter().any(|&b| close(b, 0.0)), "{shown:?}");
        assert!(shown.iter().any(|&b| close(b, -55.0)), "{shown:?}");
    }

    #[test]
    fn a_source_on_top_of_the_player_shows_nothing() {
        let mut indicators = DamageIndicators::default();
        indicators.hit(Vec3::ZERO, Vec3::new(0.01, 2.0, 0.0));
        assert!(bearings(&indicators).is_empty());
    }
}
