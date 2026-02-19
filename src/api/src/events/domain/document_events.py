"""Document lifecycle domain events."""

from __future__ import annotations

from typing import Any

from src.events.types import DomainEvent

__all__ = [
    "DocumentIndexedEvent",
    "DocumentIndexingFailedEvent",
    "DocumentDeletedEvent",
]


class DocumentIndexedEvent(DomainEvent):
    """Event emitted when a document is successfully indexed.

    This event is published after a document has been:
    - Text extracted
    - Split into chunks
    - Embeddings generated
    - Stored in the database

    Data fields:
        document_id: ID of the indexed document
        file_path: Path to the source file
        chunk_count: Number of chunks created
        embedding_model: Model used for embeddings

    Metadata fields:
        user_id: User who triggered indexing (optional)
        indexing_duration_ms: Time taken to index (optional)
    """

    def __init__(
        self,
        document_id: int,
        file_path: str,
        chunk_count: int,
        embedding_model: str,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a DocumentIndexedEvent.

        Args:
            document_id: ID of the indexed document
            file_path: Path to the source file
            chunk_count: Number of chunks created
            embedding_model: Model used for embeddings
            metadata: Optional metadata (user_id, duration, etc.)
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="document.indexed",
            data={
                "document_id": document_id,
                "file_path": file_path,
                "chunk_count": chunk_count,
                "embedding_model": embedding_model,
            },
            metadata=metadata or {},
            **kwargs,
        )


class DocumentIndexingFailedEvent(DomainEvent):
    """Event emitted when document indexing fails.

    This event is published when an error occurs during:
    - Text extraction
    - Chunking
    - Embedding generation
    - Database storage

    Data fields:
        file_path: Path to the source file
        error_type: Type of error (e.g., "ExtractionError")
        error_message: Human-readable error description

    Metadata fields:
        user_id: User who triggered indexing (optional)
        stack_trace: Full exception traceback (optional)
    """

    def __init__(
        self,
        file_path: str,
        error_type: str,
        error_message: str,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a DocumentIndexingFailedEvent.

        Args:
            file_path: Path to the source file
            error_type: Type of error
            error_message: Error description
            metadata: Optional metadata (user_id, stack_trace, etc.)
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="document.indexing_failed",
            data={
                "file_path": file_path,
                "error_type": error_type,
                "error_message": error_message,
            },
            metadata=metadata or {},
            **kwargs,
        )


class DocumentDeletedEvent(DomainEvent):
    """Event emitted when a document is deleted.

    This event is published after a document and its chunks
    have been removed from the database.

    Data fields:
        document_id: ID of the deleted document
        file_path: Path to the source file
        chunk_count: Number of chunks deleted

    Metadata fields:
        user_id: User who triggered deletion (optional)
        reason: Reason for deletion (optional)
    """

    def __init__(
        self,
        document_id: int,
        file_path: str,
        chunk_count: int,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a DocumentDeletedEvent.

        Args:
            document_id: ID of the deleted document
            file_path: Path to the source file
            chunk_count: Number of chunks deleted
            metadata: Optional metadata (user_id, reason, etc.)
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="document.deleted",
            data={
                "document_id": document_id,
                "file_path": file_path,
                "chunk_count": chunk_count,
            },
            metadata=metadata or {},
            **kwargs,
        )
