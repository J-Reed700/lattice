"""Test fixtures and factories for function calling tests.

Provides reusable test data, mock objects, and helper functions.
"""

from __future__ import annotations

from datetime import datetime
from unittest.mock import AsyncMock

from pydantic import BaseModel

from src.schemas.function_calling import (
    DocumentResult,
    FetchUrlContentInput,
    FetchUrlContentOutput,
    GetDocumentInput,
    GetDocumentOutput,
    SemanticSearchInput,
    SemanticSearchOutput,
    WebSearchInput,
    WebSearchOutput,
    WebSearchResult,
)
from src.services.llm.function_calling.registry import ToolDefinition

# =============================================================================
# Tool Definition Factories
# =============================================================================


def create_test_tool(
    name: str = "test_tool",
    description: str = "Test tool description",
    rate_limit: int = 100,
) -> ToolDefinition:
    """Create a test tool definition."""

    class TestInput(BaseModel):
        query: str

    class TestOutput(BaseModel):
        result: str

    async def handler(input_data: TestInput) -> TestOutput:
        return TestOutput(result=f"Processed: {input_data.query}")

    return ToolDefinition(
        name=name,
        description=description,
        input_schema=TestInput,
        output_schema=TestOutput,
        handler=handler,
        rate_limit=rate_limit,
        metadata={"test": True},
    )


def create_semantic_search_tool_mock() -> ToolDefinition:
    """Create mock semantic_search tool."""

    async def handler(input_data: SemanticSearchInput) -> SemanticSearchOutput:
        return SemanticSearchOutput(
            results=[
                DocumentResult(
                    document_id="doc_1",
                    filename="test.txt",
                    file_path="/path/to/test.txt",
                    mime_type="text/plain",
                    score=0.95,
                    snippet="Test document content",
                    chunk_index=0,
                    modified_at=datetime.now(),
                    size_bytes=1024,
                )
            ],
            total_found=1,
            search_time_ms=50.0,
            query=input_data.query,
        )

    return ToolDefinition(
        name="semantic_search",
        description="Search documents semantically",
        input_schema=SemanticSearchInput,
        output_schema=SemanticSearchOutput,
        handler=handler,
        rate_limit=100,
    )


# =============================================================================
# Schema Factories
# =============================================================================


def create_semantic_search_input(
    query: str = "test query",
    limit: int = 10,
    search_mode: str = "hybrid",
) -> SemanticSearchInput:
    """Create SemanticSearchInput instance."""
    return SemanticSearchInput(
        query=query,
        limit=limit,
        search_mode=search_mode,  # type: ignore
    )


def create_semantic_search_output(
    query: str = "test query",
    num_results: int = 1,
) -> SemanticSearchOutput:
    """Create SemanticSearchOutput instance."""
    results = [
        DocumentResult(
            document_id=f"doc_{i}",
            filename=f"test_{i}.txt",
            file_path=f"/path/to/test_{i}.txt",
            mime_type="text/plain",
            score=0.9 - (i * 0.1),
            snippet=f"Test content {i}",
            chunk_index=i,
            modified_at=datetime.now(),
            size_bytes=1024 * (i + 1),
        )
        for i in range(num_results)
    ]

    return SemanticSearchOutput(
        results=results,
        total_found=num_results,
        search_time_ms=50.0,
        query=query,
    )


def create_get_document_input(
    document_id: str = "doc_123",
    include_metadata: bool = True,
) -> GetDocumentInput:
    """Create GetDocumentInput instance."""
    return GetDocumentInput(
        document_id=document_id,
        include_metadata=include_metadata,
    )


def create_get_document_output(
    document_id: str = "doc_123",
    content: str = "Test document content",
    truncated: bool = False,
) -> GetDocumentOutput:
    """Create GetDocumentOutput instance."""
    return GetDocumentOutput(
        document_id=document_id,
        content=content,
        content_truncated=truncated,
        metadata=None,
    )


def create_web_search_input(
    query: str = "test web query",
    max_results: int = 5,
) -> WebSearchInput:
    """Create WebSearchInput instance."""
    return WebSearchInput(
        query=query,
        max_results=max_results,
    )


def create_web_search_output(
    query: str = "test web query",
    num_results: int = 3,
) -> WebSearchOutput:
    """Create WebSearchOutput instance."""
    results = [
        WebSearchResult(
            title=f"Test Result {i}",
            url=f"https://example.com/result{i}",
            snippet=f"This is test result {i} snippet",
        )
        for i in range(num_results)
    ]

    return WebSearchOutput(
        results=results,
        query=query,
        result_count=num_results,
    )


def create_fetch_url_content_input(
    url: str = "https://example.com/test",
    max_content_length: int = 50000,
) -> FetchUrlContentInput:
    """Create FetchUrlContentInput instance."""
    return FetchUrlContentInput(
        url=url,
        max_content_length=max_content_length,
    )


def create_fetch_url_content_output(
    url: str = "https://example.com/test",
    content: str = "Test page content",
    truncated: bool = False,
) -> FetchUrlContentOutput:
    """Create FetchUrlContentOutput instance."""
    return FetchUrlContentOutput(
        url=url,
        title="Test Page",
        content=content,
        content_truncated=truncated,
        word_count=len(content.split()),
        fetch_time_ms=100.0,
        content_type="text/html",
    )


# =============================================================================
# Mock Service Factories
# =============================================================================


def create_mock_search_service() -> AsyncMock:
    """Create mock SearchService."""
    mock = AsyncMock()
    mock.search = AsyncMock(return_value=create_semantic_search_output())
    return mock


def create_mock_document_service() -> AsyncMock:
    """Create mock DocumentService."""
    mock = AsyncMock()
    mock.get_document = AsyncMock(return_value=create_get_document_output())
    return mock


def create_mock_web_service() -> AsyncMock:
    """Create mock WebService."""
    mock = AsyncMock()
    mock.search_web = AsyncMock(return_value=create_web_search_output())
    mock.fetch_url_content = AsyncMock(return_value=create_fetch_url_content_output())
    mock.initialize = AsyncMock()
    mock.cleanup = AsyncMock()
    return mock


def create_mock_rate_limiter() -> AsyncMock:
    """Create mock RateLimiter."""
    mock = AsyncMock()
    mock.check = AsyncMock()
    mock.reset = AsyncMock()
    return mock


def create_mock_db_session() -> AsyncMock:
    """Create mock database session."""
    mock = AsyncMock()
    mock.execute = AsyncMock()
    mock.commit = AsyncMock()
    mock.rollback = AsyncMock()
    return mock


# =============================================================================
# Test HTML Content
# =============================================================================


def create_test_html(
    title: str = "Test Page",
    content: str = "This is test content",
) -> str:
    """Create test HTML document."""
    return f"""
<!DOCTYPE html>
<html>
<head>
    <title>{title}</title>
    <script>console.log('test');</script>
    <style>.test {{ color: red; }}</style>
</head>
<body>
    <article>
        <h1>Main Article</h1>
        <p>{content}</p>
        <p>Additional paragraph with more text.</p>
    </article>
</body>
</html>
"""


# =============================================================================
# Test Data Constants
# =============================================================================


VALID_URLS = [
    "https://example.com",
    "https://docs.python.org/3/",
    "http://www.google.com",
    "https://en.wikipedia.org/wiki/Python",
]

INVALID_URLS = [
    "http://localhost:8080",  # Localhost
    "http://127.0.0.1",  # Loopback
    "http://192.168.1.1",  # Private IP
    "http://10.0.0.1",  # Private IP
    "http://172.16.0.1",  # Private IP
    "http://169.254.169.254",  # Link-local (AWS metadata)
    "ftp://example.com",  # Wrong scheme
    "not-a-url",  # Invalid format
]

SAMPLE_QUERIES = [
    "machine learning papers",
    "project meeting notes",
    "Python tutorial",
    "weather forecast",
    "breaking news",
]
