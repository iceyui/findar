//! Ownership mapping for generated result files.
//!
//! Persists a sidecar JSON (`.index.json`) inside the outputs directory so the
//! mapping survives restarts. Every produced workbook is recorded with the
//! email of the authenticated user who ran the search; downloads are only
//! served back to the same user.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

pub const INDEX_FILE_NAME: &str = ".index.json";

#[derive(Debug, Clone)]
struct Entry {
    owner: String,
}

#[derive(Debug)]
pub struct OutputIndex {
    inner: Mutex<HashMap<String, Entry>>,
}

fn sidecar_path(dir: &Path) -> PathBuf {
    dir.join(INDEX_FILE_NAME)
}

impl OutputIndex {
    /// Loads the sidecar index if present; missing/corrupt files yield empty.
    pub fn load(dir: &Path) -> Self {
        let mut map = HashMap::new();
        if let Ok(raw) = std::fs::read_to_string(sidecar_path(dir)) {
            if let Ok(parsed) =
                serde_json::from_str::<HashMap<String, HashMap<String, String>>>(&raw)
            {
                for (file_name, fields) in parsed {
                    if let Some(owner) = fields.get("owner").cloned() {
                        map.insert(file_name, Entry { owner });
                    }
                }
            }
        }
        OutputIndex {
            inner: Mutex::new(map),
        }
    }

    pub fn insert(&self, dir: &Path, file_name: &str, owner: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                file_name.to_string(),
                Entry {
                    owner: owner.to_string(),
                },
            );
            persist(dir, &guard);
        }
    }

    /// Returns true when the file exists in the index AND belongs to `owner`.
    pub fn is_owner(&self, file_name: &str, owner: &str) -> bool {
        match self.inner.lock() {
            Ok(guard) => guard
                .get(file_name)
                .map(|entry| entry.owner == owner)
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    /// Drops entries whose backing file is gone or older than `ttl`, keeping
    /// the sidecar in lockstep with `cleanup_old_files`.
    pub fn prune(&self, dir: &Path, ttl_seconds: u64) {
        let mut removed = false;
        if let Ok(mut guard) = self.inner.lock() {
            let ttl = Duration::from_secs(ttl_seconds);
            guard.retain(|file_name, _| {
                let path = dir.join(file_name);
                match std::fs::metadata(&path).and_then(|m| m.modified()) {
                    Ok(modified) => match SystemTime::now().duration_since(modified) {
                        Ok(age) => {
                            if age >= ttl {
                                removed = true;
                                false
                            } else {
                                true
                            }
                        }
                        Err(_) => false,
                    },
                    Err(_) => {
                        removed = true;
                        false
                    }
                }
            });
            if removed {
                persist(dir, &guard);
            }
        }
    }

    #[cfg(test)]
    pub fn len_of(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }
}

/// Atomic-ish best-effort write of the sidecar index.
fn persist(dir: &Path, map: &HashMap<String, Entry>) {
    let payload: HashMap<&String, HashMap<&str, &String>> = map
        .iter()
        .map(|(name, entry)| {
            (
                name,
                HashMap::from([("owner", &entry.owner)]),
            )
        })
        .collect();

    let serialized = match serde_json::to_string(&payload) {
        Ok(json) => json,
        Err(_) => return,
    };

    let target = sidecar_path(dir);
    let tmp = dir.join(".index.json.tmp");
    if std::fs::write(&tmp, serialized).is_ok() {
        #[cfg(windows)]
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::rename(&tmp, &target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ar_vanila_idx_{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn insert_and_ownership_check() {
        let dir = temp_dir("own");
        let index = OutputIndex::load(&dir);

        assert!(!index.is_owner("a.xlsx", "u@vanila.id")); // unknown file

        index.insert(&dir, "a.xlsx", "u@vanila.id");
        assert!(index.is_owner("a.xlsx", "u@vanila.id"));
        assert!(!index.is_owner("a.xlsx", "other@vanila.id"));
    }

    #[test]
    fn survives_reload_via_sidecar() {
        let dir = temp_dir("reload");

        {
            let index = OutputIndex::load(&dir);
            index.insert(&dir, "b.xlsx", "fin@vanila.id");
        }

        // Fresh instance reads persisted sidecar.
        let reloaded = OutputIndex::load(&dir);
        assert!(reloaded.is_owner("b.xlsx", "fin@vanila.id"));
        assert!(dir.join(".index.json").exists());
    }

    #[test]
    fn prune_drops_missing_files() {
        let dir = temp_dir("prune");
        std::fs::write(dir.join("exists.xlsx"), b"x").unwrap();

        let index = OutputIndex::load(&dir);
        index.insert(&dir, "exists.xlsx", "u@vanila.id");
        index.insert(&dir, "ghost.xlsx", "u@vanila.id");
        assert_eq!(index.len_of(), 2);

        index.prune(&dir, 3600);
        assert_eq!(index.len_of(), 1);
        assert!(index.is_owner("exists.xlsx", "u@vanila.id"));
        assert!(!index.is_owner("ghost.xlsx", "u@vanila.id"));
    }

    #[test]
    fn corrupt_sidecar_starts_empty_without_panic() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join(".index.json"), b"not json{{{").unwrap();
        let index = OutputIndex::load(&dir);
        assert_eq!(index.len_of(), 0);
    }
}
