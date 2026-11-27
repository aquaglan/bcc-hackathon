//! The API end to end, with a scripted harness and model in place of real ones.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyq_core::domain::{
    LlmRequest, LlmResponse, Question, Report, StepRequest, StepResult, Usage,
};
use anyq_core::memory::MemoryStore;
use anyq_core::ports::{Harness, HarnessError, LlmError, LlmProvider};
use anyq_core::progress::ProgressBoard;
use anyq_core::service::{LlmGateway, ReviewService};
use anyq_server::api::{AppState, app};
use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

struct AsksOnce;

#[async_trait]
impl Harness for AsksOnce {
    async fn step(&self, req: &StepRequest) -> Result<StepResult, HarnessError> {
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
        Ok(StepResult::Done {
            report: Report {
                summary: req.rounds[0].answers[0].text.clone(),
                requirements: Vec::new(),
                findings: Vec::new(),
                open_questions: Vec::new(),
                models: Vec::new(),
                dropped: Vec::new(),
            },
        })
    }
}

struct Echo;

#[async_trait]
impl LlmProvider for Echo {
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse {
            text: req.messages[0].content.to_uppercase(),
            model: "echo".into(),
            usage: Usage {
                input_tokens: 1,
                output_tokens: 1,
            },
        })
    }
}

fn test_app(api_token: Option<&str>) -> Router {
    let harnesses: HashMap<String, Arc<dyn Harness>> =
        HashMap::from([("asks".to_owned(), Arc::new(AsksOnce) as Arc<dyn Harness>)]);
    let providers: HashMap<String, Arc<dyn LlmProvider>> =
        HashMap::from([("echo".to_owned(), Arc::new(Echo) as Arc<dyn LlmProvider>)]);
    app(AppState {
        reviews: Arc::new(ReviewService::new(
            Arc::new(MemoryStore::default()),
            harnesses,
            "asks".into(),
        )),
        llm: Arc::new(LlmGateway::new(providers, "echo".into())),
        progress: Arc::new(ProgressBoard::default()),
        api_token: api_token.map(Arc::from),
    })
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Steps run in the background, so the status is polled like a real client would.
async fn wait_for(app: &Router, id: &str, status: &str) -> Value {
    for _ in 0..200 {
        let (_, review) = call(app, "GET", &format!("/v1/reviews/{id}"), None).await;
        if review["status"] == status {
            return review;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("review {id} never became {status}");
}

#[tokio::test]
async fn review_runs_from_spec_to_report() {
    let app = test_app(None);
    let spec = json!({"spec": {"title": null, "file_name": "transfers.md", "text": "Transfers"}});
    let (status, created) = call(&app, "POST", "/v1/reviews", Some(spec)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(created["status"], "running");
    let id = created["id"].as_str().unwrap().to_owned();

    let asked = wait_for(&app, &id, "awaiting_answers").await;
    assert_eq!(asked["rounds"][0]["questions"][0]["id"], "r1q1");

    let answers = json!({"answers": [{"question_id": "r1q1", "text": "KZT"}]});
    let uri = format!("/v1/reviews/{id}/answers");
    let (status, _) = call(&app, "POST", &uri, Some(answers.clone())).await;
    assert_eq!(status, StatusCode::ACCEPTED);

    let done = wait_for(&app, &id, "done").await;
    assert_eq!(done["report"]["summary"], "KZT");

    // The review is finished: answers go to the report's questions, and this
    // report asks none of them.
    let (status, error) = call(&app, "POST", &uri, Some(answers)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "invalid_input");

    // This harness reports no progress.
    let (status, _) = call(&app, "GET", &format!("/v1/reviews/{id}/progress"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, list) = call(&app, "GET", "/v1/reviews", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["findings"], 0);
    assert_eq!(list[0]["file_name"], "transfers.md");
}

#[tokio::test]
async fn a_document_is_turned_into_text() {
    let app = test_app(None);
    let request = Request::builder()
        .method("POST")
        .uri("/v1/documents?name=spec.md")
        .body(Body::from("# Переводы\n\nЛимит 100 000 ₸."))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["title"], "Переводы");
    assert!(document["text"].as_str().unwrap().contains("100 000"));

    let request = Request::builder()
        .method("POST")
        .uri("/v1/documents?name=spec.docx")
        .body(Body::from("x"))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn errors_have_a_status_and_a_code() {
    let app = test_app(None);
    let missing = "/v1/reviews/5f0c1b3e-8a55-4d0c-9c2f-3d1f6f6f0a11";
    assert_eq!(
        call(&app, "GET", missing, None).await.0,
        StatusCode::NOT_FOUND
    );

    let empty = json!({"spec": {"text": " "}});
    let (status, error) = call(&app, "POST", "/v1/reviews", Some(empty)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "invalid_input");

    let unknown = json!({"provider": "nope", "messages": [{"role": "user", "content": "hi"}]});
    let (status, error) = call(&app, "POST", "/v1/llm/complete", Some(unknown)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["code"], "unknown_provider");
}

#[tokio::test]
async fn gateway_serves_the_default_provider() {
    let app = test_app(None);
    let request = json!({"messages": [{"role": "user", "content": "hi"}]});
    let (status, reply) = call(&app, "POST", "/v1/llm/complete", Some(request)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["text"], "HI");
}

#[tokio::test]
async fn token_guards_v1_but_not_health_or_the_schema() {
    let app = test_app(Some("s3cret"));
    let missing = "/v1/reviews/5f0c1b3e-8a55-4d0c-9c2f-3d1f6f6f0a11";
    assert_eq!(
        call(&app, "GET", missing, None).await.0,
        StatusCode::UNAUTHORIZED
    );

    let authorized = Request::builder()
        .uri(missing)
        .header("authorization", "Bearer s3cret")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(authorized).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let health = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(health).await.unwrap().status(),
        StatusCode::OK
    );

    let (status, schema) = call(&app, "GET", "/openapi.json", None).await;
    assert_eq!(status, StatusCode::OK);
    for path in [
        "/v1/reviews",
        "/v1/reviews/{id}",
        "/v1/reviews/{id}/answers",
        "/v1/llm/complete",
    ] {
        assert!(
            schema["paths"][path].is_object(),
            "{path} is missing from the schema"
        );
    }
}
