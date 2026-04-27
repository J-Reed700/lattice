"""Watch service domain events."""

from __future__ import annotations

from typing import Any

from src.events.types import DomainEvent

__all__ = [
    "DirectoryWatchStarted",
    "DirectoryWatchStopped",
    "FileChangeDetected",
    "WatchErrorOccurred",
]


class DirectoryWatchStarted(DomainEvent):
    """Event emitted when a directory watch starts.

    Data fields:
        directory_path: Absolute path to watched directory
        user_id: User who started the watch
        recursive: Whether subdirectories are watched

    Metadata fields:
        Any additional context (optional)
    """

    def __init__(
        self,
        directory_path: str,
        user_id: int,
        recursive: bool,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a DirectoryWatchStarted event.

        Args:
            directory_path: Absolute path to watched directory
            user_id: User who started the watch
            recursive: Whether subdirectories are watched
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="watch.directory.started",
            data={
                "directory_path": directory_path,
                "user_id": user_id,
                "recursive": recursive,
            },
            metadata=metadata or {},
            **kwargs,
        )


class DirectoryWatchStopped(DomainEvent):
    """Event emitted when a directory watch stops.

    Data fields:
        directory_path: Absolute path to watched directory
        user_id: User who owned the watch
        reason: Reason for stopping (user_request, error, shutdown)

    Metadata fields:
        Any additional context (optional)
    """

    def __init__(
        self,
        directory_path: str,
        user_id: int,
        reason: str,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a DirectoryWatchStopped event.

        Args:
            directory_path: Absolute path to watched directory
            user_id: User who owned the watch
            reason: Reason for stopping
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="watch.directory.stopped",
            data={
                "directory_path": directory_path,
                "user_id": user_id,
                "reason": reason,
            },
            metadata=metadata or {},
            **kwargs,
        )


class FileChangeDetected(DomainEvent):
    """Event emitted when a file change is detected.

    Data fields:
        file_path: Absolute path to changed file
        change_type: Type of change (created, modified, deleted)
        directory_path: Parent watched directory path
        user_id: User who owns the watch

    Metadata fields:
        timestamp: When change was detected (auto-added by base)
        Any additional context (optional)
    """

    def __init__(
        self,
        file_path: str,
        change_type: str,
        directory_path: str,
        user_id: int,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a FileChangeDetected event.

        Args:
            file_path: Absolute path to changed file
            change_type: Type of change (created, modified, deleted)
            directory_path: Parent watched directory path
            user_id: User who owns the watch
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="watch.file.changed",
            data={
                "file_path": file_path,
                "change_type": change_type,
                "directory_path": directory_path,
                "user_id": user_id,
            },
            metadata=metadata or {},
            **kwargs,
        )


class WatchErrorOccurred(DomainEvent):
    """Event emitted when a watch operation encounters an error.

    Data fields:
        directory_path: Directory path where error occurred
        error_message: Error message
        error_type: Type of error (e.g., PermissionError)
        user_id: User who owns the watch

    Metadata fields:
        stack_trace: Full exception traceback (optional)
        Any additional context (optional)
    """

    def __init__(
        self,
        directory_path: str,
        error_message: str,
        error_type: str,
        user_id: int,
        metadata: dict[str, Any] | None = None,
        **kwargs: Any,
    ) -> None:
        """Create a WatchErrorOccurred event.

        Args:
            directory_path: Directory path where error occurred
            error_message: Error message
            error_type: Type of error
            user_id: User who owns the watch
            metadata: Optional metadata
            **kwargs: Additional DomainEvent fields
        """
        super().__init__(
            event_type="watch.error",
            data={
                "directory_path": directory_path,
                "error_message": error_message,
                "error_type": error_type,
                "user_id": user_id,
            },
            metadata=metadata or {},
            **kwargs,
        )
