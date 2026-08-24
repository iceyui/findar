use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Multipart, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::matcher::{loader::LoaderError, MatcherError, RunOutcome};
use crate::services::storage;
use crate::services::xlsx_guard;
use crate::state::AppState;

const DEFAULT_TOLERANCE: i64 = 100;
const DEFAULT_MAX_INVOICES: usize = 5;
const ONLY_XLSX_MSG: &str = "Only .xlsx files are supported";

/// Removes a freshly-saved upload on any early exit unless disarmed,
/// mirroring the legacy `except: unlink` behaviour.
struct TempFileGuard(Option<PathBuf>);

impl TempFileGuard {
    fn disarm(&mut self) -> Option<PathBuf> {
        self.0.take()
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            storage::remove_file_if_exists(&path);
        }
    }
}

fn parse_targets(raw: &str) -> Vec<i64> {
    raw.split(',')
        .map(|part| {
            let digits: String = part.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse::<i64>().unwrap_or(0)
        })
        .filter(|&value| value > 0)
        .collect()
}

/// POST /api/process
pub async fn process_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    let user = auth::require_user(&state, &headers).await?;

    storage::cleanup_old_files(&state.config.upload_dir, state.config.cleanup_ttl_seconds);
    storage::cleanup_old_files(&state.config.output_dir, state.config.cleanup_ttl_seconds);
    state
        .output_index
        .prune(&state.config.output_dir, state.config.cleanup_ttl_seconds);

    let mut upload_id: Option<String> = None;
    let mut targets_raw: Option<String> = None;
    let mut tolerance: Option<i64> = None;
    let mut max_invoices: Option<i64> = None;
    let mut guard = TempFileGuard(None);

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad(err.to_string()))?
    {
        match field.name() {
            Some("upload_id") => {
                upload_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|err| AppError::bad(err.to_string()))?,
                );
            }
            Some("targets") => {
                targets_raw = Some(
                    field
                        .text()
                        .await
                        .map_err(|err| AppError::bad(err.to_string()))?,
                );
            }
            Some("tolerance") => {
                if let Ok(text) = field.text().await {
                    tolerance = text.trim().parse::<i64>().ok();
                }
            }
            Some("max_invoices") => {
                if let Ok(text) = field.text().await {
                    max_invoices = text.trim().parse::<i64>().ok();
                }
            }
            Some("file") => {
                let original_name = field.file_name().map(|s| s.to_string());
                let valid_ext = original_name
                    .as_deref()
                    .map(|n| n.to_lowercase().ends_with(".xlsx"))
                    .unwrap_or(false);
                if !valid_ext {
                    return Err(AppError::bad(ONLY_XLSX_MSG));
                }

                let (id, _) = storage::new_upload_id(&original_name.unwrap_or_default());
                let dest = storage::upload_path(&state.config, &id);

                // Buffer with cap (uploads are hard-capped at 10MB).
                let mut bytes: Vec<u8> = Vec::new();
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            if bytes.len() as u64 + chunk.len() as u64
                                > state.config.max_upload_bytes
                            {
                                return Err(AppError::too_large("File too large"));
                            }
                            bytes.extend_from_slice(&chunk);
                        }
                        Ok(None) => break,
                        Err(err) => return Err(AppError::internal(err.to_string())),
                    }
                }

                std::fs::write(&dest, &bytes)
                    .map_err(|err| AppError::internal(err.to_string()))?;
                guard.0 = Some(dest);
            }
            _ => {}
        }
    }

    let targets = parse_targets(targets_raw.as_deref().unwrap_or(""));
    if targets.is_empty() {
        return Err(AppError::bad("Target must be a positive number"));
    }

    let tolerance = tolerance.unwrap_or(DEFAULT_TOLERANCE).max(0);
    let max_invoices =
        (max_invoices.unwrap_or(DEFAULT_MAX_INVOICES as i64)).clamp(1, 20) as usize;

    storage::ensure_dirs(&state.config);

    // Resolve which file to process. `upload_id` wins over an attached file
    // (legacy if/elif ordering); zip-bomb validation applies to fresh uploads.
    let (input_path, _fresh_upload): (PathBuf, bool) = if let Some(id) = upload_id {
        if !storage::is_safe_name(&id) {
            return Err(AppError::bad("Invalid upload id"));
        }
        let path = storage::upload_path(&state.config, &id);
        if !path.exists() {
            return Err(AppError::not_found("Upload not found"));
        }
        (path, false)
    } else if let Some(path) = guard.disarm() {
        if let Err(err) = xlsx_guard::validate_xlsx(&path, &state.config) {
            storage::remove_file_if_exists(&path);
            return Err(err);
        }
        (path, true)
    } else {
        return Err(AppError::bad("File or upload id must be provided"));
    };

    // Run the CPU-bound search off the async runtime with a hard deadline.
    let output_dir = state.config.output_dir.clone();
    let timeout_ms = state.config.process_timeout_seconds * 1000;
    let node_budget = state.config.search_node_budget;
    let result_cap = state.config.max_result_rows;

    let permit = state
        .process_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| AppError::internal("Proses gagal. Coba lagi."))?;

    let run_path = input_path.clone();
    let joined = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        crate::matcher::run_for_targets(
            &run_path,
            &targets,
            tolerance,
            max_invoices,
            &output_dir,
            Duration::from_millis(timeout_ms),
            node_budget,
            result_cap,
        )
    })
    .await;

    // Legacy `finally`: the processed input file is always deleted afterwards.
    // Any freshly-saved-but-unused attachment is removed when `guard` drops.
    storage::remove_file_if_exists(&input_path);

    let result = match joined {
        Ok(inner) => inner,
        Err(_) => return Err(AppError::internal("Proses gagal. Coba lagi.")),
    };

    outcome_response(&state, &user.email, result)
}

fn outcome_response(
    state: &AppState,
    owner_email: &str,
    result: Result<RunOutcome, MatcherError>,
) -> AppResult<Json<Value>> {
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(MatcherError::Timeout) => {
            return Err(AppError::timeout(
                "Proses terlalu lama. Coba kurangi jumlah piutang atau target.",
            ));
        }
        Err(MatcherError::BudgetExceeded) => {
            return Err(AppError::bad(
                "Kombinasi terlalu banyak untuk Max Invoice ini pada file tersebut. Turunkan Max Invoice per kombinasi (misal 5–8).",
            ));
        }
        Err(MatcherError::Loader(loader_err)) => {
            return Err(AppError::bad(loader_detail(loader_err)));
        }
        Err(MatcherError::Write(err)) => {
            eprintln!("failed writing result workbook: {err}");
            return Err(AppError::internal("Proses gagal. Coba lagi."));
        }
    };

    match outcome.output_file {
        None => Ok(Json(json!({
            "found": false,
            "total_rows": 0,
            "download_url": Value::Null,
            "truncated": outcome.truncated,
        }))),
        Some(output_path) => {
            let file_name = output_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            // Record ownership so only this user can download the result.
            state.output_index.insert(
                &state.config.output_dir,
                &file_name,
                owner_email,
            );
            Ok(Json(json!({
                "found": true,
                "total_rows": outcome.total_rows,
                "download_url": format!("/api/download/{file_name}"),
                "file_name": file_name,
                "truncated": outcome.truncated,
            })))
        }
    }
}

fn loader_detail(err: LoaderError) -> &'static str {
    match err {
        LoaderError::ReadFailed => {
            "Gagal membaca file Excel. Pastikan file .xlsx valid dan tidak rusak."
        }
        LoaderError::HeaderNotFound => {
            "Header tidak ditemukan. Pastikan ada kolom 'Nama Pelanggan'."
        }
        LoaderError::ColumnsIncomplete => {
            "Kolom penting tidak lengkap. Wajib ada: 'Nama Pelanggan', 'No. Faktur', 'Tgl. Faktur', 'Total'."
        }
        LoaderError::EmptyAfterClean => {
            "Data kosong setelah dibersihkan. Pastikan kolom Total berisi angka dan tanggal faktur valid."
        }
    }
}
