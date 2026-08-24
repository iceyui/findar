use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde_json::{json, Value};

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::services::storage;
use crate::state::AppState;

const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

fn resolve_output(state: &AppState, file_name: &str) -> AppResult<std::path::PathBuf> {
    storage::cleanup_old_files(&state.config.output_dir, state.config.cleanup_ttl_seconds);

    if !storage::is_safe_name(file_name) {
        return Err(AppError::bad("Invalid file name"));
    }

    let path = state.config.output_dir.join(file_name);
    if !path.exists() {
        return Err(AppError::not_found("File not found"));
    }
    Ok(path)
}

/// GET /api/download/{file_name}
pub async fn download_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
) -> AppResult<impl IntoResponse> {
    auth::require_user(&state, &headers).await?;
    let path = resolve_output(&state, &file_name)?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;

    Ok((
        [
            (header::CONTENT_TYPE, XLSX_MIME.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
        ],
        bytes,
    ))
}

/// GET /api/download-data/{file_name}
pub async fn download_file_data(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
) -> AppResult<Json<Value>> {
    auth::require_user(&state, &headers).await?;
    let path = resolve_output(&state, &file_name)?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;

    Ok(Json(json!({
        "file_name": file_name,
        "media_type": XLSX_MIME,
        "data": BASE64.encode(bytes),
    })))
}
