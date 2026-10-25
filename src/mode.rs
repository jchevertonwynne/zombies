//! The Mode a Run is played under.

use serde::{Deserialize, Serialize};

/// The ruleset a Run is played under. Every Run has exactly one, chosen before it starts.
/// Only Classic is playable so far; the last Mode chosen is remembered in `settings.ron`.
#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Classic,
    Gauntlet,
}
