//! Map Files: a Map as data, a folder holding `map.ron` and an `assets/` folder (ADR 0011).
//!
//! This is the foundation: the model ([`model`]), the content hash that names a Map in the Room,
//! in `Hello` and in the cache ([`hash`]), and loading, storing and versioning ([`load`]). Nothing
//! reads a Map File in a Run yet.
//!
//! Where Map Files live: the Shipped Maps in `assets/maps/<name>/`, a Player's own in
//! `~/.zombies/maps/<map-id>/`, and Maps received in Co-op in `~/.zombies/cache/<hash>/`.

pub mod hash;
pub mod load;
pub mod model;

#[allow(unused_imports, reason = "the public face of the module; later tickets read it")]
pub use hash::MapHash;
#[allow(unused_imports, reason = "the public face of the module; later tickets read it")]
pub use load::{LoadedMap, MapFileError, MapSource};
#[allow(unused_imports, reason = "the public face of the module; later tickets read it")]
pub use model::{FORMAT, MapFile};
