use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::config::Config;

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    /// Limits concurrent heavy matcher runs so memory stays bounded.
    pub process_semaphore: Arc<Semaphore>,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .expect("failed to build http client");
        AppState {
            config,
            process_semaphore: Arc::new(Semaphore::new(2)),
            http,
        }
    }
}
