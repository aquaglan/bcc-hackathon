import importlib.util
from pathlib import Path

ROOT = Path(__file__).parents[2]
_spec = importlib.util.spec_from_file_location("eval_script", ROOT / "scripts/eval.py")
assert _spec and _spec.loader
eval_script = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(eval_script)


def finding(section, problem, requirement_id=None):
    return {
        "requirement_id": requirement_id,
        "category": "X",
        "section": section,
        "problem": problem,
        "suggestion": "",
    }


def test_defects_match_by_line_or_keyword_and_the_rest_are_missed():
    review = {
        "report": {
            "requirements": [{"id": "R-01", "section": "ФТ, строка 39"}],
            "findings": [
                finding(None, "Противоречие", requirement_id="R-01"),  # D01 via its requirement
                finding("Весь документ", "Нет схемы отката"),  # D18 by keyword
                finding("Цели, строка 23", "Цель размыта"),  # matches nothing known
            ],
            "open_questions": [{"section": "НФТ, строка 46", "text": "Какая доля?", "reason": ""}],
        }
    }
    result = eval_script.evaluate(ROOT / "eval/cases/card-alerts", review)
    assert result["found_by_finding"] == ["D01", "D18"]
    assert result["found_by_question"] == ["D04"]
    assert result["defects"] - 3 == len(result["missed"])
    assert result["unmatched_findings"] == ["X · Цели, строка 23: Цель размыта"]


def question(section, text):
    return {"section": section, "text": text, "reason": ""}


def test_expected_questions_count_from_the_report_and_dialog_rounds():
    review = {
        "report": {
            "requirements": [],
            "findings": [],
            "open_questions": [question("НФТ, строка 46", "В течение минуты от чего?")],  # Q12
        },
        "rounds": [
            {"questions": [question(None, "Что делать после возврата?"), question(None, "Цвет?")]}
        ],
    }
    result = eval_script.evaluate(ROOT / "eval/cases/card-alerts", review)
    missed = {m.split(":")[0] for m in result["missed_questions"]}
    # Line 46 fits Q11 and Q12; the one question goes to one of them.
    assert "Q03" not in missed and len({"Q11", "Q12"} - missed) == 1
    assert len(missed) == result["expected_questions"] - 2
    assert result["unmatched_questions"] == ["Цвет?"]


def test_one_question_is_credited_to_one_expected_question():
    # Line 39 fits Q01, Q03, Q10 and Q11; «возврат» fits only Q03. The matching gives
    # Q03 to the second question and one of the others to the first, never three.
    asked = [question("Ф-3, строка 39", "Сколько push?"), question(None, "А возврат?")]
    review = {"report": {"requirements": [], "findings": [], "open_questions": asked}}
    result = eval_script.evaluate(ROOT / "eval/cases/card-alerts", review)
    assert result["expected_questions"] - len(result["missed_questions"]) == 2
    assert "Q03" not in {m.split(":")[0] for m in result["missed_questions"]}
