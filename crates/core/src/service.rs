//! Use cases. The HTTP layer calls these and nothing else.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::domain::{
    Answer, LlmRequest, LlmResponse, Question, Review, ReviewStatus, ReviewSummary, Round, Spec,
    StepRequest, StepResult,
};
use crate::ports::{Harness, LlmError, LlmProvider, ReviewStore, StoreError};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("review {0} not found")]
    NotFound(Uuid),
    #[error("unknown harness `{0}`")]
    UnknownHarness(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Conflict(String),
    #[error("storage failed: {0}")]
    Store(#[from] StoreError),
}

/// The review lifecycle: `running` -> (`awaiting_answers` -> `running`)* -> `done` | `failed`.
pub struct ReviewService {
    store: Arc<dyn ReviewStore>,
    harnesses: HashMap<String, Arc<dyn Harness>>,
    default_harness: String,
}

impl ReviewService {
    pub fn new(
        store: Arc<dyn ReviewStore>,
        harnesses: HashMap<String, Arc<dyn Harness>>,
        default_harness: String,
    ) -> Self {
        Self {
            store,
            harnesses,
            default_harness,
        }
    }

    /// Registers a review in `running`; the caller then runs [`Self::advance`].
    pub async fn create(
        &self,
        spec: Spec,
        harness: Option<String>,
    ) -> Result<Review, ServiceError> {
        if spec.text.trim().is_empty() {
            return Err(ServiceError::InvalidInput("spec.text is empty".into()));
        }
        let harness = harness.unwrap_or_else(|| self.default_harness.clone());
        if !self.harnesses.contains_key(&harness) {
            return Err(ServiceError::UnknownHarness(harness));
        }
        let review = Review {
            id: Uuid::new_v4(),
            harness,
            spec,
            status: ReviewStatus::Running,
            rounds: Vec::new(),
            report: None,
            error: None,
        };
        self.store.put(review.clone()).await?;
        Ok(review)
    }

    pub async fn list(&self) -> Result<Vec<ReviewSummary>, ServiceError> {
        Ok(self
            .store
            .list()
            .await?
            .iter()
            .map(ReviewSummary::from)
            .collect())
    }

    pub async fn get(&self, id: Uuid) -> Result<Review, ServiceError> {
        self.store.get(id).await?.ok_or(ServiceError::NotFound(id))
    }

    /// Records the author's answers and moves the review back to `running`.
    /// In `awaiting_answers` they answer the last round; an empty list means
    /// "skip the questions". In `done` they answer the report's open questions
    /// and start a re-check with them, so at least one answer is needed.
    // shortcut: read-check-write is not atomic, two simultaneous submissions
    // can both pass; add a version check when the store becomes a database.
    pub async fn answer(&self, id: Uuid, answers: Vec<Answer>) -> Result<Review, ServiceError> {
        let mut review = self.get(id).await?;
        let asked = |questions: &[Question]| match answers
            .iter()
            .find(|a| !questions.iter().any(|q| q.id == a.question_id))
        {
            Some(stray) => Err(ServiceError::InvalidInput(format!(
                "no question `{}` to answer",
                stray.question_id
            ))),
            None => Ok(()),
        };
        match (
            review.status,
            review.rounds.last_mut(),
            review.report.take(),
        ) {
            (ReviewStatus::AwaitingAnswers, Some(round), report) => {
                asked(&round.questions)?;
                round.answers = answers;
                review.report = report;
            }
            (ReviewStatus::Done, _, Some(report)) => {
                if answers.is_empty() {
                    return Err(ServiceError::InvalidInput(
                        "a re-check needs at least one answer".into(),
                    ));
                }
                asked(&report.open_questions)?;
                review.rounds.push(Round {
                    questions: report.open_questions,
                    answers,
                });
            }
            _ => {
                return Err(ServiceError::Conflict(
                    "review is neither awaiting answers nor done".into(),
                ));
            }
        }
        review.status = ReviewStatus::Running;
        self.store.put(review.clone()).await?;
        Ok(review)
    }

    /// Runs one harness step for a `running` review and stores the outcome.
    /// A harness failure is an outcome too: the review becomes `failed`.
    pub async fn advance(&self, id: Uuid) -> Result<Review, ServiceError> {
        let mut review = self.get(id).await?;
        if review.status != ReviewStatus::Running {
            return Err(ServiceError::Conflict("review is not running".into()));
        }
        let harness = self
            .harnesses
            .get(&review.harness)
            .ok_or_else(|| ServiceError::UnknownHarness(review.harness.clone()))?;
        let request = StepRequest {
            review_id: review.id,
            spec: review.spec.clone(),
            rounds: review.rounds.clone(),
        };
        let failure = match harness.step(&request).await {
            Ok(StepResult::Ask { questions }) if questions.is_empty() => {
                Some("harness asked zero questions".to_owned())
            }
            Ok(StepResult::Ask { questions }) => {
                review.rounds.push(Round {
                    questions,
                    answers: Vec::new(),
                });
                review.status = ReviewStatus::AwaitingAnswers;
                None
            }
            Ok(StepResult::Done { report }) => {
                review.report = Some(report);
                review.status = ReviewStatus::Done;
                None
            }
            Err(e) => Some(e.to_string()),
        };
        if let Some(error) = failure {
            review.status = ReviewStatus::Failed;
            review.error = Some(error);
        }
        self.store.put(review.clone()).await?;
        Ok(review)
    }
}

/// The one door to language models: picks a configured provider by name.
/// Harnesses call it over HTTP, so keys and the cloud/on-premise choice stay in
/// the server config.
pub struct LlmGateway {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    default_provider: String,
}

impl LlmGateway {
    pub fn new(providers: HashMap<String, Arc<dyn LlmProvider>>, default_provider: String) -> Self {
        Self {
            providers,
            default_provider,
        }
    }

    pub async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        if req.messages.is_empty() {
            return Err(LlmError::InvalidRequest("messages is empty".into()));
        }
        let name = req.provider.as_deref().unwrap_or(&self.default_provider);
        let provider = self
            .providers
            .get(name)
            .ok_or_else(|| LlmError::UnknownProvider(name.to_owned()))?;
        provider.complete(req).await
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::domain::{Question, Report};
    use crate::memory::MemoryStore;
    use crate::ports::HarnessError;

    /// Asks once, then finishes; fails on a spec that says "boom".
    struct Scripted;

    #[async_trait]
    impl Harness for Scripted {
        async fn step(&self, req: &StepRequest) -> Result<StepResult, HarnessError> {
            if req.spec.text == "boom" {
                return Err(HarnessError("model is down".into()));
            }
            if req.rounds.is_empty() {
                return Ok(StepResult::Ask {
                    questions: vec![Question {
                        id: "r1q1".into(),
                        text: "Which currency?".into(),
                        reason: "Limits depend on it".into(),
                        section: None,
                    }],
                });
            }
            let answered: usize = req.rounds.iter().map(|r| r.answers.len()).sum();
            Ok(StepResult::Done {
                report: Report {
                    summary: format!("{answered} answers"),
                    requirements: Vec::new(),
                    findings: Vec::new(),
                    open_questions: vec![Question {
                        id: "r2q1".into(),
                        text: "Which limit?".into(),
                        reason: "It sets the threshold".into(),
                        section: None,
                    }],
                    models: Vec::new(),
                    dropped: Vec::new(),
                },
            })
        }
    }

    fn service() -> ReviewService {
        let harnesses: HashMap<String, Arc<dyn Harness>> = HashMap::from([(
            "scripted".to_owned(),
            Arc::new(Scripted) as Arc<dyn Harness>,
        )]);
        ReviewService::new(
            Arc::new(MemoryStore::default()),
            harnesses,
            "scripted".into(),
        )
    }

    fn spec(text: &str) -> Spec {
        Spec {
            title: None,
            file_name: None,
            text: text.into(),
        }
    }

    #[tokio::test]
    async fn review_goes_through_a_clarification_round() {
        let svc = service();
        let id = svc.create(spec("Transfers"), None).await.unwrap().id;

        let asked = svc.advance(id).await.unwrap();
        assert_eq!(asked.status, ReviewStatus::AwaitingAnswers);
        assert_eq!(asked.rounds[0].questions[0].id, "r1q1");

        let answers = vec![Answer {
            question_id: "r1q1".into(),
            text: "KZT".into(),
        }];
        assert_eq!(
            svc.answer(id, answers).await.unwrap().status,
            ReviewStatus::Running
        );

        let done = svc.advance(id).await.unwrap();
        assert_eq!(done.status, ReviewStatus::Done);
        assert_eq!(done.report.unwrap().summary, "1 answers");
    }

    #[tokio::test]
    async fn answers_to_a_report_start_a_re_check() {
        let svc = service();
        let id = svc.create(spec("Transfers"), None).await.unwrap().id;
        svc.advance(id).await.unwrap();
        svc.answer(id, Vec::new()).await.unwrap();
        svc.advance(id).await.unwrap();

        // Nothing to re-check with, or an answer to a question the report does not ask.
        assert!(matches!(
            svc.answer(id, Vec::new()).await,
            Err(ServiceError::InvalidInput(_))
        ));
        let stray = vec![Answer {
            question_id: "r1q1".into(),
            text: "KZT".into(),
        }];
        assert!(matches!(
            svc.answer(id, stray).await,
            Err(ServiceError::InvalidInput(_))
        ));

        let answers = vec![Answer {
            question_id: "r2q1".into(),
            text: "50 000".into(),
        }];
        let rechecking = svc.answer(id, answers).await.unwrap();
        assert_eq!(rechecking.status, ReviewStatus::Running);
        assert_eq!(rechecking.rounds.len(), 2);
        assert!(rechecking.report.is_none());
        let done = svc.advance(id).await.unwrap();
        assert_eq!(done.report.unwrap().summary, "1 answers");
    }

    #[tokio::test]
    async fn bad_input_is_rejected() {
        let svc = service();
        assert!(matches!(
            svc.create(spec("  "), None).await,
            Err(ServiceError::InvalidInput(_))
        ));
        assert!(matches!(
            svc.create(spec("x"), Some("nope".into())).await,
            Err(ServiceError::UnknownHarness(_))
        ));

        let id = svc.create(spec("x"), None).await.unwrap().id;
        // Answers before any question was asked.
        assert!(matches!(
            svc.answer(id, Vec::new()).await,
            Err(ServiceError::Conflict(_))
        ));
        svc.advance(id).await.unwrap();
        let stray = vec![Answer {
            question_id: "r9q9".into(),
            text: "?".into(),
        }];
        assert!(matches!(
            svc.answer(id, stray).await,
            Err(ServiceError::InvalidInput(_))
        ));
        // A step for a review that waits for the author.
        assert!(matches!(
            svc.advance(id).await,
            Err(ServiceError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn the_list_is_newest_first_with_counts_once_done() {
        let svc = service();
        let first = svc.create(spec("Transfers"), None).await.unwrap().id;
        let second = svc.create(spec("Cards"), None).await.unwrap().id;
        svc.advance(first).await.unwrap();
        svc.answer(first, Vec::new()).await.unwrap();
        svc.advance(first).await.unwrap();
        let list = svc.list().await.unwrap();
        let ids: Vec<Uuid> = list.iter().map(|r| r.id).collect();
        assert_eq!(ids, [second, first]);
        assert_eq!((list[0].findings, list[1].findings), (None, Some(0)));
        assert_eq!(list[1].status, ReviewStatus::Done);
    }

    #[tokio::test]
    async fn harness_failure_marks_the_review_failed() {
        let svc = service();
        let id = svc.create(spec("boom"), None).await.unwrap().id;
        let failed = svc.advance(id).await.unwrap();
        assert_eq!(failed.status, ReviewStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("model is down"));
    }

    #[tokio::test]
    async fn gateway_routes_by_provider_name() {
        let gateway = LlmGateway::new(HashMap::new(), "cloud".into());
        let mut req = LlmRequest {
            provider: None,
            model: None,
            system: None,
            messages: Vec::new(),
            max_tokens: None,
            json_schema: None,
        };
        assert!(matches!(
            gateway.complete(&req).await,
            Err(LlmError::InvalidRequest(_))
        ));
        req.messages.push(crate::domain::Message {
            role: crate::domain::Role::User,
            content: "hi".into(),
        });
        assert!(matches!(
            gateway.complete(&req).await,
            Err(LlmError::UnknownProvider(name)) if name == "cloud"
        ));
    }
}
