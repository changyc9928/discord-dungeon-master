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
pub struct Inventory {
    pub items: Vec<Item>,
}

// Tell SQLx that Inventory can be decoded from JSONB
impl Type<Postgres> for Inventory {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Inventory
impl<'r> Decode<'r, Postgres> for Inventory {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Inventory = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Inventory into JSONB column
impl Encode<'_, Postgres> for Inventory {
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
pub struct Item {
    pub name: String,
    pub weight: i64,
    pub quantity: Option<i64>,
    pub cost_gp: i64,
    pub equiped: bool,
}
