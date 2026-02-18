"""
Type definitions for the summarizer module.

This module defines all data structures and enums used for document summarization.
"""

from datetime import UTC, datetime
from enum import Enum

from pydantic import BaseModel, Field, field_validator


class SummaryType(str, Enum):
    """Types of summaries that can be generated."""

    EXTRACTIVE = "extractive"  # Key sentences from original
    ABSTRACTIVE = "abstractive"  # Generated natural language
    BULLET_POINTS = "bullet_points"  # Concise bullet list
    TLDR = "tldr"  # Very short (1-2 sentences)
    DETAILED = "detailed"  # Comprehensive multi-paragraph
    CUSTOM = "custom"  # User-specified length


class ModelQuantization(str, Enum):
    """Available quantization levels for models."""

    Q4_K_M = "Q4_K_M"  # 4-bit, medium quality (recommended)
    Q5_K_M = "Q5_K_M"  # 5-bit, higher quality
    Q8_0 = "Q8_0"  # 8-bit, highest quality


class ModelInfo(BaseModel):
    """Information about an available model.

    Attributes:
        name: Model identifier (e.g., "phi-3.5-mini")
        full_name: Complete model name
        size_gb: Download size in GB
        parameters: Model parameter count (in billions)
        quantization: Quantization level
        languages: Supported languages
        downloaded: Whether model is downloaded locally
        path: Local path if downloaded

    Example:
        >>> model = ModelInfo(
        ...     name="phi-3.5-mini",
        ...     full_name="Phi-3.5-mini-instruct",
        ...     size_gb=2.3,
        ...     parameters=3.8,
        ...     quantization=ModelQuantization.Q4_K_M
        ... )
    """

    name: str
    full_name: str
    size_gb: float
    parameters: float
    quantization: ModelQuantization
    languages: list[str] = Field(default_factory=lambda: ["en"])
    downloaded: bool = False
    path: str | None = None
    download_url: str | None = None
    description: str | None = None

    class Config:
        json_schema_extra = {
            "example": {
                "name": "phi-3.5-mini",
                "full_name": "Phi-3.5-mini-instruct-Q4_K_M",
                "size_gb": 2.3,
                "parameters": 3.8,
                "quantization": "Q4_K_M",
                "languages": ["en"],
                "downloaded": True,
                "path": "/models/phi-3.5-mini-Q4_K_M.gguf",
            }
        }


class SummaryRequest(BaseModel):
    """Request to generate a summary.

    Attributes:
        text: Input text to summarize (100-50000 chars)
        summary_type: Type of summary to generate
        max_words: Target length in words (50-500)
        model: Model identifier to use
        language: Language code (auto-detect if None)
        stream: Whether to stream response chunks
        cache_key: Optional key for caching result

    Example:
        >>> request = SummaryRequest(
        ...     text="Long document text here...",
        ...     summary_type=SummaryType.BULLET_POINTS,
        ...     max_words=100,
        ...     model="phi-3.5-mini"
        ... )
    """

    text: str = Field(min_length=100, max_length=50000, description="Text to summarize")
    summary_type: SummaryType = Field(
        default=SummaryType.ABSTRACTIVE, description="Type of summary"
    )
    max_words: int = Field(default=150, ge=50, le=500, description="Target word count")
    model: str = Field(default="phi-3.5-mini", description="Model to use")
    language: str | None = Field(default=None, description="Language code (auto-detect if None)")
    stream: bool = Field(default=False, description="Stream response chunks")
    cache_key: str | None = Field(default=None, description="Cache key for result")

    @field_validator("text")
    @classmethod
    def validate_text_not_empty(cls, v: str) -> str:
        """Ensure text is not just whitespace."""
        if not v.strip():
            raise ValueError("Text cannot be empty or whitespace only")
        return v.strip()

    class Config:
        json_schema_extra = {
            "example": {
                "text": "This is a long document that needs to be summarized...",
                "summary_type": "bullet_points",
                "max_words": 100,
                "model": "phi-3.5-mini",
                "stream": False,
            }
        }


class Summary(BaseModel):
    """Generated summary result.

    Attributes:
        id: Unique identifier
        text: Generated summary text
        summary_type: Type of summary generated
        source_length: Input character count
        summary_length: Output character count
        compression_ratio: Ratio of source to summary
        model: Model used for generation
        language: Detected or specified language
        generation_time: Time taken in seconds
        tokens_per_second: Generation speed metric
        created_at: Timestamp of creation
        cache_hit: Whether result came from cache

    Example:
        >>> summary = Summary(
        ...     id="abc123",
        ...     text="Key points: 1. Topic A, 2. Topic B, 3. Topic C",
        ...     summary_type=SummaryType.BULLET_POINTS,
        ...     source_length=5000,
        ...     summary_length=150
        ... )
    """

    id: str
    text: str
    summary_type: SummaryType
    source_length: int
    summary_length: int
    compression_ratio: float
    model: str
    language: str
    generation_time: float
    tokens_per_second: float
    created_at: datetime = Field(default_factory=lambda: datetime.now(UTC))
    cache_hit: bool = False
    metadata: dict = Field(default_factory=dict)

    class Config:
        json_schema_extra = {
            "example": {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "text": "• Main point 1\n• Main point 2\n• Main point 3",
                "summary_type": "bullet_points",
                "source_length": 5000,
                "summary_length": 150,
                "compression_ratio": 33.3,
                "model": "phi-3.5-mini",
                "language": "en",
                "generation_time": 4.5,
                "tokens_per_second": 12.3,
                "created_at": "2025-01-10T12:00:00Z",
                "cache_hit": False,
            }
        }


class BatchSummaryRequest(BaseModel):
    """Request to summarize multiple texts.

    Attributes:
        texts: List of texts to summarize
        summary_type: Type of summary for all texts
        max_words: Target length for all summaries
        model: Model to use for all
        parallel: Process in parallel (default: True)

    Example:
        >>> request = BatchSummaryRequest(
        ...     texts=["Document 1...", "Document 2..."],
        ...     summary_type=SummaryType.TLDR,
        ...     max_words=50
        ... )
    """

    texts: list[str] = Field(min_length=1, max_length=100, description="Texts to summarize")
    summary_type: SummaryType = Field(default=SummaryType.ABSTRACTIVE)
    max_words: int = Field(default=150, ge=50, le=500)
    model: str = Field(default="phi-3.5-mini")
    parallel: bool = Field(default=True, description="Process in parallel")

    @field_validator("texts")
    @classmethod
    def validate_texts(cls, v: list[str]) -> list[str]:
        """Validate each text in the list."""
        for text in v:
            if len(text.strip()) < 100:
                raise ValueError("Each text must be at least 100 characters")
        return v


class BatchSummaryResponse(BaseModel):
    """Response from batch summarization.

    Attributes:
        summaries: Generated summaries
        total_count: Total number of texts
        success_count: Successfully summarized
        failed_count: Failed to summarize
        total_time: Total processing time
        errors: List of errors for failed items
    """

    summaries: list[Summary]
    total_count: int
    success_count: int
    failed_count: int
    total_time: float
    errors: list[dict] = Field(default_factory=list)

    class Config:
        json_schema_extra = {
            "example": {
                "summaries": [],
                "total_count": 10,
                "success_count": 9,
                "failed_count": 1,
                "total_time": 45.2,
                "errors": [{"index": 5, "error": "Text too short"}],
            }
        }


class ModelDownloadProgress(BaseModel):
    """Progress update during model download.

    Attributes:
        model_name: Model being downloaded
        bytes_downloaded: Bytes downloaded so far
        total_bytes: Total file size
        progress_percent: Percentage complete
        speed_mbps: Download speed in MB/s
        eta_seconds: Estimated time remaining
    """

    model_name: str
    bytes_downloaded: int
    total_bytes: int
    progress_percent: float
    speed_mbps: float
    eta_seconds: float

    class Config:
        json_schema_extra = {
            "example": {
                "model_name": "phi-3.5-mini",
                "bytes_downloaded": 1200000000,
                "total_bytes": 2400000000,
                "progress_percent": 50.0,
                "speed_mbps": 5.2,
                "eta_seconds": 230,
            }
        }


class SummarizerConfig(BaseModel):
    """Configuration for summarizer service.

    Attributes:
        default_model: Default model to use
        quantization: Default quantization level
        max_context_length: Model context window
        temperature: Sampling temperature
        max_tokens: Maximum output tokens
        gpu_layers: GPU layers (-1 = all)
        threads: CPU threads to use
        batch_size: Inference batch size
        model_cache_dir: Directory for models
        enable_caching: Enable summary caching
        cache_ttl: Cache TTL in seconds
    """

    default_model: str = "phi-3.5-mini"
    quantization: ModelQuantization = ModelQuantization.Q4_K_M
    max_context_length: int = 4096
    temperature: float = 0.3
    max_tokens: int = 512
    gpu_layers: int = -1
    threads: int = 4
    batch_size: int = 512
    model_cache_dir: str = "models/summarization"
    enable_caching: bool = True
    cache_ttl: int = 86400  # 24 hours

    class Config:
        json_schema_extra = {
            "example": {
                "default_model": "phi-3.5-mini",
                "quantization": "Q4_K_M",
                "temperature": 0.3,
                "gpu_layers": -1,
                "threads": 4,
            }
        }
