"""Pydantic schemas for request/response validation."""

from .agent import (
    IntegrationStatus,
    OllamaModelInfo,
    QueryRequest,
    QueryResponse,
    StatusResponse,
    SyncRequest,
    SyncResponse,
)
from .agent import (
    SourceDocument as AgentSourceDocument,
)
from .agentic_rag import (
    AgenticComparisonRequest,
    AgenticComparisonResponse,
    AgenticQueryRequest,
    AgenticQueryResponse,
    AgenticStreamEvent,
    ComparisonResult,
    ReasoningStepResponse,
    SourceDocumentResponse,
)
from .common import (
    ErrorResponse,
    PaginatedResponse,
    PaginationParams,
    SuccessResponse,
)
from .documents import (
    DocumentAccessResponse,
    DocumentBase,
    FavoriteRequest,
    FavoriteResponse,
    FavoritesResponse,
    RecentDocumentsResponse,
)
from .file import (
    FileListParams,
    FileListResponse,
    FileMetadata,
    FileMetadataUpdate,
    FileUploadResponse,
)
from .health import (
    ComponentStatus,
    HealthCheck,
    IndexingQueueStatus,
    StorageStatistics,
    SystemStatus,
)
from .llm import (
    AskRequest,
    AskResponse,
    HealthCheckResponse,
    ListModelsResponse,
)
from .llm import (
    SourceDocument as LLMSourceDocument,
)
from .mfa import (
    MFASetupResponse,
    MFAStatusResponse,
    MFAVerifyRequest,
    TokenResponseWithMFA,
)
from .search import (
    AutocompleteRequest,
    AutocompleteResponse,
    SearchRequest,
    SearchResponse,
    SearchResultItem,
)
from .watch import (
    ReindexRequest,
    WatchDirectory,
    WatchDirectoryCreate,
    WatchDirectoryList,
    WatchStatus,
)

__all__ = [
    # Agent schemas
    "QueryRequest",
    "AgentSourceDocument",
    "QueryResponse",
    "OllamaModelInfo",
    "IntegrationStatus",
    "StatusResponse",
    "SyncRequest",
    "SyncResponse",
    # Agentic RAG schemas
    "AgenticQueryRequest",
    "ReasoningStepResponse",
    "SourceDocumentResponse",
    "AgenticQueryResponse",
    "AgenticStreamEvent",
    "AgenticComparisonRequest",
    "ComparisonResult",
    "AgenticComparisonResponse",
    # Common schemas
    "ErrorResponse",
    "PaginationParams",
    "PaginatedResponse",
    "SuccessResponse",
    # Document schemas
    "DocumentBase",
    "DocumentAccessResponse",
    "FavoriteResponse",
    "RecentDocumentsResponse",
    "FavoritesResponse",
    "FavoriteRequest",
    # File schemas
    "FileUploadResponse",
    "FileMetadataUpdate",
    "FileMetadata",
    "FileListResponse",
    "FileListParams",
    # Health schemas
    "HealthCheck",
    "ComponentStatus",
    "SystemStatus",
    "IndexingQueueStatus",
    "StorageStatistics",
    # LLM schemas
    "AskRequest",
    "AskResponse",
    "HealthCheckResponse",
    "ListModelsResponse",
    "LLMSourceDocument",
    # MFA schemas
    "MFASetupResponse",
    "MFAVerifyRequest",
    "MFAStatusResponse",
    "TokenResponseWithMFA",
    # Search schemas
    "SearchRequest",
    "SearchResultItem",
    "SearchResponse",
    "AutocompleteRequest",
    "AutocompleteResponse",
    # Watch schemas
    "WatchDirectoryCreate",
    "WatchDirectory",
    "WatchDirectoryList",
    "WatchStatus",
    "ReindexRequest",
]
