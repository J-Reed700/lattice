"""LLM service exports with lazy imports."""

from __future__ import annotations

from typing import Any

from .models import (
    OllamaError,
    OllamaGenerateRequest,
    OllamaGenerateResponse,
    OllamaListResponse,
    OllamaModel,
    OllamaStreamResponse,
)
from .ollama_client import (
    OllamaAPIError,
    OllamaClient,
    OllamaConnectionError,
    OllamaTimeoutError,
)
from .ollama_service import OllamaService

__all__ = [
    "LLMService",
    "OllamaAPIError",
    "OllamaClient",
    "OllamaConnectionError",
    "OllamaError",
    "OllamaGenerateRequest",
    "OllamaGenerateResponse",
    "OllamaListResponse",
    "OllamaModel",
    "OllamaService",
    "OllamaStreamResponse",
    "OllamaTimeoutError",
    "create_context_from_results",
    "create_rag_prompt",
    "create_rag_system_prompt",
]


def __getattr__(name: str) -> Any:
    if name == "LLMService":
        from .service import LLMService

        return LLMService

    if name in {
        "create_context_from_results",
        "create_rag_prompt",
        "create_rag_system_prompt",
    }:
        from .prompts import (
            create_context_from_results,
            create_rag_prompt,
            create_rag_system_prompt,
        )

        return {
            "create_context_from_results": create_context_from_results,
            "create_rag_prompt": create_rag_prompt,
            "create_rag_system_prompt": create_rag_system_prompt,
        }[name]

    raise AttributeError(f"module 'src.services.llm' has no attribute '{name}'")
