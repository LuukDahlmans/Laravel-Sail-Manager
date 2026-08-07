//! Small helpers for durable, corruption-resilient JSON persistence.
//!
//! The settings and templates stores are the app's only source of user config.
//! A plain `std::fs::write` truncates the destination in place, so a crash or
//! full disk mid-write can leave a half-written (unparseable) file — and the
//! old code then silently fell back to `Default`, wiping every setting and
//! collapsing templates to an empty list on the next launch. These helpers
//! make writes atomic and make a corrupt read quarantine-and-warn instead of
//! silently discarding the user's data.

use std::path::{Path, PathBuf};

/// Atomically write `contents` to `path`: write a sibling temp file, then
/// rename it over the destination. `rename(2)` within a directory is atomic on
/// macOS, so a reader/observer sees either the complete old file or the
/// complete new one — never a truncated middle state.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("store");
    let tmp = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    // Best-effort: clear a stale temp from a previous crashed run.
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(&tmp, contents)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Move an unparseable file aside (to `<name>.corrupt-<pid>`) so its contents
/// survive for recovery/debugging instead of being overwritten by defaults.
/// Returns the backup path when the move succeeds.
pub fn quarantine_corrupt(path: &Path) -> Option<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("store");
    let backup = parent.join(format!("{file_name}.corrupt-{}", std::process::id()));
    std::fs::rename(path, &backup).ok().map(|_| backup)
}

/// Load JSON of type `T` from `path`, recovering gracefully:
/// - missing file  → `None` (caller decides: default, seed, etc.)
/// - unreadable     → `None`
/// - unparseable    → quarantine the bad file, warn on stderr, return `None`
///
/// Never panics and never silently overwrites the file.
pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("persist: could not read {}: {e}", path.display());
            return None;
        }
    };
    match serde_json::from_str::<T>(&raw) {
        Ok(v) => Some(v),
        Err(e) => {
            let where_to = quarantine_corrupt(path)
                .map(|b| b.display().to_string())
                .unwrap_or_else(|| "(could not move aside)".to_string());
            eprintln!(
                "persist: {} failed to parse ({e}); moved aside to {where_to}, using defaults",
                path.display()
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Sample {
        a: u32,
        b: String,
    }

    fn tmp_dir() -> PathBuf {
        // Unique-ish per test via a counter file would need Date/rand (both
        // unavailable in tests here), so lean on the OS temp dir + pid + a
        // caller-provided suffix.
        std::env::temp_dir()
    }

    #[test]
    fn write_atomic_then_load_round_trips() {
        let path = tmp_dir().join(format!("persist-rt-{}.json", std::process::id()));
        let s = Sample {
            a: 7,
            b: "hi".into(),
        };
        write_atomic(&path, &serde_json::to_string(&s).unwrap()).unwrap();
        let back: Sample = load_json(&path).unwrap();
        assert_eq!(back, s);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_loads_none() {
        let path = tmp_dir().join(format!("persist-missing-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(load_json::<Sample>(&path).is_none());
    }

    #[test]
    fn corrupt_file_is_quarantined_and_returns_none() {
        let path = tmp_dir().join(format!("persist-corrupt-{}.json", std::process::id()));
        std::fs::write(&path, "{ not valid json ]").unwrap();
        assert!(load_json::<Sample>(&path).is_none());
        // Original path no longer holds the bad content (it was moved aside).
        assert!(!path.exists());
        // Clean up whatever quarantine file we produced.
        let backup = path.parent().unwrap().join(format!(
            "persist-corrupt-{}.json.corrupt-{}",
            std::process::id(),
            std::process::id()
        ));
        let _ = std::fs::remove_file(&backup);
    }
}
