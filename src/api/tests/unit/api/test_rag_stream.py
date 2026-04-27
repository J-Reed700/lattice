from __future__ import annotations

from unittest.mock import MagicMock

import pytest

from src.api.errors import ValidationError
from src.api.v1.rag import stream_deep_research
from src.schemas.agentic_rag import AgenticQueryRequest


class _FakeDeepResearch:
    async def ask_deep_research_stream(self, **kwargs):
        yield {"type": "status", "phase": "planning", "content": "starting"}
        yield {"type": "observation", "phase": "action", "content": "retrieved docs"}
        yield {"type": "complete", "phase": "complete", "content": "done"}


class _FakeAgenticRAG:
    def __init__(self):
        self.deep_research = _FakeDeepResearch()


@pytest.mark.asyncio()
async def test_stream_deep_research_returns_sse_frames() -> None:
    response = await stream_deep_research(
        http_request=MagicMock(),
        request=AgenticQueryRequest(query="Provide a deep research summary"),
        current_user=MagicMock(),
        _=None,
        rag_engine=_FakeAgenticRAG(),
    )

    frames: list[str] = []
    async for chunk in response.body_iterator:
        frames.append(chunk.decode() if isinstance(chunk, bytes) else chunk)

    payload = "".join(frames)
    assert "event: status" in payload
    assert "event: observation" in payload
    assert "event: complete" in payload


@pytest.mark.asyncio()
async def test_stream_deep_research_rejects_blank_query() -> None:
    with pytest.raises(ValidationError):
        await stream_deep_research(
            http_request=MagicMock(),
            request=AgenticQueryRequest(query="   "),
            current_user=MagicMock(),
            _=None,
            rag_engine=_FakeAgenticRAG(),
        )
