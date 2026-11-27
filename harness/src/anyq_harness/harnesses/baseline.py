"""One model call per step: read the spec and the answers so far, then either
ask the author or write the report.

The prompt is a placeholder until the organisers publish the spec template and
the quality criteria (25 Nov); the structure around it is what stays.
"""

from anyq_harness.llm import Llm
from anyq_harness.protocol import (
    Ask,
    Done,
    Finding,
    FindingDraft,
    Question,
    QuestionDraft,
    Report,
    StepRequest,
    Wire,
)

# After this many answered rounds the agent stops asking and reports.
MAX_ROUNDS = 2

SYSTEM = """\
Ты проверяешь техническое задание (ТЗ) финтех-проекта до того, как по нему начнут разработку. \
Твой читатель — автор ТЗ: ему нужно понять, что в тексте помешает разработчикам и тестировщикам, \
и как это исправить.

Ищи то, что стоит денег на разработке: пропущенные требования, двусмысленные формулировки, \
противоречия между разделами, требования, которые нельзя проверить тестом, упущенные для \
финтеха вещи (безопасность, персональные данные, лимиты, отказоустойчивость, интеграции, \
регуляторные требования). Каждую проблему привязывай к разделу и, где возможно, к цитате.

Вопросы автору задавай только такие, ответ на которые изменит оценку и которого нет в тексте ТЗ \
и в уже данных ответах. Если спрашивать больше нечего, верни пустой список вопросов. \
Учитывай данные ответы: проблема, которую ответ снял, проблемой больше не считается.

Пиши на языке ТЗ."""


class Analysis(Wire):
    summary: str
    findings: list[FindingDraft]
    questions: list[QuestionDraft]


class Baseline:
    async def step(self, request: StepRequest, llm: Llm) -> Ask | Done:
        analysis, model = await llm.complete(system=SYSTEM, user=render(request), output=Analysis)
        round_number = len(request.rounds) + 1
        questions = [
            Question(id=f"r{round_number}q{n}", **draft.model_dump())
            for n, draft in enumerate(analysis.questions, start=1)
        ]
        if questions and len(request.rounds) < MAX_ROUNDS:
            return Ask(questions=questions)
        return Done(
            report=Report(
                summary=analysis.summary,
                requirements=[],
                findings=[
                    Finding(
                        **draft.model_dump(), requirement_id=None, verified=False, critic_note=None
                    )
                    for draft in analysis.findings
                ],
                open_questions=questions,
                models=[model],
                dropped=[],
            )
        )


def render(request: StepRequest) -> str:
    parts = []
    if request.spec.title:
        parts.append(f"Название: {request.spec.title}")
    parts.append(f"<tz>\n{request.spec.text}\n</tz>")
    for number, round_ in enumerate(request.rounds, start=1):
        answers = {answer.question_id: answer.text for answer in round_.answers}
        lines = [
            f"Вопрос: {question.text}\nОтвет: {answers.get(question.id, 'автор не ответил')}"
            for question in round_.questions
        ]
        parts.append(f'<round n="{number}">\n' + "\n\n".join(lines) + "\n</round>")
    if len(request.rounds) >= MAX_ROUNDS:
        parts.append(
            "Это последний шаг: вопросы, оставшиеся без ответа, уйдут в отчёт как открытые."
        )
    return "\n\n".join(parts)
