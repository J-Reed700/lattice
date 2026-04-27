import asyncio
import logging
from typing import Any, Literal

from .bm25 import BM25SearchEngine
from .fusion import RRFFusion
from .scoring import normalize_dict_scores

logger = logging.getLogger(__name__)


class HybridSearchError(Exception):
    pass


class HybridSearchEngine:
    def __init__(
        self,
        vector_engine: Any,
        bm25_engine: BM25SearchEngine,
        default_strategy: Literal["rrf", "weighted"] = "rrf",
        default_vector_weight: float = 0.7,
        default_bm25_weight: float = 0.3,
        rrf_k: int = 60,
    ):
        self.vector_engine = vector_engine
        self.bm25_engine = bm25_engine
        self.default_strategy = default_strategy
        self.default_vector_weight = default_vector_weight
        self.default_bm25_weight = default_bm25_weight
        self.rrf_k = rrf_k

    async def search(
        self,
        query: str,
        embedding: list[float] | None = None,
        top_k: int = 20,
        strategy: Literal["rrf", "weighted"] | None = None,
        vector_weight: float | None = None,
        bm25_weight: float | None = None,
        filters: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]:
        if not query or not query.strip():
            raise HybridSearchError("Query cannot be empty")

        strategy = strategy or self.default_strategy
        vector_weight = vector_weight or self.default_vector_weight
        bm25_weight = bm25_weight or self.default_bm25_weight

        if strategy == "rrf":
            return await self._search_rrf(
                query=query, embedding=embedding, top_k=top_k, filters=filters
            )
        if strategy == "weighted":
            return await self._search_weighted(
                query=query,
                embedding=embedding,
                top_k=top_k,
                vector_weight=vector_weight,
                bm25_weight=bm25_weight,
                filters=filters,
            )
        raise HybridSearchError(f"Invalid strategy: {strategy}")

    async def _search_rrf(
        self,
        query: str,
        embedding: list[float] | None,
        top_k: int,
        filters: dict[str, Any] | None,
    ) -> list[dict[str, Any]]:
        try:
            vector_results, bm25_results = await asyncio.gather(
                self._run_vector_search(embedding, query, top_k, filters),
                self._run_bm25_search(query, top_k, filters),
                return_exceptions=True,
            )

            result_lists = []

            if isinstance(vector_results, list):
                result_lists.append(vector_results)
            else:
                logger.warning(f"Vector search failed: {vector_results}")

            if isinstance(bm25_results, list):
                result_lists.append(bm25_results)
            else:
                logger.warning(f"BM25 search failed: {bm25_results}")

            if not result_lists:
                logger.error("Both search methods failed in RRF")
                raise HybridSearchError("All search methods failed")

            if len(result_lists) == 1:
                logger.info(
                    f"Only one search method succeeded, returning {len(result_lists[0])} results"
                )
                return result_lists[0][:top_k]

            merged = RRFFusion.fuse(result_lists, k=self.rrf_k)
            merged = self._merge_results(merged)

            logger.info(f"RRF fusion returned {len(merged)} results")
            return merged[:top_k]

        except Exception as e:
            logger.error(f"RRF search failed: {e}", exc_info=True)
            return await self._fallback_search(query, embedding, top_k, filters)

    async def _search_weighted(
        self,
        query: str,
        embedding: list[float] | None,
        top_k: int,
        vector_weight: float,
        bm25_weight: float,
        filters: dict[str, Any] | None,
    ) -> list[dict[str, Any]]:
        try:
            vector_results, bm25_results = await asyncio.gather(
                self._run_vector_search(embedding, query, top_k, filters),
                self._run_bm25_search(query, top_k, filters),
                return_exceptions=True,
            )

            result_lists = []
            weights = []

            if isinstance(vector_results, list):
                result_lists.append(vector_results)
                weights.append(vector_weight)
            else:
                logger.warning(f"Vector search failed: {vector_results}")

            if isinstance(bm25_results, list):
                result_lists.append(bm25_results)
                weights.append(bm25_weight)
            else:
                logger.warning(f"BM25 search failed: {bm25_results}")

            if not result_lists:
                logger.error("Both search methods failed in weighted search")
                raise HybridSearchError("All search methods failed")

            if len(result_lists) == 1:
                logger.info(
                    f"Only one search method succeeded, returning {len(result_lists[0])} results"
                )
                return result_lists[0][:top_k]

            merged = RRFFusion.fuse_with_weights(result_lists, weights, k=self.rrf_k)
            merged = self._merge_results(merged)

            logger.info(f"Weighted fusion returned {len(merged)} results")
            return merged[:top_k]

        except Exception as e:
            logger.error(f"Weighted search failed: {e}", exc_info=True)
            return await self._fallback_search(query, embedding, top_k, filters)

    async def _run_vector_search(
        self,
        embedding: list[float] | None,
        query: str,
        top_k: int,
        filters: dict[str, Any] | None,
    ) -> list[dict[str, Any]]:
        if hasattr(self.vector_engine, "search"):
            if embedding:
                results = await self.vector_engine.search(
                    embedding=embedding, limit=top_k * 2, filters=filters
                )
            else:
                results = await self.vector_engine.search(
                    query=query, limit=top_k * 2, filters=filters
                )

            formatted_results = []
            for r in results:
                formatted_results.append(
                    {
                        "file_id": str(getattr(r, "id", None) or getattr(r, "file_id", None)),
                        "file_path": getattr(r, "file_path", None) or getattr(r, "path", None),
                        "filename": getattr(r, "filename", ""),
                        "score": float(getattr(r, "score", 0.0)),
                        "snippet": getattr(r, "snippet", None),
                    }
                )

            normalized = normalize_dict_scores(formatted_results, "score")
            return normalized

        raise HybridSearchError("Vector engine does not support search")

    async def _run_bm25_search(
        self, query: str, top_k: int, filters: dict[str, Any] | None
    ) -> list[dict[str, Any]]:
        results = await self.bm25_engine.search(query=query, top_k=top_k * 2, filters=filters)

        formatted_results = []
        for r in results:
            formatted_results.append(
                {
                    "file_id": str(r.get("document_id", "")),
                    "file_path": r.get("file_path", ""),
                    "filename": r.get("filename", ""),
                    "score": float(r.get("bm25_score", 0.0)),
                }
            )

        return formatted_results

    def _merge_results(self, results: list[dict[str, Any]]) -> list[dict[str, Any]]:
        seen = set()
        merged = []

        for result in results:
            file_id = result.get("file_id")
            if file_id and file_id not in seen:
                seen.add(file_id)
                merged.append(result)

        return merged

    async def _fallback_search(
        self,
        query: str,
        embedding: list[float] | None,
        top_k: int,
        filters: dict[str, Any] | None,
    ) -> list[dict[str, Any]]:
        try:
            logger.info("Attempting fallback to vector search only")
            return await self._run_vector_search(embedding, query, top_k, filters)
        except Exception as vector_error:
            logger.error(f"Vector fallback failed: {vector_error}")
            try:
                logger.info("Attempting fallback to BM25 search only")
                return await self._run_bm25_search(query, top_k, filters)
            except Exception as bm25_error:
                logger.error(f"BM25 fallback failed: {bm25_error}")
                return []
