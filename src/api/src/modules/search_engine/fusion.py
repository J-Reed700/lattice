class RRFFusion:
    @staticmethod
    def fuse(result_lists: list[list[dict]], k: int = 60) -> list[dict]:
        if not result_lists:
            return []

        rrf_scores: dict[str, float] = {}
        result_map: dict[str, dict] = {}

        for result_list in result_lists:
            for rank, result in enumerate(result_list, start=1):
                doc_id = result.get("file_id") or result.get("document_id") or result.get("id")
                if not doc_id:
                    continue

                doc_id_str = str(doc_id)

                score = 1.0 / (k + rank)
                rrf_scores[doc_id_str] = rrf_scores.get(doc_id_str, 0.0) + score

                if doc_id_str not in result_map:
                    result_map[doc_id_str] = result.copy()

        merged = []
        for doc_id, rrf_score in rrf_scores.items():
            result = result_map[doc_id].copy()
            result["score"] = rrf_score
            result["rrf_score"] = rrf_score
            merged.append(result)

        merged.sort(key=lambda x: x["score"], reverse=True)
        return merged

    @staticmethod
    def fuse_with_weights(
        result_lists: list[list[dict]], weights: list[float], k: int = 60
    ) -> list[dict]:
        if not result_lists:
            return []

        if len(weights) != len(result_lists):
            raise ValueError(
                f"Weights length {len(weights)} must match result_lists length {len(result_lists)}"
            )

        total_weight = sum(weights)
        if total_weight == 0:
            raise ValueError("Sum of weights cannot be zero")

        normalized_weights = [w / total_weight for w in weights]

        rrf_scores: dict[str, float] = {}
        result_map: dict[str, dict] = {}

        for result_list, weight in zip(result_lists, normalized_weights, strict=False):
            for rank, result in enumerate(result_list, start=1):
                doc_id = result.get("file_id") or result.get("document_id") or result.get("id")
                if not doc_id:
                    continue

                doc_id_str = str(doc_id)

                score = weight * (1.0 / (k + rank))
                rrf_scores[doc_id_str] = rrf_scores.get(doc_id_str, 0.0) + score

                if doc_id_str not in result_map:
                    result_map[doc_id_str] = result.copy()

        merged = []
        for doc_id, rrf_score in rrf_scores.items():
            result = result_map[doc_id].copy()
            result["score"] = rrf_score
            result["rrf_score"] = rrf_score
            merged.append(result)

        merged.sort(key=lambda x: x["score"], reverse=True)
        return merged
