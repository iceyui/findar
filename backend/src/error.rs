use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub detail: String,
}

impl AppError {
    pub fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        AppError {
            status,
            detail: detail.into(),
        }
    }

    pub fn bad(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, detail)
    }

    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, detail)
    }

    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, detail)
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, detail)
    }

    pub fn too_large(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, detail)
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, detail)
    }

    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, detail)
    }

    pub fn timeout(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, detail)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "detail": self.detail }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
