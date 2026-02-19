"""Upload service domain events."""

from __future__ import annotations

from typing import Any

from src.events.types import DomainEvent

__all__ = [
    "FileUploadStarted",
    "FileUploadProgressUpdated",
    "FileUploadCompleted",
    "FileUploadFailed",
    "BatchUploadCompleted",
]


class FileUploadStarted(DomainEvent):
    """Event emitted when a file upload starts.

    Data fields:
        file_id: Unique identifier for the upload
        filename: Original filename
        size_bytes: File size in bytes
        user_id: User uploading the file

    Metadata fields:
        Any additional context (optional)
    """

    def __init__(
        self,
        file_id: str,
        filename: str,
        size_bytes: int,
        user_id: int,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a FileUploadStarted event.

        Args:
            file_id: Unique identifier for the upload
            filename: Original filename
            size_bytes: File size in bytes
            user_id: User uploading the file
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="upload.file.started",
            data={
                "file_id": file_id,
                "filename": filename,
                "size_bytes": size_bytes,
                "user_id": user_id,
            },
            metadata=metadata or {},
            **kwargs,
        )


class FileUploadProgressUpdated(DomainEvent):
    """Event emitted when upload progress changes.

    Data fields:
        file_id: Unique identifier for the upload
        bytes_uploaded: Bytes uploaded so far
        total_bytes: Total file size
        percentage: Upload percentage (0-100)
        user_id: User uploading the file

    Metadata fields:
        Any additional context (optional)
    """

    def __init__(
        self,
        file_id: str,
        bytes_uploaded: int,
        total_bytes: int,
        percentage: float,
        user_id: int,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a FileUploadProgressUpdated event.

        Args:
            file_id: Unique identifier for the upload
            bytes_uploaded: Bytes uploaded so far
            total_bytes: Total file size
            percentage: Upload percentage (0-100)
            user_id: User uploading the file
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="upload.file.progress",
            data={
                "file_id": file_id,
                "bytes_uploaded": bytes_uploaded,
                "total_bytes": total_bytes,
                "percentage": percentage,
                "user_id": user_id,
            },
            metadata=metadata or {},
            **kwargs,
        )


class FileUploadCompleted(DomainEvent):
    """Event emitted when a file upload completes successfully.

    Data fields:
        file_id: Unique identifier for the upload
        filename: Original filename
        size_bytes: File size in bytes
        storage_path: Path where file was stored
        user_id: User who uploaded the file
        duration_seconds: Upload duration in seconds

    Metadata fields:
        Any additional context (optional)
    """

    def __init__(
        self,
        file_id: str,
        filename: str,
        size_bytes: int,
        storage_path: str,
        user_id: int,
        duration_seconds: float,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a FileUploadCompleted event.

        Args:
            file_id: Unique identifier for the upload
            filename: Original filename
            size_bytes: File size in bytes
            storage_path: Path where file was stored
            user_id: User who uploaded the file
            duration_seconds: Upload duration in seconds
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="upload.file.completed",
            data={
                "file_id": file_id,
                "filename": filename,
                "size_bytes": size_bytes,
                "storage_path": storage_path,
                "user_id": user_id,
                "duration_seconds": duration_seconds,
            },
            metadata=metadata or {},
            **kwargs,
        )


class FileUploadFailed(DomainEvent):
    """Event emitted when a file upload fails.

    Data fields:
        file_id: Unique identifier for the upload
        filename: Original filename
        error_message: Error message
        error_type: Type of error (e.g., ValidationError)
        user_id: User who attempted the upload

    Metadata fields:
        stack_trace: Full exception traceback (optional)
        Any additional context (optional)
    """

    def __init__(
        self,
        file_id: str,
        filename: str,
        error_message: str,
        error_type: str,
        user_id: int,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a FileUploadFailed event.

        Args:
            file_id: Unique identifier for the upload
            filename: Original filename
            error_message: Error message
            error_type: Type of error
            user_id: User who attempted the upload
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="upload.file.failed",
            data={
                "file_id": file_id,
                "filename": filename,
                "error_message": error_message,
                "error_type": error_type,
                "user_id": user_id,
            },
            metadata=metadata or {},
            **kwargs,
        )


class BatchUploadCompleted(DomainEvent):
    """Event emitted when a batch of uploads completes.

    Data fields:
        batch_id: Unique identifier for the batch
        total_files: Total files in batch
        successful_uploads: Number of successful uploads
        failed_uploads: Number of failed uploads
        user_id: User who initiated the batch
        duration_seconds: Total batch duration in seconds

    Metadata fields:
        Any additional context (optional)
    """

    def __init__(
        self,
        batch_id: str,
        total_files: int,
        successful_uploads: int,
        failed_uploads: int,
        user_id: int,
        duration_seconds: float,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a BatchUploadCompleted event.

        Args:
            batch_id: Unique identifier for the batch
            total_files: Total files in batch
            successful_uploads: Number of successful uploads
            failed_uploads: Number of failed uploads
            user_id: User who initiated the batch
            duration_seconds: Total batch duration in seconds
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="upload.batch.completed",
            data={
                "batch_id": batch_id,
                "total_files": total_files,
                "successful_uploads": successful_uploads,
                "failed_uploads": failed_uploads,
                "user_id": user_id,
                "duration_seconds": duration_seconds,
            },
            metadata=metadata or {},
            **kwargs,
        )
