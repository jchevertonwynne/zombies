//! Fitting a unit model into a Prop's box. A Prop keeps its collider and its size exactly; a model
//! is only a visual laid inside it: the unit is repeated N times along the Prop's long axis, turned
//! 90 degrees about Y if that fits better, and stretched per axis by at most 15%. A Prop no unit
//! fits within that stays its textured box.
//!
//! A unit model's origin is the middle of its foot, with its length along +X and its depth along +Z.
//! The pure rules (`fit`, `Fit::placements`) are unit-tested; `FittedModel` and `fit_models` do the
//! loading, and only when the glTF is on disk, so headless runs and a missing file leave the box.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use super::textures::{Dress, dress, on_disk};

/// How far a unit may be stretched or squashed on any axis: 15%.
pub const MAX_STRETCH: f32 = 0.15;

/// How a unit model fills a Prop's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// How many copies lie side by side along the Prop's long axis.
    pub copies: usize,
    /// Whether the unit is turned a quarter about Y from lying with its length along the long axis.
    pub turned: bool,
    /// The scale on each of the unit's own axes, each within `1 +- MAX_STRETCH`.
    pub scale: Vec3,
}

fn within(scale: f32) -> bool {
    (1.0 - MAX_STRETCH..=1.0 + MAX_STRETCH).contains(&scale)
}

/// How `unit` (its size, before scaling) fits a Prop box `size` across: the arrangement whose
/// worst stretch is least, or `None` if no arrangement keeps every axis within 15%.
pub fn fit(unit: Vec3, size: Vec3) -> Option<Fit> {
    let (long, across) = if size.x >= size.z { (size.x, size.z) } else { (size.z, size.x) };
    let mut best: Option<(f32, Fit)> = None;
    for turned in [false, true] {
        // The unit's extent along the lay axis and across it.
        let (along_unit, across_unit) = if turned { (unit.z, unit.x) } else { (unit.x, unit.z) };
        if along_unit <= 0.0 || across_unit <= 0.0 || unit.y <= 0.0 {
            return None;
        }
        let guess = (long / along_unit).round().max(1.0) as usize;
        for copies in [guess.saturating_sub(1).max(1), guess, guess + 1] {
            let along = long / (copies as f32 * along_unit);
            let cross = across / across_unit;
            let up = size.y / unit.y;
            if ![along, cross, up].into_iter().all(within) {
                continue;
            }
            let worst = [along, cross, up].into_iter().map(|s| s.ln().abs()).fold(0.0, f32::max);
            if best.is_none_or(|(least, _)| worst < least) {
                let scale = if turned { Vec3::new(cross, up, along) } else { Vec3::new(along, up, cross) };
                best = Some((worst, Fit { copies, turned, scale }));
            }
        }
    }
    best.map(|(_, fit)| fit)
}

impl Fit {
    /// Where each copy goes for a Prop whose foot is centred on `at` and whose box is `size`: the
    /// transform of its origin (the middle of its foot), scale included. `turn` quarter turns more about Y face the model another way
    /// (the Props' `turn`); copies are laid along the same axis either way.
    pub fn placements(&self, at: Vec3, size: Vec3, turn: u8) -> Vec<Transform> {
        let along_z = size.z > size.x;
        let long = if along_z { size.z } else { size.x };
        // A unit's length runs along +X; turn it onto Z for a Prop long that way, a quarter more if
        // turned.
        let quarters = usize::from(along_z) + usize::from(self.turned) + usize::from(turn);
        let rotation = Quat::from_rotation_y(quarters as f32 * FRAC_PI_2);
        let lay = if along_z { Vec3::Z } else { Vec3::X };
        let each = long / self.copies as f32;
        (0..self.copies)
            .map(|k| {
                let offset = -long / 2.0 + each * (k as f32 + 0.5);
                Transform { translation: at + lay * offset, rotation, scale: self.scale }
            })
            .collect()
    }
}

/// A Prop's unit model, to be laid over its box once the glTF has loaded. `pieces` are the box
/// pieces that draw the Prop meanwhile (and for good if the model never loads).
#[derive(Component, Debug, Clone)]
pub struct FittedModel {
    /// The glTF under `assets/`, read as its first scene.
    pub path: &'static str,
    /// The unit's size as modelled.
    pub unit: Vec3,
    pub at: Vec3,
    pub size: Vec3,
    /// Extra quarter turns about Y (`Prop::turn`).
    pub turn: u8,
    pub pieces: Vec<Entity>,
}

#[derive(Component)]
pub(super) struct Fitted(Handle<WorldAsset>);

#[derive(Component)]
pub(super) struct Settled;

/// Starts loading each new `FittedModel`'s glTF if it's on disk and fits; settles it in its box
/// otherwise.
pub(super) fn request_models(mut commands: Commands, asset_server: Res<AssetServer>, models: Query<(Entity, &FittedModel), Added<FittedModel>>) {
    for (entity, model) in &models {
        if on_disk(model.path) && fit(model.unit, model.size).is_some() {
            let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(model.path));
            commands.entity(entity).insert(Fitted(scene));
        } else {
            debug!("{}: missing or unfittable, staying a box", model.path);
            commands.entity(entity).insert(Settled);
        }
    }
}

/// Swaps a Prop's box pieces for its fitted copies once the scene has loaded.
pub(super) fn put_on_models(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    models: Query<(Entity, &FittedModel, &Fitted), Without<Settled>>,
) {
    for (entity, model, Fitted(scene)) in &models {
        let load = load_of_scene(&asset_server, scene);
        match dress(true, &[load]) {
            Dress::Wait => continue,
            Dress::Textured => {
                if let Some(fit) = fit(model.unit, model.size) {
                    for placement in fit.placements(model.at, model.size, model.turn) {
                        commands.spawn((crate::state::InRun, WorldAssetRoot(scene.clone()), placement));
                    }
                    for &piece in &model.pieces {
                        commands.entity(piece).try_despawn();
                    }
                }
            }
            Dress::Palette => {}
        }
        commands.entity(entity).insert(Settled);
    }
}

fn load_of_scene(asset_server: &AssetServer, scene: &Handle<WorldAsset>) -> super::textures::Load {
    use super::textures::Load;
    match asset_server.load_state(scene) {
        bevy::asset::LoadState::Failed(_) => Load::Failed,
        bevy::asset::LoadState::Loaded if asset_server.is_loaded_with_dependencies(scene) => Load::Loaded,
        _ => Load::Loading,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a copy's box reaches, in the world, given the unit's size.
    fn reach(unit: Vec3, placement: &Transform) -> (Vec3, Vec3) {
        let corners = [-0.5, 0.5].into_iter().flat_map(|x| [0.0, 1.0].into_iter().flat_map(move |y| [-0.5, 0.5].map(|z| Vec3::new(x, y, z) * unit)));
        corners.map(|c| placement.transform_point(c)).fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| (lo.min(p), hi.max(p)))
    }

    #[test]
    fn a_unit_that_just_fits_is_laid_once_unscaled() {
        let fit = fit(Vec3::new(1.0, 2.0, 0.5), Vec3::new(1.0, 2.0, 0.5)).unwrap();
        assert_eq!(fit.copies, 1);
        assert!(!fit.turned);
        assert!(fit.scale.abs_diff_eq(Vec3::ONE, 1e-6));
    }

    #[test]
    fn copies_lie_along_the_long_axis_each_stretched_a_little() {
        // A 2.9 m shelf from 1 m units: three, each stretched 3.3%.
        let fit = fit(Vec3::new(1.0, 2.0, 0.4), Vec3::new(2.9, 2.0, 0.4)).unwrap();
        assert_eq!(fit.copies, 3);
        assert!((fit.scale.x - 2.9 / 3.0).abs() < 1e-6);
        // Squashing is as welcome as stretching: 2.6 m is three of 0.87.
        assert_eq!(super::fit(Vec3::new(1.0, 2.0, 0.4), Vec3::new(2.6, 2.0, 0.4)).unwrap().copies, 3);
    }

    #[test]
    fn no_axis_is_ever_scaled_past_fifteen_percent() {
        for long in [0.6, 0.9, 1.0, 1.3, 1.9, 2.4, 3.1, 4.2, 5.7] {
            for deep in [0.3, 0.45, 0.6, 1.0] {
                for tall in [0.8, 1.0, 1.8, 2.3] {
                    if let Some(fit) = fit(Vec3::new(1.0, 1.8, 0.5), Vec3::new(long, tall, deep)) {
                        for s in fit.scale.to_array() {
                            assert!(within(s), "{long}x{tall}x{deep}: {fit:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn what_cannot_fit_stays_a_box() {
        // Too short, too low or too deep for any unit, within 15%.
        assert!(fit(Vec3::new(1.0, 2.0, 0.4), Vec3::new(1.0, 1.4, 0.4)).is_none());
        assert!(fit(Vec3::new(1.0, 2.0, 0.4), Vec3::new(1.0, 2.0, 0.9)).is_none());
        // A gap between whole copies: 1.5 m from 1 m units is 1.5 or 0.75 of one.
        assert!(fit(Vec3::new(1.0, 2.0, 0.4), Vec3::new(1.5, 2.0, 0.4)).is_none());
        // And a degenerate unit.
        assert!(fit(Vec3::new(0.0, 2.0, 0.4), Vec3::new(1.0, 2.0, 0.4)).is_none());
    }

    #[test]
    fn a_unit_is_turned_when_its_other_side_fits() {
        // A unit 0.5 long and 1.0 deep in a box 1.0 long and 0.5 deep only fits turned.
        let fit = fit(Vec3::new(0.5, 1.0, 1.0), Vec3::new(1.0, 1.0, 0.5)).unwrap();
        assert!(fit.turned);
        assert_eq!(fit.copies, 1);
    }

    #[test]
    fn copies_stay_inside_the_box_for_either_long_axis() {
        let unit = Vec3::new(1.0, 2.0, 0.4);
        for size in [Vec3::new(3.1, 2.0, 0.42), Vec3::new(0.42, 2.0, 3.1), Vec3::new(2.0, 1.9, 0.38)] {
            let at = Vec3::new(10.0, 0.5, -4.0);
            let fit = fit(unit, size).unwrap();
            let placements = fit.placements(at, size, 0);
            assert_eq!(placements.len(), fit.copies);
            let (mut lo, mut hi) = (Vec3::MAX, Vec3::MIN);
            for placement in &placements {
                let (a, b) = reach(unit, placement);
                lo = lo.min(a);
                hi = hi.max(b);
            }
            let eps = Vec3::splat(1e-4);
            assert!(lo.cmpge(at + Vec3::new(-size.x / 2.0, 0.0, -size.z / 2.0) - eps).all(), "{size}: {lo}");
            assert!(hi.cmple(at + Vec3::new(size.x / 2.0, size.y, size.z / 2.0) + eps).all(), "{size}: {hi}");
            // Nothing falls far short of it either.
            assert!((hi - lo).abs_diff_eq(size, size.max_element() * 0.001 + 1e-3), "{size}: {}", hi - lo);
        }
    }

    #[test]
    fn copies_do_not_overlap() {
        let unit = Vec3::new(1.0, 2.0, 0.4);
        let fit = fit(unit, Vec3::new(3.0, 2.0, 0.4)).unwrap();
        let reaches: Vec<_> = fit.placements(Vec3::ZERO, Vec3::new(3.0, 2.0, 0.4), 0).iter().map(|p| reach(unit, p)).collect();
        for pair in reaches.windows(2) {
            assert!(pair[0].1.x <= pair[1].0.x + 1e-5);
        }
    }
}
