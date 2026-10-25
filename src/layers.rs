use avian3d::prelude::*;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    #[default]
    World,
    Player,
    Zombie,
    /// A Zombie's limb, torso, neck and head areas, which attacks hit instead of its body capsule.
    ZombieHitArea,
}
