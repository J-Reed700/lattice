"""ORM query builders for VectorStore operations.

This module contains ORM-based query builders used by the VectorStore.
All queries use SQLAlchemy ORM to prevent SQL injection vulnerabilities.
Pgvector operators are used for efficient similarity search.
"""


from sqlalchemy import func, literal_column, select
from sqlalchemy.sql import Select

from ...models.database import File, Image, ImageEmbedding, TextContent, TextEmbedding


def build_search_text_similar(query_embedding: list[float], threshold: float, limit: int) -> Select:
    """Build query for basic text similarity search.

    Args:
        query_embedding: Query embedding vector
        threshold: Minimum similarity threshold (0-1)
        limit: Maximum number of results

    Returns:
        SQLAlchemy Select statement
    """
    similarity = (1 - TextEmbedding.embedding.op("<=>")(query_embedding)).label("similarity")
    distance = TextEmbedding.embedding.op("<=>")(query_embedding)

    stmt = (
        select(
            TextEmbedding.id.label("text_embedding_id"), TextEmbedding.text_content_id, similarity
        )
        .where(similarity >= threshold)
        .order_by(distance)
        .limit(limit)
    )

    return stmt


def build_search_text_similar_with_metadata(
    query_embedding: list[float], threshold: float, limit: int
) -> Select:
    """Build query for text similarity search with file metadata.

    Args:
        query_embedding: Query embedding vector
        threshold: Minimum similarity threshold (0-1)
        limit: Maximum number of results

    Returns:
        SQLAlchemy Select statement
    """
    similarity = (1 - TextEmbedding.embedding.op("<=>")(query_embedding)).label("similarity")
    distance = TextEmbedding.embedding.op("<=>")(query_embedding)

    stmt = (
        select(
            TextEmbedding.id.label("text_embedding_id"),
            TextEmbedding.text_content_id,
            TextContent.file_id,
            TextContent.content,
            TextContent.content_length,
            TextContent.language,
            TextContent.created_at,
            File.path.label("file_path"),
            File.filename,
            similarity,
        )
        .join(TextContent, TextEmbedding.text_content_id == TextContent.id)
        .join(File, TextContent.file_id == File.id)
        .where(similarity >= threshold, File.is_deleted == False)
        .order_by(distance)
        .limit(limit)
    )

    return stmt


def build_search_text_similar_with_preview(
    query_embedding: list[float], threshold: float, limit: int, preview_length: int
) -> Select:
    """Build query for text similarity search with content preview.

    Args:
        query_embedding: Query embedding vector
        threshold: Minimum similarity threshold (0-1)
        limit: Maximum number of results
        preview_length: Maximum length of content preview

    Returns:
        SQLAlchemy Select statement
    """
    similarity = (1 - TextEmbedding.embedding.op("<=>")(query_embedding)).label("similarity")
    distance = TextEmbedding.embedding.op("<=>")(query_embedding)
    content_preview = func.left(TextContent.content, preview_length).label("content_preview")

    stmt = (
        select(
            TextEmbedding.id.label("text_embedding_id"),
            TextEmbedding.text_content_id,
            TextContent.file_id,
            content_preview,
            TextContent.content_length,
            TextContent.language,
            TextContent.created_at,
            File.path.label("file_path"),
            File.filename,
            similarity,
        )
        .join(TextContent, TextEmbedding.text_content_id == TextContent.id)
        .join(File, TextContent.file_id == File.id)
        .where(similarity >= threshold, File.is_deleted == False)
        .order_by(distance)
        .limit(limit)
    )

    return stmt


def build_search_image_similar(
    query_embedding: list[float], threshold: float, limit: int
) -> Select:
    """Build query for basic image similarity search.

    Args:
        query_embedding: Query embedding vector
        threshold: Minimum similarity threshold (0-1)
        limit: Maximum number of results

    Returns:
        SQLAlchemy Select statement
    """
    similarity = (1 - ImageEmbedding.embedding.op("<=>")(query_embedding)).label("similarity")
    distance = ImageEmbedding.embedding.op("<=>")(query_embedding)

    stmt = (
        select(ImageEmbedding.id.label("image_embedding_id"), ImageEmbedding.image_id, similarity)
        .where(similarity >= threshold)
        .order_by(distance)
        .limit(limit)
    )

    return stmt


def build_search_image_similar_with_metadata(
    query_embedding: list[float], threshold: float, limit: int
) -> Select:
    """Build query for image similarity search with file metadata.

    Args:
        query_embedding: Query embedding vector
        threshold: Minimum similarity threshold (0-1)
        limit: Maximum number of results

    Returns:
        SQLAlchemy Select statement
    """
    similarity = (1 - ImageEmbedding.embedding.op("<=>")(query_embedding)).label("similarity")
    distance = ImageEmbedding.embedding.op("<=>")(query_embedding)

    stmt = (
        select(
            ImageEmbedding.id.label("image_embedding_id"),
            ImageEmbedding.image_id,
            Image.file_id,
            Image.width,
            Image.height,
            Image.format,
            Image.color_mode,
            Image.thumbnail_path,
            Image.created_at,
            File.path.label("file_path"),
            File.filename,
            similarity,
        )
        .join(Image, ImageEmbedding.image_id == Image.id)
        .join(File, Image.file_id == File.id)
        .where(similarity >= threshold, File.is_deleted == False)
        .order_by(distance)
        .limit(limit)
    )

    return stmt


def build_get_embedding_stats() -> Select:
    """Build query for embedding statistics.

    Returns:
        SQLAlchemy Select statement
    """
    text_models_subq = select(
        func.json_object_agg(TextEmbedding.model_name, literal_column("model_count")).label(
            "text_models"
        )
    ).select_from(
        select(TextEmbedding.model_name, func.count().label("model_count"))
        .group_by(TextEmbedding.model_name)
        .subquery("t")
    )

    image_models_subq = select(
        func.json_object_agg(ImageEmbedding.model_name, literal_column("model_count")).label(
            "image_models"
        )
    ).select_from(
        select(ImageEmbedding.model_name, func.count().label("model_count"))
        .group_by(ImageEmbedding.model_name)
        .subquery("i")
    )

    stmt = select(
        select(func.count())
        .select_from(TextEmbedding)
        .scalar_subquery()
        .label("total_text_embeddings"),
        select(func.count())
        .select_from(ImageEmbedding)
        .scalar_subquery()
        .label("total_image_embeddings"),
        select(func.min(TextEmbedding.created_at)).scalar_subquery().label("oldest_text_embedding"),
        select(func.max(TextEmbedding.created_at)).scalar_subquery().label("newest_text_embedding"),
        select(func.min(ImageEmbedding.created_at))
        .scalar_subquery()
        .label("oldest_image_embedding"),
        select(func.max(ImageEmbedding.created_at))
        .scalar_subquery()
        .label("newest_image_embedding"),
        text_models_subq.scalar_subquery().label("text_models"),
        image_models_subq.scalar_subquery().label("image_models"),
    )

    return stmt
