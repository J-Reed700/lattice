from typing import Any

from pydantic import BaseModel, Field


class OllamaGenerateRequest(BaseModel):
    model: str = Field(..., description="Model name to use for generation")
    prompt: str = Field(..., description="Prompt text to generate from")
    system: str | None = Field(None, description="System prompt to set context")
    template: str | None = Field(None, description="Prompt template override")
    context: list[int] | None = Field(None, description="Context from previous request")
    stream: bool = Field(default=False, description="Enable streaming response")
    raw: bool = Field(default=False, description="Disable prompt formatting")
    format: str | None = Field(None, description="Response format (json)")
    options: dict[str, Any] | None = Field(None, description="Model-specific options")
    keep_alive: str | None = Field(None, description="Duration to keep model loaded")


class OllamaGenerateResponse(BaseModel):
    model: str = Field(..., description="Model used for generation")
    created_at: str = Field(..., description="Timestamp of generation")
    response: str = Field(..., description="Generated text response")
    done: bool = Field(..., description="Whether generation is complete")
    context: list[int] | None = Field(None, description="Context for continuation")
    total_duration: int | None = Field(None, description="Total duration in nanoseconds")
    load_duration: int | None = Field(None, description="Model load duration in nanoseconds")
    prompt_eval_count: int | None = Field(None, description="Number of tokens in prompt")
    prompt_eval_duration: int | None = Field(None, description="Prompt evaluation duration")
    eval_count: int | None = Field(None, description="Number of tokens generated")
    eval_duration: int | None = Field(None, description="Generation duration in nanoseconds")


class OllamaStreamResponse(BaseModel):
    model: str = Field(..., description="Model used for generation")
    created_at: str = Field(..., description="Timestamp of chunk")
    response: str = Field(default="", description="Generated text chunk")
    done: bool = Field(..., description="Whether generation is complete")


class OllamaModel(BaseModel):
    name: str = Field(..., description="Model name")
    modified_at: str = Field(..., description="Last modification timestamp")
    size: int = Field(..., description="Model size in bytes")
    digest: str = Field(..., description="Model digest hash")
    details: dict[str, Any] | None = Field(None, description="Additional model details")


class OllamaListResponse(BaseModel):
    models: list[OllamaModel] = Field(..., description="Available models")


class OllamaError(BaseModel):
    error: str = Field(..., description="Error message from Ollama")
