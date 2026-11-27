"""Registry of harnesses. A harness is one way to run the review agent.

To add one: write a class with `step`, register it here, add its URL to the
`[harnesses.<name>]` section of the server config.
"""

from typing import Protocol

from anyq_harness.harnesses.baseline import Baseline
from anyq_harness.llm import Llm
from anyq_harness.protocol import Ask, Done, StepRequest


class Harness(Protocol):
    async def step(self, request: StepRequest, llm: Llm) -> Ask | Done:
        """Either ask the author or finish; all state arrives in `request`."""
        ...


HARNESSES: dict[str, Harness] = {
    "baseline": Baseline(),
}
