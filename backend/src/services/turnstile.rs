use crate::error::{AppError, AppResult};
use crate::state::AppState;

const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

#[derive(serde::Deserialize)]
pub struct TurnstileRequest {
    pub token: String,
}

#[derive(serde::Serialize)]
pub struct TurnstileResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
}

/// Verifies a Cloudflare Turnstile token. When no secret key is configured
/// the check is skipped so local development stays frictionless.
pub async fn verify(state: &AppState, body: TurnstileRequest) -> AppResult<TurnstileResponse> {
    if !state.config.turnstile_configured() {
        return Ok(TurnstileResponse {
            success: true,
            skipped: Some(true),
        });
    }

    let params = [
        ("secret", state.config.turnstile_secret_key.as_str()),
        ("response", body.token.as_str()),
    ];

    let response = state
        .http
        .post(SITEVERIFY_URL)
        .form(&params)
        .send()
        .await
        .map_err(|_| AppError::unavailable("Gagal memverifikasi Turnstile"))?;

    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|_| AppError::unavailable("Gagal memverifikasi Turnstile"))?;

    Ok(TurnstileResponse {
        success: payload
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        skipped: None,
    })
}
