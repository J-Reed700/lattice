"""
Storage Manager Exceptions

Custom exception hierarchy for storage management operations.
"""


class StorageError(Exception):
    """Base exception for all storage manager errors."""


class StorageAnalysisError(StorageError):
    """Raised when storage analysis fails.

    Example:
        >>> raise StorageAnalysisError("Cannot access storage directory")
    """


class CleanupError(StorageError):
    """Raised when cleanup operation fails.

    Example:
        >>> raise CleanupError("Permission denied while deleting file")
    """


class QuotaExceededError(StorageError):
    """Raised when storage quota is exceeded.

    Attributes:
        current_bytes: Current storage usage in bytes
        quota_bytes: Maximum allowed storage in bytes
        overage_bytes: Amount over quota in bytes

    Example:
        >>> raise QuotaExceededError(
        ...     "Storage quota exceeded",
        ...     current_bytes=60*1024**3,
        ...     quota_bytes=50*1024**3
        ... )
    """

    def __init__(
        self,
        message: str,
        current_bytes: int | None = None,
        quota_bytes: int | None = None,
    ) -> None:
        super().__init__(message)
        self.current_bytes = current_bytes
        self.quota_bytes = quota_bytes
        self.overage_bytes = current_bytes - quota_bytes if current_bytes and quota_bytes else None


class PermissionDeniedError(StorageError):
    """Raised when file/directory access is denied.

    Example:
        >>> raise PermissionDeniedError("Cannot read /protected/file.txt")
    """


class InvalidPathError(StorageError):
    """Raised when a path is invalid or doesn't exist.

    Example:
        >>> raise InvalidPathError("Path does not exist: /nonexistent")
    """
