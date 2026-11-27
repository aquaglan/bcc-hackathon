import asyncio

from anyq_harness.harnesses.baseline import MAX_ROUNDS, Analysis, Baseline, render
from anyq_harness.llm import Reply
from anyq_harness.protocol import (
    Answer,
    Ask,
    Done,
    Question,
    QuestionDraft,
    Round,
    Spec,
    StepRequest,
)


class FakeLlm:
    """Stands in for the gateway: returns a prepared analysis, records the prompt."""

    def __init__(self, analysis: Analysis) -> None:
        self.analysis = analysis
        self.user = ""

    async def complete(self, *, system, user, output):
        self.user = user
        return Reply(self.analysis, "fake-model")


def analysis(questions: int) -> Analysis:
    drafts = [
        QuestionDraft(text=f"Вопрос {n}?", reason="важно", section=None) for n in range(questions)
    ]
    return Analysis(summary="Итог", findings=[], questions=drafts)


def request(rounds: int) -> StepRequest:
    answered = Round(
        questions=[Question(id="r1q1", text="Валюта?", reason="лимиты", section=None)],
        answers=[Answer(question_id="r1q1", text="KZT")],
    )
    return StepRequest(
        review_id="5f0c1b3e-8a55-4d0c-9c2f-3d1f6f6f0a11",
        spec=Spec(title="Переводы", file_name=None, text="Система должна быть быстрой."),
        rounds=[answered] * rounds,
    )


def step(rounds: int, questions: int) -> Ask | Done:
    return asyncio.run(Baseline().step(request(rounds), FakeLlm(analysis(questions))))


def test_asks_with_ids_unique_within_the_review():
    result = step(rounds=1, questions=2)
    assert isinstance(result, Ask)
    assert [q.id for q in result.questions] == ["r2q1", "r2q2"]


def test_finishes_when_there_is_nothing_to_ask():
    result = step(rounds=0, questions=0)
    assert isinstance(result, Done)
    assert result.report.summary == "Итог"
    assert result.report.models == ["fake-model"]


def test_stops_asking_after_the_last_round():
    result = step(rounds=MAX_ROUNDS, questions=1)
    assert isinstance(result, Done)
    assert [q.id for q in result.report.open_questions] == [f"r{MAX_ROUNDS + 1}q1"]


def test_prompt_carries_the_spec_and_the_answers():
    prompt = render(request(rounds=1))
    assert "Система должна быть быстрой." in prompt
    assert "Валюта?" in prompt and "KZT" in prompt


def test_output_schema_fits_structured_outputs():
    """Cloud and OpenAI-compatible servers both reject a schema with an open
    object or an optional property."""

    def check(node):
        if isinstance(node, dict):
            if node.get("type") == "object":
                assert node.get("additionalProperties") is False, node
                assert set(node.get("required", [])) == set(node.get("properties", {})), node
            for value in node.values():
                check(value)
        elif isinstance(node, list):
            for value in node:
                check(value)

    check(Analysis.model_json_schema())
