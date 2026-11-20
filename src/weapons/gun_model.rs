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
//! too: a right hand slashing across the screen. So is each Perk's bottle, in the right hand that
//! drinks it. Equipment is too: the left hand with a frag, a Monkey Bomb or a Claymore in its
//! fist, or empty once it's let go.
//!
//! Each model also marks where its Attachments go: the top of the receiver for an optic, the
//! muzzle, and under the fore-end for an Underbarrel or a Grip. An Extended Mag or Dual Mag
//! replaces the magazine itself, so it comes out in a Reload; a Weapon with no magazine to swap
//! (shells, a cylinder) carries spare rounds on its side instead.

use bevy::math::Affine2;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::weapons::arsenal::{WeaponClass, WeaponId};
use crate::weapons::atlas::{Finish, Paint, UPGRADED_SHIFT, jitter};
use crate::weapons::attachment::Attachment;
use crate::weapons::weapon::UPGRADED_PURPLE;
use crate::economy::perk::Perk;
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
pub const WORLD_SCALE: f32 = 1.5;

const METAL: Paint = Paint::new(Finish::Gunmetal, Color::srgb(0.2, 0.2, 0.22));
const DARK: Paint = Paint::new(Finish::BluedSteel, Color::srgb(0.08, 0.08, 0.09));
const STEEL: Paint = Paint::new(Finish::Gunmetal, Color::srgb(0.55, 0.56, 0.6));
const WOOD: Paint = Paint::new(Finish::Wood, Color::srgb(0.45, 0.27, 0.12));
const OLIVE: Paint = Paint::new(Finish::OlivePaint, Color::srgb(0.3, 0.34, 0.2));
const RUST: Color = Color::srgb(0.6, 0.15, 0.05);
const TAPE: Paint = Paint::new(Finish::Tape, Color::srgb(0.7, 0.62, 0.38));
const SHELLS: Color = Color::srgb(0.75, 0.12, 0.08);
const BRASS: Paint = Paint::new(Finish::Brass, Color::srgb(0.75, 0.6, 0.2));
const SKIN: Paint = Paint::new(Finish::Glove, Color::srgb(0.82, 0.62, 0.46));
const SLEEVE: Paint = Paint::new(Finish::Sleeve, Color::srgb(0.27, 0.31, 0.21));
/// The red dot glowing inside a Red Dot Sight's hood.
const RED_DOT: Color = Color::srgb(1.0, 0.12, 0.06);
/// The blue lens at the front of an ACOG.
const ACOG_LENS: Color = Color::srgb(0.3, 0.5, 0.75);
/// How high a sniper's built-in scope sits: its tube's centre, where the eye looks through.
const SCOPE_CENTRE: f32 = 0.085;
/// How far above an optic's mount its reticle's centre is: the middle of its hood or tube.
const OPTIC_CENTRE: f32 = 0.03;

/// A moving part of a model in the hands: its mesh, and where it sits at rest.
#[derive(Clone)]
pub struct Part {
    pub mesh: Handle<Mesh>,
    pub rest: Transform,
}

/// Every Weapon's model, by `WeaponId`.
/// What the left hand holds while it handles Equipment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handheld {
    /// Nothing: it has let go.
    Empty,
    Frag,
    MonkeyBomb,
    Claymore,
}

impl Handheld {
    pub const ALL: [Handheld; 4] = [Handheld::Empty, Handheld::Frag, Handheld::MonkeyBomb, Handheld::Claymore];
}

#[derive(Resource)]
pub struct GunModels {
    /// In the Player's hands, with the right hand on it, but without its magazine or left hand.
    view: Vec<Handle<Mesh>>,
    magazine: Vec<Option<Part>>,
    /// The left hand on its fore-end. None where it's part of the model, as on the second gun
    /// of a dual-wield.
    left_hand: Vec<Option<Part>>,
    /// Its magazine as an Extended Mag, and as a Dual Mag: two taped side by side.
    extended_magazine: Vec<Option<Part>>,
    dual_magazine: Vec<Option<Part>>,
    /// Each Attachment fitted to it, by its place in `Attachment::ALL`. Not the magazine
    /// Attachments on a Weapon with a magazine: those replace the magazine (see `magazine_with`).
    attachments: Vec<[Option<Part>; 10]>,
    /// As it floats in the world, lying along X, magazine and all.
    world: Vec<Handle<Mesh>>,
    /// The Knife, and the Bowie Knife, in a right hand: the hand at the origin, the blade along −Z.
    pub knife: Handle<Mesh>,
    pub bowie: Handle<Mesh>,
    /// A bare right hand for pushing Planks, its fist at the origin.
    pub palm: Handle<Mesh>,
    /// Each Perk's bottle in a right hand, by its place in `Perk::ALL`: the hand at the origin,
    /// the bottle standing up out of it along +Y, its label facing +Z.
    bottles: [Handle<Mesh>; 4],
    /// The left hand handling Equipment, by place in `Handheld::ALL`: the fist at the origin.
    handheld: [Handle<Mesh>; 4],
    /// Where each model's own sights line up, by place in `WEAPONS`: see `Anchors::sight`.
    sights: Vec<Vec2>,
    /// What every model is drawn in: one material, the vertex colours carrying each part's colour
    /// until the weapon atlas has loaded (see `WeaponAtlasPlugin`), and the atlas after it.
    pub material: Handle<StandardMaterial>,
    /// The same for an Upgraded Weapon: its metal and wood in the atlas's camo, with a faint glow.
    /// Looks just like `material` until the atlas has loaded.
    pub upgraded_material: Handle<StandardMaterial>,
    /// Whether the atlas has loaded and every model wears it.
    pub textured: bool,
}

impl GunModels {
    /// What a model is drawn in: `upgraded_material` for an Upgraded Weapon.
    pub fn material_for(&self, upgraded: bool) -> Handle<StandardMaterial> {
        if upgraded { self.upgraded_material.clone() } else { self.material.clone() }
    }

    pub fn view(&self, id: WeaponId) -> Handle<Mesh> {
        self.view[id as usize].clone()
    }

    /// Its magazine with `attachments` fitted: an Extended Mag's or a Dual Mag's in place of its own.
    pub fn magazine_with(&self, id: WeaponId, attachments: &[Attachment]) -> Option<&Part> {
        let i = id as usize;
        let fitted = if attachments.contains(&Attachment::DualMag) {
            self.dual_magazine[i].as_ref()
        } else if attachments.contains(&Attachment::ExtendedMag) {
            self.extended_magazine[i].as_ref()
        } else {
            None
        };
        fitted.or(self.magazine[i].as_ref())
    }

    /// `attachment`'s model fitted to it, unless it's a magazine that replaces its own.
    pub fn attachment(&self, id: WeaponId, attachment: Attachment) -> Option<&Part> {
        let slot = Attachment::ALL.iter().position(|&a| a == attachment)?;
        self.attachments[id as usize][slot].as_ref()
    }

    /// Where the eye looks along when `id` is aimed with `fitted` on it, across and up from its
    /// origin: a fitted optic's reticle centre, or else its own sights (the tip of the front post
    /// or the middle of a built-in scope).
    pub fn sight_point(&self, id: WeaponId, fitted: &[Attachment]) -> Vec2 {
        let optic = fitted.iter().find(|a| matches!(a, Attachment::RedDotSight | Attachment::AcogSight));
        match optic.and_then(|&optic| self.attachment(id, optic)) {
            Some(part) => part.rest.translation.xy() + Vec2::Y * OPTIC_CENTRE,
            None => self.sights[id as usize],
        }
    }

    pub fn left_hand(&self, id: WeaponId) -> Option<&Part> {
        self.left_hand[id as usize].as_ref()
    }

    pub fn world(&self, id: WeaponId) -> Handle<Mesh> {
        self.world[id as usize].clone()
    }

    pub fn bottle(&self, perk: Perk) -> Handle<Mesh> {
        self.bottles[perk as usize].clone()
    }

    pub fn handheld(&self, held: Handheld) -> Handle<Mesh> {
        self.handheld[held as usize].clone()
    }
}

impl FromWorld for GunModels {
    fn from_world(world: &mut World) -> Self {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        let mut view = Vec::new();
        let mut magazine = Vec::new();
        let mut extended_magazine = Vec::new();
        let mut dual_magazine = Vec::new();
        let mut attachments = Vec::new();
        let mut left_hand = Vec::new();
        let mut world_meshes = Vec::new();
        let mut sight_points = Vec::new();
        for row in &WEAPONS {
            let mut gun = Gun::new(row.class);
            let hands = build(&mut gun, row.id, row.class);
            sight_points.push(gun.sight());
            let mut add = |(mesh, rest): (Mesh, Transform)| Part { mesh: meshes.add(mesh), rest };
            let swapped = gun.magazine.as_ref().map(|(_, rest)| *rest);
            extended_magazine.push(swapped.map(|rest| add((extended(gun.magazine_size), rest))));
            dual_magazine.push(swapped.map(|rest| add((dual(gun.magazine_size), rest))));
            attachments.push(Attachment::ALL.map(|attachment| {
                fitted(attachment, &gun.anchors, swapped.is_some()).map(|mesh| {
                    let rest = Transform::from_translation(gun.anchors.at(attachment));
                    add((mesh, rest))
                })
            }));
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
        let palm = {
            let mut palm = Gun::default();
            hand(&mut palm, Vec3::ZERO, 1.0);
            meshes.add(palm.mesh())
        };
        let bottles = Perk::ALL.map(|perk| meshes.add(bottle(perk)));
        let handheld = Handheld::ALL.map(|held| meshes.add(handheld(held)));
        let plain = StandardMaterial {
            // The vertex colours carry every part's colour.
            base_color: Color::WHITE,
            perceptual_roughness: 0.6,
            metallic: 0.15,
            ..default()
        };
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let material = materials.add(plain.clone());
        let upgraded_material = materials.add(plain);
        Self {
            view,
            magazine,
            extended_magazine,
            dual_magazine,
            attachments,
            left_hand,
            world: world_meshes,
            knife,
            bowie,
            palm,
            bottles,
            handheld,
            sights: sight_points,
            material,
            upgraded_material,
            textured: false,
        }
    }
}

/// A model being built, one box at a time.
#[derive(Default)]
struct Gun {
    mesh: Option<Mesh>,
    /// Its magazine, built on its own around the point where it meets the gun, and where that is.
    magazine: Option<(Mesh, Transform)>,
    /// The magazine's size, for its Extended and Dual Mag versions.
    magazine_size: Vec3,
    anchors: Anchors,
}

/// Where a model's Attachments go.
#[derive(Debug, Clone, Copy, Default)]
struct Anchors {
    /// The top of the receiver, where an optic's base sits.
    optic: Vec3,
    /// Where its own sights line up, across and up from the origin: the tip of the front post, which
    /// sits level with the rear notch's rim, or the middle of a built-in scope. Set by whatever
    /// builds the sights, so it can't drift from them.
    sight: Option<Vec2>,
    /// The middle of the muzzle's face, and the barrel's width and height there.
    muzzle: Vec3,
    bore: Vec2,
    /// Under the fore-end, where an Underbarrel or a Grip hangs from.
    under: Vec3,
    /// The left of the receiver, where spare rounds ride on a Weapon with no magazine to swap.
    side: Vec3,
}

impl Anchors {
    /// Where `attachment` is fitted.
    fn at(&self, attachment: Attachment) -> Vec3 {
        match attachment {
            Attachment::RedDotSight | Attachment::AcogSight => self.optic,
            Attachment::LongBarrel | Attachment::RapidFire => self.muzzle,
            Attachment::ExtendedMag | Attachment::DualMag => self.side,
            Attachment::Masterkey | Attachment::GrenadeLauncher | Attachment::Flamethrower | Attachment::Grip => {
                self.under
            }
        }
    }
}

impl Gun {
    /// An empty model of `class`, its Attachments where most models of its length have them.
    fn new(class: WeaponClass) -> Self {
        let half = 0.15 * model_length(class);
        Self {
            anchors: Anchors {
                sight: None,
                optic: Vec3::new(0.0, 0.05, 0.0),
                muzzle: Vec3::new(0.0, 0.01, -half),
                bore: Vec2::splat(0.025),
                under: Vec3::new(0.0, -0.045, -0.2),
                side: Vec3::new(-0.033, 0.0, 0.03),
            },
            ..default()
        }
    }

    /// A `size` box centred `at`, in `paint`.
    fn part(&mut self, size: Vec3, at: Vec3, paint: impl Into<Paint>) -> &mut Self {
        self.turned(size, at, Quat::IDENTITY, paint)
    }

    /// A `size` box centred `at`, turned by `rotation`, in `paint`: its vertex colours the paint's
    /// own colour, and its UVs in its finish's tile of the weapon atlas.
    fn turned(&mut self, size: Vec3, at: Vec3, rotation: Quat, paint: impl Into<Paint>) -> &mut Self {
        let paint = paint.into();
        let mut part = Mesh::from(Cuboid::from_size(size));
        let uvs: Vec<[f32; 2]> = match (part.attribute(Mesh::ATTRIBUTE_NORMAL), part.attribute(Mesh::ATTRIBUTE_UV_0)) {
            (Some(VertexAttributeValues::Float32x3(normals)), Some(VertexAttributeValues::Float32x2(corners))) => normals
                .iter()
                .zip(corners)
                .map(|(n, corner)| {
                    // The face's width and height in metres, along the UV's own axes.
                    let face = if n[0] != 0.0 {
                        Vec2::new(size.z, size.y)
                    } else if n[1] != 0.0 {
                        Vec2::new(size.x, size.z)
                    } else {
                        Vec2::new(size.x, size.y)
                    };
                    let salt = (n[0] * 1.0 + n[1] * 2.0 + n[2] * 3.0).abs();
                    let patch = Vec2::new(jitter(at, salt), jitter(at, salt + 7.0));
                    paint.finish.place(Vec2::from(*corner), face, patch).to_array()
                })
                .collect(),
            _ => unreachable!("a cuboid carries normals and UVs"),
        };
        part.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        let mut part = part.with_generated_tangents().expect("a cuboid has everything tangents need").transformed_by(Transform {
            translation: at,
            rotation,
            ..default()
        });
        let colour = LinearRgba::from(paint.colour).to_f32_array();
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

    /// Where its sights line up. A Weapon that can't aim has none built, so it gets the height an
    /// optic's reticle would have.
    fn sight(&self) -> Vec2 {
        self.anchors.sight.unwrap_or(self.anchors.optic.xy() + Vec2::Y * OPTIC_CENTRE)
    }

    /// A front sight: a `size` box centred `at`, whose top is where the sights line up. The first
    /// one built counts (a dual-wield's second gun has its own, which doesn't).
    fn front_sight(&mut self, size: Vec3, at: Vec3, colour: impl Into<Paint>) -> &mut Self {
        self.anchors.sight.get_or_insert(Vec2::new(at.x, at.y + size.y / 2.0));
        self.part(size, at, colour)
    }

    /// A rear notch at `z` on `x`: a floor from `base` up, and an ear either side of the post's
    /// gap, rising to `tip`, level with the front sight's top so the post shows inside it.
    fn rear_notch(&mut self, x: f32, base: f32, tip: f32, z: f32) -> &mut Self {
        let floor = tip - 0.018;
        self.part(Vec3::new(0.03, floor - base, 0.012), Vec3::new(x, (base + floor) / 2.0, z), DARK);
        for side in [-1.0, 1.0] {
            self.part(Vec3::new(0.008, tip - floor, 0.012), Vec3::new(x + side * 0.0125, (floor + tip) / 2.0, z), DARK);
        }
        self
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

/// The shape of a Perk's bottle: its glass, label and cap colours, how wide and tall its body and
/// neck are, and the mark on the front of its label.
struct BottleLook {
    glass: Color,
    label: Color,
    cap: Paint,
    body: Vec2,
    neck: Vec2,
    mark: fn(&mut Gun, Vec3),
}

fn bottle_look(perk: Perk) -> BottleLook {
    match perk {
        // A big, heavy red bottle with a long neck, and a white shield on its label.
        Perk::Juggernog => BottleLook {
            glass: Color::srgb(0.5, 0.04, 0.04),
            label: Color::srgb(0.85, 0.82, 0.75),
            cap: BRASS,
            body: Vec2::new(0.075, 0.17),
            neck: Vec2::new(0.032, 0.08),
            mark: |gun, front| {
                gun.part(Vec3::new(0.034, 0.03, 0.004), front + Vec3::Y * 0.004, SHELLS);
                gun.part(Vec3::new(0.02, 0.012, 0.004), front - Vec3::Y * 0.016, SHELLS);
            },
        },
        // A slim green bottle with a lightning bolt across its lime label.
        Perk::SpeedCola => BottleLook {
            glass: Color::srgb(0.05, 0.35, 0.1),
            label: Color::srgb(0.55, 0.85, 0.3),
            cap: STEEL,
            body: Vec2::new(0.058, 0.15),
            neck: Vec2::new(0.026, 0.09),
            mark: |gun, front| {
                let bolt = Color::srgb(0.95, 0.85, 0.15);
                gun.turned(Vec3::new(0.008, 0.042, 0.004), front, Quat::from_rotation_z(-0.6), bolt);
            },
        },
        // A stubby amber bottle with two bullets on its orange label.
        Perk::DoubleTap => BottleLook {
            glass: Color::srgb(0.35, 0.18, 0.05),
            label: Color::srgb(0.95, 0.55, 0.1),
            cap: Color::srgb(0.6, 0.1, 0.05).into(),
            body: Vec2::new(0.068, 0.12),
            neck: Vec2::new(0.03, 0.05),
            mark: |gun, front| {
                for x in [-0.009, 0.009] {
                    gun.part(Vec3::new(0.008, 0.03, 0.004), front + Vec3::X * x, DARK);
                }
            },
        },
        // A squat, round pale-blue bottle with a blue cross on its white label.
        Perk::QuickRevive => BottleLook {
            glass: Color::srgb(0.3, 0.6, 0.9),
            label: Color::srgb(0.92, 0.95, 1.0),
            cap: Color::srgb(0.92, 0.95, 1.0).into(),
            body: Vec2::new(0.085, 0.11),
            neck: Vec2::new(0.034, 0.045),
            mark: |gun, front| {
                let cross = Color::srgb(0.15, 0.4, 0.85);
                gun.part(Vec3::new(0.03, 0.01, 0.004), front, cross);
                gun.part(Vec3::new(0.01, 0.03, 0.004), front, cross);
            },
        },
    }
}

/// The left hand at the origin with `held` in its fist, its top above the fist.
fn handheld(held: Handheld) -> Mesh {
    let mut gun = Gun::default();
    hand(&mut gun, Vec3::ZERO, -1.0);
    // The top of the fist `hand` makes.
    let top = 0.0425;
    match held {
        Handheld::Empty => {}
        Handheld::Frag => {
            // An olive body with a neck, a steel lever down its side and the pin's ring.
            gun.part(Vec3::new(0.06, 0.08, 0.06), Vec3::Y * (top + 0.02), OLIVE);
            gun.part(Vec3::new(0.03, 0.02, 0.03), Vec3::Y * (top + 0.07), STEEL);
            gun.part(Vec3::new(0.012, 0.075, 0.03), Vec3::new(0.034, top + 0.03, 0.0), STEEL);
            gun.part(Vec3::new(0.03, 0.006, 0.012), Vec3::new(-0.01, top + 0.085, 0.0), STEEL);
        }
        Handheld::MonkeyBomb => {
            // A brown monkey with a pale face and a pair of brass cymbals.
            gun.part(Vec3::new(0.06, 0.07, 0.05), Vec3::Y * (top + 0.015), WOOD);
            gun.part(Vec3::new(0.05, 0.045, 0.045), Vec3::Y * (top + 0.075), WOOD);
            gun.part(Vec3::new(0.034, 0.025, 0.01), Vec3::new(0.0, top + 0.07, 0.025), TAPE);
            for side in [-1.0, 1.0] {
                gun.part(Vec3::new(0.008, 0.05, 0.05), Vec3::new(side * 0.045, top + 0.03, 0.0), BRASS);
            }
        }
        Handheld::Claymore => {
            // A curved-face slab on two legs, its face to the front.
            gun.part(Vec3::new(0.15, 0.06, 0.03), Vec3::Y * (top + 0.025), OLIVE);
            gun.part(Vec3::new(0.12, 0.04, 0.012), Vec3::new(0.0, top + 0.025, -0.02), DARK);
            for x in [-0.05, 0.05] {
                gun.part(Vec3::new(0.01, 0.03, 0.01), Vec3::new(x, top - 0.005, 0.0), STEEL);
            }
        }
    }
    gun.mesh()
}

/// A Perk's bottle in a right hand at the origin: its base in the fist, its label above it.
fn bottle(perk: Perk) -> Mesh {
    let look = bottle_look(perk);
    let mut gun = Gun::default();
    hand(&mut gun, Vec3::ZERO, 1.0);
    // The top of the fist `hand` makes.
    let fist_top = 0.0425;
    let (width, height) = (look.body.x, look.body.y);
    let bottom = fist_top - 0.4 * height;
    let top = bottom + height;
    gun.part(Vec3::new(width, height, width), Vec3::Y * (bottom + height / 2.0), look.glass);
    // A rounded shoulder up into the neck.
    let shoulder = Vec3::new(width * 0.7, 0.025, width * 0.7);
    gun.part(shoulder, Vec3::Y * (top + shoulder.y / 2.0), look.glass);
    let neck_base = top + shoulder.y;
    let neck = Vec3::new(look.neck.x, look.neck.y, look.neck.x);
    gun.part(neck, Vec3::Y * (neck_base + neck.y / 2.0), look.glass);
    gun.part(Vec3::new(neck.x + 0.008, 0.014, neck.x + 0.008), Vec3::Y * (neck_base + neck.y + 0.007), look.cap);
    // The label, wrapped round what shows of the body above the fist.
    let shown = top - fist_top;
    let label = Vec3::new(width + 0.004, (shown * 0.6).min(0.06), width + 0.004);
    let label_at = Vec3::Y * (fist_top + shown / 2.0);
    gun.part(label, label_at, look.label);
    (look.mark)(&mut gun, label_at + Vec3::Z * (label.z / 2.0 + 0.002));
    gun.mesh()
}

/// A pistol's grip, angled back: `at` is its middle.
fn grip(gun: &mut Gun, at: Vec3, colour: impl Into<Paint>) {
    gun.turned(Vec3::new(0.045, 0.12, 0.06), at, Quat::from_rotation_x(-0.25), colour);
}

/// A magazine hanging under the receiver at `z`, `length` long, curving forward by `curve`.
fn magazine(gun: &mut Gun, z: f32, length: f32, curve: f32, colour: impl Into<Paint> + Copy) {
    let rest = Transform::from_xyz(0.0, -0.04, z).with_rotation(Quat::from_rotation_x(curve));
    detachable(gun, Vec3::new(0.035, length, 0.06), rest, colour);
}

/// The magazine: a `size` box hanging down from `rest`, where it meets the gun.
fn detachable(gun: &mut Gun, size: Vec3, rest: Transform, colour: impl Into<Paint>) {
    let mut magazine = Gun::default();
    magazine.part(size, Vec3::new(0.0, -size.y / 2.0, 0.0), colour);
    gun.magazine = Some((magazine.mesh(), rest));
    gun.magazine_size = size;
}

/// An Extended Mag in place of a `size` magazine: the same, but much longer.
fn extended(size: Vec3) -> Mesh {
    let long = size * Vec3::new(1.0, 1.6, 1.0);
    let mut magazine = Gun::default();
    magazine.part(long, Vec3::new(0.0, -long.y / 2.0, 0.0), DARK);
    // A base plate, to tell it from the standard one.
    magazine.part(Vec3::new(size.x + 0.01, 0.015, size.z + 0.01), Vec3::new(0.0, -long.y, 0.0), METAL);
    magazine.mesh()
}

/// A Dual Mag in place of a `size` magazine: a second magazine taped alongside, upside down.
fn dual(size: Vec3) -> Mesh {
    let mut magazine = Gun::default();
    let beside = -(size.x + 0.006);
    magazine.part(size, Vec3::new(0.0, -size.y / 2.0, 0.0), DARK);
    magazine.part(size, Vec3::new(beside, -size.y / 2.0 - 0.02, 0.0), DARK);
    magazine.part(
        Vec3::new(size.x * 2.0 + 0.016, 0.025, size.z + 0.008),
        Vec3::new(beside / 2.0, -size.y * 0.55, 0.0),
        TAPE,
    );
    magazine.mesh()
}

/// `attachment`'s model, built around where it's fitted (see `Anchors`). None for a magazine
/// Attachment on a Weapon whose magazine `swaps`: that replaces the magazine instead.
fn fitted(attachment: Attachment, anchors: &Anchors, swaps: bool) -> Option<Mesh> {
    let bore = anchors.bore.extend(0.0);
    let mut model = Gun::default();
    match attachment {
        Attachment::RedDotSight => {
            model.part(Vec3::new(0.03, 0.012, 0.05), Vec3::new(0.0, 0.006, 0.0), DARK);
            // A square hood with the red dot glowing inside it.
            model.part(Vec3::new(0.036, 0.036, 0.028), Vec3::new(0.0, 0.03, 0.0), DARK);
            model.part(Vec3::new(0.024, 0.024, 0.032), Vec3::new(0.0, OPTIC_CENTRE, 0.0), RED_DOT);
        }
        Attachment::AcogSight => {
            model.part(Vec3::new(0.03, 0.012, 0.06), Vec3::new(0.0, 0.006, 0.0), DARK);
            model.part(Vec3::new(0.036, 0.036, 0.11), Vec3::new(0.0, 0.03, 0.0), DARK);
            model.part(Vec3::new(0.048, 0.048, 0.035), Vec3::new(0.0, 0.03, -0.06), DARK);
            model.part(Vec3::new(0.042, 0.042, 0.02), Vec3::new(0.0, 0.03, 0.055), DARK);
            model.part(Vec3::new(0.034, 0.034, 0.004), Vec3::new(0.0, OPTIC_CENTRE, -0.079), ACOG_LENS);
        }
        Attachment::LongBarrel => {
            model.part(Vec3::new(bore.x * 0.85, bore.y * 0.85, 0.13), Vec3::new(0.0, 0.0, -0.065), DARK);
            model.part(Vec3::new(bore.x * 1.3, bore.y * 1.3, 0.03), Vec3::new(0.0, 0.0, -0.13), METAL);
        }
        Attachment::RapidFire => {
            model.part(Vec3::new(bore.x * 1.5, bore.y * 1.5, 0.06), Vec3::new(0.0, 0.0, -0.03), METAL);
            for z in [-0.015, -0.045] {
                model.part(Vec3::new(bore.x * 1.65, bore.y * 1.65, 0.008), Vec3::new(0.0, 0.0, z), DARK);
            }
        }
        Attachment::ExtendedMag | Attachment::DualMag if swaps => return None,
        Attachment::ExtendedMag | Attachment::DualMag => {
            // A holder of spare rounds on the side, longer for a Dual Mag.
            let long = if attachment == Attachment::DualMag { 0.16 } else { 0.1 };
            model.part(Vec3::new(0.014, 0.05, long), Vec3::new(-0.007, 0.0, 0.0), DARK);
            for y in [-0.012, 0.012] {
                model.part(Vec3::new(0.008, 0.016, long - 0.012), Vec3::new(-0.016, y, 0.0), SHELLS);
            }
        }
        Attachment::Masterkey => {
            // A sawn-off pump shotgun clamped under the barrel.
            model.part(Vec3::new(0.02, 0.012, 0.12), Vec3::new(0.0, -0.006, 0.0), DARK);
            model.part(Vec3::new(0.03, 0.03, 0.24), Vec3::new(0.0, -0.027, -0.06), METAL);
            model.part(Vec3::new(0.042, 0.036, 0.08), Vec3::new(0.0, -0.03, -0.05), DARK);
            model.part(Vec3::new(0.036, 0.042, 0.06), Vec3::new(0.0, -0.03, 0.08), DARK);
        }
        Attachment::GrenadeLauncher => {
            model.part(Vec3::new(0.02, 0.012, 0.1), Vec3::new(0.0, -0.006, 0.0), DARK);
            model.part(Vec3::new(0.048, 0.048, 0.17), Vec3::new(0.0, -0.036, -0.05), DARK);
            model.part(Vec3::new(0.036, 0.042, 0.06), Vec3::new(0.0, -0.034, 0.06), METAL);
            model.part(Vec3::new(0.01, 0.03, 0.014), Vec3::new(0.0, -0.07, 0.07), DARK);
        }
        Attachment::Flamethrower => {
            model.part(Vec3::new(0.02, 0.012, 0.1), Vec3::new(0.0, -0.006, 0.0), DARK);
            model.part(Vec3::new(0.022, 0.022, 0.2), Vec3::new(0.0, -0.022, -0.09), METAL);
            model.part(Vec3::new(0.03, 0.03, 0.016), Vec3::new(0.0, -0.022, -0.19), Color::srgb(1.0, 0.55, 0.1));
            model.part(Vec3::new(0.05, 0.05, 0.13), Vec3::new(0.0, -0.05, 0.04), RUST);
        }
        Attachment::Grip => {
            model.part(Vec3::new(0.03, 0.012, 0.045), Vec3::new(0.0, -0.006, 0.0), DARK);
            model.part(Vec3::new(0.026, 0.085, 0.032), Vec3::new(0.0, -0.05, 0.0), DARK);
        }
    }
    Some(model.mesh())
}

/// Iron sights standing on `y` at `x`: a post at the front and a notch at the back, the post's
/// tip level with the notch's rim. Aiming puts the tip at the middle of the screen.
fn sights(gun: &mut Gun, x: f32, y: f32, front: f32, back: f32) {
    tall_sights(gun, x, y + 0.03, (y, front), (y, back));
}

/// Iron sights whose post tip and notch rim reach `tip`, the post standing on `front.0` at
/// `front.1` and the notch on `back.0` at `back.1`: for a model whose top is higher up.
fn tall_sights(gun: &mut Gun, x: f32, tip: f32, front: (f32, f32), back: (f32, f32)) {
    let post = tip - front.0;
    gun.front_sight(Vec3::new(0.012, post, 0.012), Vec3::new(x, front.0 + post / 2.0, front.1), DARK);
    gun.rear_notch(x, back.0, tip, back.1);
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
                    gun.front_sight(Vec3::new(0.012, 0.025, 0.012), Vec3::new(x, 0.055, -0.14), STEEL);
                    gun.rear_notch(x, 0.025, 0.0675, 0.12);
                } else {
                    gun.part(Vec3::new(0.055, 0.06, 0.3), Vec3::new(x, 0.02, 0.0), METAL);
                    gun.part(Vec3::new(0.05, 0.04, 0.2), Vec3::new(x, -0.03, -0.04), DARK);
                    gun.part(Vec3::new(0.03, 0.03, 0.02), Vec3::new(x, 0.02, -0.15), DARK);
                    sights(gun, x, 0.05, -0.13, 0.13);
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
            gun.anchors.optic = Vec3::new(0.0, 0.05, 0.03);
            gun.anchors.muzzle = Vec3::new(0.0, 0.02, -0.16);
            gun.anchors.bore = Vec2::splat(0.03);
            gun.anchors.side = Vec3::new(-0.028, -0.005, 0.0);
            if id == WeaponId::Python {
                gun.anchors.optic = Vec3::new(0.0, 0.045, -0.03);
                gun.anchors.muzzle = Vec3::new(0.0, 0.025, -0.15);
                gun.anchors.bore = Vec2::new(0.035, 0.04);
                gun.anchors.side = Vec3::new(-0.03, 0.01, 0.04);
            }
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
            // A little post on the guard to line up.
            gun.front_sight(Vec3::new(0.012, 0.02, 0.012), Vec3::new(0.0, 0.0325, -0.03), STEEL);
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
            gun.anchors.optic = Vec3::new(0.0, if id == WeaponId::Spectre { 0.07 } else { 0.045 }, 0.03);
            gun.anchors.under = Vec3::new(0.0, -0.045, -0.12);
            // A folding stock: two thin rails and a butt plate.
            for x in [-0.025, 0.025] {
                gun.part(Vec3::new(0.012, 0.012, 0.13), Vec3::new(x, -0.01, half - 0.07), DARK);
            }
            gun.part(Vec3::new(0.06, 0.08, 0.02), Vec3::new(0.0, -0.02, half - 0.01), DARK);
            sights(gun, 0.0, 0.045, -0.15, 0.1);
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
                    tall_sights(gun, 0.0, 0.125, (0.075, -0.3), (0.075, 0.22));
                    gun.anchors.optic = Vec3::new(0.0, 0.125, -0.05);
                    gun.anchors.muzzle = Vec3::new(0.0, 0.0, -half - 0.01);
                    gun.anchors.under = Vec3::new(0.0, -0.075, -0.2);
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
                    gun.anchors.under = Vec3::new(0.0, -0.04, -0.2);
                    // On top of the carry handle, or the AUG's rail.
                    gun.anchors.optic = Vec3::new(0.0, if id == WeaponId::Famas { 0.105 } else { 0.1 }, 0.0);
                    if id == WeaponId::Famas {
                        // The carry handle running along the top.
                        gun.part(Vec3::new(0.025, 0.03, 0.32), Vec3::new(0.0, 0.09, 0.0), DARK);
                        gun.part(Vec3::new(0.02, 0.04, 0.02), Vec3::new(0.0, 0.065, -0.15), DARK);
                    } else {
                        gun.part(Vec3::new(0.04, 0.04, 0.12), Vec3::new(0.0, 0.08, 0.0), DARK);
                    }
                    let tip = if id == WeaponId::Famas { 0.105 } else { 0.1 };
                    tall_sights(gun, 0.0, tip, (0.04, -0.22), (0.055, 0.2));
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
            sights(gun, 0.0, 0.05, -half + 0.06, 0.08);
            gun.anchors.under = Vec3::new(0.0, -0.045, -0.24);
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
                sights(gun, 0.0, 0.035, -0.33, 0.12);
                gun.anchors.optic = Vec3::new(0.0, 0.04, 0.12);
                gun.anchors.muzzle = Vec3::new(0.0, 0.02, -0.34);
                gun.anchors.bore = Vec2::new(0.064, 0.03);
                gun.anchors.under = Vec3::new(0.0, -0.03, -0.12);
                gun.anchors.side = Vec3::new(-0.03, 0.0, 0.12);
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
            sights(gun, 0.0, 0.045, -half + 0.03, 0.1);
            gun.anchors.optic = Vec3::new(0.0, 0.05, 0.06);
            gun.anchors.muzzle = Vec3::new(0.0, 0.025, -0.33);
            gun.anchors.bore = Vec2::splat(0.035);
            gun.anchors.under = Vec3::new(0.0, -0.05, -0.2);
            gun.anchors.side = Vec3::new(-0.03, 0.0, 0.04);
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
            gun.part(Vec3::new(0.035, 0.035, 0.22), Vec3::new(0.0, SCOPE_CENTRE, 0.0), DARK);
            gun.anchors.sight = Some(Vec2::new(0.0, SCOPE_CENTRE));
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
            gun.anchors.optic = Vec3::new(0.0, 0.105, 0.0);
            gun.anchors.under = Vec3::new(0.0, -0.045, -0.24);
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
            // Over the carry handle's top.
            tall_sights(gun, 0.0, 0.115, (0.03, -0.38), (0.06, 0.15));
            // Behind the carry handle.
            gun.anchors.optic = Vec3::new(0.0, 0.06, 0.09);
            gun.anchors.bore = Vec2::splat(0.035);
            gun.anchors.under = Vec3::new(0.0, -0.05, -0.28);
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
                gun.front_sight(Vec3::new(0.02, 0.05, 0.02), Vec3::new(0.0, 0.08, -0.1), DARK);
                gun.rear_notch(0.0, 0.055, 0.105, 0.2);
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
            gun.front_sight(Vec3::new(0.012, 0.06, 0.012), Vec3::new(0.0, 0.08, -0.28), DARK);
            gun.rear_notch(0.0, 0.05, 0.11, 0.12);
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
            gun.front_sight(Vec3::new(0.014, 0.04, 0.014), Vec3::new(0.0, 0.075, -0.1), red);
            gun.rear_notch(0.0, 0.05, 0.095, 0.1);
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
            tall_sights(gun, 0.0, 0.14, (0.11, -0.22), (0.075, 0.1));
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
            tall_sights(gun, 0.0, 0.075, (0.025, -0.22), (0.04, 0.06));
            grip(gun, Vec3::new(0.0, -0.09, 0.1), WOOD);
            Hands {
                right: Vec3::new(0.0, -0.12, 0.1),
                left: Vec3::new(0.0, -0.06, -0.12),
            }
        }
    }
}

impl GunModels {
    /// Every mesh in it.
    fn all_meshes(&self) -> Vec<Handle<Mesh>> {
        let mut all: Vec<Handle<Mesh>> = Vec::new();
        all.extend(self.view.iter().cloned());
        all.extend(self.world.iter().cloned());
        all.extend([self.knife.clone(), self.bowie.clone(), self.palm.clone()]);
        all.extend(self.bottles.iter().cloned());
        all.extend(self.handheld.iter().cloned());
        let parts = self.magazine.iter().chain(&self.left_hand).chain(&self.extended_magazine).chain(&self.dual_magazine);
        all.extend(parts.flatten().map(|part| part.mesh.clone()));
        all.extend(self.attachments.iter().flat_map(|row| row.iter().flatten()).map(|part| part.mesh.clone()));
        all
    }
}

/// Where the weapon atlas's three maps are under `assets/`.
const ATLAS_FILES: [&str; 3] = ["weapons/atlas/color.ktx2", "weapons/atlas/normal.ktx2", "weapons/atlas/orm.ktx2"];
/// How much of the camo's purple an Upgraded Weapon glows: faint, so it still reads as a gun.
const CAMO_GLOW: f32 = 0.5;

/// Takes a mesh's vertex colours off every vertex in a textured finish, so the atlas shows as it
/// is and not tinted by the flat palette. Plain parts keep their colour.
fn whiten(mesh: &mut Mesh) {
    let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
        return;
    };
    let textured: Vec<bool> = uvs.iter().map(|&uv| Finish::at(Vec2::from(uv)).is_some_and(|finish| finish != Finish::Plain)).collect();
    if let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        for (colour, textured) in colours.iter_mut().zip(textured) {
            if textured {
                *colour = [1.0; 4];
            }
        }
    }
}

/// How far the weapon atlas has got.
#[derive(Default)]
enum Atlas {
    #[default]
    Unasked,
    Loading([Handle<Image>; 3]),
    /// Worn, or given up on for good: the models stay in their vertex colours.
    Settled,
}

/// Loads the weapon atlas in the real game and, once all three maps have loaded, dresses every
/// model in it. With a map missing or failed the models keep today's vertex colours; headless runs
/// never add this plugin, so never load.
pub struct WeaponAtlasPlugin;

impl Plugin for WeaponAtlasPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, dress_guns.run_if(resource_exists::<GunModels>));
    }
}

fn dress_guns(
    mut atlas: Local<Atlas>,
    asset_server: Res<AssetServer>,
    mut guns: ResMut<GunModels>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use crate::maps::map::textures::{Dress, Load, dress, load_of, on_disk};
    match &*atlas {
        Atlas::Settled => {}
        Atlas::Unasked => {
            *atlas = if ATLAS_FILES.iter().all(|file| on_disk(file)) {
                Atlas::Loading(ATLAS_FILES.map(|file| asset_server.load(file)))
            } else {
                debug!("weapon atlas files missing, the guns stay in their vertex colours");
                Atlas::Settled
            };
        }
        Atlas::Loading(images) => {
            let loads: [Load; 3] = images.each_ref().map(|image| load_of(&asset_server, image));
            match dress(true, &loads) {
                Dress::Wait => {}
                Dress::Palette => *atlas = Atlas::Settled,
                Dress::Textured => {
                    let [color, normal, orm] = images.clone();
                    for (handle, upgraded) in [(guns.material.clone(), false), (guns.upgraded_material.clone(), true)] {
                        let Some(mut material) = materials.get_mut(&handle) else { continue };
                        material.base_color_texture = Some(color.clone());
                        material.normal_map_texture = Some(normal.clone());
                        material.metallic_roughness_texture = Some(orm.clone());
                        material.occlusion_texture = Some(orm.clone());
                        // The ORM map's green and blue are the roughness and metal, as they are.
                        material.perceptual_roughness = 1.0;
                        material.metallic = 1.0;
                        if upgraded {
                            // The lower half of the atlas: the same finishes with metal and wood in camo.
                            material.uv_transform = Affine2::from_translation(UPGRADED_SHIFT);
                            material.emissive_texture = Some(color.clone());
                            material.emissive = LinearRgba::from(UPGRADED_PURPLE) * CAMO_GLOW;
                        }
                    }
                    for handle in guns.all_meshes() {
                        if let Some(mut mesh) = meshes.get_mut(&handle) {
                            whiten(&mut mesh);
                        }
                    }
                    guns.textured = true;
                    debug!("weapon atlas worn by every model");
                    *atlas = Atlas::Settled;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    #[test]
    fn every_handheld_builds_and_the_empty_hand_is_the_smallest() {
        let sizes = Handheld::ALL.map(|held| handheld(held).count_vertices());
        assert!(sizes[1..].iter().all(|&n| n > sizes[0]));
        assert_eq!(Handheld::ALL[Handheld::Claymore as usize], Handheld::Claymore);
    }

    #[test]
    fn every_perk_has_its_own_bottle_shape() {
        let shapes = Perk::ALL.map(|perk| {
            let look = bottle_look(perk);
            (look.body, look.neck)
        });
        for (i, a) in shapes.iter().enumerate() {
            assert!(shapes[i + 1..].iter().all(|b| b != a), "{:?} shares a shape", Perk::ALL[i]);
        }
    }

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

    fn uvs(mesh: &Mesh) -> Vec<[f32; 2]> {
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
            panic!("no UVs");
        };
        uvs.clone()
    }

    #[test]
    fn every_part_of_every_model_has_uvs_in_a_finish_tile_and_tangents() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<GunModels>();
        let guns = app.world().resource::<GunModels>();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let all = guns.all_meshes();
        assert!(all.len() > WEAPONS.len() * 2);
        for handle in all {
            let mesh = meshes.get(&handle).unwrap();
            assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
            for uv in uvs(mesh) {
                let finish = Finish::at(Vec2::from(uv)).unwrap_or_else(|| panic!("{uv:?} is in no finish's tile"));
                let (min, max) = finish.inner();
                assert!(Vec2::from(uv).cmpge(min - 1e-5).all() && Vec2::from(uv).cmple(max + 1e-5).all());
            }
        }
    }

    #[test]
    fn whitening_clears_textured_finishes_and_keeps_plain_colours() {
        let mut gun = Gun::default();
        gun.part(Vec3::splat(0.1), Vec3::ZERO, WOOD).part(Vec3::splat(0.1), Vec3::X, RED_DOT);
        let mut mesh = gun.mesh();
        whiten(&mut mesh);
        let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!() };
        let red = LinearRgba::from(RED_DOT).to_f32_array();
        assert!(colours[..24].iter().all(|c| *c == [1.0; 4]));
        assert!(colours[24..].iter().all(|c| *c == red));
    }

    #[test]
    fn the_pieces_of_a_model_keep_the_palette_colours_before_the_atlas_loads() {
        let mut gun = Gun::default();
        gun.part(Vec3::splat(0.1), Vec3::ZERO, SKIN);
        let mesh = gun.mesh();
        let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!() };
        assert_eq!(colours[0], LinearRgba::from(Color::srgb(0.82, 0.62, 0.46)).to_f32_array());
    }

    fn positions(mesh: &Mesh) -> Vec<[f32; 3]> {
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("no positions");
        };
        positions.clone()
    }

    /// The middle of the box that bounds the vertices of `mesh` painted `colour`.
    fn centre_of(mesh: &Mesh, colour: Color) -> Vec2 {
        let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else {
            panic!("not coloured per vertex");
        };
        let wanted = LinearRgba::from(colour).to_f32_array();
        let (mut low, mut high) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for ([x, y, _], _) in positions(mesh).into_iter().zip(colours).filter(|(_, c)| **c == wanted) {
            low = low.min(Vec2::new(x, y));
            high = high.max(Vec2::new(x, y));
        }
        assert!(low.x <= high.x, "nothing is painted {colour:?}");
        (low + high) / 2.0
    }

    #[test]
    fn every_weapons_sight_point_is_where_its_sights_are() {
        for row in &WEAPONS {
            let mut gun = Gun::new(row.class);
            build(&mut gun, row.id, row.class);
            let sight = gun.sight();
            let points = positions(&gun.mesh());
            if row.class == WeaponClass::Sniper {
                // Through a built-in scope: its tube's centre, with the scope's body round it.
                assert_eq!(sight, Vec2::new(0.0, SCOPE_CENTRE), "{}", row.name);
                assert!(points.iter().any(|p| p[1] > sight.y + 0.015 && p[0].abs() < 0.03), "{} scope top", row.name);
                assert!(points.iter().any(|p| p[1] < sight.y - 0.015 && p[0].abs() < 0.03), "{} scope bottom", row.name);
            } else {
                // Over the iron sights: the post's tip is a corner of the model at the sight point.
                let tip = points.iter().filter(|p| (p[1] - sight.y).abs() < 1e-5 && (p[0] - sight.x).abs() <= 0.01);
                assert!(tip.count() >= 4, "{} has no front post tipping at {sight}", row.name);
                assert!(gun.anchors.sight.is_some(), "{} never built sights", row.name);
            }
        }
    }

    #[test]
    fn an_optic_fitted_to_any_weapon_has_its_reticle_at_the_sight_point() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<GunModels>();
        let guns = app.world().resource::<GunModels>();
        let meshes = app.world().resource::<Assets<Mesh>>();
        for row in &WEAPONS {
            for (optic, reticle) in [(Attachment::RedDotSight, RED_DOT), (Attachment::AcogSight, ACOG_LENS)] {
                let Some(part) = guns.attachment(row.id, optic) else {
                    continue;
                };
                let centre = centre_of(meshes.get(&part.mesh).unwrap(), reticle);
                let sight = guns.sight_point(row.id, &[optic]);
                let at = part.rest.translation.xy() + centre;
                assert!((at - sight).length() < 1e-5, "{}'s {}: reticle at {at}, sight point {sight}", row.name, optic.name());
            }
            // Without one, the iron sights.
            assert_eq!(guns.sight_point(row.id, &[Attachment::Grip]), guns.sights[row.id as usize], "{}", row.name);
        }
    }

    #[test]
    fn every_attachment_a_weapon_can_get_has_a_model_on_it() {
        use crate::weapons::attachment::mounts;

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<GunModels>();
        let guns = app.world().resource::<GunModels>();
        for row in &WEAPONS {
            for &mount in mounts(row.id.upgraded()) {
                for attachment in Attachment::ALL.into_iter().filter(|a| a.mount() == mount) {
                    let shown = match guns.magazine_with(row.id, &[]) {
                        Some(own) if attachment.mount() == crate::weapons::attachment::Mount::Magazine => {
                            guns.magazine_with(row.id, &[attachment]).is_some_and(|fitted| fitted.mesh != own.mesh)
                        }
                        _ => guns.attachment(row.id, attachment).is_some(),
                    };
                    assert!(shown, "{} has no {}", row.name, attachment.name());
                }
            }
        }
    }
}
