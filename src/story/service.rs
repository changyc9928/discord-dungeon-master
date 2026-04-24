use std::sync::Arc;

use crate::story::{
    entity::DialogueEntity,
    error::StoryError,
    repository::{DialogueRepository, StoryRepository},
};

pub struct StoryService {
    pub repository: Arc<StoryRepository>,
    pub dialogue_repository: Arc<DialogueRepository>,
    pub compile_trigger: i64,
}

impl StoryService {
    pub async fn get_latest_story(&self) -> Result<String, StoryError> {
        Ok(self.repository.get_story().await?.summary)
    }

    pub async fn insert_new_story(&self, story: &str) -> Result<(), StoryError> {
        self.repository.insert_new_story(story).await?;
        Ok(())
    }

    pub async fn get_latest_dialogues(&self) -> Result<Vec<DialogueEntity>, StoryError> {
        Ok(self
            .dialogue_repository
            .get_latest_dialogues(self.compile_trigger)
            .await?)
    }

    pub async fn insert_new_dialogue(
        &self,
        dialogue: &str,
        author_name: &str,
        author_character: &str,
        author_discord_id: &str,
    ) -> Result<(), StoryError> {
        self.dialogue_repository
            .insert_new_dialogue(dialogue, author_name, author_character, author_discord_id)
            .await
    }

    pub async fn clear_dialogue_table(&self) -> Result<(), StoryError> {
        self.dialogue_repository.clear_table().await
    }
}
