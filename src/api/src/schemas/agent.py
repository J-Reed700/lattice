from __future__ import annotations

from datetime import datetime

from pydantic import BaseModel, Field


class QueryRequest(BaseModel):
    query: str = Field(
        ..., min_length=1, max_length=2000, description="Natural language query for the RAG agent"
    )
    context_limit: int = Field(
        default=5, ge=1, le=20, description="Maximum number of context documents to retrieve"
    )
    use_local_llm: bool = Field(
        default=True, description="Whether to use local Ollama LLM (true) or external API (false)"
    )
    streaming: bool = Field(default=False, description="Enable streaming response for long queries")

    class Config:
        json_schema_extra = {
            "example": {
                "query": "What are the main findings in the Q4 sales report?",
                "context_limit": 5,
                "use_local_llm": True,
                "streaming": False,
            }
        }


class SourceDocument(BaseModel):
    file_path: str = Field(..., description="Path to source document")
    filename: str = Field(..., description="Name of source file")
    snippet: str = Field(..., description="Relevant text excerpt")
    score: float = Field(..., ge=0.0, le=1.0, description="Relevance score")
    page_number: int | None = Field(None, description="Page number if applicable")

    class Config:
        json_schema_extra = {
            "example": {
                "file_path": "/documents/reports/Q4_sales.pdf",
                "filename": "Q4_sales.pdf",
                "snippet": "Total revenue increased by 23% year-over-year...",
                "score": 0.89,
                "page_number": 3,
            }
        }


class QueryResponse(BaseModel):
    answer: str = Field(..., description="Generated answer from the agent")
    sources: list[SourceDocument] = Field(
        default_factory=list, description="Source documents used to generate the answer"
    )
    confidence: float = Field(
        ..., ge=0.0, le=1.0, description="Confidence score of the answer (0-1)"
    )
    query_time_ms: int = Field(..., description="Total query execution time in milliseconds")
    model_used: str | None = Field(None, description="LLM model name used for generation")
    token_count: int | None = Field(None, description="Total tokens used in generation")

    class Config:
        json_schema_extra = {
            "example": {
                "answer": "The Q4 sales report shows a 23% increase in revenue year-over-year, driven primarily by increased enterprise customer adoption.",
                "sources": [
                    {
                        "file_path": "/documents/reports/Q4_sales.pdf",
                        "filename": "Q4_sales.pdf",
                        "snippet": "Total revenue increased by 23% year-over-year...",
                        "score": 0.89,
                        "page_number": 3,
                    }
                ],
                "confidence": 0.85,
                "query_time_ms": 1234,
                "model_used": "llama3.2",
                "token_count": 450,
            }
        }


class OllamaModelInfo(BaseModel):
    name: str = Field(..., description="Model name")
    size: str | None = Field(None, description="Model size")
    modified_at: datetime | None = Field(None, description="Last modification date")

    class Config:
        json_schema_extra = {
            "example": {"name": "llama3.2", "size": "7B", "modified_at": "2024-01-15T10:30:00Z"}
        }


class IntegrationStatus(BaseModel):
    enabled: bool = Field(..., description="Whether integration is enabled")
    connected: bool = Field(..., description="Whether integration is currently connected")
    last_sync: datetime | None = Field(None, description="Last successful sync timestamp")
    document_count: int | None = Field(None, description="Number of documents synced")
    error: str | None = Field(None, description="Last error message if any")

    class Config:
        json_schema_extra = {
            "example": {
                "enabled": True,
                "connected": True,
                "last_sync": "2024-01-15T10:30:00Z",
                "document_count": 1234,
                "error": None,
            }
        }


class StatusResponse(BaseModel):
    ollama_available: bool = Field(..., description="Whether Ollama service is available")
    ollama_models: list[str] = Field(
        default_factory=list, description="List of available Ollama models"
    )
    integrations: dict[str, IntegrationStatus] = Field(
        default_factory=dict, description="Status of each integration (e.g., google_drive, dropbox)"
    )
    agent_initialized: bool = Field(
        default=False, description="Whether the LangChain agent is initialized"
    )
    default_model: str | None = Field(None, description="Default LLM model being used")

    class Config:
        json_schema_extra = {
            "example": {
                "ollama_available": True,
                "ollama_models": ["llama3.2", "mistral", "llama2"],
                "integrations": {
                    "google_drive": {
                        "enabled": True,
                        "connected": True,
                        "last_sync": "2024-01-15T10:30:00Z",
                        "document_count": 856,
                        "error": None,
                    },
                    "dropbox": {
                        "enabled": False,
                        "connected": False,
                        "last_sync": None,
                        "document_count": 0,
                        "error": None,
                    },
                },
                "agent_initialized": True,
                "default_model": "llama3.2",
            }
        }


class SyncRequest(BaseModel):
    integration_name: str = Field(
        ...,
        min_length=1,
        description="Name of the integration to sync (e.g., 'google_drive', 'dropbox')",
    )
    force: bool = Field(default=False, description="Force sync even if recently synced")

    class Config:
        json_schema_extra = {"example": {"integration_name": "google_drive", "force": False}}


class SyncResponse(BaseModel):
    integration_name: str = Field(..., description="Name of the integration that was synced")
    documents_synced: int = Field(..., description="Number of documents synced")
    sync_time_ms: int = Field(..., description="Sync duration in milliseconds")
    last_sync: datetime = Field(..., description="Timestamp of this sync")
    status: str = Field(..., description="Sync status (success, partial, failed)")
    errors: list[str] = Field(
        default_factory=list, description="List of errors encountered during sync"
    )

    class Config:
        json_schema_extra = {
            "example": {
                "integration_name": "google_drive",
                "documents_synced": 42,
                "sync_time_ms": 5678,
                "last_sync": "2024-01-15T10:35:00Z",
                "status": "success",
                "errors": [],
            }
        }
