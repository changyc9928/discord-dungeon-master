use poise::CreateReply;

use crate::discord_bot::error::DiscordBotError;
use crate::discord_bot::handler::Context;

/// Adds characters metadata to the game
#[poise::command(slash_command)]
pub async fn add_character_meta(
    ctx: Context<'_>,
    meta_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_meta(ctx.author().id.to_string().as_str(), &meta_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters metadata to the game
#[poise::command(slash_command)]
pub async fn add_character_meta_init(ctx: Context<'_>) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_meta_initiate(&ctx.author().id.to_string())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters identity information to the game
#[poise::command(slash_command)]
pub async fn add_character_identity(
    ctx: Context<'_>,
    identity_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_identity(ctx.author().id.to_string().as_str(), &identity_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters progression information to the game
#[poise::command(slash_command)]
pub async fn add_character_progression(
    ctx: Context<'_>,
    progression_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_progression(
            ctx.author().id.to_string().as_str(),
            &progression_description,
        )
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters combat information to the game
#[poise::command(slash_command)]
pub async fn add_character_combat(
    ctx: Context<'_>,
    combat_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_combat(ctx.author().id.to_string().as_str(), &combat_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters inventory information to the game
#[poise::command(slash_command)]
pub async fn add_character_inventory(
    ctx: Context<'_>,
    inventory_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_inventory(ctx.author().id.to_string().as_str(), &inventory_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters spell information to the game
#[poise::command(slash_command)]
pub async fn add_character_spells(
    ctx: Context<'_>,
    spell_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_spells(&spell_description, ctx.author().id.to_string().as_str())
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters abilities information to the game
#[poise::command(slash_command)]
pub async fn add_character_abilities(
    ctx: Context<'_>,
    abilities_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_abilities(ctx.author().id.to_string().as_str(), &abilities_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters skills information to the game
#[poise::command(slash_command)]
pub async fn add_character_skills(
    ctx: Context<'_>,
    skills_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_skills(ctx.author().id.to_string().as_str(), &skills_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters traits information to the game
#[poise::command(slash_command)]
pub async fn add_character_traits(
    ctx: Context<'_>,
    traits_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_traits(ctx.author().id.to_string().as_str(), &traits_description)
        .await?;

    let reply = CreateReply::default().content(response);
    // 3️⃣ Send follow-up response
    ctx.send(reply).await?;

    Ok(())
}

/// Adds characters notes information to the game
#[poise::command(slash_command)]
pub async fn add_character_notes(
    ctx: Context<'_>,
    notes_description: String,
) -> Result<(), DiscordBotError> {
    // 1️⃣ Defer interaction so Discord doesn't timeout
    ctx.defer().await?;

    // 2️⃣ Call your LLM
    let data = ctx.data();
    let llm = &data.llm;
    let response = llm
        .lock()
        .await
        .add_character_notes(ctx.author().id.to_string().as_str(), &notes_description)
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
