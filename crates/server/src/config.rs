//! `anyq.toml`: which providers and harnesses exist. Secrets are never in
//! the file, only the names of the environment variables that hold them.

use std::collections::HashMap;
use std::sync::Arc;

use anyq_adapters::harness::HttpHarness;
use anyq_adapters::llm::anthropic::{Anthropic, AnthropicConfig};
use anyq_adapters::llm::openai_compat::{OpenAiCompat, OpenAiCompatConfig};
use anyq_core::memory::MemoryStore;
use anyq_core::ports::{Harness, LlmProvider};
use anyq_core::progress::ProgressBoard;
use anyq_core::service::{LlmGateway, ReviewService};
use anyq_pipeline::agent::AgentError;
use anyq_pipeline::{Mode, Pipeline};
use serde::Deserialize;

use crate::api::AppState;

/// When set, every `/v1` request must carry it as a bearer token.
const API_TOKEN_ENV: &str = "ANYQ_API_TOKEN";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {0}: {1}")]
    Read(String, std::io::Error),
    #[error("invalid config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("environment variable {0} is not set")]
    MissingEnv(String),
    #[error("default_{0} `{1}` is not configured")]
    UnknownDefault(&'static str, String),
    #[error("cannot build an HTTP client: {0}")]
    Http(String),
    #[error("built-in pipeline: {0}")]
    Agent(#[from] AgentError),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    pub default_provider: String,
    pub default_harness: String,
    pub providers: HashMap<String, ProviderConfig>,
    pub harnesses: HashMap<String, HarnessConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
    Anthropic {
        model: String,
        #[serde(default = "default_anthropic_key_env")]
        api_key_env: String,
        #[serde(default = "default_anthropic_url")]
        base_url: String,
        effort: Option<String>,
        #[serde(default)]
        fallbacks: bool,
    },
    OpenaiCompat {
        base_url: String,
        model: String,
        /// On-premise servers usually need no key.
        api_key_env: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HarnessConfig {
    /// A separate service that answers `POST url` with a step result.
    Http { url: String },
    /// The built-in Rust pipeline; `ask_first` asks the author one round of
    /// questions before the report.
    Pipeline {
        #[serde(default)]
        ask_first: bool,
    },
}

// Loopback by default: the gateway spends the provider's money.
fn default_listen() -> String {
    "127.0.0.1:8080".into()
}

fn default_anthropic_key_env() -> String {
    "ANTHROPIC_API_KEY".into()
}

fn default_anthropic_url() -> String {
    "https://api.anthropic.com".into()
}

fn http_error(e: impl std::fmt::Display) -> ConfigError {
    ConfigError::Http(e.to_string())
}

/// Where secrets come from: the process environment, or a `.env` file for the CLI.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

fn required(env: Env, name: &str) -> Result<String, ConfigError> {
    env(name).ok_or_else(|| ConfigError::MissingEnv(name.to_owned()))
}

impl Config {
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Read(path.to_owned(), e))?;
        Ok(toml::from_str(&text)?)
    }

    /// Builds every configured adapter. A missing key fails here, at startup,
    /// not on the first request.
    pub fn into_state(self, env: Env) -> Result<AppState, ConfigError> {
        if !self.providers.contains_key(&self.default_provider) {
            return Err(ConfigError::UnknownDefault(
                "provider",
                self.default_provider,
            ));
        }
        if !self.harnesses.contains_key(&self.default_harness) {
            return Err(ConfigError::UnknownDefault("harness", self.default_harness));
        }

        let mut providers: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
        for (name, provider) in self.providers {
            let built: Arc<dyn LlmProvider> = match provider {
                ProviderConfig::Anthropic {
                    model,
                    api_key_env,
                    base_url,
                    effort,
                    fallbacks,
                } => Arc::new(
                    Anthropic::new(AnthropicConfig {
                        api_key: required(env, &api_key_env)?,
                        model,
                        base_url,
                        effort,
                        fallbacks,
                    })
                    .map_err(http_error)?,
                ),
                ProviderConfig::OpenaiCompat {
                    base_url,
                    model,
                    api_key_env,
                } => Arc::new(
                    OpenAiCompat::new(OpenAiCompatConfig {
                        base_url,
                        model,
                        api_key: api_key_env
                            .as_deref()
                            .map(|name| required(env, name))
                            .transpose()?,
                    })
                    .map_err(http_error)?,
                ),
            };
            providers.insert(name, built);
        }

        let llm = Arc::new(LlmGateway::new(providers, self.default_provider));
        let progress = Arc::new(ProgressBoard::default());
        let mut harnesses: HashMap<String, Arc<dyn Harness>> = HashMap::new();
        for (name, harness) in self.harnesses {
            let built: Arc<dyn Harness> = match harness {
                HarnessConfig::Http { url } => Arc::new(HttpHarness::new(url).map_err(http_error)?),
                HarnessConfig::Pipeline { ask_first } => {
                    let mode = if ask_first {
                        Mode::Dialog
                    } else {
                        Mode::Static
                    };
                    Arc::new(Pipeline::new(llm.clone(), mode, progress.clone())?)
                }
            };
            harnesses.insert(name, built);
        }

        Ok(AppState {
            reviews: Arc::new(ReviewService::new(
                Arc::new(MemoryStore::default()),
                harnesses,
                self.default_harness,
            )),
            llm,
            progress,
            api_token: env(API_TOKEN_ENV).map(Arc::from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_is_valid() {
        let config: Config = toml::from_str(include_str!("../../../anyq.example.toml")).unwrap();
        assert!(config.providers.contains_key(&config.default_provider));
        assert!(config.harnesses.contains_key(&config.default_harness));
    }

    #[test]
    fn the_built_in_pipeline_starts_from_the_example_config() {
        let config: Config = toml::from_str(include_str!("../../../anyq.example.toml")).unwrap();
        let state = config.into_state(&|_| Some("key".into())).unwrap();
        drop(state);
    }

    #[test]
    fn typos_are_rejected() {
        let text = r#"
            default_provider = "local"
            default_harness = "baseline"
            [providers.local]
            kind = "openai_compat"
            base_url = "http://127.0.0.1:11434/v1"
            modle = "qwen"
            [harnesses.baseline]
            kind = "http"
            url = "http://127.0.0.1:8090"
        "#;
        assert!(toml::from_str::<Config>(text).is_err());
    }
}
