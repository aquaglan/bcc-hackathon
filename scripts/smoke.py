"""End-to-end check of the wiring without a real model.

Starts the Rust server and the Python harness service against a fake
OpenAI-compatible model and walks one review through every hop:
API -> harness -> LLM gateway -> provider adapter -> model, and back.
Then runs `anyq-server review` on a file, static and in a dialog, against the
same fake model with the built-in pipeline, and checks the report it writes.

Run: make smoke
"""

import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOKEN = "smoke-token"

ANALYSIS = {
    "summary": "ТЗ неполное.",
    "findings": [
        {
            "severity": "major",
            "category": "testability",
            "section": None,
            "quote": "быстро",
            "problem": "Требование нельзя проверить тестом.",
            "suggestion": "Указать время обработки в секундах.",
        }
    ],
    "questions": [{"text": "Какое время допустимо?", "reason": "Нужен порог.", "section": None}],
}


SPEC = "# Переводы\n\n## ФТ\n\n* **Ф-1.** Перевод проходит быстро.\n"
# Replies by stage: the baseline harness, then the stages of the built-in pipeline.
STAGES = {
    "Analysis": ANALYSIS,
    "FinalQuestions": {
        "final_questions": [{"from": ["Q0"], "text": "Сколько секунд?", "reason": "Нужен порог."}]
    },
    "Requirements": {
        "requirements": [
            {
                "source_id": "Ф-1",
                "section": "ФТ",
                "quote": "Перевод проходит быстро.",
                "summary": "Скорость",
            }
        ]
    },
    "RequirementChecks": {
        "requirements": [
            {
                "requirement_id": "R-01",
                "findings": [
                    {
                        "criterion": "REQ-VERIFIABLE",
                        "severity": "critical",
                        "quote": "проходит быстро",
                        "problem": "Нет порога.",
                        "suggestion": "Указать секунды.",
                    }
                ],
                "questions": [],
            }
        ]
    },
    "SetCheck": {
        "summary": "Не готов.",
        "findings": [],
        "questions": [{"text": "Сколько секунд?", "reason": "Нужен порог."}],
    },
    "Verdicts": {
        "verdicts": [
            {
                "finding_id": "F0",
                "verdict": "confirmed",
                "reason": "Порога нет.",
                "question": None,
                "duplicate_of": None,
            }
        ]
    },
}


class FakeModel(BaseHTTPRequestHandler):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert body["response_format"]["type"] == "json_schema", body
        content = STAGES[stage(body["response_format"]["json_schema"]["schema"])]
        reply = {
            "model": body["model"],
            "choices": [{"finish_reason": "stop", "message": {"content": json.dumps(content)}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
        }
        data = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


def stage(schema: dict) -> str:
    """Which stage asks, by the shape of its schema."""
    if schema.get("title") == "Analysis":
        return "Analysis"
    properties = schema["properties"]
    if "verdicts" in properties:
        return "Verdicts"
    if "final_questions" in properties:
        return "FinalQuestions"
    if "summary" in properties:
        return "SetCheck"
    if "requirement_id" in properties["requirements"]["items"]["properties"]:
        return "RequirementChecks"
    return "Requirements"


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def call(method: str, url: str, body: dict | None = None):
    request = urllib.request.Request(
        url,
        method=method,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        text = response.read().decode()
    return json.loads(text) if text.startswith(("{", "[")) else text


def wait_until_up(url: str) -> None:
    for _ in range(150):
        try:
            call("GET", f"{url}/health")
            return
        except (urllib.error.URLError, ConnectionError):
            time.sleep(0.2)
    sys.exit(f"{url} did not start")


def main() -> None:
    model = HTTPServer(("127.0.0.1", 0), FakeModel)
    threading.Thread(target=model.serve_forever, daemon=True).start()
    api = f"http://127.0.0.1:{free_port()}"
    harness = f"http://127.0.0.1:{free_port()}"

    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as config:
        config.write(f"""
listen = "{api.removeprefix("http://")}"
default_provider = "fake"
default_harness = "baseline"

[providers.fake]
kind = "openai_compat"
base_url = "http://127.0.0.1:{model.server_port}/v1"
model = "fake-model"

[harnesses.baseline]
kind = "http"
url = "{harness}/v1/harnesses/baseline/step"

[harnesses.pipeline]
kind = "pipeline"

[harnesses.dialog]
kind = "pipeline"
ask_first = true
""")

    env = os.environ | {"ANYQ_API_TOKEN": TOKEN, "ANYQ_GATEWAY_URL": api}
    uvicorn = [
        "uv",
        "run",
        "--project",
        str(ROOT / "harness"),
        "uvicorn",
        "anyq_harness.app:app",
    ]
    processes = [
        subprocess.Popen([str(ROOT / "target/debug/anyq-server"), config.name], env=env),
        subprocess.Popen(
            [*uvicorn, "--port", harness.rsplit(":", 1)[1], "--log-level", "warning"], env=env
        ),
    ]
    try:
        wait_until_up(api)
        wait_until_up(harness)
        spec = {"title": "Переводы", "text": "Система должна быстро обрабатывать переводы."}
        review = call("POST", f"{api}/v1/reviews", {"spec": spec})
        rounds_answered = 0
        while review["status"] != "done":
            if review["status"] == "failed":
                sys.exit(f"review failed: {review['error']}")
            if review["status"] == "awaiting_answers":
                answers = [
                    {"question_id": question["id"], "text": "До 3 секунд."}
                    for question in review["rounds"][-1]["questions"]
                ]
                call("POST", f"{api}/v1/reviews/{review['id']}/answers", {"answers": answers})
                rounds_answered += 1
            time.sleep(0.1)
            review = call("GET", f"{api}/v1/reviews/{review['id']}")

        report = review["report"]
        # The fake model always has a question, so the harness stops at its round limit.
        assert rounds_answered == 2, rounds_answered
        assert report["findings"][0]["quote"] == "быстро", report
        assert report["open_questions"][0]["id"] == "r3q1", report
        print(f"smoke: ok ({rounds_answered} rounds, {len(report['findings'])} finding)")
    finally:
        for process in processes:
            process.terminate()
        for process in processes:
            process.wait(timeout=10)

    try:
        cli(config.name, env)
    finally:
        os.unlink(config.name)


def cli(config: str, env: dict[str, str]) -> None:
    command = [str(ROOT / "target/debug/anyq-server"), "review"]
    with tempfile.TemporaryDirectory() as tmp:
        spec, out = Path(tmp) / "spec.md", Path(tmp) / "out" / "report.md"
        spec.write_text(SPEC, encoding="utf-8")
        for flags, answer in (([], ""), (["--interactive"], "3 секунды\n")):
            subprocess.run(
                [*command, str(spec), str(out), "--config", config, *flags],
                env=env,
                input=answer,
                text=True,
                check=True,
                timeout=120,
            )
            report = out.read_text(encoding="utf-8")
            assert "## 1. Паспорт прогона" in report and "fake-model" in report, report
            assert "| R-01 | Ф-1 | Скорость |" in report, report
            assert "**Нельзя.**" in report, report
    # The dialog asked once, so the fake model's question is now from round 2.
    assert "**В-1**" in report and "`dialog`" in report, report
    print("smoke: ok (anyq-server review wrote the report, static and in a dialog)")


if __name__ == "__main__":
    main()
