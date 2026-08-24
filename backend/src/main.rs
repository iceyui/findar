mod auth;
mod config;
mod error;
mod matcher;
mod routes;
mod services;
mod state;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::HeaderValue;
use axum::routing::{delete, get, post};
use axum::{Router, serve};
use tower_http::cors::{AllowMethods, AllowHeaders, AllowOrigin, CorsLayer};

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    let _ = dotenvy::from_filename(".env");

    let base_dir = std::env::current_dir().expect("failed to resolve working directory");
    let config = Config::from_env(&base_dir);
    services::storage::ensure_dirs(&config);

    let addr = format!("{}:{}", config.host, config.port);
    let ttl = config.cleanup_ttl_seconds;
    let interval = config.cleanup_interval_seconds.max(1);

    let output_index = Arc::new(services::output_index::OutputIndex::load(
        &config.output_dir,
    ));

    let state = Arc::new(AppState::new(config, output_index));

    // Periodic cleanup of stale uploads/outputs (port of `_periodic_cleanup_loop`).
    {
        let state = state.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval));
            loop {
                ticker.tick().await;
                services::storage::cleanup_old_files(&state.config.upload_dir, ttl);
                services::storage::cleanup_old_files(&state.config.output_dir, ttl);
                state.output_index.prune(&state.config.output_dir, ttl);
            }
        });
    }

    let cors = build_cors(&state);
    let body_limit = state.config.max_upload_bytes + 256 * 1024;

    let app = Router::new()
        .route("/api/health", get(routes::health::health))
        .route(
            "/api/verify-turnstile",
            post(routes::turnstile::verify_turnstile),
        )
        .route("/api/upload", post(routes::upload::upload_file))
        .route("/api/upload/:upload_id", delete(routes::upload::delete_upload))
        .route("/api/process", post(routes::process::process_file))
        .route("/api/download/:file_name", get(routes::download::download_file))
        .route(
            "/api/download-data/:file_name",
            get(routes::download::download_file_data),
        )
        .layer(DefaultBodyLimit::max(body_limit as usize))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|err| panic!("failed to bind {addr}: {err}"));
    println!("AR Vanila Matcher API listening on http://{addr}");

    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

fn build_cors(state: &AppState) -> CorsLayer {
    let origins = state
        .config
        .cors_origins
        .iter()
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(AllowMethods::any())
        .allow_headers(AllowHeaders::any())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
