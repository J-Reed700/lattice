def min_max_normalize(scores: list[float]) -> list[float]:
    if not scores:
        return []

    min_score = min(scores)
    max_score = max(scores)

    if max_score == min_score:
        return [0.5] * len(scores)

    return [(s - min_score) / (max_score - min_score) for s in scores]


def normalize_dict_scores(results: list[dict], score_key: str = "score") -> list[dict]:
    if not results:
        return results

    scores = [r[score_key] for r in results if score_key in r]
    if not scores:
        return results

    normalized = min_max_normalize(scores)

    result_copy = [r.copy() for r in results]
    idx = 0
    for r in result_copy:
        if score_key in r:
            r[score_key] = normalized[idx]
            idx += 1

    return result_copy
