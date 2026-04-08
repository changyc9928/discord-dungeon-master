use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use strum::Display;

use crate::character::entities::{
    abilities_block::AbilitiesBlock, combat::Combat, identity::Identity, inventory::Inventory,
    magic::Magic, meta::Meta, notes::Notes, progression::Progression, skills::Skills,
    traits::Traits,
};

pub mod abilities_block;
pub mod combat;
pub mod identity;
pub mod inventory;
pub mod magic;
pub mod meta;
pub mod notes;
pub mod progression;
pub mod skills;
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

#[derive(Debug, FromRow, Deserialize, Serialize, Clone, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSheet {
    pub meta: Meta,
    pub identity: Identity,
    pub progression: Progression,
    pub combat: Combat,
    pub abilities_block: AbilitiesBlock,
    pub skills: Skills,
    pub magic: Magic,
    pub inventory: Inventory,
    pub traits: Traits,
    pub notes: Notes,
}
