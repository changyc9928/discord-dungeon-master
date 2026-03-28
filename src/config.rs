use std::{collections::HashMap, path::Path};

use config::{Config, ConfigError};
use serde::{Deserialize, de::DeserializeOwned};
use tracing::Level;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_enable_swagger_ui")]
    pub enable_swagger_ui: bool,
    #[serde(default = "default_cors_origins")]
    pub cors_origins: Vec<String>,
}

fn default_enable_swagger_ui() -> bool {
    true
}

/// Production and staging should use the actual domain names
fn default_cors_origins() -> Vec<String> {
    vec![
        "http://localhost:23002".to_string(),
        "http://localhost:23001".to_string(),
    ]
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub db_name: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_max_open_conns")]
    pub max_open_conns: u32,
    #[serde(default = "default_conn_max_lifetime_secs")]
    pub conn_max_lifetime_secs: u64,
}

fn default_max_open_conns() -> u32 {
    5
}

fn default_conn_max_lifetime_secs() -> u64 {
    1800
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TracingConfig {
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,
}

fn default_otlp_endpoint() -> String {
    "http://localhost:24317".to_string()
}

fn default_buffer_check_interval_seconds() -> u64 {
    5
}

/// Logging configuration per Rust module path.
///
/// Example:
///
/// ```yaml
/// logging:
///   default: TRACE
///   "auth_module": INFO
///   "auth_module::routes::auth": ERROR
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Default trace level for ALL targets
    pub default: LevelInner,
    #[serde(flatten)]
    pub others: HashMap<String, LevelInner>,
}

// This inner implementation was needed because the one from the crate doesn't implement Deserialize
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LevelInner {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl From<LevelInner> for Level {
    fn from(value: LevelInner) -> Self {
        match value {
            LevelInner::Trace => Level::TRACE,
            LevelInner::Debug => Level::DEBUG,
            LevelInner::Info => Level::INFO,
            LevelInner::Warn => Level::WARN,
            LevelInner::Error => Level::ERROR,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ServiceConfig<C> {
    pub service_name: String,
    pub server: ServerConfig,
    #[serde(default)]
    pub database: Option<DatabaseConfig>,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(flatten)]
    pub config: C,
}

impl<C: DeserializeOwned> ServiceConfig<C> {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let config = Config::builder()
            .add_source(config::File::with_name(&path.display().to_string()))
            .add_source(
                config::Environment::default()
                    .separator("__")
                    .convert_case(config::Case::Kebab)
                    .list_separator(",")
                    .with_list_parse_key("server.cors-origins")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize::<ServiceConfig<C>>()?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AiDmConfig {
    pub gemini_model: String,
    #[serde(default)]
    pub discord_token: Option<String>,
    pub channel_id: String,
    pub dm_id: String,
    pub self_discord_id: String,
    pub buffered_message_expiry_seconds: u64,
    #[serde(default = "default_buffer_check_interval_seconds")]
    pub buffer_check_interval_seconds: u64,
}
