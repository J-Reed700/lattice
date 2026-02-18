"""
Multi-Step RAG: Complex Question Decomposition

Implements multi-step reasoning pattern where complex questions are:
1. Decomposed into simpler sub-questions
2. Each sub-question answered independently
3. Sub-answers synthesized into comprehensive final answer

This is cutting-edge 2025 technology for handling complex, multi-faceted questions.

Basic Usage:
    >>> from src.modules.rag_engine.multi_step import MultiStepRAG
    >>>
    >>> multi_step = MultiStepRAG(search_engine, ollama_client)
    >>>
    >>> result = await multi_step.ask_multi_step(
    ...     "How do neural networks learn and what are their limitations?"
    ... )
    >>> print(f"Reasoning steps: {len(result.reasoning_steps)}")
    >>> for step in result.reasoning_steps:
    ...     print(f"Q: {step.question}")
    ...     print(f"A: {step.answer}")
"""

import logging
import time
from typing import Any

import httpx

from src.modules.rag_engine.types import (
    AgenticResult,
    QueryDecompositionError,
    RAGMode,
    ReasoningStep,
    SourceCitation,
)

logger = logging.getLogger(__name__)


class MultiStepRAG:
    """
    Multi-Step RAG: Decompose complex questions into sub-questions.

    Key capabilities:
    - Automatically detects complex questions
    - Breaks down into 2-4 simpler sub-questions
    - Answers each sub-question with retrieval
    - Synthesizes sub-answers into comprehensive final answer
    - Provides transparent reasoning chain

    Best for:
    - Complex questions with multiple parts
    - Questions requiring comparison or analysis
    - Questions needing information synthesis

    Performance:
    - 3-5x latency vs standard RAG (multiple searches)
    - Higher quality for complex questions
    - Transparent reasoning process

    Example:
        >>> multi_step = MultiStepRAG(search_engine, ollama_client)
        >>>
        >>> # Complex question
        >>> result = await multi_step.ask_multi_step(
        ...     "Compare supervised and unsupervised learning"
        ... )
        >>>
        >>> # See reasoning steps
        >>> for i, step in enumerate(result.reasoning_steps):
        ...     print(f"Step {i+1}: {step.question}")
        ...     print(f"Answer: {step.answer[:100]}...")
    """

    def __init__(self, search_engine, ollama_client):
        """
        Initialize Multi-Step RAG.

        Args:
            search_engine: SearchEngine instance for document retrieval
            ollama_client: HTTP client for Ollama LLM API
        """
        self.search = search_engine
        self.ollama = ollama_client

    async def ask_multi_step(
        self, question: str, max_sub_questions: int = 4, top_k_per_step: int = 3
    ) -> AgenticResult:
        """
        Answer complex question via multi-step reasoning.

        Pipeline:
        1. Decompose question into sub-questions
        2. For each sub-question:
           a. Retrieve relevant documents
           b. Generate answer
        3. Synthesize all sub-answers into final answer
        4. Return result with reasoning chain

        Args:
            question: Complex question to answer
            max_sub_questions: Maximum number of sub-questions
            top_k_per_step: Documents to retrieve per sub-question

        Returns:
            AgenticResult with reasoning steps and final answer

        Raises:
            QueryDecompositionError: If question decomposition fails

        Example:
            >>> result = await multi_step.ask_multi_step(
            ...     "What are the pros and cons of microservices vs monoliths?"
            ... )
            >>> # Check reasoning
            >>> for step in result.reasoning_steps:
            ...     print(f"Q: {step.question}")
            ...     print(f"Sources: {len(step.sources)}")
        """
        start_time = time.time()

        # 1. Decompose into sub-questions
        logger.info(f"Decomposing question: {question}")
        sub_questions = await self._decompose_question(question, max_sub_questions)

        if not sub_questions:
            raise QueryDecompositionError("Failed to decompose question")

        logger.info(f"Generated {len(sub_questions)} sub-questions")

        # 2. Answer each sub-question
        reasoning_steps = []
        all_sources = []
        all_citations = []
        citation_counter = 0

        for i, sub_q in enumerate(sub_questions):
            logger.info(f"Answering sub-question {i+1}/{len(sub_questions)}: {sub_q}")

            # Retrieve documents for this sub-question
            try:
                search_results = await self.search.search(sub_q, top_k=top_k_per_step)
            except Exception as e:
                logger.error(f"Search failed for sub-question: {e}")
                search_results = []

            docs = [
                {
                    "file_path": result.file_path,
                    "content": result.content,
                    "snippet": result.snippet,
                    "score": result.score,
                }
                for result in search_results
            ]

            # Generate answer for sub-question
            sub_answer, sub_confidence = await self._answer_sub_question(sub_q, docs)

            # Create reasoning step
            step = ReasoningStep(
                question=sub_q,
                answer=sub_answer,
                sources=docs,
                confidence=sub_confidence,
            )
            reasoning_steps.append(step)

            # Collect sources and citations
            for doc in docs:
                citation_counter += 1
                all_citations.append(
                    SourceCitation(
                        file_path=doc["file_path"],
                        snippet=doc["snippet"],
                        score=doc["score"],
                        citation_id=citation_counter,
                    )
                )
            all_sources.extend(docs)

        # 3. Synthesize final answer
        logger.info("Synthesizing final answer")
        final_answer = await self._synthesize_answers(question, reasoning_steps)

        # Calculate overall confidence (average of sub-question confidences)
        avg_confidence = sum(step.confidence for step in reasoning_steps) / len(
            reasoning_steps
        )
        confidence = round(avg_confidence, 2)

        execution_time = (time.time() - start_time) * 1000

        return AgenticResult(
            answer=final_answer,
            mode=RAGMode.MULTI_STEP,
            confidence=confidence,
            citations=all_citations,
            sources=all_sources,
            iterations=len(sub_questions),
            reasoning_steps=reasoning_steps,
            execution_time_ms=execution_time,
            metadata={
                "num_sub_questions": len(sub_questions),
                "original_question": question,
            },
        )

    async def _decompose_question(
        self, question: str, max_sub_questions: int
    ) -> list[str]:
        """Break complex question into simpler sub-questions."""

        prompt = f"""Break down this complex question into 2-{max_sub_questions} simpler sub-questions that would help answer it.

Each sub-question should:
- Be clear and specific
- Address one aspect of the main question
- Be answerable independently

Question: {question}

Sub-questions (one per line, numbered):
1."""

        try:
            response = await self._call_ollama(prompt, max_tokens=300)

            # Parse numbered list
            lines = response.strip().split("\n")
            sub_questions = []

            for line in lines:
                line = line.strip()
                if not line:
                    continue

                # Remove numbering (1., 2), etc.)
                import re

                clean_line = re.sub(r"^\d+[\.\)\-\:]?\s*", "", line)

                if clean_line and not clean_line.startswith("#"):
                    sub_questions.append(clean_line)

            # Limit to max_sub_questions
            sub_questions = sub_questions[:max_sub_questions]

            logger.info(f"Decomposed into sub-questions: {sub_questions}")
            return sub_questions

        except Exception as e:
            logger.error(f"Failed to decompose question: {e}")
            # Fallback: treat original question as single sub-question
            return [question]

    async def _answer_sub_question(
        self, sub_question: str, docs: list[dict[str, Any]]
    ) -> tuple[str, float]:
        """Answer a single sub-question using retrieved documents."""

        if not docs:
            return "No relevant information found.", 0.20

        # Build context from documents
        context = ""
        for i, doc in enumerate(docs, 1):
            context += f"\n\n[{i}] {doc['file_path']}\n{doc['content'][:800]}"

        prompt = f"""Use the following sources to answer the question.

Sources:{context}

Question: {sub_question}

Answer (concise and focused):"""

        try:
            answer = await self._call_ollama(prompt, max_tokens=300)

            # Calculate confidence based on document scores
            if docs:
                avg_score = sum(d["score"] for d in docs) / len(docs)
                confidence = round(avg_score * 0.85, 2)  # Slight discount
            else:
                confidence = 0.20

            return answer.strip(), confidence

        except Exception as e:
            logger.error(f"Failed to answer sub-question: {e}")
            return "Error generating answer.", 0.10

    async def _synthesize_answers(
        self, original_question: str, reasoning_steps: list[ReasoningStep]
    ) -> str:
        """Synthesize sub-answers into comprehensive final answer."""

        # Build summary of sub-QA pairs
        sub_qa_text = ""
        for i, step in enumerate(reasoning_steps, 1):
            sub_qa_text += (
                f"\n\nSub-question {i}: {step.question}\nAnswer: {step.answer}"
            )

        prompt = f"""Original question: {original_question}

I've broken this down into sub-questions and found answers:
{sub_qa_text}

Now synthesize these sub-answers into a comprehensive, well-structured answer to the original question.
The answer should:
- Directly address the original question
- Integrate information from all sub-answers
- Be coherent and well-organized
- Be concise but complete

Final answer:"""

        try:
            final_answer = await self._call_ollama(prompt, max_tokens=600)
            return final_answer.strip()

        except Exception as e:
            logger.error(f"Failed to synthesize answers: {e}")

            # Fallback: concatenate sub-answers
            fallback = "Based on analyzing multiple aspects:\n\n"
            for i, step in enumerate(reasoning_steps, 1):
                fallback += f"{i}. {step.question}\n   {step.answer}\n\n"
            return fallback

    async def _call_ollama(self, prompt: str, max_tokens: int = 500) -> str:
        """Call Ollama API to generate response."""
        try:
            response = await self.ollama.request(
                "POST",
                "/api/generate",
                json={
                    "model": "llama3.1:8b",
                    "prompt": prompt,
                    "stream": False,
                    "options": {
                        "num_predict": max_tokens,
                        "temperature": 0.4,  # Slightly higher for creative synthesis
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


__all__ = ["MultiStepRAG"]
