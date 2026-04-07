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
use tracing::{debug, info, warn};

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

    fn clean_schema(&self, value: &mut serde_json::Value) {
        if let Some(obj) = value.as_object_mut() {
            if let Some(t) = obj.get_mut("type") {
                if let Some(arr) = t.as_array() {
                    if arr.len() == 2 && arr.contains(&"null".into()) {
                        let non_null = arr.iter().find(|v| *v != "null").unwrap();
                        *t = non_null.clone();
                    }
                }
            }
            for v in obj.values_mut() {
                self.clean_schema(v);
            }
        }
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
        println!("Parsing tool call from text: {}", text);
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

        let _args = tool_call
            .get("arguments")
            .cloned()
            .ok_or_else(|| LlmError::InvalidResponse("Missing arguments".into()))?;

        let context = tool_call
            .get("context")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        Ok(Some((name.to_string(), tool_call.clone(), context)))
    }

    /// Generic tool-calling loop that handles conversation with tools
    async fn insert_initial_cache<F, G>(
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
        let schema = schemars::schema_for!(F);
        let mut schema = serde_json::to_value(&schema).unwrap();
        self.clean_schema(&mut schema);

        let dummy_instance = RemoveCacheRequest {
            discord_id: "".to_owned(),
        };
        let mut remove_cache_schema = serde_json::to_value(&dummy_instance).unwrap();
        self.clean_schema(&mut remove_cache_schema);

        let tool_info = tool_call_object.get_tool_name();

        let tool_call = FunctionDeclaration::new(tool_info.0, tool_info.1, None)
            .with_parameters::<F>()
            .with_response::<G>();

        let clear_cache = FunctionDeclaration::new(
            "remove_cache",
            "对话结束后你能使用这个工具来移除上下文的缓存",
            None,
        );

        let tool = Tool::with_functions(vec![tool_call, clear_cache]);

        let response = self
            .client
            .generate_content()
            .with_system_instruction(prompt)
            .with_tool(tool)
            .execute()
            .await?;

        self.cached_context.insert(
            discord_user_id.to_owned(),
            response
                .candidates
                .into_iter()
                .map(|c| c.content)
                .collect::<Vec<_>>(),
        );

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
            .ok_or(LlmError::MissingContent(discord_user_id.to_owned()))?
            .clone();

        let mut request = self.client.generate_content();
        request.contents.extend(cache);

        let response = request
            .with_user_message(discord_channel_message)
            .execute()
            .await?;

        let contents = response
            .candidates
            .into_iter()
            .map(|c| c.content)
            .collect::<Vec<_>>();

        self.cached_context
            .insert(discord_user_id.to_owned(), contents.clone());

        let mut function_queue = VecDeque::<FunctionCall>::new();
        for content in &contents {
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

        let mut reply = self.client.generate_content();

        reply.contents.extend(contents);

        for function_call in function_queue {
            info!(
                "Function call received: {} with args:\n{}",
                function_call.name,
                serde_json::to_string_pretty(&function_call.args)?
            );

            let res = if function_call.name == "remove_cache" {
                debug!(discord_user_id = %discord_user_id, "Handling remove_cache special case");
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
                // Dispatch tool
                let response = self
                    .tool_service
                    .dispatch(serde_json::to_value(&function_call)?)
                    .await;
                match response {
                    Ok(r) => r,
                    Err(e) => serde_json::to_value(json!(
                        {   "result": "Error calling tool",
                            "error": e.to_string()}
                    ))?,
                }
            };

            let content = Content::function_response(FunctionResponse::from_schema(
                function_call.name.clone(),
                res,
            )?)
            .with_role(Role::User);

            reply.contents.push(content);
        }

        info!("Sending function response...",);

        let final_response = reply.execute().await?;

        info!("Final response from model: {}", final_response.text(),);

        Ok(final_response.text())
    }

    async fn add_character_spells(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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

    async fn request_to_llm(
        &self,
        discord_user_id: &str,
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

    async fn add_character_meta(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
        self.insert_initial_cache(
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
}
