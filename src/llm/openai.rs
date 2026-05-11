use std::sync::Arc;

use rig::{agent::Agent, client::CompletionClient, providers::openai, tool::ToolDyn};

use crate::{
    character::service::CharacterSheetService,
    llm::{common::LlmCore, error::LlmError, provider::LlmProvider},
    story::service::StoryService,
    tool::service::ToolService,
};

pub struct OpenAi {
    pub core: LlmCore,
    base_url: String,
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
        base_url: String,
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
            ),
            base_url,
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
        let api_key = std::env::var("OPENAI_KEY")?;
        Ok(openai::Client::builder()
            .base_url(self.base_url.clone())
            .api_key(api_key)
            .build()?
            .agent(self.core.model.clone())
            .preamble(prompt)
            .tools(tools)
            .build())
    }
}
