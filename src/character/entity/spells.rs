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
use utoipa::ToSchema;

use crate::character::entity::Ability;

// Tell SQLx that Spells can be decoded from JSONB
impl Type<Postgres> for Spells {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Spells
impl<'r> Decode<'r, Postgres> for Spells {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Spells = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Spells into JSONB column
impl Encode<'_, Postgres> for Spells {
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

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Spells {
    pub spells: Vec<Spell>,
    pub spell_slots: Vec<SpellSlot>,
    pub ability_type: Ability,
    pub ability_modifier: i64,
    pub spell_attack: i64,
    pub save_dc: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Spell {
    pub name: String,
    pub level: i64,
    pub cast_time: String,
    pub range: String,
    #[schemars(
        description = "The required value for the spell to affect a target, either as an attack hit threshold or a saving throw DC. Required for spells that use attack rolls or saving throws, such as Thunderwave."
    )]
    pub hit_dc: Option<i64>,
    pub effect: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct SpellSlot {
    pub level: i64,
    pub slot: i64,
    pub used: i64,
}
