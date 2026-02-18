# Document Summarizer Module

**Module**: Privacy-Preserving On-Device Document Summarization
**Purpose**: Generate summaries using Small Language Models (1-4B params) with 100% local processing
**Contract**: Document/Text → Summary (multiple types and formats)

## Overview

This module provides on-device document summarization using quantized Small Language Models (SLMs) via llama.cpp. All processing happens locally with zero API calls, ensuring complete privacy.

## Core Functionality

### Summarization Types

1. **Extractive** - Key sentences/paragraphs from original text
2. **Abstractive** - Natural language generated summary
3. **Bullet Points** - Concise list format
4. **TL;DR** - Very short (1-2 sentences)
5. **Detailed** - Comprehensive multi-paragraph
6. **Custom Length** - User-specified (50-500 words)

### Supported Models

| Model | Size | Speed | Quality | Languages | Use Case |
|-------|------|-------|---------|-----------|----------|
| Phi-3.5-mini-Q4 | 2.3GB | Fast | Excellent | EN | Best balance |
| Qwen2.5-3B-Q4 | 1.9GB | Very Fast | Good | Multi | Multilingual |
| SmolLM2-1.7B-Q4 | 1.0GB | Fastest | Good | EN | Speed priority |
| Gemma-2-2B-Q4 | 1.5GB | Fast | Very Good | EN | Quality priority |

## Public Interface

```python
from summarizer import SummarizerService, SummaryRequest, SummaryType

class SummarizerService:
    async def summarize(
        self,
        text: str,
        summary_type: SummaryType = SummaryType.ABSTRACTIVE,
        max_words: int = 150,
        model: str = "phi-3.5-mini",
        stream: bool = False
    ) -> Summary:
        """Generate summary of text

        Args:
            text: Input text (100-50000 chars)
            summary_type: Type of summary
            max_words: Target length in words
            model: Model to use
            stream: Stream response chunks

        Returns:
            Summary with text, metadata, and stats

        Raises:
            ValueError: Invalid input
            ModelNotFoundError: Model not downloaded
            SummarizationError: Generation failed

        Example:
            >>> service = SummarizerService()
            >>> summary = await service.summarize(
            ...     text=long_document,
            ...     summary_type=SummaryType.BULLET_POINTS,
            ...     max_words=100
            ... )
            >>> print(summary.text)
        """

    async def batch_summarize(
        self,
        texts: list[str],
        **kwargs
    ) -> list[Summary]:
        """Summarize multiple documents in parallel"""

    async def download_model(
        self,
        model_name: str,
        quantization: str = "Q4_K_M",
        progress_callback: callable = None
    ) -> ModelInfo:
        """Download and cache model"""

    def list_models(self) -> list[ModelInfo]:
        """List available and downloaded models"""
```

## Input/Output Contracts

### Input: SummaryRequest

```python
{
    "text": str,                    # 100-50000 chars
    "summary_type": str,            # extractive|abstractive|bullets|tldr|detailed|custom
    "max_words": int,               # 50-500 (default: 150)
    "model": str,                   # Model identifier (default: phi-3.5-mini)
    "language": str | None,         # Auto-detect if None
    "stream": bool                  # Stream chunks (default: False)
}
```

### Output: Summary

```python
{
    "id": str,                      # UUID
    "text": str,                    # Generated summary
    "summary_type": str,            # Type used
    "source_length": int,           # Input char count
    "summary_length": int,          # Output char count
    "compression_ratio": float,     # source/summary
    "model": str,                   # Model used
    "language": str,                # Detected language
    "generation_time": float,       # Seconds
    "tokens_per_second": float,     # Performance metric
    "created_at": datetime
}
```

## Side Effects

- **Model Downloads**: Models downloaded to `vault/backend/models/summarization/`
- **Caching**: Summaries optionally cached in database
- **GPU Usage**: Attempts GPU if available, falls back to CPU
- **Memory**: 2-4GB RAM during inference
- **Disk**: 1-3GB per model (GGUF quantized)

## Dependencies

- `llama-cpp-python>=0.2.0` - Inference engine
- `httpx>=0.25.0` - Model downloads
- `langdetect>=1.0.9` - Language detection
- `tiktoken>=0.5.0` - Token counting

## Configuration

```python
# config.py
SUMMARIZER_CONFIG = {
    "default_model": "phi-3.5-mini",
    "quantization": "Q4_K_M",           # Q4_K_M, Q5_K_M, Q8_0
    "max_context_length": 4096,         # Model context window
    "temperature": 0.3,                 # Low for consistency
    "max_tokens": 512,                  # Max output tokens
    "gpu_layers": -1,                   # -1 = all on GPU if available
    "threads": 4,                       # CPU threads
    "batch_size": 512,                  # Batch size for inference
    "model_cache_dir": "models/summarization",
    "enable_caching": True,             # Cache summaries
    "cache_ttl": 86400,                 # 24 hours
}
```

## Error Handling

| Error Type | Condition | Recovery Strategy |
|------------|-----------|-------------------|
| ValueError | Invalid input (too short/long) | Return error with constraints |
| ModelNotFoundError | Model not downloaded | Download model or use fallback |
| OutOfMemoryError | Model too large for RAM | Use smaller model/higher quantization |
| SummarizationError | Generation failed | Retry once, fallback to extractive |
| LanguageNotSupportedError | Model doesn't support language | Use multilingual model |

## Performance Characteristics

### Latency (on typical laptop CPU)

- **1-page document (500 words)**: 3-8 seconds
- **5-page document (2500 words)**: 10-20 seconds
- **Chunked long document**: 15-40 seconds

### Throughput

- **Sequential**: 1-2 docs/minute
- **Batch (parallel)**: 5-10 docs/minute (with proper chunking)

### Resource Usage

- **CPU**: 100% during generation (4 cores)
- **RAM**: 2-4GB (model + context)
- **GPU**: Optional acceleration (2-5x faster)

## Testing

```bash
# Run unit tests
pytest vault/backend/src/modules/summarizer/tests/

# Run contract validation
pytest vault/backend/src/modules/summarizer/tests/test_contract.py

# Run documentation tests
pytest vault/backend/src/modules/summarizer/tests/test_documentation.py

# Run model tests (requires downloaded models)
pytest vault/backend/src/modules/summarizer/tests/test_models.py
```

## Regeneration Specification

This module can be regenerated from this specification alone.

**Key invariants:**
- Public function signatures remain stable
- Input/output data structures unchanged
- Error types and conditions preserved
- Performance within 2x of specified targets
- Model compatibility maintained

## Model Selection Guide

### For Speed (< 5s per page)
- **SmolLM2-1.7B-Q4**: Fastest, good quality
- **Use when**: Real-time summarization, many documents

### For Quality (best summaries)
- **Phi-3.5-mini-Q4**: Best instruction following
- **Use when**: Important documents, user-facing summaries

### For Multilingual
- **Qwen2.5-3B-Q4**: 20+ languages
- **Use when**: Non-English content

### For Privacy + Quality Balance
- **Phi-3.5-mini-Q4**: Default choice
- **Use when**: General purpose

## Privacy Guarantees

✅ **100% Local Processing**: No API calls, no telemetry
✅ **On-Device Models**: All inference happens locally
✅ **No Data Leakage**: Text never leaves user's machine
✅ **Open Source Models**: Auditable, no proprietary black boxes
✅ **Optional Caching**: User can disable summary storage

## Integration Examples

### Basic Usage

```python
service = SummarizerService()

# Simple summary
summary = await service.summarize(
    text="Long document text...",
    summary_type=SummaryType.TLDR
)
print(summary.text)
```

### Batch Processing

```python
# Summarize multiple documents
documents = load_documents()
summaries = await service.batch_summarize(
    texts=[doc.text for doc in documents],
    summary_type=SummaryType.BULLET_POINTS,
    max_words=100
)
```

### Streaming

```python
# Stream summary generation
async for chunk in service.summarize(
    text=document,
    stream=True
):
    print(chunk, end="", flush=True)
```

### With Caching

```python
# Cache summaries to avoid regeneration
summary = await service.summarize(
    text=document,
    cache_key=f"doc_{document_id}"
)
```

## API Endpoints

See `vault/backend/src/api/v1/summarize.py` for HTTP API documentation.

- `POST /api/v1/summarize/document/{doc_id}` - Summarize indexed document
- `POST /api/v1/summarize/batch` - Batch summarization
- `POST /api/v1/summarize/text` - Summarize raw text
- `GET /api/v1/summarize/models` - List available models
- `POST /api/v1/summarize/models/download` - Download model
- `GET /api/v1/summarize/{summary_id}` - Get cached summary
- `DELETE /api/v1/summarize/{summary_id}` - Delete cached summary

## Architecture

```
┌─────────────────────────────────────────────┐
│           Summarizer Module                 │
│                                             │
│  ┌──────────────────────────────────────┐  │
│  │    SummarizerService                 │  │
│  │  • summarize()                       │  │
│  │  • batch_summarize()                 │  │
│  │  • stream_summarize()                │  │
│  └────────┬─────────────────────────────┘  │
│           │                                 │
│  ┌────────▼─────────┐  ┌──────────────┐   │
│  │  ModelManager    │  │ PromptBuilder│   │
│  │  • download()    │  │ • build()    │   │
│  │  • load()        │  │ • optimize() │   │
│  │  • list()        │  └──────────────┘   │
│  └────────┬─────────┘                      │
│           │                                 │
│  ┌────────▼──────────────────────────────┐ │
│  │     llama-cpp-python                  │ │
│  │  • GGUF model loading                 │ │
│  │  • Inference (CPU/GPU)                │ │
│  │  • Streaming generation               │ │
│  └───────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```
