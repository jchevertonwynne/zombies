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

use bevy::prelude::*;

use crate::arsenal::{WeaponClass, WeaponId};
use crate::attachment::Attachment;
use crate::perk::Perk;
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
const RUST: Color = Color::srgb(0.6, 0.15, 0.05);
const TAPE: Color = Color::srgb(0.7, 0.62, 0.38);
const SHELLS: Color = Color::srgb(0.75, 0.12, 0.08);
const BRASS: Color = Color::srgb(0.75, 0.6, 0.2);
const SKIN: Color = Color::srgb(0.82, 0.62, 0.46);
const SLEEVE: Color = Color::srgb(0.27, 0.31, 0.21);

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
    /// Each Perk's bottle in a right hand, by its place in `Perk::ALL`: the hand at the origin,
    /// the bottle standing up out of it along +Y, its label facing +Z.
    bottles: [Handle<Mesh>; 4],
    /// The left hand handling Equipment, by place in `Handheld::ALL`: the fist at the origin.
    handheld: [Handle<Mesh>; 4],
    pub material: Handle<StandardMaterial>,
}

impl GunModels {
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

    /// How far above its origin the eye looks along when it's aimed: a fitted optic's centre, or
    /// the tops of the iron sights, which stand about as high.
    pub fn sight_height(&self, id: WeaponId) -> f32 {
        // Every model marks where an optic sits, and fits every optic there.
        let receiver = self.attachment(id, Attachment::RedDotSight).map_or(0.05, |optic| optic.rest.translation.y);
        receiver + 0.03
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
        for row in &WEAPONS {
            let mut gun = Gun::new(row.class);
            let hands = build(&mut gun, row.id, row.class);
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
        let bottles = Perk::ALL.map(|perk| meshes.add(bottle(perk)));
        let handheld = Handheld::ALL.map(|held| meshes.add(handheld(held)));
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
            extended_magazine,
            dual_magazine,
            attachments,
            left_hand,
            world: world_meshes,
            knife,
            bowie,
            bottles,
            handheld,
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
    /// The magazine's size, for its Extended and Dual Mag versions.
    magazine_size: Vec3,
    anchors: Anchors,
}

/// Where a model's Attachments go.
#[derive(Debug, Clone, Copy, Default)]
struct Anchors {
    /// The top of the receiver, where an optic's base sits.
    optic: Vec3,
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
                optic: Vec3::new(0.0, 0.05, 0.0),
                muzzle: Vec3::new(0.0, 0.01, -half),
                bore: Vec2::splat(0.025),
                under: Vec3::new(0.0, -0.045, -0.2),
                side: Vec3::new(-0.033, 0.0, 0.03),
            },
            ..default()
        }
    }

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

/// The shape of a Perk's bottle: its glass, label and cap colours, how wide and tall its body and
/// neck are, and the mark on the front of its label.
struct BottleLook {
    glass: Color,
    label: Color,
    cap: Color,
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
            cap: Color::srgb(0.75, 0.6, 0.2),
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
            cap: Color::srgb(0.6, 0.1, 0.05),
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
            cap: Color::srgb(0.92, 0.95, 1.0),
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
            model.part(Vec3::new(0.024, 0.024, 0.032), Vec3::new(0.0, 0.03, 0.0), Color::srgb(1.0, 0.12, 0.06));
        }
        Attachment::AcogSight => {
            model.part(Vec3::new(0.03, 0.012, 0.06), Vec3::new(0.0, 0.006, 0.0), DARK);
            model.part(Vec3::new(0.036, 0.036, 0.11), Vec3::new(0.0, 0.03, 0.0), DARK);
            model.part(Vec3::new(0.048, 0.048, 0.035), Vec3::new(0.0, 0.03, -0.06), DARK);
            model.part(Vec3::new(0.042, 0.042, 0.02), Vec3::new(0.0, 0.03, 0.055), DARK);
            model.part(Vec3::new(0.034, 0.034, 0.004), Vec3::new(0.0, 0.03, -0.079), Color::srgb(0.3, 0.5, 0.75));
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
            sights(gun, 0.045, -half + 0.03, 0.1);
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

    #[test]
    fn every_attachment_a_weapon_can_get_has_a_model_on_it() {
        use crate::attachment::mounts;

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
                        Some(own) if attachment.mount() == crate::attachment::Mount::Magazine => {
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
