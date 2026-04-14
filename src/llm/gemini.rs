use std::{
    collections::{HashMap, VecDeque},
    env, fs,
    sync::Arc,
};

use async_trait::async_trait;
use gemini_rust::{
    Content, ContentBuilder, FunctionCall, FunctionDeclaration, FunctionResponse,
    GenerateContentRequest, Part, Role, Tool,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serenity::all::{ChannelId, Context};
use tracing::{debug, info, warn};

use crate::{
    character::entity::{
        CharacterSheet, abilities_block::AbilitiesBlock, combat::Combat, identity::Identity,
        inventory::Inventory, magic::Magic, meta::Meta, notes::Notes, progression::Progression,
        skills::Skills, traits::Traits,
    },
    llm::{LLM, error::LlmError},
    tool::{
        service::ToolService,
        types::{
            AbilitiesWithDiscordId, AddItemRequest, AddSpellRequest, CombatWithDiscordId,
            GetCharacterByNameRequest, GetCharacterRequest, GetToolInfo, IdentityWithDiscordId,
            InventoryWithDiscordId, NotesWithDiscordId, ProgressionWithDiscordId,
            RemoveItemRequest, SkillsWithDiscordId, SpellsWithDiscordId, TraitsWithDiscordId,
            UpdateCharacterLevelRequest, UpdateCurrentHpRequest, UpdateMaxHpRequest,
            UpdateSpellSlotsRequest,
        },
    },
};

const FOLDER_PATH: &str = "/app/prompts";

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
    cached_context: HashMap<String, GenerateContentRequest>,
    channel_id: u64,
}

impl Gemini {
    pub fn new(
        model: &str,
        tool_service: Arc<ToolService>,
        channel_id: u64,
    ) -> Result<Self, LlmError> {
        let api_key = env::var("GEMINI_API_KEY")?;
        let client = gemini_rust::Gemini::with_model(api_key, model.to_owned())?;
        Ok(Self {
            client,
            tool_service,
            cached_context: HashMap::new(),
            channel_id,
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

        let clear_cache = FunctionDeclaration::new(
            InternalTool::RemoveCache.name(),
            "对话结束后你能使用这个工具来移除上下文的缓存",
            None,
        )
        .with_parameters::<RemoveCacheRequest>()
        .with_response::<RemoveCacheResponse>();

        Ok(Tool::with_functions(vec![tool_call, clear_cache]))
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
        ctx: &Context,
        discord_user_id: &str,
        prompt: &str,
    ) -> Result<String, LlmError>
    where
        F: JsonSchema + GetToolInfo + Serialize,
        G: JsonSchema + Serialize,
    {
        let tool = self.build_tool::<F, G>()?;

        let request = self
            .client
            .generate_content()
            .with_tool(tool.clone())
            .with_system_instruction(prompt)
            .build();

        debug!("Request: {:#?}", request);

        self.merge_request(request, discord_user_id)?;

        self.conversation_continue(
            ctx,
            discord_user_id,
            &format!(
                "我的Discord ID是{}，你好，请问你需要什么信息？",
                discord_user_id
            ),
        )
        .await
    }
}

#[async_trait]
impl LLM for Gemini {
    async fn conversation_continue(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let mut remove_cache_flag = None;
        info!(discord_user_id = %discord_user_id, "Starting conversation_continue");

        let builder = self
            .client
            .generate_content()
            .with_user_message(discord_channel_message)
            .build();

        let cached_content = self.merge_request(builder, discord_user_id)?.build();

        debug!("First cache: {:#?}", cached_content);

        loop {
            let request = self.client.generate_content().build();
            let request = self.merge_request(request, discord_user_id)?;

            debug!("Full request: {:#?}", request.clone().build());

            let response = request.execute().await?;

            let contents = response
                .candidates
                .clone()
                .into_iter()
                .map(|c| c.content)
                .collect::<Vec<_>>();

            debug!("Responded content: {:#?}", contents);

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
                let channel_id = ChannelId::new(self.channel_id);
                let response = channel_id.say(ctx, response_text).await;
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
                        Err(e) => serde_json::to_value(json!({
                            "result": "Error calling tool",
                            "error": e.to_string()
                        }))?,
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
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_spells.txt"))?;
        self.add_character_with_tool::<SpellsWithDiscordId, Magic>(ctx, discord_user_id, &prompt)
            .await
    }

    async fn add_character_abilities(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_abilities.txt"))?;
        self.add_character_with_tool::<AbilitiesWithDiscordId, AbilitiesBlock>(
            ctx,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_skills(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_skills.txt"))?;
        self.add_character_with_tool::<SkillsWithDiscordId, Skills>(ctx, discord_user_id, &prompt)
            .await
    }

    async fn add_character_traits(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_traits.txt"))?;
        self.add_character_with_tool::<TraitsWithDiscordId, Traits>(ctx, discord_user_id, &prompt)
            .await
    }

    async fn add_character_notes(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_notes.txt"))?;
        self.add_character_with_tool::<NotesWithDiscordId, Notes>(ctx, discord_user_id, &prompt)
            .await
    }

    async fn add_character_meta(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_meta.txt"))?;
        self.add_character_with_tool::<Meta, Meta>(ctx, discord_user_id, &prompt)
            .await
    }

    async fn add_character_identity(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_identity.txt"))?;
        self.add_character_with_tool::<IdentityWithDiscordId, Identity>(
            ctx,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_progression(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_progression.txt"))?;
        self.add_character_with_tool::<ProgressionWithDiscordId, Progression>(
            ctx,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn add_character_combat(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_combat.txt"))?;
        self.add_character_with_tool::<CombatWithDiscordId, Combat>(ctx, discord_user_id, &prompt)
            .await
    }

    async fn add_character_inventory(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/add_character_inventory.txt"))?;
        self.add_character_with_tool::<InventoryWithDiscordId, Inventory>(
            ctx,
            discord_user_id,
            &prompt,
        )
        .await
    }

    async fn request_to_llm(
        &mut self,
        ctx: &Context,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let prompt = fs::read_to_string(format!("{FOLDER_PATH}/main.txt"))?;

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
            self.build_tool::<UpdateCharacterLevelRequest, CharacterSheet>()?,
        ];

        let clear_cache = FunctionDeclaration::new(
            InternalTool::RemoveCache.name(),
            "对话结束后你能使用这个工具来移除上下文的缓存",
            None,
        )
        .with_parameters::<RemoveCacheRequest>()
        .with_response::<RemoveCacheResponse>();

        let mut request = self.client.generate_content();

        for tool in tool {
            request = request.with_tool(tool);
        }

        request = request.with_tool(Tool::with_functions(vec![clear_cache]));

        let request = request.with_system_instruction(prompt).build();

        debug!("Request: {:#?}", request);

        self.merge_request(request, discord_user_id)?;

        self.conversation_continue(ctx, discord_user_id, discord_channel_message)
            .await
    }
}
