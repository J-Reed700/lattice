"""Unit tests for tool definitions and factories.

Tests:
- Tool creation functions
- Tool definition structure
- Tool metadata
- Input/output schema validation
- create_all_tools factory
"""

from __future__ import annotations

from unittest.mock import AsyncMock

from pydantic import BaseModel
import pytest

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
from src.services.llm.function_calling.registry import ToolDefinition
from src.services.llm.function_calling.tools import (
    create_all_tools,
    create_fetch_url_content_tool,
    create_get_document_tool,
    create_list_documents_tool,
    create_semantic_search_tool,
    create_web_search_tool,
)


@pytest.mark.unit()
class TestToolStructure:
    """Test tool definition structure."""

    def test_semantic_search_tool_structure(self) -> None:
        """Test semantic_search tool has correct structure."""
        mock_db = AsyncMock()
        tool = create_semantic_search_tool(mock_db)

        assert isinstance(tool, ToolDefinition)
        assert tool.name == "semantic_search"
        assert len(tool.description) > 0
        assert tool.input_schema == SemanticSearchInput
        assert tool.output_schema == SemanticSearchOutput
        assert callable(tool.handler)
        assert tool.rate_limit > 0

    def test_get_document_tool_structure(self) -> None:
        """Test get_document tool has correct structure."""
        mock_db = AsyncMock()
        tool = create_get_document_tool(mock_db)

        assert tool.name == "get_document"
        assert tool.input_schema == GetDocumentInput
        assert tool.output_schema == GetDocumentOutput
        assert callable(tool.handler)

    def test_list_documents_tool_structure(self) -> None:
        """Test list_documents tool has correct structure."""
        mock_db = AsyncMock()
        tool = create_list_documents_tool(mock_db)

        assert tool.name == "list_documents"
        assert tool.input_schema == ListDocumentsInput
        assert tool.output_schema == ListDocumentsOutput
        assert callable(tool.handler)

    def test_web_search_tool_structure(self) -> None:
        """Test web_search tool has correct structure."""
        mock_web_service = AsyncMock()
        tool = create_web_search_tool(mock_web_service)

        assert tool.name == "web_search"
        assert tool.input_schema == WebSearchInput
        assert tool.output_schema == WebSearchOutput
        assert callable(tool.handler)

    def test_fetch_url_content_tool_structure(self) -> None:
        """Test fetch_url_content tool has correct structure."""
        mock_web_service = AsyncMock()
        tool = create_fetch_url_content_tool(mock_web_service)

        assert tool.name == "fetch_url_content"
        assert tool.input_schema == FetchUrlContentInput
        assert tool.output_schema == FetchUrlContentOutput
        assert callable(tool.handler)


@pytest.mark.unit()
class TestToolMetadata:
    """Test tool metadata."""

    def test_semantic_search_metadata(self) -> None:
        """Test semantic_search tool metadata."""
        mock_db = AsyncMock()
        tool = create_semantic_search_tool(mock_db)

        assert "phase" in tool.metadata
        assert tool.metadata["phase"] == 1
        assert tool.metadata["category"] == "retrieval"

    def test_get_document_metadata(self) -> None:
        """Test get_document tool metadata."""
        mock_db = AsyncMock()
        tool = create_get_document_tool(mock_db)

        assert tool.metadata["phase"] == 1
        assert tool.metadata["category"] == "retrieval"

    def test_list_documents_metadata(self) -> None:
        """Test list_documents tool metadata."""
        mock_db = AsyncMock()
        tool = create_list_documents_tool(mock_db)

        assert tool.metadata["phase"] == 1
        assert tool.metadata["category"] == "retrieval"

    def test_web_search_metadata(self) -> None:
        """Test web_search tool metadata."""
        mock_web_service = AsyncMock()
        tool = create_web_search_tool(mock_web_service)

        assert tool.metadata["phase"] == 2
        assert tool.metadata["category"] == "web"

    def test_fetch_url_content_metadata(self) -> None:
        """Test fetch_url_content tool metadata."""
        mock_web_service = AsyncMock()
        tool = create_fetch_url_content_tool(mock_web_service)

        assert tool.metadata["phase"] == 2
        assert tool.metadata["category"] == "web"


@pytest.mark.unit()
class TestToolRateLimits:
    """Test tool rate limits."""

    def test_semantic_search_rate_limit(self) -> None:
        """Test semantic_search has appropriate rate limit."""
        mock_db = AsyncMock()
        tool = create_semantic_search_tool(mock_db)

        assert tool.rate_limit == 100  # 100 requests/minute

    def test_get_document_rate_limit(self) -> None:
        """Test get_document has higher rate limit."""
        mock_db = AsyncMock()
        tool = create_get_document_tool(mock_db)

        assert tool.rate_limit == 200  # Higher for simple retrieval

    def test_web_search_rate_limit(self) -> None:
        """Test web_search has lower rate limit (external API)."""
        mock_web_service = AsyncMock()
        tool = create_web_search_tool(mock_web_service)

        assert tool.rate_limit == 50  # Lower for external API

    def test_fetch_url_content_rate_limit(self) -> None:
        """Test fetch_url_content has lowest rate limit."""
        mock_web_service = AsyncMock()
        tool = create_fetch_url_content_tool(mock_web_service)

        assert tool.rate_limit == 30  # Lowest for fetching


@pytest.mark.unit()
class TestToolSchemas:
    """Test tool input/output schemas."""

    def test_all_tools_have_pydantic_schemas(self) -> None:
        """Test all tools use Pydantic models for schemas."""
        mock_db = AsyncMock()
        mock_web_service = AsyncMock()

        tools = [
            create_semantic_search_tool(mock_db),
            create_get_document_tool(mock_db),
            create_list_documents_tool(mock_db),
            create_web_search_tool(mock_web_service),
            create_fetch_url_content_tool(mock_web_service),
        ]

        for tool in tools:
            assert issubclass(tool.input_schema, BaseModel)
            assert issubclass(tool.output_schema, BaseModel)

    def test_schemas_can_generate_json_schema(self) -> None:
        """Test schemas can generate JSON schema for LLM providers."""
        mock_db = AsyncMock()
        tool = create_semantic_search_tool(mock_db)

        # Should not raise
        json_schema = tool.input_schema.model_json_schema()

        assert "properties" in json_schema
        assert "required" in json_schema


@pytest.mark.unit()
class TestCreateAllTools:
    """Test create_all_tools factory function."""

    def test_create_all_tools_with_db_only(self) -> None:
        """Test creating tools with only db_session."""
        mock_db = AsyncMock()

        tools = create_all_tools(db_session=mock_db, web_service=None)

        assert len(tools) == 3  # Only Phase 1 tools
        tool_names = {t.name for t in tools}
        assert tool_names == {"semantic_search", "get_document", "list_documents"}

    def test_create_all_tools_with_web_only(self) -> None:
        """Test creating tools with only web_service."""
        mock_web_service = AsyncMock()

        tools = create_all_tools(db_session=None, web_service=mock_web_service)

        assert len(tools) == 2  # Only Phase 2 tools
        tool_names = {t.name for t in tools}
        assert tool_names == {"web_search", "fetch_url_content"}

    def test_create_all_tools_with_both(self) -> None:
        """Test creating all tools with both dependencies."""
        mock_db = AsyncMock()
        mock_web_service = AsyncMock()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        assert len(tools) == 5  # All tools
        tool_names = {t.name for t in tools}
        assert tool_names == {
            "semantic_search",
            "get_document",
            "list_documents",
            "web_search",
            "fetch_url_content",
        }

    def test_create_all_tools_with_neither(self) -> None:
        """Test creating tools with no dependencies returns empty list."""
        tools = create_all_tools(db_session=None, web_service=None)

        assert len(tools) == 0


@pytest.mark.unit()
class TestToolDescriptions:
    """Test tool descriptions are informative."""

    def test_all_tools_have_descriptions(self) -> None:
        """Test all tools have non-empty descriptions."""
        mock_db = AsyncMock()
        mock_web_service = AsyncMock()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        for tool in tools:
            assert len(tool.description) > 20  # Meaningful description
            assert "." in tool.description  # Proper sentence

    def test_descriptions_explain_when_to_use(self) -> None:
        """Test descriptions include usage guidance."""
        mock_db = AsyncMock()
        tool = create_semantic_search_tool(mock_db)

        # Should mention when to use the tool
        assert "when" in tool.description.lower() or "use this" in tool.description.lower()

    def test_descriptions_are_provider_friendly(self) -> None:
        """Test descriptions are formatted for LLM providers."""
        mock_db = AsyncMock()
        mock_web_service = AsyncMock()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        for tool in tools:
            # Should not have technical jargon that confuses LLMs
            assert "async" not in tool.description.lower()
            assert "await" not in tool.description.lower()
            assert "coroutine" not in tool.description.lower()


@pytest.mark.unit()
class TestToolUniqueness:
    """Test tool uniqueness and naming."""

    def test_all_tool_names_unique(self) -> None:
        """Test all tool names are unique."""
        mock_db = AsyncMock()
        mock_web_service = AsyncMock()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        tool_names = [t.name for t in tools]
        assert len(tool_names) == len(set(tool_names))  # No duplicates

    def test_tool_names_follow_convention(self) -> None:
        """Test tool names follow snake_case convention."""
        mock_db = AsyncMock()
        mock_web_service = AsyncMock()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        for tool in tools:
            # Should be snake_case (no spaces, no camelCase)
            assert " " not in tool.name
            assert tool.name == tool.name.lower()
            assert "_" in tool.name or len(tool.name.split("_")) == 1
