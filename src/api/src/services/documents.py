"""Document service for recent access and favorites tracking.

WARNING: This service is currently DISABLED due to missing database models.
The following models need to be created before this service can be used:
- Document
- DocumentAccess
- Favorites
- Views: v_recent_documents, v_favorite_documents

TODO: Create proper SQLAlchemy ORM models and re-enable this service.
"""

import logging
from typing import Any

from sqlalchemy.ext.asyncio import AsyncSession

logger = logging.getLogger(__name__)


class DocumentService:
    """Service for managing document access and favorites (CURRENTLY DISABLED)."""

    def __init__(self, db: AsyncSession):
        self.db = db
        logger.warning(
            "DocumentService instantiated but is non-functional: "
            "Missing database models (Document, DocumentAccess, Favorites)"
        )

    async def record_access(self, document_id: str, user_id: str = "default") -> dict[str, Any]:
        """Record document access (DISABLED - missing models)."""
        raise NotImplementedError(
            "DocumentService.record_access requires Document and DocumentAccess models"
        )

    async def add_favorite(
        self, document_id: str, user_id: str = "default", note: str | None = None
    ) -> dict[str, Any]:
        """Add document to favorites (DISABLED - missing models).

        If document is already favorited, updates the favorite metadata.
        """
        raise NotImplementedError("DocumentService.add_favorite requires Favorites model")

    async def get_recent_documents(
        self, user_id: str = "default", limit: int = 50
    ) -> list[dict[str, Any]]:
        """Get recently accessed documents (DISABLED - missing models)."""
        raise NotImplementedError(
            "DocumentService.get_recent_documents requires v_recent_documents view"
        )

    async def get_favorites(
        self, user_id: str = "default", limit: int = 100
    ) -> list[dict[str, Any]]:
        """Get favorite documents (DISABLED - missing models)."""
        raise NotImplementedError(
            "DocumentService.get_favorites requires v_favorite_documents view"
        )

    async def remove_favorite(self, document_id: str, user_id: str = "default"):
        """Remove a favorite (DISABLED - missing models)."""
        raise NotImplementedError("DocumentService.remove_favorite requires Favorites model")
