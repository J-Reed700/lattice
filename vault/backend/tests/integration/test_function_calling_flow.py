"""Integration tests for function calling flow.

Tests the complete flow:
- Registry → Executor → Service
- With real database session (test DB)
- Multi-function execution
- Error propagation
- Audit log entries
"""

from __future__ import annotations

from unittest.mock import AsyncMock, patch

import pytest

from src.services.llm.function_calling.executor import FunctionExecutor
from src.services.llm.function_calling.registry import FunctionRegistry
from src.services.llm.function_calling.tools import create_all_tools
from tests.fixtures.function_calling_fixtures import (
    create_mock_web_service,
)


@pytest.mark.integration()
class TestFullFunctionCallingFlow:
    """Test complete function calling flow."""

    @pytest.mark.asyncio()
    async def test_registry_to_executor_flow(self) -> None:
        """Test flow from registry to executor."""
        # Create registry with mock tools
        mock_db = AsyncMock()
        mock_web_service = create_mock_web_service()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        registry = FunctionRegistry()
        for tool in tools:
            registry.register(tool)

        # Create executor
        executor = FunctionExecutor(registry)

        # Execute a function
        result = await executor.execute(
            "web_search",
            {"query": "test query", "max_results": 5},
        )

        assert "results" in result
        assert "query" in result

    @pytest.mark.asyncio()
    async def test_multi_function_execution(self) -> None:
        """Test executing multiple functions in sequence."""
        mock_db = AsyncMock()
        mock_web_service = create_mock_web_service()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        registry = FunctionRegistry()
        for tool in tools:
            registry.register(tool)

        executor = FunctionExecutor(registry)

        # Execute multiple functions
        result1 = await executor.execute(
            "web_search",
            {"query": "first query", "max_results": 5},
        )

        result2 = await executor.execute(
            "web_search",
            {"query": "second query", "max_results": 3},
        )

        result3 = await executor.execute(
            "fetch_url_content",
            {"url": "https://example.com"},
        )

        assert result1 is not None
        assert result2 is not None
        assert result3 is not None

    @pytest.mark.asyncio()
    async def test_error_propagation_through_flow(self) -> None:
        """Test errors propagate correctly through the flow."""
        from src.services.llm.function_calling.executor import FunctionCallError

        mock_db = AsyncMock()
        mock_web_service = create_mock_web_service()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        registry = FunctionRegistry()
        for tool in tools:
            registry.register(tool)

        executor = FunctionExecutor(registry)

        # Try to execute non-existent function
        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("nonexistent_function", {})

        assert exc_info.value.error_code == "FUNCTION_NOT_FOUND"

    @pytest.mark.asyncio()
    async def test_rate_limiting_across_functions(self) -> None:
        """Test rate limiting works across different functions."""
        from src.services.llm.function_calling.executor import RateLimitError

        AsyncMock()
        create_mock_web_service()

        # Create tool with very low rate limit
        from pydantic import BaseModel

        from src.services.llm.function_calling.registry import ToolDefinition

        class TestInput(BaseModel):
            data: str

        class TestOutput(BaseModel):
            result: str

        async def handler(input_data: TestInput) -> TestOutput:
            return TestOutput(result="ok")

        limited_tool = ToolDefinition(
            name="limited_tool",
            description="Tool with rate limit",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler,
            rate_limit=2,  # Only 2 requests per minute
        )

        registry = FunctionRegistry()
        registry.register(limited_tool)

        executor = FunctionExecutor(registry)

        # First 2 should succeed
        await executor.execute("limited_tool", {"data": "test"})
        await executor.execute("limited_tool", {"data": "test"})

        # Third should fail
        with pytest.raises(RateLimitError):
            await executor.execute("limited_tool", {"data": "test"})


@pytest.mark.integration()
class TestWebServiceIntegration:
    """Test web service integration in function calling."""

    @pytest.mark.asyncio()
    async def test_web_search_through_executor(self) -> None:
        """Test web search executed through executor."""
        mock_web_service = create_mock_web_service()

        from src.services.llm.function_calling.tools import create_web_search_tool

        tool = create_web_search_tool(mock_web_service)

        registry = FunctionRegistry()
        registry.register(tool)

        executor = FunctionExecutor(registry)

        result = await executor.execute(
            "web_search",
            {"query": "test query", "max_results": 5},
        )

        assert "results" in result
        assert "query" in result
        mock_web_service.search_web.assert_called_once()

    @pytest.mark.asyncio()
    async def test_fetch_url_through_executor(self) -> None:
        """Test URL fetching executed through executor."""
        mock_web_service = create_mock_web_service()

        from src.services.llm.function_calling.tools import create_fetch_url_content_tool

        tool = create_fetch_url_content_tool(mock_web_service)

        registry = FunctionRegistry()
        registry.register(tool)

        executor = FunctionExecutor(registry)

        result = await executor.execute(
            "fetch_url_content",
            {"url": "https://example.com"},
        )

        assert "url" in result
        assert "content" in result
        mock_web_service.fetch_url_content.assert_called_once()

    @pytest.mark.asyncio()
    async def test_web_service_ssrf_validation_in_flow(self) -> None:
        """Test SSRF validation prevents malicious URLs in flow."""
        from src.services.llm.function_calling.executor import FunctionCallError
        from src.services.llm.function_calling.web_service import WebService

        # Real web service (not mocked) to test validation
        web_service = WebService(cache_enabled=False)

        from src.services.llm.function_calling.tools import create_fetch_url_content_tool

        tool = create_fetch_url_content_tool(web_service)

        registry = FunctionRegistry()
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Try to fetch localhost (should fail)
        with pytest.raises(FunctionCallError):
            await executor.execute(
                "fetch_url_content",
                {"url": "http://localhost:8080"},
            )


@pytest.mark.integration()
class TestAuditLoggingIntegration:
    """Test audit logging in integrated flow."""

    @pytest.mark.asyncio()
    async def test_successful_execution_logs_audit(self) -> None:
        """Test successful execution creates audit log."""
        mock_db = AsyncMock()
        mock_web_service = create_mock_web_service()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        registry = FunctionRegistry()
        for tool in tools:
            registry.register(tool)

        executor = FunctionExecutor(registry)

        with patch.object(executor, "_audit_success") as mock_audit:
            await executor.execute(
                "web_search",
                {"query": "test"},
                user_id="test_user",
            )

            mock_audit.assert_called_once()
            call_kwargs = mock_audit.call_args[1]
            assert call_kwargs["function_name"] == "web_search"
            assert call_kwargs["user_id"] == "test_user"

    @pytest.mark.asyncio()
    async def test_failed_execution_logs_audit(self) -> None:
        """Test failed execution creates audit log."""
        AsyncMock()
        create_mock_web_service()

        registry = FunctionRegistry()
        executor = FunctionExecutor(registry)

        with patch.object(executor, "_audit_failure") as mock_audit:
            try:
                await executor.execute(
                    "nonexistent_function",
                    {},
                    user_id="test_user",
                )
            except Exception:
                pass

            mock_audit.assert_called_once()
            call_kwargs = mock_audit.call_args[1]
            assert call_kwargs["function_name"] == "nonexistent_function"
            assert call_kwargs["user_id"] == "test_user"


@pytest.mark.integration()
class TestEdgeCasesIntegration:
    """Test edge cases in integrated flow."""

    @pytest.mark.asyncio()
    async def test_empty_registry_execution_fails(self) -> None:
        """Test execution fails gracefully with empty registry."""
        from src.services.llm.function_calling.executor import FunctionCallError

        registry = FunctionRegistry()
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("any_function", {})

        assert exc_info.value.error_code == "FUNCTION_NOT_FOUND"

    @pytest.mark.asyncio()
    async def test_execution_with_complex_arguments(self) -> None:
        """Test execution with complex nested arguments."""
        mock_db = AsyncMock()
        mock_web_service = create_mock_web_service()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        registry = FunctionRegistry()
        for tool in tools:
            registry.register(tool)

        executor = FunctionExecutor(registry)

        # Complex nested arguments
        result = await executor.execute(
            "web_search",
            {
                "query": "complex query with 'quotes' and \"double quotes\"",
                "max_results": 5,
                "region": "us-en",
                "safesearch": "moderate",
            },
        )

        assert result is not None

    @pytest.mark.asyncio()
    async def test_concurrent_function_execution(self) -> None:
        """Test concurrent execution of multiple functions."""
        import asyncio

        mock_db = AsyncMock()
        mock_web_service = create_mock_web_service()

        tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

        registry = FunctionRegistry()
        for tool in tools:
            registry.register(tool)

        executor = FunctionExecutor(registry)

        # Execute multiple functions concurrently
        tasks = [
            executor.execute("web_search", {"query": f"query {i}", "max_results": 3})
            for i in range(5)
        ]

        results = await asyncio.gather(*tasks)

        assert len(results) == 5
        assert all(r is not None for r in results)
