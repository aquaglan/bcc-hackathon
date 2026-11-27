//! Built-in review harness: cut the requirements, check each, check the set,
//! merge repeats, let a critic try to refute the serious findings, report.
//!
//! The order of stages is this code; what the model is told is `agent.toml`;
//! what it checks against is `criteria.toml`. Everything the model points at
//! is located in the text by code (`document`): a quote that is not there drops
//! the requirement or the finding. No I/O here: the model is reached through
//! the gateway port.

pub mod agent;
pub mod document;
mod lint;
pub mod report;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use anyq_core::domain::{
    DropReason, Dropped, EventKind, Finding, LlmRequest, Message, Progress, Question, Report,
    Requirement, Role, Round, Severity, Stage, StepRequest, StepResult,
};
use anyq_core::ports::{Harness, HarnessError, LlmError};
use anyq_core::progress::ProgressBoard;
use anyq_core::service::LlmGateway;
use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use crate::agent::{
    Agent, AgentError, AuthorQuestion, CheckedRequirement, Criteria, FinalQuestions,
    RequirementChecks, Requirements, Scope, SetCheck, Verdict, VerdictKind, Verdicts,
};
use crate::document::Document;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("model call failed: {0}")]
    Llm(#[from] LlmError),
    #[error("the {stage} reply does not match its schema: {error}")]
    Schema {
        stage: &'static str,
        error: serde_json::Error,
    },
}

pub fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Critical => 0,
        Severity::Major => 1,
        Severity::Minor => 2,
    }
}

/// Whether the first step asks the author before reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Questions go into the report; the review never waits for a person.
    Static,
    /// The first step asks; the next one reviews again with the answers.
    Dialog,
}

pub struct Pipeline {
    llm: Arc<LlmGateway>,
    mode: Mode,
    agent: Agent,
    criteria: Criteria,
    progress: Arc<ProgressBoard>,
}

impl Pipeline {
    pub fn new(
        llm: Arc<LlmGateway>,
        mode: Mode,
        progress: Arc<ProgressBoard>,
    ) -> Result<Self, AgentError> {
        Ok(Self {
            llm,
            mode,
            progress,
            agent: agent::agent()?,
            criteria: agent::criteria()?,
        })
    }

    pub fn version(&self) -> &str {
        &self.agent.version
    }

    pub fn criteria(&self) -> &Criteria {
        &self.criteria
    }
}

/// What verification removed: reported, never silently lost.
#[derive(Default)]
struct Drops {
    requirements: usize,
    findings: usize,
    questions: usize,
}

impl Drops {
    fn note(&self, dropped: &[Dropped]) -> String {
        let count = |reason| dropped.iter().filter(|d| d.reason == reason).count();
        let mut parts = Vec::new();
        for (count, what) in [
            (
                self.requirements,
                "требований без найденной в тексте цитаты",
            ),
            (self.findings, "замечаний без найденной в тексте цитаты"),
            (count(DropReason::Duplicate), "повторов одного замечания"),
            (
                count(DropReason::Refuted),
                "замечаний, опровергнутых критиком",
            ),
            (self.questions, "вопросов, слитых с другими"),
        ] {
            if count > 0 {
                parts.push(format!("{what}: {count}"));
            }
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("Отброшено при проверке: {}.", parts.join("; "))
        }
    }
}

/// The document and the author's answers: both are data for the model.
fn context(text: &str, rounds: &[Round]) -> String {
    let mut out = format!("<document>\n{text}\n</document>");
    let answered: Vec<String> = rounds
        .iter()
        .flat_map(|r| {
            r.questions.iter().filter_map(|q| {
                let answer = r.answers.iter().find(|a| a.question_id == q.id)?;
                (!answer.text.trim().is_empty())
                    .then(|| format!("Вопрос: {}\nОтвет автора: {}", q.text, answer.text))
            })
        })
        .collect();
    if !answered.is_empty() {
        out.push_str(&format!(
            "\n\n<answers>\n{}\n</answers>",
            answered.join("\n\n")
        ));
    }
    out
}

fn requirements_block(requirements: &[Requirement]) -> String {
    let lines: Vec<String> = requirements
        .iter()
        .map(|r| {
            let id = r.source_id.as_deref().unwrap_or("без номера");
            format!(
                "- {} [{id}; {}] «{}» — {}",
                r.id, r.section, r.quote, r.summary
            )
        })
        .collect();
    format!("<requirements>\n{}\n</requirements>", lines.join("\n"))
}

/// A finding from either check, before the code anchors it.
struct Draft {
    criterion: String,
    severity: Severity,
    quote: Option<String>,
    section: Option<String>,
    problem: String,
    suggestion: String,
}

/// A finding as a line of the progress feed: short, with its place.
fn feed_line(place: &str, criterion: &str, problem: &str) -> String {
    let problem: String = problem.chars().take(140).collect();
    format!("{place} · {criterion}: {problem}")
}

fn count(progress: &mut Progress, severity: Severity) {
    match severity {
        Severity::Critical => progress.critical += 1,
        Severity::Major => progress.major += 1,
        Severity::Minor => progress.minor += 1,
    }
}

struct Run<'a> {
    pipeline: &'a Pipeline,
    id: Uuid,
    document: Document,
    context: String,
    drops: Drops,
    dropped: Vec<Dropped>,
    models: BTreeSet<String>,
}

/// The page and line of an anchor. In a typeset PDF the model's section hint
/// changes the heading part of the anchor, never the position.
fn place(anchor: &str) -> &str {
    anchor.rfind("стр. ").map_or(anchor, |i| &anchor[i..])
}

impl Run<'_> {
    fn stage(&self, stage: Stage, text: String) {
        let board = &self.pipeline.progress;
        board.update(self.id, |p| p.stage = stage);
        board.event(self.id, EventKind::Stage, None, text);
    }

    async fn ask<T: DeserializeOwned>(
        &self,
        stage: &'static str,
        system: &str,
        user: String,
        schema: Value,
    ) -> Result<(T, String), PipelineError> {
        let request = LlmRequest {
            provider: None,
            model: None,
            system: Some(system.to_owned()),
            messages: vec![Message {
                role: Role::User,
                content: user,
            }],
            max_tokens: None,
            json_schema: Some(schema),
        };
        // One stage failing would throw away minutes of the others' work.
        match self.attempt(stage, &request).await {
            Ok(done) => Ok(done),
            Err(_) => self.attempt(stage, &request).await,
        }
    }

    async fn attempt<T: DeserializeOwned>(
        &self,
        stage: &'static str,
        request: &LlmRequest,
    ) -> Result<(T, String), PipelineError> {
        let reply = self.pipeline.llm.complete(request).await?;
        let output = serde_json::from_str(&reply.text)
            .map_err(|error| PipelineError::Schema { stage, error })?;
        Ok((output, reply.model))
    }

    fn criteria_block(&self, scope: Scope) -> String {
        let lines: Vec<String> = self
            .pipeline
            .criteria
            .of(scope)
            .map(|c| format!("- {} ({}): {}", c.id, c.name, c.check))
            .collect();
        format!("<criteria>\n{}\n</criteria>", lines.join("\n"))
    }

    async fn extract(&mut self) -> Result<Vec<Requirement>, PipelineError> {
        let prompts = &self.pipeline.agent.prompts;
        let (drafts, model) = self
            .ask::<Requirements>(
                "extract",
                &prompts.extract,
                self.context.clone(),
                agent::requirements_schema(),
            )
            .await?;
        self.models.insert(model);
        let mut located = Vec::new();
        for draft in drafts.requirements {
            match self.document.locate(&draft.quote, Some(&draft.section)) {
                Some(location) => {
                    located.push((location.line, location.page, draft, location.anchor()))
                }
                None => self.drops.requirements += 1,
            }
        }
        located.sort_by_key(|(line, page, ..)| (*page, *line));
        Ok(located
            .into_iter()
            .enumerate()
            .map(|(n, (_, _, draft, anchor))| Requirement {
                id: format!("R-{:02}", n + 1),
                source_id: draft.source_id,
                section: anchor,
                quote: draft.quote,
                summary: draft.summary,
            })
            .collect())
    }

    async fn check(
        &self,
        requirements: &[Requirement],
    ) -> Result<(Vec<CheckedRequirement>, SetCheck, Vec<String>), PipelineError> {
        let agent = &self.pipeline.agent;
        let head = format!(
            "{}\n\n{}",
            self.context,
            self.criteria_block(Scope::Requirement)
        );
        let schema = agent::requirement_checks_schema(&self.pipeline.criteria);
        let users: Vec<String> = requirements
            .chunks(agent.limits.batch_size.max(1))
            .map(|batch| format!("{head}\n\n{}", requirements_block(batch)))
            .collect();
        let board = &self.pipeline.progress;
        let batches = async {
            let mut done = Vec::new();
            let mut replies = stream::iter(users)
                .map(|user| {
                    self.ask::<RequirementChecks>(
                        "check",
                        &agent.prompts.check,
                        user,
                        schema.clone(),
                    )
                })
                .buffered(agent.limits.concurrency.max(1));
            while let Some(reply) = replies.next().await {
                if let Ok((checks, _)) = &reply {
                    for item in &checks.requirements {
                        board.update(self.id, |p| p.checked += 1);
                        for f in &item.findings {
                            board.update(self.id, |p| count(p, f.severity));
                            let line = feed_line(&item.requirement_id, &f.criterion, &f.problem);
                            board.event(self.id, EventKind::Found, Some(f.severity), line);
                        }
                    }
                }
                done.push(reply);
            }
            done
        };
        let set_user = [
            self.context.clone(),
            self.criteria_block(Scope::Set),
            requirements_block(requirements),
        ]
        .join("\n\n");
        let set = self.ask::<SetCheck>(
            "set",
            &agent.prompts.set,
            set_user,
            agent::set_check_schema(&self.pipeline.criteria),
        );
        let (batches, set) = futures_util::future::join(batches, set).await;
        let mut models = Vec::new();
        let mut checked = Vec::new();
        for result in batches {
            let (checks, model) = result?;
            models.push(model);
            checked.extend(checks.requirements);
        }
        let (set, model) = set?;
        models.push(model);
        for f in &set.findings {
            board.update(self.id, |p| count(p, f.severity));
            let line = feed_line("набор", &f.criterion, &f.problem);
            board.event(self.id, EventKind::Found, Some(f.severity), line);
        }
        Ok((checked, set, models))
    }

    /// Placeholders and empty sections found by code. They join the report after
    /// the critic: there is nothing to refute. A place the model already
    /// reported is not reported twice.
    fn add_lint_findings(&self, hits: &[lint::Hit], kept: &mut Vec<Finding>) {
        let board = &self.pipeline.progress;
        for hit in hits {
            let section = self.document.at_line(hit.line, None).anchor();
            let at_place = |f: &Finding| f.section.as_deref().map(place) == Some(place(&section));
            if kept.iter().any(at_place) {
                continue;
            }
            board.update(self.id, |p| count(p, Severity::Major));
            let line = feed_line(&section, "SET-COMPLETE", hit.problem());
            board.event(self.id, EventKind::Found, Some(Severity::Major), line);
            kept.push(Finding {
                requirement_id: None,
                severity: Severity::Major,
                category: "SET-COMPLETE".into(),
                section: Some(section),
                quote: Some(hit.quote.clone()),
                problem: hit.problem().into(),
                suggestion: lint::SUGGESTION.into(),
                verified: true,
                critic_note: None,
            });
        }
    }

    /// Anchors a finding in the text, or drops it when its quote is not there.
    fn finding(&mut self, requirement: Option<&Requirement>, draft: Draft) -> Option<Finding> {
        let Draft {
            criterion,
            severity,
            quote,
            section,
            problem,
            suggestion,
        } = draft;
        let section = match &quote {
            Some(quote) => {
                let near = section
                    .as_deref()
                    .or(requirement.map(|r| r.section.as_str()));
                match self.document.locate(quote, near) {
                    Some(location) => Some(location.anchor()),
                    None => {
                        self.drops.findings += 1;
                        return None;
                    }
                }
            }
            None => requirement.map(|r| r.section.clone()).or(section),
        };
        Some(Finding {
            requirement_id: requirement.map(|r| r.id.clone()),
            severity,
            category: criterion,
            section,
            quote,
            problem,
            suggestion,
            verified: false,
            critic_note: None,
        })
    }

    /// One defect seen from two requirements, or from a requirement and the
    /// set, is one finding: same place, same criterion. The first one stays.
    /// A finding about missing text has no place of its own, so two of them
    /// under one criterion are different defects: the critic sorts their repeats.
    fn merge_duplicates(&mut self, findings: Vec<Finding>) -> Vec<Finding> {
        let mut seen: HashMap<(Option<String>, String), Option<String>> = HashMap::new();
        let mut kept = Vec::new();
        for finding in findings {
            if finding.quote.is_some() {
                let key = (finding.section.clone(), finding.category.clone());
                if let Some(first) = seen.get(&key) {
                    self.dropped.push(Dropped {
                        finding,
                        reason: DropReason::Duplicate,
                        kept_in: first.clone(),
                        note: None,
                    });
                    continue;
                }
                seen.insert(key, finding.requirement_id.clone());
            }
            kept.push(finding);
        }
        kept
    }

    /// Verdicts by index into `findings`, for the severities the critic covers.
    async fn criticise(
        &self,
        findings: &[Finding],
    ) -> Result<(HashMap<usize, Verdict>, Option<String>), PipelineError> {
        let covered = &self.pipeline.agent.limits.critic_severities;
        let picked: Vec<(String, usize)> = findings
            .iter()
            .enumerate()
            .filter(|(_, f)| covered.contains(&f.severity) && !f.verified)
            .map(|(i, _)| (format!("F{i}"), i))
            .collect();
        if picked.is_empty() {
            return Ok((HashMap::new(), None));
        }
        let lines: Vec<String> = picked
            .iter()
            .map(|(id, i)| {
                let f = &findings[*i];
                format!(
                    "- {id} [{}; {:?}; {}] цитата: {:?}; {}",
                    f.category,
                    f.severity,
                    f.section.as_deref().unwrap_or("—"),
                    f.quote.as_deref().unwrap_or("нет"),
                    f.problem
                )
            })
            .collect();
        let user = format!(
            "{}\n\n<findings>\n{}\n</findings>",
            self.context,
            lines.join("\n")
        );
        let prompts = &self.pipeline.agent.prompts;
        let (verdicts, model) = self
            .ask::<Verdicts>("critic", &prompts.critic, user, agent::verdicts_schema())
            .await?;
        let index: HashMap<&str, usize> = picked.iter().map(|(id, i)| (id.as_str(), *i)).collect();
        let by_finding = verdicts
            .verdicts
            .into_iter()
            .filter_map(|v| index.get(v.finding_id.as_str()).map(|&i| (i, v)))
            .collect();
        Ok((by_finding, Some(model)))
    }
}

impl Run<'_> {
    /// The questions to put to the author: repeats merged, most important first.
    /// Each keeps the place of its first source that has one; a source the model
    /// left out is kept at the end, so no question is lost.
    async fn final_questions(
        &mut self,
        questions: Vec<(Option<String>, AuthorQuestion)>,
    ) -> Result<Vec<(Option<String>, AuthorQuestion)>, PipelineError> {
        if questions.len() < 2 {
            return Ok(questions);
        }
        let lines: Vec<String> = questions
            .iter()
            .enumerate()
            .map(|(i, (section, q))| {
                let place = section.as_deref().unwrap_or("место не указано");
                format!("- Q{i} [{place}] {} (зачем: {})", q.text, q.reason)
            })
            .collect();
        let user = format!("<questions>\n{}\n</questions>", lines.join("\n"));
        let prompts = &self.pipeline.agent.prompts;
        let (reply, model) = self
            .ask::<FinalQuestions>(
                "questions",
                &prompts.questions,
                user,
                agent::final_questions_schema(),
            )
            .await?;
        self.models.insert(model);
        let index = |id: &str| {
            id.strip_prefix('Q')?
                .parse::<usize>()
                .ok()
                .filter(|&i| i < questions.len())
        };
        let mut covered = HashSet::new();
        let mut kept = Vec::new();
        for q in reply.final_questions {
            let sources: Vec<usize> = q.from.iter().filter_map(|id| index(id)).collect();
            // A question with no real source was invented by this stage.
            if sources.is_empty() {
                continue;
            }
            covered.extend(sources.iter().copied());
            let section = sources.iter().find_map(|&i| questions[i].0.clone());
            kept.push((
                section,
                AuthorQuestion {
                    text: q.text,
                    reason: q.reason,
                },
            ));
        }
        self.drops.questions += covered.len().saturating_sub(kept.len());
        kept.extend(
            questions
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !covered.contains(i))
                .map(|(_, q)| q),
        );
        Ok(kept)
    }
}

impl Pipeline {
    async fn run(&self, request: &StepRequest) -> Result<StepResult, PipelineError> {
        self.progress.start(request.review_id);
        let mut run = Run {
            pipeline: self,
            id: request.review_id,
            document: Document::new(&request.spec.text),
            context: context(&request.spec.text, &request.rounds),
            drops: Drops::default(),
            dropped: Vec::new(),
            models: BTreeSet::new(),
        };

        let requirements = run.extract().await?;
        self.progress
            .update(run.id, |p| p.requirements = requirements.len());
        run.stage(
            Stage::Check,
            format!("Нарезано требований: {}", requirements.len()),
        );
        let (checked, set_check, models) = run.check(&requirements).await?;
        run.models.extend(models);
        let by_id: HashMap<&str, &Requirement> =
            requirements.iter().map(|r| (r.id.as_str(), r)).collect();

        let mut findings = Vec::new();
        let mut questions: Vec<(Option<String>, AuthorQuestion)> = Vec::new();
        for item in checked {
            let Some(requirement) = by_id.get(item.requirement_id.as_str()).copied() else {
                run.drops.findings += item.findings.len();
                continue;
            };
            for f in item.findings {
                let draft = Draft {
                    criterion: f.criterion,
                    severity: f.severity,
                    quote: f.quote,
                    section: None,
                    problem: f.problem,
                    suggestion: f.suggestion,
                };
                findings.extend(run.finding(Some(requirement), draft));
            }
            questions.extend(
                item.questions
                    .into_iter()
                    .map(|q| (Some(requirement.section.clone()), q)),
            );
        }
        for f in set_check.findings {
            let draft = Draft {
                criterion: f.criterion,
                severity: f.severity,
                quote: f.quote,
                section: f.section,
                problem: f.problem,
                suggestion: f.suggestion,
            };
            findings.extend(run.finding(None, draft));
        }
        questions.extend(set_check.questions.into_iter().map(|q| (None, q)));

        let mut findings = run.merge_duplicates(findings);
        // Code decides these places before the critic: a placeholder is a defect
        // whoever is meant to fill it in later, so the model's finding is confirmed.
        let lint_hits = lint::find(&run.document, &self.agent.lint);
        let lint_places: HashSet<String> = lint_hits
            .iter()
            .map(|h| place(&run.document.at_line(h.line, None).anchor()).to_owned())
            .collect();
        for f in &mut findings {
            if f.section
                .as_deref()
                .is_some_and(|s| lint_places.contains(place(s)))
            {
                f.verified = true;
            }
        }
        run.stage(
            Stage::Critic,
            format!("Критик перепроверяет замечания: {}", findings.len()),
        );
        let (verdicts, critic_model) = run.criticise(&findings).await?;
        run.models.extend(critic_model);
        // A repeat goes only when the finding it repeats stays in the report.
        let kept_one = |index: usize, v: &Verdict| {
            let target = v
                .duplicate_of
                .as_deref()?
                .strip_prefix('F')?
                .parse::<usize>()
                .ok()?;
            let stays = verdicts
                .get(&target)
                .is_some_and(|t| t.verdict == VerdictKind::Confirmed);
            (target != index && stays).then_some(target)
        };
        let requirement_of: Vec<Option<String>> =
            findings.iter().map(|f| f.requirement_id.clone()).collect();
        let mut kept = Vec::new();
        for (index, mut finding) in findings.into_iter().enumerate() {
            match verdicts.get(&index) {
                None => kept.push(finding),
                Some(v) if v.verdict == VerdictKind::Duplicate && kept_one(index, v).is_some() => {
                    let target = kept_one(index, v).and_then(|t| requirement_of[t].clone());
                    run.dropped.push(Dropped {
                        finding,
                        reason: DropReason::Duplicate,
                        kept_in: target,
                        note: Some(v.reason.clone()),
                    });
                }
                Some(v) if matches!(v.verdict, VerdictKind::Confirmed | VerdictKind::Duplicate) => {
                    finding.verified = true;
                    finding.critic_note = Some(v.reason.clone());
                    kept.push(finding);
                }
                Some(v) if v.verdict == VerdictKind::Refuted => run.dropped.push(Dropped {
                    finding,
                    reason: DropReason::Refuted,
                    kept_in: None,
                    note: Some(v.reason.clone()),
                }),
                Some(v) => questions.push((
                    finding.section.clone(),
                    AuthorQuestion {
                        text: v
                            .question
                            .clone()
                            .unwrap_or_else(|| finding.problem.clone()),
                        reason: finding.problem.clone(),
                    },
                )),
            }
        }

        run.add_lint_findings(&lint_hits, &mut kept);

        let refuted = run
            .dropped
            .iter()
            .filter(|d| d.reason == DropReason::Refuted)
            .count();
        if refuted > 0 {
            let text = format!("Критик опроверг и убрал: {refuted}");
            self.progress.event(run.id, EventKind::Dropped, None, text);
        }
        run.stage(
            Stage::Questions,
            format!("Собираю вопросы автору: {}", questions.len()),
        );
        let questions = run.final_questions(questions).await?;
        run.stage(Stage::Done, "Готово".into());
        let order: HashMap<&str, usize> = requirements
            .iter()
            .enumerate()
            .map(|(n, r)| (r.id.as_str(), n))
            .collect();
        kept.sort_by_key(|f| {
            let place = f
                .requirement_id
                .as_deref()
                .and_then(|id| order.get(id))
                .copied();
            (severity_rank(f.severity), place.unwrap_or(order.len()))
        });
        let round = request.rounds.len() + 1;
        let questions: Vec<Question> = questions
            .into_iter()
            .enumerate()
            .map(|(n, (section, q))| Question {
                id: format!("r{round}q{}", n + 1),
                text: q.text,
                reason: q.reason,
                section,
            })
            .collect();
        if self.mode == Mode::Dialog && request.rounds.is_empty() && !questions.is_empty() {
            let main = self.agent.limits.main_questions;
            return Ok(StepResult::Ask {
                questions: questions.into_iter().take(main).collect(),
            });
        }
        let note = run.drops.note(&run.dropped);
        Ok(StepResult::Done {
            report: Report {
                summary: format!("{} {note}", set_check.summary).trim().to_owned(),
                requirements,
                findings: kept,
                open_questions: questions,
                models: run.models.into_iter().collect(),
                dropped: run.dropped,
            },
        })
    }
}

#[async_trait]
impl Harness for Pipeline {
    async fn step(&self, req: &StepRequest) -> Result<StepResult, HarnessError> {
        self.run(req).await.map_err(|e| HarnessError(e.to_string()))
    }
}

#[cfg(test)]
mod tests;
