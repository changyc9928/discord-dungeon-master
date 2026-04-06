use std::{collections::HashMap, sync::Arc};

use api_gemini::{
    Content, FunctionDeclaration, FunctionResponse, GenerateContentRequest, Part,
    SystemInstruction, Tool, client::Client,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};

use crate::{
    character::entity::Meta,
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
    client: Client,
    model: String,
    dm_discord_id: String,
    tool_service: Arc<ToolService>,
    cached_context: HashMap<String, GenerateContentRequest>,
}

impl Gemini {
    pub fn new(
        model: String,
        tool_service: Arc<ToolService>,
        dm_discord_id: String,
    ) -> Result<Self, LlmError> {
        let client = Client::new()?;
        Ok(Self {
            client,
            model,
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
        contents: Vec<Content>,
        followup_builder: impl Fn(&str, &str, &serde_json::Value) -> String,
    ) -> Result<String, LlmError> {
        let text = self.call_llm(contents).await?;

        let Some((name, args, context)) = self.parse_tool_call(&text)? else {
            return Ok(text);
        };

        let tool_result = self.tool_service.dispatch(args).await?;

        let followup_prompt =
            followup_builder(&name, &serde_json::to_string(&tool_result)?, &context);

        let followup_contents = vec![self.user_text_content(followup_prompt)];

        self.call_llm(followup_contents).await
    }

    async fn call_llm(&self, contents: Vec<Content>) -> Result<String, LlmError> {
        let request = GenerateContentRequest {
            contents,
            ..Default::default()
        };

        let response = self
            .client
            .models()
            .by_name(&self.model)
            .generate_content(&request)
            .await?;

        response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.as_ref())
            .cloned()
            .ok_or_else(|| {
                LlmError::GeminiError(api_gemini::error::Error::Unknown(
                    "No response from Gemini".to_owned(),
                ))
            })
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

    fn user_text_content(&self, text: impl Into<String>) -> Content {
        Content {
            role: "user".to_string(),
            parts: vec![Part {
                text: Some(text.into()),
                ..Default::default()
            }],
        }
    }

    /// Helper to extract final text from response content
    fn extract_final_text(&mut self, contents: &[Content]) -> Result<String, LlmError> {
        contents
            .last()
            .ok_or_else(|| LlmError::MissingContent("final response".to_owned()))?
            .parts
            .first()
            .ok_or_else(|| LlmError::MissingContent("final response part".to_owned()))?
            .text
            .as_ref()
            .cloned()
            .ok_or_else(|| LlmError::InvalidResponse("final response text".to_owned()))
    }

    /// Generic tool-calling loop that handles conversation with tools
    async fn insert_initial_cache<F>(
        &mut self,
        discord_user_id: &str,
        tool_call_object: F,
        prompt: &str,
    ) -> Result<String, LlmError>
    where
        F: JsonSchema + GetToolInfo,
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

        let context = GenerateContentRequest {
            contents: vec![],
            system_instruction: Some(SystemInstruction {
                role: "system".to_owned(),
                parts: vec![Part {
                    text: Some(prompt.to_owned()),
                    ..Default::default()
                }],
            }),
            tools: Some(vec![Tool {
                function_declarations: Some(vec![
                    FunctionDeclaration {
                        name: tool_info.0,
                        description: tool_info.1,
                        parameters: Some(serde_json::json!({
                            "type": schema.get("type"),
                            "properties": schema.get("properties"),
                            "required": schema.get("required"),
                        })),
                    },
                    FunctionDeclaration {
                        name: "remove_cache".to_owned(),
                        description: "对话结束后你能使用这个工具来移除上下文的缓存".to_owned(),
                        parameters: Some(serde_json::json!({
                            "type": remove_cache_schema.get("type"),
                            "properties": remove_cache_schema.get("properties"),
                            "required": remove_cache_schema.get("required"),
                        })),
                    },
                ]),
                code_execution: None,
                google_search_retrieval: None,
                code_execution_tool: None,
            }]),
            ..Default::default()
        };

        self.cached_context
            .insert(discord_user_id.to_owned(), context);

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

        let mut cache = match self.cached_context.get(discord_user_id) {
            Some(c) => {
                debug!(discord_user_id = %discord_user_id, "Cache hit, using existing context");
                c.clone()
            }
            None => {
                warn!(discord_user_id = %discord_user_id, "Cache miss - no existing context found");
                todo!()
            }
        };

        debug!(discord_user_id = %discord_user_id, "Pushing user message to cache");
        cache.contents.push(Content {
            parts: vec![Part {
                text: Some(discord_channel_message.to_owned()),
                ..Default::default()
            }],
            role: "user".to_owned(),
        });

        debug!(discord_user_id = %discord_user_id, "Calling Gemini API for initial response");
        let mut response = self
            .client
            .models()
            .by_name(&self.model)
            .generate_content(&cache)
            .await?;

        // Push assistant's response to context
        if let Some(candidate) = response.candidates.first() {
            debug!(discord_user_id = %discord_user_id, "Pushing assistant's initial response to cache");
            cache.contents.push(candidate.content.clone());
        }

        // Tool-calling loop
        let mut tool_call_count = 0;
        loop {
            let tool_call = if let Some(candidate) = response.candidates.first() {
                if let Some(part) = candidate.content.parts.first() {
                    part.function_call.as_ref()
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(tool_call) = tool_call {
                tool_call_count += 1;
                info!(
                    discord_user_id = %discord_user_id,
                    tool_name = %tool_call.name,
                    tool_call_count = tool_call_count,
                    "Detected tool call"
                );

                let res = if tool_call.name == "remove_cache" {
                    debug!(discord_user_id = %discord_user_id, "Handling remove_cache special case");
                    let args: RemoveCacheRequest = serde_json::from_value(tool_call.args.clone())?;
                    self.cached_context.remove(&args.discord_id);
                    serde_json::to_value(json!({
                        "removed_cache": true,
                        "discord_id": args.discord_id
                    }))?
                } else {
                    debug!(
                        discord_user_id = %discord_user_id,
                        tool_name = %tool_call.name,
                        "Dispatching tool to service"
                    );
                    // Dispatch tool
                    let response = self
                        .tool_service
                        .dispatch(serde_json::to_value(tool_call)?)
                        .await;
                    match response {
                        Ok(r) => r,
                        Err(e) => serde_json::to_value(json!(
                            {   "result": "Error calling tool",
                                "error": e.to_string()}
                        ))?,
                    }
                };

                // Push tool response to context
                debug!(discord_user_id = %discord_user_id, "Pushing tool response to cache");
                cache.contents.push(Content {
                    role: "tool".to_string(),
                    parts: vec![Part {
                        function_response: Some(FunctionResponse {
                            name: tool_call.name.clone(),
                            response: res,
                        }),
                        ..Default::default()
                    }],
                });

                // Get next response
                debug!(discord_user_id = %discord_user_id, "Calling Gemini API for next response after tool call");
                response = self
                    .client
                    .models()
                    .by_name(&self.model)
                    .generate_content(&cache)
                    .await?;

                // Store this response in context
                if let Some(candidate) = response.candidates.first() {
                    debug!(discord_user_id = %discord_user_id, "Pushing next assistant response to cache");
                    cache.contents.push(candidate.content.clone());
                }
            } else {
                info!(
                    discord_user_id = %discord_user_id,
                    tool_call_count = tool_call_count,
                    "No more tool calls, exiting loop"
                );
                break;
            }
        }

        debug!(discord_user_id = %discord_user_id, "Storing final cache");
        self.cached_context
            .insert(discord_user_id.to_owned(), cache.clone());

        debug!(discord_user_id = %discord_user_id, "Extracting final text from cache");
        self.extract_final_text(&cache.contents)
    }

    async fn add_character_spells(&mut self, discord_user_id: &str) -> Result<String, LlmError> {
        self.insert_initial_cache(
            discord_user_id,
            SpellsWithDiscordId::default(),
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
        let contents = vec![
            Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: Some(
                        MAIN_PROMPT
                            .to_string()
                            .replace("{dm_discord_id}", &self.dm_discord_id),
                    ),
                    ..Default::default()
                }],
            },
            Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: Some(
                        serde_json::json!({
                            "discord_id": discord_user_id,
                            "message": discord_channel_message
                        })
                        .to_string(),
                    ),
                    ..Default::default()
                }],
            },
        ];

        self.run_with_tools(contents, |name, result, context| {
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
