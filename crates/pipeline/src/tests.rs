#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyq_core::domain::EventKind;
use anyq_core::domain::{
    Answer, LlmRequest, LlmResponse, Round, Spec, StepRequest, StepResult, Usage,
};
use anyq_core::ports::{LlmError, LlmProvider};
use anyq_core::progress::ProgressBoard;
use anyq_core::service::LlmGateway;
use async_trait::async_trait;
use serde_json::{Value, json};
use uuid::Uuid;

use super::*;
use crate::document::PAGE_BREAK;

const SPEC: &str = "\
# Переводы

## Функциональные требования

* **Ф-1.** Клиент переводит до 100 000 ₸ в сутки.
* **Ф-2.** Перевод проходит быстро.

## Тест-кейсы

Перевод 150 000 ₸ проходит успешно.
";

/// Which stage a request is, by the shape of the schema it asks for.
fn stage(schema: &Value) -> &'static str {
    let properties = &schema["properties"];
    if properties.get("verdicts").is_some() {
        "critic"
    } else if properties.get("final_questions").is_some() {
        "questions"
    } else if properties.get("summary").is_some() {
        "set"
    } else if properties["requirements"]["items"]["properties"]
        .get("requirement_id")
        .is_some()
    {
        "check"
    } else {
        "extract"
    }
}

fn finding(quote: &str) -> Value {
    json!({"criterion": "REQ-VERIFIABLE", "severity": "major", "quote": quote,
           "problem": "п", "suggestion": "и"})
}

fn replies(verdicts: Value) -> HashMap<&'static str, Value> {
    HashMap::from([
        (
            "extract",
            json!({"requirements": [
                {"source_id": "Ф-2", "section": "ФТ", "quote": "Перевод проходит быстро.", "summary": "Скорость"},
                {"source_id": "Ф-1", "section": "ФТ", "quote": "Клиент переводит до 100 000 ₸ в сутки.", "summary": "Лимит"},
                {"source_id": "Ф-9", "section": "ФТ", "quote": "Такого текста нет.", "summary": "?"}
            ]}),
        ),
        (
            "check",
            json!({"requirements": [
                {"requirement_id": "R-02",
                 "findings": [finding("проходит быстро"), finding("выдуманная цитата"), finding("Перевод проходит быстро")],
                 "questions": [{"text": "Сколько секунд?", "reason": "порог"}]},
                {"requirement_id": "R-01", "findings": [], "questions": []}
            ]}),
        ),
        (
            "set",
            json!({"summary": "Документ не готов.", "findings": [
            {"criterion": "SET-CONSISTENT", "severity": "critical", "section": null,
             "quote": "Перевод 150 000 ₸ проходит успешно.", "problem": "Тест противоречит R-01.", "suggestion": "Исправить тест."},
            {"criterion": "CHK-ROLLBACK", "severity": "major", "section": "Весь документ",
             "quote": null, "problem": "Нет отката.", "suggestion": "Описать откат."}
        ], "questions": []}),
        ),
        ("critic", json!({"verdicts": verdicts})),
    ])
}

/// Answers each stage from `replies`; records the prompt each stage got.
struct Fake {
    replies: HashMap<&'static str, Value>,
    seen: Mutex<HashMap<&'static str, String>>,
}

#[async_trait]
impl LlmProvider for Fake {
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let stage = stage(req.json_schema.as_ref().unwrap());
        let prompt = req.messages[0].content.clone();
        self.seen.lock().unwrap().insert(stage, prompt.clone());
        let reply = match self.replies.get(stage) {
            Some(reply) => reply.clone(),
            None => keep_every_question(&prompt),
        };
        Ok(LlmResponse {
            text: reply.to_string(),
            model: format!("model-for-{stage}"),
            usage: Usage {
                input_tokens: 1,
                output_tokens: 1,
            },
        })
    }
}

/// The questions stage when a test does not script it: every question kept as asked.
fn keep_every_question(prompt: &str) -> Value {
    let kept: Vec<Value> = prompt
        .lines()
        .filter_map(|line| {
            let (id, rest) = line.strip_prefix("- ")?.split_once(' ')?;
            let text = rest.split_once("] ")?.1.rsplit_once(" (зачем: ")?.0;
            Some(json!({"from": [id], "text": text, "reason": "р"}))
        })
        .collect();
    json!({ "final_questions": kept })
}

struct Broken;

#[async_trait]
impl LlmProvider for Broken {
    async fn complete(&self, _: &LlmRequest) -> Result<LlmResponse, LlmError> {
        Err(LlmError::Transport("timed out".into()))
    }
}

fn pipeline_on(
    provider: Arc<dyn LlmProvider>,
    mode: Mode,
    progress: Arc<ProgressBoard>,
) -> Pipeline {
    let gateway = LlmGateway::new(
        HashMap::from([("fake".to_owned(), provider)]),
        "fake".into(),
    );
    Pipeline::new(Arc::new(gateway), mode, progress).unwrap()
}

fn pipeline(provider: Arc<dyn LlmProvider>, mode: Mode) -> Pipeline {
    pipeline_on(provider, mode, Arc::default())
}

fn request(rounds: Vec<Round>) -> StepRequest {
    StepRequest {
        review_id: Uuid::nil(),
        spec: Spec {
            title: None,
            file_name: None,
            text: SPEC.into(),
        },
        rounds,
    }
}

fn fake(verdicts: Value) -> Arc<Fake> {
    Arc::new(Fake {
        replies: replies(verdicts),
        seen: Mutex::default(),
    })
}

async fn report(provider: Arc<Fake>) -> Report {
    report_on(SPEC, provider).await
}

async fn report_on(text: &str, provider: Arc<Fake>) -> Report {
    let mut req = request(vec![]);
    req.spec.text = text.into();
    match pipeline(provider, Mode::Static).step(&req).await.unwrap() {
        StepResult::Done { report } => report,
        StepResult::Ask { .. } => panic!("a static run asked"),
    }
}

fn verdict(id: &str, kind: &str, question: Option<&str>) -> Value {
    json!({"finding_id": id, "verdict": kind, "reason": "р", "question": question, "duplicate_of": null})
}

fn duplicate(id: &str, of: &str) -> Value {
    json!({"finding_id": id, "verdict": "duplicate", "reason": "р", "question": null, "duplicate_of": of})
}

#[tokio::test]
async fn requirements_are_numbered_in_document_order_and_invented_ones_dropped() {
    let report = report(fake(json!([]))).await;
    let ids: Vec<(&str, Option<&str>)> = report
        .requirements
        .iter()
        .map(|r| (r.id.as_str(), r.source_id.as_deref()))
        .collect();
    assert_eq!(ids, [("R-01", Some("Ф-1")), ("R-02", Some("Ф-2"))]);
    assert_eq!(
        report.requirements[0].section,
        "Функциональные требования, строка 5"
    );
    assert!(
        report
            .summary
            .contains("требований без найденной в тексте цитаты: 1")
    );
}

#[tokio::test]
async fn findings_are_anchored_by_code_and_the_critic_decides_their_fate() {
    // F0 is the R-02 finding (its repeat was merged), F1 the contradiction, F2 the missing rollback.
    let verdicts = json!([
        verdict("F0", "confirmed", None),
        verdict("F1", "refuted", None),
        verdict("F2", "uncertain", Some("Откат есть?"))
    ]);
    let report = report(fake(verdicts)).await;
    let kept: Vec<(Option<&str>, &str, bool)> = report
        .findings
        .iter()
        .map(|f| (f.requirement_id.as_deref(), f.category.as_str(), f.verified))
        .collect();
    assert_eq!(kept, [(Some("R-02"), "REQ-VERIFIABLE", true)]);
    assert_eq!(report.findings[0].critic_note.as_deref(), Some("р"));
    assert_eq!(
        report.findings[0].section.as_deref(),
        Some("Функциональные требования, строка 6")
    );
    let questions: Vec<&str> = report
        .open_questions
        .iter()
        .map(|q| q.text.as_str())
        .collect();
    assert_eq!(questions, ["Сколько секунд?", "Откат есть?"]);
    assert_eq!(report.open_questions[0].id, "r1q1");
    for note in [
        "замечаний без найденной в тексте цитаты: 1",
        "повторов одного замечания: 1",
        "опровергнутых критиком: 1",
    ] {
        assert!(report.summary.contains(note), "{}", report.summary);
    }
    let dropped: Vec<(DropReason, Option<&str>, Option<&str>)> = report
        .dropped
        .iter()
        .map(|d| (d.reason, d.kept_in.as_deref(), d.note.as_deref()))
        .collect();
    // The code merge comes first and has no critic's note.
    assert_eq!(
        dropped,
        [
            (DropReason::Duplicate, Some("R-02"), None),
            (DropReason::Refuted, None, Some("р"))
        ]
    );
    assert_eq!(report.dropped[1].finding.category, "SET-CONSISTENT");
    assert_eq!(
        report.models,
        [
            "model-for-check",
            "model-for-critic",
            "model-for-extract",
            "model-for-questions",
            "model-for-set"
        ]
    );
}

#[tokio::test]
async fn findings_the_critic_did_not_see_stay_unverified_and_sorted_by_severity() {
    let report = report(fake(json!([]))).await;
    let kept: Vec<(Severity, bool)> = report
        .findings
        .iter()
        .map(|f| (f.severity, f.verified))
        .collect();
    assert_eq!(
        kept,
        [
            (Severity::Critical, false),
            (Severity::Major, false),
            (Severity::Major, false)
        ]
    );
}

#[tokio::test]
async fn the_document_is_passed_as_data_with_criteria_and_requirements() {
    let provider = fake(json!([]));
    report(provider.clone()).await;
    let seen = provider.seen.lock().unwrap();
    assert!(seen["extract"].starts_with("<document>\n# Переводы"));
    assert!(seen["check"].contains("REQ-VERIFIABLE") && seen["check"].contains("R-02"));
    assert!(seen["set"].contains("R-01") && seen["set"].contains("CHK-ROLLBACK"));
    assert!(!seen["extract"].contains("<answers>"));
}

#[tokio::test]
async fn dialog_asks_first_then_reviews_with_the_answers() {
    let provider = fake(json!([]));
    let dialog = pipeline(provider.clone(), Mode::Dialog);
    let StepResult::Ask { questions } = dialog.step(&request(vec![])).await.unwrap() else {
        panic!("the dialog did not ask");
    };
    assert_eq!(questions[0].id, "r1q1");
    let answered = Round {
        answers: vec![Answer {
            question_id: "r1q1".into(),
            text: "3 секунды".into(),
        }],
        questions,
    };
    let second = dialog.step(&request(vec![answered])).await.unwrap();
    assert!(matches!(second, StepResult::Done { .. }));
    let seen = provider.seen.lock().unwrap();
    assert!(seen["check"].contains("<answers>") && seen["set"].contains("3 секунды"));
}

#[tokio::test]
async fn a_model_failure_or_a_reply_off_schema_fails_the_step() {
    let error = pipeline(Arc::new(Broken), Mode::Static)
        .step(&request(vec![]))
        .await
        .unwrap_err();
    assert!(error.0.contains("timed out"), "{error}");

    let mut garbage = replies(json!([]));
    garbage.insert("extract", json!({"requirements": "not a list"}));
    let provider = Arc::new(Fake {
        replies: garbage,
        seen: Mutex::default(),
    });
    let error = pipeline(provider, Mode::Static)
        .step(&request(vec![]))
        .await
        .unwrap_err();
    assert!(
        error.0.contains("extract reply does not match its schema"),
        "{error}"
    );
}

#[tokio::test]
async fn a_repeat_goes_only_when_what_it_repeats_stays() {
    // F2 repeats F1, which is confirmed: F2 goes. F1 claims to repeat F0, which is
    // refuted: F1 must stay, or the problem would vanish from the report.
    let verdicts = json!([
        verdict("F0", "refuted", None),
        verdict("F1", "confirmed", None),
        duplicate("F2", "F1")
    ]);
    let report = report(fake(verdicts)).await;
    let kept: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.category.as_str())
        .collect();
    assert_eq!(kept, ["SET-CONSISTENT"]);
    assert!(
        report.summary.contains("повторов одного замечания: 2"),
        "{}",
        report.summary
    );

    let verdicts = json!([
        verdict("F0", "refuted", None),
        duplicate("F1", "F0"),
        duplicate("F2", "F2")
    ]);
    let report = super::tests::report(fake(verdicts)).await;
    let kept: Vec<(&str, bool)> = report
        .findings
        .iter()
        .map(|f| (f.category.as_str(), f.verified))
        .collect();
    assert_eq!(kept, [("SET-CONSISTENT", true), ("CHK-ROLLBACK", true)]);
}

fn merged_questions() -> HashMap<&'static str, Value> {
    // Q0 comes from the check, Q1 and Q2 from the critic. The stage merges Q1 into Q0,
    // invents one question and forgets Q2.
    let verdicts = json!([
        verdict("F0", "uncertain", Some("Секунд сколько?")),
        verdict("F1", "uncertain", Some("Тест верен?")),
        verdict("F2", "confirmed", None)
    ]);
    let mut replies = replies(verdicts);
    replies.insert(
        "questions",
        json!({"final_questions": [
            {"from": ["Q1", "Q0"], "text": "Сколько секунд на перевод?", "reason": "р"},
            {"from": ["Q9"], "text": "Выдуманный?", "reason": "р"}
        ]}),
    );
    replies
}

#[tokio::test]
async fn questions_are_merged_and_none_is_lost_or_invented() {
    let provider = Arc::new(Fake {
        replies: merged_questions(),
        seen: Mutex::default(),
    });
    let report = report(provider.clone()).await;
    let questions: Vec<(&str, Option<&str>)> = report
        .open_questions
        .iter()
        .map(|q| (q.text.as_str(), q.section.as_deref()))
        .collect();
    // The merged question takes the place of its first source; the forgotten Q2 is kept.
    assert_eq!(
        questions,
        [
            (
                "Сколько секунд на перевод?",
                Some("Функциональные требования, строка 6")
            ),
            ("Тест верен?", Some("Тест-кейсы, строка 10"))
        ]
    );
    assert!(
        report.summary.contains("вопросов, слитых с другими: 1"),
        "{}",
        report.summary
    );
    assert!(provider.seen.lock().unwrap()["questions"].contains("Q2 ["));
}

#[tokio::test]
async fn the_first_dialog_round_asks_only_the_main_questions() {
    let provider = Arc::new(Fake {
        replies: merged_questions(),
        seen: Mutex::default(),
    });
    let mut dialog = pipeline(provider, Mode::Dialog);
    dialog.agent.limits.main_questions = 1;
    let StepResult::Ask { questions } = dialog.step(&request(vec![])).await.unwrap() else {
        panic!("the dialog did not ask");
    };
    let asked: Vec<&str> = questions.iter().map(|q| q.text.as_str()).collect();
    assert_eq!(asked, ["Сколько секунд на перевод?"]);
}

#[tokio::test]
async fn the_run_reports_its_progress() {
    let board = Arc::new(ProgressBoard::default());
    let run = pipeline_on(fake(json!([])), Mode::Static, board.clone());
    run.step(&request(vec![])).await.unwrap();
    let progress = board.get(Uuid::nil()).unwrap();
    assert_eq!(progress.stage, Stage::Done);
    assert_eq!((progress.requirements, progress.checked), (2, 2));
    // Before verification: three from R-02 (one has no real quote), two from the set.
    assert_eq!(
        (progress.critical, progress.major, progress.minor),
        (1, 4, 0)
    );
    let found: Vec<&str> = progress
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Found)
        .map(|e| e.text.as_str())
        .collect();
    assert!(found[0].starts_with("R-02 · REQ-VERIFIABLE: "), "{found:?}");
    assert_eq!(progress.events.last().unwrap().text, "Готово");
}

/// Fails the first call of every stage, then answers like `Fake`.
struct Flaky {
    inner: Arc<Fake>,
    failed: Mutex<HashSet<&'static str>>,
}

#[async_trait]
impl LlmProvider for Flaky {
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let stage = stage(req.json_schema.as_ref().unwrap());
        if self.failed.lock().unwrap().insert(stage) {
            return Err(LlmError::BadResponse("no text in the reply".into()));
        }
        self.inner.complete(req).await
    }
}

#[tokio::test]
async fn a_stage_that_fails_once_is_asked_again() {
    let provider = Arc::new(Flaky {
        inner: fake(json!([])),
        failed: Mutex::default(),
    });
    let result = pipeline(provider, Mode::Static)
        .step(&request(vec![]))
        .await
        .unwrap();
    assert!(matches!(result, StepResult::Done { .. }));
}

#[tokio::test]
async fn findings_about_missing_text_are_not_merged_by_place() {
    let mut replies = replies(json!([]));
    let missing = |problem: &str| {
        json!({"criterion": "SET-COMPLETE", "severity": "minor", "section": null,
               "quote": null, "problem": problem, "suggestion": "Описать."})
    };
    replies.insert(
        "set",
        json!({"summary": "с", "findings": [missing("Нет отката."), missing("Нет приоритетов.")],
               "questions": []}),
    );
    let report = report(Arc::new(Fake {
        replies,
        seen: Mutex::default(),
    }))
    .await;
    let set: Vec<&str> = report
        .findings
        .iter()
        .filter(|f| f.category == "SET-COMPLETE")
        .map(|f| f.problem.as_str())
        .collect();
    assert_eq!(set, ["Нет отката.", "Нет приоритетов."]);
}

const SPEC_WITH_PLACEHOLDER: &str = "\
# Переводы

## Функциональные требования

* **Ф-1.** Клиент переводит до 100 000 ₸ в сутки.
* **Ф-2.** Перевод проходит быстро.

## Тестирование

заполняются тестировщиками
";

#[tokio::test]
async fn a_placeholder_becomes_a_verified_finding_the_critic_never_sees() {
    let provider = fake(json!([]));
    let report = report_on(SPEC_WITH_PLACEHOLDER, provider.clone()).await;
    let lint: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.problem.starts_with("Раздел не заполнен"))
        .collect();
    assert_eq!(lint.len(), 1);
    assert_eq!(lint[0].category, "SET-COMPLETE");
    assert_eq!(lint[0].severity, Severity::Major);
    assert!(lint[0].verified && lint[0].requirement_id.is_none());
    assert_eq!(lint[0].quote.as_deref(), Some("заполняются тестировщиками"));
    assert_eq!(lint[0].section.as_deref(), Some("Тестирование, строка 10"));
    assert!(!provider.seen.lock().unwrap()["critic"].contains("Раздел не заполнен"));
}

#[tokio::test]
async fn a_model_finding_at_the_same_place_suppresses_the_lint_one() {
    let mut replies = replies(json!([]));
    replies.insert(
        "set",
        json!({"summary": "Документ не готов.", "findings": [
            {"criterion": "CHK-ROLLBACK", "severity": "major", "section": null,
             "quote": "заполняются тестировщиками", "problem": "Раздел тестов пуст.",
             "suggestion": "Написать тесты."}
        ], "questions": []}),
    );
    let provider = Arc::new(Fake {
        replies,
        seen: Mutex::default(),
    });
    let report = report_on(SPEC_WITH_PLACEHOLDER, provider).await;
    let here: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.section.as_deref() == Some("Тестирование, строка 10"))
        .collect();
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].problem, "Раздел тестов пуст.");
}

#[tokio::test]
async fn a_model_finding_at_a_placeholder_is_confirmed_by_code_not_shown_to_the_critic() {
    let mut replies = replies(json!([]));
    replies.insert(
        "set",
        json!({"summary": "Документ не готов.", "findings": [
            {"criterion": "CHK-ROLLBACK", "severity": "major", "section": null,
             "quote": "заполняются тестировщиками", "problem": "Раздел тестов пуст.",
             "suggestion": "Написать тесты."}
        ], "questions": []}),
    );
    let provider = Arc::new(Fake {
        replies,
        seen: Mutex::default(),
    });
    let report = report_on(SPEC_WITH_PLACEHOLDER, provider.clone()).await;
    assert!(!provider.seen.lock().unwrap()["critic"].contains("Раздел тестов пуст."));
    let here: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.section.as_deref() == Some("Тестирование, строка 10"))
        .collect();
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].problem, "Раздел тестов пуст.");
    assert!(here[0].verified);
    assert!(
        report
            .dropped
            .iter()
            .all(|d| d.finding.section.as_deref() != Some("Тестирование, строка 10"))
    );
}

#[tokio::test]
async fn a_critic_duplicate_names_the_requirement_that_stays() {
    // F1 (set-level) repeats F0, the confirmed finding of R-02.
    let verdicts = json!([
        verdict("F0", "confirmed", None),
        duplicate("F1", "F0"),
        verdict("F2", "confirmed", None)
    ]);
    let report = report(fake(verdicts)).await;
    let critic: Vec<_> = report
        .dropped
        .iter()
        .filter(|d| d.note.is_some())
        .map(|d| {
            (
                d.reason,
                d.kept_in.as_deref(),
                d.finding.requirement_id.as_deref(),
            )
        })
        .collect();
    assert_eq!(critic, [(DropReason::Duplicate, Some("R-02"), None)]);
}

#[test]
fn an_anchor_is_compared_by_its_position_not_its_heading() {
    assert_eq!(
        place("Тестирование, стр. 2, строка 37"),
        place("стр. 2, строка 37")
    );
    assert_ne!(place("Раздел, строка 5"), place("Раздел, строка 6"));
}

#[tokio::test]
async fn a_placeholder_in_a_typeset_document_matches_despite_the_models_heading() {
    let text = format!(
        "Переводы\n* Ф-1. Клиент переводит до 100 000 ₸ в сутки.\n* Ф-2. Перевод проходит быстро.\
         {PAGE_BREAK}Тестирование требований\nзаполняются тестировщиками\n"
    );
    let mut replies = replies(json!([]));
    replies.insert(
        "set",
        json!({"summary": "Документ не готов.", "findings": [
            {"criterion": "CHK-ROLLBACK", "severity": "major", "section": "Тестирование требований",
             "quote": "заполняются тестировщиками", "problem": "Раздел тестов пуст.",
             "suggestion": "Написать тесты."}
        ], "questions": []}),
    );
    let provider = Arc::new(Fake {
        replies,
        seen: Mutex::default(),
    });
    let report = report_on(&text, provider.clone()).await;
    assert!(!provider.seen.lock().unwrap()["critic"].contains("Раздел тестов пуст."));
    let here: Vec<&Finding> = report
        .findings
        .iter()
        .filter(|f| f.quote.as_deref() == Some("заполняются тестировщиками"))
        .collect();
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].problem, "Раздел тестов пуст.");
    assert!(here[0].verified);
    assert!(report.dropped.iter().all(|d| {
        d.finding.quote.as_deref() != Some("заполняются тестировщиками")
    }));
}
