"""Base classes for external integrations.

This module provides abstract base classes for building integration connectors
to external services (Gmail, Slack, etc.).
"""

from abc import ABC, abstractmethod
from datetime import datetime
from enum import Enum
from typing import Any

from pydantic import BaseModel, Field
import structlog

logger = structlog.get_logger(__name__)


class IntegrationStatus(str, Enum):
    """Integration connection status."""

    CONNECTED = "connected"
    DISCONNECTED = "disconnected"
    ERROR = "error"
    AUTHENTICATING = "authenticating"


class IntegrationConfig(BaseModel):
    """Base configuration for integrations.

    Attributes:
        enabled: Whether integration is enabled
        user_id: User ID this integration belongs to
        credentials: Encrypted credentials storage path
        last_sync: Timestamp of last successful sync
        status: Current connection status
    """

    enabled: bool = True
    user_id: str
    credentials_path: str | None = None
    last_sync: datetime | None = None
    status: IntegrationStatus = IntegrationStatus.DISCONNECTED


class IntegrationData(BaseModel):
    """Standardized data format returned by integrations.

    Attributes:
        id: Unique identifier from source system
        content: Main content/body text
        metadata: Additional context and attributes
        timestamp: When the item was created/modified
        source: Which integration this came from
    """

    id: str = Field(..., description="Unique identifier from source")
    content: str = Field(..., description="Main text content")
    metadata: dict[str, Any] = Field(default_factory=dict, description="Additional context")
    timestamp: datetime = Field(..., description="Creation/modification time")
    source: str = Field(..., description="Integration source name")

    class Config:
        json_schema_extra = {
            "example": {
                "id": "msg_12345",
                "content": "Email body text",
                "metadata": {
                    "subject": "Meeting notes",
                    "sender": "user@example.com",
                    "labels": ["important", "work"],
                },
                "timestamp": "2024-01-15T10:30:00Z",
                "source": "gmail",
            }
        }


class AuthenticationError(Exception):
    """Raised when authentication fails."""


class RateLimitError(Exception):
    """Raised when rate limit is exceeded."""

    def __init__(self, message: str, retry_after: int | None = None):
        super().__init__(message)
        self.retry_after = retry_after


class SyncError(Exception):
    """Raised when data synchronization fails."""


class BaseIntegration(ABC):
    """Abstract base class for all integrations.

    Integrations are self-contained modules that connect to external services,
    authenticate users, and sync data into standardized format.

    Example:
        >>> class MyIntegration(BaseIntegration):
        ...     async def authenticate(self):
        ...         # OAuth flow
        ...         pass
        ...
        ...     async def sync_data(self, since=None):
        ...         # Fetch and transform data
        ...         return [IntegrationData(...)]
    """

    def __init__(self, config: IntegrationConfig):
        """Initialize integration with configuration.

        Args:
            config: Integration-specific configuration
        """
        self.config = config
        self.logger = logger.bind(integration=self.__class__.__name__, user_id=config.user_id)

    @property
    @abstractmethod
    def name(self) -> str:
        """Integration name identifier."""

    @abstractmethod
    async def authenticate(self) -> bool:
        """Authenticate with the external service.

        Returns:
            True if authentication successful

        Raises:
            AuthenticationError: If authentication fails
        """

    @abstractmethod
    async def sync_data(
        self, since: datetime | None = None, limit: int | None = None
    ) -> list[IntegrationData]:
        """Sync data from external service.

        Args:
            since: Only fetch items modified after this timestamp
            limit: Maximum number of items to fetch

        Returns:
            List of standardized integration data

        Raises:
            RateLimitError: If rate limit exceeded
            SyncError: If sync fails
        """

    @abstractmethod
    async def search(self, query: str, limit: int | None = None) -> list[IntegrationData]:
        """Search for items in the external service.

        Args:
            query: Search query string
            limit: Maximum results to return

        Returns:
            List of matching items

        Raises:
            RateLimitError: If rate limit exceeded
            SyncError: If search fails
        """

    async def disconnect(self) -> None:
        """Disconnect and cleanup resources.

        Default implementation updates status. Override for custom cleanup.
        """
        self.config.status = IntegrationStatus.DISCONNECTED
        self.logger.info("integration_disconnected")

    async def get_status(self) -> IntegrationStatus:
        """Get current integration status.

        Returns:
            Current connection status
        """
        return self.config.status

    async def healthcheck(self) -> bool:
        """Check if integration is healthy and connected.

        Returns:
            True if healthy and connected
        """
        try:
            return self.config.status == IntegrationStatus.CONNECTED
        except Exception as e:
            self.logger.error("healthcheck_failed", error=str(e))
            return False
