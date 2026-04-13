use std::{
    collections::{HashMap, VecDeque},
    env,
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
use tracing::{debug, info, warn};

use crate::{
    character::entity::{
        abilities_block::AbilitiesBlock, combat::Combat, identity::Identity, inventory::Inventory,
        magic::Magic, meta::Meta, notes::Notes, progression::Progression, skills::Skills,
        traits::Traits,
    },
    llm::{LLM, error::LlmError},
    tool::{
        service::ToolService,
        types::{
            AbilitiesWithDiscordId, CombatWithDiscordId, GetToolInfo, IdentityWithDiscordId,
            InventoryWithDiscordId, NotesWithDiscordId, ProgressionWithDiscordId,
            SkillsWithDiscordId, SpellsWithDiscordId, TraitsWithDiscordId,
        },
    },
};

const MAIN_PROMPT: &str = include_str!("./../../prompts/main.txt");
const MAIN_FOLLOWUP_PROMPT: &str = include_str!("./../../prompts/main_followup.txt");

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
    dm_discord_id: String,
    tool_service: Arc<ToolService>,
    cached_context: HashMap<String, GenerateContentRequest>,
}

impl Gemini {
    pub fn new(
        model: &str,
        tool_service: Arc<ToolService>,
        dm_discord_id: String,
    ) -> Result<Self, LlmError> {
        let api_key = env::var("GEMINI_API_KEY")?;
        let client = gemini_rust::Gemini::with_model(api_key, model.to_owned())?;
        Ok(Self {
            client,
            dm_discord_id,
            tool_service,
            cached_context: HashMap::new(),
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

    async fn run_with_tools(
        &self,
        contents: &str,
        followup_builder: impl Fn(&str, &str, &serde_json::Value) -> String,
    ) -> Result<String, LlmError> {
        let text = self.call_llm(contents).await?;

        let Some((name, args, context)) = self.parse_tool_call(&text)? else {
            return Ok(text);
        };

        let tool_result = self.tool_service.dispatch(args).await?;

        let followup_prompt =
            followup_builder(&name, &serde_json::to_string(&tool_result)?, &context);

        self.call_llm(&followup_prompt).await
    }

    async fn call_llm(&self, contents: &str) -> Result<String, LlmError> {
        let response = self
            .client
            .generate_content()
            .with_user_message(contents)
            .execute()
            .await?;

        Ok(response.text())
    }

    fn parse_tool_call(
        &self,
        text: &str,
    ) -> Result<Option<(String, serde_json::Value, serde_json::Value)>, LlmError> {
        debug!("Parsing tool call from text: {}", text);
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            return Ok(None);
        };

        let Some(tool_call) = json.get("tool_call") else {
            return Ok(None);
        };

        let name = tool_call
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| LlmError::InvalidResponse("Missing tool name".into()))?;

        let args = tool_call
            .get("arguments")
            .cloned()
            .ok_or_else(|| LlmError::InvalidResponse("Missing arguments".into()))?;

        let context = tool_call
            .get("context")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        Ok(Some((name.to_string(), args, context)))
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
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
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
                return Ok(response.text());
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
                    self.cached_context.remove(&args.discord_id);
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

    async fn add_character_spells(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<SpellsWithDiscordId, Magic>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的法术相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色法术以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些法术信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_abilities(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<AbilitiesWithDiscordId, AbilitiesBlock>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的能力相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色能力以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些能力信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_skills(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<SkillsWithDiscordId, Skills>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的技能相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色技能以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些技能信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_traits(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<TraitsWithDiscordId, Traits>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的特性相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色特性以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些特性信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_notes(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<NotesWithDiscordId, Notes>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的笔记相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色笔记以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些笔记信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_meta(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<Meta, Meta>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色元数据相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色元数据以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些笔记信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_identity(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<IdentityWithDiscordId, Identity>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的身份相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色身份信息以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些身份信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_progression(
        &mut self,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        self.add_character_with_tool::<ProgressionWithDiscordId, Progression>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的进度相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色进度以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些进度信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_combat(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<CombatWithDiscordId, Combat>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的战斗相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色战斗以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些战斗信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_inventory(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool::<InventoryWithDiscordId, Inventory>(
            discord_user_id,
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的物品栏相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入，不要询问或关心任何超过角色物品以外的任何事物， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些物品栏信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn request_to_llm(
        &self,
        _discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.run_with_tools(discord_channel_message, |name, result, context| {
            MAIN_FOLLOWUP_PROMPT
                .to_string()
                .replace("{discord_channel_message}", discord_channel_message)
                .replace("{tool_name}", name)
                .replace("{tool_result}", result)
                .replace("{context}", &context.to_string())
        })
        .await
    }
}
