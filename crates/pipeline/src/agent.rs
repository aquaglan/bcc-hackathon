//! The agent as data: prompts and limits (`agent.toml`), criteria
//! (`criteria.toml`), and the shape of every reply the model gives.

use anyq_core::domain::Severity;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("agent.toml: {0}")]
    Agent(toml::de::Error),
    #[error("criteria.toml: {0}")]
    Criteria(toml::de::Error),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub version: String,
    pub limits: Limits,
    pub lint: Lint,
    pub prompts: Prompts,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lint {
    /// Lowercase substrings that mark a short line as a placeholder.
    pub placeholder_markers: Vec<String>,
    /// A longer line is a sentence, not a placeholder.
    pub placeholder_max_chars: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// How many requirements one model call checks.
    pub batch_size: usize,
    /// How many model calls run at once.
    pub concurrency: usize,
    /// Findings of these severities go to the critic; the rest stay unverified.
    pub critic_severities: Vec<Severity>,
    /// How many of the questions, most important first, are the main ones: the
    /// first dialog round asks them, the report sets them apart from the rest.
    pub main_questions: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prompts {
    pub extract: String,
    pub check: String,
    pub set: String,
    pub critic: String,
    pub questions: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Requirement,
    Set,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub scope: Scope,
    pub name: String,
    pub source: String,
    pub check: String,
    /// A withdrawn criterion keeps its id in the file but is no longer checked.
    #[serde(default)]
    pub withdrawn: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criteria {
    pub version: String,
    #[serde(rename = "criterion")]
    all: Vec<Criterion>,
}

impl Criteria {
    pub fn active(&self) -> impl Iterator<Item = &Criterion> {
        self.all.iter().filter(|c| !c.withdrawn)
    }

    pub fn of(&self, scope: Scope) -> impl Iterator<Item = &Criterion> {
        self.active().filter(move |c| c.scope == scope)
    }

    pub fn name(&self, id: &str) -> Option<&str> {
        self.active().find(|c| c.id == id).map(|c| c.name.as_str())
    }
}

pub fn agent() -> Result<Agent, AgentError> {
    toml::from_str(include_str!("../agent.toml")).map_err(AgentError::Agent)
}

pub fn criteria() -> Result<Criteria, AgentError> {
    toml::from_str(include_str!("../criteria.toml")).map_err(AgentError::Criteria)
}

// --- What the model fills: parsed strictly, the schemas below must match. ---

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementDraft {
    pub source_id: Option<String>,
    pub section: String,
    pub quote: String,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    pub requirements: Vec<RequirementDraft>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorQuestion {
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementFinding {
    pub criterion: String,
    pub severity: Severity,
    pub quote: Option<String>,
    pub problem: String,
    pub suggestion: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckedRequirement {
    pub requirement_id: String,
    pub findings: Vec<RequirementFinding>,
    pub questions: Vec<AuthorQuestion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementChecks {
    pub requirements: Vec<CheckedRequirement>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetFinding {
    pub criterion: String,
    pub severity: Severity,
    pub section: Option<String>,
    pub quote: Option<String>,
    pub problem: String,
    pub suggestion: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetCheck {
    pub summary: String,
    pub findings: Vec<SetFinding>,
    pub questions: Vec<AuthorQuestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictKind {
    Confirmed,
    Refuted,
    Uncertain,
    Duplicate,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub finding_id: String,
    pub verdict: VerdictKind,
    pub reason: String,
    pub question: Option<String>,
    pub duplicate_of: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdicts {
    pub verdicts: Vec<Verdict>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalQuestion {
    pub text: String,
    pub reason: String,
    pub from: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalQuestions {
    pub final_questions: Vec<FinalQuestion>,
}

// --- JSON schemas for structured output: closed objects, every field required. ---

fn object(fields: &[(&str, Value)]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        "properties": properties,
    })
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description})
}

fn nullable(description: &str) -> Value {
    json!({"anyOf": [{"type": "string"}, {"type": "null"}], "description": description})
}

fn list(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

fn one_of(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn severity() -> Value {
    one_of(&["critical", "major", "minor"])
}

fn question() -> Value {
    object(&[
        (
            "text",
            text("Вопрос автору, на который можно ответить одной-двумя фразами"),
        ),
        ("reason", text("Что изменится в требованиях от ответа")),
    ])
}

pub fn requirements_schema() -> Value {
    object(&[(
        "requirements",
        list(object(&[
            (
                "source_id",
                nullable("Номер требования в документе, если он есть"),
            ),
            ("section", text("Раздел документа")),
            (
                "quote",
                text("Фрагмент, скопированный из документа символ в символ"),
            ),
            ("summary", text("Суть требования одной фразой")),
        ])),
    )])
}

pub fn requirement_checks_schema(criteria: &Criteria) -> Value {
    let ids: Vec<&str> = criteria
        .of(Scope::Requirement)
        .map(|c| c.id.as_str())
        .collect();
    object(&[(
        "requirements",
        list(object(&[
            ("requirement_id", text("Идентификатор R-.. из списка")),
            (
                "findings",
                list(object(&[
                    ("criterion", one_of(&ids)),
                    ("severity", severity()),
                    (
                        "quote",
                        nullable("Фрагмент из документа символ в символ или null"),
                    ),
                    ("problem", text("В чём нарушение")),
                    ("suggestion", text("Как переписать текст документа")),
                ])),
            ),
            ("questions", list(question())),
        ])),
    )])
}

pub fn set_check_schema(criteria: &Criteria) -> Value {
    let ids: Vec<&str> = criteria.of(Scope::Set).map(|c| c.id.as_str()).collect();
    object(&[
        ("summary", text("Общая оценка документа в 2–4 предложениях")),
        (
            "findings",
            list(object(&[
                ("criterion", one_of(&ids)),
                ("severity", severity()),
                (
                    "section",
                    nullable("Раздел документа, к которому относится проблема"),
                ),
                (
                    "quote",
                    nullable("Фрагмент из документа символ в символ или null"),
                ),
                (
                    "problem",
                    text("Проблема; для противоречия — обе стороны и R-.. из списка"),
                ),
                ("suggestion", text("Как исправить документ")),
            ])),
        ),
        ("questions", list(question())),
    ])
}

pub fn verdicts_schema() -> Value {
    object(&[(
        "verdicts",
        list(object(&[
            ("finding_id", text("Идентификатор F.. из списка")),
            (
                "verdict",
                one_of(&["confirmed", "refuted", "uncertain", "duplicate"]),
            ),
            ("reason", text("Чем подтверждается или опровергается")),
            ("question", nullable("Вопрос автору, только при uncertain")),
            (
                "duplicate_of",
                nullable("Какое замечание F.. оставить, только при duplicate"),
            ),
        ])),
    )])
}

pub fn final_questions_schema() -> Value {
    object(&[(
        "final_questions",
        list(object(&[
            ("from", list(text("Исходный вопрос Q.."))),
            ("text", text("Вопрос автору")),
            ("reason", text("Что изменится в требованиях от ответа")),
        ])),
    )])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn closed_and_required(node: &Value) {
        match node {
            Value::Object(map) => {
                if map.get("type") == Some(&json!("object")) {
                    assert_eq!(
                        map.get("additionalProperties"),
                        Some(&json!(false)),
                        "{node}"
                    );
                    let required: Vec<&Value> =
                        map["required"].as_array().unwrap().iter().collect();
                    let properties = map["properties"].as_object().unwrap();
                    assert_eq!(required.len(), properties.len(), "{node}");
                }
                map.values().for_each(closed_and_required);
            }
            Value::Array(items) => items.iter().for_each(closed_and_required),
            _ => {}
        }
    }

    #[test]
    fn data_files_parse_and_criteria_ids_are_unique() {
        let agent = agent().unwrap();
        assert!(agent.limits.batch_size > 0 && agent.limits.concurrency > 0);
        let criteria = criteria().unwrap();
        let mut ids: Vec<&str> = criteria.active().map(|c| c.id.as_str()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total);
        assert!(criteria.of(Scope::Requirement).count() > 0 && criteria.of(Scope::Set).count() > 0);
    }

    #[test]
    fn schemas_fit_structured_outputs() {
        let criteria = criteria().unwrap();
        for schema in [
            requirements_schema(),
            requirement_checks_schema(&criteria),
            set_check_schema(&criteria),
            verdicts_schema(),
        ] {
            closed_and_required(&schema);
        }
    }

    #[test]
    fn fields_keep_the_order_the_model_should_think_in() {
        let schema = requirement_checks_schema(&criteria().unwrap());
        let item = &schema["properties"]["requirements"]["items"]["properties"];
        let keys: Vec<&String> = item.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["requirement_id", "findings", "questions"]);
        let verdict = &verdicts_schema()["properties"]["verdicts"]["items"]["properties"];
        let keys: Vec<&String> = verdict.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "finding_id",
                "verdict",
                "reason",
                "question",
                "duplicate_of"
            ]
        );
    }

    #[test]
    fn a_reply_with_an_unknown_field_is_rejected() {
        let reply = r#"{"verdicts": [{"finding_id": "F0", "verdict": "confirmed", "reason": "", "question": null, "duplicate_of": null, "extra": 1}]}"#;
        assert!(serde_json::from_str::<Verdicts>(reply).is_err());
    }
}
