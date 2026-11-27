"""`contracts/*.json` is the wire format shared with the Rust server, which
checks the same files in `crates/core/tests/contracts.rs`."""

import json
from pathlib import Path

import pytest
from pydantic import TypeAdapter

from anyq_harness.protocol import LlmRequest, LlmResponse, StepRequest, StepResult

CONTRACTS = Path(__file__).parents[2] / "contracts"


@pytest.mark.parametrize(
    ("file", "wire_type"),
    [
        ("step_request.json", StepRequest),
        ("step_result_ask.json", StepResult),
        ("step_result_done.json", StepResult),
        ("llm_request.json", LlmRequest),
        ("llm_response.json", LlmResponse),
    ],
)
def test_fixture_survives_a_round_trip(file, wire_type):
    fixture = json.loads((CONTRACTS / file).read_text())
    adapter = TypeAdapter(wire_type)
    assert adapter.dump_python(adapter.validate_python(fixture), mode="json") == fixture
