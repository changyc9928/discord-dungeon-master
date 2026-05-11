use std::sync::Arc;

use rig::{
    agent::Agent,
    client::{CompletionClient, ProviderClient},
    providers::openai,
    tool::ToolDyn,
};

use crate::{
    character::service::CharacterSheetService,
    llm::core::common::LlmCore,
    llm::{error::LlmError, provider::LlmProvider},
    story::service::StoryService,
    tool::service::ToolService,
};

pub struct OpenAi {
    pub core: LlmCore,
}

impl OpenAi {
    pub fn new(
        model: &str,
        tool_service: Arc<ToolService>,
        story_service: Arc<StoryService>,
        character_sheet_service: Arc<CharacterSheetService>,
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

impl LlmProvider for OpenAi {
    type Agent = Agent<openai::responses_api::ResponsesCompletionModel>;
    type CompletionModel = openai::responses_api::ResponsesCompletionModel;

    fn core(&self) -> &LlmCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut LlmCore {
        &mut self.core
    }

    fn build_agent(
        &self,
        prompt: &str,
        tools: Vec<Box<dyn ToolDyn>>,
    ) -> Result<Self::Agent, LlmError> {
        if let Some(base_url) = &self.core.base_url {
            let api_key = std::env::var("OPENAI_API_KEY")?;
            return Ok(openai::Client::builder()
                .base_url(base_url)
                .api_key(api_key)
                .build()?
                .agent(self.core.model.clone())
                .preamble(prompt)
                .tools(tools)
                .build());
        }
        Ok(openai::Client::from_env()?
            .agent(self.core.model.clone())
            .preamble(prompt)
            .tools(tools)
            .build())
    }
}
