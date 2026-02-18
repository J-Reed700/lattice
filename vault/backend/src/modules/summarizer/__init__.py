"""
Document Summarization Module

Privacy-preserving on-device summarization using Small Language Models.
See README.md for complete contract specification.

Public exports:
- SummarizerService: Main service for summarization
- ModelManager: Manage model downloads and caching
- SummaryType: Enum of summary types
- Summary: Summary result model
- SummaryRequest: Summary request model
- SummarizerConfig: Configuration model
"""

from .model_manager import ModelDownloadError, ModelManager, ModelNotFoundError
from .prompts import PromptBuilder
from .service import LanguageNotSupportedError, SummarizationError, SummarizerService
from .types import (
    BatchSummaryRequest,
    BatchSummaryResponse,
    ModelDownloadProgress,
    ModelInfo,
    ModelQuantization,
    SummarizerConfig,
    Summary,
    SummaryRequest,
    SummaryType,
)

__all__ = [
    # Main service
    "SummarizerService",
    "ModelManager",
    "PromptBuilder",
    # Types
    "SummaryType",
    "Summary",
    "SummaryRequest",
    "BatchSummaryRequest",
    "BatchSummaryResponse",
    "ModelInfo",
    "ModelQuantization",
    "ModelDownloadProgress",
    "SummarizerConfig",
    # Exceptions
    "SummarizationError",
    "LanguageNotSupportedError",
    "ModelNotFoundError",
    "ModelDownloadError",
]
