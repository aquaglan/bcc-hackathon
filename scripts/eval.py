"""Measure one run against a case: known defects and expected questions the agent missed.

Run: python3 scripts/eval.py eval/cases/card-alerts review.json
where review.json is written by `anyq-server review ... --json review.json`.

Both measures are losses, smaller is better. «пропущено дефектов, %»: a defect
counts as found by a finding or, separately, by a question to the author.
«пропущено ожидаемых вопросов, %»: an expected question counts as asked by any
question of the report or of a dialog round, one agent question for at most one
expected question. Findings and questions that match
nothing known are listed for a human to judge: the case cannot tell noise from
a real point the list missed.
"""

import json
import re
import sys
import tomllib
from pathlib import Path

LINE = re.compile(r"строка (\d+)")


def lines_of(*anchors: str | None) -> set[int]:
    return {int(m) for anchor in anchors if anchor for m in LINE.findall(anchor)}


def matches(defect: dict, lines: set[int], text: str) -> bool:
    if lines & set(defect.get("lines", [])):
        return True
    return any(word in text.lower() for word in defect.get("keywords", []))


def assign(wanted: list[dict], asked: list[tuple[set[int], str]]) -> dict[int, int]:
    """Expected question -> the agent question it is credited to, one to one.

    A maximum bipartite matching: one broad question cannot cover several
    expected ones, and no expected question is lost to a greedy order.
    """
    fits = [[a for a, (ls, t) in enumerate(asked) if matches(w, ls, t)] for w in wanted]
    owner: dict[int, int] = {}

    def take(w: int, seen: set[int]) -> bool:
        for a in fits[w]:
            if a not in seen:
                seen.add(a)
                if a not in owner or take(owner[a], seen):
                    owner[a] = w
                    return True
        return False

    for w in range(len(wanted)):
        take(w, set())
    return {w: a for a, w in owner.items()}


def evaluate(case: Path, review: dict) -> dict:
    expected = tomllib.loads((case / "expected.toml").read_text())
    defects, wanted = expected["defect"], expected.get("question", [])
    report = review["report"]
    sections = {r["id"]: r["section"] for r in report["requirements"]}
    findings = [
        (
            lines_of(f["section"], sections.get(f["requirement_id"] or "")),
            f"{f['problem']} {f['suggestion']}",
            f,
        )
        for f in report["findings"]
    ]
    questions = [
        (lines_of(q["section"]), f"{q['text']} {q['reason']}") for q in report["open_questions"]
    ]
    # A dialog asks before the report; those questions count as asked too.
    asked = questions + [
        (lines_of(q["section"]), f"{q['text']} {q['reason']}")
        for r in review.get("rounds", [])
        for q in r["questions"]
    ]
    credited = assign(wanted, asked)
    missed_questions = [w for n, w in enumerate(wanted) if n not in credited]
    unmatched_questions = [
        t.strip() for a, (ls, t) in enumerate(asked) if a not in credited.values()
    ]
    by_finding = {d["id"] for d in defects if any(matches(d, ls, t) for ls, t, _ in findings)}
    by_question = {
        d["id"] for d in defects if any(matches(d, ls, t) for ls, t in questions)
    } - by_finding
    missed = [d for d in defects if d["id"] not in by_finding | by_question]
    unmatched = [f for ls, t, f in findings if not any(matches(d, ls, t) for d in defects)]
    return {
        "defects": len(defects),
        "found_by_finding": sorted(by_finding),
        "found_by_question": sorted(by_question),
        "missed": [f"{d['id']}: {d['what']}" for d in missed],
        "missed_pct": round(100 * len(missed) / len(defects), 1),
        "findings": len(report["findings"]),
        "unmatched_findings": [
            f"{f['category']} · {f['section']}: {f['problem']}" for f in unmatched
        ],
        "requirements": len(report["requirements"]),
        "questions": len(report["open_questions"]),
        "expected_questions": len(wanted),
        "missed_questions": [f"{w['id']}: {w['what']}" for w in missed_questions],
        "missed_questions_pct": round(100 * len(missed_questions) / len(wanted), 1)
        if wanted
        else None,
        "unmatched_questions": unmatched_questions,
    }


def main() -> None:
    case, review = Path(sys.argv[1]), json.loads(Path(sys.argv[2]).read_text())
    print(json.dumps(evaluate(case, review), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
