use sqlx::PgPool;

use crate::story::{
    entity::{DialogueEntity, StoryEntity},
    error::StoryError,
};

pub struct StoryRepository {
    pool: PgPool,
}

impl StoryRepository {
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn get_story(&self) -> Result<StoryEntity, StoryError> {
        Ok(sqlx::query_as::<_, StoryEntity>(
            r#"
            SELECT
                *
            FROM story
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn insert_new_story(&self, story: &str) -> Result<StoryEntity, StoryError> {
        Ok(sqlx::query_as::<_, StoryEntity>(
            r#"
            INSERT INTO story (story, updated_at)
            VALUES ($1, NOW())
            RETURNING *
            "#,
        )
        .bind(story)
        .fetch_one(&self.pool)
        .await?)
    }
}

pub struct DialogueRepository {
    pool: PgPool,
}

impl DialogueRepository {
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert_new_dialogue(
        &self,
        dialogue: &str,
        author_name: &str,
        author_character: &str,
        author_discord_id: &str,
    ) -> Result<(), StoryError> {
        sqlx::query_as::<_, DialogueEntity>(
            r#"
            INSERT INTO dialogues (dialogue, author_name, author_character, author_discord_id, updated_at)
            VALUES ($1, $2, $3, $4, NOW())
            RETURNING *
            "#,
        )
        .bind(dialogue)
        .bind(author_name)
        .bind(author_character)
        .bind(author_discord_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_table(&self) -> Result<(), StoryError> {
        sqlx::query(
            r#"
            DELETE FROM dialogues
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_latest_dialogues(
        &self,
        number_of_dialogues: i64,
    ) -> Result<Vec<DialogueEntity>, StoryError> {
        Ok(sqlx::query_as::<_, DialogueEntity>(
            r#"
            SELECT * FROM dialogues
            ORDER BY updated_at DESC
            LIMIT $1
            RETURNING *
            "#,
        )
        .bind(number_of_dialogues)
        .fetch_all(&self.pool)
        .await?)
    }
}
