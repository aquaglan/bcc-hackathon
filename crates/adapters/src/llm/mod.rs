//! Model providers. `anthropic` talks to the Claude API; `openai_compat` covers
//! everything that speaks `/chat/completions`: on-premise servers (Ollama, vLLM,
//! llama.cpp, LM Studio) and hosted ones.

use std::time::Duration;

use anyq_core::ports::LlmError;
use serde_json::Value;

pub mod anthropic;
pub mod openai_compat;

// One call, streamed or not, must finish within this timeout.
pub(crate) fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .connect_timeout(Duration::from_secs(15))
        // A non-streamed call can sit silent for minutes; without keepalive a
        // NAT or proxy drops the idle connection and the call dies with ETIMEDOUT.
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
}

/// Sends the request and returns the JSON body.
pub(crate) async fn post_json(request: reqwest::RequestBuilder) -> Result<Value, LlmError> {
    let body = post(request).await?;
    serde_json::from_str(&body).map_err(|e| LlmError::BadResponse(e.to_string()))
}

/// Sends the request and returns the body of a successful reply. Retries
/// connection errors, a body cut off midway, 429 and 5xx twice; a timeout is
/// not retried, it would only wait again.
pub(crate) async fn post(request: reqwest::RequestBuilder) -> Result<String, LlmError> {
    const ATTEMPTS: u32 = 3;
    let mut attempt = 1;
    loop {
        let this_try = request
            .try_clone()
            .ok_or_else(|| LlmError::Transport("request body cannot be resent".into()))?;
        let error = match this_try.send().await {
            Ok(response) => {
                let status = response.status();
                match response.text().await {
                    Err(e) if e.is_timeout() => return Err(LlmError::Transport(describe(&e))),
                    Err(e) => LlmError::Transport(describe(&e)),
                    Ok(body) if status.is_success() => return Ok(body),
                    Ok(body) => {
                        let error = LlmError::Status {
                            status: status.as_u16(),
                            body,
                        };
                        if status.as_u16() != 429 && !status.is_server_error() {
                            return Err(error);
                        }
                        error
                    }
                }
            }
            Err(e) if e.is_timeout() => return Err(LlmError::Transport(describe(&e))),
            Err(e) => LlmError::Transport(describe(&e)),
        };
        if attempt == ATTEMPTS {
            return Err(error);
        }
        tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
        attempt += 1;
    }
}

/// The whole cause chain: reqwest's own message ("error sending request")
/// hides whether DNS, TLS or the connection failed.
fn describe(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test]
    async fn transport_error_names_its_cause() {
        // Nothing listens on port 1: the cause chain must say the connection was refused.
        let client = http_client().unwrap();
        let error = post_json(client.post("http://127.0.0.1:1/"))
            .await
            .unwrap_err();
        let LlmError::Transport(text) = error else {
            panic!("{error:?}")
        };
        assert!(text.to_lowercase().contains("refused"), "{text}");
    }
}
