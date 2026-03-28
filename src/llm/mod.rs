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

    async fn dispatch(&self, name: &str, args: serde_json::Value) -> Result<String, LlmError>;
}
