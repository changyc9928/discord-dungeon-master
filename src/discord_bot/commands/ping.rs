use crate::discord_bot::handler::Context;
use crate::discord_bot::error::DiscordBotError;

/// Responds with Pong! - Use this to check if the bot is alive
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    ctx.say("Pong! 🏓").await?;
    Ok(())
}

