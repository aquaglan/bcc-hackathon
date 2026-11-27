//! The public HTTP API. Handlers translate HTTP to use cases and back; they
//! hold no logic of their own.

use std::sync::Arc;

use anyq_core::domain::{Answer, LlmRequest, LlmResponse, Progress, Review, ReviewSummary, Spec};
use anyq_core::ports::LlmError;
use anyq_core::progress::ProgressBoard;
use anyq_core::service::{LlmGateway, ReviewService, ServiceError};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::document::{self, DocumentError};

/// A requirements PDF is rarely above a few megabytes; scans are refused anyway.
const DOCUMENT_LIMIT: usize = 20 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub reviews: Arc<ReviewService>,
    pub llm: Arc<LlmGateway>,
    /// Filled by built-in harnesses; an external harness reports no progress.
    pub progress: Arc<ProgressBoard>,
    pub api_token: Option<Arc<str>>,
}

#[derive(OpenApi)]
#[openapi(info(
    title = "AnyQ API",
    description = "Review of fintech technical specifications by an AI agent."
))]
struct ApiDoc;

pub fn app(state: AppState) -> Router {
    let (v1, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(create_review, list_reviews))
        .routes(routes!(get_review))
        .routes(routes!(get_progress))
        .routes(routes!(document_text))
        .routes(routes!(submit_answers))
        .routes(routes!(llm_complete))
        .split_for_parts();
    Router::new()
        .merge(v1.layer(middleware::from_fn_with_state(state.clone(), require_token)))
        .route("/health", get(|| async { "ok" }))
        .route("/openapi.json", get(move || async move { Json(openapi) }))
        .layer(DefaultBodyLimit::max(DOCUMENT_LIMIT))
        .with_state(state)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateReview {
    pub spec: Spec,
    /// Harness name from the config; the default harness when absent.
    pub harness: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitAnswers {
    /// Answers to questions of the current round; empty skips the round.
    pub answers: Vec<Answer>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct DocumentName {
    /// The file name; its extension picks the format: `.pdf`, `.md`, `.txt`.
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentText {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Starts a review. The agent works in the background: poll `GET /v1/reviews/{id}`.
#[utoipa::path(
    post,
    path = "/v1/reviews",
    request_body = CreateReview,
    responses((status = 202, body = Review), (status = 400, body = ApiError))
)]
async fn create_review(
    State(state): State<AppState>,
    Json(body): Json<CreateReview>,
) -> Result<(StatusCode, Json<Review>), Failure> {
    let review = state.reviews.create(body.spec, body.harness).await?;
    spawn_step(&state, review.id);
    Ok((StatusCode::ACCEPTED, Json(review)))
}

/// All reviews, newest first, with the report's counts once done.
#[utoipa::path(
    get,
    path = "/v1/reviews",
    responses((status = 200, body = [ReviewSummary]))
)]
async fn list_reviews(State(state): State<AppState>) -> Result<Json<Vec<ReviewSummary>>, Failure> {
    Ok(Json(state.reviews.list().await?))
}

/// The text of a requirements file and its title, ready for `POST /v1/reviews`.
#[utoipa::path(
    post,
    path = "/v1/documents",
    params(DocumentName),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses((status = 200, body = DocumentText), (status = 400, body = ApiError))
)]
async fn document_text(
    Query(file): Query<DocumentName>,
    body: Bytes,
) -> Result<Json<DocumentText>, Failure> {
    let name = file.name.clone();
    let text = tokio::task::spawn_blocking(move || document::text_of(&name, &body))
        .await
        .map_err(|e| {
            Failure::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "parser_failed",
                e.to_string(),
            )
        })??;
    let title = document::title_of(&text, &file.name);
    Ok(Json(DocumentText { title, text }))
}

/// The review with its status, clarification rounds and, when done, the report.
#[utoipa::path(
    get,
    path = "/v1/reviews/{id}",
    params(("id" = Uuid, Path,)),
    responses((status = 200, body = Review), (status = 404, body = ApiError))
)]
async fn get_review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Review>, Failure> {
    Ok(Json(state.reviews.get(id).await?))
}

/// Where the running step is: stage, counts so far and a feed of what was found.
/// 404 when nothing reports progress for this review (an external harness).
#[utoipa::path(
    get,
    path = "/v1/reviews/{id}/progress",
    params(("id" = Uuid, Path,)),
    responses((status = 200, body = Progress), (status = 404, body = ApiError))
)]
async fn get_progress(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Progress>, Failure> {
    state.progress.get(id).map(Json).ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no progress for {id}"),
        )
    })
}

/// Answers the questions of a review and resumes the agent: in `awaiting_answers`
/// the last round's, in `done` the report's open questions, which starts a re-check.
#[utoipa::path(
    post,
    path = "/v1/reviews/{id}/answers",
    params(("id" = Uuid, Path,)),
    request_body = SubmitAnswers,
    responses(
        (status = 202, body = Review),
        (status = 400, body = ApiError),
        (status = 404, body = ApiError),
        (status = 409, body = ApiError)
    )
)]
async fn submit_answers(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubmitAnswers>,
) -> Result<(StatusCode, Json<Review>), Failure> {
    let review = state.reviews.answer(id, body.answers).await?;
    spawn_step(&state, id);
    Ok((StatusCode::ACCEPTED, Json(review)))
}

/// The LLM gateway used by harnesses: one request shape for every provider.
#[utoipa::path(
    post,
    path = "/v1/llm/complete",
    request_body = LlmRequest,
    responses(
        (status = 200, body = LlmResponse),
        (status = 400, body = ApiError),
        (status = 422, body = ApiError),
        (status = 502, body = ApiError)
    )
)]
async fn llm_complete(
    State(state): State<AppState>,
    Json(body): Json<LlmRequest>,
) -> Result<Json<LlmResponse>, Failure> {
    Ok(Json(state.llm.complete(&body).await?))
}

// shortcut: a step runs as a task of this process, so a restart loses it and
// the review stays `running`; move steps to a job queue when that matters.
fn spawn_step(state: &AppState, id: Uuid) {
    let reviews = state.reviews.clone();
    tokio::spawn(async move {
        if let Err(error) = reviews.advance(id).await {
            tracing::error!(%id, %error, "review step was not recorded");
        }
    });
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if let Some(expected) = &state.api_token {
        let given = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !given.is_some_and(|given| same_bytes(given.as_bytes(), expected.as_bytes())) {
            return Failure::new(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "bearer token required",
            )
            .into_response();
        }
    }
    next.run(request).await
}

/// Comparison in time independent of where the inputs differ.
fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

struct Failure(StatusCode, ApiError);

impl Failure {
    fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self(
            status,
            ApiError {
                code: code.to_owned(),
                message: message.into(),
            },
        )
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, Json(self.1)).into_response()
    }
}

impl From<ServiceError> for Failure {
    fn from(error: ServiceError) -> Self {
        let (status, code) = match &error {
            ServiceError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ServiceError::UnknownHarness(_) => (StatusCode::BAD_REQUEST, "unknown_harness"),
            ServiceError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            ServiceError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ServiceError::Store(_) => (StatusCode::INTERNAL_SERVER_ERROR, "storage_failed"),
        };
        Self::new(status, code, error.to_string())
    }
}

impl From<DocumentError> for Failure {
    fn from(error: DocumentError) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "unreadable_document",
            error.to_string(),
        )
    }
}

impl From<LlmError> for Failure {
    fn from(error: LlmError) -> Self {
        let (status, code) = match &error {
            LlmError::UnknownProvider(_) => (StatusCode::BAD_REQUEST, "unknown_provider"),
            LlmError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            LlmError::Refused(_) => (StatusCode::UNPROCESSABLE_ENTITY, "refused"),
            LlmError::Truncated => (StatusCode::UNPROCESSABLE_ENTITY, "truncated"),
            LlmError::Status { .. } | LlmError::Transport(_) | LlmError::BadResponse(_) => {
                (StatusCode::BAD_GATEWAY, "provider_failed")
            }
        };
        Self::new(status, code, error.to_string())
    }
}
