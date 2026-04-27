"""SQLite FTS5 BM25 scoring and normalization utilities."""

import logging
from typing import Any

from sqlalchemy import text
from sqlalchemy.ext.asyncio import AsyncSession

logger = logging.getLogger(__name__)


class BM25SearchError(Exception):
    pass


class BM25Scorer:
    """SQLite FTS5 BM25 scoring with min-max normalization."""

    @staticmethod
    async def search_fts5(
        query: str,
        db_session: AsyncSession,
        filters: dict[str, Any] | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        """Execute FTS5 BM25 search and normalize scores.

        Args:
            query: Search query text
            db_session: Database session
            filters: Optional SQL filters (file_type, date_range, etc.)
            limit: Maximum results to return

        Returns:
            List of dicts with keys:
            - document_id: str
            - file_path: str
            - filename: str
            - bm25_score: float (normalized 0-1)
            - raw_bm25: float (unnormalized)

        Raises:
            ValueError: If query is empty
        """
        if not query or not query.strip():
            raise ValueError("Query cannot be empty")

        sql = """
        WITH bm25_results AS (
            SELECT
                d.id as document_id,
                d.path as file_path,
                d.filename,
                d.extension,
                bm25(fts_documents) as raw_bm25
            FROM fts_documents fts
            JOIN documents d ON fts.document_id = d.id
            WHERE fts_documents MATCH :query
            ORDER BY raw_bm25
            LIMIT :limit
        )
        SELECT
            document_id,
            file_path,
            filename,
            extension,
            raw_bm25,
            CASE
                WHEN MAX(raw_bm25) OVER () - MIN(raw_bm25) OVER () = 0 THEN 0.5
                ELSE (raw_bm25 - MIN(raw_bm25) OVER ()) /
                     (MAX(raw_bm25) OVER () - MIN(raw_bm25) OVER ())
            END as bm25_score
        FROM bm25_results
        ORDER BY bm25_score DESC
        """

        try:
            result = await db_session.execute(text(sql), {"query": query, "limit": limit})

            rows = result.fetchall()

            results = []
            for row in rows:
                results.append(
                    {
                        "document_id": str(row.document_id),
                        "file_path": row.file_path,
                        "filename": row.filename,
                        "extension": row.extension,
                        "raw_bm25": float(row.raw_bm25),
                        "bm25_score": float(row.bm25_score),
                    }
                )

            logger.info(f"BM25 search for '{query}': {len(results)} results")
            return results

        except Exception as e:
            logger.error(f"BM25 search failed: {e}", exc_info=True)
            raise

    @staticmethod
    def normalize_scores(
        results: list[dict[str, Any]], score_key: str = "score"
    ) -> list[dict[str, Any]]:
        """Min-max normalize scores to 0-1 range.

        Args:
            results: List of result dicts
            score_key: Key name for score field

        Returns:
            Results with normalized scores
        """
        if not results:
            return results

        scores = [r[score_key] for r in results]
        min_score = min(scores)
        max_score = max(scores)

        if max_score == min_score:
            for r in results:
                r[score_key] = 0.5
            return results

        for r in results:
            r[score_key] = (r[score_key] - min_score) / (max_score - min_score)

        return results


class BM25SearchEngine:
    def __init__(self, db_session: AsyncSession, top_k: int = 100):
        self.db_session = db_session
        self.top_k = top_k
        self._corpus_built = False

    async def build_corpus(self, force_rebuild: bool = False) -> None:
        if self._corpus_built and not force_rebuild:
            logger.debug("BM25 corpus already built, skipping")
            return

        try:
            result = await self.db_session.execute(text("SELECT COUNT(*) FROM fts_documents"))
            count = result.scalar()
            logger.info(f"BM25 corpus check: {count} documents in FTS index")
            self._corpus_built = True
        except Exception as e:
            logger.warning(f"Could not verify FTS index: {e}")
            raise BM25SearchError(f"Failed to build corpus: {e}") from e

    async def search(
        self, query: str, top_k: int | None = None, filters: dict[str, Any] | None = None
    ) -> list[dict[str, Any]]:
        if not query or not query.strip():
            raise BM25SearchError("Query cannot be empty")

        limit = top_k or self.top_k

        try:
            results = await BM25Scorer.search_fts5(
                query=query, db_session=self.db_session, filters=filters, limit=limit
            )

            logger.info(f"BM25 search returned {len(results)} results for query: {query[:50]}")
            return results

        except Exception as e:
            logger.error(f"BM25 search failed: {e}", exc_info=True)
            raise BM25SearchError(f"BM25 search failed: {e}") from e

    def tokenize(self, text: str) -> list[str]:
        return text.lower().split()
