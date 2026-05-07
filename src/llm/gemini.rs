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
                        _ => {
                            let split_id = discord_user_id.split("_").collect::<Vec<_>>();
                            let discord_id = split_id.first().ok_or_else(|| {
                                LlmError::MissingContent("discord_user_id".to_string())
                            })?;
                            if *discord_id == self.dm_discord_id {
                                "Dungeon Master".to_string()
                            } else {
                                format!("Unknown Adventurer - {}", discord_username)
                            }
                        }
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
            .conversation_continue(ctx, &discord_user_id, discord_username, &message)
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
            .conversation_continue(ctx, &user_id, "system", &message)
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
                Ability, CharacterSheet,
                abilities_block::{AbilitiesBlock, AbilityScore},
                combat::{Action, Combat, CombatAction, Defenses, SavingThrows, Sense, Speed},
                identity::{Characteristics, Identity},
                inventory::{Inventory, Item},
                magic::{Magic, Spell, SpellSlot, Spells},
                meta::Meta,
                notes::Notes,
                progression::{ProficianciesTrainings, Progression},
                skills::{SkillStatus, Skills},
                traits::{FeatureTraits, Traits},
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
                character_sheet_service: character_sheet_service.clone(),
                cached_context: HashMap::new(),
                dm_discord_id: "1483098634601107476".to_owned(),
                folder_path: "./prompts".to_string(),
                compile_trigger: 4,
            },
            pool,
            db_name,
            character_sheet_service,
        ))
    }

    fn test_character() -> CharacterSheet {
        CharacterSheet {
            meta: Meta {
                discord_id: "400114655500042240".to_owned(),
                location: "Tavern".to_owned(),
                story_summary: Some("Just started to join the game".to_owned()),
                extra_creatures: vec![],
                dead: false,
                performing_action: Some("Drinking".to_owned()),
                action_end_time: Some("Harptos 24 Mar 1555 12:35PM".to_owned()),
            },
            identity: Identity {
                character_name: "真珠".to_owned(),
                species: "Aasimar".to_owned(),
                sub_species: None,
                class: "Sorcerer".to_owned(),
                sub_class: Some("Draconic Bloodline - Gold".to_owned()),
                characteristics: Characteristics {
                    background: "Noble".to_owned(),
                    background_feature: "Position of Privilege".to_owned(),
                    background_feature_description: "Thanks to your noble birth, \
                        people are inclined to think the best of you. \
                        You are welcome in high society, \
                        and people assume you have the right \
                        to be wherever you are. \
                        The common folk make every effort to accommodate you and \
                        avoid your displeasure, and other people of high birth \
                        treat you as a member of the same social sphere. \
                        You can secure an audience with a local noble \
                        if you need to."
                        .to_owned(),
                    alignment: "Lawful neutral".to_owned(),
                    gender: "Female".to_owned(),
                    eyes: "Blue".to_owned(),
                    size: "Medium".to_owned(),
                    height: "5'7″".to_owned(),
                    faith: "Amber Lord".to_owned(),
                    hair: "Blonde".to_owned(),
                    skin: "White".to_owned(),
                    age: 28,
                    weight: "104 lbs.".to_owned(),
                    personality_traits: "My eloquent flattery makes everyone \
                        I talk to feel like the most wonderful and important \
                        person in the world."
                        .to_owned(),
                    ideals: "Responsibility. It is my duty to respect \
                        the authority of those above me, just as those below \
                        me must respect mine. (Lawful)"
                        .to_owned(),
                    bonds: "The common folk must see me as a hero of the people.".to_owned(),
                    flaws: "I too often hear veiled insults and threats \
                        in every word addressed to me, and I'm quick to anger."
                        .to_owned(),
                    appearance_trait: vec![],
                },
            },
            progression: Progression {
                level: 2,
                xp: 330,
                total_hit_dice: "2d6".to_owned(),
                max_hp: 16,
                proficiencies: ProficianciesTrainings {
                    proficiency_bonus: 2,
                    armor: vec![],
                    weapons: vec![
                        "Crossbow".to_owned(),
                        "Light".to_owned(),
                        "Dagger".to_owned(),
                        "Dart".to_owned(),
                        "Quarterstaff".to_owned(),
                        "Sling".to_owned(),
                    ],
                    tools: vec![
                        "Dragonchess Set".to_owned(),
                        "Painter's Supplies".to_owned(),
                    ],
                    languages: vec![
                        "Celestial".to_owned(),
                        "Common".to_owned(),
                        "Draconic".to_owned(),
                    ],
                },
            },
            combat: Combat {
                armor_class: 15,
                initiative: 2,
                hit_points: 16,
                speed: vec![Speed {
                    name: "Walking".to_owned(),
                    value: "30 ft".to_owned(),
                }],
                senses: vec![Sense {
                    name: "Darkvision".to_owned(),
                    value: "60 ft".to_owned(),
                }],
                defenses: Defenses {
                    resistance: vec!["Necrotic".to_owned(), "Radiant".to_owned()],
                    immunities: vec![],
                    vulnerabilities: vec![],
                },
                conditions: vec![],
                exhaustion_level: 0,
                saving_throws: SavingThrows {
                    proficiency: vec![Ability::Constitution, Ability::Charisma],
                    constitution_saving_throws: 0,
                    strength_saving_throws: 0,
                    intelligence_saving_throws: 0,
                    dexterity_saving_throws: 0,
                    wisdom_saving_throws: 0,
                    charisma_saving_throws: 0,
                },
                actions: vec![
                    Action {
                        name: "Attack".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                    Action {
                        name: "Dash".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                    Action {
                        name: "Disengage".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                ],
                combat_actions: vec![
                    CombatAction {
                        name: "Dagger".to_owned(),
                        hit_dc: Some(4),
                        damage: "1d4+2".to_owned(),
                    },
                    CombatAction {
                        name: "Dagger".to_owned(),
                        hit_dc: Some(4),
                        damage: "1d4+2".to_owned(),
                    },
                    CombatAction {
                        name: "Ray of Front".to_owned(),
                        hit_dc: Some(5),
                        damage: "1d8".to_owned(),
                    },
                    CombatAction {
                        name: "Unarmed Strike".to_owned(),
                        hit_dc: Some(1),
                        damage: "0".to_owned(),
                    },
                ],
            },
            abilities_block: AbilitiesBlock {
                strength: AbilityScore {
                    base: 8,
                    modifier: 0,
                },
                dexterity: AbilityScore {
                    base: 14,
                    modifier: 0,
                },
                constitution: AbilityScore {
                    base: 14,
                    modifier: 0,
                },
                intelligence: AbilityScore {
                    base: 10,
                    modifier: 0,
                },
                wisdom: AbilityScore {
                    base: 11,
                    modifier: 0,
                },
                charisma: AbilityScore {
                    base: 17,
                    modifier: 0,
                },
            },
            skills: Skills {
                acrobatics: SkillStatus {
                    prof: false,
                    bonus: 2,
                    modifier: Ability::Dexterity,
                    passive: 0,
                },
                animal_handling: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                arcana: SkillStatus {
                    prof: true,
                    bonus: 2,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                athletics: SkillStatus {
                    prof: false,
                    bonus: -1,
                    modifier: Ability::Strength,
                    passive: 0,
                },
                deception: SkillStatus {
                    prof: false,
                    bonus: 3,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                history: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                insight: SkillStatus {
                    prof: true,
                    bonus: 2,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                intimidation: SkillStatus {
                    prof: true,
                    bonus: 5,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                investigation: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                medicine: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                nature: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                perception: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                performance: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                persuasion: SkillStatus {
                    prof: true,
                    bonus: 0,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                religion: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                sleight_of_hand: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Dexterity,
                    passive: 0,
                },
                stealth: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Dexterity,
                    passive: 0,
                },
                survival: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
            },
            magic: Magic {
                spells: Spells {
                    spells: vec![
                        Spell {
                            name: "Light".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "Touch".to_owned(),
                            hit_dc: Some(13),
                            effect: "Creation".to_owned(),
                        },
                        Spell {
                            name: "Message".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "120 ft.".to_owned(),
                            hit_dc: None,
                            effect: "Communication".to_owned(),
                        },
                        Spell {
                            name: "Minor Illusion".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "30 ft.".to_owned(),
                            hit_dc: None,
                            effect: "Control".to_owned(),
                        },
                        Spell {
                            name: "Prestidigitation".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "10 ft.".to_owned(),
                            hit_dc: None,
                            effect: "Utility".to_owned(),
                        },
                        Spell {
                            name: "Ray of Frost".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "60 ft.".to_owned(),
                            hit_dc: Some(5),
                            effect: "1d8".to_owned(),
                        },
                        Spell {
                            name: "Magic Missile".to_owned(),
                            level: 1,
                            cast_time: "1 action".to_owned(),
                            range: "120 ft.".to_owned(),
                            hit_dc: None,
                            effect: "1d4+1".to_owned(),
                        },
                        Spell {
                            name: "Shield".to_owned(),
                            level: 1,
                            cast_time: "1 reaction".to_owned(),
                            range: "Self".to_owned(),
                            hit_dc: None,
                            effect: "Warding".to_owned(),
                        },
                        Spell {
                            name: "Thunderwave".to_owned(),
                            level: 1,
                            cast_time: "1 action".to_owned(),
                            range: "Self".to_owned(),
                            hit_dc: Some(13),
                            effect: "2d8".to_owned(),
                        },
                    ],
                    spell_slots: vec![SpellSlot {
                        level: 1,
                        slot: 3,
                        used: 0,
                    }],
                    ability_type: Ability::Charisma,
                    ability_modifier: 3,
                    spell_attack: 5,
                    save_dc: 13,
                },
            },
            inventory: Inventory {
                items: vec![
                    Item {
                        name: "Clothes, Fine".to_owned(),
                        weight: 6,
                        quantity: Some(1),
                        cost_gp: 15,
                        equiped: false,
                    },
                    Item {
                        name: "Dagger".to_owned(),
                        weight: 1,
                        quantity: None,
                        cost_gp: 2,
                        equiped: true,
                    },
                ],
            },
            traits: Traits {
                features_and_traits: vec![
                    FeatureTraits {
                        name: "Spellcasting".to_owned(),
                        description: "You can cast known sorcerer spells using CHA as your spellcasting modifier (Spell DC 13, Spell Attack +5). You can use an arcane focus as spellcasting focus."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                    FeatureTraits {
                        name: "Sorcerous Origin".to_owned(),
                        description: "Draconic Bloodline"
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                    FeatureTraits {
                        name: "Dragon Ancestor".to_owned(),
                        description: "You have a specific dragon type as your ancestor. You can speak, read, and write Draconic and you double your proficiency bonus for CHA checks involving dragons. -- Gold Dragon"
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                    FeatureTraits {
                        name: "Draconic Resilience".to_owned(),
                        description: "Your max HP increases by 2. When you aren't wearing armor, your AC equals 15."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                    FeatureTraits {
                        name: "Font of Magic".to_owned(),
                        description: "You have 2 sorcery points that you regain when you finish a long rest. You can use your sorcery points to gain additional spell slots or sacrifice spell slots to gain additional sorcery points as a bonus action."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: Some(0),
                        max_charges: Some(2),
                    },
                    FeatureTraits {
                        name: "Healing Hands".to_owned(),
                        description: "Once per long rest as an action, youch a creature and restore 2 hit points."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: Some(0),
                        max_charges: Some(1),
                    },
                    FeatureTraits {
                        name: "Ability Score Increase".to_owned(),
                        description: "Your Wisdom score increases by 1, and your Charisma score increases by 2."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                    FeatureTraits {
                        name: "Celestial Resistance".to_owned(),
                        description: "You have resistance to necrotic damage and radiant damage."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                    FeatureTraits {
                        name: "Celestial Legacy".to_owned(),
                        description: "You know the light cantrip. [3rd] You can cast lesser restoration once per long rest. [5th] You can cast daylight once per long rest. CHA is your spellcasting ability."
                            .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                        used_charges: None,
                        max_charges: None,
                    },
                ],
            },
            notes: Notes {
                organizations: None,
                allies: None,
                enemies: None,
                backstory: "Pearl is a rank P45 senior manager of the Strategic Investment \
            Department in the Interastral Peace Corporation, a member of the Ten Stonehearts, \
            the leader of Pearluxe Corp, and the CEO of Planarcadia."
                    .to_owned(),
                other: None,
            },
        }
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

        let test_character = test_character();

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
                        "真珠获得了1000点经验值，升级了，学习了新法术蛛网术，选择了远程法术和瞬发法术作为初始超魔特性",
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

        // No dialogues, should not generate summary
        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        // Insert dialogues to trigger summary generation

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
