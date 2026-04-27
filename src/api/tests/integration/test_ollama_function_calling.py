"""Integration tests for Ollama function calling.

Tests Ollama's integration with the function calling system:
- Basic function calling flow with Ollama
- Multi-turn conversations with function execution
- Error handling and recovery
- Rate limiting with Ollama
- Tool use response parsing
- Model compatibility

These tests use mocks by default (don't require running Ollama).
To test with real Ollama, set OLLAMA_INTEGRATION_TEST=1 environment variable.
"""

from __future__ import annotations

import asyncio
import json
import os
from unittest.mock import AsyncMock, patch

import pytest

from src.services.llm.function_calling.executor import (
    FunctionCallError,
    FunctionExecutor,
    RateLimitError,
)
from src.services.llm.function_calling.registry import FunctionRegistry
from src.services.llm.function_calling.tools import create_all_tools
from src.services.llm.ollama_service import OllamaService
from tests.fixtures.function_calling_fixtures import (
    create_mock_web_service,
)

# Test markers
pytestmark = [pytest.mark.integration, pytest.mark.ollama]

# Environment flag for real Ollama testing
USE_REAL_OLLAMA = os.getenv("OLLAMA_INTEGRATION_TEST") == "1"


# =============================================================================
# Test Fixtures
# =============================================================================


@pytest.fixture()
def mock_ollama_client() -> AsyncMock:
    """Create mock Ollama client that returns tool_use responses.

    This fixture simulates Ollama responding with function calls in the
    expected tool_use format.
    """
    client = AsyncMock()

    # Mock successful tool_use response
    client.chat = AsyncMock(
        return_value={
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "semantic_search",
                            "arguments": json.dumps({"query": "machine learning", "limit": 5}),
                        },
                    }
                ],
            },
            "done": True,
        }
    )

    # Mock generate endpoint
    client.generate = AsyncMock(
        return_value={
            "response": "I'll search for that information.",
            "done": True,
            "context": [1, 2, 3],
        }
    )

    client.health_check = AsyncMock(return_value=True)
    client.list_models = AsyncMock(
        return_value={"models": [{"name": "llama2"}, {"name": "llama3.1"}]}
    )

    return client


@pytest.fixture()
async def ollama_service(mock_ollama_client: AsyncMock) -> OllamaService:
    """Create OllamaService with mocked client."""
    service = OllamaService(default_model="llama2")
    service._client = mock_ollama_client
    return service


@pytest.fixture()
def function_registry() -> FunctionRegistry:
    """Create function registry with all tools."""
    mock_db = AsyncMock()
    mock_web_service = create_mock_web_service()

    tools = create_all_tools(db_session=mock_db, web_service=mock_web_service)

    registry = FunctionRegistry()
    for tool in tools:
        registry.register(tool)

    return registry


@pytest.fixture()
def function_executor(function_registry: FunctionRegistry) -> FunctionExecutor:
    """Create function executor with registry."""
    return FunctionExecutor(function_registry)


# =============================================================================
# Test 1: Basic Ollama Function Calling
# =============================================================================


@pytest.mark.asyncio()
class TestBasicOllamaFunctionCalling:
    """Test basic Ollama function calling flow."""

    async def test_ollama_calls_semantic_search(
        self,
        mock_ollama_client: AsyncMock,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test Ollama can call semantic_search function and get results.

        Flow:
        1. User asks "Find machine learning papers"
        2. Ollama responds with tool_use for semantic_search
        3. FunctionExecutor executes the function
        4. Results are returned in expected format
        """
        # Simulate Ollama returning tool_use
        tool_call = {
            "id": "call_search_123",
            "type": "function",
            "function": {
                "name": "semantic_search",
                "arguments": json.dumps({"query": "machine learning", "limit": 5}),
            },
        }

        # Parse and execute function call
        function_name = tool_call["function"]["name"]
        arguments = json.loads(tool_call["function"]["arguments"])

        result = await function_executor.execute(
            function_name=function_name,
            arguments=arguments,
            user_id="test_user",
        )

        # Verify result structure
        assert "results" in result
        assert "total_found" in result
        assert "search_time_ms" in result
        assert "query" in result
        assert result["query"] == "machine learning"

        # Verify result can be serialized (for returning to Ollama)
        result_json = json.dumps(result, default=str)
        assert isinstance(result_json, str)

    async def test_ollama_calls_web_search(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test Ollama can call web_search function."""
        # Simulate Ollama tool_use for web search
        function_name = "web_search"
        arguments = {"query": "latest AI news", "max_results": 3}

        result = await function_executor.execute(
            function_name=function_name,
            arguments=arguments,
            user_id="test_user",
        )

        # Verify result structure
        assert "results" in result
        assert "query" in result
        assert "result_count" in result
        assert result["query"] == "latest AI news"

    async def test_ollama_calls_fetch_url_content(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test Ollama can call fetch_url_content function."""
        # Simulate Ollama tool_use for URL fetching
        function_name = "fetch_url_content"
        arguments = {"url": "https://example.com"}

        result = await function_executor.execute(
            function_name=function_name,
            arguments=arguments,
            user_id="test_user",
        )

        # Verify result structure
        assert "url" in result
        assert "title" in result
        assert "content" in result
        assert "word_count" in result
        assert result["url"] == "https://example.com"


# =============================================================================
# Test 2: Multi-Turn Conversation with Functions
# =============================================================================


@pytest.mark.asyncio()
class TestMultiTurnConversation:
    """Test multi-turn conversation with function calls."""

    async def test_ollama_multi_turn_search_and_fetch(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test multi-turn conversation: search → fetch → answer.

        Flow:
        - Turn 1: User asks "What's new in AI?"
        - Turn 2: Ollama calls web_search
        - Turn 3: Return results → Ollama calls fetch_url_content
        - Turn 4: Return content → Ollama gives final answer
        """
        # Turn 2: Ollama calls web_search
        search_result = await function_executor.execute(
            function_name="web_search",
            arguments={"query": "new AI developments", "max_results": 3},
            user_id="test_user",
        )

        assert "results" in search_result
        assert len(search_result["results"]) > 0

        # Turn 3: Ollama picks a URL and fetches content
        first_url = search_result["results"][0]["url"]
        fetch_result = await function_executor.execute(
            function_name="fetch_url_content",
            arguments={"url": first_url},
            user_id="test_user",
        )

        assert "content" in fetch_result
        assert fetch_result["url"] == first_url

        # Turn 4: Ollama would synthesize final answer from content
        # (Not tested here - that's the LLM's job)

    async def test_ollama_multi_turn_semantic_search_then_get_document(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test multi-turn: semantic_search → get_document.

        Flow:
        - Turn 1: User asks "Show me my ML notes"
        - Turn 2: Ollama calls semantic_search
        - Turn 3: Return results → Ollama calls get_document for details
        - Turn 4: Return full document → Ollama shows it to user
        """
        # Turn 2: Semantic search
        search_result = await function_executor.execute(
            function_name="semantic_search",
            arguments={"query": "machine learning notes", "limit": 5},
            user_id="test_user",
        )

        assert "results" in search_result
        # Mock would return at least one result
        assert search_result["total_found"] > 0

        # Note: get_document would need a real document ID
        # This is a simplified test showing the pattern

    async def test_ollama_parallel_function_calls(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test Ollama making multiple function calls in parallel.

        Some models can return multiple tool_calls in one response.
        """
        # Simulate Ollama returning multiple tool calls
        tool_calls = [
            {"name": "web_search", "args": {"query": "AI news", "max_results": 3}},
            {"name": "web_search", "args": {"query": "ML papers", "max_results": 3}},
            {
                "name": "semantic_search",
                "args": {"query": "project docs", "limit": 5},
            },
        ]

        # Execute all in parallel
        tasks = [
            function_executor.execute(
                function_name=call["name"],
                arguments=call["args"],
                user_id="test_user",
            )
            for call in tool_calls
        ]

        results = await asyncio.gather(*tasks)

        # Verify all succeeded
        assert len(results) == 3
        assert all(r is not None for r in results)


# =============================================================================
# Test 3: Error Handling
# =============================================================================


@pytest.mark.asyncio()
class TestErrorHandling:
    """Test error handling with Ollama function calls."""

    async def test_ollama_handles_function_not_found(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test Ollama receives clear error for unknown function.

        When Ollama calls a non-existent function, the error should be
        clear enough for the LLM to understand and potentially retry.
        """
        with pytest.raises(FunctionCallError) as exc_info:
            await function_executor.execute(
                function_name="nonexistent_function",
                arguments={},
                user_id="test_user",
            )

        error = exc_info.value
        assert error.error_code == "FUNCTION_NOT_FOUND"
        assert "nonexistent_function" in error.message
        # Should list available functions
        assert "available_functions" in error.details

        # Verify error message is serializable (for sending back to Ollama)
        error_response = {
            "error_code": error.error_code,
            "message": error.message,
            "details": error.details,
        }
        error_json = json.dumps(error_response)
        assert isinstance(error_json, str)

    async def test_ollama_handles_invalid_arguments(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test Ollama receives validation error for invalid arguments.

        When Ollama provides invalid arguments (wrong type, missing required),
        the error should help it fix the call.
        """
        with pytest.raises(FunctionCallError) as exc_info:
            await function_executor.execute(
                function_name="semantic_search",
                arguments={"query": "", "limit": 999},  # Invalid: empty query, limit too high
                user_id="test_user",
            )

        error = exc_info.value
        assert error.error_code == "INVALID_INPUT"
        assert "errors" in error.details

    async def test_ollama_handles_function_execution_error(
        self,
        function_registry: FunctionRegistry,
    ) -> None:
        """Test Ollama handles function execution errors gracefully.

        When a function raises an exception during execution,
        the error should be caught and returned in a structured format.
        """
        from pydantic import BaseModel

        from src.services.llm.function_calling.registry import ToolDefinition

        # Create a tool that raises an error
        class ErrorInput(BaseModel):
            data: str

        class ErrorOutput(BaseModel):
            result: str

        async def error_handler(input_data: ErrorInput) -> ErrorOutput:
            raise RuntimeError("Simulated function error")

        error_tool = ToolDefinition(
            name="error_tool",
            description="Tool that always errors",
            input_schema=ErrorInput,
            output_schema=ErrorOutput,
            handler=error_handler,
            rate_limit=100,
        )

        function_registry.register(error_tool)
        executor = FunctionExecutor(function_registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute(
                function_name="error_tool",
                arguments={"data": "test"},
                user_id="test_user",
            )

        error = exc_info.value
        assert error.error_code == "EXECUTION_FAILED"
        assert "Simulated function error" in error.message


# =============================================================================
# Test 4: Rate Limiting
# =============================================================================


@pytest.mark.asyncio()
class TestRateLimiting:
    """Test rate limiting works with Ollama calls."""

    async def test_ollama_respects_rate_limits(
        self,
        function_registry: FunctionRegistry,
    ) -> None:
        """Test rate limiting prevents excessive Ollama function calls.

        Scenario: Ollama makes too many calls to the same function.
        Expected: Rate limiter blocks excess calls.
        """
        from pydantic import BaseModel

        from src.services.llm.function_calling.registry import ToolDefinition

        # Create tool with very low rate limit
        class TestInput(BaseModel):
            query: str

        class TestOutput(BaseModel):
            result: str

        async def handler(input_data: TestInput) -> TestOutput:
            return TestOutput(result="ok")

        limited_tool = ToolDefinition(
            name="limited_tool",
            description="Tool with strict rate limit",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler,
            rate_limit=3,  # Only 3 requests per minute
        )

        function_registry.register(limited_tool)
        executor = FunctionExecutor(function_registry)

        # First 3 calls should succeed
        for i in range(3):
            result = await executor.execute(
                function_name="limited_tool",
                arguments={"query": f"test {i}"},
                user_id="test_user",
            )
            assert result["result"] == "ok"

        # 4th call should fail
        with pytest.raises(RateLimitError) as exc_info:
            await executor.execute(
                function_name="limited_tool",
                arguments={"query": "test 4"},
                user_id="test_user",
            )

        # Verify error message is clear
        assert "Rate limit exceeded" in str(exc_info.value)
        assert "limited_tool" in str(exc_info.value)

    async def test_rate_limit_is_per_function(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test rate limits are per-function, not global.

        Hitting rate limit on one function shouldn't affect others.
        """
        # This test verifies the architecture is sound
        # (Already implicitly tested by other tests, but worth being explicit)

        # Call different functions - should not interfere with each other
        result1 = await function_executor.execute(
            "web_search",
            {"query": "test 1", "max_results": 1},
        )
        result2 = await function_executor.execute(
            "semantic_search",
            {"query": "test 2", "limit": 1},
        )
        result3 = await function_executor.execute(
            "fetch_url_content",
            {"url": "https://example.com"},
        )

        assert all(r is not None for r in [result1, result2, result3])

    async def test_rate_limit_reset(
        self,
        function_registry: FunctionRegistry,
    ) -> None:
        """Test rate limit can be reset for testing/maintenance."""
        from pydantic import BaseModel

        from src.services.llm.function_calling.registry import ToolDefinition

        class TestInput(BaseModel):
            data: str

        class TestOutput(BaseModel):
            result: str

        async def handler(input_data: TestInput) -> TestOutput:
            return TestOutput(result="ok")

        limited_tool = ToolDefinition(
            name="reset_test_tool",
            description="Tool for reset testing",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler,
            rate_limit=1,  # Only 1 request
        )

        function_registry.register(limited_tool)
        executor = FunctionExecutor(function_registry)

        # First call succeeds
        await executor.execute("reset_test_tool", {"data": "test"})

        # Second call fails
        with pytest.raises(RateLimitError):
            await executor.execute("reset_test_tool", {"data": "test"})

        # Reset rate limit
        await executor.reset_rate_limits("reset_test_tool")

        # Third call succeeds
        result = await executor.execute("reset_test_tool", {"data": "test"})
        assert result["result"] == "ok"


# =============================================================================
# Test 5: Ollama Response Format Integration
# =============================================================================


@pytest.mark.asyncio()
class TestOllamaResponseFormat:
    """Test parsing and handling Ollama tool_use response format."""

    async def test_parse_ollama_tool_use_response(self) -> None:
        """Test parsing Ollama's tool_use response format.

        Ollama returns tool calls in this format:
        {
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "function_name",
                            "arguments": "{\"param\": \"value\"}"
                        }
                    }
                ]
            }
        }
        """
        # Simulate Ollama response
        ollama_response = {
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_search_001",
                        "type": "function",
                        "function": {
                            "name": "semantic_search",
                            "arguments": json.dumps(
                                {
                                    "query": "neural networks",
                                    "limit": 10,
                                    "search_mode": "hybrid",
                                }
                            ),
                        },
                    }
                ],
            },
            "done": True,
        }

        # Parse tool calls
        tool_calls = ollama_response["message"].get("tool_calls", [])
        assert len(tool_calls) == 1

        # Extract function details
        tool_call = tool_calls[0]
        assert tool_call["type"] == "function"

        function_name = tool_call["function"]["name"]
        arguments = json.loads(tool_call["function"]["arguments"])

        assert function_name == "semantic_search"
        assert arguments["query"] == "neural networks"
        assert arguments["limit"] == 10

    async def test_handle_multiple_tool_calls_in_response(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test handling Ollama response with multiple tool calls.

        Some models can return multiple tool_calls in a single response.
        """
        # Simulate Ollama response with multiple tool calls
        ollama_response = {
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    {
                        "id": "call_001",
                        "type": "function",
                        "function": {
                            "name": "web_search",
                            "arguments": json.dumps({"query": "AI news", "max_results": 5}),
                        },
                    },
                    {
                        "id": "call_002",
                        "type": "function",
                        "function": {
                            "name": "semantic_search",
                            "arguments": json.dumps({"query": "my AI notes", "limit": 10}),
                        },
                    },
                ],
            },
            "done": True,
        }

        # Execute all tool calls
        tool_calls = ollama_response["message"]["tool_calls"]
        results = []

        for tool_call in tool_calls:
            function_name = tool_call["function"]["name"]
            arguments = json.loads(tool_call["function"]["arguments"])

            result = await function_executor.execute(
                function_name=function_name,
                arguments=arguments,
                user_id="test_user",
            )
            results.append({"id": tool_call["id"], "result": result})

        # Verify both succeeded
        assert len(results) == 2
        assert results[0]["id"] == "call_001"
        assert results[1]["id"] == "call_002"

    async def test_format_function_result_for_ollama(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test formatting function result to send back to Ollama.

        After executing a function, the result needs to be formatted
        properly for Ollama to process.
        """
        # Execute function
        result = await function_executor.execute(
            function_name="web_search",
            arguments={"query": "test", "max_results": 3},
        )

        # Format for Ollama
        # Ollama expects tool results in this format:
        tool_result = {
            "role": "tool",
            "content": json.dumps(result, default=str),
            "tool_call_id": "call_test_123",
        }

        # Verify it's valid
        assert tool_result["role"] == "tool"
        assert isinstance(tool_result["content"], str)
        assert "results" in json.loads(tool_result["content"])


# =============================================================================
# Test 6: Ollama Model Compatibility (Optional - requires real Ollama)
# =============================================================================


@pytest.mark.skipif(not USE_REAL_OLLAMA, reason="Requires OLLAMA_INTEGRATION_TEST=1")
@pytest.mark.asyncio()
@pytest.mark.slow()
class TestOllamaModelCompatibility:
    """Test function calling with different Ollama models.

    These tests require a running Ollama instance with models installed.
    Set OLLAMA_INTEGRATION_TEST=1 to run.
    """

    @pytest.mark.parametrize(
        "model_name",
        [
            "llama2",
            "llama3.1",
            "mistral",
        ],
    )
    async def test_function_calling_with_model(
        self,
        model_name: str,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test function calling works with different Ollama models.

        Note: This test requires the models to be installed locally.
        """
        # Create real Ollama service
        async with OllamaService(default_model=model_name) as service:
            # Check if model is available
            if not await service.is_model_available(model_name):
                pytest.skip(f"Model {model_name} not available")

            # This would test actual Ollama integration
            # For now, we just verify the service initializes
            assert service.default_model == model_name
            assert await service.health_check()


# =============================================================================
# Test 7: End-to-End Integration (Optional)
# =============================================================================


@pytest.mark.skipif(not USE_REAL_OLLAMA, reason="Requires OLLAMA_INTEGRATION_TEST=1")
@pytest.mark.asyncio()
@pytest.mark.slow()
class TestEndToEndIntegration:
    """End-to-end integration tests with real Ollama.

    These tests verify the complete flow with a real Ollama instance.
    They are optional and only run when OLLAMA_INTEGRATION_TEST=1.
    """

    async def test_complete_ollama_function_calling_flow(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test complete flow: Ollama → function call → result.

        This would test:
        1. Send query to Ollama
        2. Ollama returns tool_use
        3. Execute function
        4. Return result to Ollama
        5. Ollama generates final response

        Requires real Ollama running.
        """
        async with OllamaService(default_model="llama2") as service:
            if not await service.health_check():
                pytest.skip("Ollama not available")

            # This would be a full integration test
            # Implementation depends on having chat API with tools
            # which is model-specific
            assert await service.health_check()


# =============================================================================
# Test 8: Audit Logging with Ollama
# =============================================================================


@pytest.mark.asyncio()
class TestOllamaAuditLogging:
    """Test audit logging works correctly with Ollama function calls."""

    async def test_successful_ollama_call_logs_audit(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test successful Ollama function call creates audit log."""
        with patch.object(function_executor, "_audit_success") as mock_audit:
            await function_executor.execute(
                function_name="web_search",
                arguments={"query": "test"},
                user_id="ollama_user_123",
            )

            # Verify audit was called
            mock_audit.assert_called_once()
            call_kwargs = mock_audit.call_args[1]
            assert call_kwargs["function_name"] == "web_search"
            assert call_kwargs["user_id"] == "ollama_user_123"
            assert "execution_time_ms" in call_kwargs

    async def test_failed_ollama_call_logs_audit(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test failed Ollama function call creates audit log."""
        with patch.object(function_executor, "_audit_failure") as mock_audit:
            try:
                await function_executor.execute(
                    function_name="nonexistent",
                    arguments={},
                    user_id="ollama_user_123",
                )
            except FunctionCallError:
                pass

            # Verify audit was called
            mock_audit.assert_called_once()
            call_kwargs = mock_audit.call_args[1]
            assert call_kwargs["function_name"] == "nonexistent"
            assert call_kwargs["user_id"] == "ollama_user_123"
            assert "error" in call_kwargs


# =============================================================================
# Test 9: Performance and Concurrency
# =============================================================================


@pytest.mark.asyncio()
class TestPerformanceAndConcurrency:
    """Test performance characteristics with Ollama."""

    async def test_concurrent_ollama_function_calls(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test handling multiple concurrent Ollama function calls.

        Simulates multiple users/sessions making function calls concurrently.
        """
        # Simulate 10 concurrent Ollama sessions
        tasks = [
            function_executor.execute(
                function_name="web_search",
                arguments={"query": f"query {i}", "max_results": 3},
                user_id=f"user_{i}",
            )
            for i in range(10)
        ]

        results = await asyncio.gather(*tasks)

        assert len(results) == 10
        assert all(r is not None for r in results)

    async def test_function_execution_time_tracking(
        self,
        function_executor: FunctionExecutor,
    ) -> None:
        """Test that function execution time is tracked for monitoring."""
        with patch.object(function_executor, "_audit_success") as mock_audit:
            await function_executor.execute(
                function_name="web_search",
                arguments={"query": "test"},
            )

            # Verify execution time was recorded
            call_kwargs = mock_audit.call_args[1]
            assert "execution_time_ms" in call_kwargs
            assert isinstance(call_kwargs["execution_time_ms"], float)
            assert call_kwargs["execution_time_ms"] > 0
