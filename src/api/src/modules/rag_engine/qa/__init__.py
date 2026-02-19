"""
Q&A Module - RAG-based Question Answering

A complete RAG (Retrieval-Augmented Generation) system that combines semantic
search with LLM generation to answer questions from your knowledge base.

Components:
    - QAEngine: Main question answering engine with streaming support
    - QAError, OllamaUnavailableError, ContextTooLargeError: Exception types
    - SourceReference: Source document reference with attribution
    - count_tokens, truncate_to_tokens: Token management utilities
    - SYSTEM_PROMPT, USER_PROMPT_TEMPLATE: Prompt templates

Basic Usage:
    >>> from src.modules.rag_engine import QAEngine
    >>> from src.modules.search_engine import SearchService
    >>>
    >>> # Initialize
    >>> qa_engine = QAEngine(
    ...     search_service=search_service,
    ...     ollama_url="http://localhost:11434",
    ...     model_name="llama3.1:8b"
    ... )
    >>>
    >>> # Check availability
    >>> if await qa_engine.health_check():
    ...     # Ask question with streaming
    ...     async for chunk in qa_engine.ask("What is machine learning?"):
    ...         print(chunk, end="", flush=True)
"""

from __future__ import annotations

from .engine import QAEngine
from .prompts import SYSTEM_PROMPT, USER_PROMPT_TEMPLATE
from .tokenizer import count_tokens, truncate_to_tokens
from .types import (
    ContextTooLargeError,
    OllamaUnavailableError,
    QAError,
    SourceReference,
)

__all__ = [
    "QAEngine",
    "QAError",
    "OllamaUnavailableError",
    "ContextTooLargeError",
    "SourceReference",
    "count_tokens",
    "truncate_to_tokens",
    "SYSTEM_PROMPT",
    "USER_PROMPT_TEMPLATE",
]
