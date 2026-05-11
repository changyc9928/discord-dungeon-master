use std::{
    collections::{HashMap, VecDeque},
    fs,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use rig::{
    client::{CompletionClient, ProviderClient},
    completion::{Completion, CompletionError, CompletionResponse},
    message::{AssistantContent, Message, ToolCall},
    providers::gemini,
    tool::ToolDyn,
};
use serenity::http::StatusCode;
use tokio::time::sleep;
use tracing::{info, warn};

use crate::{
    character::service::CharacterSheetService,
    discord_bot::MessageSender,
    llm::{LLM, error::LlmError},
    story::service::StoryService,
    tool::{
        service::ToolService,
        types::{
            AbilitiesToolCall, AddItemToolCall, AddSpellToolCall, CombatToolCall,
            GetCharacterByNameToolCall, GetCharacterToolCall, IdentityToolCall, InventoryToolCall,
            MetaToolCall, NewDialogueToolCall, NotesToolCall, ProgressionToolCall, RemoveCache,
            RemoveItemToolCall, SkillsToolCall, SpellToolCall, TraitsToolCall,
            UpdateCharacterLevelToolCall, UpdateCurrentHpToolCall, UpdateMaxHpToolCall,
            UpdateSpellSlotsToolCall,
        },
    },
};

type ToolFactory = Arc<dyn Fn() -> Box<dyn ToolDyn> + Send + Sync>;

#[derive(Clone)]
pub struct Cache {
    pub history: Vec<Message>,
    pub prompt: String,
    pub tools: Vec<ToolFactory>,
}

pub struct Gemini {
    model: String,
    story_service: Arc<StoryService>,
    character_sheet_service: Arc<CharacterSheetService>,
    tool_service: Arc<ToolService>,
    cached_context: HashMap<String, Cache>,
    dm_discord_id: String,
    folder_path: String,
    compile_trigger: i64,
    retry_attempt: i64,
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
            model: model.to_owned(),
            story_service,
            character_sheet_service,
            cached_context: HashMap::new(),
            dm_discord_id,
            folder_path,
            compile_trigger,
            tool_service,
            retry_attempt,
        })
    }

    async fn handle_llm_interaction(
        &mut self,
        discord_id: &str,
        prompt: &str,
        message: &str,
        tools: Vec<ToolFactory>,
    ) -> Result<String, LlmError> {
        let mut memory = self
            .cached_context
            .get(discord_id)
            .cloned()
            .unwrap_or(Cache {
                history: vec![],
                prompt: prompt.to_owned(),
                tools,
            });

        let agent = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&memory.prompt)
            .tools(memory.tools.iter().map(|f| f()).collect())
            .build();

        let mut texts = Vec::new();
        let mut tool_calls = VecDeque::new();
        let mut should_clear_cache = false;

        // Initial completion
        let response = self
            .completion_with_retry(|| async {
                agent
                    .completion(message, memory.history.clone())
                    .await?
                    .send()
                    .await
            })
            .await?;

        memory.history.push(message.into());
        memory.history.push(response.choice.clone().into());

        self.collect_response(&response, &mut texts, &mut tool_calls);

        while let Some(tool_call) = tool_calls.pop_front() {
            if tool_call.function.name == "remove_cache" {
                should_clear_cache = true;
                continue;
            }

            let tool_result_text = match serde_json::to_value(tool_call.function.clone()) {
                Ok(payload) => match self.tool_service.dispatch(payload).await {
                    Ok(result) => serde_json::to_string(&result)?,
                    Err(err) => serde_json::to_string(&err.to_string())?,
                },

                Err(err) => serde_json::to_string(&err.to_string())?,
            };

            let message = Message::tool_result_with_call_id(
                tool_call.id,
                tool_call.call_id,
                tool_result_text,
            );

            let followup = self
                .completion_with_retry(|| async {
                    agent
                        .completion(message.clone(), memory.history.clone())
                        .await?
                        .send()
                        .await
                })
                .await?;

            memory.history.push(message);
            memory.history.push(followup.choice.clone().into());

            self.collect_response(&followup, &mut texts, &mut tool_calls);
        }

        if should_clear_cache {
            self.cached_context.remove(discord_id);
        } else {
            self.cached_context.insert(discord_id.to_owned(), memory);
        }

        Ok(texts
            .into_iter()
            .last()
            .unwrap_or_else(|| "<模型没有给予任何回复>".to_owned()))
    }

    async fn completion_with_retry<T, F, Fut>(
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
        attempt < self.retry_attempt as usize && self.is_retryable_completion_error(err)
    }

    fn is_retryable_completion_error(&self, err: &CompletionError) -> bool {
        matches!(
            err,
            CompletionError::HttpError(rig::http_client::Error::InvalidStatusCodeWithMessage(
                status,
                _,
            )) if *status == StatusCode::SERVICE_UNAVAILABLE || *status == StatusCode::GATEWAY_TIMEOUT || *status == StatusCode::BAD_GATEWAY
        )
    }

    async fn sleep_before_retry(&self, attempt: usize, err: &(impl std::fmt::Display + ?Sized)) {
        let delay = Duration::from_millis(500 * 2_u64.pow(attempt as u32));
        warn!(
            attempt = attempt + 1,
            delay_ms = delay.as_millis(),
            error = %err,
            "OpenAI request returned a retryable error; retrying"
        );
        sleep(delay).await;
    }

    fn collect_response<T>(
        &self,
        response: &CompletionResponse<T>,
        texts: &mut Vec<String>,
        tool_calls: &mut VecDeque<ToolCall>,
    ) {
        for item in response.choice.iter() {
            match item {
                AssistantContent::Text(text) => {
                    texts.push(text.text.clone());
                }

                AssistantContent::ToolCall(call) => {
                    tool_calls.push_back(call.clone());
                }

                AssistantContent::Reasoning(reasoning) => {
                    info!("Reasoning: {reasoning:?}");
                }

                AssistantContent::Image(image) => {
                    info!("Image sent: {image:?}");
                }
            }
        }
    }
}

#[async_trait]
impl LLM for Gemini {
    async fn conversation_continue(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        if !self.cached_context.contains_key(discord_user_id) {
            return Ok("对话上下文不存在，请使用slash command来开启你需要的功能对话".to_owned());
        }
        self.handle_llm_interaction(discord_user_id, "", discord_channel_message, vec![])
            .await
    }

    async fn add_character_spells(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_spells.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(SpellToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_abilities(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt =
            fs::read_to_string(format!("{}/add_character_abilities.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(AbilitiesToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_skills(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_skills.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(SkillsToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_traits(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_traits.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(TraitsToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_notes(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_notes.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(NotesToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_meta(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_meta.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(MetaToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_identity(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt =
            fs::read_to_string(format!("{}/add_character_identity.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(IdentityToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_progression(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!(
            "{}/add_character_progression.txt",
            self.folder_path
        ))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(ProgressionToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_combat(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_combat.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(CombatToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_inventory(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt =
            fs::read_to_string(format!("{}/add_character_inventory.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(InventoryToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn request_to_llm(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/main.txt", self.folder_path))?;

        let message = format!(
            "Discord channnel里的用户{}发送了消息：{}",
            discord_user_id, discord_channel_message
        );

        let summary = self.story_service.get_latest_story().await?;
        let dialogues = self.story_service.get_latest_dialogues().await?;
        let mut dialogues = dialogues
            .iter()
            .map(|d| {
                format!(
                    "[{}{}]：{}",
                    d.author_name,
                    if d.author_character.is_empty() {
                        "".to_string()
                    } else {
                        format!("（{}）", d.author_character)
                    },
                    d.dialogue
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if dialogues.is_empty() {
            dialogues = "<目前无任何对话记录>".to_string();
        }

        let prompt = format!(
            "{}

DM的discord ID为{}

【背景信息-剧情总结（用于理解当前局势）】
{summary}

【背景信息-最近对话（按时间顺序）】
{dialogues}

【说明】
- 上述内容仅作为背景信息
- 请基于这些信息进行判断
- 不要重复或总结上述内容",
            prompt, self.dm_discord_id
        );

        let client = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&prompt)
            .tools(vec![
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
            ])
            .build();

        let reply = self
            .completion_with_retry(async || {
                let dummy_history: Vec<Message> = vec![];
                client
                    .completion(message.clone(), dummy_history)
                    .await?
                    .send()
                    .await
            })
            .await?;

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();

        self.collect_response(&reply, &mut texts, &mut tool_calls);

        let reply = texts.join("\n");

        Ok(reply)
    }

    async fn store_new_dialogue(
        &mut self,
        ctx: &dyn MessageSender,
        message: &str,
        author_id: &str,
        author_name: &str,
    ) -> Result<(), LlmError> {
        let _ = ctx;
        let prompt = fs::read_to_string(format!("{}/new_dialogue.txt", self.folder_path))?;

        let prompt = format!(
            "{prompt}

DM的discord ID为{}",
            self.dm_discord_id
        );

        let client = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&prompt)
            .tool(NewDialogueToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
                story_service: self.story_service.clone(),
                dialogue: message.to_owned(),
                author_name: author_name.to_owned(),
            })
            .build();

        let reply = self
            .completion_with_retry(async || {
                let dummy_history: Vec<Message> = vec![];
                client
                    .completion(
                        format!(
                            "用户Discord ID {}; 用户名 {}: {}",
                            author_id, author_name, message
                        ),
                        dummy_history,
                    )
                    .await?
                    .send()
                    .await
            })
            .await?;

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();

        self.collect_response(&reply, &mut texts, &mut tool_calls);

        let reply = texts.join("\n");

        info!(reply = %reply, author_id = %author_id, author_name = %author_name, "Stored new dialogue through LLM tool");

        Ok(())
    }

    async fn new_summary(&mut self, ctx: &dyn MessageSender) -> Result<(), LlmError> {
        let _ = ctx;
        let dialogues = self.story_service.get_latest_dialogues().await?;
        if dialogues.len() < self.compile_trigger as usize {
            return Ok(());
        }
        let prompt = fs::read_to_string(format!("{}/new_summary.txt", self.folder_path))?;

        let story = self.story_service.get_latest_story().await?;

        let dialogues = dialogues
            .iter()
            .map(|d| {
                format!(
                    "[玩家{}{}]：{}",
                    d.author_name,
                    if d.author_character.is_empty() {
                        "".to_string()
                    } else {
                        format!("（角色名：{}）", d.author_character)
                    },
                    d.dialogue
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let message = format!(
            "【历史剧情总结】
{story}

【最近对话记录（按时间顺序）】
{dialogues}

【任务】
请基于以上信息，生成一段更新后的完整剧情总结。"
        );

        let client = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&prompt)
            .build();

        let reply = self
            .completion_with_retry(async || {
                let dummy_history: Vec<Message> = vec![];
                client
                    .completion(message.clone(), dummy_history)
                    .await?
                    .send()
                    .await
            })
            .await?;

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();

        self.collect_response(&reply, &mut texts, &mut tool_calls);

        let res = texts.join("\n");

        self.story_service.insert_new_story(&res).await?;

        self.story_service.clear_dialogue_table().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, fs, sync::Arc, thread::sleep, time::Duration};

    use chrono::Utc;
    use insta::assert_json_snapshot;
    use serenity::async_trait;
    use sqlx::{Pool, Postgres};

    use crate::{
        character::{
            entity::{
                CharacterSheet,
                abilities_block::AbilitiesBlock,
                combat::Combat,
                identity::{Characteristics, Identity},
                inventory::Inventory,
                meta::Meta,
                notes::Notes,
                progression::Progression,
                skills::Skills,
                spells::Spells,
                traits::Traits,
            },
            repository::CharacterSheetRepository,
            service::CharacterSheetService,
        },
        config::{AiDmConfig, ServiceConfig},
        discord_bot::MessageSender,
        llm::{LLM, gemini::Gemini},
        pg_pool::{TestPgPool, TestPgPoolConfig},
        story::{
            entity::{DialogueEntity, StoryEntity},
            repository::{DialogueRepository, StoryRepository},
            service::StoryService,
        },
        tool::service::ToolService,
    };

    struct MockMessageSender;

    #[async_trait]
    impl MessageSender for MockMessageSender {
        async fn send(&self, msg: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            println!("Mock send message: {}", msg);
            Ok(())
        }
    }

    async fn service_setup() -> Result<
        (Gemini, Pool<Postgres>, String, Arc<CharacterSheetService>),
        Box<dyn std::error::Error>,
    > {
        let service_config: ServiceConfig<AiDmConfig> =
            ServiceConfig::load("./config/config.yaml")?;
        let db_config = service_config
            .database
            .as_ref()
            .ok_or_else(|| crate::error::Error::MissingConfig("database"))?;
        let timestamp = Utc::now().timestamp();
        let db_name = format!("test_db_{}_{}", timestamp, rand::random::<u16>());
        let pg_pool = TestPgPool::init(TestPgPoolConfig {
            migrations: "./migrations".into(),
            db_name: db_name.clone(),
            host: db_config.host.clone(),
            port: db_config.port,
            default_database: "postgres".to_owned(),
            username: db_config.username.clone(),
            password: db_config.password.clone(),
        })
        .await;
        let pool = pg_pool.resource().await;

        let character_sheet_repository =
            Arc::new(CharacterSheetRepository::from_pool(pool.clone()));
        let story_repository = Arc::new(StoryRepository::from_pool(pool.clone()));
        let dialogue_repository = Arc::new(DialogueRepository::from_pool(pool.clone()));

        let character_sheet_service = Arc::new(CharacterSheetService {
            repo: character_sheet_repository,
        });
        let story_service = Arc::new(StoryService {
            repository: story_repository,
            dialogue_repository,
            compile_trigger: 10,
        });
        let tool_service = Arc::new(ToolService::new(
            character_sheet_service.clone(),
            story_service.clone(),
        ));

        Ok((
            Gemini {
                model: "gemini-3.1-flash-lite-preview".to_owned(),
                story_service,
                character_sheet_service: character_sheet_service.clone(),
                cached_context: HashMap::new(),
                dm_discord_id: "1483098634601107476".to_owned(),
                folder_path: "./prompts".to_string(),
                compile_trigger: 4,
                tool_service,
                retry_attempt: 20,
            },
            pool,
            db_name,
            character_sheet_service,
        ))
    }

    fn test_character() -> Result<CharacterSheet, Box<dyn std::error::Error>> {
        let character_json = fs::File::open("./src/llm/test_character/test_character.json")?;
        let character: CharacterSheet = serde_json::from_reader(character_json)?;
        Ok(character)
    }

    #[tokio::test]
    async fn test_add_spells() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;

        let res = gemini_service
            .add_character_spells(&message_sender, "1483098634601107486", "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                "1483098634601107486",
                "anyTHING",
                "我目前会光亮术、寒冰射线和魔法飞弹，没有其他的了，用的魅力值作为判定属性，魅力加值3，熟练加值2，DC 13，一环法术位4个二环法术位2个，无消耗",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, "1483098634601107486", "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service
            .get_character("1483098634601107486")
            .await?;

        assert_json_snapshot!(character.magic);

        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_meta() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107487";

        let res = gemini_service
            .add_character_meta(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "我的位置是银月城酒馆，剧情摘要是刚加入冒险队伍，没有额外生物，没有死亡，正在喝茶，行动结束时间是Harptos 24 Mar 1555 12:35PM",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.meta);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_identity() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107488";

        let res = gemini_service
            .add_character_identity(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "角色名叫艾琳，是高等精灵，职业法师，子职业防护学派，背景是贤者，背景特性是研究员，阵营守序善良，女性，蓝眼，体型中型，身高5尺6寸，信仰密斯特拉，银发，白皙皮肤，年龄120岁，体重110磅，性格特质是喜欢记录所有奥秘，理念是知识应被守护，牵绊是导师留下的法典，缺陷是过度好奇，外貌特征有银色长发和蓝色长袍",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.identity);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_progression() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107489";

        let res = gemini_service
            .add_character_progression(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "角色等级3级，经验900，总生命骰3d8，最大生命值24，熟练加值2，护甲熟练轻甲和中甲，武器熟练长剑和短弓，工具熟练盗贼工具，语言通用语和精灵语",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.progression);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_combat() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107490";
        let mut character = test_character()?;
        character.meta.discord_id = discord_id.to_owned();
        character_sheet_service.upsert_character(character).await?;

        let res = gemini_service
            .add_character_combat(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "护甲等级16，先攻2，当前生命值20，速度步行30尺，感官黑暗视觉60尺，抗性火焰，免疫无，易伤无，没有状态，力竭0级，豁免熟练体质和魅力，动作有攻击、冲刺、撤离，战斗动作有长剑命中加5伤害1d8+3、短弓命中加4伤害1d6+2",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.combat);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_inventory() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107491";

        let res = gemini_service
            .add_character_inventory(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "物品栏有长剑一把，重量3，价值15金币，已装备；治疗药水2瓶，每瓶重量1，价值50金币，未装备；旅行者衣服一套，重量4，价值2金币，未装备",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.inventory);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_abilities() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107492";

        let res = gemini_service
            .add_character_abilities(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "力量10，敏捷14，体质12，智力16，感知13，魅力8，所有额外修正值都是0",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.abilities_block);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_skills() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107493";
        let mut character = test_character()?;
        character.meta.discord_id = discord_id.to_owned();
        character_sheet_service.upsert_character(character).await?;

        let res = gemini_service
            .add_character_skills(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "技能熟练项是奥秘、历史、调查、察觉，其他技能不熟练。所有技能的属性按DND默认：运动力量，体操敏捷，巧手敏捷，隐匿敏捷，奥秘历史调查自然宗教智力，驯兽洞悉医药察觉生存感知，欺瞒威吓表演游说魅力。被动值使用默认0",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.skills);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_traits() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107494";

        let res = gemini_service
            .add_character_traits(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "已解锁特性有黑暗视觉，描述是在微光中视为明亮、黑暗中视为微光，距离60尺；精类血统，描述是魅惑豁免有优势且不能被魔法睡眠。锁定特性有额外攻击，描述是每次攻击动作可攻击两次，5级解锁，没有其他解锁条件",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.traits);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_notes() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;
        let discord_id = "1483098634601107495";

        let res = gemini_service
            .add_character_notes(&message_sender, discord_id, "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                discord_id,
                "anyTHING",
                "组织是银月法师会，盟友是导师赛琳，敌人是红袍巫师，背景故事是从烛堡出发寻找失落星图，其他备注是喜欢收集古代硬币",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, discord_id, "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.notes);
        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_judge() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "系统提示：角色创建完成。
        你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
            )
            .await?;

        assert_eq!(response, "收到，规则已就绪。作为审计员，我已记录本次初始设置。

        鉴于目前处于角色创建完成阶段，请 DM 告知相关角色的名称，以便我将该角色关联至后续的审计流程中，并建立初始状态记录。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
            )
            .await?;

        assert_eq!(response, "收到，地下城主。我已经记录当前的剧情背景，并随时准备对涉及游戏规则、角色数据变动或判定逻辑的内容进行审核。

        目前该消息属于剧情描述与氛围铺陈，不涉及具体的数值变更、角色判定或物品管理，因此无需进行强制性规则校验。请继续您的叙述或发起后续的判定。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
            )
            .await?;

        assert_eq!(response, "好的，我已收到关于角色“泽阿里尔”和“庄芳宜”的状态更新。

        根据你的指令，我将对以上叙述进行规则审计：

        1.  **动作审核**：
            *   **泽阿里尔**：“整理装备”。这是一个叙述性的动作，不涉及数值改动或规则判定。
            *   **庄芳宜**：“走进酒馆开始喝酒”。同样属于角色扮演的叙述，不涉及硬性规则冲突，但请注意，如果在饮酒过程中伴随有酒量检定（如体质豁免）或特殊消耗（金币购买），请在后续的对话中具体说明，届时我将对该操作进行规则验证。

        2.  **数据变更**：
            *   当前消息不涉及任何数据修改（如物品增减、属性变动等），因此无需调用修改工具。

        **审计结论**：该描述符合一般游戏流程，无规则违规，可以继续进行。请在接下来的游戏进程中，如果有具体的规则判定请求（如检定、战斗、物品交易），请及时告知。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "庄芳宜使用了大剑斩杀酒馆混混，造成了不小的骚动。",
            )
            .await?;

        assert_eq!(response, "关于“庄芳宜使用大剑斩杀酒馆混混”这一行为，由于数据库中未能查找到名为“庄芳宜”的角色卡，我无法对其当前的数据状态（如物品属性、等级及行为能力）进行具体的规则校验。

        请确认：
        1. 该角色是否已登记在册？（名称是否存在拼写错误）
        2. 如果该角色为临时NPC或玩家角色，请提供其角色卡相关数据以便我进行后续的合规性审计。

        此外，作为规则审计员，提醒DM注意：在《龙与地下城》规则中，**“斩杀”**（即直接导致角色死亡）通常涉及复杂的战斗判定与伤害计算。请确保在叙述该行为时，已遵循战斗规则中的攻击检定、伤害投掷及死亡判定程序。");

        let test_character = test_character()?;

        character_sheet_service
            .upsert_character(test_character)
            .await?;

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠刚用毛笔画（变）出了一头牛击退了敌人",
            )
            .await?;

        assert_eq!(response, "审核员报告：

        针对“真珠”使用毛笔画出牛并击退敌人的描述，根据其角色卡（术士，2级）及 D&D 规则进行如下审计：

        1. **规则可行性分析**：
            *   **法术/能力判定**：真珠的角色卡中目前已知法术仅包含《法术戏法》和《魔法飞弹》，没有名为“画物成真”或类似召唤牛只的法术。
            *   **职业特性**：术士并未直接拥有“通过绘画创造生物”的职业能力。
            *   **动作判定**：若该行为被定义为法术，则应属于消耗法术位（或其他资源）的动作；若作为戏法，则需满足特定的法术列表规则。

        2. **审计结论**：
            *   该描述的行为目前无法在现有角色数据规则中找到对应的法术或特性支持，存在规则疑点。
            *   若 DM 希望保留此设定，请确认该行为是否属于某种特制物品（例如特殊魔法道具）、法术重写（Flavor text/reskinning）或 DM 的额外叙事性裁定。

        3. **操作建议**：
            *   如该行为意在替代某个现有法术，建议说明其对应的法术名称与消耗的资源（如消耗法术位）。
            *   在未明确其规则来源前，暂不对该角色的数值进行变更。请 DM 补充说明该行动的规则逻辑，以便后续核准。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠获得了400点经验值，升级了",
            )
            .await?;

        assert_eq!(response, "审核结果：

        1.  **当前信息核对**：角色“真珠”（Discord ID: 400114655500042240）目前等级为2级，经验值为330。
        2.  **经验值校验**：DM 提到增加 400 点经验值。330 + 400 = 730。根据 D&D 5E 标准升级表，从 2 级升至 3 级需要达到 900 经验值。
        3.  **结论**：
            *   经验值变动后的总额（730）未达到 3 级所需的 900 点阈值。
            *   因此，**当前不支持“升级”这一判定**。

        **修正建议**：
        请 DM 核对所发放的经验值数值，或者确认是否采用了自定义升级规则。若按照标准规则，角色“真珠”目前仍应保持在 2 级。本次数据修改（升级）暂不予执行。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠获得了1000点经验值，升级了，学习了新法术冰霜世界",
            )
            .await?;

        assert_eq!(response, "审核结果：

        1. **经验值与等级变动**：
           - 角色“真珠”当前等级为 2 级，经验值为 330。
           - DM 提出增加 1000 经验值。根据 5e 标准规则，从 2 级升级至 3 级需要 900 累计经验值（目前角色已有 330，符合升级条件）。
           - **审核结论**：升级请求符合规则，准予更新。

        2. **新法术学习**：
           - DM 提到学习新法术“冰霜世界”。在 D&D 规则手册中，未查到名为“冰霜世界”的标准法术。
           - **审核结论**：请 DM 核实该法术名称是否为自定义内容（Homebrew）或误称（如原名是否为 *Frostbite* 等）。若为自定义法术，请提供其详细效果（环阶、施法时间、射程、成分、持续时间及效应），以便评估其平衡性。

        **目前状态**：
        - 由于法术名称存在疑义，**暂不执行数据库更新**。请 DM 补充说明“冰霜世界”的具体规则细节，待审核确认后，我将为您执行等级及法术库的同步更新。");

        let updated_character = character_sheet_service
            .get_character_by_name("真珠")
            .await?;

        assert_eq!(updated_character.progression.level, 2);

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠获得了1000点经验值，升级了，学习了新法术蛛网术，新最大HP为22",
            )
            .await?;

        assert_eq!(response, "【事务状态】SUCCESS

        【角色】
        - 名称：真珠 (Pearl)
        - 当前等级：3
        - 当前职业：术士 (Sorcerer - Draconic Bloodline: Gold)

        【事务步骤】
        1. **经验值变更**：增加 1000 XP，当前总经验值 1330 XP。
        2. **等级提升**：经验值满足升级条件，角色从 2 级提升至 3 级。
        3. **属性推演**：
            - **生命值 (HP)**：增加 7 点（固定值 4 + 体质 2 + 龙族血脉 1），最大生命值更新为 23。当前生命值同步补满。
            - **法术位**：获得 2 环法术位（2个），1 环法术位增加至 4 个。
            - **术法点 (Sorcery Points)**：上限提升至 3 点。
        4. **法术学习**：习得 2 环法术《蛛网术 (Web)》。
        5. **超魔选择**：选择了“远程法术 (Distant Spell)”和“瞬发法术 (Quickened Spell)”。
        6. **种族特性更新**：作为 Aasimar，达到 3 级自动解锁《次级恢复术 (Lesser Restoration)》（每日一次）。

        【自动推导结果】
        - **最大 HP**：16 -> 23
        - **法术位**：1环 [4], 2环 [2]
        - **术法点上限**：3
        - **生命骰 (Hit Dice)**：增加 1d6，总计 3d6（数据库显示仍为 2d6，已记录待同步）
        - **种族特性**：解锁《天界传承：次级恢复术》

        【玩家待选择项】
        - 无（所有关键选择已在消息中明确）

        【规则审核结果】
        - **合法性确认**：
            - 术士 3 级允许习得第 4 个已知法术，且允许学习 2 环法术。《蛛网术》合法。
            - 术士 3 级解锁超魔特性，数量为 2 个。所选超魔选项合法。
            - 经验值计算正确。

        【数据库写入状态】已完成核心数据（等级、XP、HP、法术位、已知法术）的提交。

        【最终结论】
        事务已成功提交。真珠现已正式升至 3 级。由于系统限制，部分文本描述类特性（如超魔描述、生命骰文本）建议在角色卡备注中手动同步，核心数值已更新完毕。");

        Ok(())
    }

    #[tokio::test]
    async fn test_new_summary() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, pool, _, _) = service_setup().await?;
        let message_sender = MockMessageSender;

        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 系统提示：角色创建完成。
你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:37:56.756385+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 如果我攻击哥布林会发生什么？我拔出剑冲上去。', 'shaomo1', 'Unknown Adventurer - shaomo1', '1483098634601107489', '2026-05-07 02:38:52.373598+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 街角传来骚动，似乎有人在议论最近失踪的旅人……', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:39:22.069725+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:40:55.613852+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' （系统正在初始化）酒馆里人声鼎沸，冒险者们在讨论最近的失踪事件。', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:41:29.238675+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 我决定先喝酒', 'anyTHING', '庄芳宜', '1483098634601107486', '2026-05-07 02:41:56.587162+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 喝酒。', 'anyTHING', '庄芳宜', '1483098634601107486', '2026-05-07 02:42:19.420479+00')",
            )
            .execute(&pool)
            .await?;

        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_json_snapshot!(response, {
            "[].updated_at" => "[timestamp]",
        });
        Ok(())
    }

    #[tokio::test]
    async fn test_new_dialogue() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, pool, _, _) = service_setup().await?;
        let message_sender = MockMessageSender;

        sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            identity = EXCLUDED.identity
        RETURNING *
        "#,
        )
        .bind("1483098634601107486")
        .bind(Meta::default())
        .bind(Identity {
            character_name: "庄芳宜".to_string(),
            species: "人类".to_string(),
            sub_species: Some("人类".to_string()),
            class: "战士".to_string(),
            sub_class: Some("剑士".to_string()),
            characteristics: Characteristics::default(),
        })
        .bind(Progression::default())
        .bind(Combat::default())
        .bind(AbilitiesBlock::default())
        .bind(Skills::default())
        .bind(Spells::default())
        .bind(Inventory::default())
        .bind(Traits::default())
        .bind(Notes::default())
        .fetch_one(&pool)
        .await?;

        gemini_service
            .store_new_dialogue(&message_sender, "", "", "")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(&message_sender, "当前在建卡步骤：选择职业。可回复：我选战士 / 我选游荡者 / 我选法师 / 我选牧师 / 我选游侠 / 我选圣武士。
如果要退出，回复：取消建卡。", "1483098634601107476", "dnd-5")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "当前在建卡步骤：选择职业，但如果你已经决定背景，可以直接说“我在酒馆喝酒”进入剧情。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "系统提示：角色创建完成。
你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我可能会考虑去酒馆看看情况，但还没决定。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "如果我攻击哥布林会发生什么？我拔出剑冲上去。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "你现在可以自由探索城镇，酒馆就在前方。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "三天后，你已经离开村庄，踏上前往北方的道路。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "（系统正在初始化）酒馆里人声鼎沸，冒险者们在讨论最近的失踪事件。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我决定先喝酒",
                "1483098634601107486",
                "anyTHING",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(&message_sender, "喝酒。", "1483098634601107486", "anyTHING")
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我退出游戏，不再继续剧情。",
                "1483098634601107486",
                "anyTHING",
            )
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_json_snapshot!(response, {
            "[].updated_at" => "[timestamp]",
        });
        Ok(())
    }
}
