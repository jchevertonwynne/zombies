use avian3d::prelude::*;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum GameLayer {
    #[default]
    World,
    Player,
    Zombie,
    ZombieHead,
}
