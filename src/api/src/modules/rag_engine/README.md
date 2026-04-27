# Agentic RAG: Self-Correcting Retrieval-Augmented Generation

Cutting-edge 2025 RAG implementation with self-reflection, verification, and adaptive retrieval.

## Overview

Traditional RAG systems retrieve documents and generate answers without verification. **Agentic RAG** adds intelligence:

- 🔍 **Self-RAG**: Verifies retrieval quality and answer support
- 🔄 **CRAG**: Falls back to alternative sources if local retrieval fails
- 🧩 **Multi-Step**: Decomposes complex questions into sub-questions
- 🎯 **Adaptive**: Automatically selects best mode for each question

## Quick Start

```python
from recall.rag import AgenticRAG
from recall.search import SearchEngine

# Initialize
search = SearchEngine(database_path="recall.db")
rag = AgenticRAG(search_engine=search)

# Ask question with automatic mode selection
result = await rag.ask("What is machine learning?")

print(f"Answer: {result.answer}")
print(f"Mode used: {result.mode}")
print(f"Confidence: {result.confidence}")
print(f"Citations: {len(result.citations)}")
```

## RAG Modes

### 1. Self-RAG (Self-Reflective RAG)

**When to use:** Factual questions needing verification and citations

**How it works:**
1. Retrieve documents
2. LLM assesses: "Are these documents relevant?"
3. Generate answer with inline citations [1], [2]
4. LLM verifies: "Is answer supported by sources?"
5. Retry with different query if quality is poor

**Example:**

```python
result = await rag.ask(
    question="What is supervised learning?",
    mode=RAGMode.SELF_RAG,
    max_iterations=3
)

# Check verification status
print(f"Retrieval: {result.retrieval_assessment}")  # "highly_relevant"
print(f"Answer: {result.answer_assessment}")  # "fully_supported"
print(f"Iterations: {result.iterations}")  # 1

# Access citations
for citation in result.citations:
    print(f"[{citation.citation_id}] {citation.file_path}")
    print(f"  {citation.snippet}")
```

**Assessment Levels:**

Retrieval Quality:
- `highly_relevant`: Documents directly answer question
- `relevant`: Documents contain useful information
- `partially_relevant`: Some related information
- `not_relevant`: Documents don't help

Answer Support:
- `fully_supported`: All claims backed by sources
- `partially_supported`: Some claims lack support
- `not_supported`: Claims not in sources
- `hallucination`: Answer contradicts sources

### 2. CRAG (Corrective RAG)

**When to use:** Questions where local knowledge might be incomplete

**How it works:**
1. Try local knowledge base
2. Assess document quality: HIGH/MEDIUM/LOW
3. Based on quality:
   - HIGH → Use local only
   - MEDIUM → Supplement with web (if enabled)
   - LOW → Use web only (if enabled)

**Example:**

```python
result = await rag.ask(
    question="What's the latest Python version?",
    mode=RAGMode.CRAG,
    enable_web_fallback=True  # Enable web search
)

if result.fallback_used:
    print("Used fallback source (web)")
print(f"Source: {result.metadata['source']}")  # "hybrid" or "web"
```

**Quality Assessment:**
- `high`: Documents directly answer with clear info
- `medium`: Some relevant info, may need supplementation
- `low`: Documents don't contain needed information

### 3. Multi-Step Reasoning

**When to use:** Complex questions with multiple parts

**How it works:**
1. Decompose question into 2-4 sub-questions
2. Answer each sub-question independently
3. Synthesize comprehensive final answer

**Example:**

```python
result = await rag.ask(
    question="Compare supervised and unsupervised learning",
    mode=RAGMode.MULTI_STEP,
    max_sub_questions=4
)

# View reasoning chain
for i, step in enumerate(result.reasoning_steps):
    print(f"\nStep {i+1}: {step.question}")
    print(f"Answer: {step.answer}")
    print(f"Confidence: {step.confidence}")
    print(f"Sources: {len(step.sources)}")

print(f"\nFinal Answer: {result.answer}")
```

**Example Decomposition:**

Original: "Compare supervised and unsupervised learning"

Sub-questions:
1. "What is supervised learning?"
2. "What is unsupervised learning?"
3. "What are the key differences?"
4. "What are common use cases for each?"

### 4. Adaptive Mode

**When to use:** Let the system choose automatically

**How it works:**
- Analyzes question complexity and characteristics
- Selects best mode based on heuristics
- Falls back to Self-RAG for balanced quality

**Example:**

```python
# Adaptive is the default
result = await rag.ask("What is Python?")
# System chooses Self-RAG (factual, needs verification)

result = await rag.ask("Compare Python and JavaScript")
# System chooses Multi-Step (complex comparison)
```

**Selection Heuristics:**

→ Multi-Step if:
- Contains: "compare", "contrast", "difference", "pros and cons"
- Multiple parts: "and", "as well as", "both"
- Multiple questions: "? ... ?"

→ Self-RAG if:
- Factual indicators: "what is", "who is", "when did"
- Needs verification: definite claims

→ Standard otherwise

## API Integration

### REST API

```python
# Backend server (FastAPI)
from recall.api.routes.agentic_rag import router
app.include_router(router)
```

**Endpoints:**

```bash
# Ask with agentic RAG
POST /api/v1/agentic/ask
{
  "question": "What is machine learning?",
  "mode": "self_rag",
  "max_iterations": 3,
  "top_k": 5
}

# Stream answer
POST /api/v1/agentic/ask/stream
# Returns Server-Sent Events stream

# Decompose complex question
POST /api/v1/agentic/decompose
{
  "question": "Compare X and Y",
  "max_sub_questions": 4
}

# Health check
GET /api/v1/agentic/health
```

### Response Format

```json
{
  "answer": "Machine learning is a branch of AI...",
  "mode": "self_rag",
  "confidence": 0.92,
  "citations": [
    {
      "file_path": "C:\\docs\\ml_guide.txt",
      "snippet": "Machine learning is...",
      "score": 0.95,
      "citation_id": 1
    }
  ],
  "sources": [
    {
      "file_path": "C:\\docs\\ml_guide.txt",
      "score": 0.95,
      "snippet": "..."
    }
  ],
  "iterations": 1,
  "retrieval_assessment": "highly_relevant",
  "answer_assessment": "fully_supported",
  "execution_time_ms": 1250.5,
  "metadata": {}
}
```

## Performance

### Latency Comparison

| Mode | Latency | Quality | Use Case |
|------|---------|---------|----------|
| Standard | ~1-2s | Baseline | Simple questions |
| Self-RAG | ~2-4s | High (verified) | Factual questions |
| CRAG | ~1-3s (local)<br>~3-5s (web) | High (robust) | Incomplete knowledge |
| Multi-Step | ~3-8s | Best for complex | Multi-part questions |
| Adaptive | Variable | Balanced | General use |

### Why Slower?

- **Self-RAG**: 3 LLM calls (assess retrieval, generate, assess answer)
- **CRAG**: Quality assessment + possible web search
- **Multi-Step**: Multiple retrievals + synthesis

### Optimization Tips

1. **Use Standard for simple questions**
   ```python
   result = await rag.ask(question, mode=RAGMode.STANDARD)
   ```

2. **Reduce iterations for Self-RAG**
   ```python
   result = await rag.ask(question, max_iterations=1)
   ```

3. **Limit sub-questions for Multi-Step**
   ```python
   result = await rag.ask(question, max_sub_questions=2)
   ```

4. **Cache assessment results**
   - Similar questions get similar assessments
   - TODO: Implement caching layer

## Configuration

```python
from recall.rag import AgenticRAG, RAGMode

rag = AgenticRAG(
    search_engine=search,
    web_search=None,  # Web search not yet implemented
    ollama_url="http://localhost:11434",
    model_name="llama3.1:8b"
)

# Check health
if await rag.health_check():
    print("Ollama is ready")
```

## Testing

```bash
# Run all agentic RAG tests
pytest tests/rag/

# Run specific test file
pytest tests/rag/test_self_rag.py -v

# Run with coverage
pytest tests/rag/ --cov=recall.rag
```

## Architecture

```
recall/rag/
├── __init__.py          # Public API
├── types.py             # Data classes and enums
├── self_rag.py          # Self-RAG implementation
├── crag.py              # CRAG implementation
├── multi_step.py        # Multi-step reasoning
├── agentic.py           # Unified interface
└── README.md            # This file

tests/rag/
├── __init__.py
├── test_self_rag.py     # Self-RAG tests
├── test_crag.py         # CRAG tests
├── test_multi_step.py   # Multi-step tests
└── test_agentic.py      # Integration tests
```

## Error Handling

```python
from recall.rag.types import (
    RAGError,
    RetrievalFailedError,
    VerificationFailedError,
    QueryDecompositionError
)

try:
    result = await rag.ask(question)
except RetrievalFailedError:
    print("Failed to find relevant documents")
except VerificationFailedError:
    print("Answer failed verification")
except QueryDecompositionError:
    print("Failed to decompose question")
except RAGError as e:
    print(f"RAG error: {e}")
```

## Future Enhancements

### Phase 1 (Implemented) ✅
- [x] Self-RAG with reflection
- [x] CRAG framework
- [x] Multi-step reasoning
- [x] Adaptive mode selection
- [x] REST API endpoints
- [x] Citation extraction

### Phase 2 (Planned)
- [ ] Web search integration for CRAG
- [ ] Streaming at RAG layer (not just API)
- [ ] Cache assessment results
- [ ] Parallel sub-question answering
- [ ] Graph-based reasoning
- [ ] Active learning from feedback

### Phase 3 (Future)
- [ ] Multi-agent collaboration
- [ ] Tool use (calculator, code execution)
- [ ] Memory/history integration
- [ ] Cost tracking per mode
- [ ] A/B testing framework

## Research Background

This implementation is based on:

- **Self-RAG** (Asai et al., 2023): "Self-Reflective Retrieval-Augmented Generation"
- **CRAG** (Yan et al., 2024): "Corrective Retrieval Augmented Generation"
- **Adaptive RAG** (Jeong et al., 2024): "Adaptive-RAG: Learning to Adapt"

Key innovations:
- Self-assessment of retrieval quality
- Answer verification against sources
- Iterative query refinement
- Multi-step decomposition
- Adaptive mode selection

## License

Part of the Recall project. See main LICENSE file.

## Contributing

See CONTRIBUTING.md in project root.

## Support

- Issues: GitHub Issues
- Docs: See `docs/` directory
- Examples: See `examples/` directory
