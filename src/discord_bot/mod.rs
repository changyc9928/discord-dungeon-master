pub mod commands;
pub mod error;
pub mod handler;

pub use error::DiscordBotError;
pub use handler::{Data, start_bot};
use serenity::{
    all::{ChannelId, Context},
    async_trait,
};

#[async_trait]
pub trait MessageSender: Send + Sync {
    async fn send(&self, msg: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub struct DiscordSender {
    ctx: Context,
    channel_id: String,
}

#[async_trait]
impl MessageSender for DiscordSender {
    async fn send(&self, msg: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let channel_id = ChannelId::new(self.channel_id.parse()?);
        channel_id.say(&self.ctx, msg).await?;
        Ok(())
    }
}
