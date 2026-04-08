use crate::character::entities::Ability;

#[derive(Debug, thiserror::Error)]
pub enum CharacterSheetError {
    #[error("Missing {0} for {1}")]
    MissingAbilityBonus(Ability, String),

    #[error("Multiple characters found with name '{0}': found {1} results")]
    MultipleResultsFound(String, usize),

    #[error(transparent)]
    SerdeYamlError(#[from] serde_json::Error),

    #[error(transparent)]
    SqlxError(#[from] sqlx::Error),

    #[error(transparent)]
    InternalError(#[from] stable_eyre::Report),
}
