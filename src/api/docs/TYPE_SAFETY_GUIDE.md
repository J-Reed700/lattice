# Type Safety Guide

This document outlines the type safety patterns and standards for the Vault backend codebase.

## Overview

The Vault backend uses **strict type checking** with mypy to ensure code quality and catch errors early. All Python files in the `src/` directory must follow these standards.

## Required Standards

### 1. Future Annotations

**All Python files must start with:**

```python
from __future__ import annotations
```

This import enables postponed evaluation of annotations (PEP 563), allowing:
- Use of modern type syntax (`list[str]` instead of `List[str]`)
- Forward references without quotes
- Cleaner, more readable type hints

**Example:**

```python
from __future__ import annotations

from fastapi import APIRouter

router = APIRouter()

@router.get("/items")
async def get_items() -> list[dict[str, str]]:
    """Get list of items."""
    return [{"name": "item1"}, {"name": "item2"}]
```

### 2. Return Type Hints

**All public functions MUST have explicit return type hints.**

#### FastAPI Route Handlers

```python
from __future__ import annotations

from fastapi import APIRouter, Depends
from sqlalchemy.ext.asyncio import AsyncSession

from src.api.schemas.search import SearchRequest, SearchResponse
from src.db import get_session

router = APIRouter()

@router.post("/search", response_model=SearchResponse)
async def search_files(
    request: SearchRequest,
    session: AsyncSession = Depends(get_session)
) -> SearchResponse:
    """
    Search for files.

    Return type matches response_model for documentation and validation.
    """
    # Implementation
    return SearchResponse(...)
```

#### Functions Returning Dicts

```python
@router.get("/health")
async def health_check() -> dict[str, str]:
    """Basic health check."""
    return {
        "status": "healthy",
        "timestamp": datetime.now(timezone.utc).isoformat()
    }
```

#### Functions with Complex Return Types

```python
@router.get("/stats")
async def get_statistics() -> dict[str, int | float | list[str]]:
    """Get system statistics."""
    return {
        "count": 42,
        "average": 3.14,
        "tags": ["python", "fastapi"]
    }
```

#### Service Layer Methods

```python
class SearchService:
    """Service for search operations."""

    def __init__(self, session: AsyncSession) -> None:
        self.session = session

    async def search(
        self,
        query: str,
        limit: int = 10
    ) -> list[SearchResult]:
        """
        Perform search query.

        Args:
            query: Search text
            limit: Maximum results to return

        Returns:
            List of search results ordered by relevance
        """
        # Implementation
        return results
```

### 3. Parameter Type Hints

**All parameters must have type hints** (except `self` and `cls`).

```python
async def process_file(
    file_path: str,
    chunk_size: int = 1024,
    encoding: str | None = None
) -> bytes:
    """Process a file and return contents."""
    # Implementation
    ...
```

### 4. Modern Type Syntax

With `from __future__ import annotations`, use modern syntax:

**✅ Preferred:**
```python
def get_items() -> list[str]:
    """Get list of strings."""
    return ["a", "b", "c"]

def get_mapping() -> dict[str, int]:
    """Get string to int mapping."""
    return {"count": 42}

def get_optional(value: str | None = None) -> str | None:
    """Get optional string."""
    return value
```

**❌ Avoid (old syntax):**
```python
from typing import List, Dict, Optional

def get_items() -> List[str]:  # Use list[str] instead
    return ["a", "b", "c"]

def get_optional(value: Optional[str] = None) -> Optional[str]:  # Use str | None
    return value
```

### 5. TYPE_CHECKING Pattern

For expensive imports only needed for type checking:

```python
from __future__ import annotations

from typing import TYPE_CHECKING

from fastapi import APIRouter

if TYPE_CHECKING:
    from src.services.heavy_module import ExpensiveClass
    from src.db.models import LargeModel

router = APIRouter()

@router.get("/data")
async def get_data(
    service: ExpensiveClass,  # Forward reference works with future annotations
) -> LargeModel:
    """Get data from service."""
    return await service.fetch()
```

**When to use TYPE_CHECKING:**
- Imports that cause circular dependencies
- Expensive imports (large ML models, heavy dependencies)
- Type-only imports from modules not used at runtime

**When NOT to use:**
- FastAPI dependencies and schemas (needed at runtime)
- Pydantic models (needed at runtime)
- SQLAlchemy models (if used in queries)
- Actual dependencies injected at runtime

## Mypy Configuration

The project uses **strict mypy settings**:

```toml
[tool.mypy]
strict = true
disallow_untyped_defs = true
disallow_any_generics = true
disallow_untyped_calls = true
warn_return_any = true
```

This means:
- All functions need type hints
- Generic types must specify type parameters
- No implicit `Any` types
- Return types must be specific

## Common Patterns

### Pydantic Models

```python
from __future__ import annotations

from pydantic import BaseModel, Field

class SearchRequest(BaseModel):
    """Request schema for search endpoint."""

    query: str = Field(..., min_length=1, max_length=500)
    limit: int = Field(10, ge=1, le=100)
    filters: dict[str, str | int | list[str]] | None = None
```

### Database Models

```python
from __future__ import annotations

from sqlalchemy import String
from sqlalchemy.orm import Mapped, mapped_column

from src.db.base import Base

class File(Base):
    """File database model."""

    __tablename__ = "files"

    id: Mapped[int] = mapped_column(primary_key=True)
    filename: Mapped[str] = mapped_column(String(255))
    path: Mapped[str] = mapped_column(String(1024))
```

### Async Functions

```python
from __future__ import annotations

from typing import AsyncGenerator

from sqlalchemy.ext.asyncio import AsyncSession

async def get_session() -> AsyncGenerator[AsyncSession, None]:
    """Get database session."""
    async with session_maker() as session:
        yield session
```

### Error Handling

```python
from __future__ import annotations

from fastapi import HTTPException

async def get_file(file_id: int) -> dict[str, str]:
    """Get file by ID."""
    file = await fetch_file(file_id)

    if not file:
        raise HTTPException(
            status_code=404,
            detail=f"File {file_id} not found"
        )

    return {
        "id": str(file.id),
        "name": file.name
    }
```

## Pre-Commit Hooks

The repository has pre-commit hooks to enforce type safety:

1. **check-future-annotations**: Ensures all Python files have future annotations
2. **mypy**: Runs strict type checking
3. **ruff**: Lints for type-related issues (TCH rules)

Run checks manually:
```bash
poetry run pre-commit run --all-files
```

## Checking Type Coverage

Use the provided script to check type hint coverage:

```bash
python check_type_hints.py src/api/
```

This will report:
- Files missing `from __future__ import annotations`
- Functions missing return type hints
- Overall type hint coverage

## Migration Guide

### Adding Future Annotations

1. Add import at the top of the file (after any shebang, before other imports):
   ```python
   from __future__ import annotations
   ```

2. Update type syntax:
   - `List[str]` → `list[str]`
   - `Dict[str, int]` → `dict[str, int]`
   - `Optional[str]` → `str | None`
   - `Union[str, int]` → `str | int`

### Adding Return Types

1. For route handlers, match the `response_model`:
   ```python
   @router.get("/items", response_model=ItemList)
   async def get_items() -> ItemList:
       ...
   ```

2. For dict returns, specify structure:
   ```python
   async def get_status() -> dict[str, str | int]:
       return {"status": "ok", "count": 42}
   ```

3. For None returns (side effects):
   ```python
   async def update_record(record_id: int) -> None:
       await db.update(record_id)
   ```

## Troubleshooting

### Error: "Name 'X' is not defined"

**Solution:** Add import or use forward reference:
```python
from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from module import X

def process() -> X:  # Forward reference works
    ...
```

### Error: "Incompatible return value type"

**Solution:** Check actual return matches declared type:
```python
# Wrong
async def get_count() -> int:
    return "42"  # String, not int!

# Right
async def get_count() -> int:
    return 42
```

### Error: "Missing type parameters for generic type"

**Solution:** Specify type parameters:
```python
# Wrong
def get_items() -> list:
    ...

# Right
def get_items() -> list[str]:
    ...
```

## Resources

- [PEP 563 - Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
- [Mypy Documentation](https://mypy.readthedocs.io/)
- [FastAPI Type Hints](https://fastapi.tiangolo.com/python-types/)
- [Pydantic Type Hints](https://docs.pydantic.dev/latest/usage/types/)

## Summary Checklist

- [ ] All files have `from __future__ import annotations`
- [ ] All public functions have return type hints
- [ ] All parameters have type hints
- [ ] Using modern type syntax (`list`, `dict`, `|`)
- [ ] TYPE_CHECKING used for expensive imports
- [ ] Pre-commit hooks pass
- [ ] Mypy strict mode passes

---

**Last Updated:** 2025-11-17
**Maintained by:** Vault Backend Team
