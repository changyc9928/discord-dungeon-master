use crate::character;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error(transparent)]
    GeminiError(#[from] api_gemini::error::Error),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    CharacterSheetError(#[from] character::error::CharacterSheetError),
    #[error("{0}")]
    InvalidResponse(String),
    #[error(transparent)]
    ToolError(#[from] crate::tool::error::ToolError),
}
