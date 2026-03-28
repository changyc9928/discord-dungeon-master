use serde::{Deserialize, Serialize};
use crate::character::entity::{Item, Spell};

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
