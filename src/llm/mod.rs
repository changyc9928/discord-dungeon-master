use async_trait::async_trait;

use crate::llm::error::LlmError;

pub mod common;
pub mod error;
pub mod gemini;
pub mod openai;
pub mod provider;

#[async_trait]
pub trait LLM: Send + Sync {
    async fn request_to_llm(
        &mut self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn conversation_continue(
        &mut self,
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

    async fn store_new_dialogue(
        &mut self,
        message: &str,
        author_id: &str,
        author_name: &str,
    ) -> Result<(), LlmError>;

    async fn new_summary(&mut self) -> Result<(), LlmError>;
}
