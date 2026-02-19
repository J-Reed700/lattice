"""Tool definitions for all function calling capabilities.

This module defines the 5 callable tools:

Phase 1 (Core Retrieval):
- semantic_search: Search vault documents
- get_document: Retrieve full document content
- list_documents: Browse and filter documents

Phase 2 (Web Integration):
- web_search: Search the web via DuckDuckGo
- fetch_url_content: Fetch and extract URL content
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

from src.schemas.function_calling import (
    FetchUrlContentInput,
    FetchUrlContentOutput,
    GetDocumentInput,
    GetDocumentOutput,
    ListDocumentsInput,
    ListDocumentsOutput,
    SemanticSearchInput,
    SemanticSearchOutput,
    WebSearchInput,
    WebSearchOutput,
)

from .registry import ToolDefinition

if TYPE_CHECKING:
    from sqlalchemy.ext.asyncio import AsyncSession

    from .web_service import WebService


def validate_file_path(file_path: str | Path) -> Path:
    """
    Validate file path to prevent path traversal (CWE-22).

    This function:
    1. Resolves path to absolute path (handles .., symlinks)
    2. Ensures path exists
    3. Validates path is within allowed directories
    4. Prevents directory traversal attacks

    Args:
        file_path: File path to validate

    Returns:
        Validated absolute Path object

    Raises:
        ValueError: If path is invalid or outside allowed directories
        FileNotFoundError: If path doesn't exist
    """
    from src.config.settings import get_settings

    settings = get_settings()

    # Convert to Path and resolve to absolute path (handles .., symlinks)
    try:
        path = Path(file_path).resolve(strict=True)
    except (OSError, RuntimeError) as e:
        msg = f"Invalid file path: {file_path} - {e}"
        raise ValueError(msg) from e

    # Define allowed directories
    allowed_dirs = [
        Path(settings.storage_documents_path).resolve(),
        Path(settings.storage_screenshots_path).resolve(),
        Path(settings.storage_base_path).resolve(),
    ]

    # Check if path is within allowed directories
    is_allowed = False
    for allowed_dir in allowed_dirs:
        try:
            # Check if path is relative to allowed directory
            path.relative_to(allowed_dir)
            is_allowed = True
            break
        except ValueError:
            # Not relative to this directory, try next
            continue

    if not is_allowed:
        msg = (
            f"Access denied: {file_path} is outside allowed directories. "
            f"Allowed: {[str(d) for d in allowed_dirs]}"
        )
        raise ValueError(msg)

    return path


# =============================================================================
# Phase 1 Tools: Core Retrieval
# =============================================================================


def create_semantic_search_tool(db_session: AsyncSession) -> ToolDefinition:
    """
    Create semantic_search tool definition.

    Searches the user's document vault using semantic similarity.
    """

    async def handler(input_data: SemanticSearchInput) -> SemanticSearchOutput:
        """Execute semantic search on vault documents."""
        from datetime import datetime

        from src.modules.search_engine import SearchService

        # Initialize search service
        search_service = SearchService(db_session)

        # Execute search
        start_time = datetime.now()
        search_results = await search_service.search(
            query=input_data.query,
            mode=input_data.search_mode,
            limit=input_data.limit,
            offset=0,
            filters=None,  # TODO: Map from input filters
            rerank=True,
        )
        search_time_ms = (datetime.now() - start_time).total_seconds() * 1000

        # Map to output schema
        from src.schemas.function_calling import DocumentResult

        doc_results = []
        for r in search_results.results:
            doc_results.append(
                DocumentResult(
                    document_id=str(r.file_id),
                    filename=r.filename,
                    file_path=r.file_path,
                    mime_type=r.mime_type or "application/octet-stream",
                    score=r.score,
                    snippet=r.snippet[:500] if r.snippet else "",
                    chunk_index=r.chunk_index,
                    modified_at=r.modified_at or datetime.now(),
                    size_bytes=r.file_metadata.get("size_bytes", 0) if r.file_metadata else 0,
                )
            )

        return SemanticSearchOutput(
            results=doc_results,
            total_found=search_results.total_results,
            search_time_ms=search_time_ms,
            query=input_data.query,
        )

    return ToolDefinition(
        name="semantic_search",
        description=(
            "Search the user's document vault using semantic similarity. "
            "Use this when the user asks to find, search, or retrieve information from their documents. "
            "Supports semantic (embedding-based), keyword (BM25), and hybrid search modes. "
            "Returns relevant document excerpts with similarity scores."
        ),
        input_schema=SemanticSearchInput,
        output_schema=SemanticSearchOutput,
        handler=handler,
        rate_limit=100,  # 100 requests/minute
        metadata={"phase": 1, "category": "retrieval"},
    )


def create_get_document_tool(db_session: AsyncSession) -> ToolDefinition:
    """
    Create get_document tool definition.

    Retrieves the full content of a specific document by ID.
    """

    async def handler(input_data: GetDocumentInput) -> GetDocumentOutput:
        """Retrieve full document content by ID."""
        from sqlalchemy import select

        from src.db.models.file import File
        from src.schemas.function_calling import DocumentMetadata

        # Fetch document from database
        result = await db_session.execute(
            select(File).where(File.file_id == input_data.document_id)
        )
        doc = result.scalar_one_or_none()

        if not doc:
            msg = f"Document {input_data.document_id} not found"
            raise FileNotFoundError(msg)

        # Validate file path (prevents CWE-22 path traversal)
        try:
            validated_path = validate_file_path(doc.file_path)
        except ValueError as e:
            msg = f"Path validation failed: {e}"
            raise ValueError(msg) from e

        # Read file content
        try:
            content = validated_path.read_text(encoding="utf-8", errors="ignore")
        except Exception as e:
            msg = f"Failed to read file: {e}"
            raise RuntimeError(msg) from e

        # Truncate if needed
        truncated = False
        if len(content) > input_data.max_content_length:
            content = content[: input_data.max_content_length]
            truncated = True

        # Build metadata if requested
        metadata = None
        if input_data.include_metadata:
            metadata = DocumentMetadata(
                filename=doc.filename,
                file_path=doc.file_path,
                mime_type=doc.mime_type or "application/octet-stream",
                extension=validated_path.suffix.lstrip("."),
                size_bytes=doc.size_bytes or 0,
                created_at=doc.created_at,
                modified_at=doc.modified_at,
                indexed_at=doc.indexed_at,
                tags=[],  # TODO: Add tag support
                chunk_count=0,  # TODO: Query chunk count
            )

        return GetDocumentOutput(
            document_id=input_data.document_id,
            content=content,
            content_truncated=truncated,
            metadata=metadata,
        )

    return ToolDefinition(
        name="get_document",
        description=(
            "Retrieve the full content of a specific document by ID. "
            "Use this after semantic_search to get complete document text. "
            "Returns the full text content and optional metadata (file info, tags, etc.)."
        ),
        input_schema=GetDocumentInput,
        output_schema=GetDocumentOutput,
        handler=handler,
        rate_limit=200,  # 200 requests/minute
        metadata={"phase": 1, "category": "retrieval"},
    )


def create_list_documents_tool(db_session: AsyncSession) -> ToolDefinition:
    """
    Create list_documents tool definition.

    Browse and filter documents in the vault.
    """

    async def handler(input_data: ListDocumentsInput) -> ListDocumentsOutput:
        """List and filter documents in vault."""
        from sqlalchemy import func, select

        from src.db.models.file import File
        from src.schemas.function_calling import DocumentListItem

        # Build base query
        query = select(File)

        # Apply filters based on mode
        if input_data.filter_mode == "recent":
            query = query.order_by(File.modified_at.desc())
        elif input_data.filter_mode == "favorites":
            # TODO: Add favorites support
            query = query.order_by(File.modified_at.desc())
        elif input_data.filter_mode == "by_tag":
            # TODO: Add tag filtering
            query = query.order_by(File.modified_at.desc())
        elif input_data.filter_mode == "by_type":
            if input_data.file_types:
                # Filter by extensions
                extensions = [f".{ext.lstrip('.')}" for ext in input_data.file_types]
                query = query.where(func.lower(File.filename).op("~")(f"({'|'.join(extensions)})$"))

        # Apply date filters
        if input_data.date_from:
            query = query.where(File.modified_at >= input_data.date_from)
        if input_data.date_to:
            query = query.where(File.modified_at <= input_data.date_to)

        # Apply sorting
        if input_data.sort_by == "modified":
            sort_col = File.modified_at
        elif input_data.sort_by == "created":
            sort_col = File.created_at
        elif input_data.sort_by == "name":
            sort_col = File.filename
        elif input_data.sort_by == "size":
            sort_col = File.size_bytes
        else:
            sort_col = File.modified_at

        if input_data.sort_order == "desc":
            query = query.order_by(sort_col.desc())
        else:
            query = query.order_by(sort_col.asc())

        # Apply pagination
        query = query.limit(input_data.limit).offset(input_data.offset)

        # Execute query
        result = await db_session.execute(query)
        documents = result.scalars().all()

        # Count total
        count_query = select(func.count()).select_from(File)
        if input_data.date_from:
            count_query = count_query.where(File.modified_at >= input_data.date_from)
        if input_data.date_to:
            count_query = count_query.where(File.modified_at <= input_data.date_to)

        total_result = await db_session.execute(count_query)
        total = total_result.scalar() or 0

        # Map to output
        doc_items = []
        for doc in documents:
            from pathlib import Path

            doc_items.append(
                DocumentListItem(
                    document_id=doc.file_id,
                    filename=doc.filename,
                    file_path=doc.file_path,
                    mime_type=doc.mime_type or "application/octet-stream",
                    extension=Path(doc.filename).suffix.lstrip("."),
                    size_bytes=doc.size_bytes or 0,
                    modified_at=doc.modified_at,
                    tags=[],  # TODO: Add tag support
                    is_favorite=False,  # TODO: Add favorites
                    access_count=0,  # TODO: Add access tracking
                )
            )

        return ListDocumentsOutput(
            documents=doc_items,
            total=total,
            limit=input_data.limit,
            offset=input_data.offset,
            has_more=(input_data.offset + len(doc_items)) < total,
        )

    return ToolDefinition(
        name="list_documents",
        description=(
            "Browse and filter documents in the vault. "
            "Use this to explore what documents exist, filter by type/date/tags, "
            "or get recent/favorite documents. Supports pagination and sorting."
        ),
        input_schema=ListDocumentsInput,
        output_schema=ListDocumentsOutput,
        handler=handler,
        rate_limit=200,  # 200 requests/minute
        metadata={"phase": 1, "category": "retrieval"},
    )


# =============================================================================
# Phase 2 Tools: Web Integration
# =============================================================================


def create_web_search_tool(web_service: WebService) -> ToolDefinition:
    """
    Create web_search tool definition.

    Searches the web using DuckDuckGo.
    """

    async def handler(input_data: WebSearchInput) -> WebSearchOutput:
        """Search the web via DuckDuckGo."""
        return await web_service.search_web(input_data)

    return ToolDefinition(
        name="web_search",
        description=(
            "Search the web using DuckDuckGo when information isn't in the vault. "
            "Use this when the user asks about current events, external information, "
            "or when vault search returns no results. Returns web page titles, URLs, and snippets."
        ),
        input_schema=WebSearchInput,
        output_schema=WebSearchOutput,
        handler=handler,
        rate_limit=10,  # 10 requests/minute (reduced for security)
        metadata={"phase": 2, "category": "web"},
    )


def create_fetch_url_content_tool(web_service: WebService) -> ToolDefinition:
    """
    Create fetch_url_content tool definition.

    Fetches and extracts readable text from a web URL.
    """

    async def handler(input_data: FetchUrlContentInput) -> FetchUrlContentOutput:
        """Fetch and extract content from URL."""
        return await web_service.fetch_url_content(input_data)

    return ToolDefinition(
        name="fetch_url_content",
        description=(
            "Fetch and extract readable text from a web URL. "
            "Use this after web_search to get full content from interesting results, "
            "or when the user provides a URL to read/analyze. "
            "Extracts main article content and handles various HTML formats."
        ),
        input_schema=FetchUrlContentInput,
        output_schema=FetchUrlContentOutput,
        handler=handler,
        rate_limit=5,  # 5 requests/minute (reduced for security)
        metadata={"phase": 2, "category": "web"},
    )


# =============================================================================
# Factory Function
# =============================================================================


def create_all_tools(
    db_session: AsyncSession | None = None,
    web_service: WebService | None = None,
) -> list[ToolDefinition]:
    """
    Create all tool definitions.

    Args:
        db_session: Database session for Phase 1 tools
        web_service: Web service for Phase 2 tools

    Returns:
        List of all tool definitions

    Note:
        If db_session or web_service is None, corresponding tools will be skipped.
    """
    tools = []

    # Phase 1 tools (require db_session)
    if db_session is not None:
        tools.extend(
            [
                create_semantic_search_tool(db_session),
                create_get_document_tool(db_session),
                create_list_documents_tool(db_session),
            ]
        )

    # Phase 2 tools (require web_service)
    if web_service is not None:
        tools.extend(
            [
                create_web_search_tool(web_service),
                create_fetch_url_content_tool(web_service),
            ]
        )

    return tools
