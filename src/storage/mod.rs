//! What is kept between launches: the save-file layer, Run Records, and Settings.

#[allow(dead_code, reason = "Map Files are loaded by no Run yet (#545); later Map tickets read them")]
pub mod map_file;
pub mod records;
pub mod save;
pub mod settings;
