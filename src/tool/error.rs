use crate::character::error::CharacterSheetError;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Character sheet service error: {0}")]
    CharacterSheetServiceError(#[from] CharacterSheetError),

    #[error("JSON deserialization error: {0}")]
    JsonDeserializationError(#[from] serde_json::Error),
}
