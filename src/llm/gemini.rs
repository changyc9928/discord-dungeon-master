use std::sync::Arc;

use api_gemini::{Content, GenerateContentRequest, Part, client::Client};
use async_trait::async_trait;

use crate::{
    character::service::CharacterSheetService,
    llm::{LLM, error::LlmError, types::ToolCall},
};

const MAIN_PROMPT: &str = include_str!("./../../prompts/main.txt");
const MAIN_FOLLOWUP_PROMPT: &str = include_str!("./../../prompts/main_followup.txt");
const META_PROMPT: &str = include_str!("./../../prompts/meta.txt");
const IDENTITY_PROMPT: &str = include_str!("./../../prompts/identity.txt");
const PROGRESSION_PROMPT: &str = include_str!("./../../prompts/progression.txt");
const COMBAT_PROMPT: &str = include_str!("./../../prompts/combat.txt");
const INVENTORY_PROMPT: &str = include_str!("./../../prompts/inventory.txt");

pub struct Gemini {
    character_sheet_service: Arc<CharacterSheetService>,
    client: Client,
    model: String,
    dm_discord_id: String,
}

impl Gemini {
    pub fn new(
        model: String,
        character_sheet_service: Arc<CharacterSheetService>,
        dm_discord_id: String,
    ) -> Result<Self, LlmError> {
        // Create client from GEMINI_API_KEY environment variable
        let client = Client::new()?;
        Ok(Self {
            client,
            character_sheet_service,
            model,
            dm_discord_id,
        })
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

        let args = tool_call
            .get("arguments")
            .cloned()
            .ok_or_else(|| LlmError::InvalidResponse("Missing arguments".into()))?;

        let context = tool_call
            .get("context")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        Ok(Some((name.to_string(), tool_call.clone(), context)))
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

    fn system_content(&self, text: impl Into<String>) -> Content {
        Content {
            role: "user".to_string(),
            parts: vec![Part {
                text: Some(text.into()),
                ..Default::default()
            }],
        }
    }

    fn user_json_content<T: serde::Serialize>(&self, value: &T) -> Content {
        Content {
            role: "user".to_string(),
            parts: vec![Part {
                text: Some(serde_json::to_string(value).unwrap()),
                ..Default::default()
            }],
        }
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

    async fn run_prompt(
        &self,
        system_prompt: &str,
        discord_user_id: &str,
        discord_channel_message: &str,
        followup: impl Fn(&str, &str, &serde_json::Value) -> String,
    ) -> Result<String, LlmError> {
        let contents = vec![
            self.system_content(system_prompt),
            self.user_json_content(&serde_json::json!({
                "discord_id": discord_user_id,
                "message": discord_channel_message
            })),
        ];

        self.run_with_tools(contents, followup).await
    }

    fn default_followup(&self, name: &str, result: &str, _: &serde_json::Value) -> String {
        format!(
            r#"
            你是一个DND助手...

            工具名称：{name}
            工具结果：
            {result}

            请总结状态...
            "#
        )
    }
}

#[async_trait]
impl LLM for Gemini {
    async fn request_to_llm(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let contents = vec![
            Content {
                role: "system".to_string(),
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

    async fn add_character_meta(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.run_prompt(
            META_PROMPT,
            discord_user_id,
            discord_channel_message,
            |name, result, context| self.default_followup(name, result, context),
        )
        .await
    }

    async fn add_character_identity(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.run_prompt(
            IDENTITY_PROMPT,
            discord_user_id,
            discord_channel_message,
            |name, result, context| self.default_followup(name, result, context),
        )
        .await
    }

    async fn add_character_progression(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.run_prompt(
            PROGRESSION_PROMPT,
            discord_user_id,
            discord_channel_message,
            |name, result, context| self.default_followup(name, result, context),
        )
        .await
    }

    async fn add_character_combat(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.run_prompt(
            COMBAT_PROMPT,
            discord_user_id,
            discord_channel_message,
            |name, result, context| self.default_followup(name, result, context),
        )
        .await
    }

    async fn add_character_inventory(
        &self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        self.run_prompt(
            INVENTORY_PROMPT,
            discord_user_id,
            discord_channel_message,
            |name, result, context| self.default_followup(name, result, context),
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
        };

        Ok(res)
    }
}
