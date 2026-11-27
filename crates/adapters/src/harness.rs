use std::time::Duration;

use anyq_core::domain::{StepRequest, StepResult};
use anyq_core::ports::{Harness, HarnessError};
use async_trait::async_trait;

/// A harness running as a separate service: `POST url` with a `StepRequest`
/// returns a `StepResult`. Any language can implement it.
pub struct HttpHarness {
    http: reqwest::Client,
    url: String,
}

impl HttpHarness {
    pub fn new(url: String) -> Result<Self, reqwest::Error> {
        // A step is one or more model calls, so it can take minutes.
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(900))
            .build()?;
        Ok(Self { http, url })
    }
}

#[async_trait]
impl Harness for HttpHarness {
    async fn step(&self, req: &StepRequest) -> Result<StepResult, HarnessError> {
        let failed = |e: reqwest::Error| HarnessError(format!("harness unreachable: {e}"));
        let response = self
            .http
            .post(&self.url)
            .json(req)
            .send()
            .await
            .map_err(failed)?;
        let status = response.status();
        let body = response.text().await.map_err(failed)?;
        if !status.is_success() {
            return Err(HarnessError(format!(
                "harness returned HTTP {status}: {body}"
            )));
        }
        serde_json::from_str(&body).map_err(|e| HarnessError(format!("bad step result: {e}")))
    }
}
