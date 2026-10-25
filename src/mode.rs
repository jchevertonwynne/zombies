//! The Mode a Run is played under.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// The ruleset a Run is played under. Every Run has exactly one, chosen before it starts.
/// Only Classic is playable so far; the last Mode chosen is remembered in `settings.ron`.
/// As a resource, it's the Mode of the current (or just-ended) Run.
#[derive(Resource, Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Classic,
    Gauntlet,
}
