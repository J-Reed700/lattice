"""
Self-RAG: Self-Reflective Retrieval-Augmented Generation

Implements Self-RAG pattern where the LLM:
1. Assesses if retrieved documents are relevant
2. Generates answer with inline citations
3. Self-reflects on whether answer is supported by sources
4. Retries with query rewriting if quality is low

This is cutting-edge 2025 technology for verified, citation-backed answers.

Basic Usage:
    >>> from src.modules.rag_engine.self_rag import SelfRAG
    >>>
    >>> self_rag = SelfRAG(search_engine, ollama_client)
    >>>
    >>> result = await self_rag.ask_with_reflection(
    ...     question="What is machine learning?",
    ...     max_iterations=3
    ... )
    >>> print(f"Answer: {result.answer}")
    >>> print(f"Confidence: {result.confidence}")
    >>> print(f"Verified: {result.answer_assessment}")
"""

import logging
import re
import time
from typing import Any

import httpx

from src.modules.rag_engine.types import (
    AgenticResult,
    AnswerAssessment,
    RAGMode,
    RetrievalAssessment,
    RetrievalFailedError,
    SourceCitation,
)

logger = logging.getLogger(__name__)


class SelfRAG:
    """
    Self-RAG: Self-reflective retrieval with answer verification.

    Key capabilities:
    - Assesses retrieval relevance before generation
    - Generates answers with inline citations [1], [2]
    - Verifies answer is grounded in sources
    - Rewrites query if retrieval/answer quality is poor
    - Iterates up to max_iterations to get high-quality answer

    Performance:
    - 2-3x latency vs standard RAG (multiple LLM calls)
    - Higher quality: answers are citation-backed and verified
    - Self-correcting: automatically improves via query rewriting

    Example:
        >>> self_rag = SelfRAG(search_engine, ollama_client)
        >>>
        >>> # Basic usage
        >>> result = await self_rag.ask_with_reflection(
        ...     "What is supervised learning?"
        ... )
        >>>
        >>> # Check verification status
        >>> if result.answer_assessment == AnswerAssessment.FULLY_SUPPORTED:
        ...     print("Answer is fully verified!")
    """

    def __init__(self, search_engine, ollama_client):
        """
        Initialize Self-RAG.

        Args:
            search_engine: SearchEngine instance for document retrieval
            ollama_client: HTTP client for Ollama LLM API
        """
        self.search = search_engine
        self.ollama = ollama_client

    async def ask_with_reflection(
        self, question: str, max_iterations: int = 3, top_k: int = 5
    ) -> AgenticResult:
        """
        Ask question with self-reflection and iterative refinement.

        Pipeline:
        1. Retrieve documents
        2. Assess retrieval relevance
        3. If not relevant, rewrite query and retry
        4. Generate answer with citations
        5. Assess answer support in sources
        6. If not supported, rewrite query and retry
        7. Return verified result

        Args:
            question: Question to answer
            max_iterations: Max retrieval/generation attempts
            top_k: Number of documents to retrieve

        Returns:
            AgenticResult with verified answer and metadata

        Raises:
            RetrievalFailedError: If unable to find relevant docs after max_iterations

        Example:
            >>> result = await self_rag.ask_with_reflection(
            ...     "What is deep learning?",
            ...     max_iterations=3,
            ...     top_k=5
            ... )
            >>> print(f"Confidence: {result.confidence}")
            >>> print(f"Iterations needed: {result.iterations}")
        """
        start_time = time.time()
        original_question = question

        for iteration in range(max_iterations):
            logger.info(f"Self-RAG iteration {iteration + 1}/{max_iterations}")
            logger.info(f"Query: {question}")

            # 1. Retrieve documents
            try:
                search_results = await self.search.search(question, top_k=top_k)
            except Exception as e:
                logger.error(f"Search failed: {e}")
                if iteration == max_iterations - 1:
                    raise RetrievalFailedError(
                        f"Search failed after {max_iterations} attempts"
                    ) from e
                continue

            # Convert to list of dicts
            docs = [
                {
                    "file_path": result.file_path,
                    "content": result.content,
                    "snippet": result.snippet,
                    "score": result.score,
                }
                for result in search_results
            ]

            if not docs:
                logger.warning(f"No documents found for query: {question}")
                if iteration == max_iterations - 1:
                    return self._create_no_info_result(
                        original_question, max_iterations, time.time() - start_time
                    )
                question = await self._rewrite_query(question, iteration)
                continue

            # 2. Assess retrieval relevance
            relevance = await self._assess_retrieval_relevance(question, docs)
            logger.info(f"Retrieval assessment: {relevance.value}")

            if relevance == RetrievalAssessment.NOT_RELEVANT:
                logger.info("Documents not relevant, rewriting query")
                question = await self._rewrite_query(question, iteration)
                continue

            # 3. Generate answer with citations
            answer, citations = await self._generate_with_citations(question, docs)

            # 4. Self-reflection: Is answer supported by sources?
            support = await self._assess_answer_support(answer, docs)
            logger.info(f"Answer assessment: {support.value}")

            # Calculate confidence based on assessments
            confidence = self._calculate_confidence(relevance, support, iteration)

            if support in [
                AnswerAssessment.FULLY_SUPPORTED,
                AnswerAssessment.PARTIALLY_SUPPORTED,
            ]:
                # Success! Return verified result
                execution_time = (time.time() - start_time) * 1000

                return AgenticResult(
                    answer=answer,
                    mode=RAGMode.SELF_RAG,
                    confidence=confidence,
                    citations=citations,
                    sources=docs,
                    iterations=iteration + 1,
                    retrieval_assessment=relevance,
                    answer_assessment=support,
                    execution_time_ms=execution_time,
                    metadata={
                        "original_question": original_question,
                        "final_query": question
                        if question != original_question
                        else None,
                    },
                )

            # Answer not well-supported, try again with different query
            logger.info("Answer not well-supported, rewriting query")
            question = await self._rewrite_query(question, iteration)

        # Max iterations reached without satisfactory answer
        execution_time = (time.time() - start_time) * 1000
        return self._create_no_info_result(
            original_question, max_iterations, execution_time
        )

    async def _assess_retrieval_relevance(
        self, question: str, docs: list[dict[str, Any]]
    ) -> RetrievalAssessment:
        """Use LLM to assess if retrieved documents are relevant to question."""

        # Take top 3 docs for assessment (reduce tokens)
        docs_summary = "\n".join([f"- {doc['snippet'][:200]}..." for doc in docs[:3]])

        prompt = f"""Question: {question}

Retrieved Documents:
{docs_summary}

Rate how relevant these documents are to answering the question.

Options:
- highly_relevant: Documents directly answer the question
- relevant: Documents contain useful information
- partially_relevant: Documents have some related information
- not_relevant: Documents don't help answer the question

Answer with just one word (the rating):"""

        try:
            response = await self._call_ollama(prompt, max_tokens=20)
            rating = response.strip().lower().replace("_", "_")

            # Parse rating
            for assessment in RetrievalAssessment:
                if assessment.value in rating:
                    return assessment

            # Default to relevant if can't parse
            return RetrievalAssessment.RELEVANT

        except Exception as e:
            logger.error(f"Failed to assess retrieval: {e}")
            return RetrievalAssessment.RELEVANT

    async def _assess_answer_support(
        self, answer: str, docs: list[dict[str, Any]]
    ) -> AnswerAssessment:
        """Check if answer is supported by source documents."""

        # Combine source content (limit to reduce tokens)
        sources_text = "\n\n".join(
            [f"Source {i+1}: {doc['content'][:500]}" for i, doc in enumerate(docs[:3])]
        )

        prompt = f"""Answer: {answer}

Source Documents:
{sources_text}

Verify if this answer is supported by the source documents.

Check:
1. Are all claims backed by the sources?
2. Does it contradict any sources?
3. Does it add information not in sources?

Rate the answer:
- fully_supported: All claims backed by sources
- partially_supported: Some claims backed, some not
- not_supported: Claims not found in sources
- hallucination: Answer contradicts sources

Answer with just one word (the rating):"""

        try:
            response = await self._call_ollama(prompt, max_tokens=30)
            rating = response.strip().lower().replace("_", "_")

            # Parse rating
            for assessment in AnswerAssessment:
                if assessment.value in rating:
                    return assessment

            # Default to partially supported if can't parse
            return AnswerAssessment.PARTIALLY_SUPPORTED

        except Exception as e:
            logger.error(f"Failed to assess answer support: {e}")
            return AnswerAssessment.PARTIALLY_SUPPORTED

    async def _generate_with_citations(
        self, question: str, docs: list[dict[str, Any]]
    ) -> tuple[str, list[SourceCitation]]:
        """Generate answer with inline citations [1], [2]."""

        # Build context with numbered sources
        context = ""
        for i, doc in enumerate(docs, 1):
            context += f"\n\n[{i}] {doc['file_path']}\n{doc['content'][:1000]}"

        prompt = f"""Use the following sources to answer the question. Include inline citations [1], [2] for each claim.

Sources:{context}

Question: {question}

Answer with citations (use [1], [2], etc. to cite sources):"""

        try:
            answer = await self._call_ollama(prompt, max_tokens=500)

            # Extract citation numbers from answer
            citation_numbers = re.findall(r"\[(\d+)\]", answer)
            unique_citations = sorted(set(int(n) for n in citation_numbers))

            # Build citation objects
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

            return answer, citations

        except Exception as e:
            logger.error(f"Failed to generate answer: {e}")
            # Return simple answer without citations
            return "I couldn't generate a reliable answer with citations.", []

    async def _rewrite_query(self, original_query: str, iteration: int) -> str:
        """Rewrite query to get better results."""

        strategies = [
            "Make the query more specific and detailed",
            "Try a different phrasing or perspective",
            "Break down into a simpler sub-question",
        ]

        strategy = strategies[min(iteration, len(strategies) - 1)]

        prompt = f"""Original question: {original_query}

The previous search didn't find relevant information.
{strategy}.

Rewritten question (just the question, nothing else):"""

        try:
            rewritten = await self._call_ollama(prompt, max_tokens=100)
            rewritten = rewritten.strip()

            # Remove quotes if present
            rewritten = rewritten.strip("\"'")

            logger.info(f"Rewrote query: {original_query} -> {rewritten}")
            return rewritten

        except Exception as e:
            logger.error(f"Failed to rewrite query: {e}")
            return original_query

    async def _call_ollama(self, prompt: str, max_tokens: int = 500) -> str:
        """Call Ollama API to generate response."""
        try:
            response = await self.ollama.request(
                "POST",
                "/api/generate",
                json={
                    "model": "llama3.1:8b",  # TODO: Make configurable
                    "prompt": prompt,
                    "stream": False,
                    "options": {
                        "num_predict": max_tokens,
                        "temperature": 0.3,  # Lower temp for more factual responses
                    },
                },
                timeout=30.0,
            )
            response.raise_for_status()
            data = response.json()
            return data.get("response", "")

        except httpx.ConnectError as e:
            logger.error(f"Failed to connect to Ollama: {e}")
            raise RuntimeError(
                "Cannot connect to Ollama service. Ensure Ollama is running with 'ollama serve'"
            ) from e
        except httpx.TimeoutException as e:
            logger.error(f"Ollama request timed out: {e}")
            raise RuntimeError("Ollama request timed out after 30 seconds") from e
        except httpx.HTTPStatusError as e:
            logger.error(f"Ollama returned error status {e.response.status_code}: {e}")
            raise RuntimeError(
                f"Ollama API error: {e.response.status_code} - {e.response.text}"
            ) from e
        except Exception as e:
            logger.error(f"Unexpected Ollama API call failure: {e}")
            raise

    def _calculate_confidence(
        self, relevance: RetrievalAssessment, support: AnswerAssessment, iteration: int
    ) -> float:
        """Calculate confidence score based on assessments."""

        # Base confidence from answer support
        support_scores = {
            AnswerAssessment.FULLY_SUPPORTED: 0.95,
            AnswerAssessment.PARTIALLY_SUPPORTED: 0.75,
            AnswerAssessment.NOT_SUPPORTED: 0.50,
            AnswerAssessment.HALLUCINATION: 0.30,
        }
        confidence = support_scores.get(support, 0.50)

        # Adjust based on retrieval relevance
        relevance_multipliers = {
            RetrievalAssessment.HIGHLY_RELEVANT: 1.0,
            RetrievalAssessment.RELEVANT: 0.95,
            RetrievalAssessment.PARTIALLY_RELEVANT: 0.85,
            RetrievalAssessment.NOT_RELEVANT: 0.70,
        }
        confidence *= relevance_multipliers.get(relevance, 0.90)

        # Slight penalty for multiple iterations (uncertainty)
        confidence *= 0.95**iteration

        return round(confidence, 2)

    def _create_no_info_result(
        self, question: str, iterations: int, execution_time: float
    ) -> AgenticResult:
        """Create result when no satisfactory answer found."""
        return AgenticResult(
            answer="I couldn't find sufficient information to answer this question reliably. The retrieved documents were not relevant enough or didn't contain the needed information.",
            mode=RAGMode.SELF_RAG,
            confidence=0.20,
            citations=[],
            sources=[],
            iterations=iterations,
            retrieval_assessment=RetrievalAssessment.NOT_RELEVANT,
            answer_assessment=AnswerAssessment.NOT_SUPPORTED,
            execution_time_ms=execution_time,
            metadata={"reason": "max_iterations_reached"},
        )


__all__ = ["SelfRAG"]
