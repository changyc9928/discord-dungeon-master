use rig::client::{CompletionClient, ProviderClient};
use rig::providers::{
    anthropic as rig_anthropic, deepseek as rig_deepseek, gemini as rig_gemini,
    ollama as rig_ollama, openai as rig_openai, openrouter as rig_openrouter,
};

use crate::{
    character::service::CharacterSheetService,
    llm::{error::LlmError, provider::LlmProvider},
    story::service::StoryService,
    tool::service::ToolService,
};

use self::state::LlmCore;

pub mod state;

macro_rules! impl_llm_provider_core {
    (
        $provider_name:ident,
        $provider_client:path,
        $completion_model:path,
        $api_key_env:literal
    ) => {
        pub struct $provider_name {
            pub core: LlmCore,
        }

        impl $provider_name {
            pub fn new(
                model: &str,
                tool_service: std::sync::Arc<ToolService>,
                story_service: std::sync::Arc<StoryService>,
                character_sheet_service: std::sync::Arc<CharacterSheetService>,
                dm_discord_id: String,
                folder_path: String,
                compile_trigger: i64,
                base_url: Option<String>,
                retry_attempt: i64,
            ) -> Result<Self, LlmError> {
                Ok(Self {
                    core: LlmCore::new(
                        model,
                        tool_service,
                        story_service,
                        character_sheet_service,
                        dm_discord_id,
                        folder_path,
                        compile_trigger as usize,
                        retry_attempt as usize,
                        base_url,
                    ),
                })
            }
        }

        impl LlmProvider for $provider_name {
            type Agent = rig::agent::Agent<$completion_model>;
            type CompletionModel = $completion_model;

            fn core(&self) -> &LlmCore {
                &self.core
            }

            fn core_mut(&mut self) -> &mut LlmCore {
                &mut self.core
            }

            fn build_agent(
                &self,
                prompt: &str,
                tools: Vec<Box<dyn rig::tool::ToolDyn>>,
            ) -> Result<Self::Agent, LlmError> {
                if let Some(base_url) = &self.core.base_url {
                    let api_key = std::env::var($api_key_env)?;
                    let client = <$provider_client>::builder()
                        .base_url(base_url)
                        .api_key(api_key)
                        .build()?;
                    return Ok(CompletionClient::agent(&client, self.core.model.clone())
                        .preamble(prompt)
                        .tools(tools)
                        .build());
                }

                let client = <$provider_client>::from_env()?;
                Ok(CompletionClient::agent(&client, self.core.model.clone())
                    .preamble(prompt)
                    .tools(tools)
                    .build())
            }
        }
    };
}

impl_llm_provider_core!(
    OpenAi,
    rig_openai::Client,
    rig_openai::responses_api::ResponsesCompletionModel,
    "OPENAI_API_KEY"
);

impl_llm_provider_core!(
    DeepSeek,
    rig_deepseek::Client,
    rig_deepseek::CompletionModel,
    "DEEPSEEK_API_KEY"
);

impl_llm_provider_core!(
    Gemini,
    rig_gemini::Client,
    rig_gemini::CompletionModel,
    "GEMINI_API_KEY"
);

impl_llm_provider_core!(
    Anthropic,
    rig_anthropic::Client,
    rig_anthropic::completion::CompletionModel,
    "ANTHROPIC_API_KEY"
);

impl_llm_provider_core!(
    OpenRouter,
    rig_openrouter::Client,
    rig_openrouter::CompletionModel,
    "OPENROUTER_API_KEY"
);

impl_llm_provider_core!(
    Ollama,
    rig_ollama::Client,
    rig_ollama::CompletionModel,
    "OLLAMA_API_KEY"
);

impl_llm_provider_core!(
    Qwen,
    rig_openrouter::Client,
    rig_openrouter::CompletionModel,
    "OPENROUTER_API_KEY"
);
