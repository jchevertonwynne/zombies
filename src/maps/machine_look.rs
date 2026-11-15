//! How the machines are dressed: the Mystery Box, Pack-a-Punch and the Perk Machines as several
//! textured parts (a body, a panel, a sign frame, a base) laid over their plain boxes, and the cue
//! that lights with the Power. The pure layout of the parts, in the machine's own frame (centred on
//! its collider box, +Z out into the room), is here; `dress` turns it into hidden children that
//! `TexturesPlugin` shows once the material has loaded, so with the files missing (and in headless
//! runs) a machine keeps its plain look. Colliders and use points are never touched.

use bevy::light::cluster::GlobalClusterSettings;
use bevy::prelude::*;

use crate::maps::map::textures::{Material, Shape, Textured, Tint, Veneer, box_uv_mesh};
use crate::maps::power::Power;
use crate::state::GameState;

/// How far a part may stand out of its machine's box.
pub const PROUD: f32 = 0.1;

/// How a part is painted: the Perk's own colour, or a fixed tone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tone {
    /// The material as it is.
    Bare,
    /// The machine's paint.
    Paint,
    /// The machine's paint, brighter.
    Bright,
    /// Near-black.
    Dark,
    /// Hazard yellow.
    Hazard,
}

/// What a part is made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Make {
    Textured(Material, Tone),
    /// A strip that glows in the machine's colour whatever the material, hidden with the rest.
    Glow,
}

/// One box of a machine's look.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub make: Make,
    pub size: Vec3,
    pub at: Vec3,
}

const fn piece(make: Make, size: (f32, f32, f32), at: (f32, f32, f32)) -> Piece {
    Piece { make, size: Vec3::new(size.0, size.1, size.2), at: Vec3::new(at.0, at.1, at.2) }
}

pub const STEEL: Material = Material::MetalPlate02;
const PAINTED: Material = Material::RustyMetal02;

/// The size of a Perk Machine's box.
pub const PERK_BODY: Vec3 = Vec3::new(1.2, 2.2, 0.9);

/// A Perk Machine: one body for every Perk, which differs only by its paint, sign and light. A
/// cabinet in the Perk's paint on a steel base, with a steel cap, a framed sign, a panel and a
/// window.
pub fn perk_machine() -> Vec<Piece> {
    use Make::Textured as T;
    vec![
        piece(T(STEEL, Tone::Bare), (1.28, 0.2, 0.98), (0.0, -1.0, 0.0)),
        piece(T(PAINTED, Tone::Paint), (1.24, 1.87, 0.94), (0.0, 0.015, 0.0)),
        piece(T(PAINTED, Tone::Bright), (1.25, 0.1, 0.95), (0.0, -0.6, 0.0)),
        piece(T(STEEL, Tone::Bare), (1.28, 0.16, 0.98), (0.0, 1.03, 0.0)),
        // The sign's frame: its bottom bar and two sides (the cap is its top).
        piece(T(STEEL, Tone::Bare), (1.14, 0.05, 0.08), (0.0, 0.52, 0.5)),
        piece(T(STEEL, Tone::Bare), (0.06, 0.5, 0.08), (-0.54, 0.75, 0.5)),
        piece(T(STEEL, Tone::Bare), (0.06, 0.5, 0.08), (0.54, 0.75, 0.5)),
        // The panel, and the window in it.
        piece(T(STEEL, Tone::Bare), (0.86, 0.5, 0.05), (0.0, -0.2, 0.49)),
        piece(T(STEEL, Tone::Dark), (0.5, 0.3, 0.03), (0.0, -0.2, 0.52)),
    ]
}

/// The size of Pack-a-Punch's box.
pub const PUNCH_BODY: Vec3 = Vec3::new(1.6, 1.6, 0.9);

/// Pack-a-Punch: a purple cabinet in steel corner posts on a steel base, a framed glow strip and a
/// panel with slats. Nothing covers the slot in its top.
pub fn pack_a_punch() -> Vec<Piece> {
    use Make::Textured as T;
    let mut pieces = vec![
        piece(T(STEEL, Tone::Bare), (1.68, 0.18, 0.98), (0.0, -0.71, 0.0)),
        piece(T(PAINTED, Tone::Paint), (1.64, 1.38, 0.94), (0.0, 0.08, 0.0)),
        piece(T(STEEL, Tone::Bare), (1.1, 0.55, 0.05), (0.0, -0.1, 0.49)),
        piece(T(STEEL, Tone::Bare), (1.3, 0.18, 0.04), (0.0, 0.45, 0.465)),
    ];
    for x in [-0.78, 0.78] {
        for z in [-0.43, 0.43] {
            pieces.push(piece(T(STEEL, Tone::Bare), (0.14, 1.62, 0.14), (x, 0.0, z)));
        }
    }
    for y in [-0.36, -0.28, -0.2] {
        pieces.push(piece(T(STEEL, Tone::Dark), (0.9, 0.04, 0.03), (0.0, y - 0.0, 0.52)));
    }
    pieces
}

/// The size of a Mystery Box's body.
pub const CRATE_BODY: Vec3 = Vec3::new(1.6, 0.8, 0.9);

/// The Mystery Box as a wooden crate: weathered planks with steel straps, corner posts, a lid and
/// foot rim, a latch on its front, and a seam of its blue glow round the lid.
pub fn crate_box() -> Vec<Piece> {
    use Make::Textured as T;
    let mut pieces = vec![
        piece(T(Material::WeatheredPlanks, Tone::Bare), (1.64, 0.82, 0.94), (0.0, 0.0, 0.0)),
        piece(T(STEEL, Tone::Bare), (1.68, 0.08, 0.98), (0.0, 0.36, 0.0)),
        piece(T(STEEL, Tone::Bare), (1.68, 0.06, 0.98), (0.0, -0.37, 0.0)),
        piece(T(STEEL, Tone::Dark), (0.22, 0.16, 0.04), (0.0, 0.22, 0.47)),
        piece(Make::Glow, (1.66, 0.02, 0.96), (0.0, 0.31, 0.0)),
    ];
    for x in [-0.55, 0.55] {
        pieces.push(piece(T(STEEL, Tone::Bare), (0.1, 0.84, 0.96), (x, 0.0, 0.0)));
    }
    for x in [-0.77, 0.77] {
        for z in [-0.42, 0.42] {
            pieces.push(piece(T(Material::WeatheredPlanks, Tone::Dark), (0.12, 0.84, 0.12), (x, 0.0, z)));
        }
    }
    pieces
}

/// The size of the Power Switch's box.
pub const SWITCH_BODY: Vec3 = Vec3::new(0.8, 1.2, 0.3);

/// The Power Switch's hazard header and the dark slot the lever swings over. Its face is its -Z.
pub fn power_switch() -> Vec<Piece> {
    use Make::Textured as T;
    vec![
        piece(T(PAINTED, Tone::Hazard), (0.86, 0.14, 0.34), (0.0, 0.53, 0.0)),
        piece(T(STEEL, Tone::Dark), (0.16, 0.8, 0.03), (0.0, -0.05, -0.165)),
    ]
}

/// What a Perk's paint makes of each tone.
fn tone_colour(tone: Tone, paint: Color) -> Option<Color> {
    match tone {
        Tone::Bare => None,
        Tone::Paint => Some(paint),
        Tone::Bright => Some(paint.lighter(0.2)),
        Tone::Dark => Some(Color::srgb(0.18, 0.18, 0.2)),
        Tone::Hazard => Some(Color::srgb(0.9, 0.7, 0.12)),
    }
}

/// A look's pieces, with their meshes and plain materials made, ready to be spawned on any number
/// of machines.
#[derive(Clone)]
pub struct Parts(Vec<Part>);

#[derive(Clone)]
struct Part {
    piece: Piece,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    tint: Option<Color>,
}

/// Makes the meshes and plain materials of `pieces`. A `Make::Glow` piece glows `glow`. `paint` is
/// the machine's own colour.
pub fn prepare(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>, pieces: &[Piece], paint: Color, glow: LinearRgba) -> Parts {
    Parts(
        pieces
            .iter()
            .map(|&piece| match piece.make {
                Make::Textured(material, tone) => {
                    let tint = tone_colour(tone, paint);
                    Part {
                        piece,
                        mesh: meshes.add(box_uv_mesh(piece.size, piece.at, Quat::IDENTITY, material.tile())),
                        material: materials.add(tint.unwrap_or(Color::srgb(0.35, 0.35, 0.37))),
                        tint,
                    }
                }
                Make::Glow => Part {
                    piece,
                    mesh: meshes.add(Cuboid::from_size(piece.size)),
                    material: materials.add(StandardMaterial { base_color: paint, emissive: glow, ..default() }),
                    tint: None,
                },
            })
            .collect(),
    )
}

/// Spawns `parts` as hidden children of `parent`, to be shown once their material has loaded.
pub fn spawn(commands: &mut Commands, parent: Entity, parts: &Parts) {
    for part in &parts.0 {
        let mut child = commands.spawn((
            Mesh3d(part.mesh.clone()),
            MeshMaterial3d(part.material.clone()),
            Transform::from_translation(part.piece.at),
            Visibility::Hidden,
        ));
        match part.piece.make {
            Make::Textured(material, _) => {
                child.insert((Veneer(material), Textured { material, shape: Shape::Baked }));
                if let Some(tint) = part.tint {
                    child.insert(Tint(tint));
                }
            }
            // Shown with the crate's wood.
            Make::Glow => {
                child.insert(Veneer(Material::WeatheredPlanks));
            }
        }
        let id = child.id();
        commands.entity(parent).add_child(id);
    }
}

/// `prepare` and `spawn` in one, for a machine spawned once.
pub fn dress(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    parent: Entity,
    pieces: &[Piece],
    paint: Color,
    glow: LinearRgba,
) {
    let parts = prepare(meshes, materials, pieces, paint, glow);
    spawn(commands, parent, &parts);
}

/// A material that lights with the Power: dark (or dim) off, glowing on. Its entity's lamp light, if
/// it has one, is the second colour's.
#[derive(Component)]
pub struct PowerGlow {
    pub material: Handle<StandardMaterial>,
    pub off: LinearRgba,
    pub on: LinearRgba,
    /// The point light's colour off and on, if the entity has one.
    pub light: Option<(Color, Color)>,
}

/// Makes every `PowerGlow` follow the Switch's `Power`.
pub struct MachineLookPlugin;

impl Plugin for MachineLookPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, roomier_clustering).add_systems(Update, follow_power.run_if(in_state(GameState::Playing)));
    }
}

/// A room full of lit machines, lamps and glows needs more room in the GPU's light-cluster lists
/// than Bevy starts with. Left to Bevy, the lists overflow when the view jumps, and its resize
/// reads back garbage and asks for a buffer past the largest one the GPU allows.
fn roomier_clustering(settings: Option<ResMut<GlobalClusterSettings>>) {
    if let Some(mut settings) = settings
        && let Some(gpu) = settings.gpu_clustering.as_mut()
    {
        gpu.initial_index_list_capacity = gpu.initial_index_list_capacity.max(1 << 21);
        gpu.initial_z_slice_list_capacity = gpu.initial_z_slice_list_capacity.max(1 << 16);
    }
}

fn follow_power(
    power: Option<Res<Power>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut glows: Query<(&PowerGlow, Option<&mut PointLight>)>,
    added: Query<(), Added<PowerGlow>>,
    mut last: Local<Option<bool>>,
) {
    let on = power.is_some_and(|power| power.is_on());
    if *last == Some(on) && added.is_empty() {
        return;
    }
    *last = Some(on);
    for (glow, light) in &mut glows {
        if let Some(mut material) = materials.get_mut(&glow.material) {
            material.emissive = if on { glow.on } else { glow.off };
        }
        if let (Some(mut light), Some((off, lit))) = (light, glow.light) {
            light.color = if on { lit } else { off };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn within(pieces: &[Piece], body: Vec3) {
        let reach = body / 2.0 + Vec3::splat(PROUD);
        for piece in pieces {
            let far = piece.at.abs() + piece.size / 2.0;
            assert!(far.cmple(reach).all(), "{piece:?} reaches {far} out of {reach}");
        }
    }

    #[test]
    fn every_look_stays_near_the_box_it_dresses() {
        within(&perk_machine(), PERK_BODY);
        within(&pack_a_punch(), PUNCH_BODY);
        within(&crate_box(), CRATE_BODY);
    }

    #[test]
    fn nothing_covers_pack_a_punchs_slot() {
        // The slot sits on the top face, 0.005 above it, over its middle.
        let top = PUNCH_BODY.y / 2.0;
        for piece in pack_a_punch() {
            let covers_slot = piece.at.x.abs() - piece.size.x / 2.0 < 0.7
                && piece.at.y + piece.size.y / 2.0 > top
                && (piece.at.z - 0.1).abs() - piece.size.z / 2.0 < 0.1;
            assert!(!covers_slot, "{piece:?} covers the slot");
        }
    }

    #[test]
    fn a_perk_machine_covers_its_box_from_foot_to_cap() {
        let pieces = perk_machine();
        let low = pieces.iter().map(|p| p.at.y - p.size.y / 2.0).fold(f32::MAX, f32::min);
        let high = pieces.iter().map(|p| p.at.y + p.size.y / 2.0).fold(f32::MIN, f32::max);
        assert!(low <= -PERK_BODY.y / 2.0 && high >= PERK_BODY.y / 2.0);
    }

    #[test]
    fn paint_tones_leave_bare_metal_alone() {
        assert_eq!(tone_colour(Tone::Bare, Color::WHITE), None);
        assert_eq!(tone_colour(Tone::Paint, Color::BLACK), Some(Color::BLACK));
    }
}
