"""External integrations for syncing data from third-party services.

This module provides a unified interface for integrating with external services.
"""

from .base import (
    AuthenticationError,
    BaseIntegration,
    IntegrationConfig,
    IntegrationData,
    IntegrationStatus,
    RateLimitError,
    SyncError,
)

__all__ = [
    "AuthenticationError",
    "BaseIntegration",
    "IntegrationConfig",
    "IntegrationData",
    "IntegrationStatus",
    "RateLimitError",
    "SyncError",
]
