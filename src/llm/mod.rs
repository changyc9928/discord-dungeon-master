use async_trait::async_trait;

use crate::llm::error::LlmError;

pub mod error;
pub mod gemini;
pub mod types;

#[async_trait]
pub trait LLM: Send + Sync {
    async fn request_to_llm(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn dispatch(&self, tool_call: serde_json::Value) -> Result<String, LlmError>;

    async fn add_character_meta(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_identity(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_progression(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_combat(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_inventory(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;
}
