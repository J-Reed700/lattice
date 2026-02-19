"""
Result Ranker

Reciprocal Rank Fusion (RRF) and result boosting for hybrid search.
"""

from datetime import datetime, timedelta
import logging
import math

from .types import SearchResult

logger = logging.getLogger(__name__)
RRF_K = 60


class ResultRanker:
    """Rank and merge search results using RRF.

    Example:
        >>> semantic_results = [result1, result2]
        >>> keyword_results = [result3, result1]
        >>> merged = ResultRanker.reciprocal_rank_fusion([
        ...     semantic_results,
        ...     keyword_results
        ... ])
    """

    @staticmethod
    def reciprocal_rank_fusion(
        result_lists: list[list[SearchResult]], k: int = RRF_K
    ) -> list[SearchResult]:
        """Merge multiple ranked lists using RRF.

        Formula: score(doc) = Σ 1/(k + rank_i(doc))
        where k=60, rank_i is 1-indexed rank in list i

        Args:
            result_lists: List of ranked result lists
            k: RRF constant (default: 60)

        Returns:
            Merged and re-ranked list of SearchResult objects

        Example:
            >>> results = ResultRanker.reciprocal_rank_fusion([
            ...     [result_a, result_b],
            ...     [result_b, result_c]
            ... ])
        """
        rrf_scores: dict[str, float] = {}
        result_map: dict[str, SearchResult] = {}

        for result_list in result_lists:
            for rank, result in enumerate(result_list, start=1):
                file_id = result.file_id

                score = 1.0 / (k + rank)

                if file_id in rrf_scores:
                    rrf_scores[file_id] += score
                else:
                    rrf_scores[file_id] = score
                    result_map[file_id] = result

        merged_results = []
        for file_id, rrf_score in rrf_scores.items():
            result = result_map[file_id]
            result = result.with_score(rrf_score)
            merged_results.append(result)

        merged_results.sort(key=lambda r: r.score, reverse=True)

        return merged_results

    @staticmethod
    def apply_boosts(
        results: list[SearchResult],
        recency_boost: float = 1.0,
        type_boosts: dict[str, float] = None,
    ) -> list[SearchResult]:
        """Apply recency and file type boosts to scores.

        Args:
            results: List of SearchResult objects
            recency_boost: Boost multiplier for recent files (0.0-1.0)
            type_boosts: Dictionary of file extensions to boost multipliers

        Returns:
            Results with boosted scores, re-sorted

        Example:
            >>> boosted = ResultRanker.apply_boosts(
            ...     results,
            ...     recency_boost=0.2,
            ...     type_boosts={'.pdf': 1.1, '.txt': 1.05}
            ... )
        """
        if type_boosts is None:
            type_boosts = {}

        now = datetime.now()
        thirty_days_ago = now - timedelta(days=30)

        for result in results:
            modified_date = result.metadata.get("modified_date")
            if modified_date and isinstance(modified_date, datetime):
                if modified_date > thirty_days_ago:
                    days_old = (now - modified_date).days
                    boost = 1.0 + (recency_boost * (30 - days_old) / 30)
                    result.score *= boost

            for ext, boost in type_boosts.items():
                if result.file_path.endswith(ext):
                    result.score *= boost
                    break

        results.sort(key=lambda r: r.score, reverse=True)

        return results

    @staticmethod
    def deduplicate(results: list[SearchResult]) -> list[SearchResult]:
        """Remove duplicate file_ids, keeping highest scored.

        Args:
            results: List of SearchResult objects

        Returns:
            Deduplicated list of SearchResult objects

        Example:
            >>> unique = ResultRanker.deduplicate(results)
        """
        seen_ids = set()
        deduplicated = []

        for result in results:
            if result.file_id not in seen_ids:
                seen_ids.add(result.file_id)
                deduplicated.append(result)

        return deduplicated

    @staticmethod
    def weighted_reciprocal_rank_fusion(
        result_lists: list[list[SearchResult]], weights: list[float], k: int = 60
    ) -> list[SearchResult]:
        """Weighted RRF merging of multiple result lists.

        Formula: score(doc) = Σ w_i * (1/(k + rank_i(doc)))

        Args:
            result_lists: List of ranked result lists
            weights: List of weights (same length as result_lists)
            k: RRF constant (default: 60)

        Returns:
            Merged results with weighted scores

        Raises:
            ValueError: If weights length doesn't match result_lists

        Example:
            >>> merged = ResultRanker.weighted_reciprocal_rank_fusion(
            ...     [semantic_results, keyword_results],
            ...     [0.7, 0.3]
            ... )
        """
        if len(weights) != len(result_lists):
            raise ValueError(
                f"weights length ({len(weights)}) must match result_lists length ({len(result_lists)})"
            )

        total = sum(weights)
        if not math.isclose(total, 1.0, abs_tol=0.01):
            logger.info(f"Normalizing weights from {weights} (sum={total}) to sum=1.0")
            weights = [w / total for w in weights]

        rrf_scores: dict[str, float] = {}
        result_map: dict[str, SearchResult] = {}

        for weight, result_list in zip(weights, result_lists, strict=False):
            for rank, result in enumerate(result_list, start=1):
                file_id = result.file_id
                score = weight * (1.0 / (k + rank))

                if file_id in rrf_scores:
                    rrf_scores[file_id] += score
                else:
                    rrf_scores[file_id] = score
                    result_map[file_id] = result

        merged = []
        for file_id, rrf_score in rrf_scores.items():
            result = result_map[file_id]
            result = result.with_score(rrf_score)

            if not hasattr(result, "metadata") or result.metadata is None:
                result.metadata = {}
            result.metadata["rrf_score"] = rrf_score

            merged.append(result)

        merged.sort(key=lambda r: r.score, reverse=True)

        logger.info(
            f"Weighted RRF merged {sum(len(rl) for rl in result_lists)} results to {len(merged)} "
            f"with weights {weights}"
        )

        return merged
