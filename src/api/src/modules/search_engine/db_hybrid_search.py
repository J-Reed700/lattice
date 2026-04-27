import asyncio
from datetime import UTC
import logging

from sqlalchemy.ext.asyncio import AsyncSession

from .db_results import SearchResult
from .db_text_search import TextSearchService
from .db_vector_search import VectorSearchService
from .filters import SearchFilters

logger = logging.getLogger(__name__)


class HybridSearchService:
    def __init__(self, session: AsyncSession):
        self.session = session
        self.vector_search = VectorSearchService(session)
        self.text_search = TextSearchService(session)

    async def search(
        self,
        query_text: str,
        embedding: list[float],
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        vector_weight: float = 0.7,
        text_weight: float = 0.3,
        similarity_threshold: float = 0.3,
    ) -> list[SearchResult]:
        if filters:
            filters.validate()

        if vector_weight + text_weight != 1.0:
            logger.warning(
                f"Weights do not sum to 1.0: vector={vector_weight}, text={text_weight}. "
                "Normalizing weights."
            )
            total = vector_weight + text_weight
            vector_weight = vector_weight / total
            text_weight = text_weight / total

        try:
            intermediate_limit = max(100, limit * 5)

            vector_results, text_results = await asyncio.gather(
                self.vector_search.search_with_best_chunk(
                    embedding=embedding,
                    limit=intermediate_limit,
                    offset=0,
                    filters=filters,
                    similarity_threshold=similarity_threshold,
                ),
                self.text_search.search(
                    query_text=query_text,
                    limit=intermediate_limit,
                    offset=0,
                    filters=filters,
                ),
            )

            merged_scores: dict[str, tuple[float, SearchResult]] = {}

            for result in vector_results:
                file_id = str(result.id)
                weighted_score = result.score * vector_weight
                merged_scores[file_id] = (weighted_score, result)

            for result in text_results:
                file_id = str(result.id)

                if result.score > 0:
                    normalized_text_score = min(result.score / 0.1, 1.0)
                else:
                    normalized_text_score = result.score

                weighted_score = normalized_text_score * text_weight

                if file_id in merged_scores:
                    existing_score, existing_result = merged_scores[file_id]
                    combined_score = existing_score + weighted_score

                    if result.snippet and not existing_result.snippet:
                        existing_result = existing_result.with_snippet(result.snippet)

                    merged_scores[file_id] = (combined_score, existing_result)
                else:
                    merged_result = result.with_score(weighted_score)
                    merged_scores[file_id] = (weighted_score, merged_result)

            sorted_results = sorted(merged_scores.items(), key=lambda x: x[1][0], reverse=True)

            paginated_results = sorted_results[offset : offset + limit]

            final_results = []
            for file_id, (score, result) in paginated_results:
                final_result = result.with_score(score)
                final_results.append(final_result)

            logger.info(
                f"Hybrid search completed: {len(final_results)} results "
                f"(vector: {len(vector_results)}, text: {len(text_results)}, "
                f"weights: {vector_weight:.2f}/{text_weight:.2f})"
            )

            return final_results

        except Exception as e:
            logger.error(f"Hybrid search error: {e}", exc_info=True)
            raise

    async def search_reranked(
        self,
        query_text: str,
        embedding: list[float],
        limit: int = 20,
        offset: int = 0,
        filters: SearchFilters | None = None,
        vector_weight: float = 0.6,
        text_weight: float = 0.3,
        recency_weight: float = 0.1,
        similarity_threshold: float = 0.3,
    ) -> list[SearchResult]:
        if filters:
            filters.validate()

        total_weight = vector_weight + text_weight + recency_weight
        if abs(total_weight - 1.0) > 0.001:
            logger.warning(
                f"Weights do not sum to 1.0: "
                f"vector={vector_weight}, text={text_weight}, recency={recency_weight}. "
                "Normalizing weights."
            )
            vector_weight = vector_weight / total_weight
            text_weight = text_weight / total_weight
            recency_weight = recency_weight / total_weight

        try:
            results = await self.search(
                query_text=query_text,
                embedding=embedding,
                limit=limit * 3,
                offset=0,
                filters=filters,
                vector_weight=vector_weight / (vector_weight + text_weight),
                text_weight=text_weight / (vector_weight + text_weight),
                similarity_threshold=similarity_threshold,
            )

            if not results:
                return []

            from datetime import datetime

            now = datetime.now(UTC)
            max_age_days = 365 * 2

            reranked_results = []
            for result in results:
                if result.modified_at.tzinfo is None:
                    modified_at = result.modified_at.replace(tzinfo=UTC)
                else:
                    modified_at = result.modified_at.astimezone(UTC)

                age_days = (now - modified_at).days
                recency_score = max(0.0, 1.0 - (age_days / max_age_days))

                new_score = result.score * (1 - recency_weight) + recency_score * recency_weight
                reranked_results.append(result.with_score(new_score))

            reranked_results.sort(key=lambda r: r.score, reverse=True)

            paginated_results = reranked_results[offset : offset + limit]

            logger.info(
                f"Hybrid search with reranking completed: {len(paginated_results)} results "
                f"(weights: v={vector_weight:.2f}, t={text_weight:.2f}, r={recency_weight:.2f})"
            )

            return paginated_results

        except Exception as e:
            logger.error(f"Hybrid search with reranking error: {e}", exc_info=True)
            raise
