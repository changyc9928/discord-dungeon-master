use async_trait::async_trait;
use gemini_rust::ContentBuilder;

use crate::llm::error::LlmError;

pub mod error;
pub mod gemini;

#[async_trait]
pub trait LLM: Send + Sync {
    async fn request_to_llm(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn conversation_continue(
        &mut self,
        request: Option<ContentBuilder>,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_meta(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_identity(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_progression(
        &mut self,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_combat(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_inventory(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_spells(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_abilities(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_skills(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_traits(&mut self, discord_user_id: &str) -> Result<String, LlmError>;

    async fn add_character_notes(&mut self, discord_user_id: &str) -> Result<String, LlmError>;
}
