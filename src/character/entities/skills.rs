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

use crate::character::entities::{Ability, abilities_block::AbilitiesBlock};

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct Skills {
    pub acrobatics: SkillStatus,
    pub animal_handling: SkillStatus,
    pub arcana: SkillStatus,
    pub athletics: SkillStatus,
    pub deception: SkillStatus,
    pub history: SkillStatus,
    pub insight: SkillStatus,
    pub intimidation: SkillStatus,
    pub investigation: SkillStatus,
    pub medicine: SkillStatus,
    pub nature: SkillStatus,
    pub perception: SkillStatus,
    pub performance: SkillStatus,
    pub persuasion: SkillStatus,
    pub religion: SkillStatus,
    pub sleight_of_hand: SkillStatus,
    pub stealth: SkillStatus,
    pub survival: SkillStatus,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Skills {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
impl<'r> Decode<'r, Postgres> for Skills {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Skills = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Meta into JSONB column
impl Encode<'_, Postgres> for Skills {
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

impl Skills {
    pub fn update_skills(&mut self, abilities: &AbilitiesBlock, proficiency_bonus: i64) {
        self.acrobatics
            .update_skill_status(abilities, proficiency_bonus);
        self.animal_handling
            .update_skill_status(abilities, proficiency_bonus);
        self.arcana
            .update_skill_status(abilities, proficiency_bonus);
        self.athletics
            .update_skill_status(abilities, proficiency_bonus);
        self.deception
            .update_skill_status(abilities, proficiency_bonus);
        self.history
            .update_skill_status(abilities, proficiency_bonus);
        self.insight
            .update_skill_status(abilities, proficiency_bonus);
        self.intimidation
            .update_skill_status(abilities, proficiency_bonus);
        self.medicine
            .update_skill_status(abilities, proficiency_bonus);
        self.nature
            .update_skill_status(abilities, proficiency_bonus);
        self.perception
            .update_skill_status(abilities, proficiency_bonus);
        self.performance
            .update_skill_status(abilities, proficiency_bonus);
        self.persuasion
            .update_skill_status(abilities, proficiency_bonus);
        self.religion
            .update_skill_status(abilities, proficiency_bonus);
        self.sleight_of_hand
            .update_skill_status(abilities, proficiency_bonus);
        self.stealth
            .update_skill_status(abilities, proficiency_bonus);
        self.survival
            .update_skill_status(abilities, proficiency_bonus);
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
#[schemars(inline)]
pub struct SkillStatus {
    pub prof: bool,
    pub bonus: i64,
    pub modifier: Ability,
    pub passive: i64,
}

impl SkillStatus {
    pub fn update_skill_status(&mut self, abilities: &AbilitiesBlock, proficiency_bonus: i64) {
        self.bonus = match self.modifier {
            Ability::Strength => abilities.strength.modifier,
            Ability::Intelligence => abilities.intelligence.modifier,
            Ability::Dexterity => abilities.dexterity.modifier,
            Ability::Wisdom => abilities.wisdom.modifier,
            Ability::Constitution => abilities.constitution.modifier,
            Ability::Charisma => abilities.charisma.modifier,
        };

        if self.prof {
            self.bonus += proficiency_bonus as i64;
        }
        self.passive = self.bonus + 10;
    }
}
