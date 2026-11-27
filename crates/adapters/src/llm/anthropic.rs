//! Claude API over raw HTTP: there is no official Anthropic SDK for Rust.

use anyq_core::domain::{LlmRequest, LlmResponse, Usage};
use anyq_core::ports::{LlmError, LlmProvider};
use async_trait::async_trait;
use serde_json::{Value, json};

use super::{http_client, post};

const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const DEFAULT_MAX_TOKENS: u32 = 16_000;

pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    /// `output_config.effort`; the model's own default when `None`.
    pub effort: Option<String>,
    /// Re-run a request the safety classifiers declined on Anthropic's
    /// recommended fallback model. Not every model accepts the parameter.
    pub fallbacks: bool,
}

pub struct Anthropic {
    http: reqwest::Client,
    config: AnthropicConfig,
}

impl Anthropic {
    pub fn new(config: AnthropicConfig) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: http_client()?,
            config,
        })
    }
}

#[async_trait]
impl LlmProvider for Anthropic {
    async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let mut request = self
            .http
            .post(format!("{}/v1/messages", self.config.base_url))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&build_body(&self.config, req));
        if self.config.fallbacks {
            request = request.header("anthropic-beta", FALLBACK_BETA);
        }
        parse(&fold_stream(&post(request).await?)?)
    }
}

// No `thinking` and no sampling parameters: current models think adaptively by
// default and reject `temperature` on this API version.
fn build_body(config: &AnthropicConfig, req: &LlmRequest) -> Value {
    let mut body = json!({
        "model": req.model.as_deref().unwrap_or(&config.model),
        "max_tokens": req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": req.messages,
        // Streamed so the connection carries data while the model works: a
        // silent call of several minutes was dropped by the network.
        "stream": true,
    });
    if let Some(system) = &req.system {
        body["system"] = json!(system);
    }
    let mut output_config = serde_json::Map::new();
    if let Some(effort) = &config.effort {
        output_config.insert("effort".into(), json!(effort));
    }
    if let Some(schema) = &req.json_schema {
        output_config.insert(
            "format".into(),
            json!({"type": "json_schema", "schema": schema}),
        );
    }
    if !output_config.is_empty() {
        body["output_config"] = Value::Object(output_config);
    }
    if config.fallbacks {
        body["fallbacks"] = json!("default");
    }
    body
}

/// Server-sent events folded into the message a non-streamed call returns.
fn fold_stream(body: &str) -> Result<Value, LlmError> {
    let mut message = Value::Null;
    let mut finished = false;
    for data in body.lines().filter_map(|l| l.strip_prefix("data:")) {
        let event: Value =
            serde_json::from_str(data.trim()).map_err(|e| LlmError::BadResponse(e.to_string()))?;
        match event["type"].as_str() {
            Some("message_start") => message = event["message"].clone(),
            Some("content_block_start") => {
                if let Some(content) = message["content"].as_array_mut() {
                    content.push(event["content_block"].clone());
                }
            }
            Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                let index = event["index"].as_u64().unwrap_or(0) as usize;
                let block = &mut message["content"][index];
                let text = format!(
                    "{}{}",
                    block["text"].as_str().unwrap_or_default(),
                    event["delta"]["text"].as_str().unwrap_or_default()
                );
                block["text"] = json!(text);
            }
            Some("message_delta") => {
                for (key, value) in event["delta"].as_object().into_iter().flatten() {
                    message[key] = value.clone();
                }
                // Usage in message_delta is cumulative: its counts replace the start's.
                for (key, value) in event["usage"].as_object().into_iter().flatten() {
                    message["usage"][key] = value.clone();
                }
            }
            Some("message_stop") => finished = true,
            Some("error") => {
                return Err(LlmError::BadResponse(format!(
                    "stream error: {}",
                    event["error"]
                )));
            }
            _ => {}
        }
    }
    if !finished {
        return Err(LlmError::BadResponse(
            "the stream ended before message_stop".into(),
        ));
    }
    Ok(message)
}

fn parse(body: &Value) -> Result<LlmResponse, LlmError> {
    match body["stop_reason"].as_str() {
        Some("refusal") => {
            let why = body["stop_details"]["explanation"]
                .as_str()
                .or(body["stop_details"]["category"].as_str())
                .unwrap_or("no details");
            return Err(LlmError::Refused(why.to_owned()));
        }
        Some("max_tokens") => return Err(LlmError::Truncated),
        _ => {}
    }
    let text: String = body["content"]
        .as_array()
        .ok_or_else(|| LlmError::BadResponse("no `content` array".into()))?
        .iter()
        .filter(|block| block["type"] == "text")
        .filter_map(|block| block["text"].as_str())
        .collect();
    if text.is_empty() {
        return Err(LlmError::BadResponse("no text in the reply".into()));
    }
    Ok(LlmResponse {
        text,
        model: body["model"].as_str().unwrap_or_default().to_owned(),
        usage: Usage {
            input_tokens: body["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: body["usage"]["output_tokens"].as_u64().unwrap_or(0),
        },
    })
}

#[cfg(test)]
mod tests {
    use anyq_core::domain::{Message, Role};

    use super::*;

    fn config(effort: Option<&str>, fallbacks: bool) -> AnthropicConfig {
        AnthropicConfig {
            api_key: "k".into(),
            model: "claude-opus-5-5".into(),
            base_url: "https://api.anthropic.com".into(),
            effort: effort.map(str::to_owned),
            fallbacks,
        }
    }

    fn request(json_schema: Option<Value>) -> LlmRequest {
        LlmRequest {
            provider: None,
            model: None,
            system: Some("sys".into()),
            messages: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            max_tokens: None,
            json_schema,
        }
    }

    #[test]
    fn body_carries_schema_effort_and_fallbacks() {
        let schema = json!({"type": "object"});
        let body = build_body(&config(Some("high"), true), &request(Some(schema.clone())));
        assert_eq!(
            body,
            json!({
                "model": "claude-opus-5-5",
                "max_tokens": 16000,
                "system": "sys",
                "messages": [{"role": "user", "content": "hi"}],
                "stream": true,
                "output_config": {"effort": "high", "format": {"type": "json_schema", "schema": schema}},
                "fallbacks": "default",
            })
        );
    }

    #[test]
    fn body_omits_what_is_not_configured() {
        let body = build_body(&config(None, false), &request(None));
        assert!(body.get("output_config").is_none());
        assert!(body.get("fallbacks").is_none());
    }

    #[test]
    fn reply_text_skips_thinking_blocks() {
        let reply = json!({
            "model": "claude-opus-5-5",
            "stop_reason": "end_turn",
            "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": "{}"}],
            "usage": {"input_tokens": 7, "output_tokens": 3},
        });
        let parsed = parse(&reply).unwrap();
        assert_eq!(parsed.text, "{}");
        assert_eq!(
            parsed.usage,
            Usage {
                input_tokens: 7,
                output_tokens: 3
            }
        );
    }

    #[test]
    fn a_stream_folds_into_the_message() {
        let stream = r#"event: message_start
data: {"type": "message_start", "message": {"model": "claude-opus-5-5", "content": [], "stop_reason": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}

event: content_block_start
data: {"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}

event: content_block_start
data: {"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}

event: ping
data: {"type": "ping"}

event: content_block_delta
data: {"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "{\"a\""}}

event: content_block_delta
data: {"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": ": 1}"}}

event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 15}}

event: message_stop
data: {"type": "message_stop"}
"#;
        let reply = parse(&fold_stream(stream).unwrap()).unwrap();
        assert_eq!(reply.text, r#"{"a": 1}"#);
        assert_eq!(reply.model, "claude-opus-5-5");
        assert_eq!(
            reply.usage,
            Usage {
                input_tokens: 25,
                output_tokens: 15
            }
        );
    }

    #[test]
    fn a_stream_cut_short_or_carrying_an_error_fails() {
        let cut = "data: {\"type\": \"message_start\", \"message\": {\"content\": []}}\n";
        assert!(
            matches!(fold_stream(cut), Err(LlmError::BadResponse(e)) if e.contains("message_stop"))
        );
        let error = "data: {\"type\": \"error\", \"error\": {\"type\": \"overloaded_error\"}}\n";
        assert!(
            matches!(fold_stream(error), Err(LlmError::BadResponse(e)) if e.contains("overloaded"))
        );
        let refusal = "data: {\"type\": \"message_start\", \"message\": {\"content\": []}}\n\
                       data: {\"type\": \"message_delta\", \"delta\": {\"stop_reason\": \"refusal\"}}\n\
                       data: {\"type\": \"message_stop\"}\n";
        assert!(matches!(
            parse(&fold_stream(refusal).unwrap()),
            Err(LlmError::Refused(_))
        ));
    }

    #[test]
    fn refusal_and_truncation_are_errors() {
        let refusal =
            json!({"stop_reason": "refusal", "stop_details": {"category": "cyber"}, "content": []});
        assert!(matches!(parse(&refusal), Err(LlmError::Refused(why)) if why == "cyber"));
        let cut =
            json!({"stop_reason": "max_tokens", "content": [{"type": "text", "text": "{\"a\":"}]});
        assert!(matches!(parse(&cut), Err(LlmError::Truncated)));
    }
}
