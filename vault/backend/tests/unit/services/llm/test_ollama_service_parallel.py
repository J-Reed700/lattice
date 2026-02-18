from __future__ import annotations

import asyncio
from datetime import datetime

import pytest

from src.services.llm.models import OllamaGenerateResponse
from src.services.llm.ollama_service import OllamaService


@pytest.mark.asyncio()
async def test_generate_many_respects_parallel_limit() -> None:
    service = OllamaService(max_parallel_requests=6)

    inflight = 0
    max_inflight = 0

    async def fake_generate(
        prompt: str,
        model: str | None = None,
        system: str | None = None,
        context: list[int] | None = None,
        options: dict[str, float] | None = None,
        format: str | None = None,
        raw: bool = False,
        keep_alive: str | None = None,
        context_text: str | None = None,
    ) -> OllamaGenerateResponse:
        nonlocal inflight, max_inflight
        inflight += 1
        max_inflight = max(max_inflight, inflight)
        await asyncio.sleep(0.03)
        inflight -= 1

        return OllamaGenerateResponse(
            model=model or "test-model",
            created_at=datetime.utcnow().isoformat(),
            response=f"resp:{prompt}",
            done=True,
        )

    service.generate = fake_generate  # type: ignore[assignment]

    prompts = [f"prompt-{idx}" for idx in range(8)]
    responses = await service.generate_many(prompts, max_parallel_requests=3)

    assert len(responses) == len(prompts)
    assert [item.response for item in responses] == [f"resp:{item}" for item in prompts]
    assert max_inflight <= 3
