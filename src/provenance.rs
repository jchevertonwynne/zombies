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

/// The Archive's stacks level and cellar (#541): nine kitbashes, each with its own CC0 manifest entry and its files under `assets/`. They reuse
/// the Material Sets and Poly Haven models already there (no new download), so both Props tickets' kitbashes together stay within the 4.5 MB the
/// look decision budgets.
#[test]
fn the_archives_stacks_and_cellar_kitbashes_have_manifest_entries_and_both_props_tickets_stay_within_4_5_mb() {
    let manifest = manifest();
    let files = all_files();
    let reading = ["card_catalogue", "issue_desk", "reading_lamp", "lectern", "theatre_bench", "projector", "display_case", "globe"];
    let stacks = ["book_trolley", "book_pile", "book_press", "guillotine", "safe", "deed_boxes", "pump_engine", "bucket", "candles"];
    let mut kitbash_bytes = 0;
    for id in reading.iter().chain(&stacks) {
        let entry = format!("props/{id}");
        let found = manifest.assets.iter().find(|e| e.path == entry).unwrap_or_else(|| panic!("no manifest entry for {entry}"));
        assert_eq!(found.licence, "CC0", "{entry}");
        assert!(found.source.contains("tools/props/kitbash.py"), "{entry}: source {}", found.source);
        let mine: Vec<_> = files.iter().filter(|(path, _)| covers(&entry, path)).collect();
        assert!(mine.iter().any(|(path, _)| path.ends_with(".gltf")) && mine.iter().any(|(path, _)| path.ends_with(".bin")), "{entry} lacks its glTF or its buffer");
        kitbash_bytes += mine.iter().map(|(_, size)| size).sum::<u64>();
    }
    assert!(kitbash_bytes <= 4_500_000, "the Archive's seventeen kitbashes weigh {:.2} MB", kitbash_bytes as f64 / 1e6);
}

/// The Archive's sound (#542): twenty Freesound files, each with its own manifest entry that names the pages it was made from (each read for its
/// CC0 link), and the Archive's sounds together under the 2 MB the look decision budgets. `BoilerDrone` is the Sanatorium's file, reused.
#[test]
fn the_archives_sound_files_have_cc0_manifest_entries_and_stay_under_2_mb() {
    let manifest = manifest();
    let files = all_files();
    let slots: [(&str, u8); 10] = [
        ("range_rattle", 1),
        ("range_roll", 1),
        ("range_thud", 2),
        ("wheel_turn", 2),
        ("archive_tone", 1),
        ("cellar_bed", 1),
        ("drip", 3),
        ("page_rustle", 4),
        ("shelf_creak", 3),
        ("book_fall", 2),
    ];
    let mut bytes = 0;
    let mut count = 0;
    for (slot, variants) in slots {
        for n in 1..=variants {
            let entry = format!("audio/{slot}_{n}.ogg");
            let found = manifest.assets.iter().find(|e| e.path == entry).unwrap_or_else(|| panic!("no manifest entry for {entry}"));
            assert_eq!(found.licence, "CC0", "{entry}");
            assert!(found.source.split(" ; ").all(|page| page.starts_with("https://freesound.org/people/") && page.contains("/sounds/")), "{entry}: source {}", found.source);
            let notes = found.notes.as_deref().unwrap_or_default();
            assert!(notes.contains("CC0 on each page"), "{entry}: the licence was read on the page: {notes}");
            bytes += files.iter().find(|(path, _)| *path == entry).unwrap_or_else(|| panic!("{entry} is not under assets/")).1;
            count += 1;
        }
    }
    assert_eq!(count, 20);
    assert!(bytes < 2_000_000, "the Archive's sounds weigh {bytes} bytes");
}
