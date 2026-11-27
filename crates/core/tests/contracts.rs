//! `contracts/*.json` is the wire format shared with the Python harnesses,
//! which check the same files in `harness/tests/test_contract.py`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyq_core::domain::{LlmRequest, LlmResponse, StepRequest, StepResult};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// The fixture must survive a round trip unchanged: no field lost, none added.
fn roundtrip<T: DeserializeOwned + Serialize>(file: &str) {
    let path = format!("{}/../../contracts/{file}", env!("CARGO_MANIFEST_DIR"));
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let typed: T = serde_json::from_value(fixture.clone()).unwrap();
    assert_eq!(serde_json::to_value(typed).unwrap(), fixture, "{file}");
}

#[test]
fn fixtures_match_the_rust_types() {
    roundtrip::<StepRequest>("step_request.json");
    roundtrip::<StepResult>("step_result_ask.json");
    roundtrip::<StepResult>("step_result_done.json");
    roundtrip::<LlmRequest>("llm_request.json");
    roundtrip::<LlmResponse>("llm_response.json");
}
