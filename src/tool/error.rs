use crate::{character::error::CharacterSheetError, story::error::StoryError};

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error(transparent)]
    StoryServiceError(#[from] StoryError),

    #[error(transparent)]
    CharacterSheetServiceError(#[from] CharacterSheetError),

    #[error(transparent)]
    JsonDeserializationError(#[from] serde_json::Error),
}
