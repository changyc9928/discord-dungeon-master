use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utoipa::ToSchema;

use crate::character::entity::CharacterSheet;
use crate::character::service::CharacterSheetService;
use crate::llm::{Llm, LlmError};

#[derive(Clone)]
pub struct LlmApi {
    pub llm_service: Arc<Mutex<dyn Llm>>,
    pub character_sheet_service: Arc<CharacterSheetService>,
}

pub const LLM_PATH: &str = "/dm/v1/";
pub const LLM_TAG: &str = "llm";

impl From<LlmApi> for Router {
    fn from(value: LlmApi) -> Self {
        Router::new()
            .route(&format!("{LLM_PATH}/validate"), post(validate))
            .layer(Extension(value.clone()))
    }
}

#[utoipa::path(
    post,
    path = &format!("{LLM_PATH}/validate"),
    summary = "Validate the incoming chat request and forward it to the LLM",
    description = "Accepts a chat request, optionally validates the supplied character sheet against the database, builds the final prompt, and forwards it to the LLM service.",
    request_body(content = ValidateRequest),
    responses(
        (status = 200, description = "Ok", body = ValidateResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Insufficient permissions"),
    ),
    tag = LLM_TAG,
)]
async fn validate(
    Extension(api): Extension<LlmApi>,
    Json(request): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, LlmError> {
    let message = if let Some(character) = request.character_sheet {
        let character_sheet = match api
            .character_sheet_service
            .get_character(&character.meta.discord_id)
            .await
        {
            Ok(c) => c,
            Err(_) => {
                let c = api
                    .character_sheet_service
                    .upsert_character(character.clone())
                    .await?;
                c
            }
        };

        if character_sheet != character {
            return Err(LlmError::MissingContent(
                "Character sheet does not match the one stored in DB".to_owned(),
            ));
        }

        format!(
            "提到的角色的角色卡：{character:?}

信息：{}",
            request.message
        )
    } else {
        request.message
    };
    let response = api
        .llm_service
        .lock()
        .await
        .request_to_llm(&request.author_discord_id, &message)
        .await?;
    Ok(Json(ValidateResponse { message: response }))
}

#[derive(ToSchema, Deserialize)]
pub struct ValidateRequest {
    author_discord_id: String,
    message: String,
    character_sheet: Option<CharacterSheet>,
}

#[derive(ToSchema, Serialize)]
pub struct ValidateResponse {
    message: String,
}
