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

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct AbilitiesBlock {
    pub strength: AbilityScore,
    pub intelligence: AbilityScore,
    pub dexterity: AbilityScore,
    pub constitution: AbilityScore,
    pub charisma: AbilityScore,
    pub wisdom: AbilityScore,
}

impl AbilitiesBlock {
    pub fn update_abilities(&mut self) {
        self.strength.update_modifer();
        self.intelligence.update_modifer();
        self.dexterity.update_modifer();
        self.constitution.update_modifer();
        self.charisma.update_modifer();
        self.wisdom.update_modifer();
    }
}

// Tell SQLx that AbilitiesBlock can be decoded from JSONB
impl Type<Postgres> for AbilitiesBlock {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into AbilitiesBlock
impl<'r> Decode<'r, Postgres> for AbilitiesBlock {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: AbilitiesBlock = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert AbilitiesBlock into JSONB column
impl Encode<'_, Postgres> for AbilitiesBlock {
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
pub struct AbilityScore {
    pub base: i64,
    pub modifier: i64,
}

impl AbilityScore {
    pub fn update_modifer(&mut self) {
        self.modifier = (self.base as i64 - 10) / 2;
    }
}
