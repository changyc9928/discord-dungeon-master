use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoryError {
    #[error(transparent)]
    SqlError(#[from] sqlx::Error),
}
