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
pub struct Progression {
    pub level: i64,
    pub xp: i64,
    pub total_hit_dice: String,
    pub max_hp: i64,
    pub proficiencies: ProficianciesTrainings,
}

// Tell SQLx that Progression can be decoded from JSONB
impl Type<Postgres> for Progression {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Progression
impl<'r> Decode<'r, Postgres> for Progression {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Progression = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Progression into JSONB column
impl Encode<'_, Postgres> for Progression {
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

impl Progression {
    pub fn update_progression(&mut self) {
        let xp_table: [i64; 20] = [
            0,      // Level 1
            300,    // Level 2
            900,    // Level 3
            2700,   // Level 4
            6500,   // Level 5
            14000,  // Level 6
            23000,  // Level 7
            34000,  // Level 8
            48000,  // Level 9
            64000,  // Level 10
            85000,  // Level 11
            100000, // Level 12
            120000, // Level 13
            140000, // Level 14
            165000, // Level 15
            195000, // Level 16
            225000, // Level 17
            265000, // Level 18
            305000, // Level 19
            355000, // Level 20
        ];
        for (lvl, &xp_req) in xp_table.iter().enumerate().rev() {
            if self.xp >= xp_req {
                self.level = (lvl + 1) as i64;
                break;
            }
        }

        self.proficiencies.proficiency_bonus = 2 + ((self.level - 1) / 4);
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct ProficianciesTrainings {
    pub proficiency_bonus: i64,
    pub armor: Vec<String>,
    pub weapons: Vec<String>,
    pub tools: Vec<String>,
    pub languages: Vec<String>,
}
