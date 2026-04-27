from collections.abc import Callable
from datetime import UTC, datetime
from typing import Any

from pydantic import BaseModel, Field


class AgentConfig(BaseModel):
    model: str = Field(default="llama3.2", description="LLM model name")
    temperature: float = Field(default=0.7, ge=0.0, le=2.0, description="Sampling temperature")
    max_iterations: int = Field(default=10, ge=1, description="Maximum agent iterations")
    max_execution_time: float = Field(
        default=120.0, ge=1.0, description="Maximum execution time in seconds"
    )
    verbose: bool = Field(default=False, description="Enable verbose logging")
    enable_tracing: bool = Field(default=False, description="Enable LangChain tracing")
    system_message: str | None = Field(default=None, description="Custom system message for agent")

    class Config:
        json_schema_extra = {
            "example": {
                "model": "llama3.2",
                "temperature": 0.7,
                "max_iterations": 10,
                "max_execution_time": 120.0,
                "verbose": False,
                "enable_tracing": False,
                "system_message": "You are a helpful assistant.",
            }
        }


class AgentResponse(BaseModel):
    output: str = Field(..., description="Agent's final output")
    intermediate_steps: list[dict[str, Any]] = Field(
        default_factory=list, description="Intermediate reasoning steps"
    )
    total_tokens: int | None = Field(default=None, description="Total tokens used")
    execution_time: float = Field(..., description="Execution time in seconds")
    success: bool = Field(default=True, description="Whether execution was successful")
    error: str | None = Field(default=None, description="Error message if failed")
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional metadata")

    class Config:
        json_schema_extra = {
            "example": {
                "output": "Task completed successfully.",
                "intermediate_steps": [{"action": "search", "result": "Found 5 results"}],
                "total_tokens": 150,
                "execution_time": 2.5,
                "success": True,
                "error": None,
                "metadata": {"model": "llama3.2"},
            }
        }


class AgentStreamChunk(BaseModel):
    chunk_type: str = Field(..., description="Type of chunk: token, action, thought, final")
    content: str = Field(..., description="Chunk content")
    timestamp: datetime = Field(
        default_factory=lambda: datetime.now(UTC), description="Chunk timestamp"
    )
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional chunk metadata")

    class Config:
        json_schema_extra = {
            "example": {
                "chunk_type": "token",
                "content": "I am processing your request...",
                "timestamp": "2024-01-01T00:00:00Z",
                "metadata": {"token_count": 5},
            }
        }


class ToolDefinition(BaseModel):
    name: str = Field(..., description="Unique tool name")
    description: str = Field(..., description="Tool description for agent")
    parameters: dict[str, Any] = Field(default_factory=dict, description="Tool parameter schema")
    func: Callable[..., Any] | None = Field(
        default=None, description="Tool implementation function", exclude=True
    )
    async_func: Callable[..., Any] | None = Field(
        default=None, description="Async tool implementation", exclude=True
    )
    return_direct: bool = Field(default=False, description="Return tool result directly to user")

    class Config:
        arbitrary_types_allowed = True
        json_schema_extra = {
            "example": {
                "name": "search_documents",
                "description": "Search through indexed documents",
                "parameters": {"query": {"type": "string", "description": "Search query"}},
                "return_direct": False,
            }
        }


class ToolRegistrationResult(BaseModel):
    tool_name: str = Field(..., description="Name of registered tool")
    success: bool = Field(..., description="Whether registration was successful")
    error: str | None = Field(default=None, description="Error message if failed")

    class Config:
        json_schema_extra = {
            "example": {
                "tool_name": "search_documents",
                "success": True,
                "error": None,
            }
        }
