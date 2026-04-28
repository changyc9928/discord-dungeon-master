use chrono::DateTime;
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{Duration, interval};
use tracing::info;

use crate::character::service::CharacterSheetService;
use crate::discord_bot::{commands, error::DiscordBotError};
use crate::llm::LLM;

pub type Error = DiscordBotError;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BufferedMessage {
    pub content: String,
    pub author_id: String,
    pub author_name: String,
    pub start_time: DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct Data {
    pub llm: Arc<Mutex<dyn LLM>>,
    pub channel_id: String,
    pub self_discord_id: String,
    pub dm_discord_id: String,
    pub buffered_message_expiry_seconds: u64,
    pub buffer_check_interval_seconds: u64,
    pub buffered_messages: Arc<tokio::sync::Mutex<Vec<BufferedMessage>>>,
    pub flush_sender: mpsc::UnboundedSender<()>,
    pub character_sheet_service: Arc<CharacterSheetService>,
}

async fn event_handler(
    ctx: &serenity::Context,
    event: &serenity::FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    let serenity::FullEvent::Message { new_message } = event else {
        return Ok(());
    };

    if new_message.content.is_empty() {
        return Ok(());
    }

    let author_id = new_message.author.id.to_string();
    let channel_id = new_message.channel_id.to_string();
    let author_name = new_message.author.name.to_string();

    // Ignore messages outside target channel or from self
    if channel_id != data.channel_id || author_id == data.self_discord_id {
        return Ok(());
    }

    let buffered_message = BufferedMessage {
        content: new_message.content.clone(),
        author_id: author_id.clone(),
        author_name: author_name.clone(),
        start_time: chrono::Utc::now(),
    };

    info!(
        "Received message from {}: {}",
        author_name, new_message.content
    );

    if new_message.mentions_user_id(data.self_discord_id.parse::<u64>()?) && !new_message.author.bot
    {
        let llm = &data.llm;
        let response = llm
            .lock()
            .await
            .conversation_continue(ctx, &author_id, &new_message.content)
            .await;
        let response = match response {
            Ok(r) => r,
            Err(e) => e.to_string(),
        };
        let channel_id = serenity::ChannelId::new(data.channel_id.parse().unwrap());
        if let Err(e) = channel_id.say(ctx, &response).await {
            tracing::error!("Failed to send message: {}", e);
        }
    } else {
        {
            let mut messages = data.buffered_messages.lock().await;
            messages.push(buffered_message);
        }

        if author_id == data.dm_discord_id && should_flush_buffer(data).await? {
            flush_buffer(ctx, data).await;
        }
    }

    Ok(())
}

pub async fn start_bot(
    token: &str,
    llm: Arc<Mutex<dyn LLM>>,
    channel_id: String,
    self_discord_id: String,
    dm_discord_id: String,
    buffered_message_expiry_seconds: u64,
    buffer_check_interval_seconds: u64,
    character_sheet_service: Arc<CharacterSheetService>,
) -> Result<(), DiscordBotError> {
    let llm_service = Arc::clone(&llm);

    // Create channel for buffer flush communication
    let (flush_sender, flush_receiver) = mpsc::unbounded_channel::<()>();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping::ping(),
                commands::characters::add_character_meta(),
                commands::characters::add_character_identity(),
                commands::characters::add_character_progression(),
                commands::characters::add_character_combat(),
                commands::characters::add_character_inventory(),
                commands::characters::add_character_spells(),
                commands::characters::add_character_abilities(),
                commands::characters::add_character_skills(),
                commands::characters::add_character_traits(),
                commands::characters::add_character_notes(),
                commands::characters::get_character(),
            ],
            event_handler: |ctx, event, framework, data| {
                Box::pin(event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            let service = Arc::clone(&llm_service);
            let flush_sender_clone = flush_sender.clone();

            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let data = Data {
                    llm: service,
                    channel_id: channel_id.clone(),
                    self_discord_id: self_discord_id.clone(),
                    dm_discord_id: dm_discord_id.clone(),
                    buffered_message_expiry_seconds,
                    buffer_check_interval_seconds,
                    buffered_messages: Arc::new(tokio::sync::Mutex::new(vec![])),
                    flush_sender: flush_sender_clone,
                    character_sheet_service,
                };

                // Start the periodic buffer check task
                start_buffer_check_task(Arc::new(data.clone()), ctx.clone(), flush_receiver);

                Ok(data)
            })
        })
        .build();

    let mut client = serenity::Client::builder(
        token,
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT,
    )
    .framework(framework)
    .await?;

    client.start().await?;

    Ok(())
}

async fn should_flush_buffer(data: &Data) -> Result<bool, DiscordBotError> {
    let messages = data.buffered_messages.lock().await;
    if messages.is_empty() {
        return Ok(false);
    }

    // Check if the most recent message has exceeded the expiry time
    let most_recent_message = messages
        .iter()
        .filter(|m| m.author_id == data.dm_discord_id)
        .last()
        .ok_or(DiscordBotError::Unknown(format!(
            "Most recent message not found: {:?}",
            messages,
        )))?;
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(most_recent_message.start_time);

    Ok(elapsed.num_seconds() >= data.buffered_message_expiry_seconds as i64)
}

async fn flush_buffer(ctx: &serenity::Context, data: &Data) {
    let messages = {
        let mut messages = data.buffered_messages.lock().await;
        if messages.is_empty() {
            return;
        }
        // Take all messages and clear the buffer
        std::mem::take(&mut *messages)
    };

    if messages.is_empty() {
        return;
    }

    // Compile all messages into a single context
    let compiled_content = messages
        .iter()
        .map(|msg| format!("{}: {}", msg.author_id, msg.content))
        .collect::<Vec<_>>()
        .join("\n");

    // Use the most recent message's author for the LLM request
    let primary_author = &messages.last().unwrap().author_id;

    if let Err(e) = data
        .llm
        .lock()
        .await
        .store_new_dialogue(
            ctx,
            &compiled_content,
            primary_author,
            &messages.last().unwrap().author_name,
        )
        .await
    {
        tracing::error!("LLM error: {}", e);
        let channel_id = serenity::ChannelId::new(data.channel_id.parse().unwrap());
        if let Err(send_err) = channel_id
            .say(ctx, format!("Error processing buffered messages: {}", e))
            .await
        {
            tracing::error!("Failed to send error message: {}", send_err);
        }
    }

    if let Err(e) = data.llm.lock().await.new_summary(ctx).await {
        tracing::error!("LLM error: {}", e);
        let channel_id = serenity::ChannelId::new(data.channel_id.parse().unwrap());
        if let Err(send_err) = channel_id
            .say(ctx, format!("Error processing buffered messages: {}", e))
            .await
        {
            tracing::error!("Failed to send error message: {}", send_err);
        }
    }

    match data
        .llm
        .lock()
        .await
        .request_to_llm(ctx, primary_author, &compiled_content)
        .await
    {
        Ok(response) => {
            // Send response back to channel
            let channel_id = serenity::ChannelId::new(data.channel_id.parse().unwrap());
            if let Err(e) = channel_id.say(ctx, &response).await {
                tracing::error!("Failed to send message: {}", e);
            }
        }
        Err(e) => {
            tracing::error!("LLM error: {}", e);
            let channel_id = serenity::ChannelId::new(data.channel_id.parse().unwrap());
            if let Err(send_err) = channel_id
                .say(ctx, format!("Error processing buffered messages: {}", e))
                .await
            {
                tracing::error!("Failed to send error message: {}", send_err);
            }
        }
    }
}

fn start_buffer_check_task(
    data: Arc<Data>,
    ctx: serenity::Context,
    mut flush_receiver: mpsc::UnboundedReceiver<()>,
) {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(data.buffer_check_interval_seconds)); // Check every configured seconds

        loop {
            tokio::select! {
                // Periodic check
                _ = interval.tick() => {
                    if should_flush_buffer(&data).await.unwrap() {
                        flush_buffer(&ctx, &data).await;
                    }
                }
                // Manual flush signal
                Some(_) = flush_receiver.recv() => {
                    if should_flush_buffer(&data).await.unwrap() {
                        flush_buffer(&ctx, &data).await;
                    }
                }
            }
        }
    });
}
