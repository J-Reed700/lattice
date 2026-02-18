"""
Deep Research RAG: Recursive planning, retrieval, and synthesis.

Implements a bounded recursive research loop inspired by recent research-agent work:
- Structured trajectory memory (evidence, failed queries, open questions)
- Adaptive query expansion with multi-branch planning
- Confidence + marginal-gain stopping criteria
- Citation-first synthesis
"""

from __future__ import annotations

import asyncio
from collections import deque
from collections.abc import AsyncIterator, Awaitable, Callable
import contextlib
from dataclasses import dataclass, field
import hashlib
import json
import logging
import re
import time
from typing import Any

import httpx

from src.modules.rag_engine.types import AgenticResult, RAGMode, ReasoningStep, SourceCitation

logger = logging.getLogger(__name__)


@dataclass
class ResearchNode:
    """A queued research question to investigate."""

    question: str
    depth: int
    parent_question: str | None = None


@dataclass
class EvidenceItem:
    """Single evidence unit from local or web retrieval."""

    source_type: str
    source_id: str
    title: str
    location: str
    snippet: str
    score: float
    query: str
    depth: int

    def dedupe_key(self) -> str:
        raw_key = f"{self.source_type}|{self.location}|{self.snippet[:240]}"
        return hashlib.sha256(raw_key.encode("utf-8")).hexdigest()

    def as_source_dict(self) -> dict[str, Any]:
        return {
            "file_path": self.location,
            "source_type": self.source_type,
            "title": self.title,
            "snippet": self.snippet,
            "score": self.score,
            "query": self.query,
            "depth": self.depth,
        }


@dataclass
class CoverageAssessment:
    """Coverage score and remaining gaps for current research state."""

    confidence: float
    marginal_gain: float
    summary: str
    uncovered_aspects: list[str] = field(default_factory=list)


@dataclass
class ResearchState:
    """Mutable state carried across recursive research iterations."""

    evidence: list[EvidenceItem] = field(default_factory=list)
    failed_queries: list[str] = field(default_factory=list)
    open_questions: list[str] = field(default_factory=list)
    visited_questions: set[str] = field(default_factory=set)
    trajectory: list[dict[str, Any]] = field(default_factory=list)


class DeepResearchRAG:
    """
    Recursive deep-research engine.

    The loop alternates:
    1) plan query set for a node,
    2) retrieve local/web evidence,
    3) critique coverage and propose follow-up questions,
    4) recurse while confidence gain justifies more cost.
    """

    RETRYABLE_STATUS = {429, 500, 502, 503, 504}

    def __init__(
        self,
        search_engine: Any,
        ollama_client: httpx.AsyncClient,
        model_name: str = "llama3.1:8b",
        enable_web_search: bool = True,
        max_parallel_llm_calls: int = 3,
        max_parallel_search_calls: int = 4,
    ):
        self.search = search_engine
        self.ollama = ollama_client
        self.model_name = model_name
        self.enable_web_search = enable_web_search
        self.max_parallel_llm_calls = max(1, max_parallel_llm_calls)
        self.max_parallel_search_calls = max(1, max_parallel_search_calls)

        self._llm_semaphore = asyncio.Semaphore(self.max_parallel_llm_calls)
        self._search_semaphore = asyncio.Semaphore(self.max_parallel_search_calls)
        self._ddgs: Any | None = None

        if enable_web_search:
            try:
                from duckduckgo_search import DDGS

                self._ddgs = DDGS()
            except Exception as exc:  # pragma: no cover - depends on runtime package state
                logger.warning(
                    "Web search requested but unavailable; continuing local-only. Error: %s", exc
                )
                self.enable_web_search = False

    async def ask_deep_research(
        self,
        question: str,
        search_mode: str = "hybrid",
        max_depth: int = 2,
        max_iterations: int = 8,
        branch_factor: int = 3,
        top_k: int = 5,
        target_confidence: float = 0.82,
        min_marginal_gain: float = 0.05,
        time_budget_seconds: int = 120,
        event_emitter: Callable[[dict[str, Any]], Awaitable[None]] | None = None,
    ) -> AgenticResult:
        """Run recursive deep research and synthesize a citation-backed final answer."""
        start = time.time()

        async def emit(event: dict[str, Any]) -> None:
            if event_emitter is None:
                return
            try:
                await event_emitter(event)
            except Exception as exc:  # pragma: no cover - defensive, emitter-owned path
                logger.debug("Deep research event emitter failed: %s", exc)

        root_question = question.strip()
        if not root_question:
            return AgenticResult(
                answer="Question is empty.",
                mode=RAGMode.DEEP_RESEARCH,
                confidence=0.0,
                metadata={"error": "empty_question"},
            )

        await emit(
            {
                "type": "status",
                "phase": "planning",
                "content": "Starting deep research loop",
                "iteration": 0,
                "metadata": {"question": root_question},
            }
        )

        queue: deque[ResearchNode] = deque([ResearchNode(question=root_question, depth=0)])
        state = ResearchState(
            visited_questions={self._normalize_question(root_question)},
            open_questions=[root_question],
        )
        reasoning_steps: list[ReasoningStep] = []

        best_confidence = 0.15
        last_confidence = 0.0
        iterations = 0

        while queue and iterations < max_iterations:
            elapsed = time.time() - start
            if elapsed >= time_budget_seconds:
                logger.info("Deep research stopping: time budget reached")
                await emit(
                    {
                        "type": "status",
                        "phase": "action",
                        "content": "Stopping: time budget reached",
                        "iteration": iterations,
                        "metadata": {"elapsed_seconds": round(elapsed, 2)},
                    }
                )
                break

            node = queue.popleft()
            iterations += 1

            await emit(
                {
                    "type": "status",
                    "phase": "planning",
                    "content": f"Planning retrieval for depth {node.depth}",
                    "iteration": iterations,
                    "metadata": {"question": node.question, "depth": node.depth},
                }
            )

            planned_queries = await self._plan_queries(
                root_question=root_question,
                node_question=node.question,
                evidence=state.evidence,
                branch_factor=branch_factor,
            )
            planned_queries = planned_queries[:branch_factor] or [node.question]
            await emit(
                {
                    "type": "thought",
                    "phase": "planning",
                    "content": f"Planned {len(planned_queries)} queries",
                    "iteration": iterations,
                    "metadata": {"queries": planned_queries},
                }
            )

            await emit(
                {
                    "type": "action",
                    "phase": "action",
                    "content": "Retrieving local/web evidence",
                    "iteration": iterations,
                    "metadata": {"query_count": len(planned_queries)},
                }
            )
            node_evidence, failed_queries = await self._retrieve_evidence_parallel(
                queries=planned_queries,
                depth=node.depth,
                top_k=top_k,
                search_mode=search_mode,
            )
            state.failed_queries.extend(failed_queries)

            new_count = self._merge_evidence(state.evidence, node_evidence)
            await emit(
                {
                    "type": "observation",
                    "phase": "action",
                    "content": f"Retrieved {len(node_evidence)} evidence items ({new_count} new)",
                    "iteration": iterations,
                    "metadata": {
                        "new_evidence": new_count,
                        "failed_queries": failed_queries,
                    },
                }
            )
            coverage = await self._assess_coverage(
                root_question=root_question,
                node_question=node.question,
                evidence=state.evidence,
                previous_confidence=last_confidence,
            )
            last_confidence = coverage.confidence
            best_confidence = max(best_confidence, coverage.confidence)
            await emit(
                {
                    "type": "observation",
                    "phase": "reasoning",
                    "content": coverage.summary,
                    "iteration": iterations,
                    "metadata": {
                        "confidence": coverage.confidence,
                        "marginal_gain": coverage.marginal_gain,
                        "uncovered_aspects": coverage.uncovered_aspects,
                    },
                }
            )

            node_sources = [ev.as_source_dict() for ev in node_evidence]
            reasoning_steps.append(
                ReasoningStep(
                    question=node.question,
                    answer=coverage.summary,
                    sources=node_sources,
                    confidence=coverage.confidence,
                )
            )

            follow_ups = await self._propose_follow_up_questions(
                root_question=root_question,
                current_question=node.question,
                uncovered_aspects=coverage.uncovered_aspects,
                evidence=state.evidence,
                branch_factor=branch_factor,
            )
            state.open_questions = follow_ups
            queued_now = 0
            for follow_up in follow_ups:
                if node.depth >= max_depth:
                    break

                normalized = self._normalize_question(follow_up)
                if normalized in state.visited_questions:
                    continue

                state.visited_questions.add(normalized)
                queue.append(
                    ResearchNode(
                        question=follow_up,
                        depth=node.depth + 1,
                        parent_question=node.question,
                    )
                )
                queued_now += 1
                if queued_now >= branch_factor:
                    break
            await emit(
                {
                    "type": "thought",
                    "phase": "planning",
                    "content": f"Queued {queued_now} follow-up questions",
                    "iteration": iterations,
                    "metadata": {"follow_ups": follow_ups[:queued_now]},
                }
            )

            state.trajectory.append(
                {
                    "question": node.question,
                    "depth": node.depth,
                    "planned_queries": planned_queries,
                    "new_evidence": new_count,
                    "confidence": coverage.confidence,
                    "marginal_gain": coverage.marginal_gain,
                    "follow_up_questions": follow_ups,
                }
            )

            if (
                coverage.confidence >= target_confidence
                and coverage.marginal_gain <= min_marginal_gain
                and (not coverage.uncovered_aspects or node.depth >= max_depth)
            ):
                logger.info(
                    "Deep research stopping: confidence %.2f, marginal gain %.2f",
                    coverage.confidence,
                    coverage.marginal_gain,
                )
                await emit(
                    {
                        "type": "status",
                        "phase": "reasoning",
                        "content": "Stopping: confidence target reached with low marginal gain",
                        "iteration": iterations,
                        "metadata": {
                            "confidence": coverage.confidence,
                            "marginal_gain": coverage.marginal_gain,
                        },
                    }
                )
                break

        await emit(
            {
                "type": "status",
                "phase": "reasoning",
                "content": "Synthesizing final report",
                "iteration": iterations,
                "metadata": {"evidence_count": len(state.evidence)},
            }
        )
        answer_text, citations = await self._synthesize_answer(
            question=root_question,
            evidence=state.evidence,
        )
        await emit(
            {
                "type": "answer",
                "phase": "complete",
                "content": answer_text,
                "iteration": iterations,
                "metadata": {"citations": len(citations)},
            }
        )

        execution_time_ms = (time.time() - start) * 1000
        return AgenticResult(
            answer=answer_text,
            mode=RAGMode.DEEP_RESEARCH,
            confidence=round(best_confidence, 3),
            citations=citations,
            sources=[item.as_source_dict() for item in state.evidence],
            iterations=iterations,
            reasoning_steps=reasoning_steps,
            execution_time_ms=execution_time_ms,
            metadata={
                "search_mode": search_mode,
                "max_depth": max_depth,
                "branch_factor": branch_factor,
                "time_budget_seconds": time_budget_seconds,
                "web_search_enabled": self.enable_web_search,
                "parallel_llm_calls": self.max_parallel_llm_calls,
                "parallel_search_calls": self.max_parallel_search_calls,
                "failed_queries": state.failed_queries,
                "trajectory": state.trajectory,
            },
        )

    async def ask_deep_research_stream(
        self,
        question: str,
        search_mode: str = "hybrid",
        max_depth: int = 2,
        max_iterations: int = 8,
        branch_factor: int = 3,
        top_k: int = 5,
        target_confidence: float = 0.82,
        min_marginal_gain: float = 0.05,
        time_budget_seconds: int = 120,
    ) -> AsyncIterator[dict[str, Any]]:
        """Stream deep research events as the recursive loop progresses."""
        queue: asyncio.Queue[dict[str, Any] | None] = asyncio.Queue()

        async def emitter(event: dict[str, Any]) -> None:
            await queue.put(event)

        async def run() -> None:
            try:
                result = await self.ask_deep_research(
                    question=question,
                    search_mode=search_mode,
                    max_depth=max_depth,
                    max_iterations=max_iterations,
                    branch_factor=branch_factor,
                    top_k=top_k,
                    target_confidence=target_confidence,
                    min_marginal_gain=min_marginal_gain,
                    time_budget_seconds=time_budget_seconds,
                    event_emitter=emitter,
                )
                await queue.put(
                    {
                        "type": "complete",
                        "phase": "complete",
                        "content": "Deep research completed",
                        "metadata": {
                            "confidence": result.confidence,
                            "iterations": result.iterations,
                            "sources": len(result.sources),
                            "citations": len(result.citations),
                            "execution_time_ms": result.execution_time_ms,
                        },
                    }
                )
            except Exception as exc:
                logger.exception("Deep research streaming failed")
                await queue.put(
                    {
                        "type": "error",
                        "phase": "complete",
                        "content": str(exc),
                        "metadata": {},
                    }
                )
            finally:
                await queue.put(None)

        producer = asyncio.create_task(run())
        try:
            while True:
                event = await queue.get()
                if event is None:
                    break
                yield event
        finally:
            if not producer.done():
                producer.cancel()
                with contextlib.suppress(asyncio.CancelledError):
                    await producer

    async def _plan_queries(
        self,
        root_question: str,
        node_question: str,
        evidence: list[EvidenceItem],
        branch_factor: int,
    ) -> list[str]:
        """Generate focused retrieval queries for the current node."""
        evidence_text = self._format_evidence_for_prompt(evidence, limit=4, chars=260)
        prompt = f"""You are planning web and local retrieval queries.
Return ONLY valid JSON.

Task:
- Root question: {root_question}
- Current sub-question: {node_question}
- Existing evidence:
{evidence_text}

Produce at most {branch_factor} highly specific retrieval queries.
Response JSON schema:
{{
  "queries": ["query 1", "query 2"],
  "reasoning": "short rationale"
}}"""

        raw = await self._call_ollama_text(prompt=prompt, max_tokens=220, temperature=0.2)
        parsed = self._parse_json_object(raw)
        raw_queries = parsed.get("queries", [])
        queries = [
            str(item).strip()
            for item in raw_queries
            if isinstance(item, str) and str(item).strip()
        ]
        if not queries:
            return [node_question]
        return list(dict.fromkeys(queries))

    async def _retrieve_evidence_parallel(
        self,
        queries: list[str],
        depth: int,
        top_k: int,
        search_mode: str,
    ) -> tuple[list[EvidenceItem], list[str]]:
        """Run local and optional web retrieval for each query in parallel."""
        tasks = [
            self._retrieve_for_query(query=query, depth=depth, top_k=top_k, search_mode=search_mode)
            for query in queries
        ]

        retrieved = await asyncio.gather(*tasks, return_exceptions=True)
        evidence: list[EvidenceItem] = []
        failed_queries: list[str] = []

        for query, result in zip(queries, retrieved, strict=False):
            if isinstance(result, Exception):
                logger.warning("Failed retrieval for query '%s': %s", query, result)
                failed_queries.append(query)
                continue

            query_evidence = result
            if not query_evidence:
                failed_queries.append(query)
                continue
            evidence.extend(query_evidence)

        return evidence, failed_queries

    async def _retrieve_for_query(
        self,
        query: str,
        depth: int,
        top_k: int,
        search_mode: str,
    ) -> list[EvidenceItem]:
        """Retrieve local and web evidence for a single query."""
        async with self._search_semaphore:
            local_task = self._search_local(query=query, depth=depth, top_k=top_k, mode=search_mode)

            if self.enable_web_search and self._ddgs is not None:
                web_task = self._search_web(query=query, depth=depth, top_k=max(2, top_k // 2))
                local_evidence, web_evidence = await asyncio.gather(local_task, web_task)
                return local_evidence + web_evidence

            local_evidence = await local_task
            return local_evidence

    async def _search_local(
        self, query: str, depth: int, top_k: int, mode: str
    ) -> list[EvidenceItem]:
        """Search indexed local corpus via SearchService."""
        results: Any
        try:
            results = await self.search.search(query=query, mode=mode, limit=top_k, rerank=True)
        except TypeError:
            results = await self.search.search(query, top_k=top_k)

        raw_results = getattr(results, "results", results)
        evidence: list[EvidenceItem] = []
        for item in raw_results:
            file_path = str(getattr(item, "file_path", ""))
            snippet = str(getattr(item, "snippet", "") or "")
            title = str(getattr(item, "filename", file_path.rsplit("/", maxsplit=1)[-1]))
            score = float(getattr(item, "score", 0.0))
            source_id = str(getattr(item, "id", file_path))
            evidence.append(
                EvidenceItem(
                    source_type="local",
                    source_id=source_id,
                    title=title or "Local document",
                    location=file_path or f"local:{source_id}",
                    snippet=snippet[:800],
                    score=score,
                    query=query,
                    depth=depth,
                )
            )
        return evidence

    async def _search_web(self, query: str, depth: int, top_k: int) -> list[EvidenceItem]:
        """Search DuckDuckGo and normalize result format."""
        if not self.enable_web_search or self._ddgs is None:
            return []

        try:
            web_results = await asyncio.to_thread(
                lambda: list(self._ddgs.text(query, max_results=max(1, top_k)))
            )
        except Exception as exc:
            logger.warning("Web search failed for query '%s': %s", query, exc)
            return []

        evidence: list[EvidenceItem] = []
        for idx, result in enumerate(web_results, start=1):
            link = str(result.get("link", f"web://{query}/{idx}"))
            body = str(result.get("body", ""))
            title = str(result.get("title", f"Web result {idx}"))
            score = max(0.3, 0.75 - (idx * 0.07))
            evidence.append(
                EvidenceItem(
                    source_type="web",
                    source_id=link,
                    title=title,
                    location=link,
                    snippet=body[:900],
                    score=score,
                    query=query,
                    depth=depth,
                )
            )
        return evidence

    async def _assess_coverage(
        self,
        root_question: str,
        node_question: str,
        evidence: list[EvidenceItem],
        previous_confidence: float,
    ) -> CoverageAssessment:
        """Score current evidence coverage and identify remaining gaps."""
        evidence_text = self._format_evidence_for_prompt(evidence, limit=10, chars=230)
        prompt = f"""You are evaluating research coverage.
Return ONLY valid JSON.

Root question: {root_question}
Current question: {node_question}
Current confidence estimate: {previous_confidence:.2f}
Evidence:
{evidence_text}

Response schema:
{{
  "confidence": 0.0,
  "summary": "short progress summary",
  "uncovered_aspects": ["gap 1", "gap 2"],
  "marginal_gain": 0.0
}}
Rules:
- confidence and marginal_gain must be in [0, 1].
- marginal_gain should reflect expected value of another iteration."""

        raw = await self._call_ollama_text(prompt=prompt, max_tokens=260, temperature=0.2)
        parsed = self._parse_json_object(raw)

        confidence = self._clamp_float(parsed.get("confidence", 0.35))
        marginal_gain = self._clamp_float(parsed.get("marginal_gain", 0.2))
        summary = str(parsed.get("summary", "Gathered additional evidence."))[:600]

        uncovered_raw = parsed.get("uncovered_aspects", [])
        uncovered = [
            str(item).strip()
            for item in uncovered_raw
            if isinstance(item, str) and str(item).strip()
        ]

        return CoverageAssessment(
            confidence=confidence,
            marginal_gain=marginal_gain,
            summary=summary,
            uncovered_aspects=uncovered,
        )

    async def _propose_follow_up_questions(
        self,
        root_question: str,
        current_question: str,
        uncovered_aspects: list[str],
        evidence: list[EvidenceItem],
        branch_factor: int,
    ) -> list[str]:
        """Generate follow-up questions from current gaps."""
        if not uncovered_aspects:
            return []

        evidence_text = self._format_evidence_for_prompt(evidence, limit=6, chars=190)
        prompt = f"""You are generating follow-up research questions.
Return ONLY valid JSON.

Root question: {root_question}
Current sub-question: {current_question}
Uncovered aspects: {json.dumps(uncovered_aspects)}
Evidence:
{evidence_text}

Generate at most {branch_factor} next questions that maximize information gain.
Response schema:
{{
  "questions": ["question 1", "question 2"]
}}"""

        raw = await self._call_ollama_text(prompt=prompt, max_tokens=220, temperature=0.25)
        parsed = self._parse_json_object(raw)
        raw_questions = parsed.get("questions", [])

        questions = [
            str(item).strip()
            for item in raw_questions
            if isinstance(item, str) and str(item).strip()
        ]
        if not questions:
            return [aspect for aspect in uncovered_aspects[:branch_factor]]
        return list(dict.fromkeys(questions))[:branch_factor]

    async def _synthesize_answer(
        self, question: str, evidence: list[EvidenceItem]
    ) -> tuple[str, list[SourceCitation]]:
        """Produce final report and build citations from referenced evidence IDs."""
        if not evidence:
            return (
                "I couldn't collect enough evidence to answer this question reliably.",
                [],
            )

        ranked = sorted(evidence, key=lambda item: item.score, reverse=True)
        cited_evidence = ranked[:18]
        evidence_lines = []
        for idx, item in enumerate(cited_evidence, start=1):
            evidence_lines.append(
                f"[S{idx}] ({item.source_type}) {item.title} | {item.location}\n"
                f"Snippet: {item.snippet}"
            )
        evidence_text = "\n\n".join(evidence_lines)

        prompt = f"""You are writing a deep research report.
Answer the question using ONLY the evidence.

Question: {question}

Evidence:
{evidence_text}

Requirements:
- Cite supporting evidence inline with [S#].
- Distinguish established facts from uncertainties.
- Include a short "Remaining uncertainties" section.
- Do not invent sources.

Report:"""

        answer = await self._call_ollama_text(prompt=prompt, max_tokens=1200, temperature=0.25)
        citations = self._extract_citations(answer, cited_evidence)
        return answer.strip(), citations

    async def _call_ollama_text(
        self,
        prompt: str,
        max_tokens: int,
        temperature: float,
        retries: int = 3,
    ) -> str:
        """Bounded, retrying Ollama call with backpressure control."""
        last_error: Exception | None = None

        for attempt in range(retries):
            try:
                async with self._llm_semaphore:
                    response = await self.ollama.post(
                        "/api/generate",
                        json={
                            "model": self.model_name,
                            "prompt": prompt,
                            "stream": False,
                            "options": {
                                "num_predict": max_tokens,
                                "temperature": temperature,
                            },
                        },
                        timeout=45.0,
                    )
                    response.raise_for_status()
                    payload = response.json()
                    return str(payload.get("response", "")).strip()
            except httpx.HTTPStatusError as exc:
                last_error = exc
                status = exc.response.status_code
                if status not in self.RETRYABLE_STATUS or attempt == retries - 1:
                    break
            except (httpx.TimeoutException, httpx.ConnectError, httpx.ReadError) as exc:
                last_error = exc
                if attempt == retries - 1:
                    break

            await asyncio.sleep(0.35 * (2**attempt))

        logger.warning("Ollama call failed after retries: %s", last_error)
        return ""

    def _extract_citations(
        self, answer: str, ranked_evidence: list[EvidenceItem]
    ) -> list[SourceCitation]:
        matches = re.findall(r"\[S(\d+)\]", answer)
        citation_numbers = sorted({int(match) for match in matches if match.isdigit()})

        citations: list[SourceCitation] = []
        for citation_id in citation_numbers:
            if citation_id <= 0 or citation_id > len(ranked_evidence):
                continue
            item = ranked_evidence[citation_id - 1]
            citations.append(
                SourceCitation(
                    file_path=item.location,
                    snippet=item.snippet,
                    score=item.score,
                    citation_id=citation_id,
                )
            )
        return citations

    def _merge_evidence(self, existing: list[EvidenceItem], incoming: list[EvidenceItem]) -> int:
        existing_keys = {item.dedupe_key() for item in existing}
        added = 0
        for item in incoming:
            key = item.dedupe_key()
            if key in existing_keys:
                continue
            existing.append(item)
            existing_keys.add(key)
            added += 1
        return added

    @staticmethod
    def _normalize_question(question: str) -> str:
        return " ".join(question.lower().split())

    @staticmethod
    def _clamp_float(value: Any) -> float:
        try:
            numeric = float(value)
        except (TypeError, ValueError):
            return 0.0
        return max(0.0, min(1.0, numeric))

    @staticmethod
    def _parse_json_object(raw_text: str) -> dict[str, Any]:
        """Parse JSON object with a tolerant fallback for model-formatted text."""
        if not raw_text.strip():
            return {}

        try:
            parsed = json.loads(raw_text)
            if isinstance(parsed, dict):
                return parsed
            return {}
        except json.JSONDecodeError:
            match = re.search(r"\{.*\}", raw_text, flags=re.DOTALL)
            if not match:
                return {}
            try:
                parsed = json.loads(match.group(0))
                if isinstance(parsed, dict):
                    return parsed
            except json.JSONDecodeError:
                return {}
        return {}

    @staticmethod
    def _format_evidence_for_prompt(
        evidence: list[EvidenceItem], limit: int, chars: int
    ) -> str:
        if not evidence:
            return "- none yet"

        ranked = sorted(evidence, key=lambda item: item.score, reverse=True)[:limit]
        lines = []
        for idx, item in enumerate(ranked, start=1):
            snippet = item.snippet.replace("\n", " ").strip()[:chars]
            lines.append(
                f"{idx}. ({item.source_type}) {item.title} | score={item.score:.2f} | "
                f"{item.location} | {snippet}"
            )
        return "\n".join(lines)


__all__ = ["DeepResearchRAG"]
