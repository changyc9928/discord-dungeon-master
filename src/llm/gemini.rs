use std::{
    collections::{HashMap, VecDeque},
    env,
    sync::Arc,
};

use async_trait::async_trait;
use gemini_rust::{Content, FunctionCall, FunctionDeclaration, FunctionResponse, Part, Role, Tool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info};

use crate::{
    character::entity::{CharacterSheet, Meta},
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

#[derive(Serialize, Deserialize)]
struct RemoveCacheRequest {
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
    cached_context: HashMap<String, Vec<Content>>,
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
        let mut queue = VecDeque::new();

        for content in contents {
            if let Some(parts) = &content.parts {
                for part in parts {
                    if let Part::FunctionCall { function_call, .. } = part {
                        queue.push_back(function_call.clone());
                    }
                }
            }
        }

        queue
    }

    /// Build a tool with the given function declarations
    fn build_tool<F, G>(&self, tool_call_object: F, _response_object: G) -> Result<Tool, LlmError>
    where
        F: JsonSchema + GetToolInfo + Serialize,
        G: JsonSchema + Serialize,
    {
        let tool_info = tool_call_object.get_tool_name();

        let tool_call = FunctionDeclaration::new(tool_info.0, tool_info.1, None)
            .with_parameters::<F>()
            .with_response::<G>();

        let clear_cache = FunctionDeclaration::new(
            InternalTool::RemoveCache.name(),
            "对话结束后你能使用这个工具来移除上下文的缓存",
            None,
        );

        Ok(Tool::with_functions(vec![tool_call, clear_cache]))
    }

    /// Initialize context for a new conversation
    async fn initialize_context(
        &self,
        _discord_user_id: &str,
        prompt: &str,
        tool: Tool,
    ) -> Result<Vec<Content>, LlmError> {
        let response = self
            .client
            .generate_content()
            .with_system_instruction(prompt)
            .with_tool(tool)
            .execute()
            .await?;

        Ok(response
            .candidates
            .into_iter()
            .map(|c| c.content)
            .collect::<Vec<_>>())
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

    /// Helper method to add a character with a specific tool
    async fn add_character_with_tool<F, G>(
        &mut self,
        discord_user_id: &str,
        tool_call_object: F,
        response_object: G,
        prompt: &str,
    ) -> Result<String, LlmError>
    where
        F: JsonSchema + GetToolInfo + Serialize,
        G: JsonSchema + Serialize,
    {
        let tool = self.build_tool(tool_call_object, response_object)?;

        let contents = Self::initialize_context(self, discord_user_id, prompt, tool).await?;

        self.cached_context
            .insert(discord_user_id.to_owned(), contents.clone());

        self.conversation_continue(
            discord_user_id,
            &format!("我的Discord ID是{}", discord_user_id),
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

        let cache = self
            .cached_context
            .get(discord_user_id)
            .ok_or_else(|| LlmError::MissingContent(discord_user_id.to_owned()))?;

        let mut request = self.client.generate_content();
        request.contents.extend(cache.iter().cloned());

        let response = request
            .with_user_message(discord_channel_message)
            .execute()
            .await?;

        let contents = response
            .candidates
            .into_iter()
            .map(|c| c.content)
            .collect::<Vec<_>>();

        // Append to cache instead of overwriting
        self.cached_context
            .entry(discord_user_id.to_owned())
            .and_modify(|existing| existing.extend(contents.iter().cloned()))
            .or_insert(contents.clone());

        let function_queue = self.extract_function_calls(&contents);

        if function_queue.is_empty() {
            let final_response = contents
                .first()
                .and_then(|c| c.parts.as_ref())
                .and_then(|parts| parts.first())
                .and_then(|part| match part {
                    Part::Text { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .ok_or_else(|| LlmError::InvalidResponse("No text response from model".into()))?;

            return Ok(final_response);
        }

        let mut reply = self.client.generate_content();
        reply.contents.extend(contents);

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
                let args: RemoveCacheRequest = serde_json::from_value(function_call.args.clone())?;
                self.cached_context.remove(&args.discord_id);
                serde_json::to_value(json!({
                    "removed_cache": true,
                    "discord_id": args.discord_id
                }))?
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

            reply.contents.push(content);
        }

        info!(discord_user_id = %discord_user_id, "Sending function responses to model");

        let final_response = reply.execute().await?;

        let response_text = final_response.text();
        info!(discord_user_id = %discord_user_id, response = %response_text, "Final response from model");

        Ok(response_text)
    }

    async fn add_character_spells(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            SpellsWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的法术相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些法术信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_abilities(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            AbilitiesWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的能力相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些能力信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_skills(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            SkillsWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的技能相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些技能信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_traits(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            TraitsWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的特性相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些特性信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_notes(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            NotesWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的笔记相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些笔记信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_meta(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            Meta::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的元数据相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些元数据信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_identity(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            IdentityWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的身份相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些身份信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_progression(
        &mut self,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            ProgressionWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的进度相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些进度信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_combat(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            CombatWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的战斗相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些战斗信息并移除对话上下文的缓存",
        )
        .await
    }

    async fn add_character_inventory(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.add_character_with_tool(
            discord_user_id,
            InventoryWithDiscordId::default(),
            CharacterSheet::default(),
            "你是一个龙与地下城2024版本的DM助手，\
            你需要用中文引导用户提供的资料录入该角色的物品栏相关信息，\
            玩家向你发出第一次问候之后你必须向玩家提出你的缺失的信息以 \
            完成全部相关信息的录入， \
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
