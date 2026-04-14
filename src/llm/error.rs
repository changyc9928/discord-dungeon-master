use crate::character;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    VarError(#[from] std::env::VarError),
    #[error(transparent)]
    CacheError(#[from] gemini_rust::cache::Error),
    #[error(transparent)]
    GeminiError(#[from] gemini_rust::ClientError),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    CharacterSheetError(#[from] character::error::CharacterSheetError),
    #[error("{0}")]
    InvalidResponse(String),
    #[error(transparent)]
    ToolError(#[from] crate::tool::error::ToolError),
    #[error("Content not found: {0}")]
    MissingContent(String),
}
