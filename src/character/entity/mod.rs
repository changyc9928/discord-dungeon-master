use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use strum::Display;
use utoipa::ToSchema;

use crate::character::entity::{
    abilities_block::AbilitiesBlock, combat::Combat, identity::Identity, inventory::Inventory,
    meta::Meta, notes::Notes, progression::Progression, skills::Skills, spells::Spells,
    traits::Traits,
};

pub mod abilities_block;
pub mod combat;
pub mod identity;
pub mod inventory;
pub mod meta;
pub mod notes;
pub mod progression;
pub mod skills;
pub mod spells;
pub mod traits;

#[derive(
    Debug,
    PartialEq,
    Eq,
    Hash,
    Clone,
    Copy,
    Display,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    JsonSchema,
    Default,
    ToSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[schemars(inline)]
pub enum Ability {
    #[default]
    Strength,
    Intelligence,
    Dexterity,
    Wisdom,
    Constitution,
    Charisma,
}

#[derive(
    Debug, FromRow, Deserialize, Serialize, Clone, Default, JsonSchema, ToSchema, PartialEq, Eq,
)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSheet {
    pub meta: Meta,
    pub identity: Identity,
    pub progression: Progression,
    pub combat: Combat,
    pub abilities_block: AbilitiesBlock,
    pub skills: Skills,
    pub magic: Spells,
    pub inventory: Inventory,
    pub traits: Traits,
    pub notes: Notes,
}
