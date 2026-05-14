use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "LLM Service",
        description = "LLM Service API",
        version = "1.0.0",
    ),
    servers(
        (url = "http://localhost:30000", description = "Local server"),
    ),
    tags(
        (name = "llm", description = "LLM endpoints"),
    ),
    paths(
        crate::llm::routes::validate,
    ),
)]
pub struct LlmOpenApi;
