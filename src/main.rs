use std::{error::Error, sync::Arc};

use axum::Router;
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
    sync::Mutex,
};
use utoipa::OpenApi;

use crate::{
    character::{repository::CharacterSheetRepository, service::CharacterSheetService}, config::{AiDmConfig, ServiceConfig}, llm::{Anthropic, DeepSeek, Gemini, Llm, Ollama, OpenAi, OpenRouter, Qwen, routes::LlmApi}, openapi::LlmOpenApi, pg_pool::{TestPgPool, TestPgPoolConfig}, story::{
        repository::{DialogueRepository, StoryRepository},
        service::StoryService,
    }, tool::service::ToolService
};

pub mod character;
pub mod config;
pub mod discord_bot;
pub mod error;
pub mod llm;
pub mod openapi;
pub mod pg_pool;
pub mod story;
pub mod tool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Basic initialization that prints to stdout
    tracing_subscriber::fmt::init();

    let openapi = LlmOpenApi::openapi();
    let output = "docs/openapi.yaml";
    std::fs::write(output, serde_yaml::to_string(&openapi)?)?;
    println!("OpenAPI spec written to {output}");

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
    let character_sheet_repository = Arc::new(CharacterSheetRepository::from_pool(pg_pool.clone()));
    let story_repository = Arc::new(StoryRepository::from_pool(pg_pool.clone()));
    let dialogue_repository = Arc::new(DialogueRepository::from_pool(pg_pool));
    let character_sheet_service = Arc::new(CharacterSheetService {
        repo: character_sheet_repository,
    });
    let story_service = Arc::new(StoryService {
        repository: story_repository,
        dialogue_repository,
        compile_trigger: service_config.config.compile_trigger,
    });
    let tool_service = Arc::new(ToolService {
        character_sheet_service: Arc::clone(&character_sheet_service),
        story_service: Arc::clone(&story_service),
    });

    macro_rules! create_llm {
        ($provider:ty) => {
            Arc::new(Mutex::new(<$provider>::new(
                &service_config.config.model,
                tool_service,
                story_service,
                Arc::clone(&character_sheet_service),
                service_config.config.dm_id.clone(),
                service_config.config.prompts_folder_path,
                service_config.config.compile_trigger,
                service_config.config.base_url,
                service_config.config.retry_attempt,
            )?))
        };
    }

    let llm: Arc<Mutex<dyn Llm>> = match service_config.config.provider {
        config::Provider::OpenAi => create_llm!(OpenAi),
        config::Provider::Gemini => create_llm!(Gemini),
        config::Provider::DeepSeek => create_llm!(DeepSeek),
        config::Provider::Qwen => create_llm!(Qwen),
        config::Provider::Anthropic => create_llm!(Anthropic),
        config::Provider::OpenRouter => create_llm!(OpenRouter),
        config::Provider::Ollama => create_llm!(Ollama),
    };

    let discord_token = service_config
        .config
        .discord_token
        .clone()
        .or_else(|| std::env::var("DISCORD_TOKEN").ok())
        .ok_or_else(|| discord_bot::DiscordBotError::MissingDiscordToken)?;

    let llm_api = LlmApi {
        llm_service: llm.clone(),
        character_sheet_service: character_sheet_service.clone(),
    };

    discord_bot::handler::start_bot(
        &discord_token,
        llm,
        service_config.config.channel_id.clone(),
        service_config.config.self_discord_id.clone(),
        service_config.config.dm_id.clone(),
        service_config.config.buffered_message_expiry_seconds,
        service_config.config.buffer_check_interval_seconds,
        character_sheet_service,
    )
    .await?;

    let router = Router::new().merge(llm_api);

    let listener = TcpListener::bind((
        service_config.server.host.as_str(),
        service_config.server.port,
    ))
    .await?;
    tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_handler())
            .await
    })
    .await??;

    Ok(())
}

async fn shutdown_handler() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!("Failed to install CTRL-C signal handler: {error}");
            std::process::exit(1);
        }
    };
    let terminate = async {
        match signal(SignalKind::terminate()) {
            Ok(mut signal) => signal.recv().await,
            Err(error) => {
                tracing::error!("Failed to install SIGTERM handler: {error}");
                std::process::exit(1);
            }
        }
    };
    tokio::select! {
        _ = ctrl_c => tracing::info!("Recieved CTRL-C. Shutting down..."),
        _ = terminate => tracing::info!("Recieved SIGTERM. Shutting down..."),
    }
}
