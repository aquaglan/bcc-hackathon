//! Any server with the OpenAI `/chat/completions` API. This is the on-premise
//! path: Ollama, vLLM, llama.cpp and LM Studio all expose it.

use anyq_core::domain::{LlmRequest, LlmResponse, Usage};
use anyq_core::ports::{LlmError, LlmProvider};
use async_trait::async_trait;
use serde_json::{Value, json};

use super::{http_client, post_json};

pub struct OpenAiCompatConfig {
    /// Up to and including the version segment, e.g. `http://127.0.0.1:11434/v1`.
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

pub struct OpenAiCompat {
    http: reqwest::Client,
    config: OpenAiCompatConfig,
}

impl OpenAiCompat {
    pub fn new(config: OpenAiCompatConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: http_client()?,
            config,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompat {
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let mut request = self.http.post(url).json(&build_body(&self.config, req));
        if let Some(key) = &self.config.api_key {
            request = request.bearer_auth(key);
        }
        parse(&post_json(request).await?)
    }
}

fn build_body(config: &OpenAiCompatConfig, req: &LlmRequest) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = &req.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.extend(req.messages.iter().map(|m| json!(m)));
    let mut body = json!({
        "model": req.model.as_deref().unwrap_or(&config.model),
        "messages": messages,
    });
    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(schema) = &req.json_schema {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {"name": "output", "strict": true, "schema": schema},
        });
    }
    body
}

fn parse(body: &Value) -> Result<LlmResponse, LlmError> {
    let choice = &body["choices"][0];
    match choice["finish_reason"].as_str() {
        Some("length") => return Err(LlmError::Truncated),
        Some("content_filter") => return Err(LlmError::Refused("content filter".into())),
        _ => {}
    }
    let text = choice["message"]["content"]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| LlmError::BadResponse("no text in the reply".into()))?;
    Ok(LlmResponse {
        text: text.to_owned(),
        model: body["model"].as_str().unwrap_or_default().to_owned(),
        usage: Usage {
            input_tokens: body["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: body["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        },
    })
}

#[cfg(test)]
mod tests {
    use anyq_core::domain::{Message, Role};

    use super::*;

    #[test]
    fn body_puts_system_first_and_requests_strict_json() {
        let config = OpenAiCompatConfig {
            base_url: "http://localhost/v1".into(),
            model: "qwen".into(),
            api_key: None,
        };
        let schema = json!({"type": "object"});
        let req = LlmRequest {
            provider: None,
            model: Some("other".into()),
            system: Some("sys".into()),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            max_tokens: Some(100),
            json_schema: Some(schema.clone()),
        };
        assert_eq!(
            build_body(&config, &req),
            json!({
                "model": "other",
                "messages": [{"role": "system", "content": "sys"}, {"role": "user", "content": "hi"}],
                "max_tokens": 100,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {"name": "output", "strict": true, "schema": schema},
                },
            })
        );
    }

    #[test]
    fn reply_is_parsed_and_truncation_is_an_error() {
        let reply = json!({
            "model": "qwen",
            "choices": [{"finish_reason": "stop", "message": {"content": "{}"}}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 2},
        });
        let parsed = parse(&reply).unwrap();
        assert_eq!(parsed.text, "{}");
        assert_eq!(
            parsed.usage,
            Usage {
                input_tokens: 5,
                output_tokens: 2
            }
        );

        let cut = json!({"choices": [{"finish_reason": "length", "message": {"content": "{"}}]});
        assert!(matches!(parse(&cut), Err(LlmError::Truncated)));
        assert!(matches!(parse(&json!({})), Err(LlmError::BadResponse(_))));
    }
}
