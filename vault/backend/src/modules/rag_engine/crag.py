"""
CRAG: Corrective Retrieval-Augmented Generation

Implements CRAG pattern where retrieval quality is assessed and corrective
actions are taken if quality is low:
- HIGH quality: Use local documents
- MEDIUM quality: Supplement with web search (if available)
- LOW quality: Use only web search or fail gracefully

This is cutting-edge 2025 technology for robust retrieval with fallbacks.

Basic Usage:
    >>> from src.modules.rag_engine.crag import CRAG
    >>>
    >>> # Without web search (default)
    >>> crag = CRAG(local_search, None, ollama_client)
    >>>
    >>> # With web search enabled
    >>> crag = CRAG(local_search, None, ollama_client, enable_web_search=True)
    >>>
    >>> result = await crag.ask_with_correction(
    ...     question="What is quantum computing?",
    ...     enable_web_fallback=True
    ... )
    >>> print(f"Source used: {result.metadata['source']}")
"""

import logging
import time
from typing import Any, Optional

from src.modules.rag_engine.types import (
    AgenticResult,
    DocumentQuality,
    RAGMode,
    SourceCitation,
)

logger = logging.getLogger(__name__)


class CRAG:
    """
    CRAG: Corrective RAG with fallback sources.

    Key capabilities:
    - Assesses quality of local retrieval
    - Falls back to web search if local quality is poor
    - Can use hybrid approach (local + web)
    - Graceful degradation when no good sources available

    Quality Assessment:
    - HIGH: Documents directly answer question -> Use local only
    - MEDIUM: Documents partially relevant -> Use local + web
    - LOW: Documents not relevant -> Use web only (if enabled)

    Performance:
    - Similar to Self-RAG for local-only
    - 2-3x slower if web fallback is used
    - Higher reliability through multiple source types

    Example:
        >>> # Enable web search with DuckDuckGo
        >>> crag = CRAG(
        ...     local_search,
        ...     None,
        ...     ollama_client,
        ...     enable_web_search=True
        ... )
        >>>
        >>> # With web fallback enabled
        >>> result = await crag.ask_with_correction(
        ...     "Latest AI research trends",
        ...     enable_web_fallback=True
        ... )
        >>>
        >>> # Check if fallback was used
        >>> if result.fallback_used:
        ...     print("Used web search fallback")
    """

    def __init__(
        self,
        local_search,
        web_search: Optional[Any],
        ollama_client,
        enable_web_search: bool = False,
    ):
        """
        Initialize CRAG.

        Args:
            local_search: SearchEngine for local knowledge base
            web_search: Optional web search engine (can be None, deprecated)
            ollama_client: HTTP client for Ollama LLM API
            enable_web_search: Whether to enable web search fallback (requires duckduckgo-search)
        """
        self.local_search = local_search
        self.web_search = web_search  # Keep for backward compatibility
        self.ollama = ollama_client
        self.enable_web_search = enable_web_search
        self._ddgs = None

        if enable_web_search:
            try:
                from duckduckgo_search import DDGS

                self._ddgs = DDGS()
                logger.info("Web search enabled with DuckDuckGo")
            except ImportError:
                logger.warning(
                    "Web search requested but duckduckgo-search not installed. "
                    "Install with: pip install duckduckgo-search"
                )
                self.enable_web_search = False

    async def ask_with_correction(
        self, question: str, enable_web_fallback: bool = False, top_k: int = 5
    ) -> AgenticResult:
        """
        Ask with corrective retrieval strategy.

        Pipeline:
        1. Try local knowledge base
        2. Assess document quality
        3. Based on quality:
           - HIGH: Generate from local docs
           - MEDIUM: Supplement with web (if enabled)
           - LOW: Use web only (if enabled)
        4. Return answer with source attribution

        Args:
            question: Question to answer
            enable_web_fallback: Whether to use web search as fallback
            top_k: Number of documents to retrieve

        Returns:
            AgenticResult with answer and source metadata

        Example:
            >>> # Local only (default)
            >>> result = await crag.ask_with_correction(
            ...     "What is in my notes about Python?"
            ... )
            >>>
            >>> # With web fallback for external knowledge
            >>> result = await crag.ask_with_correction(
            ...     "What's the latest version of Python?",
            ...     enable_web_fallback=True
            ... )
        """
        start_time = time.time()

        # 1. Try local knowledge base
        logger.info(f"Searching local knowledge base: {question}")
        try:
            local_results = await self.local_search.search(question, limit=top_k)
        except Exception as e:
            logger.error(f"Local search failed: {e}")
            local_results = []

        local_docs = [
            {
                "file_path": result.file_path,
                "content": result.content,
                "snippet": result.snippet,
                "score": result.score,
            }
            for result in local_results
        ]

        # 2. Assess quality
        quality = await self._assess_doc_quality(question, local_docs)
        logger.info(f"Local document quality: {quality.value}")

        # 3. Decide on retrieval strategy based on quality
        if quality == DocumentQuality.HIGH:
            # Local docs are excellent, use them
            logger.info("Using local documents (high quality)")
            result = await self._generate_answer(question, local_docs, source="local")
            result.execution_time_ms = (time.time() - start_time) * 1000
            return result

        elif (
            quality == DocumentQuality.MEDIUM
            and enable_web_fallback
            and self.enable_web_search
        ):
            # Supplement with web search
            logger.info("Supplementing with web search (medium quality)")
            web_docs = await self._search_web(question, top_k=3)

            combined_docs = local_docs + web_docs
            result = await self._generate_answer(
                question, combined_docs, source="hybrid"
            )
            result.fallback_used = True
            result.execution_time_ms = (time.time() - start_time) * 1000
            return result

        elif (
            quality == DocumentQuality.LOW
            and enable_web_fallback
            and self.enable_web_search
        ):
            # Local docs poor quality, use web only
            logger.info("Using web search only (low local quality)")
            web_docs = await self._search_web(question, top_k=top_k)

            result = await self._generate_answer(question, web_docs, source="web")
            result.fallback_used = True
            result.execution_time_ms = (time.time() - start_time) * 1000
            return result

        else:
            # No good sources available
            logger.warning(f"No good sources found for: {question}")
            execution_time = (time.time() - start_time) * 1000

            return AgenticResult(
                answer="I don't have enough information to answer this question reliably. The available documents don't contain relevant information.",
                mode=RAGMode.CRAG,
                confidence=0.20,
                citations=[],
                sources=local_docs,
                iterations=1,
                fallback_used=False,
                execution_time_ms=execution_time,
                metadata={
                    "source": "none",
                    "quality": quality.value,
                    "web_fallback_enabled": enable_web_fallback,
                },
            )

    async def _assess_doc_quality(
        self, question: str, docs: list[dict[str, Any]]
    ) -> DocumentQuality:
        """Assess overall quality of documents for answering question."""

        if not docs:
            return DocumentQuality.LOW

        # Use LLM to assess quality
        docs_summary = "\n".join(
            [
                f"- Score: {doc['score']:.2f} | {doc['snippet'][:150]}..."
                for doc in docs[:3]
            ]
        )

        prompt = f"""Question: {question}

Retrieved Documents:
{docs_summary}

Assess the overall quality of these documents for answering the question.

Quality levels:
- high: Documents directly answer the question with clear information
- medium: Documents have some relevant information but may need supplementation
- low: Documents don't contain information to answer the question

Answer with just one word (high, medium, or low):"""

        try:
            response = await self._call_ollama(prompt, max_tokens=10)
            quality_str = response.strip().lower()

            # Parse quality
            if "high" in quality_str:
                return DocumentQuality.HIGH
            elif "medium" in quality_str:
                return DocumentQuality.MEDIUM
            else:
                return DocumentQuality.LOW

        except Exception as e:
            logger.error(f"Failed to assess quality: {e}")
            # Default based on scores
            if docs and docs[0]["score"] > 0.8:
                return DocumentQuality.HIGH
            elif docs and docs[0]["score"] > 0.5:
                return DocumentQuality.MEDIUM
            else:
                return DocumentQuality.LOW

    async def _search_web(self, question: str, top_k: int = 3) -> list[dict[str, Any]]:
        """
        Search web for additional information using DuckDuckGo.

        Args:
            question: Search query
            top_k: Number of results to retrieve

        Returns:
            List of document dictionaries with web search results
        """
        if not self.enable_web_search or not self._ddgs:
            logger.info("Web search disabled, skipping")
            return []

        try:
            logger.info(f"Searching web: {question}")

            # Perform synchronous web search (DuckDuckGo doesn't have async API)
            import asyncio

            results = await asyncio.to_thread(
                lambda: list(self._ddgs.text(question, max_results=top_k))
            )

            # Convert to our document format
            web_docs = []
            for i, result in enumerate(results):
                web_docs.append(
                    {
                        "file_path": result.get("link", f"web_result_{i}"),
                        "content": result.get("body", ""),
                        "snippet": result.get("body", "")[:200],
                        "score": 0.7,  # Web results get moderate score
                    }
                )

            logger.info(f"Retrieved {len(web_docs)} web results")
            return web_docs

        except Exception as e:
            logger.error(f"Web search failed: {e}")
            return []

    async def _generate_answer(
        self, question: str, docs: list[dict[str, Any]], source: str
    ) -> AgenticResult:
        """Generate answer from documents."""

        # Build context
        context = ""
        for i, doc in enumerate(docs, 1):
            context += f"\n\n[{i}] {doc['file_path']}\n{doc['content'][:1000]}"

        prompt = f"""Use the following sources to answer the question.

Sources:{context}

Question: {question}

Answer:"""

        try:
            answer = await self._call_ollama(prompt, max_tokens=500)

            # Extract citations (simple version - look for [1], [2] patterns)
            import re

            citation_numbers = re.findall(r"\[(\d+)\]", answer)
            unique_citations = sorted(
                set(int(n) for n in citation_numbers if n.isdigit())
            )

            citations = []
            for cite_id in unique_citations:
                if 0 < cite_id <= len(docs):
                    doc = docs[cite_id - 1]
                    citations.append(
                        SourceCitation(
                            file_path=doc["file_path"],
                            snippet=doc["snippet"],
                            score=doc["score"],
                            citation_id=cite_id,
                        )
                    )

            # Calculate confidence based on source and doc scores
            avg_score = (
                sum(d["score"] for d in docs[:3]) / min(3, len(docs)) if docs else 0.0
            )
            confidence = round(avg_score * 0.9, 2)  # Slight discount

            return AgenticResult(
                answer=answer,
                mode=RAGMode.CRAG,
                confidence=confidence,
                citations=citations,
                sources=docs,
                iterations=1,
                metadata={"source": source},
            )

        except Exception as e:
            logger.error(f"Failed to generate answer: {e}")
            return AgenticResult(
                answer="Failed to generate answer due to an error.",
                mode=RAGMode.CRAG,
                confidence=0.10,
                citations=[],
                sources=docs,
                iterations=1,
                metadata={"source": source, "error": str(e)},
            )

    async def _call_ollama(self, prompt: str, max_tokens: int = 500) -> str:
        """Call Ollama API to generate response."""
        try:
            response = await self.ollama.post(
                "/api/generate",
                json={
                    "model": "llama3.1:8b",
                    "prompt": prompt,
                    "stream": False,
                    "options": {"num_predict": max_tokens, "temperature": 0.3},
                },
                timeout=30.0,
            )
            response.raise_for_status()
            data = response.json()
            return data.get("response", "")

        except Exception as e:
            logger.error(f"Ollama API call failed: {e}")
            raise


__all__ = ["CRAG"]
