use crate::character::entity::{
    CharacterSheet, CombatWithDiscordId, IdentityWithDiscordId, InventoryWithDiscordId, Item, Meta,
    ProgressionWithDiscordId, Spell,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCharacterRequest {
    pub discord_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetCharacterByNameRequest {
    pub character_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddItemRequest {
    pub discord_id: String,
    pub item: Item,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoveItemRequest {
    pub discord_id: String,
    pub item_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddSpellRequest {
    pub discord_id: String,
    pub spell: Spell,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSpellSlotsRequest {
    pub discord_id: String,
    pub level: u64,
    pub slot: u64,
    pub used: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCurrentHpRequest {
    pub discord_id: String,
    pub current_hp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMaxHpRequest {
    pub discord_id: String,
    pub max_hp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCharacterLevelRequest {
    pub discord_id: String,
    pub level: u64,
}

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

    #[serde(rename = "add_character_inventory")]
    AddCharacterInventory(InventoryWithDiscordId),
}
