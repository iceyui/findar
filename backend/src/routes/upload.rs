use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::services::storage;
use crate::services::xlsx_guard;
use crate::state::AppState;

const ONLY_XLSX_MSG: &str = "Only .xlsx files are supported";

fn ends_with_xlsx(name: Option<&str>) -> bool {
    match name {
        Some(name) => name.to_lowercase().ends_with(".xlsx"),
        None => false,
    }
}

async fn read_single_file(
    mut multipart: Multipart,
    config_max_bytes: u64,
) -> AppResult<Option<(String, Vec<u8>)>> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad(err.to_string()))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let original_name = field.file_name().map(|s| s.to_string());
        if !ends_with_xlsx(original_name.as_deref()) {
            return Err(AppError::bad(ONLY_XLSX_MSG));
        }
        let original_name = original_name.unwrap_or_default();

        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|err| AppError::internal(err.to_string()))?
        {
            let new_size = bytes.len() as u64 + chunk.len() as u64;
            if new_size > config_max_bytes {
                return Err(AppError::too_large("File too large"));
            }
            bytes.extend_from_slice(&chunk);
        }

        return Ok(Some((original_name, bytes)));
    }
    Ok(None)
}

/// POST /api/upload
pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Json<Value>> {
    auth::require_user(&state, &headers).await?;
    storage::cleanup_old_files(&state.config.upload_dir, state.config.cleanup_ttl_seconds);

    let Some((original_name, bytes)) =
        read_single_file(multipart, state.config.max_upload_bytes).await?
    else {
        return Err(AppError::bad(ONLY_XLSX_MSG));
    };

    let (upload_id, safe_name) = storage::new_upload_id(&original_name);
    let dest = storage::upload_path(&state.config, &upload_id);
    std::fs::write(&dest, &bytes)
        .map_err(|err| AppError::internal(err.to_string()))?;

    if let Err(err) = xlsx_guard::validate_xlsx(&dest, &state.config) {
        storage::remove_file_if_exists(&dest);
        return Err(err);
    }

    Ok(Json(json!({
        "upload_id": upload_id,
        "file_name": safe_name,
    })))
}

/// DELETE /api/upload/{upload_id}
pub async fn delete_upload(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(upload_id): Path<String>,
) -> AppResult<Json<Value>> {
    auth::require_user(&state, &headers).await?;
    storage::cleanup_old_files(&state.config.upload_dir, state.config.cleanup_ttl_seconds);

    if !storage::is_safe_name(&upload_id) {
        return Err(AppError::bad("Invalid upload id"));
    }

    let path = storage::upload_path(&state.config, &upload_id);
    if !path.exists() {
        return Ok(Json(json!({ "deleted": false })));
    }

    storage::remove_file_if_exists(&path);
    Ok(Json(json!({ "deleted": true })))
}
