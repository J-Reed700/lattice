"""Shared prompt templates for RAG and Q&A.

This module provides reusable prompt templates used across:
- LLM services (orchestration layer)
- RAG engines (domain logic layer)
"""

from __future__ import annotations

from src.modules.search_engine import SearchResult


def create_context_from_results(results: list[SearchResult]) -> str:
    """Build context string from search results.

    Args:
        results: List of search results

    Returns:
        Formatted context string for LLM
    """
    if not results:
        return "No relevant documents found."

    context_parts = []
    for i, result in enumerate(results, 1):
        context_parts.append(
            f"[Document {i}]\n"
            f"File: {result.filename}\n"
            f"Content: {result.snippet}\n"
        )

    return "\n\n".join(context_parts)


def create_rag_prompt(question: str, context: str) -> str:
    """Create RAG prompt with question and context.

    Args:
        question: User's question
        context: Context from search results

    Returns:
        Formatted prompt for LLM
    """
    return f"""Use the following context to answer the question. If the answer is not in the context, say "I don't have enough information to answer this question."

Context:
{context}

Question: {question}

Answer:"""


def create_rag_system_prompt() -> str:
    """Create system prompt for RAG.

    Returns:
        System prompt string
    """
    return """You are a helpful assistant that answers questions based on the provided context.
Always base your answers on the context provided. If the context doesn't contain the information needed,
say so clearly. Be concise and accurate."""


__all__ = [
    "create_context_from_results",
    "create_rag_prompt",
    "create_rag_system_prompt",
]
