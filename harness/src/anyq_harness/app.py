"""HTTP face of the harnesses: the Rust server posts a step here.

Run: uvicorn anyq_harness.app:app --port 8090
Bind to loopback or a private network: this service has no authentication.
"""

import os
from contextlib import asynccontextmanager

import httpx
from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import JSONResponse

from anyq_harness.harnesses import HARNESSES
from anyq_harness.llm import GatewayLlm
from anyq_harness.protocol import StepRequest, StepResult


@asynccontextmanager
async def lifespan(app: FastAPI):
    token = os.environ.get("ANYQ_API_TOKEN")
    async with httpx.AsyncClient(
        base_url=os.environ.get("ANYQ_GATEWAY_URL", "http://127.0.0.1:8080"),
        headers={"Authorization": f"Bearer {token}"} if token else None,
        # A model call can take minutes; the server's own timeout is 600 s.
        timeout=660,
    ) as client:
        app.state.llm = GatewayLlm(client, provider=os.environ.get("ANYQ_PROVIDER"))
        yield


app = FastAPI(title="AnyQ harnesses", lifespan=lifespan)


@app.exception_handler(httpx.HTTPError)
async def gateway_failed(_: Request, error: httpx.HTTPError) -> JSONResponse:
    # Pass the gateway's own explanation up, so the review's `error` says why.
    detail = error.response.text if isinstance(error, httpx.HTTPStatusError) else repr(error)
    return JSONResponse(status_code=502, content={"detail": f"LLM gateway: {detail}"})


@app.get("/health")
async def health() -> str:
    return "ok"


@app.post("/v1/harnesses/{name}/step")
async def step(name: str, body: StepRequest, request: Request) -> StepResult:
    harness = HARNESSES.get(name)
    if harness is None:
        raise HTTPException(status_code=404, detail=f"unknown harness `{name}`")
    return await harness.step(body, request.app.state.llm)
