use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::services::output_index::OutputIndex;

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    /// Limits concurrent heavy matcher runs so memory stays bounded.
    pub process_semaphore: Arc<Semaphore>,
    pub http: reqwest::Client,
    /// Maps generated output files to the user who created them.
    pub output_index: Arc<OutputIndex>,
}

impl AppState {
    pub fn new(config: Config, output_index: Arc<OutputIndex>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .expect("failed to build http client");
        AppState {
            config,
            process_semaphore: Arc::new(Semaphore::new(2)),
            http,
            output_index,
        }
    }
}
