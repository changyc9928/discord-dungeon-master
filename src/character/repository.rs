use sqlx::PgPool;

use crate::character::{
    entity::{
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
            skills,
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
            skills = EXCLUDED.skills,
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
            skills,
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            identity = EXCLUDED.identity
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            progression = EXCLUDED.progression
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            combat = EXCLUDED.combat
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            inventory = EXCLUDED.inventory
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            spells = EXCLUDED.spells
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            abilities = EXCLUDED.abilities
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            skills = EXCLUDED.skills
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            traits = EXCLUDED.traits
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            notes = EXCLUDED.notes
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
            skills,
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
        Ok(sqlx::query_as::<_, CharacterSheet>(
            r#"
        SELECT
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        FROM character_sheets
        WHERE identity->>'characterName' = $1
        "#,
        )
        .bind(character_name)
        .fetch_one(&self.pool)
        .await?)
    }
}
