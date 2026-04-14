use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::character::entity::{
    CharacterSheet,
    abilities_block::AbilitiesBlock,
    combat::Combat,
    identity::Identity,
    inventory::{Inventory, Item},
    magic::{Magic, Spell},
    meta::Meta,
    notes::Notes,
    progression::Progression,
    skills::Skills,
    traits::Traits,
};

#[derive(Deserialize)]
#[serde(tag = "name", content = "args")]
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
    pub spells: Magic,
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
}

pub trait GetToolInfo {
    fn get_tool_name() -> (String, String);
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
            "根据用户的 Discord ID 来插入新的法术".to_owned(),
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
