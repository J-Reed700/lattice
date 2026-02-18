# Q&A Module - RAG-based Question Answering

A self-contained RAG (Retrieval-Augmented Generation) engine that combines semantic search with LLM generation to answer questions from your knowledge base.

## Overview

The Q&A module provides an end-to-end pipeline for answering questions using your indexed documents:

1. **Retrieval**: Searches for relevant documents using semantic similarity
2. **Context Building**: Extracts and combines relevant excerpts within token budget
3. **Generation**: Sends context + question to Ollama LLM for answer generation
4. **Streaming**: Returns answer in real-time as it's generated

## Components

### Core Engine

- **`QAEngine`**: Main class that orchestrates the RAG pipeline
  - Async streaming interface
  - Automatic token management
  - Source attribution
  - Health checking for Ollama availability

### Supporting Modules

- **`types.py`**: Exceptions and data classes
  - `QAError`, `OllamaUnavailableError`, `ContextTooLargeError`
  - `SourceReference` for document attribution

- **`tokenizer.py`**: Token counting utilities using tiktoken
  - `count_tokens()`: Accurate token counting for context management
  - `truncate_to_tokens()`: Intelligent text truncation

- **`prompts.py`**: Prompt templates
  - `SYSTEM_PROMPT`: Instructions for the LLM
  - `USER_PROMPT_TEMPLATE`: Context + question formatting

## Usage

### Basic Example

```python
from recall.qa import QAEngine
from recall.search import SearchEngine

# Initialize with search engine
qa_engine = QAEngine(
    search_engine=search_engine,
    ollama_url="http://localhost:11434",
    model_name="llama3.1:8b"
)

# Check if Ollama is available
if await qa_engine.health_check():
    # Ask a question with streaming
    async for chunk in qa_engine.ask("What is machine learning?"):
        print(chunk, end="", flush=True)
    print()  # Newline after answer
else:
    print("Ollama is not available. Please run: ollama serve")
```

### Advanced Usage

```python
# Customize retrieval and context
async for chunk in qa_engine.ask(
    question="Explain neural networks in detail",
    top_k=10,  # Retrieve more documents
    max_context_tokens=3000  # Use more context
):
    print(chunk, end="", flush=True)

# Collect full answer
answer = ""
async for chunk in qa_engine.ask("What is Python?"):
    answer += chunk

print(f"Complete answer: {answer}")

# Always close when done
await qa_engine.close()
```

### Error Handling

```python
from recall.qa import QAEngine, OllamaUnavailableError, QAError

try:
    async for chunk in qa_engine.ask("What is recursion?"):
        print(chunk, end="")
except OllamaUnavailableError:
    print("Ollama service is not running. Start it with: ollama serve")
except QAError as e:
    print(f"Q&A error: {e}")
```

## Requirements

### Dependencies

- `httpx>=0.25.0` - Async HTTP client for Ollama API
- `tiktoken>=0.5.0` - Token counting for context management
- Existing modules: `search`, `core` (models)

### External Services

- **Ollama**: Local LLM server
  - Install: `curl https://ollama.ai/install.sh | sh`
  - Start: `ollama serve`
  - Pull model: `ollama pull llama3.1:8b`

## Architecture

### RAG Pipeline Flow

```
User Question
    ↓
SearchEngine.search(question) → Top K relevant documents
    ↓
Build context from search results (with token budget)
    ↓
Format prompt: SYSTEM_PROMPT + context + question
    ↓
Stream to Ollama LLM (/api/generate endpoint)
    ↓
Yield answer chunks in real-time
```

### Token Management

The module carefully manages tokens to fit within LLM context windows:

1. **Count tokens** in search results using tiktoken
2. **Truncate** documents that exceed budget
3. **Track** running total as context is built
4. **Stop** when max_context_tokens reached

### Error Handling

| Error Type | Condition | Recovery |
|------------|-----------|----------|
| `QAError` | Invalid input (empty question, negative parameters) | Fix input |
| `OllamaUnavailableError` | Ollama not reachable | Start Ollama service |
| `SearchError` | Search fails | Check search engine/database |
| `ContextTooLargeError` | Context exceeds limit | Reduce top_k or increase max_tokens |

## Performance

### Typical Latency

- **Search**: ~100ms (for 10K documents)
- **Context Building**: ~10-50ms
- **LLM First Token**: ~500-2000ms (model dependent)
- **LLM Generation**: ~10-50 tokens/second (model dependent)

### Optimization Tips

1. **Reduce top_k** for faster search (fewer documents to process)
2. **Lower max_context_tokens** for faster LLM response
3. **Use smaller models** (e.g., `llama3.1:8b` vs `llama3.1:70b`)
4. **Enable deduplicate=True** in search to avoid redundant context

## Testing

Run unit tests:

```bash
pytest tests/unit/test_qa.py -v
```

Tests cover:
- Token counting and truncation
- Context building with token limits
- Prompt formatting
- Error handling
- Mock integration tests

## Public Interface

```python
# Main class
class QAEngine:
    def __init__(search_engine, ollama_url, model_name)
    async def ask(question, top_k=5, max_context_tokens=2000) -> AsyncIterator[str]
    async def health_check() -> bool
    async def close()

# Utilities
def count_tokens(text: str, model: str = "gpt-3.5-turbo") -> int
def truncate_to_tokens(text: str, max_tokens: int, model: str = "gpt-3.5-turbo") -> str

# Data classes
@dataclass
class SourceReference:
    file_path: str
    score: float
    snippet: str

# Exceptions
class QAError(Exception)
class OllamaUnavailableError(QAError)
class ContextTooLargeError(QAError)
```

## Configuration

### Default Parameters

- `ollama_url`: "http://localhost:11434"
- `model_name`: "llama3.1:8b"
- `top_k`: 5 documents
- `max_context_tokens`: 2000 tokens
- `temperature`: 0.7 (Ollama request)
- `top_p`: 0.9 (Ollama request)

### Supported Models

Any Ollama model can be used. Recommended:

- `llama3.1:8b` - Fast, good quality (default)
- `llama3.1:70b` - Slower, higher quality
- `mistral` - Fast, concise responses
- `mixtral` - Good balance of speed and quality

## Regeneration Specification

This module is fully self-contained and can be regenerated from this specification:

**Inputs**:
- SearchEngine instance
- Ollama URL and model name
- User question string

**Outputs**:
- Async iterator yielding answer chunks
- Health check boolean

**Side Effects**:
- HTTP calls to Ollama API
- Logging to Python logger

**Error Handling**:
- Validates all inputs
- Gracefully handles connection failures
- Returns helpful error messages

**Dependencies**:
- External: httpx, tiktoken
- Internal: search engine, core models

All public functions have comprehensive docstrings with examples. The module follows the "bricks and studs" philosophy with clear contracts and isolated functionality.
