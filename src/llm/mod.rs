use async_trait::async_trait;
use serenity::all::Context;

use crate::llm::error::LlmError;

pub mod error;
pub mod gemini;

#[async_trait]
pub trait LLM: Send + Sync {
    async fn request_to_llm(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn conversation_continue(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_meta(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_identity(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_progression(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_combat(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_inventory(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_spells(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_abilities(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_skills(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_traits(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn add_character_notes(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError>;

    async fn store_new_dialogue(
        &mut self,
        ctx: &Context,
        message: &str,
        author_id: &str,
        author_name: &str,
    ) -> Result<(), LlmError>;

    async fn new_summary(&mut self, ctx: &Context) -> Result<(), LlmError>;
}
