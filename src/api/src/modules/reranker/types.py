from dataclasses import dataclass


@dataclass
class RerankRequest:
    """Request to rerank search results."""

    query: str
    documents: list[str]
    file_ids: list[str]
    top_k: int = 50


@dataclass
class RerankResult:
    """Single reranked result."""

    file_id: str
    score: float
    rank: int


@dataclass
class RerankResponse:
    """Response from reranking."""

    results: list[RerankResult]
    took_ms: float
    model_name: str
