"""Type Safety Pattern Examples.

This module demonstrates correct type safety patterns for the Vault backend.
Use these as templates when writing new code.
"""

from __future__ import annotations

from datetime import datetime, timezone
from typing import TYPE_CHECKING, AsyncGenerator
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field
from sqlalchemy import String, select
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.orm import Mapped, mapped_column

# TYPE_CHECKING imports (only for type hints, not runtime)
if TYPE_CHECKING:
    from src.services.search.service import SearchService

# Example 1: Pydantic Request/Response Models
# ============================================


class SearchRequest(BaseModel):
    """Example request schema with proper type hints."""

    query: str = Field(..., min_length=1, max_length=500, description="Search query")
    limit: int = Field(10, ge=1, le=100, description="Maximum results")
    filters: dict[str, str | int | list[str]] | None = Field(
        None, description="Optional filters"
    )


class SearchResult(BaseModel):
    """Example result item."""

    id: UUID
    filename: str
    score: float = Field(..., ge=0.0, le=1.0)
    metadata: dict[str, str | int | float]


class SearchResponse(BaseModel):
    """Example response schema."""

    results: list[SearchResult]
    total: int
    query: str
    took_ms: int


# Example 2: Database Models
# ===========================


class FileModel:
    """Example SQLAlchemy model with proper type hints."""

    __tablename__ = "files"

    id: Mapped[int] = mapped_column(primary_key=True)
    filename: Mapped[str] = mapped_column(String(255), nullable=False)
    path: Mapped[str] = mapped_column(String(1024), nullable=False)
    created_at: Mapped[datetime] = mapped_column(nullable=False)
    metadata_: Mapped[dict[str, str | int]] | None = mapped_column(nullable=True)


# Example 3: FastAPI Route Handlers
# ==================================

router = APIRouter(prefix="/examples", tags=["examples"])


@router.post("/search", response_model=SearchResponse)
async def search_endpoint(
    request: SearchRequest,
    session: AsyncSession = Depends(get_db_session),
) -> SearchResponse:
    """
    Example search endpoint with full type safety.

    Args:
        request: Validated search request
        session: Database session (injected)

    Returns:
        Search response with results and metadata

    Raises:
        HTTPException: On search errors
    """
    try:
        # Service call (type-checked)
        results = await perform_search(session, request.query, request.limit)

        return SearchResponse(
            results=results,
            total=len(results),
            query=request.query,
            took_ms=42,  # Example timing
        )

    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except Exception as e:
        raise HTTPException(status_code=500, detail="Search failed") from e


@router.get("/health")
async def health_check() -> dict[str, str]:
    """
    Example health check endpoint.

    Returns dict with specific structure instead of generic response_model.
    """
    return {
        "status": "healthy",
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "service": "recall-api",
    }


@router.get("/stats")
async def get_statistics() -> dict[str, int | float | list[str]]:
    """
    Example endpoint with complex return type.

    Shows how to type heterogeneous dicts properly.
    """
    return {
        "file_count": 1000,  # int
        "average_size": 2048.5,  # float
        "top_extensions": [".pdf", ".docx", ".txt"],  # list[str]
    }


# Example 4: Service Layer Classes
# =================================


class ExampleService:
    """Example service class with proper typing."""

    def __init__(self, session: AsyncSession) -> None:
        """
        Initialize service.

        Args:
            session: Database session for queries
        """
        self.session = session

    async def get_file_by_id(self, file_id: int) -> FileModel | None:
        """
        Get file by ID.

        Args:
            file_id: File database ID

        Returns:
            File model or None if not found
        """
        stmt = select(FileModel).where(FileModel.id == file_id)
        result = await self.session.execute(stmt)
        return result.scalar_one_or_none()

    async def search_files(
        self, query: str, limit: int = 10, filters: dict[str, str] | None = None
    ) -> list[SearchResult]:
        """
        Search for files.

        Args:
            query: Search text
            limit: Maximum results (default: 10)
            filters: Optional filter criteria

        Returns:
            List of search results ordered by relevance
        """
        # Implementation would go here
        return []

    async def bulk_update(
        self, updates: list[tuple[int, dict[str, str]]]
    ) -> dict[str, int]:
        """
        Bulk update files.

        Args:
            updates: List of (file_id, update_data) tuples

        Returns:
            Summary dict with counts
        """
        updated_count = 0
        failed_count = 0

        for file_id, update_data in updates:
            try:
                # Update logic here
                updated_count += 1
            except Exception:
                failed_count += 1

        return {"updated": updated_count, "failed": failed_count}


# Example 5: Dependency Injection
# ================================


async def get_db_session() -> AsyncGenerator[AsyncSession, None]:
    """
    Database session dependency.

    Yields async session for request scope.
    """
    # In real code, would use session maker
    session = AsyncSession()  # Example
    try:
        yield session
    finally:
        await session.close()


async def get_search_service(
    session: AsyncSession = Depends(get_db_session),
) -> SearchService:
    """
    Search service dependency.

    Args:
        session: Database session (injected)

    Returns:
        Configured search service instance
    """
    from src.services.search.service import SearchService  # noqa: PLC0415

    return SearchService(session)


# Example 6: Helper Functions
# ============================


def parse_filters(filter_str: str) -> dict[str, str | int | list[str]]:
    """
    Parse filter string into structured dict.

    Args:
        filter_str: Comma-separated filter string

    Returns:
        Parsed filter dictionary
    """
    # Implementation
    return {}


async def perform_search(
    session: AsyncSession, query: str, limit: int
) -> list[SearchResult]:
    """
    Perform database search.

    Args:
        session: Database session
        query: Search text
        limit: Maximum results

    Returns:
        List of search results
    """
    # Implementation
    return []


def format_timestamp(dt: datetime) -> str:
    """
    Format datetime to ISO string.

    Args:
        dt: Datetime to format

    Returns:
        ISO 8601 formatted string
    """
    return dt.isoformat()


# Example 7: Error Handling
# ==========================


class SearchError(Exception):
    """Custom exception for search errors."""

    def __init__(self, message: str, query: str | None = None) -> None:
        super().__init__(message)
        self.query = query


async def safe_search(query: str) -> list[SearchResult] | None:
    """
    Search with error handling.

    Args:
        query: Search query

    Returns:
        Results list or None on error
    """
    try:
        # Search logic
        return []
    except SearchError:
        return None


# Example 8: Generic Functions
# =============================


def paginate(
    items: list[SearchResult], page: int, page_size: int
) -> tuple[list[SearchResult], int]:
    """
    Paginate a list of items.

    Args:
        items: Full list to paginate
        page: Page number (1-indexed)
        page_size: Items per page

    Returns:
        Tuple of (page_items, total_pages)
    """
    start = (page - 1) * page_size
    end = start + page_size
    total_pages = (len(items) + page_size - 1) // page_size

    return items[start:end], total_pages


# Example 9: Async Generators
# ============================


async def stream_results(
    query: str, batch_size: int = 100
) -> AsyncGenerator[SearchResult, None]:
    """
    Stream search results in batches.

    Args:
        query: Search query
        batch_size: Results per batch

    Yields:
        Individual search results
    """
    offset = 0
    while True:
        # Fetch batch
        batch = await fetch_batch(query, offset, batch_size)

        if not batch:
            break

        for result in batch:
            yield result

        offset += batch_size


async def fetch_batch(query: str, offset: int, limit: int) -> list[SearchResult]:
    """Fetch a batch of results (example)."""
    return []


# Example 10: Configuration Classes
# ==================================


class ServiceConfig(BaseModel):
    """Example configuration with type safety."""

    database_url: str
    pool_size: int = Field(5, ge=1, le=100)
    timeout: float = Field(30.0, gt=0)
    features: dict[str, bool] = Field(default_factory=dict)
    allowed_extensions: list[str] = Field(default_factory=lambda: [".pdf", ".txt"])


def create_service(config: ServiceConfig) -> ExampleService:
    """
    Create service from config.

    Args:
        config: Service configuration

    Returns:
        Configured service instance
    """
    # Create session from config
    session = AsyncSession()  # Example
    return ExampleService(session)


# Summary
# =======
#
# Key Patterns Demonstrated:
# 1. ✅ from __future__ import annotations (first import)
# 2. ✅ Modern type syntax (list, dict, | for unions)
# 3. ✅ Return type hints on all functions
# 4. ✅ Parameter type hints
# 5. ✅ TYPE_CHECKING for forward references
# 6. ✅ Pydantic models for validation
# 7. ✅ SQLAlchemy 2.0 Mapped types
# 8. ✅ FastAPI dependency injection typing
# 9. ✅ Async generator typing
# 10. ✅ Generic types properly parameterized
#
# Remember:
# - All public functions MUST have return types
# - Use modern syntax: list not List, dict not Dict
# - Use str | None instead of Optional[str]
# - Document complex types in docstrings
# - Pre-commit hooks enforce these standards
