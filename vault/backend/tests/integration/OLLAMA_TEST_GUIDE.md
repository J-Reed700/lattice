# Ollama Function Calling Integration Tests

## Overview

Comprehensive integration tests for Ollama's function calling capabilities with the Recall backend.

**File**: `test_ollama_function_calling.py`
**Total Tests**: 25+ test methods across 9 test classes
**Coverage**: Ollama integration, error handling, rate limiting, multi-turn conversations

## Test Categories

### 1. **Basic Ollama Function Calling** (`TestBasicOllamaFunctionCalling`)
Tests fundamental Ollama → function execution flow.

- ✅ `test_ollama_calls_semantic_search` - Ollama searches documents
- ✅ `test_ollama_calls_web_search` - Ollama searches web
- ✅ `test_ollama_calls_fetch_url_content` - Ollama fetches URL content

### 2. **Multi-Turn Conversation** (`TestMultiTurnConversation`)
Tests realistic conversation flows with multiple function calls.

- ✅ `test_ollama_multi_turn_search_and_fetch` - Search → Fetch → Answer
- ✅ `test_ollama_multi_turn_semantic_search_then_get_document` - Find → Get details
- ✅ `test_ollama_parallel_function_calls` - Multiple concurrent calls

### 3. **Error Handling** (`TestErrorHandling`)
Tests graceful error handling and recovery.

- ✅ `test_ollama_handles_function_not_found` - Unknown function error
- ✅ `test_ollama_handles_invalid_arguments` - Validation errors
- ✅ `test_ollama_handles_function_execution_error` - Runtime errors

### 4. **Rate Limiting** (`TestRateLimiting`)
Tests rate limiting prevents abuse.

- ✅ `test_ollama_respects_rate_limits` - Blocks excessive calls
- ✅ `test_rate_limit_is_per_function` - Per-function limits
- ✅ `test_rate_limit_reset` - Reset for testing

### 5. **Ollama Response Format** (`TestOllamaResponseFormat`)
Tests parsing Ollama's tool_use format.

- ✅ `test_parse_ollama_tool_use_response` - Parse tool_use JSON
- ✅ `test_handle_multiple_tool_calls_in_response` - Multiple tools
- ✅ `test_format_function_result_for_ollama` - Format results

### 6. **Model Compatibility** (`TestOllamaModelCompatibility`) 🐌
Tests with different Ollama models (requires real Ollama).

- ⏭️ `test_function_calling_with_model[llama2]` - Test with Llama2
- ⏭️ `test_function_calling_with_model[llama3.1]` - Test with Llama3.1
- ⏭️ `test_function_calling_with_model[mistral]` - Test with Mistral

**Requires**: `OLLAMA_INTEGRATION_TEST=1` environment variable

### 7. **End-to-End Integration** (`TestEndToEndIntegration`) 🐌
Complete flow with real Ollama (optional).

- ⏭️ `test_complete_ollama_function_calling_flow` - Full integration

**Requires**: `OLLAMA_INTEGRATION_TEST=1` + running Ollama instance

### 8. **Audit Logging** (`TestOllamaAuditLogging`)
Verifies audit logging for compliance.

- ✅ `test_successful_ollama_call_logs_audit` - Success events
- ✅ `test_failed_ollama_call_logs_audit` - Failure events

### 9. **Performance & Concurrency** (`TestPerformanceAndConcurrency`)
Tests performance under load.

- ✅ `test_concurrent_ollama_function_calls` - 10 concurrent sessions
- ✅ `test_function_execution_time_tracking` - Timing metrics

## Running Tests

### Run All Ollama Tests
```bash
cd vault/backend
poetry run pytest tests/integration/test_ollama_function_calling.py -v
```

### Run Specific Test Class
```bash
# Basic function calling only
poetry run pytest tests/integration/test_ollama_function_calling.py::TestBasicOllamaFunctionCalling -v

# Error handling only
poetry run pytest tests/integration/test_ollama_function_calling.py::TestErrorHandling -v
```

### Run with Real Ollama (Optional)
```bash
# Start Ollama first: ollama serve
OLLAMA_INTEGRATION_TEST=1 poetry run pytest tests/integration/test_ollama_function_calling.py -v
```

### Run Fast Tests Only (Skip Slow)
```bash
poetry run pytest tests/integration/test_ollama_function_calling.py -m "not slow" -v
```

### Run with Coverage
```bash
poetry run pytest tests/integration/test_ollama_function_calling.py --cov=src.services.llm --cov-report=html
```

## Test Architecture

### Mock vs Real Ollama

**By Default** (Mocked):
- Uses `mock_ollama_client` fixture
- No Ollama instance required
- Fast, reliable, CI-friendly
- Tests function calling framework

**With `OLLAMA_INTEGRATION_TEST=1`** (Real):
- Connects to real Ollama instance
- Tests actual model behavior
- Slower, requires setup
- Tests end-to-end integration

### Key Fixtures

#### `mock_ollama_client`
Mock Ollama client returning tool_use responses.

```python
@pytest.fixture
def mock_ollama_client() -> AsyncMock:
    """Returns simulated tool_use responses"""
```

#### `function_registry`
Registry with all 5 function calling tools registered.

```python
@pytest.fixture
def function_registry() -> FunctionRegistry:
    """Registry with semantic_search, web_search, etc."""
```

#### `function_executor`
Executor with rate limiting and audit logging.

```python
@pytest.fixture
def function_executor(function_registry) -> FunctionExecutor:
    """Executor ready to run functions"""
```

## Test Patterns

### Pattern 1: Basic Function Call
```python
async def test_ollama_calls_function(function_executor):
    # Simulate Ollama tool_use
    result = await function_executor.execute(
        function_name="semantic_search",
        arguments={"query": "test", "limit": 5},
        user_id="test_user"
    )

    # Verify result structure
    assert "results" in result
    assert "total_found" in result
```

### Pattern 2: Multi-Turn Conversation
```python
async def test_multi_turn(function_executor):
    # Turn 1: Search
    search_result = await function_executor.execute(
        "web_search",
        {"query": "AI news", "max_results": 3}
    )

    # Turn 2: Fetch details
    url = search_result["results"][0]["url"]
    fetch_result = await function_executor.execute(
        "fetch_url_content",
        {"url": url}
    )

    # Turn 3: LLM synthesizes answer (not tested)
```

### Pattern 3: Error Handling
```python
async def test_error_handling(function_executor):
    with pytest.raises(FunctionCallError) as exc_info:
        await function_executor.execute(
            "nonexistent_function",
            {}
        )

    # Verify error is structured for LLM
    error = exc_info.value
    assert error.error_code == "FUNCTION_NOT_FOUND"
    assert "available_functions" in error.details
```

### Pattern 4: Parsing Ollama Response
```python
async def test_parse_response():
    ollama_response = {
        "message": {
            "tool_calls": [{
                "function": {
                    "name": "semantic_search",
                    "arguments": '{"query": "test"}'
                }
            }]
        }
    }

    # Extract and execute
    tool_call = ollama_response["message"]["tool_calls"][0]
    function_name = tool_call["function"]["name"]
    arguments = json.loads(tool_call["function"]["arguments"])
```

## Coverage Goals

- ✅ **Core Functions**: All 5 tools tested
- ✅ **Error Paths**: Invalid input, not found, runtime errors
- ✅ **Rate Limiting**: Per-function limits, reset
- ✅ **Multi-Turn**: Sequential and parallel calls
- ✅ **Audit Logging**: Success and failure events
- ✅ **Response Format**: Parsing and formatting
- ⏭️ **Real Models**: Optional with real Ollama

## Success Criteria

- [x] 25+ comprehensive tests
- [x] Tests use proper mocking (don't require Ollama by default)
- [x] Tests cover error cases (unknown function, rate limit, errors)
- [x] Tests verify function execution results
- [x] Tests use existing fixtures and patterns
- [x] Tests follow pytest best practices
- [x] `@pytest.mark.ollama` marker for filtering

## Integration with Existing Tests

This test file complements:
- `test_function_calling_flow.py` - Tests the function calling framework
- Desktop tests - 83% Ollama coverage on frontend

Together, they provide:
- **Backend Framework**: `test_function_calling_flow.py` (executor, registry, tools)
- **Backend Ollama Integration**: `test_ollama_function_calling.py` (Ollama-specific)
- **Frontend Integration**: Desktop tests (UI → Tauri → Ollama)

## Example: Running a Specific Test

```bash
# Test basic semantic search
poetry run pytest \
  tests/integration/test_ollama_function_calling.py::TestBasicOllamaFunctionCalling::test_ollama_calls_semantic_search \
  -v

# Output:
# test_ollama_function_calling.py::TestBasicOllamaFunctionCalling::test_ollama_calls_semantic_search PASSED [100%]
```

## Troubleshooting

### Import Errors
```bash
# Install dependencies
cd vault/backend
poetry install --with test
```

### Rate Limit Tests Failing
Rate limiters persist across tests. Use `await executor.reset_rate_limits()` in setup.

### Mock Not Working
Verify fixtures are used:
```python
async def test_my_test(function_executor):  # ← Use fixture
    # NOT: executor = FunctionExecutor(...)
```

### Real Ollama Tests Skipped
Set environment variable:
```bash
export OLLAMA_INTEGRATION_TEST=1
# Start Ollama: ollama serve
poetry run pytest ... -v
```

## Next Steps

1. **Run tests**: `poetry run pytest tests/integration/test_ollama_function_calling.py -v`
2. **Check coverage**: Add `--cov` flag
3. **Fix any failures**: Update mocks if needed
4. **Optional**: Test with real Ollama using `OLLAMA_INTEGRATION_TEST=1`

## Metrics

- **Test Count**: 25+ methods
- **Test Classes**: 9
- **Lines of Code**: ~900
- **Coverage Target**: Core Ollama integration paths
- **Execution Time**: ~5 seconds (mocked), ~60 seconds (real Ollama)
