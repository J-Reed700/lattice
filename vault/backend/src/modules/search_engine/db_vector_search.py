import logging

from sqlalchemy import case, func, select, text
from sqlalchemy.ext.asyncio import AsyncSession

from src.models.embedding import TextEmbedding
from src.models.file import File
from src.models.image import Image
from src.models.text_content import TextContent

from .db_results import SearchResult
from .filters import SearchFilters

logger = logging.getLogger(__name__)


class VectorSearchService:
    def __init__(self, session: AsyncSession):
        self.session = session

    async def search(
        self,
        embedding: list[float],
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        similarity_threshold: float = 0.3,
        ef_search: int = 100,
    ) -> list[SearchResult]:
        if filters:
            filters.validate()

        try:
            # Optimize HNSW index search
            await self.session.execute(
                text("SET LOCAL hnsw.ef_search = :ef_search"), {"ef_search": ef_search}
            )

            embedding_vector = embedding

            distance_expr = TextEmbedding.embedding.cosine_distance(embedding_vector)
            similarity_expr = 1 - distance_expr

            query = (
                select(
                    File.id,
                    File.path,
                    File.filename,
                    File.mime_type,
                    File.size_bytes,
                    File.modified_at,
                    File.extension,
                    similarity_expr.label("score"),
                    TextEmbedding.chunk_text.label("snippet"),
                    case(
                        (Image.thumbnail_path.isnot(None), Image.thumbnail_path), else_=None
                    ).label("thumbnail_url"),
                )
                .join(TextContent, File.id == TextContent.file_id)
                .join(TextEmbedding, TextContent.id == TextEmbedding.text_content_id)
                .outerjoin(Image, File.id == Image.file_id)
                .where(similarity_expr >= similarity_threshold)
            )

            if filters:
                query = filters.apply_to_query(query)

            query = query.order_by(distance_expr.asc()).limit(limit).offset(offset)

            result = await self.session.execute(query)
            rows = result.all()

            search_results = []
            for row in rows:
                search_result = SearchResult(
                    id=row.id,
                    file_path=row.path,
                    filename=row.filename,
                    mime_type=row.mime_type,
                    size_bytes=row.size_bytes,
                    modified_at=row.modified_at,
                    score=float(row.score),
                    snippet=row.snippet[:200] if row.snippet else None,
                    thumbnail_url=row.thumbnail_url,
                    extension=row.extension,
                )
                search_results.append(search_result)

            logger.info(
                f"Vector search completed: {len(search_results)} results "
                f"(threshold={similarity_threshold}, ef_search={ef_search})"
            )

            return search_results

        except Exception as e:
            logger.error(f"Vector search error: {e}", exc_info=True)
            raise

    async def search_with_best_chunk(
        self,
        embedding: list[float],
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        similarity_threshold: float = 0.3,
        ef_search: int = 100,
    ) -> list[SearchResult]:
        if filters:
            filters.validate()

        try:
            # Optimize HNSW index search
            await self.session.execute(
                text("SET LOCAL hnsw.ef_search = :ef_search"), {"ef_search": ef_search}
            )

            embedding_vector = embedding

            distance_expr = TextEmbedding.embedding.cosine_distance(embedding_vector)
            similarity_expr = 1 - distance_expr

            subquery = (
                select(
                    TextEmbedding.text_content_id,
                    func.max(similarity_expr).label("max_score"),
                    func.string_agg(
                        TextEmbedding.chunk_text,
                        func.cast(" ... ", type_=type(TextEmbedding.chunk_text)),
                    ).label("combined_snippet"),
                )
                .where(similarity_expr >= similarity_threshold)
                .group_by(TextEmbedding.text_content_id)
                .subquery()
            )

            query = (
                select(
                    File.id,
                    File.path,
                    File.filename,
                    File.mime_type,
                    File.size_bytes,
                    File.modified_at,
                    File.extension,
                    subquery.c.max_score.label("score"),
                    subquery.c.combined_snippet.label("snippet"),
                    case(
                        (Image.thumbnail_path.isnot(None), Image.thumbnail_path), else_=None
                    ).label("thumbnail_url"),
                )
                .join(TextContent, File.id == TextContent.file_id)
                .join(subquery, TextContent.id == subquery.c.text_content_id)
                .outerjoin(Image, File.id == Image.file_id)
            )

            if filters:
                query = filters.apply_to_query(query)

            query = query.order_by(subquery.c.max_score.desc()).limit(limit).offset(offset)

            result = await self.session.execute(query)
            rows = result.all()

            search_results = []
            for row in rows:
                search_result = SearchResult(
                    id=row.id,
                    file_path=row.path,
                    filename=row.filename,
                    mime_type=row.mime_type,
                    size_bytes=row.size_bytes,
                    modified_at=row.modified_at,
                    score=float(row.score),
                    snippet=row.snippet[:500] if row.snippet else None,
                    thumbnail_url=row.thumbnail_url,
                    extension=row.extension,
                )
                search_results.append(search_result)

            logger.info(
                f"Vector search (best chunk) completed: {len(search_results)} results "
                f"(threshold={similarity_threshold}, ef_search={ef_search})"
            )

            return search_results

        except Exception as e:
            logger.error(f"Vector search (best chunk) error: {e}", exc_info=True)
            raise
