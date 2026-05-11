use std::sync::Arc;

use rig::{
    agent::Agent,
    client::{CompletionClient, ProviderClient},
    providers::gemini,
    tool::ToolDyn,
};

use crate::{
    character::service::CharacterSheetService,
    llm::{common::LlmCore, error::LlmError, provider::LlmProvider},
    story::service::StoryService,
    tool::service::ToolService,
};

pub struct Gemini {
    pub core: LlmCore,
}

impl Gemini {
    pub fn new(
        model: &str,
        tool_service: Arc<ToolService>,
        story_service: Arc<StoryService>,
        character_sheet_service: Arc<CharacterSheetService>,
        dm_discord_id: String,
        folder_path: String,
        compile_trigger: i64,
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
        })
    }
}

impl LlmProvider for Gemini {
    type Agent = Agent<gemini::CompletionModel>;
    type CompletionModel = gemini::CompletionModel;

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
        Ok(gemini::Client::from_env()?
            .agent(self.core.model.clone())
            .preamble(prompt)
            .tools(tools)
            .build())
    }
}
