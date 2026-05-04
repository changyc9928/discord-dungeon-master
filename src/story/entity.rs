use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct StoryEntity {
    pub summary: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct DialogueEntity {
    pub dialogue: String,
    pub author_name: String,
    pub author_character: String,
    pub author_discord_id: String,
    pub updated_at: DateTime<Utc>,
}