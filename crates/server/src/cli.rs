//! `anyq-server review <requirements> <report.md>`: one command from a file to
//! a report. It drives the same `ReviewService` the HTTP handlers call, inside
//! this process: no second server, no ports.

use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyq_core::domain::{Answer, Question, Review, ReviewStatus, Spec};
use anyq_core::service::ServiceError;
use anyq_pipeline::agent::{self, AgentError};
use anyq_pipeline::report::{Passport, render};

use crate::config::{Config, ConfigError};
use crate::document::{self, DocumentError};

const STATIC: &str = "pipeline";
const DIALOG: &str = "dialog";
const USAGE: &str = "usage: anyq-server review <requirements.pdf|.md|.txt> <report.md> \
                     [--config anyq.toml] [--json review.json] [--interactive] [--timeout 1800]";

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{USAGE}")]
    Usage,
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("cannot write {0}: {1}")]
    Write(PathBuf, std::io::Error),
    #[error("{0}")]
    Document(#[from] DocumentError),
    #[error("config: {0}")]
    Config(#[from] ConfigError),
    #[error("{0}")]
    Agent(#[from] AgentError),
    #[error("{0}")]
    Service(#[from] ServiceError),
    #[error("a step did not finish in {0} s")]
    Timeout(u64),
    #[error("the review failed: {0}")]
    Failed(String),
    #[error("cannot start the runtime: {0}")]
    Runtime(std::io::Error),
}

struct Args {
    requirements: PathBuf,
    report: PathBuf,
    config: PathBuf,
    json: Option<PathBuf>,
    interactive: bool,
    timeout: u64,
}

fn parse(args: &[String], root: &Path) -> Result<Args, CliError> {
    let mut positional = Vec::new();
    let default_config = if root.join("anyq.toml").exists() {
        "anyq.toml"
    } else {
        "anyq.example.toml"
    };
    let mut parsed = Args {
        requirements: PathBuf::new(),
        report: PathBuf::new(),
        config: root.join(default_config),
        json: None,
        interactive: false,
        timeout: 1800,
    };
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--config" => parsed.config = rest.next().ok_or(CliError::Usage)?.into(),
            "--json" => parsed.json = Some(rest.next().ok_or(CliError::Usage)?.into()),
            "--timeout" => {
                parsed.timeout = rest
                    .next()
                    .and_then(|t| t.parse().ok())
                    .ok_or(CliError::Usage)?;
            }
            "--interactive" => parsed.interactive = true,
            flag if flag.starts_with("--") => return Err(CliError::Usage),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    let [requirements, report] =
        <[PathBuf; 2]>::try_from(positional).map_err(|_| CliError::Usage)?;
    parsed.requirements = requirements;
    parsed.report = report;
    Ok(parsed)
}

fn read_document(path: &Path) -> Result<String, CliError> {
    let bytes = std::fs::read(path).map_err(|e| CliError::Read(path.into(), e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    Ok(document::text_of(&name, &bytes)?)
}

/// `KEY=VALUE` lines; the real environment wins over the file.
pub fn dotenv(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            let value = value.trim().trim_matches(['"', '\'']);
            (!key.starts_with('#') && !key.trim().is_empty() && !value.is_empty())
                .then(|| (key.trim().to_owned(), value.to_owned()))
        })
        .collect()
}

fn agent_version(root: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["describe", "--always", "--dirty", "--tags"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "неизвестна".into())
}

fn ask_author(questions: &[Question]) -> Vec<Answer> {
    eprintln!(
        "\nВопросов от агента: {}. Пустая строка — пропустить.",
        questions.len()
    );
    let mut answers = Vec::new();
    let stdin = std::io::stdin();
    for q in questions {
        let place = q
            .section
            .as_deref()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();
        eprintln!("\n{}{place}: {}\n  зачем: {}", q.id, q.text, q.reason);
        eprint!("> ");
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).is_err() {
            break;
        }
        if !line.trim().is_empty() {
            answers.push(Answer {
                question_id: q.id.clone(),
                text: line.trim().to_owned(),
            });
        }
    }
    answers
}

async fn drive(
    config: Config,
    env: HashMap<String, String>,
    spec: Spec,
    args: &Args,
) -> Result<Review, CliError> {
    let lookup = |name: &str| std::env::var(name).ok().or_else(|| env.get(name).cloned());
    let state = config.into_state(&lookup)?;
    let harness = if args.interactive { DIALOG } else { STATIC };
    let mut review = state.reviews.create(spec, Some(harness.into())).await?;
    eprintln!("проверка {}: идёт", review.id);
    loop {
        review = tokio::time::timeout(
            Duration::from_secs(args.timeout),
            state.reviews.advance(review.id),
        )
        .await
        .map_err(|_| CliError::Timeout(args.timeout))??;
        match review.status {
            ReviewStatus::Done => return Ok(review),
            ReviewStatus::Failed => return Err(CliError::Failed(review.error.unwrap_or_default())),
            ReviewStatus::AwaitingAnswers => {
                let questions = review
                    .rounds
                    .last()
                    .map(|r| r.questions.clone())
                    .unwrap_or_default();
                // A static run never waits for a person: the questions go to the report.
                let answers = if args.interactive {
                    tokio::task::spawn_blocking(move || ask_author(&questions))
                        .await
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                state.reviews.answer(review.id, answers).await?;
            }
            ReviewStatus::Running => {}
        }
    }
}

fn write(path: &Path, text: &str) -> Result<(), CliError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| CliError::Write(dir.into(), e))?;
    }
    std::fs::write(path, text).map_err(|e| CliError::Write(path.into(), e))
}

fn run(args: &[String]) -> Result<PathBuf, CliError> {
    let root = std::env::current_dir().map_err(|e| CliError::Read(".".into(), e))?;
    let args = parse(args, &root)?;
    let text = read_document(&args.requirements)?;
    let env = std::fs::read_to_string(root.join(".env"))
        .map(|t| dotenv(&t))
        .unwrap_or_default();
    let config = Config::load(&args.config.to_string_lossy())?;
    let spec = Spec {
        title: Some(document::title_of(
            &text,
            &args.requirements.to_string_lossy(),
        )),
        file_name: args
            .requirements
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        text,
    };
    let started = chrono::Local::now();

    let runtime = tokio::runtime::Runtime::new().map_err(CliError::Runtime)?;
    let review = runtime.block_on(drive(config, env, spec, &args))?;
    let Some(report) = &review.report else {
        return Err(CliError::Failed("the review ended without a report".into()));
    };

    let passport = Passport {
        document: review.spec.title.clone().unwrap_or_default(),
        file: args
            .requirements
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        date: started.format("%Y-%m-%d %H:%M %:z").to_string(),
        duration: {
            let seconds = (chrono::Local::now() - started).num_seconds().max(0);
            format!("{} мин {:02} с", seconds / 60, seconds % 60)
        },
        agent_version: agent_version(&root),
        harness: review.harness.clone(),
        pipeline_version: agent::agent()?.version,
    };
    write(
        &args.report,
        &render(
            report,
            &passport,
            &agent::criteria()?,
            agent::agent()?.limits.main_questions,
        ),
    )?;
    if let Some(json) = &args.json {
        let dump =
            serde_json::to_string_pretty(&review).map_err(|e| CliError::Failed(e.to_string()))?;
        write(json, &dump)?;
    }
    Ok(args.report)
}

pub fn main(args: &[String]) -> ExitCode {
    match run(args) {
        Ok(report) => {
            eprintln!("отчёт: {}", report.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("anyq-server review: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn arguments_are_parsed_and_mistakes_rejected() {
        let root = Path::new("/nowhere");
        let args = parse(
            &strings(&["in.pdf", "out.md", "--interactive", "--json", "r.json"]),
            root,
        )
        .unwrap();
        assert_eq!(args.requirements, PathBuf::from("in.pdf"));
        assert!(args.interactive && args.json == Some("r.json".into()));
        assert_eq!(args.config, root.join("anyq.example.toml"));
        assert!(parse(&strings(&["in.pdf"]), root).is_err());
        assert!(parse(&strings(&["in.pdf", "out.md", "--timeout", "soon"]), root).is_err());
        assert!(parse(&strings(&["in.pdf", "out.md", "--verbose"]), root).is_err());
    }

    #[test]
    fn dotenv_skips_comments_and_empty_values() {
        let env = dotenv("# comment\nA=1\nB='quoted'\nC=\n");
        assert_eq!(env.get("A").map(String::as_str), Some("1"));
        assert_eq!(env.get("B").map(String::as_str), Some("quoted"));
        assert!(!env.contains_key("C"));
    }
}
