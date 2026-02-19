"""
Unified embedding service that wraps text and image embedders.

This service provides a single interface for generating embeddings from various sources.
"""

from __future__ import annotations

import logging

from .image_embedder import ImageEmbedder
from .text_embedder import TextEmbedder

logger = logging.getLogger(__name__)


class TextGeneratorWrapper:
    """Wrapper to provide compatibility with old TextEmbeddingGenerator interface."""

    def __init__(self, embedder: TextEmbedder) -> None:
        self._embedder = embedder

    async def generate_from_text(self, text: str) -> list[float]:
        """Generate embedding from text (compatibility method)."""
        return await self._embedder.embed(text)

    def __getattr__(self, name: str):
        """Delegate other attributes to the underlying embedder."""
        return getattr(self._embedder, name)


class ImageGeneratorWrapper:
    """Wrapper to provide compatibility with old ImageEmbeddingGenerator interface."""

    def __init__(self, embedder: ImageEmbedder) -> None:
        self._embedder = embedder

    async def generate(self, file_path: str) -> list[float]:
        """Generate embedding from image file (compatibility method)."""
        return await self._embedder.embed_image(file_path)

    def __getattr__(self, name: str):
        """Delegate other attributes to the underlying embedder."""
        return getattr(self._embedder, name)


class EmbeddingService:
    """
    Unified embedding service for text and images.

    Provides lazy loading of embedders and a unified interface compatible
    with the old service interface.
    """

    def __init__(self) -> None:
        self._image_embedder: ImageEmbedder | None = None
        self._text_embedder: TextEmbedder | None = None
        self._text_wrapper: TextGeneratorWrapper | None = None
        self._image_wrapper: ImageGeneratorWrapper | None = None

    @property
    def text_generator(self) -> TextGeneratorWrapper:
        """Get text embedder instance with compatibility wrapper (lazy loaded)."""
        if self._text_wrapper is None:
            if self._text_embedder is None:
                self._text_embedder = TextEmbedder()
            self._text_wrapper = TextGeneratorWrapper(self._text_embedder)
        return self._text_wrapper

    @property
    def image_generator(self) -> ImageGeneratorWrapper:
        """Get image embedder instance with compatibility wrapper (lazy loaded)."""
        if self._image_wrapper is None:
            if self._image_embedder is None:
                self._image_embedder = ImageEmbedder()
            self._image_wrapper = ImageGeneratorWrapper(self._image_embedder)
        return self._image_wrapper


__all__ = ["EmbeddingService"]
