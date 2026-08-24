use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;
use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Result of a successful authentication check.
pub struct AuthUser {
    pub email: String,
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    let mut parts = value.splitn(2, ' ');
    let scheme = parts.next()?;
    let token = parts.next()?.trim();
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token)
    } else {
        None
    }
}

pub async fn require_user(state: &AppState, headers: &HeaderMap) -> AppResult<AuthUser> {
    let token = extract_bearer(headers).ok_or_else(|| {
        AppError::unauthorized("Login diperlukan")
    })?;

    if !state.config.auth_configured() {
        return Err(AppError::unavailable("Supabase Auth belum dikonfigurasi"));
    }

    let url = format!("{}/auth/v1/user", state.config.supabase_url);
    let response = state
        .http
        .get(&url)
        .header("apikey", &state.config.supabase_publishable_key)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await;

    let response = match response {
        Ok(resp) => resp,
        Err(_) => {
            return Err(AppError::unavailable("Gagal memvalidasi login"));
        }
    };

    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err(AppError::unauthorized("Sesi login tidak valid"));
        }
        status if !status.is_success() => {
            return Err(AppError::unavailable("Gagal memvalidasi login"));
        }
        _ => {}
    }

    let user: Value = response
        .json()
        .await
        .map_err(|_| AppError::unavailable("Gagal memvalidasi login"))?;

    let email = user
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_lowercase();

    if !state.config.allowed_auth_emails.is_empty()
        && !state.config.allowed_auth_emails.contains(&email)
    {
        return Err(AppError::forbidden("Email tidak diizinkan"));
    }

    if !state.config.allowed_auth_domains.is_empty() {
        let domain = email.rsplit('@').next().unwrap_or("");
        let allowed: HashSet<&str> = state
            .config
            .allowed_auth_domains
            .iter()
            .map(|s| s.as_str())
            .collect();
        if !allowed.contains(domain) {
            return Err(AppError::forbidden("Domain email tidak diizinkan"));
        }
    }

    Ok(AuthUser { email })
}
