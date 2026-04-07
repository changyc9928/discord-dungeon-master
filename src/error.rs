#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database connection is not writable")]
    NotWritableDatabase,

    #[error("Missing {0} config")]
    MissingConfig(&'static str),

    #[error(transparent)]
    ConfigError(#[from] config::ConfigError),

    #[error(transparent)]
    InternalError(#[from] stable_eyre::Report),

    #[error("Invalid scheduled job: '{0}'")]
    InvalidScheduledJob(String),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    JoinTaskError(#[from] tokio::task::JoinError),

    #[error(transparent)]
    SerdeYamlError(#[from] serde_yaml::Error),

    #[error(transparent)]
    SqlxError(#[from] sqlx::Error),

    #[error(transparent)]
    GeminiError(#[from] gemini_rust::ClientError),
}
