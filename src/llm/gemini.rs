use std::{collections::HashMap, sync::Arc};

use api_gemini::{
    Content, FunctionCallingConfig, FunctionCallingMode, FunctionDeclaration, FunctionResponse,
    GenerateContentRequest, Part, SystemInstruction, Tool, ToolConfig, client::Client,
};
use async_trait::async_trait;
use schemars::JsonSchema;

use crate::{
    character::{entity::Meta, service::CharacterSheetService},
    llm::{LLM, error::LlmError},
    tool::{
        service::ToolService,
        types::{
            AbilitiesWithDiscordId, CombatWithDiscordId, GetToolInfo, IdentityWithDiscordId,
            InventoryWithDiscordId, NotesWithDiscordId, ProgressionWithDiscordId,
            SkillsWithDiscordId, SpellsWithDiscordId, ToolCall, TraitsWithDiscordId,
        },
    },
};

const MAIN_PROMPT: &str = include_str!("./../../prompts/main.txt");
const MAIN_FOLLOWUP_PROMPT: &str = include_str!("./../../prompts/main_followup.txt");

pub struct Gemini {
    character_sheet_service: Arc<CharacterSheetService>,
    client: Client,
    model: String,
    dm_discord_id: String,
    tool_service: Arc<ToolService>,
    cached_context: HashMap<String, GenerateContentRequest>,
}

impl Gemini {
    pub fn new(
        model: String,
        character_sheet_service: Arc<CharacterSheetService>,
        tool_service: Arc<ToolService>,
        dm_discord_id: String,
    ) -> Result<Self, LlmError> {
        let client = Client::new()?;
        Ok(Self {
            client,
            character_sheet_service,
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

    async fn call_with_tool<T: JsonSchema>(
        &self,
        tool_name: &str,
        system_prompt: &str,
        user_content: &str,
    ) -> Result<String, LlmError> {
        let schema = schemars::schema_for!(T);
        let mut schema = serde_json::to_value(&schema).unwrap();
        self.clean_schema(&mut schema);

        let mut contents = GenerateContentRequest {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: Some(user_content.to_string()),
                    ..Default::default()
                }],
            }],
            system_instruction: Some(SystemInstruction {
                role: "system".to_string(),
                parts: vec![Part {
                    text: Some(system_prompt.to_string()),
                    ..Default::default()
                }],
            }),
            tools: Some(vec![Tool {
                function_declarations: Some(vec![FunctionDeclaration {
                    name: tool_name.to_string(),
                    description: "".to_string(),
                    parameters: Some(serde_json::json!({
                        "type": schema.get("type"),
                        "properties": schema.get("properties"),
                        "required": schema.get("required"),
                    })),
                }]),
                code_execution: None,
                google_search_retrieval: None,
                code_execution_tool: None,
            }]),
            tool_config: Some(ToolConfig {
                function_calling_config: Some(FunctionCallingConfig {
                    mode: FunctionCallingMode::Auto,
                    allowed_function_names: None,
                }),
                code_execution: None,
            }),
            ..Default::default()
        };

        let response = self
            .client
            .models()
            .by_name(&self.model)
            .generate_content(&contents)
            .await?;

        let tool_call = response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .ok_or_else(|| {
                LlmError::GeminiError(api_gemini::error::Error::Unknown(
                    "No tool call from Gemini".to_string(),
                ))
            })?
            .function_call
            .as_ref()
            .ok_or_else(|| {
                LlmError::GeminiError(api_gemini::error::Error::Unknown(
                    "No function call in tool call from Gemini".to_string(),
                ))
            })?;

        let res = self
            .tool_service
            .dispatch(serde_json::to_value(tool_call)?)
            .await?;

        let function_response_contents = Part {
            function_response: Some(FunctionResponse {
                name: tool_call.name.clone(),
                response: res,
            }),
            ..Default::default()
        };

        contents.contents.push(
            response
                .candidates
                .first()
                .ok_or_else(|| {
                    LlmError::GeminiError(api_gemini::error::Error::Unknown(
                        "No candidates from Gemini".to_string(),
                    ))
                })?
                .content
                .clone(),
        );
        contents.contents.push(Content {
            role: "tool".to_string(),
            parts: vec![function_response_contents],
        });

        let final_response = self
            .client
            .models()
            .by_name(&self.model)
            .generate_content(&contents)
            .await?;

        println!("Final response from Gemini: {:?}", final_response);

        Ok(final_response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .and_then(|p| p.text.as_ref())
            .cloned()
            .ok_or_else(|| {
                LlmError::GeminiError(api_gemini::error::Error::Unknown(
                    "No response from Gemini".to_owned(),
                ))
            })?)
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

        let tool_result = self.dispatch(args).await?;

        let followup_prompt = followup_builder(&name, &tool_result, &context);

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
                function_declarations: Some(vec![FunctionDeclaration {
                    name: tool_info.0,
                    description: tool_info.1,
                    parameters: Some(serde_json::json!({
                        "type": schema.get("type"),
                        "properties": schema.get("properties"),
                        "required": schema.get("required"),
                    })),
                }]),
                code_execution: None,
                google_search_retrieval: None,
                code_execution_tool: None,
            }]),
            ..Default::default()
        };

        self.cached_context
            .insert(discord_user_id.to_owned(), context);

        self.conversation_continue(discord_user_id, "Hi").await
    }
}

#[async_trait]
impl LLM for Gemini {
    async fn conversation_continue(
        &mut self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let mut cache = match self.cached_context.get(discord_user_id) {
            Some(c) => c.clone(),
            None => todo!(),
        };

        cache.contents.push(Content {
            parts: vec![Part {
                text: Some(discord_channel_message.to_owned()),
                ..Default::default()
            }],
            role: "user".to_owned(),
        });

        let mut response = self
            .client
            .models()
            .by_name(&self.model)
            .generate_content(&cache)
            .await?;

        // Tool-calling loop
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
                // Dispatch tool
                let res = self
                    .tool_service
                    .dispatch(serde_json::to_value(tool_call)?)
                    .await?;

                // Push assistant's response to context
                if let Some(candidate) = response.candidates.first() {
                    cache.contents.push(candidate.content.clone());
                }

                // Push tool response to context
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
                response = self
                    .client
                    .models()
                    .by_name(&self.model)
                    .generate_content(&cache)
                    .await?;

                // Store this response in context
                cache.contents.push(response.candidates[0].content.clone());
            } else {
                break;
            }
        }

        self.cached_context
            .insert(discord_user_id.to_owned(), cache.clone());

        self.extract_final_text(&cache.contents)
    }

    async fn add_character_spells(
        &self,
        content: &str,
        discord_user_id: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<SpellsWithDiscordId>(
            "add_character_spells",
            "你是一个龙与地下城2024版本的DM助手，\
            你需要根据用户提供的资料录入该角色的法术相关信息，\
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些法术信息",
            &format!("发起者ID：{}；内容：{}", discord_user_id, content),
        )
        .await
    }

    async fn add_character_abilities(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<AbilitiesWithDiscordId>(
            "add_character_abilities",
            include_str!("./../../prompts/abilities.txt"),
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn add_character_skills(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<SkillsWithDiscordId>(
            "add_character_skills",
            include_str!("./../../prompts/skills.txt"),
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn add_character_traits(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<TraitsWithDiscordId>(
            "add_character_traits",
            include_str!("./../../prompts/traits.txt"),
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn add_character_notes(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<NotesWithDiscordId>(
            "add_character_notes",
            include_str!("./../../prompts/notes.txt"),
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
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
                            你需要根据用户提供的资料录入该角色的元数据相关信息，\
                            你将使用输入给你的discordId来使用工具，\
                            录入成功后请总结更新了角色的哪些元数据信息",
        )
        .await
    }

    async fn add_character_identity(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<IdentityWithDiscordId>(
            "add_character_identity",
            "你是一个龙与地下城2024版本的DM助手，\
            你需要根据用户提供的资料录入该角色的身份相关信息，\
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些身份信息",
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn add_character_progression(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<ProgressionWithDiscordId>(
            "add_character_progression",
            "你是一个龙与地下城2024版本的DM助手，\
            你需要根据用户提供的资料录入该角色的进度相关信息，\
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些进度信息",
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn add_character_combat(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<CombatWithDiscordId>(
            "add_character_combat",
            "你是一个龙与地下城2024版本的DM助手，\
            你需要根据用户提供的资料录入该角色的战斗相关信息，\
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些战斗信息",
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn add_character_inventory(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.call_with_tool::<InventoryWithDiscordId>(
            "add_character_inventory",
            "你是一个龙与地下城2024版本的DM助手，\
            你需要根据用户提供的资料录入该角色的物品栏相关信息，\
            你将使用输入给你的discordId来使用工具，\
            录入成功后请总结更新了角色的哪些物品栏信息",
            &format!(
                "发起者ID：{}；内容：{}",
                discord_user_id, discord_channel_message
            ),
        )
        .await
    }

    async fn dispatch(&self, tool_call: serde_json::Value) -> Result<String, LlmError> {
        println!("Dispatching tool call: {}", tool_call);
        let tool: ToolCall = serde_json::from_value(tool_call.clone())?;
        let res = match tool {
            ToolCall::AddCharacterMeta(meta) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_meta(&meta)
                    .await?;
                format!(
                    "Successfully updated character with new meta information: {}",
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterIdentity(identity_with_discord_id) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_identity(
                        &identity_with_discord_id.identity,
                        &identity_with_discord_id.discord_id,
                    )
                    .await?;
                format!(
                    "Successfully updated character with new identity information: {}",
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterProgression(progression_with_discord_id) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_progression(
                        &progression_with_discord_id.progression,
                        &progression_with_discord_id.discord_id,
                    )
                    .await?;
                format!(
                    "Successfully updated character with new progression information: {}",
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterCombat(combat_with_discord_id) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_combat(
                        &combat_with_discord_id.combat,
                        &combat_with_discord_id.discord_id,
                    )
                    .await?;
                format!(
                    "Successfully updated character with new combat information: {}",
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterSpells(add_character_spells_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_spells(
                        &add_character_spells_request.discord_id,
                        &add_character_spells_request.spells,
                    )
                    .await?;
                format!(
                    "Successfully updated character with new spells: {}",
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::UpsertCharacter(character) => {
                let sheet = self
                    .character_sheet_service
                    .upsert_character(character)
                    .await?;
                format!(
                    "Successfully updated character with new character sheet: {}",
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::GetCharacter(get_character_request) => {
                let sheet = self
                    .character_sheet_service
                    .get_character(&get_character_request.discord_id)
                    .await?;
                format!(
                    "Gotten character sheet with discord id {}: {}",
                    get_character_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::GetCharacterByName(get_character_by_name_request) => {
                let sheet = self
                    .character_sheet_service
                    .get_character_by_name(&get_character_by_name_request.character_name)
                    .await?;
                format!(
                    "Gotten character sheet for {}: {}",
                    get_character_by_name_request.character_name,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddItem(add_item_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_item(&add_item_request.discord_id, add_item_request.item)
                    .await?;
                format!(
                    "Successfully added item to character {}: {}",
                    add_item_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::RemoveItem(remove_item_request) => {
                let sheet = self
                    .character_sheet_service
                    .remove_item(
                        &remove_item_request.discord_id,
                        &remove_item_request.item_name,
                    )
                    .await?;
                format!(
                    "Successfully removed item from character {}: {}",
                    remove_item_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddSpell(add_spell_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_spell(&add_spell_request.discord_id, add_spell_request.spell)
                    .await?;
                format!(
                    "Successfully added spell to character {}: {}",
                    add_spell_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::UpdateSpellSlots(update_spell_slots_request) => {
                let sheet = self
                    .character_sheet_service
                    .update_spell_slots(
                        &update_spell_slots_request.discord_id,
                        update_spell_slots_request.level,
                        update_spell_slots_request.slot,
                        update_spell_slots_request.used,
                    )
                    .await?;
                format!(
                    "Successfully updated spell slots for level {} on character {}: {}",
                    update_spell_slots_request.level,
                    update_spell_slots_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::UpdateCurrentHp(update_current_hp_request) => {
                let sheet = self
                    .character_sheet_service
                    .update_current_hp(
                        &update_current_hp_request.discord_id,
                        update_current_hp_request.current_hp,
                    )
                    .await?;
                format!(
                    "Successfully updated current HP to {} for character {}: {}",
                    update_current_hp_request.current_hp,
                    update_current_hp_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::UpdateMaxHp(update_max_hp_request) => {
                let sheet = self
                    .character_sheet_service
                    .update_max_hp(
                        &update_max_hp_request.discord_id,
                        update_max_hp_request.max_hp,
                    )
                    .await?;
                format!(
                    "Successfully updated max HP to {} for character {}: {}",
                    update_max_hp_request.max_hp,
                    update_max_hp_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::UpdateCharacterLevel(update_character_level_request) => {
                let sheet = self
                    .character_sheet_service
                    .update_character_level(
                        &update_character_level_request.discord_id,
                        update_character_level_request.level,
                    )
                    .await?;
                format!(
                    "Successfully updated character level to {} for character {}: {}",
                    update_character_level_request.level,
                    update_character_level_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterInventory(add_character_inventory_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_inventory(
                        &add_character_inventory_request.discord_id,
                        add_character_inventory_request.inventory,
                    )
                    .await?;
                format!(
                    "Successfully updated character inventory for character {}: {}",
                    add_character_inventory_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterAbilities(add_character_abilities_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_abilities(
                        &add_character_abilities_request.discord_id,
                        add_character_abilities_request.abilities,
                    )
                    .await?;
                format!(
                    "Successfully updated character abilities for character {}: {}",
                    add_character_abilities_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterSkills(add_character_skills_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_skills(
                        &add_character_skills_request.discord_id,
                        add_character_skills_request.skills,
                    )
                    .await?;
                format!(
                    "Successfully updated character skills for character {}: {}",
                    add_character_skills_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterTraits(add_character_traits_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_traits(
                        &add_character_traits_request.discord_id,
                        add_character_traits_request.traits,
                    )
                    .await?;
                format!(
                    "Successfully updated character traits for character {}: {}",
                    add_character_traits_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
            ToolCall::AddCharacterNotes(add_character_notes_request) => {
                let sheet = self
                    .character_sheet_service
                    .add_character_notes(
                        &add_character_notes_request.discord_id,
                        add_character_notes_request.notes,
                    )
                    .await?;
                format!(
                    "Successfully updated character notes for character {}: {}",
                    add_character_notes_request.discord_id,
                    serde_json::to_string(&sheet)?
                )
            }
        };

        Ok(res)
    }
}
