//! Outbound ports. Every integration (model provider, agent harness, storage)
//! is an implementation of one of these traits in `anyq-adapters`.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{LlmRequest, LlmResponse, Review, StepRequest, StepResult};

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("unknown provider `{0}`")]
    UnknownProvider(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("the model refused the request: {0}")]
    Refused(String),
    #[error("the reply was cut at max_tokens")]
    Truncated,
    #[error("provider returned HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("provider unreachable: {0}")]
    Transport(String),
    #[error("unexpected provider response: {0}")]
    BadResponse(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError>;
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HarnessError(pub String);

/// An agent implementation. One step either asks the author or ends the review.
#[async_trait]
pub trait Harness: Send + Sync {
    async fn step(&self, req: &StepRequest) -> Result<StepResult, HarnessError>;
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StoreError(pub String);

#[async_trait]
pub trait ReviewStore: Send + Sync {
    async fn get(&self, id: Uuid) -> Result<Option<Review>, StoreError>;
    async fn put(&self, review: Review) -> Result<(), StoreError>;
    /// All reviews, newest first.
    async fn list(&self) -> Result<Vec<Review>, StoreError>;
}
