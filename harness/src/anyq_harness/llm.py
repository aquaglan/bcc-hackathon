"""The only way a harness reaches a model: the gateway of the Rust server.

Keys and the cloud/on-premise choice live in the server config, so harness
code is the same for a cloud model and for a local one.
"""

from typing import NamedTuple, Protocol

import httpx
from pydantic import BaseModel

from anyq_harness.protocol import LlmRequest, LlmResponse, Message


class Reply[T: BaseModel](NamedTuple):
    output: T
    # The model that actually answered: the gateway may fall back to another one.
    model: str


class Llm(Protocol):
    async def complete[T: BaseModel](self, *, system: str, user: str, output: type[T]) -> Reply[T]:
        """One model call whose reply is parsed into `output`."""
        ...


class GatewayLlm:
    def __init__(self, client: httpx.AsyncClient, provider: str | None = None) -> None:
        self._client = client
        self._provider = provider

    async def complete[T: BaseModel](self, *, system: str, user: str, output: type[T]) -> Reply[T]:
        request = LlmRequest(
            provider=self._provider,
            system=system,
            messages=[Message(role="user", content=user)],
            json_schema=output.model_json_schema(),
        )
        response = await self._client.post("/v1/llm/complete", json=request.model_dump())
        response.raise_for_status()
        reply = LlmResponse.model_validate(response.json())
        return Reply(output.model_validate_json(reply.text), reply.model)
