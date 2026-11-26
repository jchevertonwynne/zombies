//! Checks on `assets/`: every file in it has a provenance entry in `assets/manifest.json`, and
//! the whole stays inside its budget. `tools/texpack/README.md` says how entries are written.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The most `assets/` may hold, in bytes.
const BUDGET: u64 = 512 * 1000 * 1000;

#[derive(Deserialize)]
struct Manifest {
    assets: Vec<Entry>,
}

/// Where one asset (a file, or a directory of them) came from.
#[derive(Deserialize)]
#[allow(dead_code)]
struct Entry {
    /// The file or directory under `assets/` it covers.
    path: String,
    name: String,
    /// Where it was fetched from, or what made it.
    source: String,
    /// The licence, as its source states it.
    licence: String,
    /// When it was fetched, as YYYY-MM-DD.
    fetched: String,
    /// A capture of the source page, for sources whose terms are per-asset or contradictory.
    #[serde(default)]
    archive: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn all_files() -> Vec<(String, u64)> {
    let root = root();
    let mut found = Vec::new();
    files(&root, &mut found);
    found
        .into_iter()
        .map(|path| {
            let size = path.metadata().unwrap().len();
            (path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"), size)
        })
        .filter(|(path, _)| !path.ends_with(".DS_Store"))
        .collect()
}

fn manifest() -> Manifest {
    serde_json::from_str(&std::fs::read_to_string(root().join("manifest.json")).unwrap()).expect("assets/manifest.json parses")
}

/// Whether `entry` covers the file at `path`: it names the file, or a directory it's in.
fn covers(entry: &str, path: &str) -> bool {
    path == entry || path.strip_prefix(entry).is_some_and(|rest| rest.starts_with('/'))
}

#[test]
fn every_asset_has_a_provenance_entry() {
    let manifest = manifest();
    for entry in &manifest.assets {
        assert!(!entry.source.is_empty() && !entry.licence.is_empty(), "{}: needs a source and a licence", entry.path);
        assert!(entry.fetched.len() == 10, "{}: fetched should be YYYY-MM-DD", entry.path);
    }
    let missing: Vec<_> = all_files()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path != "manifest.json")
        .filter(|path| !manifest.assets.iter().any(|entry| covers(&entry.path, path)))
        .collect();
    assert!(missing.is_empty(), "no manifest entry for: {missing:#?}");
}

#[test]
fn assets_stay_inside_the_budget() {
    let total: u64 = all_files().iter().map(|(_, size)| size).sum();
    println!("assets/: {:.1} MB of {} MB", total as f64 / 1e6, BUDGET / 1_000_000);
    assert!(total <= BUDGET, "assets/ holds {:.1} MB, over the {} MB budget", total as f64 / 1e6, BUDGET / 1_000_000);
}

#[test]
fn an_entry_covers_its_file_or_directory_only() {
    assert!(covers("materials/brick", "materials/brick/color.ktx2"));
    assert!(covers("audio/footstep.ogg", "audio/footstep.ogg"));
    assert!(!covers("materials/brick", "materials/brick_wall/color.ktx2"));
}

/// The Archive's reading level (#540): eight Poly Haven models (about 20 MB between them) and eight kitbashes, each with its own manifest entry,
/// every file of them under `assets/`.
#[test]
fn the_archives_reading_level_models_and_kitbashes_have_manifest_entries_and_the_models_weigh_about_20_mb() {
    let manifest = manifest();
    let files = all_files();
    let models = ["dining_table", "SchoolChair_01", "wooden_ladder", "vintage_grandfather_clock_01", "marble_bust_01", "standing_chalkboard_01", "book_encyclopedia_set_01", "GothicCabinet_01"];
    let kitbashes = ["card_catalogue", "issue_desk", "reading_lamp", "lectern", "theatre_bench", "projector", "display_case", "globe"];
    let mut model_bytes = 0;
    for (dir, id, licence_source) in models.iter().map(|id| ("models", *id, "polyhaven.com")).chain(kitbashes.iter().map(|id| ("props", *id, "tools/props/kitbash.py"))) {
        let entry = format!("{dir}/{id}");
        let found = manifest.assets.iter().find(|e| e.path == entry).unwrap_or_else(|| panic!("no manifest entry for {entry}"));
        assert_eq!(found.licence, "CC0", "{entry}");
        assert!(found.source.contains(licence_source), "{entry}: source {}", found.source);
        let mine: Vec<_> = files.iter().filter(|(path, _)| covers(&entry, path)).collect();
        assert!(mine.iter().any(|(path, _)| path.ends_with(".gltf")), "{entry} has no glTF");
        if dir == "models" {
            model_bytes += mine.iter().map(|(_, size)| size).sum::<u64>();
        }
    }
    assert!((15_000_000..=25_000_000).contains(&model_bytes), "the eight models weigh {:.1} MB", model_bytes as f64 / 1e6);
}
