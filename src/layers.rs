use avian3d::prelude::*;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    #[default]
    World,
    Player,
    Zombie,
    /// A Zombie's limb, torso, neck and head areas, which attacks hit instead of its body capsule.
    ZombieHitArea,
    /// The open gap of a window in an outer wall: solid to the Player only, so Zombies climb in and
    /// bullets fly out, but the Player never leaves the Map.
    Window,
    /// Thrown Equipment, which bounces off the world and passes through everyone.
    Thrown,
}
