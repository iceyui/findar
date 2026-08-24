use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::Value;

use crate::error::AppResult;
use crate::services::turnstile::{self, TurnstileRequest};
use crate::state::AppState;

/// POST /api/verify-turnstile
///
/// Intentionally unauthenticated: the widget is verified *before* login,
/// so requiring a session here would break the login flow.
pub async fn verify_turnstile(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TurnstileRequest>,
) -> AppResult<Json<Value>> {
    let response = turnstile::verify(&state, body).await?;
    Ok(Json(serde_json::to_value(response).unwrap_or_default()))
}
