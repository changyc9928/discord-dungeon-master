use schemars::JsonSchema;
use serde::ser::StdError;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgTypeInfo;
use sqlx::prelude::Type;
use sqlx::{
    Decode, Encode, Postgres,
    encode::IsNull,
    postgres::{PgArgumentBuffer, PgValueRef},
};
use strum::Display;

use crate::character::entity::{Ability, abilities_block::AbilitiesBlock};

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Combat {
    pub armor_class: i64,
    pub initiative: i64,
    pub hit_points: i64,
    pub speed: Vec<Speed>,
    pub senses: Vec<Sense>,
    pub defenses: Defenses,
    pub conditions: Vec<Condition>,
    pub exhaustion_level: i8,
    pub saving_throws: SavingThrows,
    pub actions: Vec<Action>,
    pub combat_actions: Vec<CombatAction>,
}

// Tell SQLx that Combat can be decoded from JSONB
impl Type<Postgres> for Combat {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Combat
impl<'r> Decode<'r, Postgres> for Combat {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Combat = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Combat into JSONB column
impl Encode<'_, Postgres> for Combat {
    fn encode_by_ref(
        &self,
        buf: &mut PgArgumentBuffer,
    ) -> Result<IsNull, Box<dyn StdError + Send + Sync + 'static>> {
        // Serialize the struct to JSON bytes
        let bytes = serde_json::to_vec(self).expect("Failed to serialize Meta");
        // Write JSONB marker (0x01) + raw bytes for Postgres JSONB
        // SQLx handles it as a simple byte array
        buf.push(1); // ✅ JSONB version byte
        buf.extend_from_slice(&bytes);
        Ok(IsNull::No)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Speed {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Sense {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Defenses {
    pub resistance: Vec<String>,
    pub immunities: Vec<String>,
    pub vulnerabilities: Vec<String>,
}

#[derive(
    Debug, Deserialize, Serialize, Clone, Copy, Display, PartialEq, Eq, PartialOrd, Ord, JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Condition {
    Blinded,
    Charmed,
    Deafened,
    Frightened,
    Grappled,
    Incapacitated,
    Invisible,
    Paralyzed,
    Petrified,
    Poisoned,
    Prone,
    Restrained,
    Stunned,
    Unconscious,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct SavingThrows {
    pub proficiency: Vec<Ability>,
    pub strength_saving_throws: i64,
    pub intelligence_saving_throws: i64,
    pub dexterity_saving_throws: i64,
    pub wisdom_saving_throws: i64,
    pub constitution_saving_throws: i64,
    pub charisma_saving_throws: i64,
}

impl SavingThrows {
    pub fn update_savings(&mut self, abilities: &AbilitiesBlock, proficiency_bonus: i64) {
        self.strength_saving_throws = abilities.strength.modifier;
        self.intelligence_saving_throws = abilities.intelligence.modifier;
        self.dexterity_saving_throws = abilities.dexterity.modifier;
        self.wisdom_saving_throws = abilities.wisdom.modifier;
        self.constitution_saving_throws = abilities.constitution.modifier;
        self.charisma_saving_throws = abilities.charisma.modifier;

        for ability in &self.proficiency {
            match ability {
                Ability::Strength => self.strength_saving_throws += proficiency_bonus,
                Ability::Intelligence => self.intelligence_saving_throws += proficiency_bonus,
                Ability::Dexterity => self.dexterity_saving_throws += proficiency_bonus,
                Ability::Wisdom => self.wisdom_saving_throws += proficiency_bonus,
                Ability::Constitution => self.constitution_saving_throws += proficiency_bonus,
                Ability::Charisma => self.charisma_saving_throws += proficiency_bonus,
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Action {
    pub name: String,
    pub used_time: Option<i64>,
    pub max_use_time: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct CombatAction {
    pub name: String,
    pub hit_dc: Option<i64>,
    pub damage: String,
}
