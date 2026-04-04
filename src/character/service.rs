use std::sync::Arc;

use crate::character::{
    entity::{
        AbilitiesBlock, CharacterSheet, Combat, Identity, Inventory, Item, Magic, Meta,
        Notes, Progression, SkillsBlock, Spell, SpellSlot, Traits,
    },
    error::CharacterSheetError,
    repository::CharacterSheetRepository,
};

pub struct CharacterSheetService {
    pub repo: Arc<CharacterSheetRepository>,
}

impl CharacterSheetService {
    pub async fn upsert_character(
        &self,
        mut character_sheet: CharacterSheet,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        character_sheet.progression.update_progression();
        character_sheet.abilities_block.update_abilities();
        character_sheet.skills_block.skills.update_skills(
            &character_sheet.abilities_block,
            character_sheet.progression.proficiencies.proficiency_bonus,
        )?;
        character_sheet.combat.saving_throws.update_savings(
            &character_sheet.abilities_block,
            character_sheet.progression.proficiencies.proficiency_bonus,
        );
        let entity = self.repo.upsert_character(&character_sheet).await?;
        Ok(entity)
    }

    pub async fn get_character(
        &self,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut entity = self.repo.get_character_by_discord_id(&discord_id).await?;
        entity.combat.actions.sort_by_key(|f| f.name.clone());
        entity.combat.combat_actions.sort_by_key(|f| f.name.clone());
        entity.combat.conditions.sort();
        entity.combat.senses.sort_by_key(|f| f.name.clone());
        entity.combat.speed.sort_by_key(|f| f.name.clone());
        entity.inventory.items.sort_by_key(|f| f.name.clone());
        entity.magic.spells.spell_slots.sort_by_key(|f| f.level);
        entity.magic.spells.spells.sort_by_key(|f| f.name.clone());
        entity.meta.extra_creatures.sort();
        entity.progression.proficiencies.armor.sort();
        entity.progression.proficiencies.languages.sort();
        entity.progression.proficiencies.tools.sort();
        entity.progression.proficiencies.weapons.sort();
        entity
            .traits
            .features_and_traits
            .sort_by_key(|f| f.name.clone());
        Ok(entity)
    }

    pub async fn get_character_by_name(
        &self,
        character_name: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut entity = self.repo.get_character_by_name(character_name).await?;
        entity.combat.actions.sort_by_key(|f| f.name.clone());
        entity.combat.combat_actions.sort_by_key(|f| f.name.clone());
        entity.combat.conditions.sort();
        entity.combat.senses.sort_by_key(|f| f.name.clone());
        entity.combat.speed.sort_by_key(|f| f.name.clone());
        entity.inventory.items.sort_by_key(|f| f.name.clone());
        entity.magic.spells.spell_slots.sort_by_key(|f| f.level);
        entity.magic.spells.spells.sort_by_key(|f| f.name.clone());
        entity.meta.extra_creatures.sort();
        entity.progression.proficiencies.armor.sort();
        entity.progression.proficiencies.languages.sort();
        entity.progression.proficiencies.tools.sort();
        entity.progression.proficiencies.weapons.sort();
        entity
            .traits
            .features_and_traits
            .sort_by_key(|f| f.name.clone());
        Ok(entity)
    }

    pub async fn add_character_meta(
        &self,
        meta: &Meta,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        self.repo
            .update_character_meta(meta, &meta.discord_id)
            .await
    }

    pub async fn add_character_identity(
        &self,
        identity: &Identity,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Updating character identity for discord_id {}: {:?}",
            discord_id, identity
        );
        self.repo
            .update_character_indentity(identity, discord_id)
            .await
    }

    pub async fn add_character_progression(
        &self,
        progression: &Progression,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Updating character progression for discord_id {}: {:?}",
            discord_id, progression
        );
        self.repo
            .update_character_progression(progression, discord_id)
            .await
    }

    pub async fn add_character_combat(
        &self,
        combat: &Combat,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Updating character combat for discord_id {}: {:?}",
            discord_id, combat
        );
        self.repo.update_character_combat(combat, discord_id).await
    }

    pub async fn add_character_inventory(
        &self,
        discord_id: &str,
        item: Inventory,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Adding character inventory for discord_id {}: {:?}",
            discord_id, item
        );
        self.repo
            .update_character_inventory(&item, discord_id)
            .await
    }

    pub async fn add_character_spells(
        &self,
        discord_id: &str,
        spells: &Magic,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Adding character spells for discord_id {}: {:?}",
            discord_id, spells
        );
        self.repo.update_character_spells(spells, discord_id).await
    }

    pub async fn add_character_abilities(
        &self,
        discord_id: &str,
        abilities: AbilitiesBlock,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Adding character abilities for discord_id {}: {:?}",
            discord_id, abilities
        );
        self.repo.update_character_abilities(&abilities, discord_id).await
    }

    pub async fn add_character_skills(
        &self,
        discord_id: &str,
        skills: SkillsBlock,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Adding character skills for discord_id {}: {:?}",
            discord_id, skills
        );
        self.repo.update_character_skills(&skills, discord_id).await
    }

    pub async fn add_character_traits(
        &self,
        discord_id: &str,
        traits: Traits,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Adding character traits for discord_id {}: {:?}",
            discord_id, traits
        );
        self.repo.update_character_traits(&traits, discord_id).await
    }

    pub async fn add_character_notes(
        &self,
        discord_id: &str,
        notes: Notes,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Adding character notes for discord_id {}: {:?}",
            discord_id, notes
        );
        self.repo.update_character_notes(&notes, discord_id).await
    }

    pub async fn add_item(
        &self,
        discord_id: &str,
        item: Item,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;
        character.inventory.items.push(item);
        character.inventory.items.sort_by_key(|f| f.name.clone());
        self.upsert_character(character).await
    }

    pub async fn remove_item(
        &self,
        discord_id: &str,
        item_name: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;
        character
            .inventory
            .items
            .retain(|item| item.name != item_name);
        self.upsert_character(character).await
    }

    pub async fn add_spell(
        &self,
        discord_id: &str,
        spell: Spell,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;
        character.magic.spells.spells.push(spell);
        character
            .magic
            .spells
            .spells
            .sort_by_key(|s| s.name.clone());
        self.upsert_character(character).await
    }

    pub async fn update_spell_slots(
        &self,
        discord_id: &str,
        level: u64,
        slot: u64,
        used: u64,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;

        // Find and update the spell slot for the given level
        if let Some(spell_slot) = character
            .magic
            .spells
            .spell_slots
            .iter_mut()
            .find(|s| s.level == level)
        {
            spell_slot.slot = slot;
            spell_slot.used = used;
        } else {
            // If spell slot doesn't exist, create a new one
            character
                .magic
                .spells
                .spell_slots
                .push(SpellSlot { level, slot, used });
        }

        // Sort by level
        character.magic.spells.spell_slots.sort_by_key(|s| s.level);
        self.upsert_character(character).await
    }

    pub async fn update_current_hp(
        &self,
        discord_id: &str,
        current_hp: u64,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;
        character.combat.hit_points = current_hp;
        self.upsert_character(character).await
    }

    pub async fn update_max_hp(
        &self,
        discord_id: &str,
        max_hp: u64,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;
        character.progression.max_hp = max_hp;
        self.upsert_character(character).await
    }

    pub async fn update_character_level(
        &self,
        discord_id: &str,
        level: u64,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let mut character = self.get_character(discord_id).await?;
        character.progression.level = level;
        character.progression.update_progression();
        self.upsert_character(character).await
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeMap, error::Error, sync::Arc};

    use insta::assert_json_snapshot;

    use crate::{
        character::{
            entity::{
                AbilitiesBlock, Ability, AbilityScore, Action, CharacterSheet, Characteristics,
                Combat, CombatAction, Defenses, FeatureTraits, Identity, Inventory, Item, Magic,
                Meta, Notes, ProficianciesTrainings, Progression, SavingThrows, Sense, Skill,
                SkillStatus, Skills, SkillsBlock, Speed, Spell, SpellSlot, Spells, Traits,
            },
            repository::CharacterSheetRepository,
            service::CharacterSheetService,
        },
        config::{AiDmConfig, ServiceConfig},
        pg_pool::{TestPgPool, TestPgPoolConfig},
    };

    #[tokio::test]
    async fn test_integration() -> Result<(), Box<dyn Error>> {
        let character = CharacterSheet {
            meta: Meta {
                discord_id: "400114655500042240".to_owned(),
                location: "Tavern".to_owned(),
                story_summary: Some("Just started to join the game".to_owned()),
                extra_creatures: vec![],
                dead: false,
                performing_action: Some("Drinking".to_owned()),
                action_end_time: Some("Harptos 24 Mar 1555 12:35PM".to_owned()),
            },
            identity: Identity {
                character_name: "真珠".to_owned(),
                species: "Aasimar".to_owned(),
                sub_species: None,
                class: "Sorcerer".to_owned(),
                sub_class: Some("Draconic Bloodline - Gold".to_owned()),
                characteristics: Characteristics {
                    background: "Noble".to_owned(),
                    background_feature: "Position of Privilege".to_owned(),
                    background_feature_description: "Thanks to your noble birth, \
                        people are inclined to think the best of you. \
                        You are welcome in high society, \
                        and people assume you have the right to be wherever you are. \
                        The common folk make every effort to accommodate you and \
                        avoid your displeasure, and other people of high birth \
                        treat you as a member of the same social sphere. \
                        You can secure an audience with a local noble \
                        if you need to."
                        .to_owned(),
                    alignment: "Lawful neutral".to_owned(),
                    gender: "Female".to_owned(),
                    eyes: "Blue".to_owned(),
                    size: "Medium".to_owned(),
                    height: "5'7″".to_owned(),
                    faith: "Amber Lord".to_owned(),
                    hair: "Blonde".to_owned(),
                    skin: "White".to_owned(),
                    age: 28,
                    weight: "104 lbs.".to_owned(),
                    personality_traits: "My eloquent flattery makes everyone \
                        I talk to feel like the most wonderful and important \
                        person in the world."
                        .to_owned(),
                    ideals: "Responsibility. It is my duty to respect \
                        the authority of those above me, just as those below \
                        me must respect mine. (Lawful)"
                        .to_owned(),
                    bonds: "The common folk must see me as a hero of the people.".to_owned(),
                    flaws: "I too often hear veiled insults and threats \
                        in every word addressed to me, and I'm quick to anger."
                        .to_owned(),
                    appearance_trait: vec![],
                },
            },
            progression: Progression {
                level: 2,
                xp: 330,
                total_hit_dice: "2d8".to_owned(),
                max_hp: 16,
                proficiencies: ProficianciesTrainings {
                    proficiency_bonus: 2,
                    armor: vec![],
                    weapons: vec![
                        "Crossbow".to_owned(),
                        "Light".to_owned(),
                        "Dagger".to_owned(),
                        "Dart".to_owned(),
                        "Quarterstaff".to_owned(),
                        "Sling".to_owned(),
                    ],
                    tools: vec![
                        "Dragonchess Set".to_owned(),
                        "Painter's Supplies".to_owned(),
                    ],
                    languages: vec![
                        "Celestial".to_owned(),
                        "Common".to_owned(),
                        "Draconic".to_owned(),
                    ],
                },
            },
            combat: Combat {
                armor_class: 15,
                initiative: 2,
                hit_points: 16,
                speed: vec![Speed {
                    name: "Walking".to_owned(),
                    value: "30 ft".to_owned(),
                }],
                senses: vec![Sense {
                    name: "Darkvision".to_owned(),
                    value: "60 ft".to_owned(),
                }],
                defenses: Defenses {
                    resistance: vec!["Necrotic".to_owned(), "Radiant".to_owned()],
                    immunities: vec![],
                    vulnerabilities: vec![],
                },
                conditions: vec![],
                exhaustion_level: 0,
                saving_throws: SavingThrows {
                    proficiency: vec![Ability::Constitution, Ability::Charisma],
                    saving_sets: BTreeMap::new(),
                },
                actions: vec![
                    Action {
                        name: "Attack".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                    Action {
                        name: "Dash".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                    Action {
                        name: "Disengage".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                ],
                combat_actions: vec![
                    CombatAction {
                        name: "Dagger".to_owned(),
                        hit_dc: Some(4),
                        damage: "1d4+2".to_owned(),
                    },
                    CombatAction {
                        name: "Dagger".to_owned(),
                        hit_dc: Some(4),
                        damage: "1d4+2".to_owned(),
                    },
                    CombatAction {
                        name: "Ray of Front".to_owned(),
                        hit_dc: Some(5),
                        damage: "1d8".to_owned(),
                    },
                    CombatAction {
                        name: "Unarmed Strike".to_owned(),
                        hit_dc: Some(1),
                        damage: "0".to_owned(),
                    },
                ],
            },
            abilities_block:AbilitiesBlock {
                abilities: BTreeMap::from([
                    (
                        Ability::Strength,
                        AbilityScore {
                            base: 8,
                            modifier: 0,
                        },
                    ),
                    (
                        Ability::Dexterity,
                        AbilityScore {
                            base: 14,
                            modifier: 0,
                        },
                    ),
                    (
                        Ability::Constitution,
                        AbilityScore {
                            base: 14,
                            modifier: 0,
                        },
                    ),
                    (
                        Ability::Intelligence,
                        AbilityScore {
                            base: 10,
                            modifier: 0,
                        },
                    ),
                    (
                        Ability::Wisdom,
                        AbilityScore {
                            base: 11,
                            modifier: 0,
                        },
                    ),
                    (
                        Ability::Charisma,
                        AbilityScore {
                            base: 17,
                            modifier: 0,
                        },
                    ),
                ]),
            },
            skills_block: SkillsBlock {
                skills: Skills {
                    skills: BTreeMap::from([
                        (
                            Skill::Acrobatics,
                            SkillStatus {
                                prof: false,
                                bonus: 0,
                                modifier: Ability::Dexterity,
                                passive: 0,
                            },
                        ),
                        (
                            Skill::AnimalHandling,
                            SkillStatus {
                                prof: false,
                                bonus: 0,
                                modifier: Ability::Wisdom,
                                passive: 0,
                            },
                        ),
                        (
                            Skill::Arcana,
                            SkillStatus {
                                prof: true,
                                bonus: 0,
                                modifier: Ability::Intelligence,
                                passive: 0,
                            },
                        ),
                    ]),
                },
            },
            magic: Magic {
                spells: Spells {
                    spells: vec![
                        Spell {
                            name: "Light".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "Touch".to_owned(),
                            hit_dc: Some(13),
                            effect: "Creation".to_owned(),
                        },
                        Spell {
                            name: "Magic Missile".to_owned(),
                            level: 1,
                            cast_time: "1 action".to_owned(),
                            range: "120 ft".to_owned(),
                            hit_dc: None,
                            effect: "1d4+1".to_owned(),
                        },
                    ],
                    spell_slots: vec![SpellSlot {
                        level: 1,
                        slot: 3,
                        used: 0,
                    }],
                    ability_type: Ability::Charisma,
                    ability_modifier: 3,
                    spell_attack: 5,
                    save_dc: 13,
                },
            },
            inventory: Inventory {
                items: vec![
                    Item {
                        name: "Clothes, Fine".to_owned(),
                        weight: 6,
                        quantity: Some(1),
                        cost_gp: 15,
                        equiped: false,
                    },
                    Item {
                        name: "Dagger".to_owned(),
                        weight: 1,
                        quantity: None,
                        cost_gp: 2,
                        equiped: true,
                    },
                ],
            },
            traits: Traits {
                features_and_traits: vec![
                    FeatureTraits {
                        name: "Spellcasting Ability".to_owned(),
                        description:
                            "Charisma is your spellcasting ability for your sorcerer spells, \
                            since the power of your magic relies on your ability to project your will \
                            into the world. You use your Charisma whenever a spell refers to your \
                            spellcasting ability. In addition, you use your Charisma modifier when \
                            setting the saving throw DC for a sorcerer spell you cast and when making \
                            an attack roll with one.

Spell save DC = 8 + your proficiency bonus + your Charisma modifier

Spell attack modifier = your proficiency bonus + your Charisma modifier"
                                .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                    },
                    FeatureTraits {
                        name: "Dragon Ancestor".to_owned(),
                        description:
                            "At 1st level, you choose one type of dragon as your ancestor. \
                            The damage type associated with each dragon is used by features you gain later.
You can speak, read, and write Draconic. Additionally, whenever you make a \
                            Charisma check when interacting with dragons, your proficiency bonus is \
                            doubled if it applies to the check."
                                .to_owned(),
                        duration: None,
                        trigger: None,
                        cooldown: None,
                    },
                ],
            },
            notes: Notes {
                organizations: None,
                allies: None,
                enemies: None,
                backstory: "Pearl is a rank P45 senior manager of the Strategic Investment \
            Department in the Interastral Peace Corporation, a member of the Ten Stonehearts, \
            the leader of Pearluxe Corp, and the CEO of Planarcadia."
                    .to_owned(),
                other: None,
            },
        };

        let service_config: ServiceConfig<AiDmConfig> =
            ServiceConfig::load("./config/config.yaml")?;
        let db_config = service_config
            .database
            .as_ref()
            .ok_or_else(|| crate::error::Error::MissingConfig("database"))?;
        let pg_pool = TestPgPool::init(TestPgPoolConfig {
            migrations: "./migrations".into(),
            db_name: db_config.db_name.clone(),
            host: db_config.host.clone(),
            port: db_config.port,
            default_database: "postgres".to_owned(),
            username: db_config.username.clone(),
            password: db_config.password.clone(),
        })
        .await;
        let pool = pg_pool.resource().await;

        let repo = Arc::new(CharacterSheetRepository::from_pool(pool));

        let service = CharacterSheetService { repo };

        service.upsert_character(character.clone()).await?;

        let character = service.get_character(&character.meta.discord_id).await?;

        let character_json = serde_json::to_value(&character)?;

        assert_json_snapshot!(character_json);

        Ok(())
    }
}
