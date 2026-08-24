use std::fs::File;
use std::path::Path;

use crate::config::Config;
use crate::error::{AppError, AppResult};

const INVALID_MSG: &str = "File .xlsx tidak valid";
const TOO_COMPLEX_MSG: &str = "File Excel terlalu kompleks";
const TOO_BIG_MSG: &str = "File Excel terlalu besar";

/// Port of the Python `_validate_xlsx_file` zip-bomb protections:
/// - must be a readable zip
/// - entry count cap
/// - no absolute / traversal paths
/// - per-entry and total uncompressed size caps
/// - must contain `[Content_Types].xml`
pub fn validate_xlsx(path: &Path, config: &Config) -> AppResult<()> {
    let file = File::open(path).map_err(|_| AppError::bad(INVALID_MSG))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::bad(INVALID_MSG))?;

    if archive.len() > config.max_xlsx_entries {
        return Err(AppError::bad(TOO_COMPLEX_MSG));
    }

    let mut total_uncompressed: u64 = 0;
    let mut has_content_types = false;

    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| AppError::bad(INVALID_MSG))?;

        let normalized = entry.name().replace('\\', "/");
        if normalized == "[Content_Types].xml" {
            has_content_types = true;
        }
        if normalized.starts_with('/') || has_traversal_segment(&normalized) {
            return Err(AppError::bad(INVALID_MSG));
        }
        if entry.size() > config.max_xlsx_entry_bytes {
            return Err(AppError::bad(TOO_BIG_MSG));
        }
        total_uncompressed += entry.size();
        if total_uncompressed > config.max_xlsx_uncompressed_bytes {
            return Err(AppError::bad(TOO_BIG_MSG));
        }
    }

    if !has_content_types {
        return Err(AppError::bad(INVALID_MSG));
    }

    Ok(())
}

fn has_traversal_segment(name: &str) -> bool {
    name.split('/').any(|segment| segment == "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        dir.join(name)
    }

    fn test_config() -> Config {
        Config::from_env(std::path::Path::new("."))
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn rejects_non_zip_file() {
        let path = temp_path("guard_not_zip.xlsx");
        std::fs::write(&path, b"definitely not a zip").unwrap();
        let result = validate_xlsx(&path, &test_config());
        assert_eq!(result.unwrap_err().detail, INVALID_MSG);
    }

    #[test]
    fn accepts_valid_minimal_zip() {
        let path = temp_path("guard_ok.xlsx");
        write_zip(
            &path,
            &[("[Content_Types].xml", b"<Types/>"), ("xl/workbook.xml", b"<wb/>")],
        );
        assert!(validate_xlsx(&path, &test_config()).is_ok());
    }

    #[test]
    fn rejects_missing_content_types() {
        let path = temp_path("guard_no_ct.xlsx");
        write_zip(&path, &[("xl/workbook.xml", b"<wb/>")]);
        let result = validate_xlsx(&path, &test_config());
        assert_eq!(result.unwrap_err().detail, INVALID_MSG);
    }

    #[test]
    fn rejects_traversal_entries() {
        let path = temp_path("guard_traversal.xlsx");
        write_zip(
            &path,
            &[("[Content_Types].xml", b"<Types/>"), ("../evil.xml", b"x")],
        );
        let result = validate_xlsx(&path, &test_config());
        assert_eq!(result.unwrap_err().detail, INVALID_MSG);
    }
}
