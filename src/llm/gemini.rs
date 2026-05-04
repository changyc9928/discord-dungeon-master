use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use gemini_rust::{
    Content, ContentBuilder, FunctionCall, FunctionDeclaration, FunctionResponse,
    GenerateContentRequest, GenerationResponse, Part, Role, Tool,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::{
    character::{
        entity::{
            CharacterSheet, abilities_block::AbilitiesBlock, combat::Combat, identity::Identity,
            inventory::Inventory, magic::Magic, meta::Meta, notes::Notes, progression::Progression,
            skills::Skills, traits::Traits,
        },
        service::CharacterSheetService,
    },
    discord_bot::MessageSender,
    llm::{LLM, error::LlmError},
    story::service::StoryService,
    tool::{
        service::ToolService,
        types::{
            AbilitiesWithDiscordId, AddItemRequest, AddSpellRequest, CombatWithDiscordId,
            GetCharacterByNameRequest, GetCharacterRequest, GetToolInfo, IdentityWithDiscordId,
            InventoryWithDiscordId, NewDialogueRequest, NotesWithDiscordId,
            ProgressionWithDiscordId, RemoveItemRequest, SkillsWithDiscordId, SpellsWithDiscordId,
            TraitsWithDiscordId, UpdateCharacterLevelRequest, UpdateCurrentHpRequest,
            UpdateMaxHpRequest, UpdateSpellSlotsRequest,
        },
    },
};

#[derive(Serialize, Deserialize, JsonSchema)]
struct RemoveCacheRequest {
    discord_id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct RemoveCacheResponse {
    cache_removed: bool,
    discord_id: String,
}

#[derive(Clone, Debug)]
enum InternalTool {
    RemoveCache,
}

impl InternalTool {
    fn name(&self) -> &'static str {
        match self {
            InternalTool::RemoveCache => "remove_cache",
        }
    }
}

pub struct Gemini {
    client: gemini_rust::Gemini,
    tool_service: Arc<ToolService>,
    story_service: Arc<StoryService>,
    character_sheet_service: Arc<CharacterSheetService>,
    cached_context: HashMap<String, GenerateContentRequest>,
    dm_discord_id: String,
    folder_path: String,
    compile_trigger: i64,
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
    ) -> Result<Self, LlmError> {
        let api_key = env::var("GEMINI_API_KEY")?;
        let client = gemini_rust::Gemini::with_model(api_key, model.to_owned())?;
        Ok(Self {
            client,
            tool_service,
            story_service,
            character_sheet_service,
            cached_context: HashMap::new(),
            dm_discord_id,
            folder_path,
            compile_trigger,
        })
    }

    /// Extract function calls from content parts into a queue (FIFO)
    fn extract_function_calls(&self, contents: &[Content]) -> VecDeque<FunctionCall> {
        let mut function_queue = VecDeque::new();

        for content in contents {
            if let Some(parts) = &content.parts {
                for part in parts {
                    if let Part::FunctionCall { function_call, .. } = part {
                        function_queue.push_front(function_call.clone());
                    }
                    if let Part::FunctionResponse { function_response } = part {
                        if let Some(last_call) = function_queue.pop_front() {
                            if last_call.name != function_response.name {
                                warn!(
                                    "Warning: Function response name '{}' does not match last function call name '{}'",
                                    function_response.name, last_call.name
                                );
                            }
                        } else {
                            warn!(
                                "Warning: Function response name '{}' has no matching function call",
                                function_response.name
                            );
                        }
                    }
                }
            }
        }

        function_queue
    }

    /// Build a tool with the given function declarations
    fn build_tool<F, G>(&self) -> Result<Tool, LlmError>
    where
        F: JsonSchema + GetToolInfo + Serialize,
        G: JsonSchema + Serialize,
    {
        let tool_info = F::get_tool_name();

        let tool_call = FunctionDeclaration::new(tool_info.0, tool_info.1, None)
            .with_parameters::<F>()
            .with_response::<G>();

        // let clear_cache = FunctionDeclaration::new(
        //     InternalTool::RemoveCache.name(),
        //     "对话结束后你能使用这个工具来移除上下文的缓存",
        //     None,
        // )
        // .with_parameters::<RemoveCacheRequest>()
        // .with_response::<RemoveCacheResponse>();

        Ok(Tool::with_functions(vec![tool_call]))
    }

    fn merge_request(
        &mut self,
        ori_request: GenerateContentRequest,
        discord_user_id: &str,
    ) -> Result<ContentBuilder, LlmError> {
        let mut request = self.client.generate_content();
        let cache = self
            .cached_context
            .get(discord_user_id)
            .cloned()
            .unwrap_or(self.client.generate_content().build());

        request.contents.extend(cache.contents);

        if let Some(config) = cache.generation_config {
            request = request.with_generation_config(config);
        }

        if let Some(config) = cache.tool_config {
            request = request.with_tool_config(config);
        }

        if let Some(prompt) = cache.system_instruction {
            if let Some(part) = prompt.parts {
                for part in part {
                    if let Part::Text { text, .. } = part {
                        request = request.with_system_instruction(text);
                    }
                }
            }
        }

        if let Some(tool) = cache.tools {
            for tool in tool {
                request = request.with_tool(tool);
            }
        }

        request.contents.extend(ori_request.contents);

        if let Some(config) = ori_request.generation_config {
            request = request.with_generation_config(config);
        }

        if let Some(config) = ori_request.tool_config {
            request = request.with_tool_config(config);
        }

        if let Some(prompt) = ori_request.system_instruction {
            if let Some(part) = prompt.parts {
                for part in part {
                    if let Part::Text { text, .. } = part {
                        request = request.with_system_instruction(text);
                    }
                }
            }
        }

        if let Some(tool) = ori_request.tools {
            for tool in tool {
                request = request.with_tool(tool);
            }
        }

        let request_copy = request.clone().build();
        self.cached_context
            .insert(discord_user_id.to_owned(), request_copy);

        Ok(request)
    }

    /// Helper method to add a character with a specific tool
    async fn add_character_with_tool<F, G>(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
        prompt: &str,
    ) -> Result<String, LlmError>
    where
        F: JsonSchema + GetToolInfo + Serialize,
        G: JsonSchema + Serialize,
    {
        let tool = self.build_tool::<F, G>()?;

        let clear_cache = FunctionDeclaration::new(
            InternalTool::RemoveCache.name(),
            "对话结束后你能使用这个工具来移除上下文的缓存",
            None,
        )
        .with_parameters::<RemoveCacheRequest>()
        .with_response::<RemoveCacheResponse>();

        let request = self
            .client
            .generate_content()
            .with_tool(tool.clone())
            .with_tool(Tool::with_functions(vec![clear_cache]))
            .with_system_instruction(prompt)
            .build();

        debug!("Request: {:?}", request);

        self.merge_request(request, discord_user_id)?;

        self.conversation_continue(
            ctx,
            discord_user_id,
            discord_username,
            &format!(
                "我的Discord ID是{}，你好，请问你需要什么信息？",
                discord_user_id
            ),
        )
        .await
    }

    async fn execute_with_retry(
        &self,
        request: gemini_rust::generation::builder::ContentBuilder,
    ) -> Result<GenerationResponse, gemini_rust::client::Error> {
        let mut attempts = 0;
        let max_retries = 20;

        loop {
            match request.clone().execute().await {
                Ok(res) => return Ok(res),

                Err(e) => {
                    match &e {
                        gemini_rust::client::Error::BadResponse { code, .. } if *code == 503 => {
                            attempts += 1;

                            if attempts > max_retries {
                                return Err(e);
                            }

                            // simple backoff (can improve later)
                            let delay = Duration::from_secs(2_u64.pow(attempts));
                            sleep(delay).await;
                        }

                        _ => return Err(e), // propagate all other errors immediately
                    }
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
        let mut remove_cache_flag = None;
        info!(discord_user_id = %discord_user_id, "Starting conversation_continue");

        if !self.cached_context.contains_key(discord_user_id) {
            return Err(LlmError::CacheError(format!(
                "No cached context for Discord user ID: {}",
                discord_user_id
            )));
        }

        let builder = self
            .client
            .generate_content()
            .with_user_message(discord_channel_message)
            .build();

        let cached_content = self.merge_request(builder, discord_user_id)?.build();

        debug!("First cache: {:?}", cached_content);

        loop {
            let request = self.client.generate_content().build();
            let request = self.merge_request(request, discord_user_id)?;

            debug!("Full request: {:?}", request.clone().build());

            let response = self.execute_with_retry(request).await?;

            let contents = response
                .candidates
                .clone()
                .into_iter()
                .map(|c| c.content)
                .collect::<Vec<_>>();

            debug!("Responded content: {:?}", contents);

            let function_queue = self.extract_function_calls(&contents);

            let mut new_cache = self.client.generate_content();
            new_cache.contents.extend(contents);
            self.merge_request(new_cache.build(), discord_user_id)?;

            if function_queue.is_empty() {
                if let Some(id) = remove_cache_flag {
                    self.cached_context.remove(&id);
                }
                return Ok(response.text());
            }

            let response_text = response.text();
            if !response_text.is_empty() {
                let response = ctx.send(response_text).await;
                if let Err(e) = response {
                    let response = self.client.generate_content();
                    let response = response.with_user_message(e.to_string());
                    self.merge_request(response.build(), discord_user_id)?;
                }
            }

            let mut function_response = self.client.generate_content();

            for function_call in function_queue {
                info!(
                    discord_user_id = %discord_user_id,
                    tool_name = %function_call.name,
                    "Function call received"
                );

                debug!(
                    tool_name = %function_call.name,
                    args = %serde_json::to_string_pretty(&function_call.args).unwrap_or_default(),
                    "Tool call details"
                );

                let res = if function_call.name == InternalTool::RemoveCache.name() {
                    debug!(discord_user_id = %discord_user_id, "Handling remove_cache");
                    let args: RemoveCacheRequest =
                        serde_json::from_value(function_call.args.clone())?;
                    remove_cache_flag = Some(args.discord_id);
                    serde_json::to_value(RemoveCacheResponse {
                        cache_removed: true,
                        discord_id: discord_user_id.to_owned(),
                    })?
                } else if function_call.name == NewDialogueRequest::get_tool_name().0 {
                    debug!(
                        discord_user_id = %discord_user_id,
                        tool_name = %function_call.name,
                        "Storing new dialogue"
                    );

                    let character = self
                        .character_sheet_service
                        .get_character(
                            discord_user_id
                                .split("_")
                                .collect::<Vec<_>>()
                                .first()
                                .ok_or_else(|| {
                                    LlmError::MissingContent("discord_user_id".to_string())
                                })?,
                        )
                        .await;
                    let character_name = match character {
                        Ok(character) => character.identity.character_name,
                        _ => format!(
                            "Unknown Adventurer - {}",
                            discord_user_id
                                .split("_")
                                .collect::<Vec<_>>()
                                .first()
                                .ok_or_else(|| {
                                    LlmError::MissingContent("discord_user_id".to_string())
                                })?
                        ),
                    };
                    serde_json::to_value(
                        self.story_service
                            .insert_new_dialogue(
                                discord_channel_message
                                    .split_once(":")
                                    .ok_or_else(|| {
                                        LlmError::MissingContent(
                                            "discord_channel_message".to_string(),
                                        )
                                    })?
                                    .1,
                                discord_username,
                                &character_name,
                                discord_user_id
                                    .split("_")
                                    .collect::<Vec<_>>()
                                    .first()
                                    .ok_or_else(|| {
                                        LlmError::MissingContent("discord_user_id".to_string())
                                    })?,
                            )
                            .await?,
                    )?
                } else {
                    debug!(
                        discord_user_id = %discord_user_id,
                        tool_name = %function_call.name,
                        "Dispatching tool to service"
                    );

                    let response = self
                        .tool_service
                        .dispatch(serde_json::to_value(&function_call)?)
                        .await;

                    match response {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::error!(
                                discord_user_id = %discord_user_id,
                                tool_name = %function_call.name,
                                error = %e,
                                "Error executing tool"
                            );
                            serde_json::to_value(json!({
                                "result": "Error calling tool",
                                "error": e.to_string()
                            }))?
                        }
                    }
                };

                let content = Content::function_response(FunctionResponse::from_schema(
                    function_call.name.clone(),
                    res,
                )?)
                .with_role(Role::User);

                function_response.contents.push(content);
            }

            self.merge_request(function_response.build(), discord_user_id)?;
        }
    }

    async fn add_character_spells(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/add_character_spells.txt", self.folder_path))?;
        self.add_character_with_tool::<SpellsWithDiscordId, Magic>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_abilities(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt =
            fs::read_to_string(format!("{}/add_character_abilities.txt", self.folder_path))?;
        self.add_character_with_tool::<AbilitiesWithDiscordId, AbilitiesBlock>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_skills(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/add_character_skills.txt", self.folder_path))?;
        self.add_character_with_tool::<SkillsWithDiscordId, Skills>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_traits(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/add_character_traits.txt", self.folder_path))?;
        self.add_character_with_tool::<TraitsWithDiscordId, Traits>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_notes(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/add_character_notes.txt", self.folder_path))?;
        self.add_character_with_tool::<NotesWithDiscordId, Notes>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_meta(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/add_character_meta.txt", self.folder_path))?;
        self.add_character_with_tool::<Meta, Meta>(ctx, discord_username, discord_user_id, &prompt)
            .await
    }

    async fn add_character_identity(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt =
            fs::read_to_string(format!("{}/add_character_identity.txt", self.folder_path))?;
        self.add_character_with_tool::<IdentityWithDiscordId, Identity>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_progression(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!(
            "{}/add_character_progression.txt",
            self.folder_path
        ))?;
        self.add_character_with_tool::<ProgressionWithDiscordId, Progression>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_combat(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/add_character_combat.txt", self.folder_path))?;
        self.add_character_with_tool::<CombatWithDiscordId, Combat>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_inventory(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt =
            fs::read_to_string(format!("{}/add_character_inventory.txt", self.folder_path))?;
        self.add_character_with_tool::<InventoryWithDiscordId, Inventory>(
            ctx,
            discord_username,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn request_to_llm(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{}/main.txt", self.folder_path))?;

        let message = format!(
            "Discord channnel里的用户{}发送了消息：{}",
            discord_user_id, discord_channel_message
        );

        let discord_user_id = format!("{}_{}", discord_user_id, Utc::now().timestamp());
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

        let tool = vec![
            self.build_tool::<GetCharacterRequest, CharacterSheet>()?,
            self.build_tool::<GetCharacterByNameRequest, CharacterSheet>()?,
            self.build_tool::<AddItemRequest, CharacterSheet>()?,
            self.build_tool::<RemoveItemRequest, CharacterSheet>()?,
            self.build_tool::<AddSpellRequest, CharacterSheet>()?,
            self.build_tool::<UpdateSpellSlotsRequest, CharacterSheet>()?,
            self.build_tool::<UpdateCurrentHpRequest, CharacterSheet>()?,
            self.build_tool::<UpdateMaxHpRequest, CharacterSheet>()?,
            self.build_tool::<UpdateCharacterLevelRequest, CharacterSheet>()?,
        ];

        // let clear_cache = FunctionDeclaration::new(
        //     InternalTool::RemoveCache.name(),
        //     "对话结束后你能使用这个工具来移除上下文的缓存",
        //     None,
        // )
        // .with_parameters::<RemoveCacheRequest>()
        // .with_response::<RemoveCacheResponse>();

        let mut request = self.client.generate_content();

        for tool in tool {
            request = request.with_tool(tool);
        }

        // request = request.with_tool(Tool::with_functions(vec![clear_cache]));

        let request = request.with_system_instruction(prompt).build();

        debug!("Request: {:?}", request);

        self.merge_request(request, &discord_user_id)?;

        let reply = self
            .conversation_continue(ctx, discord_username, &discord_user_id, &message)
            .await?;

        self.cached_context.remove(&discord_user_id);

        Ok(reply)
    }

    async fn store_new_dialogue(
        &mut self,
        ctx: &dyn MessageSender,
        message: &str,
        author_id: &str,
        author_name: &str,
    ) -> Result<(), LlmError> {
        let prompt = fs::read_to_string(format!("{}/new_dialogue.txt", self.folder_path))?;

        let prompt = format!(
            "{prompt}

DM的discord ID为{}",
            self.dm_discord_id
        );

        let author_id_with_timestamp = format!("{}_{}", author_id, Utc::now().timestamp());

        let tool = self.build_tool::<NewDialogueRequest, ()>()?;

        let request = self
            .client
            .generate_content()
            .with_system_instruction(prompt)
            .with_tool(tool)
            .build();

        self.merge_request(request, &author_id_with_timestamp)?;

        self.conversation_continue(
            ctx,
            &author_id_with_timestamp,
            author_name,
            &format!(
                "用户Discord ID {}; 用户名 {}: {}",
                author_id, author_name, message
            ),
        )
        .await?;

        self.cached_context.remove(&author_id_with_timestamp);

        Ok(())
    }

    async fn new_summary(&mut self, ctx: &dyn MessageSender) -> Result<(), LlmError> {
        let user_id = format!("summary_{}", Utc::now().timestamp());
        let dialogues = self.story_service.get_latest_dialogues().await?;
        if dialogues.len() < self.compile_trigger as usize {
            return Ok(());
        }
        let prompt = fs::read_to_string(format!("{}/new_summary.txt", self.folder_path))?;

        let request = self
            .client
            .generate_content()
            .with_system_instruction(prompt)
            .build();

        self.merge_request(request, &user_id)?;

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

        let res = self
            .conversation_continue(ctx, "system", &user_id, &message)
            .await?;

        self.story_service.insert_new_story(&res).await?;

        self.story_service.clear_dialogue_table().await?;

        self.cached_context.remove(&user_id);

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc, thread::sleep, time::Duration};

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
                magic::Magic,
                meta::Meta,
                notes::Notes,
                progression::Progression,
                skills::Skills,
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

    async fn service_setup() -> Result<(Gemini, Pool<Postgres>, String), Box<dyn std::error::Error>>
    {
        let service_config: ServiceConfig<AiDmConfig> =
            ServiceConfig::load("./config/config.yaml")?;
        let db_config = service_config
            .database
            .as_ref()
            .ok_or_else(|| crate::error::Error::MissingConfig("database"))?;
        let timestamp = Utc::now().timestamp();
        let db_name = format!("test_db_{}", timestamp);
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

        Ok((
            Gemini {
                client: gemini_rust::Gemini::with_model(
                    "mock_key",
                    "models/gemini-3.1-flash-lite-preview".to_owned(),
                )?,
                tool_service: Arc::new(ToolService::new(
                    Arc::clone(&character_sheet_service),
                    Arc::clone(&story_service),
                )),
                story_service,
                character_sheet_service,
                cached_context: HashMap::new(),
                dm_discord_id: "1483098634601107476".to_owned(),
                folder_path: "./prompts".to_string(),
                compile_trigger: 4,
            },
            pool,
            db_name,
        ))
    }

    #[tokio::test]
    async fn test_new_summary() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, pool, _) = service_setup().await?;
        let message_sender = MockMessageSender;

        // No dialogues, should not generate summary
        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        // Insert 10 dialogues to trigger summary generation

        sqlx::query(
                "INSERT INTO public.dialogues VALUES ('我在。泽阿里尔 已經準備好，但目前還沒有明確的當前場景。我先不硬開酒館或任務。你可以說：『開始遊戲』、『我們在廢棄礦洞入口』，或直接描述現在的位置和你想做的第一個動作。', 'DM', 'Unknown Adventurer - DM', '1483098634601107476', '2026-05-04 03:30:35.856215+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues VALUES ('当前在建卡步骤：选择职业。可回复：我选战士 / 我选游荡者 / 我选法师 / 我选牧师 / 我选游侠 / 我选圣武士。
如果要退出，回复：取消建卡。', 'DM', 'Unknown Adventurer - DM', '1483098634601107476', '2026-05-04 03:30:51.083827+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues VALUES ('街角传来骚动，似乎有人在议论最近失踪的旅人……', 'DM (dnd-5)', 'Unknown Adventurer - DM (dnd-5)', '1483098634601107476', '2026-05-04 03:31:05.36322+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues VALUES ('我决定先喝酒', 'anyTHING', '庄芳宜', '1483098634601107486', '2026-05-04 03:32:05.36322+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues VALUES ('好的庄芳宜，她向酒保温柔的说了一句：“给我来杯你们这儿最好的葡萄酒”，过了一会儿酒保拿了酒过来，庄芳宜以娴熟优雅的姿态细细品尝', 'DM (dnd-5)', 'Unknown Adventurer - DM (dnd-5)', '1483098634601107476', '2026-05-04 03:33:05.36322+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues VALUES ('我要去找酒馆老板吵架，因为他欠我一百万铜币', 'shaomo1', '泽阿里尔', '1483098634601107489', '2026-05-04 03:34:05.36322+00')",
            )
            .execute(&pool)
            .await?;

        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_json_snapshot!(response);
        Ok(())
    }

    #[tokio::test]
    async fn test_new_dialogue() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, pool, _) = service_setup().await?;
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
        .bind(Magic::default())
        .bind(Inventory::default())
        .bind(Traits::default())
        .bind(Notes::default())
        .fetch_one(&pool)
        .await?;

        // Dummy dialogue, should not be stored due to empty content
        gemini_service
            .store_new_dialogue(&message_sender, "", "", "")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(&message_sender, "当前在建卡步骤：选择职业。可回复：我选战士 / 我选游荡者 / 我选法师 / 我选牧师 / 我选游侠 / 我选圣武士。
如果要退出，回复：取消建卡。", "1483098634601107476", "dnd-5")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "当前在建卡步骤：选择职业，但如果你已经决定背景，可以直接说“我在酒馆喝酒”进入剧情。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "系统提示：角色创建完成。
你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我可能会考虑去酒馆看看情况，但还没决定。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "如果我攻击哥布林会发生什么？我拔出剑冲上去。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "你现在可以自由探索城镇，酒馆就在前方。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "三天后，你已经离开村庄，踏上前往北方的道路。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "（系统正在初始化）酒馆里人声鼎沸，冒险者们在讨论最近的失踪事件。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我决定先喝酒",
                "1483098634601107486",
                "anyTHING",
            )
            .await?;

        sleep(Duration::from_secs(30));

        gemini_service
            .store_new_dialogue(&message_sender, "喝酒。", "1483098634601107486", "anyTHING")
            .await?;

        sleep(Duration::from_secs(30));

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
