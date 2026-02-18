import asyncio
import logging

from ..search_engine.types import SearchResult
from .model import CrossEncoderModel

logger = logging.getLogger(__name__)


class RerankService:
    """Service for reranking search results using cross-encoder."""

    _instances: dict[str, CrossEncoderModel] = {}

    def __init__(
        self,
        model_name: str = "BAAI/bge-reranker-v2-m3",
        device: str = "cpu",
        max_content_length: int = 2000,
        batch_size: int = 32,
        cache_enabled: bool = True,
        cache_ttl: int = 3600,
    ):
        """Initialize reranking service.

        Args:
            model_name: HuggingFace model ID
            device: "cpu" or "cuda"
            max_content_length: Max chars to read from each file
            batch_size: Batch size for inference
            cache_enabled: Enable result caching
            cache_ttl: Cache TTL in seconds
        """
        self.model_name = model_name
        self.device = device
        self.max_content_length = max_content_length
        self.batch_size = batch_size
        self.cache_enabled = cache_enabled
        self.cache_ttl = cache_ttl

    def _get_model(self) -> CrossEncoderModel:
        """Get singleton model instance per model name."""
        if self.model_name not in RerankService._instances:
            RerankService._instances[self.model_name] = CrossEncoderModel(
                model_name=self.model_name,
                device=self.device,
                cache_enabled=self.cache_enabled,
                cache_ttl=self.cache_ttl,
            )
        return RerankService._instances[self.model_name]

    @staticmethod
    def get_model_info() -> dict:
        """Get information about all loaded models."""
        return {
            model_name: model.get_model_info()
            for model_name, model in RerankService._instances.items()
        }

    def _load_document_content_sync(self, file_path: str) -> str:
        """Load document text from file (synchronous).

        Args:
            file_path: Path to document file

        Returns:
            Document content (first max_content_length chars)
        """
        try:
            with open(file_path, encoding="utf-8", errors="ignore") as f:
                content = f.read(self.max_content_length)
            return content
        except Exception as e:
            logger.warning(f"Failed to load {file_path}: {e}")
            return ""

    async def _load_document_content(self, file_path: str) -> str:
        """Load document text from file (async wrapper).

        Args:
            file_path: Path to document file

        Returns:
            Document content (first max_content_length chars)
        """
        return await asyncio.to_thread(self._load_document_content_sync, file_path)

    async def _rerank_batch(
        self, query: str, results: list[SearchResult], top_k: int
    ) -> list[SearchResult]:
        """Rerank a batch of search results.

        Args:
            query: Original search query
            results: Search results to rerank
            top_k: Number of results to return

        Returns:
            Reranked search results
        """
        if not results:
            return []

        document_tasks = [self._load_document_content(result.file_path) for result in results]
        documents = await asyncio.gather(*document_tasks)

        model = self._get_model()
        loop = asyncio.get_event_loop()

        def score_with_batch():
            return model.score_pairs(query, documents, batch_size=self.batch_size)

        scores = await loop.run_in_executor(None, score_with_batch)

        results = [
            result.with_score(float(score)) for result, score in zip(results, scores, strict=False)
        ]

        reranked = sorted(results, key=lambda x: x.score, reverse=True)
        return reranked[:top_k]

    async def rerank(
        self, query: str, results: list[SearchResult], top_k: int = 50, timeout: float = 0.5
    ) -> list[SearchResult]:
        """Rerank search results using cross-encoder.

        Args:
            query: Original search query
            results: Results from hybrid search
            top_k: Number of results to rerank and return
            timeout: Max reranking time (seconds)

        Returns:
            Reranked results with updated scores

        Raises:
            Does NOT raise exceptions - returns original results on error
        """
        if not results:
            return []

        try:
            reranked = await asyncio.wait_for(
                self._rerank_batch(query, results, top_k), timeout=timeout
            )

            logger.info(f"Reranked {len(results)} results to {len(reranked)} in {timeout}s budget")
            return reranked

        except TimeoutError:
            logger.warning(f"Reranking timed out after {timeout}s, returning original results")
            return results[:top_k]

        except Exception as e:
            logger.error(f"Reranking failed: {e}", exc_info=True)
            return results[:top_k]
