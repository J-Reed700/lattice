"""Unit tests for function executor with security controls.

Tests:
- Rate limiting
- Input validation
- Output validation
- Audit logging (success/failure)
- Error handling
- Function execution
"""

from __future__ import annotations

import asyncio
from unittest.mock import patch

from pydantic import BaseModel
import pytest

from src.services.llm.function_calling.executor import (
    FunctionCallError,
    FunctionExecutor,
    RateLimiter,
    RateLimitError,
)
from src.services.llm.function_calling.registry import FunctionRegistry, ToolDefinition
from tests.fixtures.function_calling_fixtures import create_test_tool


@pytest.mark.unit()
class TestRateLimiter:
    """Test RateLimiter class."""

    @pytest.mark.asyncio()
    async def test_check_within_limit(self) -> None:
        """Test check passes when within rate limit."""
        limiter = RateLimiter()

        # Should succeed for first 10 requests
        for _ in range(10):
            await limiter.check("test_function", limit=10)

    @pytest.mark.asyncio()
    async def test_check_exceeds_limit(self) -> None:
        """Test check raises RateLimitError when limit exceeded."""
        limiter = RateLimiter()

        # First 10 should succeed
        for _ in range(10):
            await limiter.check("test_function", limit=10)

        # 11th should fail
        with pytest.raises(RateLimitError, match="Rate limit exceeded"):
            await limiter.check("test_function", limit=10)

    @pytest.mark.asyncio()
    async def test_check_sliding_window(self) -> None:
        """Test rate limiter uses sliding window (resets after 1 minute)."""
        limiter = RateLimiter()

        # Fill up the limit
        for _ in range(5):
            await limiter.check("test_function", limit=5)

        # Should fail
        with pytest.raises(RateLimitError):
            await limiter.check("test_function", limit=5)

        # Mock time passage by directly manipulating internal state
        # (In production, would need to wait 60+ seconds)
        limiter._requests["test_function"].clear()

        # Should succeed after reset
        await limiter.check("test_function", limit=5)

    @pytest.mark.asyncio()
    async def test_check_per_function_limits(self) -> None:
        """Test rate limits are tracked separately per function."""
        limiter = RateLimiter()

        # Fill limit for function1
        for _ in range(5):
            await limiter.check("function1", limit=5)

        # function1 should be blocked
        with pytest.raises(RateLimitError):
            await limiter.check("function1", limit=5)

        # function2 should still work
        await limiter.check("function2", limit=5)

    @pytest.mark.asyncio()
    async def test_reset_specific_function(self) -> None:
        """Test resetting rate limit for specific function."""
        limiter = RateLimiter()

        # Fill limit
        for _ in range(5):
            await limiter.check("test_function", limit=5)

        # Should fail
        with pytest.raises(RateLimitError):
            await limiter.check("test_function", limit=5)

        # Reset
        await limiter.reset("test_function")

        # Should work now
        await limiter.check("test_function", limit=5)

    @pytest.mark.asyncio()
    async def test_reset_all_functions(self) -> None:
        """Test resetting all rate limits."""
        limiter = RateLimiter()

        # Fill limits for multiple functions
        for _ in range(5):
            await limiter.check("function1", limit=5)
            await limiter.check("function2", limit=5)

        # Reset all
        await limiter.reset()

        # Both should work
        await limiter.check("function1", limit=5)
        await limiter.check("function2", limit=5)

    @pytest.mark.asyncio()
    async def test_concurrent_access(self) -> None:
        """Test rate limiter handles concurrent access correctly."""
        limiter = RateLimiter()

        # Concurrent requests
        tasks = [limiter.check("test_function", limit=10) for _ in range(10)]
        await asyncio.gather(*tasks)

        # 11th should fail
        with pytest.raises(RateLimitError):
            await limiter.check("test_function", limit=10)


@pytest.mark.unit()
class TestFunctionExecutor:
    """Test FunctionExecutor class."""

    @pytest.mark.asyncio()
    async def test_execute_successful(self) -> None:
        """Test successful function execution."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        result = await executor.execute("test_tool", {"query": "hello"})

        assert result == {"result": "Processed: hello"}

    @pytest.mark.asyncio()
    async def test_execute_function_not_found(self) -> None:
        """Test execution fails when function doesn't exist."""
        registry = FunctionRegistry()
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("nonexistent", {"query": "hello"})

        assert exc_info.value.error_code == "FUNCTION_NOT_FOUND"
        assert "nonexistent" in exc_info.value.message

    @pytest.mark.asyncio()
    async def test_execute_with_rate_limiting(self) -> None:
        """Test rate limiting is enforced during execution."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=5)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Execute 5 times (should succeed)
        for _ in range(5):
            await executor.execute("test_tool", {"query": "hello"})

        # 6th execution should fail with rate limit error
        with pytest.raises(RateLimitError, match="Rate limit exceeded"):
            await executor.execute("test_tool", {"query": "hello"})

    @pytest.mark.asyncio()
    async def test_execute_input_validation_success(self) -> None:
        """Test input validation with valid input."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        result = await executor.execute("test_tool", {"query": "valid input"})

        assert "result" in result

    @pytest.mark.asyncio()
    async def test_execute_input_validation_failure(self) -> None:
        """Test input validation with invalid input."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Missing required field 'query'
        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("test_tool", {})

        assert exc_info.value.error_code == "INVALID_INPUT"
        assert "validation failed" in exc_info.value.message.lower()

    @pytest.mark.asyncio()
    async def test_execute_output_validation(self) -> None:
        """Test output validation catches wrong output type."""

        class TestInput(BaseModel):
            query: str

        class TestOutput(BaseModel):
            result: str

        async def bad_handler(input_data: TestInput) -> str:  # type: ignore
            # Returns wrong type (str instead of TestOutput)
            return "wrong type"  # type: ignore

        tool = ToolDefinition(
            name="bad_tool",
            description="Tool with bad handler",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=bad_handler,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("bad_tool", {"query": "test"})

        assert exc_info.value.error_code == "INVALID_OUTPUT"

    @pytest.mark.asyncio()
    async def test_execute_with_user_id(self) -> None:
        """Test execution with user_id for audit logging."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        result = await executor.execute("test_tool", {"query": "hello"}, user_id="user123")

        assert result is not None

    @pytest.mark.asyncio()
    async def test_execute_file_not_found_error(self) -> None:
        """Test handling of FileNotFoundError."""

        class TestInput(BaseModel):
            path: str

        class TestOutput(BaseModel):
            content: str

        async def handler_raises_file_not_found(input_data: TestInput) -> TestOutput:
            raise FileNotFoundError("File not found")

        tool = ToolDefinition(
            name="file_tool",
            description="Tool that raises FileNotFoundError",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler_raises_file_not_found,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("file_tool", {"path": "/nonexistent"})

        assert exc_info.value.error_code == "RESOURCE_NOT_FOUND"

    @pytest.mark.asyncio()
    async def test_execute_value_error(self) -> None:
        """Test handling of ValueError."""

        class TestInput(BaseModel):
            value: int

        class TestOutput(BaseModel):
            result: int

        async def handler_raises_value_error(input_data: TestInput) -> TestOutput:
            raise ValueError("Invalid value")

        tool = ToolDefinition(
            name="value_tool",
            description="Tool that raises ValueError",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler_raises_value_error,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("value_tool", {"value": 42})

        assert exc_info.value.error_code == "INVALID_VALUE"

    @pytest.mark.asyncio()
    async def test_execute_runtime_error(self) -> None:
        """Test handling of RuntimeError."""

        class TestInput(BaseModel):
            data: str

        class TestOutput(BaseModel):
            result: str

        async def handler_raises_runtime_error(input_data: TestInput) -> TestOutput:
            raise RuntimeError("Something went wrong")

        tool = ToolDefinition(
            name="runtime_tool",
            description="Tool that raises RuntimeError",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler_raises_runtime_error,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("runtime_tool", {"data": "test"})

        assert exc_info.value.error_code == "EXECUTION_FAILED"

    @pytest.mark.asyncio()
    async def test_execute_unexpected_error(self) -> None:
        """Test handling of unexpected errors."""

        class TestInput(BaseModel):
            data: str

        class TestOutput(BaseModel):
            result: str

        async def handler_raises_unexpected(input_data: TestInput) -> TestOutput:
            raise KeyError("Unexpected error")

        tool = ToolDefinition(
            name="error_tool",
            description="Tool that raises unexpected error",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=handler_raises_unexpected,
        )

        registry = FunctionRegistry()
        registry.register(tool)
        executor = FunctionExecutor(registry)

        with pytest.raises(FunctionCallError) as exc_info:
            await executor.execute("error_tool", {"data": "test"})

        assert exc_info.value.error_code == "INTERNAL_ERROR"

    @pytest.mark.asyncio()
    async def test_audit_logging_success(self) -> None:
        """Test audit logging for successful execution."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool")
        registry.register(tool)

        executor = FunctionExecutor(registry)

        with patch.object(executor, "_audit_success") as mock_audit:
            await executor.execute("test_tool", {"query": "hello"}, user_id="user123")

            mock_audit.assert_called_once()
            call_args = mock_audit.call_args[1]
            assert call_args["function_name"] == "test_tool"
            assert call_args["user_id"] == "user123"
            assert "execution_time_ms" in call_args

    @pytest.mark.asyncio()
    async def test_audit_logging_failure(self) -> None:
        """Test audit logging for failed execution."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=0)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        with patch.object(executor, "_audit_failure") as mock_audit:
            try:
                await executor.execute("test_tool", {"query": "hello"}, user_id="user123")
            except RateLimitError:
                pass

            mock_audit.assert_called_once()
            call_args = mock_audit.call_args[1]
            assert call_args["function_name"] == "test_tool"
            assert call_args["user_id"] == "user123"
            assert "error" in call_args

    @pytest.mark.asyncio()
    async def test_reset_rate_limits(self) -> None:
        """Test resetting rate limits."""
        registry = FunctionRegistry()
        tool = create_test_tool(name="test_tool", rate_limit=1)
        registry.register(tool)

        executor = FunctionExecutor(registry)

        # Execute once
        await executor.execute("test_tool", {"query": "hello"})

        # Should fail on second attempt
        with pytest.raises(RateLimitError):
            await executor.execute("test_tool", {"query": "hello"})

        # Reset
        await executor.reset_rate_limits("test_tool")

        # Should work now
        result = await executor.execute("test_tool", {"query": "hello"})
        assert result is not None

    @pytest.mark.asyncio()
    async def test_reset_all_rate_limits(self) -> None:
        """Test resetting all rate limits."""
        registry = FunctionRegistry()
        tool1 = create_test_tool(name="tool1", rate_limit=1)
        tool2 = create_test_tool(name="tool2", rate_limit=1)
        registry.register(tool1)
        registry.register(tool2)

        executor = FunctionExecutor(registry)

        # Execute both once
        await executor.execute("tool1", {"query": "hello"})
        await executor.execute("tool2", {"query": "hello"})

        # Reset all
        await executor.reset_rate_limits()

        # Both should work
        await executor.execute("tool1", {"query": "hello"})
        await executor.execute("tool2", {"query": "hello"})


@pytest.mark.unit()
class TestFunctionCallError:
    """Test FunctionCallError exception class."""

    def test_error_initialization(self) -> None:
        """Test error initialization with all parameters."""
        error = FunctionCallError(
            error_code="TEST_ERROR",
            message="Test error message",
            details={"key": "value"},
        )

        assert error.error_code == "TEST_ERROR"
        assert error.message == "Test error message"
        assert error.details == {"key": "value"}

    def test_error_initialization_no_details(self) -> None:
        """Test error initialization without details."""
        error = FunctionCallError(
            error_code="TEST_ERROR",
            message="Test error message",
        )

        assert error.error_code == "TEST_ERROR"
        assert error.message == "Test error message"
        assert error.details == {}

    def test_error_string_representation(self) -> None:
        """Test error string representation."""
        error = FunctionCallError(
            error_code="TEST_ERROR",
            message="Test error message",
        )

        assert str(error) == "Test error message"


@pytest.mark.unit()
class TestExecutorEdgeCases:
    """Test edge cases and corner cases."""

    @pytest.mark.asyncio()
    async def test_execute_with_empty_arguments(self) -> None:
        """Test execution with empty arguments dict."""
        registry = FunctionRegistry()

        class EmptyInput(BaseModel):
            pass

        class EmptyOutput(BaseModel):
            status: str

        async def handler(input_data: EmptyInput) -> EmptyOutput:
            return EmptyOutput(status="ok")

        tool = ToolDefinition(
            name="empty_tool",
            description="Tool with empty input",
            input_schema=EmptyInput,
            output_schema=EmptyOutput,
            handler=handler,
        )

        registry.register(tool)
        executor = FunctionExecutor(registry)

        result = await executor.execute("empty_tool", {})
        assert result == {"status": "ok"}

    @pytest.mark.asyncio()
    async def test_execute_measures_execution_time(self) -> None:
        """Test that execution time is measured."""
        registry = FunctionRegistry()

        class TestInput(BaseModel):
            query: str

        class TestOutput(BaseModel):
            result: str

        async def slow_handler(input_data: TestInput) -> TestOutput:
            await asyncio.sleep(0.1)  # 100ms delay
            return TestOutput(result="done")

        tool = ToolDefinition(
            name="slow_tool",
            description="Slow tool",
            input_schema=TestInput,
            output_schema=TestOutput,
            handler=slow_handler,
        )

        registry.register(tool)
        executor = FunctionExecutor(registry)

        with patch.object(executor, "_audit_success") as mock_audit:
            await executor.execute("slow_tool", {"query": "test"})

            call_args = mock_audit.call_args[1]
            # Should have taken at least 100ms
            assert call_args["execution_time_ms"] >= 100
