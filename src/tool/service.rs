use std::sync::Arc;

use crate::{
    character::service::CharacterSheetService,
    tool::{error::ToolError, types::ToolCall},
};

pub struct ToolService {
    pub character_sheet_service: Arc<CharacterSheetService>,
}

impl ToolService {
    pub fn new(character_sheet_service: Arc<CharacterSheetService>) -> Self {
        Self {
            character_sheet_service,
        }
    }

    pub async fn dispatch(&self, tool_call: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let tool: ToolCall = serde_json::from_value(tool_call.clone())?;
        Ok(match tool {
            ToolCall::AddCharacterMeta(meta) => serde_json::to_value(
                self.character_sheet_service
                    .add_character_meta(&meta)
                    .await?,
            )?,
            ToolCall::AddCharacterIdentity(identity_with_discord_id) => serde_json::to_value(
                self.character_sheet_service
                    .add_character_identity(
                        &identity_with_discord_id.identity,
                        &identity_with_discord_id.discord_id,
                    )
                    .await?,
            )?,
            ToolCall::AddCharacterProgression(progression_with_discord_id) => serde_json::to_value(
                self.character_sheet_service
                    .add_character_progression(
                        &progression_with_discord_id.progression,
                        &progression_with_discord_id.discord_id,
                    )
                    .await?,
            )?,
            ToolCall::AddCharacterCombat(combat_with_discord_id) => serde_json::to_value(
                self.character_sheet_service
                    .add_character_combat(
                        &combat_with_discord_id.combat,
                        &combat_with_discord_id.discord_id,
                    )
                    .await?,
            )?,
            ToolCall::AddCharacterInventory(add_character_inventory_request) => {
                serde_json::to_value(
                    self.character_sheet_service
                        .add_character_inventory(
                            &add_character_inventory_request.discord_id,
                            add_character_inventory_request.inventory,
                        )
                        .await?,
                )
            }?,
            ToolCall::AddCharacterSpells(add_character_spells_request) => serde_json::to_value(
                self.character_sheet_service
                    .add_character_spells(
                        &add_character_spells_request.discord_id,
                        &add_character_spells_request.spells,
                    )
                    .await?,
            )?,
            ToolCall::UpsertCharacter(character) => serde_json::to_value(
                self.character_sheet_service
                    .upsert_character(character)
                    .await?,
            )?,
            ToolCall::GetCharacter(get_character_request) => serde_json::to_value(
                self.character_sheet_service
                    .get_character(&get_character_request.discord_id)
                    .await?,
            )?,
            ToolCall::GetCharacterByName(get_character_by_name_request) => serde_json::to_value(
                self.character_sheet_service
                    .get_character_by_name(&get_character_by_name_request.character_name)
                    .await?,
            )?,
            ToolCall::AddItem(add_item_request) => serde_json::to_value(
                self.character_sheet_service
                    .add_item(&add_item_request.discord_id, add_item_request.item)
                    .await?,
            )?,
            ToolCall::RemoveItem(remove_item_request) => serde_json::to_value(
                self.character_sheet_service
                    .remove_item(
                        &remove_item_request.discord_id,
                        &remove_item_request.item_name,
                    )
                    .await?,
            )?,
            ToolCall::AddSpell(add_spell_request) => serde_json::to_value(
                self.character_sheet_service
                    .add_spell(&add_spell_request.discord_id, add_spell_request.spell)
                    .await?,
            )?,
            ToolCall::UpdateSpellSlots(update_spell_slots_request) => serde_json::to_value(
                self.character_sheet_service
                    .update_spell_slots(
                        &update_spell_slots_request.discord_id,
                        update_spell_slots_request.level,
                        update_spell_slots_request.slot,
                        update_spell_slots_request.used,
                    )
                    .await?,
            )?,
            ToolCall::UpdateCurrentHp(update_current_hp_request) => serde_json::to_value(
                self.character_sheet_service
                    .update_current_hp(
                        &update_current_hp_request.discord_id,
                        update_current_hp_request.current_hp,
                    )
                    .await?,
            )?,
            ToolCall::UpdateMaxHp(update_max_hp_request) => serde_json::to_value(
                self.character_sheet_service
                    .update_max_hp(
                        &update_max_hp_request.discord_id,
                        update_max_hp_request.max_hp,
                    )
                    .await?,
            )?,
            ToolCall::UpdateCharacterLevel(update_character_level_request) => serde_json::to_value(
                self.character_sheet_service
                    .update_character_level(
                        &update_character_level_request.discord_id,
                        update_character_level_request.level,
                    )
                    .await?,
            )?,
            ToolCall::AddCharacterAbilities(add_character_abilities_request) => {
                serde_json::to_value(
                    self.character_sheet_service
                        .add_character_abilities(
                            &add_character_abilities_request.discord_id,
                            add_character_abilities_request.abilities,
                        )
                        .await?,
                )
            }?,
            ToolCall::AddCharacterSkills(add_character_skills_request) => {
                serde_json::to_value(
                    self.character_sheet_service
                        .add_character_skills(
                            &add_character_skills_request.discord_id,
                            add_character_skills_request.skills,
                        )
                        .await?,
                )
            }?,
            ToolCall::AddCharacterTraits(add_character_traits_request) => {
                serde_json::to_value(
                    self.character_sheet_service
                        .add_character_traits(
                            &add_character_traits_request.discord_id,
                            add_character_traits_request.traits,
                        )
                        .await?,
                )
            }?,
            ToolCall::AddCharacterNotes(add_character_notes_request) => {
                serde_json::to_value(
                    self.character_sheet_service
                        .add_character_notes(
                            &add_character_notes_request.discord_id,
                            add_character_notes_request.notes,
                        )
                        .await?,
                )
            }?,
        })
    }
}
