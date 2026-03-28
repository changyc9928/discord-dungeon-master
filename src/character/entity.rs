use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, ser::StdError};
use sqlx::{
    Decode, Encode, Postgres,
    encode::IsNull,
    postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef},
    prelude::{FromRow, Type},
};
use strum::Display;

use crate::character::error::CharacterSheetError;

#[derive(Debug, Deserialize, Serialize, Clone, Copy, Display, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Debug, FromRow, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSheet {
    pub meta: Meta,
    pub identity: Identity,
    pub progression: Progression,
    pub combat: Combat,
    pub abilities_block: AbilitiesBlock,
    pub skills_block: SkillsBlock,
    pub magic: Magic,
    pub inventory: Inventory,
    pub traits: Traits,
    pub notes: Notes,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub discord_id: String,
    pub location: String,
    pub story_summary: Option<String>,
    pub extra_creatures: Vec<String>,
    pub dead: bool,
    pub performing_action: Option<String>,
    pub action_end_time: Option<String>,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Meta {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
impl<'r> Decode<'r, Postgres> for Meta {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Meta = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Meta into JSONB column
impl Encode<'_, Postgres> for Meta {
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub character_name: String,
    pub species: String,
    pub sub_species: Option<String>,
    pub class: String,
    pub sub_class: Option<String>,
    pub characteristics: Characteristics,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Identity {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
impl<'r> Decode<'r, Postgres> for Identity {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Identity = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Meta into JSONB column
impl Encode<'_, Postgres> for Identity {
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progression {
    pub level: u64,
    pub xp: u64,
    pub total_hit_dice: String,
    pub max_hp: u64,
    pub proficiencies: ProficianciesTrainings,
}

impl Progression {
    pub fn update_progression(&mut self) {
        let xp_table: [u64; 20] = [
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
                self.level = (lvl + 1) as u64;
                break;
            }
        }

        self.proficiencies.proficiency_bonus = 2 + ((self.level - 1) / 4);
    }
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Progression {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
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

// Encode: convert Meta into JSONB column
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Combat {
    pub armor_class: u64,
    pub initiative: u64,
    pub hit_points: u64,
    pub speed: Vec<Speed>,
    pub senses: Vec<Sense>,
    pub defenses: Defenses,
    pub conditions: Vec<Condition>,
    pub exhaustion_level: u8,

    pub saving_throws: SavingThrows,
    pub actions: Vec<Action>,
    pub combat_actions: Vec<CombatAction>,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Combat {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
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

// Encode: convert Meta into JSONB column
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AbilitiesBlock {
    pub abilities: BTreeMap<Ability, AbilityScore>,
}

impl AbilitiesBlock {
    pub fn update_abilities(&mut self) {
        self.abilities
            .iter_mut()
            .for_each(|(_, v)| v.update_modifer());
    }
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for AbilitiesBlock {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
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

// Encode: convert Meta into JSONB column
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkillsBlock {
    pub skills: Skills,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for SkillsBlock {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
impl<'r> Decode<'r, Postgres> for SkillsBlock {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: SkillsBlock = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Meta into JSONB column
impl Encode<'_, Postgres> for SkillsBlock {
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Magic {
    pub spells: Spells,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Magic {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
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

// Encode: convert Meta into JSONB column
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub items: Vec<Item>,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Inventory {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
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

// Encode: convert Meta into JSONB column
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Traits {
    pub features_and_traits: Vec<FeatureTraits>,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Traits {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
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

// Encode: convert Meta into JSONB column
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AbilityScore {
    pub base: u64,
    pub modifier: i64, // IMPORTANT: should be signed
}

impl AbilityScore {
    pub fn update_modifer(&mut self) {
        self.modifier = (self.base as i64 - 10) / 2;
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Characteristics {
    pub background: String,
    pub background_feature: String,
    pub background_feature_description: String,
    pub alignment: String,
    pub gender: String,
    pub eyes: String,
    pub size: String,
    pub height: String,
    pub faith: String,
    pub hair: String,
    pub skin: String,
    pub age: u64,
    pub weight: String,
    pub personality_traits: String,
    pub ideals: String,
    pub bonds: String,
    pub flaws: String,
    pub appearance_trait: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    pub organizations: Option<String>,
    pub allies: Option<String>,
    pub enemies: Option<String>,
    pub backstory: String,
    pub other: Option<String>,
}

// Tell SQLx that Meta can be decoded from JSONB
impl Type<Postgres> for Notes {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("jsonb")
    }
}

// Implement Decode to convert a JSONB value into Meta
impl<'r> Decode<'r, Postgres> for Notes {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn StdError + 'static + Send + Sync>> {
        // PostgreSQL JSONB is stored as text, so we deserialize from bytes
        let bytes = value.as_bytes()?;
        if bytes.is_empty() {
            return Err("Empty JSONB column".into());
        }
        let meta: Notes = serde_json::from_slice(&bytes[1..])?; // skip version byte
        Ok(meta)
    }
}

// Encode: convert Meta into JSONB column
impl Encode<'_, Postgres> for Notes {
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

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Speed {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Sense {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Defenses {
    pub resistance: Vec<String>,
    pub immunities: Vec<String>,
    pub vulnerabilities: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub name: String,
    pub used_time: Option<u64>,
    pub max_use_time: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CombatAction {
    pub name: String,
    pub hit_dc: Option<u64>,
    pub damage: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpellSlot {
    pub level: u64,
    pub slot: u64,
    pub used: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Spell {
    pub name: String,
    pub level: u64,
    pub cast_time: String,
    pub range: String,
    pub hit_dc: Option<u64>,
    pub effect: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Spells {
    pub spells: Vec<Spell>,
    pub spell_slots: Vec<SpellSlot>,
    pub ability_type: Ability,
    pub ability_modifier: u64,
    pub spell_attack: u64,
    pub save_dc: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: String,
    pub weight: u64,
    pub quantity: Option<u64>,
    pub cost_gp: u64,
    pub equiped: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FeatureTraits {
    pub name: String,
    pub description: String,
    pub duration: Option<String>, // Duration of effect
    pub trigger: Option<String>,  // Trigger conditions (e.g., "when hit by an attack")
    pub cooldown: Option<u64>,    // Cooldown time in rounds
}

#[derive(
    Debug, PartialEq, Eq, Hash, Clone, Copy, Display, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Ability {
    Strength,
    Intelligence,
    Dexterity,
    Wisdom,
    Constitution,
    Charisma,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProficianciesTrainings {
    pub proficiency_bonus: u64,
    pub armor: Vec<String>,
    pub weapons: Vec<String>,
    pub tools: Vec<String>,
    pub languages: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SavingThrows {
    pub proficiency: Vec<Ability>,
    pub saving_sets: BTreeMap<Ability, i64>,
}

impl SavingThrows {
    pub fn update_savings(&mut self, abilities: &AbilitiesBlock, proficiency_bonus: u64) {
        let mut saving_sets = BTreeMap::new();
        for (key, value) in &abilities.abilities {
            saving_sets.entry(*key).or_insert(value.modifier);
        }
        for key in &self.proficiency {
            saving_sets
                .entry(*key)
                .and_modify(|v| *v += proficiency_bonus as i64);
        }
        self.saving_sets = saving_sets;
    }
}

#[derive(
    Debug, Deserialize, Serialize, PartialEq, Eq, Hash, Clone, Copy, Display, PartialOrd, Ord,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Skill {
    Acrobatics,
    AnimalHandling,
    Arcana,
    Athletics,
    Deception,
    History,
    Insight,
    Intimidation,
    Investigation,
    Medicine,
    Nature,
    Perception,
    Performance,
    Persuasion,
    Religion,
    SleightOfHand,
    Stealth,
    Survival,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Skills {
    pub skills: BTreeMap<Skill, SkillStatus>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatus {
    pub prof: bool,
    pub bonus: i64,
    pub modifier: Ability,
    pub passive: i64,
}

impl SkillStatus {
    pub fn update_skill_status(
        &mut self,
        abilities: &AbilitiesBlock,
        proficiency_bonus: u64,
    ) -> Result<(), CharacterSheetError> {
        self.bonus = abilities
            .abilities
            .get(&self.modifier)
            .ok_or_else(|| {
                CharacterSheetError::MissingAbilityBonus(
                    self.modifier,
                    "updating skills".to_owned(),
                )
            })?
            .modifier;

        if self.prof {
            self.bonus += proficiency_bonus as i64;
        }
        self.passive = self.bonus + 10;
        Ok(())
    }
}

impl Skills {
    pub fn update_skills(
        &mut self,
        abilities: &AbilitiesBlock,
        proficiency_bonus: u64,
    ) -> Result<(), CharacterSheetError> {
        self.skills
            .iter_mut()
            .map(|(_, v)| v.update_skill_status(abilities, proficiency_bonus))
            .collect::<Result<(), CharacterSheetError>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_abilities() -> AbilitiesBlock {
        let mut abilities = BTreeMap::new();
        abilities.insert(
            Ability::Strength,
            AbilityScore {
                base: 18,
                modifier: 0,
            },
        );
        abilities.insert(
            Ability::Dexterity,
            AbilityScore {
                base: 14,
                modifier: 0,
            },
        );
        abilities.insert(
            Ability::Constitution,
            AbilityScore {
                base: 16,
                modifier: 0,
            },
        );
        abilities.insert(
            Ability::Intelligence,
            AbilityScore {
                base: 12,
                modifier: 0,
            },
        );
        abilities.insert(
            Ability::Wisdom,
            AbilityScore {
                base: 10,
                modifier: 0,
            },
        );
        abilities.insert(
            Ability::Charisma,
            AbilityScore {
                base: 8,
                modifier: 0,
            },
        );
        let mut ab = AbilitiesBlock { abilities };
        ab.update_abilities();
        ab
    }

    #[test]
    fn test_ability_modifiers_update() {
        let abilities = sample_abilities();

        assert_eq!(abilities.abilities[&Ability::Strength].modifier, 4);
        assert_eq!(abilities.abilities[&Ability::Dexterity].modifier, 2);
        assert_eq!(abilities.abilities[&Ability::Constitution].modifier, 3);
        assert_eq!(abilities.abilities[&Ability::Intelligence].modifier, 1);
        assert_eq!(abilities.abilities[&Ability::Wisdom].modifier, 0);
        assert_eq!(abilities.abilities[&Ability::Charisma].modifier, -1);
    }

    #[test]
    fn test_progression_level_and_proficiency() {
        let mut prog = Progression {
            level: 1,
            xp: 0,
            total_hit_dice: "d8".to_string(),
            max_hp: 8,
            proficiencies: ProficianciesTrainings {
                proficiency_bonus: 0,
                armor: vec![],
                weapons: vec![],
                tools: vec![],
                languages: vec![],
            },
        };

        // Test level 1
        prog.update_progression();
        assert_eq!(prog.level, 1);
        assert_eq!(prog.proficiencies.proficiency_bonus, 2);

        // XP for level 3
        prog.xp = 900;
        prog.update_progression();
        assert_eq!(prog.level, 3);
        assert_eq!(prog.proficiencies.proficiency_bonus, 2);

        // XP for level 5
        prog.xp = 14000;
        prog.update_progression();
        assert_eq!(prog.level, 6); // Table ends at index 5, level = index+1
        assert_eq!(prog.proficiencies.proficiency_bonus, 3);
    }

    #[test]
    fn test_saving_throws_update() {
        let abilities = sample_abilities();
        let mut saving = SavingThrows {
            proficiency: vec![Ability::Strength, Ability::Dexterity],
            saving_sets: BTreeMap::new(),
        };

        saving.update_savings(&abilities, 2);

        assert_eq!(saving.saving_sets[&Ability::Strength], 4 + 2);
        assert_eq!(saving.saving_sets[&Ability::Dexterity], 2 + 2);
        // Unlisted abilities should still include modifier only
        assert_eq!(saving.saving_sets[&Ability::Constitution], 3);
    }

    #[test]
    fn test_skill_status_update() {
        let abilities = sample_abilities();
        let proficiency_bonus = 2;

        let mut skills_map = BTreeMap::new();
        skills_map.insert(
            Skill::Athletics,
            SkillStatus {
                prof: true,
                bonus: 0,
                modifier: Ability::Strength,
                passive: 0,
            },
        );
        skills_map.insert(
            Skill::Arcana,
            SkillStatus {
                prof: false,
                bonus: 0,
                modifier: Ability::Intelligence,
                passive: 0,
            },
        );

        let mut skills_block = Skills { skills: skills_map };
        skills_block
            .update_skills(&abilities, proficiency_bonus)
            .unwrap();

        let athletics = &skills_block.skills[&Skill::Athletics];
        assert_eq!(athletics.bonus, 4 + 2); // Str mod + proficiency
        assert_eq!(athletics.passive, 16);

        let arcana = &skills_block.skills[&Skill::Arcana];
        assert_eq!(arcana.bonus, 1); // Int mod only
        assert_eq!(arcana.passive, 11);
    }

    #[test]
    fn test_skill_update_missing_ability() {
        let abilities = AbilitiesBlock {
            abilities: BTreeMap::new(),
        };
        let mut skill = SkillStatus {
            prof: true,
            bonus: 0,
            modifier: Ability::Strength,
            passive: 0,
        };
        let res = skill.update_skill_status(&abilities, 2);
        assert!(res.is_err());
    }

    #[test]
    fn test_edge_cases_negative_modifiers() {
        let mut abilities = BTreeMap::new();
        abilities.insert(
            Ability::Charisma,
            AbilityScore {
                base: 1,
                modifier: 0,
            },
        ); // -4 mod
        let mut block = AbilitiesBlock { abilities };
        block.update_abilities();
        assert_eq!(block.abilities[&Ability::Charisma].modifier, -9 / 2); // -5/2 = -2
    }
}
