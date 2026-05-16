use std::{error::Error, sync::Arc};

use axum::Router;
use tokio::{
    net::TcpListener,
    signal::unix::{SignalKind, signal},
    sync::Mutex,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;

use crate::{
    character::{repository::CharacterSheetRepository, service::CharacterSheetService},
    config::{AiDmConfig, LoggingConfig, ServiceConfig},
    llm::{Anthropic, DeepSeek, Gemini, Llm, Ollama, OpenAi, OpenRouter, Qwen, routes::LlmApi},
    openapi::LlmOpenApi,
    pg_pool::{TestPgPool, TestPgPoolConfig},
    story::{
        repository::{DialogueRepository, StoryRepository},
        service::StoryService,
    },
    tool::service::ToolService,
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
    let service_config: ServiceConfig<AiDmConfig> = ServiceConfig::load("config/config.yaml")?;
    let _guard = init_tracing(&service_config.logging)?;

    tracing::info!(service_name = %service_config.service_name, "service configuration loaded");
    tracing::info!(otlp_endpoint = %service_config.tracing.otlp_endpoint, "tracing settings loaded");

    let openapi = LlmOpenApi::openapi();
    let output = "docs/openapi.yaml";
    std::fs::write(output, serde_yaml::to_string(&openapi)?)?;
    tracing::info!("OpenAPI spec written to {output}");

    let db_config = service_config
        .database
        .as_ref()
        .ok_or_else(|| crate::error::Error::MissingConfig("database"))?;
    tracing::info!(
        database_host = %db_config.host,
        database_name = %db_config.db_name,
        "initializing database pool"
    );

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

    tracing::info!(
        provider = ?service_config.config.provider,
        channel_id = %service_config.config.channel_id,
        dm_id = %service_config.config.dm_id,
        "starting discord bot"
    );

    let discord_task = tokio::spawn(async move {
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
        .await
    });

    let router = Router::new().merge(llm_api);

    let listener = TcpListener::bind((
        service_config.server.host.as_str(),
        service_config.server.port,
    ))
    .await?;
    tracing::info!(
        host = %service_config.server.host,
        port = service_config.server.port,
        "starting http server"
    );
    let http_task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_handler())
            .await
    });

    let (discord_result, http_result) = tokio::try_join!(discord_task, http_task)?;

    discord_result?;
    http_result?;

    Ok(())
}

fn init_tracing(logging: &LoggingConfig) -> Result<WorkerGuard, Box<dyn Error>> {
    std::fs::create_dir_all("logs")?;

    let file_appender = tracing_appender::rolling::daily("logs", "app.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| logging.env_filter());

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_writer(std::io::stdout))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_target(true)
                .with_writer(non_blocking),
        )
        .try_init()?;

    Ok(guard)
}

async fn shutdown_handler() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %error, "failed to install CTRL-C signal handler");
            std::process::exit(1);
        }
    };
    let terminate = async {
        match signal(SignalKind::terminate()) {
            Ok(mut signal) => signal.recv().await,
            Err(error) => {
                tracing::error!(error = %error, "failed to install SIGTERM handler");
                std::process::exit(1);
            }
        }
    };
    tokio::select! {
        _ = ctrl_c => tracing::info!(signal = "ctrl_c", "shutting down"),
        _ = terminate => tracing::info!(signal = "sigterm", "shutting down"),
    }
}
