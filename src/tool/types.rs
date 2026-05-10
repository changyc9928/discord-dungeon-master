use std::sync::Arc;

use rig::{completion::ToolDefinition, tool::Tool};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

use crate::llm::error::LlmError;
use crate::{story::service::StoryService, tool::error::ToolError as AppToolError};

use crate::character::{
    entity::{
        CharacterSheet,
        abilities_block::AbilitiesBlock,
        combat::Combat,
        identity::Identity,
        inventory::{Inventory, Item},
        meta::Meta,
        notes::Notes,
        progression::Progression,
        skills::Skills,
        spells::{Spell, Spells},
        traits::Traits,
    },
    error::CharacterSheetError,
    service::CharacterSheetService,
};

#[derive(Deserialize)]
#[serde(tag = "name", content = "arguments")]
pub enum ToolCall {
    #[serde(rename = "add_character_meta")]
    AddCharacterMeta(Meta),

    #[serde(rename = "add_character_identity")]
    AddCharacterIdentity(IdentityWithDiscordId),

    #[serde(rename = "add_character_progression")]
    AddCharacterProgression(ProgressionWithDiscordId),

    #[serde(rename = "add_character_combat")]
    AddCharacterCombat(CombatWithDiscordId),

    #[serde(rename = "add_character_spells")]
    AddCharacterSpells(SpellsWithDiscordId),

    #[serde(rename = "add_character_abilities")]
    AddCharacterAbilities(AbilitiesWithDiscordId),

    #[serde(rename = "add_character_skills")]
    AddCharacterSkills(SkillsWithDiscordId),

    #[serde(rename = "add_character_traits")]
    AddCharacterTraits(TraitsWithDiscordId),

    #[serde(rename = "add_character_notes")]
    AddCharacterNotes(NotesWithDiscordId),

    #[serde(rename = "add_character_inventory")]
    AddCharacterInventory(InventoryWithDiscordId),

    #[serde(rename = "upsert_character")]
    UpsertCharacter(CharacterSheet),

    #[serde(rename = "get_character")]
    GetCharacter(GetCharacterRequest),

    #[serde(rename = "get_character_by_name")]
    GetCharacterByName(GetCharacterByNameRequest),

    #[serde(rename = "add_item")]
    AddItem(AddItemRequest),

    #[serde(rename = "remove_item")]
    RemoveItem(RemoveItemRequest),

    #[serde(rename = "add_spell")]
    AddSpell(AddSpellRequest),

    #[serde(rename = "update_spell_slots")]
    UpdateSpellSlots(UpdateSpellSlotsRequest),

    #[serde(rename = "update_current_hp")]
    UpdateCurrentHp(UpdateCurrentHpRequest),

    #[serde(rename = "update_max_hp")]
    UpdateMaxHp(UpdateMaxHpRequest),

    #[serde(rename = "update_character_level")]
    UpdateCharacterLevel(UpdateCharacterLevelRequest),

    #[serde(rename = "insert_new_dialogue")]
    NewDialogue(NewDialogueRequest),
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct IdentityWithDiscordId {
    pub discord_id: String,
    pub identity: Identity,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct ProgressionWithDiscordId {
    pub discord_id: String,
    pub progression: Progression,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct CombatWithDiscordId {
    pub discord_id: String,
    pub combat: Combat,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct AbilitiesWithDiscordId {
    pub discord_id: String,
    pub abilities: AbilitiesBlock,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct SkillsWithDiscordId {
    pub discord_id: String,
    pub skills: Skills,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct TraitsWithDiscordId {
    pub discord_id: String,
    pub traits: Traits,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct NotesWithDiscordId {
    pub discord_id: String,
    pub notes: Notes,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct InventoryWithDiscordId {
    pub discord_id: String,
    pub inventory: Inventory,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]

pub struct SpellsWithDiscordId {
    pub discord_id: String,
    pub spells: Spells,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetCharacterRequest {
    pub discord_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct GetCharacterByNameRequest {
    pub character_name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AddItemRequest {
    pub discord_id: String,
    pub item: Item,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RemoveItemRequest {
    pub discord_id: String,
    pub item_name: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]

pub struct AddSpellRequest {
    pub discord_id: String,
    pub spell: Spell,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateSpellSlotsRequest {
    pub discord_id: String,
    pub level: i64,
    pub slot: i64,
    pub used: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCurrentHpRequest {
    pub discord_id: String,
    pub current_hp: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateMaxHpRequest {
    pub discord_id: String,
    pub max_hp: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCharacterLevelRequest {
    pub discord_id: String,
    pub level: i64,
    pub experience: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct NewDialogueRequest {
    pub discord_id: String,
}

pub trait GetToolInfo {
    fn get_tool_name() -> (String, String);
}

impl GetToolInfo for NewDialogueRequest {
    fn get_tool_name() -> (String, String) {
        (
            "insert_new_dialogue".to_owned(),
            "插入新的游戏对话".to_owned(),
        )
    }
}

impl GetToolInfo for UpdateCharacterLevelRequest {
    fn get_tool_name() -> (String, String) {
        (
            "update_character_level".to_owned(),
            "根据用户的 Discord ID 来调整当前角色的等级".to_owned(),
        )
    }
}

impl GetToolInfo for UpdateMaxHpRequest {
    fn get_tool_name() -> (String, String) {
        (
            "update_max_hp".to_owned(),
            "根据用户的 Discord ID 来调整当前角色的最大血量".to_owned(),
        )
    }
}

impl GetToolInfo for UpdateCurrentHpRequest {
    fn get_tool_name() -> (String, String) {
        (
            "update_current_hp".to_owned(),
            "根据用户的 Discord ID 来调整当前角色的当前血量".to_owned(),
        )
    }
}

impl GetToolInfo for UpdateSpellSlotsRequest {
    fn get_tool_name() -> (String, String) {
        (
            "update_spell_slots".to_owned(),
            "根据用户的 Discord ID 来新增新的法术位".to_owned(),
        )
    }
}

impl GetToolInfo for AddSpellRequest {
    fn get_tool_name() -> (String, String) {
        (
            "add_spell".to_owned(),
            "根据用户的 Discord ID 来添加新的法术".to_owned(),
        )
    }
}

impl GetToolInfo for RemoveItemRequest {
    fn get_tool_name() -> (String, String) {
        (
            "remove_item".to_owned(),
            "根据用户的 Discord ID 来删除已有物品".to_owned(),
        )
    }
}

impl GetToolInfo for AddItemRequest {
    fn get_tool_name() -> (String, String) {
        (
            "add_item".to_owned(),
            "根据用户的 Discord ID 来插入新的物品".to_owned(),
        )
    }
}

impl GetToolInfo for GetCharacterByNameRequest {
    fn get_tool_name() -> (String, String) {
        (
            "get_character_by_name".to_owned(),
            "根据对话中提到的角色名来获取完整角色卡信息".to_owned(),
        )
    }
}

impl GetToolInfo for GetCharacterRequest {
    fn get_tool_name() -> (String, String) {
        (
            "get_character".to_owned(),
            "根据用户的 Discord ID 来获取完整角色卡信息".to_owned(),
        )
    }
}

impl GetToolInfo for Meta {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_meta".to_owned(),
            "根据用户的 Discord ID 插入角色元数据".to_owned(),
        )
    }
}

impl GetToolInfo for IdentityWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_identity".to_owned(),
            "根据用户的 Discord ID 插入角色身份信息".to_owned(),
        )
    }
}

impl GetToolInfo for ProgressionWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_progression".to_owned(),
            "根据用户的 Discord ID 插入角色进阶信息".to_owned(),
        )
    }
}

impl GetToolInfo for CombatWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_combat".to_owned(),
            "根据用户的 Discord ID 插入角色战斗信息".to_owned(),
        )
    }
}

impl GetToolInfo for AbilitiesWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_abilities".to_owned(),
            "根据用户的 Discord ID 插入角色能力信息".to_owned(),
        )
    }
}

impl GetToolInfo for SkillsWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_skills".to_owned(),
            "根据用户的 Discord ID 插入角色技能信息".to_owned(),
        )
    }
}

impl GetToolInfo for TraitsWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_traits".to_owned(),
            "根据用户的 Discord ID 插入角色特性信息".to_owned(),
        )
    }
}

impl GetToolInfo for NotesWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_notes".to_owned(),
            "根据用户的 Discord ID 插入角色笔记信息".to_owned(),
        )
    }
}

impl GetToolInfo for InventoryWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_inventory".to_owned(),
            "根据用户的 Discord ID 插入角色物品栏信息".to_owned(),
        )
    }
}

impl GetToolInfo for SpellsWithDiscordId {
    fn get_tool_name() -> (String, String) {
        (
            "add_character_spells".to_owned(),
            "根据用户的 Discord ID 插入角色法术信息".to_owned(),
        )
    }
}

impl Tool for SpellToolCall {
    const NAME: &'static str = "add_character_spells";

    type Error = CharacterSheetError;

    type Args = SpellsWithDiscordId;

    type Output = Spells;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色法术信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_spells(&args.discord_id, &args.spells)
            .await
    }
}

pub struct SpellToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

pub struct MetaToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for MetaToolCall {
    const NAME: &'static str = "add_character_meta";

    type Error = CharacterSheetError;

    type Args = Meta;

    type Output = Meta;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色元数据".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service.add_character_meta(&args).await
    }
}

pub struct IdentityToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for IdentityToolCall {
    const NAME: &'static str = "add_character_identity";

    type Error = CharacterSheetError;

    type Args = IdentityWithDiscordId;

    type Output = Identity;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色身份信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_identity(&args.identity, &args.discord_id)
            .await
    }
}

pub struct ProgressionToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for ProgressionToolCall {
    const NAME: &'static str = "add_character_progression";

    type Error = CharacterSheetError;

    type Args = ProgressionWithDiscordId;

    type Output = Progression;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色进阶信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_progression(&args.progression, &args.discord_id)
            .await
    }
}

pub struct CombatToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for CombatToolCall {
    const NAME: &'static str = "add_character_combat";

    type Error = CharacterSheetError;

    type Args = CombatWithDiscordId;

    type Output = Combat;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色战斗信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_combat(&args.combat, &args.discord_id)
            .await
    }
}

pub struct InventoryToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for InventoryToolCall {
    const NAME: &'static str = "add_character_inventory";

    type Error = CharacterSheetError;

    type Args = InventoryWithDiscordId;

    type Output = Inventory;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色物品栏信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_inventory(&args.discord_id, &args.inventory)
            .await
    }
}

pub struct AbilitiesToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for AbilitiesToolCall {
    const NAME: &'static str = "add_character_abilities";

    type Error = CharacterSheetError;

    type Args = AbilitiesWithDiscordId;

    type Output = AbilitiesBlock;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色能力信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_abilities(&args.discord_id, args.abilities)
            .await
    }
}

pub struct SkillsToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for SkillsToolCall {
    const NAME: &'static str = "add_character_skills";

    type Error = CharacterSheetError;

    type Args = SkillsWithDiscordId;

    type Output = Skills;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色技能信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_skills(&args.discord_id, &args.skills)
            .await
    }
}

pub struct TraitsToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for TraitsToolCall {
    const NAME: &'static str = "add_character_traits";

    type Error = CharacterSheetError;

    type Args = TraitsWithDiscordId;

    type Output = Traits;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色特性信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_traits(&args.discord_id, &args.traits)
            .await
    }
}

pub struct NotesToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for NotesToolCall {
    const NAME: &'static str = "add_character_notes";

    type Error = CharacterSheetError;

    type Args = NotesWithDiscordId;

    type Output = Notes;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 插入角色笔记信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_character_notes(&args.discord_id, &args.notes)
            .await
    }
}

pub struct GetCharacterToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for GetCharacterToolCall {
    const NAME: &'static str = "get_character";

    type Error = CharacterSheetError;

    type Args = GetCharacterRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来获取完整角色卡信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .get_character(&args.discord_id)
            .await
    }
}

pub struct GetCharacterByNameToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for GetCharacterByNameToolCall {
    const NAME: &'static str = "get_character_by_name";

    type Error = CharacterSheetError;

    type Args = GetCharacterByNameRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据对话中提到的角色名来获取完整角色卡信息".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .get_character_by_name(&args.character_name)
            .await
    }
}

pub struct AddItemToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for AddItemToolCall {
    const NAME: &'static str = "add_item";

    type Error = CharacterSheetError;

    type Args = AddItemRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来插入新的物品".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_item(&args.discord_id, &args.item)
            .await
    }
}

pub struct RemoveItemToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for RemoveItemToolCall {
    const NAME: &'static str = "remove_item";

    type Error = CharacterSheetError;

    type Args = RemoveItemRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来删除已有物品".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .remove_item(&args.discord_id, &args.item_name)
            .await
    }
}

pub struct AddSpellToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for AddSpellToolCall {
    const NAME: &'static str = "add_spell";

    type Error = CharacterSheetError;

    type Args = AddSpellRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来添加新的法术".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .add_spell(&args.discord_id, &args.spell)
            .await
    }
}

pub struct UpdateSpellSlotsToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for UpdateSpellSlotsToolCall {
    const NAME: &'static str = "update_spell_slots";

    type Error = CharacterSheetError;

    type Args = UpdateSpellSlotsRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来新增新的法术位".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .update_spell_slots(&args.discord_id, args.level, args.slot, args.used)
            .await
    }
}

pub struct UpdateCurrentHpToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for UpdateCurrentHpToolCall {
    const NAME: &'static str = "update_current_hp";

    type Error = CharacterSheetError;

    type Args = UpdateCurrentHpRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来调整当前角色的当前血量".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .update_current_hp(&args.discord_id, args.current_hp)
            .await
    }
}

pub struct UpdateMaxHpToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl Tool for UpdateMaxHpToolCall {
    const NAME: &'static str = "update_max_hp";

    type Error = CharacterSheetError;

    type Args = UpdateMaxHpRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来调整当前角色的最大血量".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .update_max_hp(&args.discord_id, args.max_hp)
            .await
    }
}

pub struct UpdateCharacterLevelToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

pub struct NewDialogueToolCall {
    pub character_sheet_service: Arc<CharacterSheetService>,
    pub story_service: Arc<StoryService>,
    pub dialogue: String,
    pub author_name: String,
}

impl Tool for NewDialogueToolCall {
    const NAME: &'static str = "insert_new_dialogue";

    type Error = AppToolError;

    type Args = NewDialogueRequest;

    type Output = ();

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "插入新的游戏对话".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let character = self
            .character_sheet_service
            .get_character(&args.discord_id)
            .await;
        let character_name = match character {
            Ok(character) => character.identity.character_name,
            _ => format!("Unknown Adventurer - {}", args.discord_id),
        };

        self.story_service
            .insert_new_dialogue(
                &self.dialogue,
                &self.author_name,
                &character_name,
                &args.discord_id,
            )
            .await?;

        Ok(())
    }
}

impl Tool for UpdateCharacterLevelToolCall {
    const NAME: &'static str = "update_character_level";

    type Error = CharacterSheetError;

    type Args = UpdateCharacterLevelRequest;

    type Output = CharacterSheet;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "根据用户的 Discord ID 来调整当前角色的等级".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.character_sheet_service
            .update_character_level(&args.discord_id, args.level, args.experience)
            .await
    }
}

pub struct RemoveCache;

#[derive(Deserialize, Serialize, JsonSchema)]
pub struct RemoveCacheRequest {
    pub remove: bool,
}

impl Tool for RemoveCache {
    const NAME: &'static str = "remove_cache";

    type Error = LlmError;

    type Args = RemoveCacheRequest;

    type Output = ();

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "用于清除agent的上下文缓存".to_string(),
            parameters: schema_for!(Self::Args).into(),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}
