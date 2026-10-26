//! Blocky Weapon models: each Weapon is a handful of coloured boxes merged into one mesh, coloured
//! per vertex so a single material draws them all. The view model holds the one in hand, with the
//! Player's hands on it; the Mystery Box spins through them.
//!
//! A model is built along −Z (its muzzle) around a 0.06 × 0.1 receiver at the origin, which the
//! Upgraded glint wraps, and spans `model_length` × 0.3 from front to back, where the view
//! model's Attachments expect to find its muzzle, sights and magazine.
//!
//! In the hands, the magazine (if it has one) and the left hand are models of their own, so a
//! Reload can pull the magazine out and the hand can fetch a new one. The Knife is its own model
//! too: a right hand slashing across the screen.

use bevy::prelude::*;

use crate::arsenal::{WeaponClass, WeaponId};
use crate::tuning::WEAPONS;

/// How long each class's model is, as a multiple of a pistol's 0.3: a switch is easy to see.
pub fn model_length(class: WeaponClass) -> f32 {
    match class {
        WeaponClass::Pistol | WeaponClass::DualWield | WeaponClass::BallisticKnife => 1.0,
        WeaponClass::RayGun => 1.3,
        WeaponClass::Smg | WeaponClass::Crossbow => 1.7,
        WeaponClass::Thundergun => 2.0,
        WeaponClass::Rifle | WeaponClass::Shotgun => 2.3,
        WeaponClass::Lmg | WeaponClass::Sniper | WeaponClass::Launcher => 2.7,
    }
}

/// How much bigger a Weapon shows floating over the Mystery Box than in the Player's hands.
const WORLD_SCALE: f32 = 1.5;

const METAL: Color = Color::srgb(0.2, 0.2, 0.22);
const DARK: Color = Color::srgb(0.08, 0.08, 0.09);
const STEEL: Color = Color::srgb(0.55, 0.56, 0.6);
const WOOD: Color = Color::srgb(0.45, 0.27, 0.12);
const OLIVE: Color = Color::srgb(0.3, 0.34, 0.2);
const SKIN: Color = Color::srgb(0.82, 0.62, 0.46);
const SLEEVE: Color = Color::srgb(0.27, 0.31, 0.21);

/// A moving part of a model in the hands: its mesh, and where it sits at rest.
#[derive(Clone)]
pub struct Part {
    pub mesh: Handle<Mesh>,
    pub rest: Transform,
}

/// Every Weapon's model, by `WeaponId`.
#[derive(Resource)]
pub struct GunModels {
    /// In the Player's hands, with the right hand on it, but without its magazine or left hand.
    view: Vec<Handle<Mesh>>,
    magazine: Vec<Option<Part>>,
    /// The left hand on its fore-end. None where it's part of the model, as on the second gun
    /// of a dual-wield.
    left_hand: Vec<Option<Part>>,
    /// As it floats in the world, lying along X, magazine and all.
    world: Vec<Handle<Mesh>>,
    /// The Knife, and the Bowie Knife, in a right hand: the hand at the origin, the blade along −Z.
    pub knife: Handle<Mesh>,
    pub bowie: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

impl GunModels {
    pub fn view(&self, id: WeaponId) -> Handle<Mesh> {
        self.view[id as usize].clone()
    }

    pub fn magazine(&self, id: WeaponId) -> Option<&Part> {
        self.magazine[id as usize].as_ref()
    }

    pub fn left_hand(&self, id: WeaponId) -> Option<&Part> {
        self.left_hand[id as usize].as_ref()
    }

    pub fn world(&self, id: WeaponId) -> Handle<Mesh> {
        self.world[id as usize].clone()
    }
}

impl FromWorld for GunModels {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let mut view = Vec::new();
        let mut magazine = Vec::new();
        let mut left_hand = Vec::new();
        let mut world_meshes = Vec::new();
        for row in &WEAPONS {
            let mut gun = Gun::default();
            let hands = build(&mut gun, row.id, row.class);
            let mut whole = gun.mesh();
            if let Some((mesh, rest)) = &gun.magazine {
                whole.merge(&mesh.clone().transformed_by(*rest)).expect("both are triangle lists");
            }
            world_meshes.push(meshes.add(
                whole
                    .rotated_by(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
                    .scaled_by(Vec3::splat(WORLD_SCALE)),
            ));
            hand(&mut gun, hands.right, 1.0);
            // A dual-wield's left hand holds the second gun, which never moves on its own.
            let left = if row.class == WeaponClass::DualWield {
                hand(&mut gun, hands.left, -1.0);
                None
            } else {
                let mut left = Gun::default();
                hand(&mut left, Vec3::ZERO, -1.0);
                Some(Part {
                    mesh: meshes.add(left.mesh()),
                    // It cups the fore-end from its left, where it can be seen.
                    rest: Transform::from_translation(hands.left + Vec3::new(-0.03, -0.01, 0.0)),
                })
            };
            view.push(meshes.add(gun.mesh()));
            magazine.push(gun.magazine.map(|(mesh, rest)| Part {
                mesh: meshes.add(mesh),
                rest,
            }));
            left_hand.push(left);
        }
        let knife = |bowie: bool| {
            let mut knife = Gun::default();
            hand(&mut knife, Vec3::ZERO, 1.0);
            // The blade's flat faces the camera as it slashes across.
            let (handle, blade) = if bowie {
                (WOOD, Vec3::new(0.014, 0.06, 0.32))
            } else {
                (DARK, Vec3::new(0.012, 0.045, 0.26))
            };
            knife.part(Vec3::new(0.03, 0.035, 0.09), Vec3::new(0.0, 0.0, -0.07), handle);
            knife.part(Vec3::new(0.02, 0.07, 0.015), Vec3::new(0.0, 0.0, -0.115), STEEL);
            knife.part(blade, Vec3::new(0.0, 0.0, -0.12 - blade.z / 2.0), STEEL);
            knife.mesh()
        };
        let (knife, bowie) = (meshes.add(knife(false)), meshes.add(knife(true)));
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            // The vertex colours carry every part's colour.
            base_color: Color::WHITE,
            perceptual_roughness: 0.6,
            metallic: 0.15,
            ..default()
        });
        Self {
            view,
            magazine,
            left_hand,
            world: world_meshes,
            knife,
            bowie,
            material,
        }
    }
}

/// A model being built, one box at a time.
#[derive(Default)]
struct Gun {
    mesh: Option<Mesh>,
    /// Its magazine, built on its own around the point where it meets the gun, and where that is.
    magazine: Option<(Mesh, Transform)>,
}

impl Gun {
    /// A `size` box centred `at`, in `colour`.
    fn part(&mut self, size: Vec3, at: Vec3, colour: Color) -> &mut Self {
        self.turned(size, at, Quat::IDENTITY, colour)
    }

    /// A `size` box centred `at`, turned by `rotation`, in `colour`.
    fn turned(&mut self, size: Vec3, at: Vec3, rotation: Quat, colour: Color) -> &mut Self {
        let mut part = Mesh::from(Cuboid::from_size(size)).transformed_by(Transform {
            translation: at,
            rotation,
            ..default()
        });
        let colour = LinearRgba::from(colour).to_f32_array();
        part.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![colour; part.count_vertices()]);
        match &mut self.mesh {
            Some(mesh) => mesh.merge(&part).expect("every part is a triangle-list box"),
            None => self.mesh = Some(part),
        }
        self
    }

    fn mesh(&self) -> Mesh {
        self.mesh.clone().expect("every Weapon has parts")
    }
}

/// Where the Player's hands hold a model: the right on its grip, the left on its fore-end.
struct Hands {
    right: Vec3,
    left: Vec3,
}

/// A blocky hand at `at`, its sleeved forearm running back and down out of view, toward the
/// right (`side` 1) or the left (−1).
fn hand(gun: &mut Gun, at: Vec3, side: f32) {
    gun.part(Vec3::new(0.075, 0.085, 0.09), at, SKIN);
    let arm = Quat::from_rotation_y(side * 0.35) * Quat::from_rotation_x(0.55);
    gun.turned(Vec3::new(0.085, 0.085, 0.36), at + arm * Vec3::new(0.0, 0.0, 0.2), arm, SLEEVE);
}

/// A pistol's grip, angled back: `at` is its middle.
fn grip(gun: &mut Gun, at: Vec3, colour: Color) {
    gun.turned(Vec3::new(0.045, 0.12, 0.06), at, Quat::from_rotation_x(-0.25), colour);
}

/// A magazine hanging under the receiver at `z`, `length` long, curving forward by `curve`.
fn magazine(gun: &mut Gun, z: f32, length: f32, curve: f32, colour: Color) {
    let rest = Transform::from_xyz(0.0, -0.04, z).with_rotation(Quat::from_rotation_x(curve));
    detachable(gun, Vec3::new(0.035, length, 0.06), rest, colour);
}

/// The magazine: a `size` box hanging down from `rest`, where it meets the gun.
fn detachable(gun: &mut Gun, size: Vec3, rest: Transform, colour: Color) {
    let mut magazine = Gun::default();
    magazine.part(size, Vec3::new(0.0, -size.y / 2.0, 0.0), colour);
    gun.magazine = Some((magazine.mesh(), rest));
}

/// Iron sights: a post at the front and a notch at the back.
fn sights(gun: &mut Gun, y: f32, front: f32, back: f32) {
    gun.part(Vec3::new(0.012, 0.03, 0.012), Vec3::new(0.0, y + 0.015, front), DARK);
    gun.part(Vec3::new(0.03, 0.025, 0.012), Vec3::new(0.0, y + 0.012, back), DARK);
}

/// Builds `id`'s model, of `class`, and says where the hands go.
fn build(gun: &mut Gun, id: WeaponId, class: WeaponClass) -> Hands {
    let half = 0.15 * model_length(class);
    match class {
        WeaponClass::Pistol | WeaponClass::DualWield => {
            let grips = if matches!(id, WeaponId::M1911 | WeaponId::Python) { WOOD } else { DARK };
            let pistol = |gun: &mut Gun, x: f32| {
                if id == WeaponId::Python {
                    // A revolver: a long barrel, a fat cylinder, and a frame under them.
                    gun.part(Vec3::new(0.035, 0.04, 0.16), Vec3::new(x, 0.025, -0.07), STEEL);
                    gun.part(Vec3::new(0.06, 0.065, 0.08), Vec3::new(x, 0.01, 0.04), STEEL);
                    gun.part(Vec3::new(0.04, 0.05, 0.1), Vec3::new(x, 0.0, 0.1), STEEL);
                    gun.part(Vec3::new(0.012, 0.025, 0.012), Vec3::new(x, 0.055, -0.14), STEEL);
                } else {
                    gun.part(Vec3::new(0.055, 0.06, 0.3), Vec3::new(x, 0.02, 0.0), METAL);
                    gun.part(Vec3::new(0.05, 0.04, 0.2), Vec3::new(x, -0.03, -0.04), DARK);
                    gun.part(Vec3::new(0.03, 0.03, 0.02), Vec3::new(x, 0.02, -0.15), DARK);
                    sights(gun, 0.05, -0.13, 0.13);
                }
                gun.part(Vec3::new(0.012, 0.035, 0.06), Vec3::new(x, -0.06, 0.03), DARK);
                grip(gun, Vec3::new(x, -0.09, 0.1), grips);
                if class == WeaponClass::Pistol && id != WeaponId::Python {
                    // Up inside the grip, its base plate just showing under it.
                    let up_grip = Quat::from_rotation_x(-0.25);
                    let top = Vec3::new(x, -0.09, 0.1) + up_grip * Vec3::new(0.0, 0.06, 0.0);
                    detachable(
                        gun,
                        Vec3::new(0.04, 0.13, 0.05),
                        Transform::from_translation(top).with_rotation(up_grip),
                        DARK,
                    );
                }
            };
            pistol(gun, 0.0);
            if class == WeaponClass::DualWield {
                // The second gun, in the left hand across the screen.
                pistol(gun, -0.44);
                Hands {
                    right: Vec3::new(0.0, -0.12, 0.11),
                    left: Vec3::new(-0.44, -0.12, 0.11),
                }
            } else {
                Hands {
                    right: Vec3::new(0.0, -0.12, 0.11),
                    left: Vec3::new(-0.05, -0.15, 0.09),
                }
            }
        }
        WeaponClass::BallisticKnife => {
            gun.part(Vec3::new(0.045, 0.05, 0.17), Vec3::new(0.0, -0.01, 0.06), DARK);
            gun.part(Vec3::new(0.07, 0.065, 0.02), Vec3::new(0.0, -0.01, -0.03), STEEL);
            gun.part(Vec3::new(0.012, 0.045, 0.1), Vec3::new(0.0, 0.0, -0.09), STEEL);
            gun.part(Vec3::new(0.012, 0.025, 0.04), Vec3::new(0.0, 0.005, -0.15), STEEL);
            Hands {
                right: Vec3::new(0.0, -0.04, 0.07),
                left: Vec3::new(-0.05, -0.1, 0.12),
            }
        }
        WeaponClass::Smg => {
            let (body, furniture) = match id {
                WeaponId::Ak74u => (METAL, WOOD),
                _ => (METAL, DARK),
            };
            gun.part(Vec3::new(0.06, 0.09, 0.3), Vec3::new(0.0, 0.0, -0.01), body);
            gun.part(Vec3::new(0.025, 0.025, 0.12), Vec3::new(0.0, 0.01, -half + 0.06), DARK);
            match id {
                WeaponId::Mp40 => magazine(gun, -0.07, 0.2, 0.0, DARK),
                WeaponId::Ak74u => {
                    gun.part(Vec3::new(0.065, 0.06, 0.1), Vec3::new(0.0, -0.01, -0.15), WOOD);
                    magazine(gun, -0.06, 0.15, 0.35, furniture);
                }
                WeaponId::Spectre => {
                    gun.part(Vec3::new(0.065, 0.03, 0.26), Vec3::new(0.0, 0.055, -0.02), DARK);
                    magazine(gun, -0.06, 0.14, 0.0, DARK);
                }
                _ => magazine(gun, -0.06, 0.15, 0.1, DARK),
            }
            grip(gun, Vec3::new(0.0, -0.09, 0.08), furniture);
            // A folding stock: two thin rails and a butt plate.
            for x in [-0.025, 0.025] {
                gun.part(Vec3::new(0.012, 0.012, 0.13), Vec3::new(x, -0.01, half - 0.07), DARK);
            }
            gun.part(Vec3::new(0.06, 0.08, 0.02), Vec3::new(0.0, -0.02, half - 0.01), DARK);
            sights(gun, 0.045, -0.15, 0.1);
            Hands {
                right: Vec3::new(0.0, -0.12, 0.09),
                left: Vec3::new(0.0, -0.06, -0.15),
            }
        }
        WeaponClass::Rifle => {
            match id {
                WeaponId::G11 => {
                    // Caseless and boxy: one long slab, with the grip and trigger under it.
                    gun.part(Vec3::new(0.075, 0.15, 0.62), Vec3::new(0.0, 0.0, 0.0), METAL);
                    gun.part(Vec3::new(0.04, 0.05, 0.14), Vec3::new(0.0, 0.1, -0.05), DARK);
                    gun.part(Vec3::new(0.02, 0.02, 0.06), Vec3::new(0.0, 0.0, -half + 0.02), DARK);
                    grip(gun, Vec3::new(0.0, -0.12, -0.02), DARK);
                    return Hands {
                        right: Vec3::new(0.0, -0.15, -0.01),
                        left: Vec3::new(0.0, -0.09, -0.22),
                    };
                }
                WeaponId::Famas | WeaponId::Aug => {
                    // Bullpups: the magazine behind the grip, and the stock is the receiver.
                    let furniture = if id == WeaponId::Aug { OLIVE } else { DARK };
                    gun.part(Vec3::new(0.065, 0.11, 0.42), Vec3::new(0.0, 0.0, 0.12), furniture);
                    gun.part(Vec3::new(0.06, 0.08, 0.16), Vec3::new(0.0, 0.0, -0.17), METAL);
                    gun.part(Vec3::new(0.025, 0.025, 0.1), Vec3::new(0.0, 0.01, -half + 0.05), DARK);
                    magazine(gun, 0.17, 0.13, 0.15, DARK);
                    grip(gun, Vec3::new(0.0, -0.1, 0.0), furniture);
                    if id == WeaponId::Famas {
                        // The carry handle running along the top.
                        gun.part(Vec3::new(0.025, 0.03, 0.32), Vec3::new(0.0, 0.09, 0.0), DARK);
                        gun.part(Vec3::new(0.02, 0.04, 0.02), Vec3::new(0.0, 0.065, -0.15), DARK);
                    } else {
                        gun.part(Vec3::new(0.04, 0.04, 0.12), Vec3::new(0.0, 0.08, 0.0), DARK);
                    }
                    return Hands {
                        right: Vec3::new(0.0, -0.13, 0.0),
                        left: Vec3::new(0.0, -0.07, -0.18),
                    };
                }
                _ => {}
            }
            let furniture = match id {
                WeaponId::M14 | WeaponId::Galil => WOOD,
                _ => DARK,
            };
            gun.part(Vec3::new(0.06, 0.1, 0.26), Vec3::new(0.0, 0.0, -0.01), METAL);
            gun.part(Vec3::new(0.065, 0.07, 0.18), Vec3::new(0.0, -0.01, -0.22), furniture);
            gun.part(Vec3::new(0.025, 0.025, 0.1), Vec3::new(0.0, 0.01, -half + 0.05), DARK);
            // The stock, dropping a little toward the shoulder.
            gun.turned(
                Vec3::new(0.05, 0.1, 0.24),
                Vec3::new(0.0, -0.03, half - 0.12),
                Quat::from_rotation_x(0.08),
                furniture,
            );
            let curve = if matches!(id, WeaponId::Galil | WeaponId::Commando) { 0.3 } else { 0.12 };
            magazine(gun, -0.05, 0.15, curve, DARK);
            grip(gun, Vec3::new(0.0, -0.1, 0.07), if id == WeaponId::M14 { WOOD } else { DARK });
            sights(gun, 0.05, -half + 0.06, 0.08);
            Hands {
                right: Vec3::new(0.0, -0.13, 0.08),
                left: Vec3::new(0.0, -0.07, -0.22),
            }
        }
        WeaponClass::Shotgun => {
            if id == WeaponId::Olympia {
                // A double-barrel: two barrels side by side and a wooden stock.
                for x in [-0.016, 0.016] {
                    gun.part(Vec3::new(0.03, 0.03, 0.44), Vec3::new(x, 0.02, -0.12), STEEL);
                }
                gun.part(Vec3::new(0.06, 0.035, 0.26), Vec3::new(0.0, -0.01, -0.12), WOOD);
                gun.part(Vec3::new(0.06, 0.08, 0.1), Vec3::new(0.0, 0.0, 0.12), METAL);
                gun.turned(Vec3::new(0.05, 0.1, 0.2), Vec3::new(0.0, -0.05, 0.25), Quat::from_rotation_x(0.2), WOOD);
                return Hands {
                    right: Vec3::new(0.0, -0.1, 0.17),
                    left: Vec3::new(0.0, -0.06, -0.12),
                };
            }
            let furniture = match id {
                WeaponId::TrenchGun => WOOD,
                _ => DARK,
            };
            gun.part(Vec3::new(0.06, 0.1, 0.22), Vec3::new(0.0, 0.0, 0.04), METAL);
            gun.part(Vec3::new(0.035, 0.035, 0.3), Vec3::new(0.0, 0.025, -0.18), METAL);
            gun.part(Vec3::new(0.03, 0.03, 0.28), Vec3::new(0.0, -0.025, -0.17), METAL);
            // The pump on the tube.
            gun.part(Vec3::new(0.055, 0.05, 0.12), Vec3::new(0.0, -0.025, -0.17), furniture);
            if id == WeaponId::TrenchGun {
                // The heat shield over the barrel.
                gun.part(Vec3::new(0.045, 0.02, 0.2), Vec3::new(0.0, 0.05, -0.24), DARK);
            }
            if id == WeaponId::Hs10 {
                gun.part(Vec3::new(0.035, 0.1, 0.05), Vec3::new(0.0, -0.1, 0.0), DARK);
                grip(gun, Vec3::new(0.0, -0.1, 0.1), DARK);
            } else {
                gun.turned(
                    Vec3::new(0.05, 0.1, 0.2),
                    Vec3::new(0.0, -0.04, half - 0.09),
                    Quat::from_rotation_x(0.18),
                    furniture,
                );
            }
            sights(gun, 0.045, -half + 0.03, 0.1);
            Hands {
                right: Vec3::new(0.0, -0.1, 0.13),
                left: Vec3::new(0.0, -0.07, -0.17),
            }
        }
        WeaponClass::Sniper => {
            let furniture = if id == WeaponId::L96a1 { OLIVE } else { WOOD };
            gun.part(Vec3::new(0.06, 0.09, 0.28), Vec3::new(0.0, 0.0, 0.02), METAL);
            gun.part(Vec3::new(0.065, 0.07, 0.22), Vec3::new(0.0, -0.01, -0.22), furniture);
            gun.part(Vec3::new(0.025, 0.025, 0.2), Vec3::new(0.0, 0.01, -half + 0.1), DARK);
            // The scope, with fat bells at each end.
            gun.part(Vec3::new(0.035, 0.035, 0.22), Vec3::new(0.0, 0.085, 0.0), DARK);
            gun.part(Vec3::new(0.05, 0.05, 0.05), Vec3::new(0.0, 0.085, -0.12), DARK);
            gun.part(Vec3::new(0.045, 0.045, 0.04), Vec3::new(0.0, 0.085, 0.11), DARK);
            gun.part(Vec3::new(0.02, 0.04, 0.02), Vec3::new(0.0, 0.06, 0.0), DARK);
            if id == WeaponId::Dragunov {
                // A skeleton stock: a frame with a hole through it.
                gun.part(Vec3::new(0.05, 0.03, 0.26), Vec3::new(0.0, 0.0, half - 0.13), WOOD);
                gun.part(Vec3::new(0.05, 0.03, 0.26), Vec3::new(0.0, -0.09, half - 0.13), WOOD);
                gun.part(Vec3::new(0.05, 0.12, 0.04), Vec3::new(0.0, -0.045, half - 0.02), WOOD);
                magazine(gun, -0.05, 0.14, 0.2, DARK);
            } else {
                gun.part(Vec3::new(0.055, 0.12, 0.24), Vec3::new(0.0, -0.04, half - 0.12), furniture);
                magazine(gun, -0.03, 0.09, 0.0, DARK);
            }
            grip(gun, Vec3::new(0.0, -0.1, 0.09), furniture);
            Hands {
                right: Vec3::new(0.0, -0.13, 0.1),
                left: Vec3::new(0.0, -0.07, -0.22),
            }
        }
        WeaponClass::Lmg => {
            gun.part(Vec3::new(0.08, 0.12, 0.34), Vec3::new(0.0, 0.0, 0.0), METAL);
            gun.part(Vec3::new(0.07, 0.08, 0.2), Vec3::new(0.0, -0.01, -0.26), DARK);
            gun.part(Vec3::new(0.035, 0.035, 0.18), Vec3::new(0.0, 0.01, -half + 0.09), DARK);
            // The bipod, folded under the barrel.
            for x in [-0.02, 0.02] {
                gun.part(Vec3::new(0.012, 0.012, 0.18), Vec3::new(x, -0.04, -half + 0.12), DARK);
            }
            if id == WeaponId::Rpk {
                // A drum under the receiver, and wood furniture.
                detachable(gun, Vec3::new(0.09, 0.12, 0.12), Transform::from_xyz(0.0, -0.07, -0.03), DARK);
                gun.turned(Vec3::new(0.05, 0.1, 0.24), Vec3::new(0.0, -0.04, half - 0.12), Quat::from_rotation_x(0.12), WOOD);
            } else {
                // A box of belted ammo hanging off the side.
                detachable(gun, Vec3::new(0.1, 0.1, 0.12), Transform::from_xyz(-0.06, -0.06, -0.03), OLIVE);
                gun.part(Vec3::new(0.06, 0.11, 0.24), Vec3::new(0.0, -0.02, half - 0.12), DARK);
            }
            // A carry handle on top.
            gun.part(Vec3::new(0.02, 0.05, 0.1), Vec3::new(0.0, 0.09, -0.03), DARK);
            grip(gun, Vec3::new(0.0, -0.12, 0.1), DARK);
            Hands {
                right: Vec3::new(0.0, -0.15, 0.1),
                left: Vec3::new(0.0, -0.08, -0.26),
            }
        }
        WeaponClass::Launcher => {
            if id == WeaponId::M72Law {
                // A green tube over the shoulder, with a flip-up sight.
                gun.part(Vec3::new(0.09, 0.09, 0.8), Vec3::new(0.0, 0.01, 0.0), OLIVE);
                gun.part(Vec3::new(0.1, 0.1, 0.04), Vec3::new(0.0, 0.01, -half + 0.03), DARK);
                gun.part(Vec3::new(0.1, 0.1, 0.04), Vec3::new(0.0, 0.01, half - 0.03), DARK);
                gun.part(Vec3::new(0.02, 0.05, 0.02), Vec3::new(0.0, 0.08, -0.1), DARK);
                gun.part(Vec3::new(0.035, 0.08, 0.04), Vec3::new(0.0, -0.08, 0.06), DARK);
                return Hands {
                    right: Vec3::new(0.0, -0.12, 0.06),
                    left: Vec3::new(0.0, -0.06, -0.2),
                };
            }
            // The China Lake: a pump-action tube.
            gun.part(Vec3::new(0.065, 0.1, 0.24), Vec3::new(0.0, 0.0, 0.06), METAL);
            gun.part(Vec3::new(0.06, 0.06, 0.4), Vec3::new(0.0, 0.025, -0.19), METAL);
            gun.part(Vec3::new(0.045, 0.045, 0.4), Vec3::new(0.0, -0.04, -0.17), DARK);
            gun.part(Vec3::new(0.07, 0.06, 0.14), Vec3::new(0.0, -0.04, -0.2), WOOD);
            gun.turned(Vec3::new(0.05, 0.1, 0.24), Vec3::new(0.0, -0.04, half - 0.11), Quat::from_rotation_x(0.15), WOOD);
            gun.part(Vec3::new(0.012, 0.06, 0.012), Vec3::new(0.0, 0.08, -0.28), DARK);
            grip(gun, Vec3::new(0.0, -0.1, 0.12), WOOD);
            Hands {
                right: Vec3::new(0.0, -0.13, 0.13),
                left: Vec3::new(0.0, -0.09, -0.2),
            }
        }
        WeaponClass::RayGun => {
            let red = Color::srgb(0.7, 0.12, 0.1);
            let glow = Color::srgb(0.35, 1.0, 0.35);
            gun.part(Vec3::new(0.07, 0.1, 0.22), Vec3::new(0.0, 0.0, 0.05), red);
            gun.part(Vec3::new(0.04, 0.04, 0.14), Vec3::new(0.0, 0.01, -0.1), STEEL);
            // Glowing rings down the barrel and a dish at the muzzle.
            for z in [-0.06, -0.1, -0.14] {
                gun.part(Vec3::new(0.09, 0.09, 0.015), Vec3::new(0.0, 0.01, z), glow);
            }
            gun.part(Vec3::new(0.12, 0.12, 0.02), Vec3::new(0.0, 0.01, -0.18), STEEL);
            gun.part(Vec3::new(0.02, 0.05, 0.05), Vec3::new(0.0, 0.07, 0.1), red);
            grip(gun, Vec3::new(0.0, -0.1, 0.12), DARK);
            Hands {
                right: Vec3::new(0.0, -0.13, 0.13),
                left: Vec3::new(-0.05, -0.16, 0.1),
            }
        }
        WeaponClass::Thundergun => {
            let brass = Color::srgb(0.55, 0.42, 0.2);
            let glow = Color::srgb(0.35, 0.6, 1.0);
            gun.part(Vec3::new(0.14, 0.15, 0.38), Vec3::new(0.0, 0.0, 0.05), brass);
            // The big turbine mouth.
            gun.part(Vec3::new(0.22, 0.22, 0.1), Vec3::new(0.0, 0.0, -0.22), DARK);
            gun.part(Vec3::new(0.16, 0.16, 0.02), Vec3::new(0.0, 0.0, -0.275), glow);
            // Coils down each side.
            for x in [-0.08, 0.08] {
                for z in [-0.05, 0.03, 0.11] {
                    gun.part(Vec3::new(0.03, 0.12, 0.04), Vec3::new(x, 0.0, z), STEEL);
                }
            }
            gun.part(Vec3::new(0.04, 0.05, 0.2), Vec3::new(0.0, 0.1, 0.03), DARK);
            grip(gun, Vec3::new(0.0, -0.13, 0.12), WOOD);
            gun.part(Vec3::new(0.05, 0.1, 0.05), Vec3::new(0.0, -0.11, -0.1), WOOD);
            Hands {
                right: Vec3::new(0.0, -0.16, 0.13),
                left: Vec3::new(0.0, -0.15, -0.1),
            }
        }
        WeaponClass::Crossbow => {
            gun.part(Vec3::new(0.05, 0.07, 0.48), Vec3::new(0.0, -0.01, 0.02), WOOD);
            gun.part(Vec3::new(0.06, 0.08, 0.1), Vec3::new(0.0, 0.0, -0.02), METAL);
            // The prow, its string drawn back, and a bolt in the groove.
            gun.part(Vec3::new(0.42, 0.03, 0.04), Vec3::new(0.0, 0.01, -0.22), DARK);
            for x in [-0.1, 0.1] {
                gun.turned(
                    Vec3::new(0.008, 0.008, 0.13),
                    Vec3::new(x, 0.03, -0.17),
                    Quat::from_rotation_y(-x.signum()),
                    STEEL,
                );
            }
            gun.part(Vec3::new(0.012, 0.012, 0.34), Vec3::new(0.0, 0.035, -0.08), STEEL);
            gun.part(Vec3::new(0.035, 0.035, 0.14), Vec3::new(0.0, 0.08, 0.02), DARK);
            grip(gun, Vec3::new(0.0, -0.09, 0.1), WOOD);
            Hands {
                right: Vec3::new(0.0, -0.12, 0.1),
                left: Vec3::new(0.0, -0.06, -0.12),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    #[test]
    fn every_weapon_builds_a_model_that_fits_its_class_length() {
        for row in &WEAPONS {
            let mut gun = Gun::default();
            build(&mut gun, row.id, row.class);
            let mesh = gun.mesh();
            let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                panic!("{} has no positions", row.name);
            };
            let half = 0.15 * model_length(row.class);
            for [_, _, z] in positions {
                assert!(z.abs() <= half + 0.02, "{} pokes out past its ends", row.name);
            }
            let colours = mesh.attribute(Mesh::ATTRIBUTE_COLOR).expect("coloured per vertex");
            assert_eq!(colours.len(), mesh.count_vertices());
        }
    }
}
