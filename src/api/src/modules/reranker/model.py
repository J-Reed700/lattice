import hashlib
import logging
import time

import numpy as np
from sentence_transformers import CrossEncoder

logger = logging.getLogger(__name__)


class CrossEncoderModel:
    """Wrapper for cross-encoder model with lazy loading and caching."""

    def __init__(
        self,
        model_name: str = "BAAI/bge-reranker-v2-m3",
        device: str = "cpu",
        max_length: int = 512,
        cache_enabled: bool = True,
        cache_ttl: int = 3600,
    ):
        """Initialize cross-encoder model.

        Args:
            model_name: HuggingFace model ID
            device: "cpu" or "cuda"
            max_length: Maximum sequence length (tokens)
            cache_enabled: Enable result caching
            cache_ttl: Cache time-to-live in seconds
        """
        self.model_name = model_name
        self.device = device
        self.max_length = max_length
        self.cache_enabled = cache_enabled
        self.cache_ttl = cache_ttl
        self._model: CrossEncoder | None = None
        self._score_cache: dict[str, tuple[list[float], float]] = {}

    def _load_model(self) -> CrossEncoder:
        """Lazy load the model on first use."""
        if self._model is None:
            logger.info(f"Loading cross-encoder model: {self.model_name}")
            try:
                self._model = CrossEncoder(
                    self.model_name, device=self.device, max_length=self.max_length
                )
                logger.info(f"Model loaded successfully on {self.device}")
            except Exception as e:
                logger.error(f"Failed to load model: {e}")
                raise
        return self._model

    def _get_cache_key(self, query: str, documents: list[str]) -> str:
        """Generate cache key from query and documents."""
        content = f"{query}|{'|'.join(documents[:10])}"
        return hashlib.md5(content.encode()).hexdigest()

    def _clean_cache(self) -> None:
        """Remove expired cache entries."""
        current_time = time.time()
        expired_keys = [
            key
            for key, (_, timestamp) in self._score_cache.items()
            if current_time - timestamp > self.cache_ttl
        ]
        for key in expired_keys:
            del self._score_cache[key]

    def score_pairs(
        self, query: str, documents: list[str], batch_size: int = 32, normalize: bool = True
    ) -> list[float]:
        """Score query-document pairs.

        Args:
            query: Search query text
            documents: List of document texts
            batch_size: Batch size for inference
            normalize: Apply sigmoid normalization to scores

        Returns:
            List of scores (0.0-1.0 if normalized), one per document
        """
        if not documents:
            return []

        if self.cache_enabled:
            cache_key = self._get_cache_key(query, documents)
            if cache_key in self._score_cache:
                scores, timestamp = self._score_cache[cache_key]
                if time.time() - timestamp <= self.cache_ttl:
                    logger.debug(f"Cache hit for query: {query[:50]}...")
                    return scores

        model = self._load_model()

        pairs = [[query, doc] for doc in documents]

        scores = model.predict(
            pairs, batch_size=batch_size, show_progress_bar=False, convert_to_numpy=True
        )

        if normalize:
            scores = 1 / (1 + np.exp(-scores))

        score_list = scores.tolist()

        if self.cache_enabled:
            self._clean_cache()
            cache_key = self._get_cache_key(query, documents)
            self._score_cache[cache_key] = (score_list, time.time())

        return score_list

    def get_model_info(self) -> dict:
        """Get information about the loaded model."""
        return {
            "model_name": self.model_name,
            "device": self.device,
            "max_length": self.max_length,
            "cache_enabled": self.cache_enabled,
            "cache_size": len(self._score_cache),
            "is_loaded": self._model is not None,
        }
