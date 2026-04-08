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

use crate::character::entities::Ability;

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Magic {
    pub spells: Spells,
}

// Tell SQLx that Magic can be decoded from JSONB
impl Type<Postgres> for Magic {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Magic
impl<'r> Decode<'r, Postgres> for Magic {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Magic = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Magic into JSONB column
impl Encode<'_, Postgres> for Magic {
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
pub struct Spells {
    pub spells: Vec<Spell>,
    pub spell_slots: Vec<SpellSlot>,
    pub ability_type: Ability,
    pub ability_modifier: i64,
    pub spell_attack: i64,
    pub save_dc: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct Spell {
    pub name: String,
    pub level: i64,
    pub cast_time: String,
    pub range: String,
    pub hit_dc: Option<i64>,
    pub effect: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct SpellSlot {
    pub level: i64,
    pub slot: i64,
    pub used: i64,
}
