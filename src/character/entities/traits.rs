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
pub struct Traits {
    pub features_and_traits: Vec<FeatureTraits>,
}

// Tell SQLx that Traits can be decoded from JSONB
impl Type<Postgres> for Traits {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Traits
impl<'r> Decode<'r, Postgres> for Traits {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Traits = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Traits into JSONB column
impl Encode<'_, Postgres> for Traits {
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
pub struct FeatureTraits {
    pub name: String,
    pub description: String,
    pub duration: Option<String>, // Duration of effect
    pub trigger: Option<String>,  // Trigger conditions (e.g., "when hit by an attack")
    pub cooldown: Option<i64>,    // Cooldown time in rounds
}
