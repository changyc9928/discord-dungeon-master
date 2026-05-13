use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rig::{
    client::ProviderClientError,
    completion::{CompletionError, PromptError},
};

use crate::{character, story::error::StoryError};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    VarError(#[from] std::env::VarError),
    #[error("{0}")]
    CacheError(String),
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
    #[error(transparent)]
    StoryError(#[from] StoryError),
    #[error(transparent)]
    ParsingError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    ProviderClientError(#[from] ProviderClientError),
    #[error(transparent)]
    PromptError(#[from] PromptError),
    #[error(transparent)]
    CompletionError(#[from] CompletionError),
    #[error(transparent)]
    HttpError(#[from] rig::http_client::Error),
}

impl IntoResponse for LlmError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self.to_string())).into_response()
    }
}
