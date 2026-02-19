# Cross-Encoder Reranker Module

Self-contained module for reranking search results using cross-encoder models.

## Purpose

Improve search precision by reordering initial retrieval results using a more accurate cross-encoder model. Provides **+10-15% nDCG@10 improvement** over hybrid search alone.

## Public Interface

```python
from src.modules.reranker.service import RerankService
from src.modules.reranker.model import CrossEncoderModel
from src.modules.search_engine.types import SearchResult

# Initialize service
service = RerankService(
    model_name="BAAI/bge-reranker-v2-m3",
    device="cpu",
    max_content_length=2000,
    batch_size=32,
    cache_enabled=True,
    cache_ttl=3600
)

# Rerank search results
reranked_results = await service.rerank(
    query="search query",
    results=initial_results,  # List[SearchResult]
    top_k=20,
    timeout=2.0
)

# Get model information
info = RerankService.get_model_info()
```

## Module Structure

```
reranker/
├── __init__.py          # Public exports
├── README.md            # This file
├── model.py             # CrossEncoderModel (model wrapper)
├── service.py           # RerankService (main service)
└── types.py             # Data types
```

## Configuration

Configured via environment variables (see `src/config/settings.py`):

```bash
RERANKING_ENABLED=true
RERANKING_MODEL=BAAI/bge-reranker-v2-m3
RERANKING_TOP_K_INPUT=100
RERANKING_TOP_K_OUTPUT=50
RERANKING_TIMEOUT=2.0
RERANKING_BATCH_SIZE=32
RERANKING_CACHE_ENABLED=true
RERANKING_CACHE_TTL=3600
RERANKING_MAX_CONTENT_LENGTH=2000
```

## Inputs

### RerankService.rerank()

| Parameter | Type | Description |
|-----------|------|-------------|
| `query` | `str` | Search query text |
| `results` | `List[SearchResult]` | Initial search results to rerank |
| `top_k` | `int` | Number of results to return (default: 50) |
| `timeout` | `float` | Max reranking time in seconds (default: 2.0) |

**Returns:** `List[SearchResult]` - Reranked results with updated scores

**Raises:** Does NOT raise exceptions - returns original results on error

## Outputs

Reranked `SearchResult` objects with updated `score` field:

```python
SearchResult(
    file_id="123",
    file_path="/path/to/document.txt",
    score=0.89,  # Updated by reranker (0.0-1.0)
    metadata={}
)
```

Scores are normalized to [0.0, 1.0] range using sigmoid function.

## Side Effects

1. **File I/O:** Reads document content from disk (up to `max_content_length` chars)
2. **Model Loading:** Lazy-loads model on first use (~200-500MB memory)
3. **Caching:** Stores query-result pairs in memory if `cache_enabled=True`
4. **Logging:** Logs reranking operations and errors

## Dependencies

- `sentence-transformers==2.2.2` - Cross-encoder models
- `torch==2.1.1` - PyTorch backend
- `numpy>=1.24.0` - Numerical operations

## Error Handling

| Error Type | Condition | Recovery Strategy |
|------------|-----------|-------------------|
| `asyncio.TimeoutError` | Reranking exceeds timeout | Returns original results[:top_k] |
| `FileNotFoundError` | Cannot read document | Returns empty string for that doc |
| `ModelLoadError` | Model fails to load | Raises exception (no fallback) |
| `OutOfMemoryError` | Batch too large | Reduce batch_size or top_k_input |

## Performance Characteristics

**Time Complexity:** O(n * m) where n = number of results, m = model inference time per result

**Memory Usage:**
- Model: ~200MB (ms-marco) to ~500MB (bge-large)
- Per-document: ~2KB (content buffer)
- Cache: ~1KB per cached query

**Latency (50 results, CPU):**
- `ms-marco-MiniLM`: ~60ms
- `bge-reranker-v2-m3`: ~120ms
- `bge-reranker-large`: ~200ms

**Throughput (CPU):**
- `ms-marco-MiniLM`: ~16 QPS
- `bge-reranker-v2-m3`: ~8 QPS

**GPU Speedup:** ~5x faster with CUDA-enabled GPU

## Testing

```bash
# Run unit tests
pytest tests/test_reranker.py -v

# Run benchmarks
python benchmarks/reranking_benchmark.py

# Run examples
python examples/reranking_example.py
```

## Regeneration Specification

This module can be regenerated from this specification alone.

**Key Invariants:**
1. `RerankService.rerank()` signature must not change
2. Output `SearchResult` objects must have `score` field updated
3. Must gracefully handle errors (no exceptions for user errors)
4. Must support timeout with fallback to original results
5. Model loading must be lazy (not on service initialization)

**Regeneration Steps:**
1. Implement `CrossEncoderModel` with lazy loading and caching
2. Implement `RerankService` with async document loading
3. Add timeout handling with `asyncio.wait_for()`
4. Add singleton model instances per model name
5. Integrate with configuration system
6. Write tests covering all error cases

## Examples

See `examples/reranking_example.py` for comprehensive examples.

**Quick Example:**

```python
import asyncio
from src.modules.reranker.service import RerankService
from src.modules.search_engine.types import SearchResult

async def main():
    service = RerankService()

    results = [
        SearchResult(file_id="1", file_path="/doc1.txt", score=0.5, metadata={}),
        SearchResult(file_id="2", file_path="/doc2.txt", score=0.6, metadata={}),
        SearchResult(file_id="3", file_path="/doc3.txt", score=0.4, metadata={})
    ]

    reranked = await service.rerank(
        query="relevant query",
        results=results,
        top_k=3,
        timeout=2.0
    )

    for i, result in enumerate(reranked):
        print(f"{i+1}. {result.file_path} - Score: {result.score:.4f}")

asyncio.run(main())
```

## Documentation

- **Comprehensive Guide:** `docs/RERANKING_GUIDE.md`
- **Configuration:** `src/config/settings.py`
- **API Integration:** `src/services/search/service.py`

## Support

For issues or questions about this module:
- Check `docs/RERANKING_GUIDE.md` for detailed documentation
- Run benchmarks to diagnose performance issues
- Review logs for error messages
- Open GitHub issue with reproduction steps
