use poise::serenity_prelude as serenity;

#[derive(Debug, thiserror::Error)]
pub enum DiscordBotError {
    #[error(transparent)]
    SerenityError(#[from] serenity::Error),

    #[error(transparent)]
    LlmError(#[from] crate::llm::error::LlmError),

    #[error("Missing Discord token")]
    MissingDiscordToken,

    #[error("Command error: {0}")]
    CommandError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}
