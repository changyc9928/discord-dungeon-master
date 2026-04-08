use sqlx::PgPool;

use crate::character::{
    entities::{
        CharacterSheet, abilities_block::AbilitiesBlock, combat::Combat, identity::Identity,
        inventory::Inventory, magic::Magic, meta::Meta, notes::Notes, progression::Progression,
        skills::Skills, traits::Traits,
    },
    error::CharacterSheetError,
};

#[cfg_attr(test, faux::create)]
pub struct CharacterSheetRepository {
    pool: PgPool,
}

#[cfg_attr(test, faux::methods)]
impl CharacterSheetRepository {
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert_character(
        &self,
        character_sheet: &CharacterSheet,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills_block,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            meta = EXCLUDED.meta,
            identity = EXCLUDED.identity,
            progression = EXCLUDED.progression,
            combat = EXCLUDED.combat,
            abilities_block = EXCLUDED.abilities_block,
            skills_block = EXCLUDED.skills_block,
            magic = EXCLUDED.magic,
            inventory = EXCLUDED.inventory,
            traits = EXCLUDED.traits,
            notes = EXCLUDED.notes,
            updated_at = NOW()
        RETURNING *
        "#,
        )
        .bind(character_sheet.meta.discord_id.clone()) // ⚠️ see note below
        .bind(&character_sheet.meta)
        .bind(&character_sheet.identity)
        .bind(&character_sheet.progression)
        .bind(&character_sheet.combat)
        .bind(&character_sheet.abilities_block)
        .bind(&character_sheet.skills)
        .bind(&character_sheet.magic)
        .bind(&character_sheet.inventory)
        .bind(&character_sheet.traits)
        .bind(&character_sheet.notes)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_meta(
        &self,
        meta: &Meta,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills_block,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            meta = EXCLUDED.meta
        RETURNING *
        "#,
        )
        .bind(discord_id)
        .bind(&meta)
        .bind(Identity::default())
        .bind(Progression::default())
        .bind(Combat::default())
        .bind(AbilitiesBlock::default())
        .bind(Skills::default())
        .bind(Magic::default())
        .bind(Inventory::default())
        .bind(Traits::default())
        .bind(Notes::default())
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_indentity(
        &self,
        identity: &Identity,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character identity for discord_id {}: {:?}",
            discord_id, identity
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            identity = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(&identity)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_progression(
        &self,
        progression: &Progression,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character progression for discord_id {}: {:?}",
            discord_id, progression
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            progression = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(&progression)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_combat(
        &self,
        combat: &Combat,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character combat for discord_id {}: {:?}",
            discord_id, combat
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            combat = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(&combat)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_inventory(
        &self,
        inventory: &Inventory,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character inventory for discord_id {}: {:?}",
            discord_id, inventory
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            inventory = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(inventory)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_spells(
        &self,
        spells: &Magic,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character spells for discord_id {}: {:?}",
            discord_id, spells
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            magic = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(spells)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_abilities(
        &self,
        abilities: &AbilitiesBlock,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character abilities for discord_id {}: {:?}",
            discord_id, abilities
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            abilities_block = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(abilities)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_skills(
        &self,
        skills: &Skills,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character skills for discord_id {}: {:?}",
            discord_id, skills
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            skills_block = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(skills)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_traits(
        &self,
        traits: &Traits,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character traits for discord_id {}: {:?}",
            discord_id, traits
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            traits = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(traits)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn update_character_notes(
        &self,
        notes: &Notes,
        discord_id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        println!(
            "Database updating character notes for discord_id {}: {:?}",
            discord_id, notes
        );
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        UPDATE character_sheets
        SET
            notes = $1
        WHERE id = $2
        RETURNING *
        "#,
        )
        .bind(notes)
        .bind(discord_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_character_by_discord_id(
        &self,
        id: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        SELECT
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills_block,
            magic,
            inventory,
            traits,
            notes
        FROM character_sheets
        WHERE id = $1
        LIMIT 1
        "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_character_by_name(
        &self,
        character_name: &str,
    ) -> Result<CharacterSheet, CharacterSheetError> {
        let results = sqlx::query_as::<_, CharacterSheet>(
            r#"
        SELECT
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills_block,
            magic,
            inventory,
            traits,
            notes
        FROM character_sheets
        WHERE identity->>'characterName' = $1
        "#,
        )
        .bind(character_name)
        .fetch_all(&self.pool)
        .await?;

        match results.len() {
            0 => Err(sqlx::Error::RowNotFound.into()),
            1 => Ok(results.into_iter().next().unwrap()),
            n => Err(CharacterSheetError::MultipleResultsFound(
                character_name.to_string(),
                n,
            )),
        }
    }
}
