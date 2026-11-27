"""Wire types shared with the Rust server (`crates/core/src/domain.rs`).

Change them together with the Rust types and `contracts/*.json`; the contract
tests on both sides fail when one of the three drifts.
"""

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field


class Wire(BaseModel):
    # Unknown fields are an error, and the generated JSON schema gets
    # `additionalProperties: false`, which structured outputs require.
    model_config = ConfigDict(extra="forbid")


class Spec(Wire):
    title: str | None
    file_name: str | None
    text: str


class QuestionDraft(Wire):
    """A question as the model writes it; the harness adds the id."""

    text: str = Field(description="Вопрос автору ТЗ, на который можно ответить одной-двумя фразами")
    reason: str = Field(description="Что в оценке ТЗ изменится от ответа")
    section: str | None = Field(description="Раздел ТЗ, к которому относится вопрос")


class Question(QuestionDraft):
    id: str


class Answer(Wire):
    question_id: str
    text: str


class Requirement(Wire):
    id: str
    source_id: str | None
    section: str
    quote: str
    summary: str


class FindingDraft(Wire):
    """A finding as the model writes it; the code adds what it can check itself."""

    severity: Literal["critical", "major", "minor"]
    category: str = Field(description="Вид проблемы одним словом, например completeness")
    section: str | None = Field(description="Раздел ТЗ, где найдена проблема")
    quote: str | None = Field(
        description="Дословная цитата из ТЗ, если проблема в конкретной фразе"
    )
    problem: str
    suggestion: str = Field(description="Как исправить текст ТЗ")


class Finding(FindingDraft):
    # Absent when the finding is about the set of requirements as a whole.
    requirement_id: str | None
    # A second model tried to refute the finding and could not.
    verified: bool
    # Why the second model kept the finding; None when it did not check it.
    critic_note: str | None


class Dropped(Wire):
    finding: Finding
    reason: Literal["duplicate", "refuted"]
    # For a duplicate, the requirement of the finding that stays; None when that
    # one is a set-level finding, and for a refuted finding.
    kept_in: str | None
    # The critic's reason; None for a merge done by code.
    note: str | None


class Report(Wire):
    summary: str
    requirements: list[Requirement]
    findings: list[Finding]
    open_questions: list[Question]
    models: list[str]
    # Findings verification removed; they never count as defects.
    dropped: list[Dropped]


class Round(Wire):
    questions: list[Question]
    answers: list[Answer]


class StepRequest(Wire):
    review_id: str
    spec: Spec
    rounds: list[Round]


class Ask(Wire):
    type: Literal["ask"] = "ask"
    questions: list[Question]


class Done(Wire):
    type: Literal["done"] = "done"
    report: Report


StepResult = Annotated[Ask | Done, Field(discriminator="type")]


class Message(Wire):
    role: Literal["user", "assistant"]
    content: str


class LlmRequest(Wire):
    provider: str | None = None
    model: str | None = None
    system: str | None = None
    messages: list[Message]
    max_tokens: int | None = None
    json_schema: dict | None = None


class Usage(Wire):
    input_tokens: int
    output_tokens: int


class LlmResponse(Wire):
    text: str
    model: str
    usage: Usage
