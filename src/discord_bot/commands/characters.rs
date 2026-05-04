use poise::CreateReply;

use crate::discord_bot::DiscordSender;
use crate::discord_bot::error::DiscordBotError;
use crate::discord_bot::handler::Context;

/// Adds characters metadata to the game
#[poise::command(slash_command)]
pub async fn add_character_meta(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_meta(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters identity information to the game
#[poise::command(slash_command)]
pub async fn add_character_identity(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_identity(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters progression information to the game
#[poise::command(slash_command)]
pub async fn add_character_progression(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_progression(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters combat information to the game
#[poise::command(slash_command)]
pub async fn add_character_combat(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_combat(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters inventory information to the game
#[poise::command(slash_command)]
pub async fn add_character_inventory(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_inventory(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters spell information to the game
#[poise::command(slash_command)]
pub async fn add_character_spells(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_spells(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters abilities information to the game
#[poise::command(slash_command)]
pub async fn add_character_abilities(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_abilities(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters skills information to the game
#[poise::command(slash_command)]
pub async fn add_character_skills(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_skills(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters traits information to the game
#[poise::command(slash_command)]
pub async fn add_character_traits(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_traits(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters notes information to the game
#[poise::command(slash_command)]
pub async fn add_character_notes(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    let message_sender = DiscordSender {
        ctx: ctx.serenity_context().clone(),
        channel_id: ctx.channel_id().to_string(),
    };

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_notes(&message_sender, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Retrieves a character's information from the game
#[poise::command(slash_command)]
pub async fn get_character(ctx: Context<'_>, discord_id: String) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let service = &data.character_sheet_service;
    let response = service.get_character(&discord_id).await?;

    let reply = CreateReply::default().content(serde_json::to_string_pretty(&response)?);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}
