"""VectorStore implementation for Vault.

This module provides the VectorStore class for managing text and image embeddings
using PostgreSQL with pgvector extension for efficient similarity search.
"""

import logging
from typing import Any
from uuid import UUID

from sqlalchemy import delete, func, select
from sqlalchemy.dialects.postgresql import insert
from sqlalchemy.ext.asyncio import AsyncSession

from ...models.database import ImageEmbedding, TextEmbedding
from ..caching.vector_cache import VectorSearchCache
from .queries import (
    build_get_embedding_stats,
    build_search_image_similar,
    build_search_image_similar_with_metadata,
    build_search_text_similar,
    build_search_text_similar_with_metadata,
    build_search_text_similar_with_preview,
)
from .types import EmbeddingStats, ImageSearchResult, TextSearchResult

logger = logging.getLogger(__name__)

DEFAULT_SIMILARITY_THRESHOLD = 0.7
DEFAULT_SEARCH_LIMIT = 10
DEFAULT_PREVIEW_LENGTH = 500
BATCH_SIZE = 100
TEXT_EMBEDDING_DIMENSION = 768
IMAGE_EMBEDDING_DIMENSION = 512
OLD_TEXT_EMBEDDING_DIMENSION = 384


class VectorStore:
    """Vector storage and similarity search for text and image embeddings.

    This class provides methods to store and search vector embeddings using
    PostgreSQL with pgvector extension. It handles both text embeddings
    (dunzhang/stella_en_1.5B_v5, 768 dimensions via MRL truncation) and image embeddings
    (CLIP ViT-B/32, 512 dimensions).

    All operations are async and designed to work with SQLAlchemy 2.0.
    """

    _text_cache = None
    _image_cache = None

    def __init__(self, session: AsyncSession, enable_cache: bool = True):
        """Initialize VectorStore with database session.

        Args:
            session: SQLAlchemy AsyncSession for database operations
            enable_cache: Enable caching for vector search results
        """
        self.session = session
        self._enable_cache = enable_cache

        if enable_cache:
            if VectorStore._text_cache is None:
                VectorStore._text_cache = VectorSearchCache(ttl_seconds=300, max_size=500)
            if VectorStore._image_cache is None:
                VectorStore._image_cache = VectorSearchCache(ttl_seconds=300, max_size=500)

            self._text_cache = VectorStore._text_cache
            self._image_cache = VectorStore._image_cache
        else:
            self._text_cache = None
            self._image_cache = None

    async def store_text_embedding(
        self,
        text_content_id: UUID,
        embedding: list[float],
        model_name: str,
        model_version: str,
    ) -> UUID:
        """Store a text embedding vector.

        Args:
            text_content_id: UUID of the text content
            embedding: Embedding vector (must be 768 dimensions)
            model_name: Name of the embedding model
            model_version: Version of the embedding model

        Returns:
            UUID of the created embedding record

        Raises:
            ValueError: If embedding dimension is incorrect
            SQLAlchemyError: If database operation fails
        """
        if len(embedding) != TEXT_EMBEDDING_DIMENSION:
            raise ValueError(
                f"Text embedding must be {TEXT_EMBEDDING_DIMENSION} dimensions, "
                f"got {len(embedding)}"
            )

        stmt = (
            insert(TextEmbedding)
            .values(
                text_content_id=text_content_id,
                embedding=embedding,
                model_name=model_name,
                model_version=model_version,
            )
            .on_conflict_do_update(
                index_elements=["text_content_id"],
                set_={
                    "embedding": embedding,
                    "model_name": model_name,
                    "model_version": model_version,
                    "electric_last_modified": func.now(),
                },
            )
            .returning(TextEmbedding.id)
        )

        result = await self.session.execute(stmt)
        embedding_id = result.scalar_one()

        logger.debug(f"Stored text embedding {embedding_id} for content {text_content_id}")
        return embedding_id

    async def store_text_embeddings_batch(
        self,
        embeddings: list[dict[str, Any]],
    ) -> list[UUID]:
        """Store multiple text embeddings in a batch.

        Args:
            embeddings: List of dicts with keys:
                - text_content_id: UUID
                - embedding: List[float]
                - model_name: str
                - model_version: str

        Returns:
            List of UUIDs for created embedding records

        Raises:
            ValueError: If any embedding has incorrect dimension
            SQLAlchemyError: If database operation fails
        """
        for emb in embeddings:
            if len(emb["embedding"]) != TEXT_EMBEDDING_DIMENSION:
                raise ValueError(
                    f"Text embedding must be {TEXT_EMBEDDING_DIMENSION} dimensions, "
                    f"got {len(emb['embedding'])}"
                )

        stmt = (
            insert(TextEmbedding)
            .values(embeddings)
            .on_conflict_do_update(
                index_elements=["text_content_id"],
                set_={
                    "embedding": insert(TextEmbedding).excluded.embedding,
                    "model_name": insert(TextEmbedding).excluded.model_name,
                    "model_version": insert(TextEmbedding).excluded.model_version,
                    "electric_last_modified": func.now(),
                },
            )
            .returning(TextEmbedding.id)
        )

        result = await self.session.execute(stmt)
        embedding_ids = [row[0] for row in result.fetchall()]

        logger.info(f"Stored {len(embedding_ids)} text embeddings in batch")
        return embedding_ids

    async def search_text_similar(
        self,
        query_embedding: list[float],
        limit: int = DEFAULT_SEARCH_LIMIT,
        threshold: float = DEFAULT_SIMILARITY_THRESHOLD,
        include_metadata: bool = True,
        preview_length: int | None = None,
    ) -> list[TextSearchResult]:
        """Search for similar text embeddings.

        Args:
            query_embedding: Query vector (must be 768 dimensions)
            limit: Maximum number of results to return
            threshold: Minimum similarity threshold (0-1)
            include_metadata: Include file and content metadata
            preview_length: If set, truncate content to this length

        Returns:
            List of TextSearchResult ordered by similarity (highest first)

        Raises:
            ValueError: If query_embedding dimension is incorrect
            SQLAlchemyError: If database operation fails
        """
        if len(query_embedding) != TEXT_EMBEDDING_DIMENSION:
            raise ValueError(
                f"Query embedding must be {TEXT_EMBEDDING_DIMENSION} dimensions, "
                f"got {len(query_embedding)}"
            )

        if self._text_cache:
            cached_results = self._text_cache.get(
                query_embedding, limit, threshold, include_metadata, preview_length
            )
            if cached_results is not None:
                logger.debug(f"Cache hit for text search (limit={limit}, threshold={threshold})")
                return cached_results

        if not include_metadata:
            pass
        elif preview_length is not None:
            pass
        else:
            pass

        params = {
            "query_embedding": query_embedding,
            "threshold": threshold,
            "limit": limit,
        }

        if preview_length is not None:
            params["preview_length"] = preview_length

        if not include_metadata:
            stmt = build_search_text_similar(query_embedding, threshold, limit)
        elif preview_length is not None:
            stmt = build_search_text_similar_with_preview(
                query_embedding, threshold, limit, preview_length
            )
        else:
            stmt = build_search_text_similar_with_metadata(query_embedding, threshold, limit)

        result = await self.session.execute(stmt)
        rows = result.fetchall()

        if not include_metadata:
            return []

        search_results = []
        for row in rows:
            search_results.append(
                TextSearchResult(
                    text_content_id=row.text_content_id,
                    file_id=row.file_id,
                    file_path=row.file_path,
                    filename=row.filename,
                    content=row.content_preview if preview_length else row.content,
                    content_length=row.content_length,
                    language=row.language,
                    similarity=float(row.similarity),
                    created_at=row.created_at,
                )
            )

        logger.debug(f"Found {len(search_results)} similar text embeddings")

        if self._text_cache:
            self._text_cache.put(
                query_embedding, limit, threshold, include_metadata, preview_length, search_results
            )

        return search_results

    async def store_image_embedding(
        self,
        image_id: UUID,
        embedding: list[float],
        model_name: str,
        model_version: str,
    ) -> UUID:
        """Store an image embedding vector.

        Args:
            image_id: UUID of the image
            embedding: Embedding vector (must be 512 dimensions)
            model_name: Name of the embedding model
            model_version: Version of the embedding model

        Returns:
            UUID of the created embedding record

        Raises:
            ValueError: If embedding dimension is incorrect
            SQLAlchemyError: If database operation fails
        """
        if len(embedding) != IMAGE_EMBEDDING_DIMENSION:
            raise ValueError(
                f"Image embedding must be {IMAGE_EMBEDDING_DIMENSION} dimensions, "
                f"got {len(embedding)}"
            )

        stmt = (
            insert(ImageEmbedding)
            .values(
                image_id=image_id,
                embedding=embedding,
                model_name=model_name,
                model_version=model_version,
            )
            .on_conflict_do_update(
                index_elements=["image_id"],
                set_={
                    "embedding": embedding,
                    "model_name": model_name,
                    "model_version": model_version,
                    "electric_last_modified": func.now(),
                },
            )
            .returning(ImageEmbedding.id)
        )

        result = await self.session.execute(stmt)
        embedding_id = result.scalar_one()

        logger.debug(f"Stored image embedding {embedding_id} for image {image_id}")
        return embedding_id

    async def store_image_embeddings_batch(
        self,
        embeddings: list[dict[str, Any]],
    ) -> list[UUID]:
        """Store multiple image embeddings in a batch.

        Args:
            embeddings: List of dicts with keys:
                - image_id: UUID
                - embedding: List[float]
                - model_name: str
                - model_version: str

        Returns:
            List of UUIDs for created embedding records

        Raises:
            ValueError: If any embedding has incorrect dimension
            SQLAlchemyError: If database operation fails
        """
        for emb in embeddings:
            if len(emb["embedding"]) != IMAGE_EMBEDDING_DIMENSION:
                raise ValueError(
                    f"Image embedding must be {IMAGE_EMBEDDING_DIMENSION} dimensions, "
                    f"got {len(emb['embedding'])}"
                )

        stmt = (
            insert(ImageEmbedding)
            .values(embeddings)
            .on_conflict_do_update(
                index_elements=["image_id"],
                set_={
                    "embedding": insert(ImageEmbedding).excluded.embedding,
                    "model_name": insert(ImageEmbedding).excluded.model_name,
                    "model_version": insert(ImageEmbedding).excluded.model_version,
                    "electric_last_modified": func.now(),
                },
            )
            .returning(ImageEmbedding.id)
        )

        result = await self.session.execute(stmt)
        embedding_ids = [row[0] for row in result.fetchall()]

        logger.info(f"Stored {len(embedding_ids)} image embeddings in batch")
        return embedding_ids

    async def search_image_similar(
        self,
        query_embedding: list[float],
        limit: int = DEFAULT_SEARCH_LIMIT,
        threshold: float = DEFAULT_SIMILARITY_THRESHOLD,
        include_metadata: bool = True,
    ) -> list[ImageSearchResult]:
        """Search for similar image embeddings.

        Args:
            query_embedding: Query vector (must be 512 dimensions)
            limit: Maximum number of results to return
            threshold: Minimum similarity threshold (0-1)
            include_metadata: Include file and image metadata

        Returns:
            List of ImageSearchResult ordered by similarity (highest first)

        Raises:
            ValueError: If query_embedding dimension is incorrect
            SQLAlchemyError: If database operation fails
        """
        if len(query_embedding) != IMAGE_EMBEDDING_DIMENSION:
            raise ValueError(
                f"Query embedding must be {IMAGE_EMBEDDING_DIMENSION} dimensions, "
                f"got {len(query_embedding)}"
            )

        if self._image_cache:
            cached_results = self._image_cache.get(
                query_embedding, limit, threshold, include_metadata, None
            )
            if cached_results is not None:
                logger.debug(f"Cache hit for image search (limit={limit}, threshold={threshold})")
                return cached_results



        if include_metadata:
            stmt = build_search_image_similar_with_metadata(query_embedding, threshold, limit)
        else:
            stmt = build_search_image_similar(query_embedding, threshold, limit)

        result = await self.session.execute(stmt)
        rows = result.fetchall()

        if not include_metadata:
            return []

        search_results = []
        for row in rows:
            search_results.append(
                ImageSearchResult(
                    image_id=row.image_id,
                    file_id=row.file_id,
                    file_path=row.file_path,
                    filename=row.filename,
                    width=row.width,
                    height=row.height,
                    format=row.format,
                    color_mode=row.color_mode,
                    thumbnail_path=row.thumbnail_path,
                    similarity=float(row.similarity),
                    created_at=row.created_at,
                )
            )

        logger.debug(f"Found {len(search_results)} similar image embeddings")

        if self._image_cache:
            self._image_cache.put(
                query_embedding, limit, threshold, include_metadata, None, search_results
            )

        return search_results

    async def delete_text_embedding(self, text_content_id: UUID) -> bool:
        """Delete a text embedding by text content ID.

        Args:
            text_content_id: UUID of the text content

        Returns:
            True if embedding was deleted, False if not found

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = delete(TextEmbedding).where(TextEmbedding.text_content_id == text_content_id)

        result = await self.session.execute(stmt)

        deleted = result.rowcount > 0
        if deleted:
            logger.debug(f"Deleted text embedding for content {text_content_id}")
        return deleted

    async def delete_image_embedding(self, image_id: UUID) -> bool:
        """Delete an image embedding by image ID.

        Args:
            image_id: UUID of the image

        Returns:
            True if embedding was deleted, False if not found

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = delete(ImageEmbedding).where(ImageEmbedding.image_id == image_id)

        result = await self.session.execute(stmt)

        deleted = result.rowcount > 0
        if deleted:
            logger.debug(f"Deleted image embedding for image {image_id}")
        return deleted

    async def has_text_embedding(self, text_content_id: UUID) -> bool:
        """Check if a text embedding exists.

        Args:
            text_content_id: UUID of the text content

        Returns:
            True if embedding exists, False otherwise

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = select(TextEmbedding.id).where(TextEmbedding.text_content_id == text_content_id)

        result = await self.session.execute(stmt)
        return result.scalar_one_or_none() is not None

    async def has_image_embedding(self, image_id: UUID) -> bool:
        """Check if an image embedding exists.

        Args:
            image_id: UUID of the image

        Returns:
            True if embedding exists, False otherwise

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = select(ImageEmbedding.id).where(ImageEmbedding.image_id == image_id)

        result = await self.session.execute(stmt)
        return result.scalar_one_or_none() is not None

    async def count_text_embeddings(self) -> int:
        """Count total text embeddings.

        Returns:
            Total number of text embeddings

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = select(func.count(TextEmbedding.id))
        result = await self.session.execute(stmt)
        return result.scalar_one()

    async def count_image_embeddings(self) -> int:
        """Count total image embeddings.

        Returns:
            Total number of image embeddings

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = select(func.count(ImageEmbedding.id))
        result = await self.session.execute(stmt)
        return result.scalar_one()

    async def get_embedding_stats(self) -> EmbeddingStats:
        """Get statistics about stored embeddings.

        Returns:
            EmbeddingStats with counts and model information

        Raises:
            SQLAlchemyError: If database operation fails
        """
        stmt = build_get_embedding_stats()
        result = await self.session.execute(stmt)
        row = result.fetchone()

        return EmbeddingStats(
            total_text_embeddings=row.total_text_embeddings or 0,
            total_image_embeddings=row.total_image_embeddings or 0,
            text_models=row.text_models or {},
            image_models=row.image_models or {},
            oldest_text_embedding=row.oldest_text_embedding,
            newest_text_embedding=row.newest_text_embedding,
            oldest_image_embedding=row.oldest_image_embedding,
            newest_image_embedding=row.newest_image_embedding,
        )

    async def validate_text_embedding_dimension(self, embedding: list[float]) -> bool:
        """Validate text embedding dimension.

        Args:
            embedding: Embedding vector to validate

        Returns:
            True if dimension is correct, False otherwise
        """
        return len(embedding) == TEXT_EMBEDDING_DIMENSION

    async def validate_image_embedding_dimension(self, embedding: list[float]) -> bool:
        """Validate image embedding dimension.

        Args:
            embedding: Embedding vector to validate

        Returns:
            True if dimension is correct, False otherwise
        """
        return len(embedding) == IMAGE_EMBEDDING_DIMENSION

    def invalidate_caches(self) -> None:
        if self._text_cache:
            self._text_cache.invalidate_all()
        if self._image_cache:
            self._image_cache.invalidate_all()

    def get_cache_stats(self) -> dict[str, Any]:
        stats = {}
        if self._text_cache:
            stats["text_search"] = self._text_cache.get_stats().to_dict()
        if self._image_cache:
            stats["image_search"] = self._image_cache.get_stats().to_dict()
        return stats
