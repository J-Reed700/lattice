"""
Q&A Module Types

Data classes and exceptions for the Q&A engine module.

Basic Usage:
    >>> from src.modules.rag_engine.qa.types import SourceReference, QAError
    >>> ref = SourceReference(
    ...     file_path="C:\\Documents\\notes.txt",
    ...     score=0.87,
    ...     snippet="Relevant context from the document"
    ... )
"""

from __future__ import annotations

from dataclasses import dataclass


class QAError(Exception):
    """Base exception for Q&A errors.

    All Q&A-related errors inherit from this exception.
    Catch this to handle all Q&A module errors.

    Example:
        >>> try:
        ...     answer = await qa_engine.ask("What is...")
        ... except QAError as e:
        ...     print(f"Q&A failed: {e}")
    """



class OllamaUnavailableError(QAError):
    """Ollama service is not available.

    Raised when the Ollama API cannot be reached or is not running.
    Usually indicates the Ollama service needs to be started.

    Example:
        >>> try:
        ...     await qa_engine.health_check()
        ... except OllamaUnavailableError:
        ...     print("Please start Ollama: ollama serve")
    """



class ContextTooLargeError(QAError):
    """Context exceeds token limit.

    Raised when the combined search results exceed the maximum
    context size allowed for the LLM.

    Example:
        >>> try:
        ...     answer = await qa_engine.ask(question, max_context_tokens=1000)
        ... except ContextTooLargeError:
        ...     answer = await qa_engine.ask(question, top_k=3)
    """



@dataclass
class SourceReference:
    """Reference to a source document used in generating an answer.

    Tracks which documents contributed to the answer and their
    relevance scores for attribution and verification.

    Attributes:
        file_path: Absolute path to the source document
        score: Relevance score (0.0 to 1.0, higher is better)
        snippet: Relevant text excerpt from the document

    Example:
        >>> ref = SourceReference(
        ...     file_path="C:\\Projects\\docs\\architecture.md",
        ...     score=0.92,
        ...     snippet="The system uses a microservices architecture..."
        ... )
        >>> print(f"Source: {ref.file_path} (score: {ref.score:.2f})")
        >>> print(f"Excerpt: {ref.snippet[:100]}...")
    """

    file_path: str
    score: float
    snippet: str


__all__ = [
    "QAError",
    "OllamaUnavailableError",
    "ContextTooLargeError",
    "SourceReference",
]
