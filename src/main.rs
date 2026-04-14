use std::{error::Error, sync::Arc};

use tokio::sync::Mutex;

use crate::{
    character::{repository::CharacterSheetRepository, service::CharacterSheetService},
    config::{AiDmConfig, ServiceConfig},
    llm::gemini::Gemini,
    pg_pool::{TestPgPool, TestPgPoolConfig},
    tool::service::ToolService,
};

pub mod character;
pub mod config;
pub mod discord_bot;
pub mod error;
pub mod llm;
pub mod pg_pool;
pub mod tool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Basic initialization that prints to stdout
    tracing_subscriber::fmt::init();

    let service_config: ServiceConfig<AiDmConfig> = ServiceConfig::load("/app/config.yaml")?;
    let db_config = service_config
        .database
        .as_ref()
        .ok_or_else(|| crate::error::Error::MissingConfig("database"))?;
    let pg_pool = TestPgPool::init(TestPgPoolConfig {
        migrations: "/app/db/migrations".into(),
        db_name: db_config.db_name.clone(),
        host: db_config.host.clone(),
        port: db_config.port,
        default_database: "postgres".to_owned(),
        username: db_config.username.clone(),
        password: db_config.password.clone(),
    })
    .await;
    let pg_pool = pg_pool.resource().await;
    let character_sheet_repository = Arc::new(CharacterSheetRepository::from_pool(pg_pool));
    let character_sheet_service = Arc::new(CharacterSheetService {
        repo: character_sheet_repository,
    });
    let tool_service = Arc::new(ToolService {
        character_sheet_service: Arc::clone(&character_sheet_service),
    });

    let gemini: Arc<Mutex<dyn llm::LLM>> = Arc::new(Mutex::new(Gemini::new(
        &service_config.config.gemini_model,
        tool_service,
        service_config.config.channel_id.parse().unwrap(),
    )?));

    let discord_token = service_config
        .config
        .discord_token
        .clone()
        .or_else(|| std::env::var("DISCORD_TOKEN").ok())
        .ok_or_else(|| discord_bot::DiscordBotError::MissingDiscordToken)?;

    discord_bot::handler::start_bot(
        &discord_token,
        gemini,
        service_config.config.channel_id.clone(),
        service_config.config.self_discord_id.clone(),
        service_config.config.dm_id.clone(),
        service_config.config.buffered_message_expiry_seconds,
        service_config.config.buffer_check_interval_seconds,
        character_sheet_service,
    )
    .await?;

    Ok(())
}
