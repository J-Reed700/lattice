import logging
import re

from sqlalchemy import case, func, select
from sqlalchemy.ext.asyncio import AsyncSession

from src.models.file import File
from src.models.image import Image
from src.models.text_content import TextContent

from .db_results import SearchResult
from .filters import SearchFilters

logger = logging.getLogger(__name__)

ALLOWED_LANGUAGES = {
    "english",
    "simple",
    "french",
    "german",
    "spanish",
    "italian",
    "portuguese",
    "russian",
    "dutch",
    "swedish",
    "norwegian",
    "danish",
    "finnish",
}


class TextSearchService:
    def __init__(self, session: AsyncSession):
        self.session = session

    def _sanitize_language(self, language: str) -> str:
        if language not in ALLOWED_LANGUAGES:
            logger.warning(f"Invalid language '{language}', defaulting to 'english'")
            return "english"
        return language

    def _sanitize_query_text(self, query_text: str) -> str:
        sanitized = re.sub(r"[;&|!<>()'\"]", " ", query_text)
        sanitized = re.sub(r"\s+", " ", sanitized)
        return sanitized.strip()

    def _prepare_query(self, query_text: str) -> str:
        sanitized_text = self._sanitize_query_text(query_text)
        tokens = sanitized_text.split()

        if not tokens:
            return ""

        processed_tokens = []
        for token in tokens:
            clean_token = token.strip()
            if clean_token and clean_token.isalnum():
                processed_tokens.append(f"{clean_token}:*")

        return " & ".join(processed_tokens)

    async def search(
        self,
        query_text: str,
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        language: str = "english",
    ) -> list[SearchResult]:
        if filters:
            filters.validate()

        if not query_text.strip():
            logger.warning("Empty query text provided to text search")
            return []

        try:
            language = self._sanitize_language(language)
            tsquery = self._prepare_query(query_text)

            if not tsquery:
                logger.warning("Query text resulted in empty tsquery")
                return []

            rank_expr = func.ts_rank(TextContent.search_vector, func.to_tsquery(language, tsquery))

            headline_expr = func.ts_headline(
                language,
                TextContent.content,
                func.to_tsquery(language, tsquery),
                "MaxWords=30, MinWords=15, ShortWord=3, HighlightAll=FALSE, MaxFragments=1",
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
                    rank_expr.label("score"),
                    headline_expr.label("snippet"),
                    case(
                        (Image.thumbnail_path.isnot(None), Image.thumbnail_path), else_=None
                    ).label("thumbnail_url"),
                )
                .join(TextContent, File.id == TextContent.file_id)
                .outerjoin(Image, File.id == Image.file_id)
                .where(TextContent.search_vector.op("@@")(func.to_tsquery(language, tsquery)))
            )

            if filters:
                query = filters.apply_to_query(query)

            query = query.order_by(rank_expr.desc()).limit(limit).offset(offset)

            result = await self.session.execute(query)
            rows = result.all()

            search_results = []
            for row in rows:
                snippet = row.snippet if row.snippet else None
                if snippet and len(snippet) > 300:
                    snippet = snippet[:297] + "..."

                search_result = SearchResult(
                    id=row.id,
                    file_path=row.path,
                    filename=row.filename,
                    mime_type=row.mime_type,
                    size_bytes=row.size_bytes,
                    modified_at=row.modified_at,
                    score=float(row.score),
                    snippet=snippet,
                    thumbnail_url=row.thumbnail_url,
                    extension=row.extension,
                )
                search_results.append(search_result)

            logger.info(
                f"Text search completed: {len(search_results)} results for query '{query_text}'"
            )

            return search_results

        except Exception as e:
            logger.error(f"Text search error: {e}", exc_info=True)
            raise

    async def search_with_phrases(
        self,
        query_text: str,
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        language: str = "english",
    ) -> list[SearchResult]:
        if filters:
            filters.validate()

        if not query_text.strip():
            logger.warning("Empty query text provided to phrase search")
            return []

        try:
            language = self._sanitize_language(language)
            sanitized_query = self._sanitize_query_text(query_text)

            if not sanitized_query:
                logger.warning("Query text resulted in empty sanitized query")
                return []

            " <-> ".join(
                [token.strip() for token in sanitized_query.split() if token.strip()]
            )

            rank_expr = func.ts_rank(
                TextContent.search_vector, func.phraseto_tsquery(language, sanitized_query)
            )

            headline_expr = func.ts_headline(
                language,
                TextContent.content,
                func.phraseto_tsquery(language, sanitized_query),
                "MaxWords=30, MinWords=15, ShortWord=3, HighlightAll=FALSE, MaxFragments=1",
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
                    rank_expr.label("score"),
                    headline_expr.label("snippet"),
                    case(
                        (Image.thumbnail_path.isnot(None), Image.thumbnail_path), else_=None
                    ).label("thumbnail_url"),
                )
                .join(TextContent, File.id == TextContent.file_id)
                .outerjoin(Image, File.id == Image.file_id)
                .where(
                    TextContent.search_vector.op("@@")(
                        func.phraseto_tsquery(language, sanitized_query)
                    )
                )
            )

            if filters:
                query = filters.apply_to_query(query)

            query = query.order_by(rank_expr.desc()).limit(limit).offset(offset)

            result = await self.session.execute(query)
            rows = result.all()

            search_results = []
            for row in rows:
                snippet = row.snippet if row.snippet else None
                if snippet and len(snippet) > 300:
                    snippet = snippet[:297] + "..."

                search_result = SearchResult(
                    id=row.id,
                    file_path=row.path,
                    filename=row.filename,
                    mime_type=row.mime_type,
                    size_bytes=row.size_bytes,
                    modified_at=row.modified_at,
                    score=float(row.score),
                    snippet=snippet,
                    thumbnail_url=row.thumbnail_url,
                    extension=row.extension,
                )
                search_results.append(search_result)

            logger.info(
                f"Phrase search completed: {len(search_results)} results for query '{query_text}'"
            )

            return search_results

        except Exception as e:
            logger.error(f"Phrase search error: {e}", exc_info=True)
            raise
