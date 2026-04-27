from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Any

import pytest

from src.modules.rag_engine.deep_research import DeepResearchRAG
from src.modules.rag_engine.types import RAGMode


class _FakeHTTPResponse:
    def __init__(self, payload: dict[str, Any], status_code: int = 200):
        self._payload = payload
        self.status_code = status_code
        self.response = "ok"

    def raise_for_status(self) -> None:
        if self.status_code >= 400:
            raise RuntimeError(f"HTTP error {self.status_code}")

    def json(self) -> dict[str, Any]:
        return self._payload


@dataclass
class _FakeSearchItem:
    file_path: str
    filename: str
    snippet: str
    score: float
    id: str


@dataclass
class _FakeSearchResponse:
    results: list[_FakeSearchItem]


class _FakeSearchService:
    async def search(
        self,
        query: str,
        mode: str = "hybrid",
        limit: int = 5,
        rerank: bool = True,
    ) -> _FakeSearchResponse:
        item = _FakeSearchItem(
            file_path=f"/notes/{query.replace(' ', '_')}.md",
            filename=f"{query[:20]}.md",
            snippet=f"Evidence about {query}",
            score=0.87,
            id=f"id-{query}",
        )
        return _FakeSearchResponse(results=[item])


class _FakeOllamaClient:
    def __init__(self) -> None:
        self.coverage_calls = 0

    async def post(
        self,
        path: str,
        json: dict[str, Any],
        timeout: float,
    ) -> _FakeHTTPResponse:
        assert path == "/api/generate"
        prompt = str(json.get("prompt", ""))

        if "Produce at most" in prompt:
            payload = {"response": '{"queries": ["baseline topic query", "topic validation"]}'}
            return _FakeHTTPResponse(payload)

        if "evaluating research coverage" in prompt:
            self.coverage_calls += 1
            if self.coverage_calls == 1:
                payload = {
                    "response": (
                        '{"confidence": 0.51, "summary": "Initial coverage is partial.", '
                        '"uncovered_aspects": ["benchmark evidence"], "marginal_gain": 0.41}'
                    )
                }
            else:
                payload = {
                    "response": (
                        '{"confidence": 0.9, "summary": "Coverage is now high.", '
                        '"uncovered_aspects": [], "marginal_gain": 0.02}'
                    )
                }
            return _FakeHTTPResponse(payload)

        if "generating follow-up research questions" in prompt:
            if self.coverage_calls == 1:
                payload = {"response": '{"questions": ["What benchmark evidence supports this?"]}'}
            else:
                payload = {"response": '{"questions": []}'}
            return _FakeHTTPResponse(payload)

        if "deep research report" in prompt:
            return _FakeHTTPResponse(
                {"response": "Final synthesized answer with citation [S1]."}
            )

        return _FakeHTTPResponse({"response": "{}"})


@pytest.mark.asyncio()
async def test_deep_research_runs_recursive_loop() -> None:
    engine = DeepResearchRAG(
        search_engine=_FakeSearchService(),
        ollama_client=_FakeOllamaClient(),
        enable_web_search=False,
        max_parallel_llm_calls=2,
    )

    result = await engine.ask_deep_research(
        question="Create a comprehensive analysis of this topic",
        max_depth=2,
        max_iterations=5,
        branch_factor=2,
        top_k=2,
    )

    assert result.mode == RAGMode.DEEP_RESEARCH
    assert result.iterations >= 2
    assert result.confidence >= 0.5
    assert len(result.sources) >= 1
    assert len(result.citations) == 1
    assert result.metadata["trajectory"]


@pytest.mark.asyncio()
async def test_deep_research_respects_zero_time_budget() -> None:
    engine = DeepResearchRAG(
        search_engine=_FakeSearchService(),
        ollama_client=_FakeOllamaClient(),
        enable_web_search=False,
    )

    result = await engine.ask_deep_research(
        question="Any question",
        max_iterations=5,
        time_budget_seconds=0,
    )

    assert result.mode == RAGMode.DEEP_RESEARCH
    assert result.iterations == 0
    assert not result.sources
    assert "couldn't collect enough evidence" in result.answer.lower()


class _TrackingOllamaClient:
    def __init__(self) -> None:
        self.inflight = 0
        self.max_inflight = 0

    async def post(
        self,
        path: str,
        json: dict[str, Any],
        timeout: float,
    ) -> _FakeHTTPResponse:
        self.inflight += 1
        self.max_inflight = max(self.max_inflight, self.inflight)
        await asyncio.sleep(0.03)
        self.inflight -= 1
        return _FakeHTTPResponse({"response": "ok"})


@pytest.mark.asyncio()
async def test_deep_research_ollama_parallelism_is_bounded() -> None:
    tracking_client = _TrackingOllamaClient()
    engine = DeepResearchRAG(
        search_engine=_FakeSearchService(),
        ollama_client=tracking_client,
        enable_web_search=False,
        max_parallel_llm_calls=2,
    )

    await asyncio.gather(
        *(engine._call_ollama_text(prompt="test", max_tokens=16, temperature=0.1) for _ in range(8))
    )

    assert tracking_client.max_inflight <= 2


@pytest.mark.asyncio()
async def test_deep_research_stream_emits_intermediate_events() -> None:
    engine = DeepResearchRAG(
        search_engine=_FakeSearchService(),
        ollama_client=_FakeOllamaClient(),
        enable_web_search=False,
    )

    events = []
    async for event in engine.ask_deep_research_stream(
        question="Give me a comprehensive topic summary",
        max_depth=1,
        max_iterations=3,
        branch_factor=2,
    ):
        events.append(event)

    event_types = [event.get("type") for event in events]
    assert "status" in event_types
    assert "thought" in event_types
    assert "observation" in event_types
    assert "answer" in event_types
    assert event_types[-1] == "complete"
