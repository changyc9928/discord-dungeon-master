use std::{
    collections::{HashMap, VecDeque},
    fs,
    future::Future,
    sync::Arc,
    time::Duration,
};

use rig::{
    completion::{CompletionError, CompletionResponse},
    message::{AssistantContent, Message, ToolCall},
    tool::ToolDyn,
};
use serenity::http::StatusCode;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::{
    character::service::CharacterSheetService,
    llm::error::LlmError,
    story::service::StoryService,
    tool::{
        service::ToolService,
        types::{
            AddItemToolCall, AddSpellToolCall, GetCharacterByNameToolCall, GetCharacterToolCall,
            RemoveCache, RemoveItemToolCall, UpdateCharacterLevelToolCall, UpdateCurrentHpToolCall,
            UpdateMaxHpToolCall, UpdateSpellSlotsToolCall,
        },
    },
};

pub type ToolFactory = Arc<dyn Fn() -> Box<dyn ToolDyn> + Send + Sync>;

#[derive(Clone)]
pub struct Cache {
    pub history: Vec<Message>,
    pub prompt: String,
    pub tools: Vec<ToolFactory>,
}

pub struct LlmCore {
    pub model: String,
    pub story_service: Arc<StoryService>,
    pub character_sheet_service: Arc<CharacterSheetService>,
    pub tool_service: Arc<ToolService>,
    pub cached_context: HashMap<String, Cache>,
    pub dm_discord_id: String,
    pub folder_path: String,
    pub compile_trigger: usize,
    pub retry_attempt: usize,
}

impl LlmCore {
    pub fn new(
        model: &str,
        tool_service: Arc<ToolService>,
        story_service: Arc<StoryService>,
        character_sheet_service: Arc<CharacterSheetService>,
        dm_discord_id: String,
        folder_path: String,
        compile_trigger: usize,
        retry_attempt: usize,
    ) -> Self {
        Self {
            model: model.to_owned(),
            story_service,
            character_sheet_service,
            tool_service,
            cached_context: HashMap::new(),
            dm_discord_id,
            folder_path,
            compile_trigger,
            retry_attempt,
        }
    }

    pub fn load_prompt(&self, file: &str) -> Result<String, LlmError> {
        Ok(fs::read_to_string(format!(
            "{}/{}",
            self.folder_path, file
        ))?)
    }

    pub fn user_prompt(&self, discord_user_id: &str) -> String {
        format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？")
    }

    pub fn channel_prompt(&self, discord_user_id: &str, discord_channel_message: &str) -> String {
        format!(
            "Discord channnel里的用户{}发送了消息：{}",
            discord_user_id, discord_channel_message
        )
    }

    pub fn dialogue_summary_prompt(&self, prompt: &str, summary: &str, dialogues: &str) -> String {
        format!(
            "{}\n\nDM的discord ID为{}\n\n【背景信息-剧情总结（用于理解当前局势）】\n{}\n\n【背景信息-最近对话（按时间顺序）】\n{}\n\n【说明】\n- 上述内容仅作为背景信息\n- 请基于这些信息进行判断\n- 不要重复或总结上述内容",
            prompt, self.dm_discord_id, summary, dialogues
        )
    }

    pub fn new_summary_prompt(&self, story: &str, dialogues: &str) -> String {
        format!(
            "【历史剧情总结】\n{}\n\n【最近对话记录（按时间顺序）】\n{}\n\n【任务】\n请基于以上信息，生成一段更新后的完整剧情总结。",
            story, dialogues
        )
    }

    pub fn character_tools(&self) -> Vec<Box<dyn ToolDyn>> {
        vec![
            Box::new(GetCharacterToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(GetCharacterByNameToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(AddItemToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(RemoveItemToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(AddSpellToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(UpdateSpellSlotsToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(UpdateCurrentHpToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(UpdateMaxHpToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
            Box::new(UpdateCharacterLevelToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
            }),
        ]
    }

    pub fn tool_factory<T, F>(build: F) -> ToolFactory
    where
        T: ToolDyn + 'static,
        F: Fn() -> T + Send + Sync + 'static,
    {
        Arc::new(move || Box::new(build()))
    }

    pub fn remove_cache_factory() -> ToolFactory {
        Arc::new(|| Box::new(RemoveCache))
    }

    pub fn character_flow_tools(tool: ToolFactory) -> Vec<ToolFactory> {
        vec![tool, Self::remove_cache_factory()]
    }

    pub async fn completion_with_retry<T, F, Fut>(
        &self,
        mut request: F,
    ) -> Result<CompletionResponse<T>, CompletionError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<CompletionResponse<T>, CompletionError>>,
    {
        let mut attempt = 0;
        loop {
            let result = request().await;
            match result {
                Ok(response) => return Ok(response),
                Err(err) if self.should_retry_completion(&err, attempt) => {
                    self.sleep_before_retry(attempt, &err).await;
                    attempt += 1;
                }
                Err(err) => return Err(err),
            }
        }
    }

    fn should_retry_completion(&self, err: &CompletionError, attempt: usize) -> bool {
        attempt < self.retry_attempt && self.is_retryable_completion_error(err)
    }

    fn is_retryable_completion_error(&self, err: &CompletionError) -> bool {
        matches!(
            err,
            CompletionError::HttpError(rig::http_client::Error::InvalidStatusCodeWithMessage(
                status,
                _,
            )) if *status == StatusCode::SERVICE_UNAVAILABLE
                || *status == StatusCode::GATEWAY_TIMEOUT
                || *status == StatusCode::BAD_GATEWAY
        )
    }

    async fn sleep_before_retry(&self, attempt: usize, err: &(impl std::fmt::Display + ?Sized)) {
        let delay = Duration::from_millis(500 * 2_u64.pow(attempt as u32));
        warn!(
            attempt = attempt + 1,
            delay_ms = delay.as_millis(),
            error = %err,
            "LLM request returned a retryable error; retrying"
        );
        sleep(delay).await;
    }

    pub fn collect_response<T>(
        &self,
        response: &CompletionResponse<T>,
        texts: &mut Vec<String>,
        tool_calls: &mut VecDeque<ToolCall>,
    ) {
        for item in response.choice.iter() {
            match item {
                AssistantContent::Text(text) => texts.push(text.text.clone()),
                AssistantContent::ToolCall(call) => tool_calls.push_back(call.clone()),
                AssistantContent::Reasoning(reasoning) => info!("Reasoning: {reasoning:?}"),
                AssistantContent::Image(image) => info!("Image sent: {image:?}"),
            }
        }
    }
}
