use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

/// GET /api/health
pub async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "auth_configured": state.config.auth_configured(),
        "turnstile_configured": state.config.turnstile_configured(),
    }))
}
