"""Agentic RAG module exports."""

from __future__ import annotations

from typing import Any

from src.modules.rag_engine.agentic import AgenticRAG
from src.modules.rag_engine.crag import CRAG
from src.modules.rag_engine.deep_research import DeepResearchRAG
from src.modules.rag_engine.multi_step import MultiStepRAG
from src.modules.rag_engine.self_rag import SelfRAG
from src.modules.rag_engine.types import (
    AgenticResult,
    AnswerAssessment,
    RAGMode,
    RetrievalAssessment,
)

__all__ = [
    "AgenticRAG",
    "AgenticRAGService",
    "AgenticResult",
    "AnswerAssessment",
    "ContextTooLargeError",
    "CRAG",
    "DeepResearchRAG",
    "MultiStepRAG",
    "OllamaUnavailableError",
    "QAEngine",
    "QAError",
    "RAGMode",
    "RetrievalAssessment",
    "SelfRAG",
    "SourceReference",
]


def __getattr__(name: str) -> Any:
    if name == "AgenticRAGService":
        from src.modules.rag_engine.agentic_rag import AgenticRAGService

        return AgenticRAGService

    if name in {"QAEngine", "QAError", "OllamaUnavailableError", "ContextTooLargeError"}:
        from src.modules.rag_engine.qa import QAEngine
        from src.modules.rag_engine.qa.types import ContextTooLargeError, OllamaUnavailableError, QAError

        return {
            "QAEngine": QAEngine,
            "QAError": QAError,
            "OllamaUnavailableError": OllamaUnavailableError,
            "ContextTooLargeError": ContextTooLargeError,
        }[name]

    if name == "SourceReference":
        from src.modules.rag_engine.qa.types import SourceReference

        return SourceReference

    raise AttributeError(f"module 'src.modules.rag_engine' has no attribute '{name}'")
