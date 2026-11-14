//! Rules for finding a slot's files and choosing which variant plays. Free of Bevy so
//! `gen-sounds` can share them.

use std::fmt;
use std::path::Path;

/// Why a slot can't be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotError {
    /// Neither `<slot>.ogg` nor `<slot>_1.ogg` exists.
    Missing { slot: String },
    /// Numbered variants skip some numbers, listed here.
    Gap { slot: String, missing: Vec<u32> },
    /// Both `<slot>.ogg` and numbered variants exist, so it's unclear which to use.
    Ambiguous { slot: String },
}

impl fmt::Display for SlotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SlotError::Missing { slot } => write!(f, "{slot}: no {slot}.ogg or {slot}_1.ogg"),
            SlotError::Gap { slot, missing } => {
                let files: Vec<_> = missing.iter().map(|n| format!("{slot}_{n}.ogg")).collect();
                write!(
                    f,
                    "{slot}: variants have a gap, missing {}",
                    files.join(", ")
                )
            }
            SlotError::Ambiguous { slot } => {
                write!(f, "{slot}: has both {slot}.ogg and numbered variants")
            }
        }
    }
}

/// Every file under `dir`, as paths relative to it with `/` separators. A missing folder has none.
pub fn list_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), String::new())];
    while let Some((path, prefix)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push((entry.path(), format!("{name}/")));
            } else {
                files.push(name);
            }
        }
    }
    files
}

/// The variant number of `file` if it is `<stem>_<n>.ogg`, with `n` from 1 and no leading zeros.
fn variant_number(stem: &str, file: &str) -> Option<u32> {
    let digits = file
        .strip_suffix(".ogg")?
        .strip_prefix(stem)?
        .strip_prefix('_')?;
    let n: u32 = digits.parse().ok()?;
    (n >= 1 && digits == n.to_string()).then_some(n)
}

/// Every file in `files` that belongs to the slot `stem`, whether valid or not.
pub fn files_for(stem: &str, files: &[String]) -> Vec<String> {
    let plain = format!("{stem}.ogg");
    files
        .iter()
        .filter(|file| **file == plain || variant_number(stem, file).is_some())
        .cloned()
        .collect()
}

/// A slot's files in variant order, or why it's unusable.
pub fn variants(stem: &str, files: &[String]) -> Result<Vec<String>, SlotError> {
    let slot = stem.to_string();
    let plain = format!("{stem}.ogg");
    let mine = files_for(stem, files);
    let has_plain = mine.contains(&plain);
    let mut numbered: Vec<u32> = mine
        .iter()
        .filter_map(|f| variant_number(stem, f))
        .collect();
    numbered.sort_unstable();

    match (has_plain, numbered.last()) {
        (true, None) => Ok(vec![plain]),
        (true, Some(_)) => Err(SlotError::Ambiguous { slot }),
        (false, None) => Err(SlotError::Missing { slot }),
        (false, Some(&max)) => {
            let missing: Vec<u32> = (1..=max)
                .filter(|n| numbered.binary_search(n).is_err())
                .collect();
            if missing.is_empty() {
                Ok((1..=max).map(|n| format!("{stem}_{n}.ogg")).collect())
            } else {
                Err(SlotError::Gap { slot, missing })
            }
        }
    }
}

/// The result of checking every slot: the files of each good slot, and why each bad one is bad.
#[derive(Debug, PartialEq, Eq)]
pub struct SlotCheck {
    /// One entry per slot, in the order given: its files in variant order, or `None` if it's
    /// unusable. A cue with `None` plays nothing.
    pub found: Vec<Option<Vec<String>>>,
    /// Every unusable slot, in the order given.
    pub errors: Vec<SlotError>,
}

/// Checks every slot, keeping the good ones whatever the bad ones do.
/// `files` are paths relative to the audio assets folder, with `/` separators.
pub fn check_slots<'a>(stems: impl IntoIterator<Item = &'a str>, files: &[String]) -> SlotCheck {
    let mut check = SlotCheck { found: Vec::new(), errors: Vec::new() };
    for stem in stems {
        match variants(stem, files) {
            Ok(found) => check.found.push(Some(found)),
            Err(error) => {
                check.found.push(None);
                check.errors.push(error);
            }
        }
    }
    check
}

/// Picks which of `count` variants plays next, never `last` again unless it's the only one.
/// `roll` is uniform in `[0, 1)`.
pub fn pick_variant(count: usize, last: Option<usize>, roll: f32) -> usize {
    match last {
        Some(last) if count > 1 && last < count => {
            // Pick among the others, then step over the last one.
            let i = ((roll * (count - 1) as f32) as usize).min(count - 2);
            if i >= last { i + 1 } else { i }
        }
        _ => ((roll * count as f32) as usize).min(count.saturating_sub(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_plain_file_is_a_single_variant() {
        assert_eq!(
            variants("hit", &files(&["hit.ogg"])),
            Ok(files(&["hit.ogg"]))
        );
    }

    #[test]
    fn numbered_variants_load_in_order() {
        let found = variants(
            "hit",
            &files(&["hit_2.ogg", "kill.ogg", "hit_1.ogg", "hit_3.ogg"]),
        );
        assert_eq!(found, Ok(files(&["hit_1.ogg", "hit_2.ogg", "hit_3.ogg"])));
    }

    #[test]
    fn a_slot_with_no_files_is_missing() {
        let found = variants(
            "hit",
            &files(&["kill.ogg", "hit.wav", "hit_0.ogg", "headshot.ogg"]),
        );
        assert_eq!(found, Err(SlotError::Missing { slot: "hit".into() }));
    }

    #[test]
    fn variants_must_start_at_one_with_no_gaps() {
        let found = variants("hit", &files(&["hit_2.ogg", "hit_5.ogg"]));
        assert_eq!(
            found,
            Err(SlotError::Gap {
                slot: "hit".into(),
                missing: vec![1, 3, 4]
            })
        );
    }

    #[test]
    fn leading_zeros_are_not_variants() {
        let found = variants("hit", &files(&["hit_1.ogg", "hit_02.ogg"]));
        assert_eq!(found, Ok(files(&["hit_1.ogg"])));
    }

    #[test]
    fn plain_and_numbered_together_are_ambiguous() {
        let found = variants("hit", &files(&["hit.ogg", "hit_1.ogg"]));
        assert_eq!(found, Err(SlotError::Ambiguous { slot: "hit".into() }));
    }

    #[test]
    fn slots_in_a_subfolder_match_by_path() {
        let all = files(&["announcer/nuke.ogg", "nuke_1.ogg"]);
        assert_eq!(
            variants("announcer/nuke", &all),
            Ok(files(&["announcer/nuke.ogg"]))
        );
    }

    #[test]
    fn a_slot_name_that_prefixes_another_does_not_claim_its_files() {
        let all = files(&["gunshot_pistol.ogg"]);
        assert_eq!(
            variants("gunshot", &all),
            Err(SlotError::Missing {
                slot: "gunshot".into()
            })
        );
    }

    #[test]
    fn the_startup_check_lists_every_bad_slot() {
        let all = files(&["hit.ogg", "kill_2.ogg", "round_start_1.ogg"]);
        let check = check_slots(["hit", "kill", "headshot", "round_start"], &all);
        assert_eq!(
            check.errors,
            vec![
                SlotError::Gap {
                    slot: "kill".into(),
                    missing: vec![1]
                },
                SlotError::Missing {
                    slot: "headshot".into()
                },
            ]
        );
    }

    #[test]
    fn bad_slots_have_no_sound_and_the_rest_keep_theirs() {
        let all = files(&["hit.ogg", "kill_2.ogg", "round_start_1.ogg", "dup.ogg", "dup_1.ogg"]);
        let check = check_slots(["hit", "kill", "headshot", "round_start", "dup"], &all);
        assert_eq!(
            check.found,
            vec![
                Some(files(&["hit.ogg"])),
                None,
                None,
                Some(files(&["round_start_1.ogg"])),
                None,
            ]
        );
        assert_eq!(check.errors.len(), 3);
    }

    #[test]
    fn the_startup_check_passes_when_every_slot_is_good() {
        let all = files(&["hit.ogg", "kill_1.ogg", "kill_2.ogg"]);
        let check = check_slots(["hit", "kill"], &all);
        assert!(check.errors.is_empty());
        assert_eq!(
            check.found,
            vec![Some(files(&["hit.ogg"])), Some(files(&["kill_1.ogg", "kill_2.ogg"]))]
        );
    }

    #[test]
    fn files_for_finds_plain_and_numbered_files() {
        let all = files(&["hit.ogg", "hit_1.ogg", "hit_x.ogg", "headshot.ogg"]);
        assert_eq!(files_for("hit", &all), files(&["hit.ogg", "hit_1.ogg"]));
    }

    #[test]
    fn a_single_variant_always_plays() {
        for roll in [0.0, 0.5, 0.999] {
            assert_eq!(pick_variant(1, None, roll), 0);
            assert_eq!(pick_variant(1, Some(0), roll), 0);
        }
    }

    #[test]
    fn the_last_variant_never_plays_twice_in_a_row() {
        for count in 2..6 {
            for last in 0..count {
                for step in 0..100 {
                    let roll = step as f32 / 100.0;
                    let pick = pick_variant(count, Some(last), roll);
                    assert!(
                        pick < count && pick != last,
                        "count {count}, last {last}, roll {roll}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_other_variant_can_follow() {
        for count in 2..6 {
            for last in 0..count {
                let mut seen = vec![false; count];
                for step in 0..100 {
                    seen[pick_variant(count, Some(last), step as f32 / 100.0)] = true;
                }
                let others = (0..count).filter(|&i| i != last);
                assert!(
                    others.into_iter().all(|i| seen[i]),
                    "count {count}, last {last}"
                );
            }
        }
    }

    #[test]
    fn the_first_pick_can_be_any_variant() {
        let picks: Vec<_> = (0..4)
            .map(|i| pick_variant(4, None, i as f32 / 4.0))
            .collect();
        assert_eq!(picks, vec![0, 1, 2, 3]);
    }
}
