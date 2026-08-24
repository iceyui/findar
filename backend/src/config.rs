use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
    pub supabase_url: String,
    pub supabase_publishable_key: String,
    pub allowed_auth_emails: Vec<String>,
    pub allowed_auth_domains: Vec<String>,
    pub turnstile_secret_key: String,
    pub upload_dir: PathBuf,
    pub output_dir: PathBuf,
    pub max_upload_bytes: u64,
    pub max_xlsx_uncompressed_bytes: u64,
    pub max_xlsx_entry_bytes: u64,
    pub max_xlsx_entries: usize,
    pub cleanup_ttl_seconds: u64,
    pub cleanup_interval_seconds: u64,
    pub process_timeout_seconds: u64,
}

const DEFAULT_CORS_ORIGINS: [&str; 4] = [
    "https://ar.vanila.id",
    "http://ar.vanila.id",
    "https://api-ar.vanila.id",
    "http://api-ar.vanila.id",
];

fn env_string(key: &str) -> String {
    env::var(key).unwrap_or_default().trim().to_string()
}

fn env_string_or(key: &str, default: &str) -> String {
    let value = env_string(key);
    if value.is_empty() {
        default.to_string()
    } else {
        value
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(default)
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect()
}

impl Config {
    pub fn from_env(base_dir: &std::path::Path) -> Self {
        let supabase_url = env_string("SUPABASE_URL").trim_end_matches('/').to_string();
        let mut supabase_key = env_string("SUPABASE_PUBLISHABLE_KEY");
        if supabase_key.is_empty() {
            supabase_key = env_string("SUPABASE_ANON_KEY");
        }

        let cors_raw = env_string("CORS_ORIGINS");
        let cors_origins: Vec<String> = if cors_raw.is_empty() {
            DEFAULT_CORS_ORIGINS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            cors_raw
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };

        let domains: Vec<String> =
            split_list(&env::var("ALLOWED_AUTH_DOMAINS").unwrap_or_default())
                .into_iter()
                .map(|d| d.trim_start_matches('@').to_string())
                .collect();

        let ttl = env_u64("CLEANUP_TTL_SECONDS", 3600);

        Config {
            host: env_string_or("HOST", "0.0.0.0"),
            port: env_u64("PORT", 9001) as u16,
            cors_origins,
            supabase_url,
            supabase_publishable_key: supabase_key,
            allowed_auth_emails: split_list(&env::var("ALLOWED_AUTH_EMAILS").unwrap_or_default()),
            allowed_auth_domains: domains,
            turnstile_secret_key: env_string("TURNSTILE_SECRET_KEY"),
            upload_dir: base_dir.join("uploads"),
            output_dir: base_dir.join("outputs"),
            max_upload_bytes: 10 * 1024 * 1024,
            max_xlsx_uncompressed_bytes: env_u64("MAX_XLSX_UNCOMPRESSED_BYTES", 100 * 1024 * 1024),
            max_xlsx_entry_bytes: env_u64("MAX_XLSX_ENTRY_BYTES", 50 * 1024 * 1024),
            max_xlsx_entries: env_usize("MAX_XLSX_ENTRIES", 1000),
            cleanup_ttl_seconds: ttl,
            cleanup_interval_seconds: env_u64("CLEANUP_INTERVAL_SECONDS", ttl),
            process_timeout_seconds: env_u64("PROCESS_TIMEOUT_SECONDS", 600),
        }
    }

    pub fn auth_configured(&self) -> bool {
        !self.supabase_url.is_empty() && !self.supabase_publishable_key.is_empty()
    }

    pub fn turnstile_configured(&self) -> bool {
        !self.turnstile_secret_key.is_empty()
    }
}
