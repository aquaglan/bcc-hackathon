//! Wire types. They are the contract with the Python harnesses and the public
//! API: change them together with `contracts/*.json` and `harness/.../protocol.py`.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The technical specification under review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Spec {
    pub title: Option<String>,
    /// Name of the uploaded file, so people can find the document they sent.
    /// Reviews stored before the field existed load with `null`.
    #[serde(default)]
    pub file_name: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Question {
    pub id: String,
    pub text: String,
    /// Why the answer changes the assessment.
    pub reason: String,
    pub section: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Answer {
    pub question_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Major,
    Minor,
}

/// Where a running step is, for a person watching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Cutting the document into requirements.
    Extract,
    /// Checking each requirement and, alongside, the whole set.
    Check,
    /// The second reviewer tries to refute the serious findings.
    Critic,
    /// Merging and ranking the questions to the author.
    Questions,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Stage,
    Found,
    Dropped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ProgressEvent {
    pub kind: EventKind,
    pub severity: Option<Severity>,
    pub text: String,
    /// Milliseconds since the step began.
    pub at_ms: u64,
}

/// The latest state of a step. Counts are what the checks reported so far,
/// before verification: the report has the final ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Progress {
    pub stage: Stage,
    pub requirements: usize,
    pub checked: usize,
    pub critical: usize,
    pub major: usize,
    pub minor: usize,
    /// Newest last, at most [`crate::progress::EVENTS_KEPT`].
    pub events: Vec<ProgressEvent>,
}

/// One requirement as the agent cut it out of the specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Requirement {
    /// Report-wide id, `R-01`.
    pub id: String,
    /// The id the document itself gives, `Ф-1`; absent when it has none.
    pub source_id: Option<String>,
    pub section: String,
    /// Verbatim text from the specification.
    pub quote: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Finding {
    /// Absent when the finding is about the set of requirements as a whole.
    pub requirement_id: Option<String>,
    pub severity: Severity,
    pub category: String,
    pub section: Option<String>,
    pub quote: Option<String>,
    pub problem: String,
    pub suggestion: String,
    /// A second model tried to refute the finding and could not.
    pub verified: bool,
    /// Why the second model kept the finding; absent when it did not check it.
    pub critic_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Report {
    pub summary: String,
    pub requirements: Vec<Requirement>,
    pub findings: Vec<Finding>,
    /// Questions that stayed unanswered when the review ended.
    pub open_questions: Vec<Question>,
    /// Models that actually answered, for the report's provenance.
    pub models: Vec<String>,
    /// Findings verification removed; they never count as defects.
    #[serde(default)]
    pub dropped: Vec<Dropped>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    /// The same defect stays elsewhere in the report.
    Duplicate,
    /// The critic showed the finding is wrong.
    Refuted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Dropped {
    pub finding: Finding,
    pub reason: DropReason,
    /// For a duplicate, the requirement of the finding that stays; absent when
    /// that one is a set-level finding, and for a refuted finding.
    pub kept_in: Option<String>,
    /// The critic's reason; absent for a merge done by code.
    pub note: Option<String>,
}

/// One round of clarification: what the agent asked and what the author answered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Round {
    pub questions: Vec<Question>,
    pub answers: Vec<Answer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// A harness step is in flight.
    Running,
    /// The last round has questions for the author.
    AwaitingAnswers,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Review {
    pub id: Uuid,
    pub harness: String,
    pub spec: Spec,
    pub status: ReviewStatus,
    pub rounds: Vec<Round>,
    pub report: Option<Report>,
    pub error: Option<String>,
}

/// A line of the review list: enough to name a review and show its outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewSummary {
    pub id: Uuid,
    pub title: Option<String>,
    pub file_name: Option<String>,
    pub status: ReviewStatus,
    /// Counts of the report; absent until the review is done.
    pub findings: Option<usize>,
    pub critical: Option<usize>,
    pub major: Option<usize>,
}

impl From<&Review> for ReviewSummary {
    fn from(review: &Review) -> Self {
        let count = |s: Severity| {
            review
                .report
                .as_ref()
                .map(|r| r.findings.iter().filter(|f| f.severity == s).count())
        };
        Self {
            id: review.id,
            title: review.spec.title.clone(),
            file_name: review.spec.file_name.clone(),
            status: review.status,
            findings: review.report.as_ref().map(|r| r.findings.len()),
            critical: count(Severity::Critical),
            major: count(Severity::Major),
        }
    }
}

/// Input of one harness step: the whole review so far. Harnesses are stateless.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StepRequest {
    pub review_id: Uuid,
    pub spec: Spec,
    pub rounds: Vec<Round>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepResult {
    Ask { questions: Vec<Question> },
    Done { report: Report },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Provider-neutral completion request served by the LLM gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LlmRequest {
    /// Provider name from the config; the default provider when absent.
    pub provider: Option<String>,
    /// Overrides the provider's configured model.
    pub model: Option<String>,
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub max_tokens: Option<u32>,
    /// When set, the reply text is JSON valid against this schema. Every
    /// object needs `additionalProperties: false` and all properties required.
    #[schema(value_type = Option<Object>)]
    pub json_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LlmResponse {
    pub text: String,
    /// The model that actually answered.
    pub model: String,
    pub usage: Usage,
}
