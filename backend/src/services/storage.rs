use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use regex::Regex;
use uuid::Uuid;

use crate::config::Config;

pub const FILENAME_PATTERN: &str = r"^[A-Za-z0-9_.-]+$";

/// Port of the Python `_sanitize_filename`.
pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let replaced = base.replace(' ', "_");
    let cleaned: String = replaced
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.' || *c == '-')
        .collect();
    cleaned
}

/// Whitelisted file/identifier names: alphanumerics, `_ . -`, and no leading
/// dot (blocks hidden files such as the `.index.json` sidecar).
pub fn is_safe_name(name: &str) -> bool {
    !name.starts_with('.') && Regex::new(FILENAME_PATTERN).unwrap().is_match(name)
}

pub fn new_upload_id(original_name: &str) -> (String, String) {
    let mut safe = sanitize_filename(original_name);
    if safe.is_empty() {
        safe = "upload.xlsx".to_string();
    }
    let id = format!("{}_{}", Uuid::new_v4().simple(), safe);
    (id, safe)
}

pub fn remove_file_if_exists(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Deletes files in `dir` whose modification time is older than `ttl`.
/// Port of the Python `_cleanup_old_files`.
pub fn cleanup_old_files(dir: &Path, ttl_seconds: u64) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    let ttl = Duration::from_secs(ttl_seconds);
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else { continue };
        let Ok(modified) = metadata.modified() else { continue };
        let Ok(age) = SystemTime::now().duration_since(modified) else { continue };
        if age >= ttl {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn ensure_dirs(config: &Config) {
    let _ = fs::create_dir_all(&config.upload_dir);
    let _ = fs::create_dir_all(&config.output_dir);
}

pub fn upload_path(config: &Config, id: &str) -> PathBuf {
    config.upload_dir.join(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_keeps_safe_chars_only() {
        assert_eq!(sanitize_filename("Laporan Piutang 2024.xlsx"), "Laporan_Piutang_2024.xlsx");
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("réport.xlsx"), "rport.xlsx");
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn safe_name_validation() {
        assert!(is_safe_name("abc_123.xlsx"));
        assert!(!is_safe_name("../evil.xlsx"));
        assert!(!is_safe_name("with space.xlsx"));
        assert!(!is_safe_name(""));
    }

    #[test]
    fn upload_id_format() {
        let (id, safe) = new_upload_id("My File.xlsx");
        assert!(id.ends_with("_My_File.xlsx"));
        assert_eq!(safe, "My_File.xlsx");
        assert!(is_safe_name(&id));
    }
}
