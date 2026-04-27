# Service Layer

## Overview

The **Service Layer** in Recall acts as the **orchestration layer**, coordinating between domain modules, managing cross-cutting concerns, and providing high-level application workflows.

Services are **thin controllers** that:
- Orchestrate calls to multiple domain modules
- Handle persistence (database sessions)
- Manage transactions and error handling
- Provide API-friendly interfaces
- Handle cross-cutting concerns (logging, metrics, tracing)

Services **DO NOT**:
- Contain core business logic (that belongs in modules)
- Directly implement algorithms (use modules)
- Manage their own state (use dependency injection)

---

## Architecture Pattern

```
API Routes (interfaces/commands.rs or api/v1/)
    ↓
Service Layer (services/) - ORCHESTRATION
    ↓
Domain Modules (modules/) - BUSINESS LOGIC
    ↓
Data Layer (repositories/persistence)
```

**Key Principle**: Services orchestrate, modules implement.

---

## Current Services

### 1. IndexingService (`indexing.py`)

**Purpose**: Orchestrate document indexing pipeline

**Responsibilities**:
- Coordinate content extraction, chunking, and embedding
- Manage file status updates in database
- Handle indexing, reindexing, and deletion workflows
- Orchestrate contextual retrieval (if enabled)

**Orchestrates**:
- `content_extractor` module (text/image extraction)
- `chunking` module (text segmentation)
- `embedding_generator` module (vector generation)
- Database persistence (File, TextContent, Image, embeddings)

**Example**:
```python
async def index_file(self, file_id: UUID, db_session: AsyncSession) -> bool:
    # 1. Fetch file from DB
    # 2. Extract content (via content_extractor module)
    # 3. Chunk text (via chunking module)
    # 4. Generate embeddings (via embedding_generator module)
    # 5. Persist to DB
    # 6. Update file status
```

**Dependency Injection Note**:
- Currently creates module instances directly (TextEmbedder, ImageEmbedder, ChunkingService)
- Future: Accept these as constructor parameters for better testability

---

### 2. LLMService (`llm/service.py`)

**Purpose**: Orchestrate RAG (Retrieval-Augmented Generation) pipeline

**Responsibilities**:
- Coordinate search → context building → LLM generation
- Manage OllamaService lifecycle (initialize/cleanup)
- Handle streaming and non-streaming responses
- Integrate search results with LLM prompts
- Manage prompt caching for performance

**Orchestrates**:
- `search_engine` module (SearchService for document retrieval)
- `OllamaService` (LLM generation via Ollama API)
- Shared prompt utilities (`src.utils.prompts`)

**Example**:
```python
async def ask_question(self, question: str, ...) -> AsyncIterator[str]:
    # 1. Search for relevant documents (via SearchService)
    # 2. Build context from results (via shared prompts)
    # 3. Create RAG prompt (via shared prompts)
    # 4. Stream LLM response (via OllamaService)
```

**Circular Dependency Note**:
- Currently creates OllamaService internally
- **TODO**: Accept OllamaService as constructor parameter (dependency injection)
- This would break circular import with agentic_rag.py

---

### 3. WatchService (`watch.py`)

**Purpose**: Orchestrate file system monitoring and auto-indexing

**Responsibilities**:
- Manage FileWatcher lifecycle (start/stop watching)
- Route file events to appropriate handlers
- Coordinate file creation/modification/deletion with indexing
- Update watch folder metadata in database

**Orchestrates**:
- `file_watcher` module (FileWatcher for FS events)
- `IndexingService` (for indexing new/modified files)
- Database persistence (File, WatchFolder)

**Example**:
```python
async def _handle_created(self, event: FileEvent, ...) -> None:
    # 1. Check if file exists in DB
    # 2. Validate file size/type
    # 3. Create File record
    # 4. Trigger indexing (via IndexingService)
```

**Event-Driven Pattern**:
- Receives events from FileWatcher
- Dispatches to handlers based on event type (CREATED, MODIFIED, DELETED, MOVED)
- Uses debouncing to avoid duplicate processing

---

### 4. UnifiedSearchService (`search_engine/db_unified_search.py`)

**Purpose**: Facade for all search modes with result mapping

**Responsibilities**:
- Route requests to appropriate search engine (BM25, hybrid, vector)
- Map engine-specific results to unified SearchResultItem format
- Record search history via SearchHistoryService
- Track metrics and tracing

**Orchestrates**:
- `BM25SearchEngine` (keyword search)
- `HybridSearchEngine` (combined vector+keyword)
- `SearchService` (vector/text search)
- `SearchHistoryService` (history recording)

**Example**:
```python
async def search(self, request: SearchRequest) -> UnifiedSearchResult:
    # 1. Route to appropriate engine based on mode
    # 2. Execute search
    # 3. Map results to unified format
    # 4. Record history
    # 5. Track metrics
```

**Circular Dependency Note**:
- Currently creates SearchHistoryService internally
- **TODO**: Accept SearchHistoryService as constructor parameter
- Alternative: Make search history optional (injected or None)

---

## Design Patterns

### 1. Dependency Injection (Recommended)

Services should accept dependencies via constructor:

```python
class MyService:
    def __init__(
        self,
        db_session: AsyncSession,
        module_a: ModuleA,
        module_b: ModuleB,
    ):
        self.db_session = db_session
        self.module_a = module_a
        self.module_b = module_b
```

**Benefits**:
- Easier testing (inject mocks)
- Clearer dependencies
- Avoids circular imports
- Follows SOLID principles

### 2. Thin Controllers

Keep service methods focused on orchestration:

```python
# GOOD - Orchestrates modules
async def process_document(self, doc_id: UUID) -> bool:
    doc = await self.repo.get(doc_id)
    extracted = self.extractor.extract(doc.path)
    chunks = self.chunker.chunk(extracted.text)
    embeddings = self.embedder.embed(chunks)
    await self.repo.save_embeddings(embeddings)
    return True

# BAD - Contains business logic
async def process_document(self, doc_id: UUID) -> bool:
    # 50 lines of chunking algorithm here...
    # This belongs in a module!
```

### 3. Error Handling

Services handle errors and map to user-friendly messages:

```python
async def index_file(self, file_id: UUID) -> bool:
    try:
        await self._do_indexing(file_id)
        return True
    except FileNotFoundError as e:
        logger.error(f"File not found: {e}")
        await self._update_status(file_id, "failed", str(e))
        return False
    except Exception as e:
        logger.exception(f"Indexing failed: {e}")
        await self._update_status(file_id, "failed", "Internal error")
        raise
```

### 4. Transaction Management

Services manage database transactions:

```python
async def create_and_index(self, file_path: str, session: AsyncSession) -> UUID:
    async with session.begin():  # Transaction
        file = File(path=file_path)
        session.add(file)
        await session.flush()  # Get ID

        await self.indexing_service.index_file(file.id, session)

        # Commit happens automatically if no exception
        return file.id
```

---

## Cross-Cutting Concerns

Services handle concerns that span multiple modules:

### 1. Observability

```python
from src.observability.tracing import get_tracer
from src.observability.metrics import record_search

tracer = get_tracer(__name__)

async def search(self, query: str):
    with tracer.start_as_current_span("search") as span:
        span.set_attribute("query", query)
        results = await self._do_search(query)
        record_search(query, len(results))
        return results
```

### 2. Configuration

```python
from src.config import get_settings

class MyService:
    def __init__(self):
        self.settings = get_settings()
        self.timeout = self.settings.search_timeout
```

### 3. Logging

```python
import logging

logger = logging.getLogger(__name__)

async def process(self, item: Item):
    logger.info(f"Processing item: {item.id}")
    try:
        result = await self._do_work(item)
        logger.info(f"Processed {item.id}: {result}")
        return result
    except Exception as e:
        logger.exception(f"Failed to process {item.id}")
        raise
```

---

## Testing Services

Services are tested with **integration tests** (modules + DB):

```python
@pytest.mark.integration
async def test_indexing_service(db_session, tmp_path):
    # Arrange
    service = IndexingService()
    file = File(path=str(tmp_path / "test.txt"))
    db_session.add(file)
    await db_session.flush()

    # Act
    result = await service.index_file(file.id, db_session)

    # Assert
    assert result is True
    assert file.processing_status == "indexed"
```

**Key Points**:
- Use real database (or test DB)
- Use real modules (not mocks) when possible
- Mock only external services (LLM APIs, etc.)
- Test error paths and edge cases

---

## Future Refactoring

### Phase 3 Improvements

1. **Move Prompt Templates**:
   - ✅ Create `src/utils/prompts.py` with shared prompts
   - ✅ Update `llm/prompts.py` to re-export from shared location

2. **Break Circular Dependencies**:
   - **TODO**: LLMService should accept OllamaService via DI
   - **TODO**: UnifiedSearchService should accept SearchHistoryService via DI
   - This requires updating calling code (API routes, tests)

3. **Add Inline Documentation**:
   - ✅ Add module docstrings to services
   - ✅ Document orchestration patterns

---

## When to Create a Service

Create a new service when you need to:

1. **Orchestrate multiple modules** for a workflow
2. **Manage persistence** for a feature
3. **Handle cross-cutting concerns** (auth, caching, etc.)
4. **Provide API-friendly interface** to complex module interactions

**Don't create a service** if:
- Logic can live in a single module
- No database interaction needed
- No orchestration required (just use the module directly)

---

## Service Checklist

When creating a service:

- [ ] Accept dependencies via constructor (DI pattern)
- [ ] Keep methods thin (orchestration, not logic)
- [ ] Handle errors gracefully
- [ ] Add logging (info for workflow, error for failures)
- [ ] Add tracing spans for observability
- [ ] Manage transactions properly
- [ ] Write integration tests
- [ ] Document orchestration pattern in docstring
- [ ] Use type hints on all public methods
- [ ] Export public interface via `__all__`

---

## Examples

### Good Service Pattern

```python
"""MyService orchestrates X, Y, and Z modules for workflow W."""

from typing import Protocol
import logging
from src.modules.x import ModuleX
from src.modules.y import ModuleY

logger = logging.getLogger(__name__)

class MyService:
    """Orchestrate modules X and Y for workflow W.

    This service coordinates:
    - Module X for extraction
    - Module Y for processing
    - Database persistence
    """

    def __init__(
        self,
        db_session: AsyncSession,
        module_x: ModuleX,
        module_y: ModuleY,
    ):
        self.db = db_session
        self.module_x = module_x
        self.module_y = module_y

    async def execute_workflow(self, input_id: UUID) -> Result:
        """Execute workflow W.

        Steps:
        1. Extract data via Module X
        2. Process via Module Y
        3. Persist to database
        """
        logger.info(f"Starting workflow for {input_id}")

        # Orchestrate modules
        data = await self.module_x.extract(input_id)
        processed = await self.module_y.process(data)

        # Persist
        await self.db.save(processed)

        logger.info(f"Workflow completed for {input_id}")
        return processed
```

---

## Summary

The Service Layer is the **orchestration hub** of Recall:

- **Routes** call **Services** for high-level operations
- **Services** orchestrate **Modules** for business logic
- **Modules** implement algorithms and domain rules
- **Repositories** handle data persistence

Keep services thin, focused, and well-tested. Move business logic to modules. Use dependency injection for flexibility and testability.
