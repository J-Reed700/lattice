import asyncio
from pathlib import Path
import threading
from typing import Any

import numpy as np
from sentence_transformers import SentenceTransformer
import torch
from transformers import AutoTokenizer

from ..caching.embedding_cache import EmbeddingCache
from .model_manager import ModelManager
from .types import (
    BATCH_SIZE_CPU,
    BATCH_SIZE_GPU,
    MAX_TEXT_LENGTH,
    TEXT_CHUNK_OVERLAP,
    TEXT_CHUNK_SIZE,
    TEXT_EMBEDDING_DIM,
    TEXT_MODEL_NAME,
    InputValidationError,
    ModelLoadError,
    OutOfMemoryError,
    ProcessingError,
    ProgressCallback,
)
from .validator import validate_embedding_dimension

_text_model = None
_text_tokenizer = None
_text_model_lock = threading.Lock()
_embedding_cache = None
_cache_lock = threading.Lock()


class TextEmbedder:
    def __init__(
        self,
        device: str | None = None,
        batch_size: int | None = None,
        enable_cache: bool = True,
        cache_file: Path | None = None,
        cache_size: int = 10000,
    ):
        if device is None:
            device = "cuda" if torch.cuda.is_available() else "cpu"
        self._device = device

        if batch_size is None:
            batch_size = BATCH_SIZE_GPU if device == "cuda" else BATCH_SIZE_CPU
        self._batch_size = batch_size

        self._model = None
        self._tokenizer = None
        self._enable_cache = enable_cache

        if enable_cache:
            global _embedding_cache
            with _cache_lock:
                if _embedding_cache is None:
                    _embedding_cache = EmbeddingCache(
                        max_size=cache_size,
                        cache_file=cache_file,
                        embedding_dim=TEXT_EMBEDDING_DIM,
                    )
            self._cache = _embedding_cache
        else:
            self._cache = None

    def _load_model(self) -> None:
        global _text_model, _text_tokenizer

        if not ModelManager.is_model_cached(TEXT_MODEL_NAME):
            ModelManager.download_model(TEXT_MODEL_NAME)

        with _text_model_lock:
            if _text_model is None:
                try:
                    _text_model = SentenceTransformer(
                        TEXT_MODEL_NAME,
                        device=self._device,
                        cache_folder=str(ModelManager.get_model_cache_dir()),
                    )
                    _text_tokenizer = AutoTokenizer.from_pretrained(
                        TEXT_MODEL_NAME, cache_dir=str(ModelManager.get_model_cache_dir())
                    )

                except Exception as e:
                    raise ModelLoadError(f"Failed to load text model: {e!s}")

        self._model = _text_model
        self._tokenizer = _text_tokenizer

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    @property
    def embedding_dim(self) -> int:
        return TEXT_EMBEDDING_DIM

    @property
    def device(self) -> str:
        return self._device

    @classmethod
    def needs_migration(cls, existing_dim: int) -> bool:
        """Check if existing embeddings need migration to new model."""
        return existing_dim != TEXT_EMBEDDING_DIM

    def _add_context(self, chunk: str, metadata: dict[str, Any], chunk_idx: int) -> str:
        context_parts = []

        if metadata.get("title"):
            context_parts.append(f"Document: {metadata['title']}")

        if metadata.get("file_name"):
            context_parts.append(f"File: {metadata['file_name']}")

        if metadata.get("file_path"):
            context_parts.append(f"Source: {metadata['file_path']}")

        if metadata.get("author"):
            context_parts.append(f"Author: {metadata['author']}")

        if metadata.get("page_count"):
            context_parts.append(f"Pages: {metadata['page_count']}")

        context_parts.append(f"Chunk {chunk_idx + 1}")

        context = ". ".join(context_parts)
        return f"{context}.\n\n{chunk}"

    def chunk_text(self, text: str) -> list[str]:
        if not text or not text.strip():
            raise InputValidationError("Text cannot be empty")

        if len(text) > MAX_TEXT_LENGTH:
            raise InputValidationError(
                f"Text exceeds maximum length of {MAX_TEXT_LENGTH} characters"
            )

        if not self.is_loaded:
            self._load_model()

        tokens = self._tokenizer.encode(text, add_special_tokens=False)

        if len(tokens) <= TEXT_CHUNK_SIZE:
            return [text]

        chunks = []
        start = 0

        while start < len(tokens):
            end = start + TEXT_CHUNK_SIZE
            chunk_tokens = tokens[start:end]

            chunk_text = self._tokenizer.decode(
                chunk_tokens, skip_special_tokens=True, clean_up_tokenization_spaces=True
            )
            chunks.append(chunk_text)

            start += TEXT_CHUNK_SIZE - TEXT_CHUNK_OVERLAP

        return chunks

    def _embed_sync(
        self, text: str, metadata: dict[str, Any] | None = None
    ) -> tuple[np.ndarray, list[tuple[str, str]]]:
        """Synchronous embedding generation (for use with asyncio.to_thread)."""
        if not text or not text.strip():
            raise InputValidationError("Text cannot be empty")

        if self._cache:
            cached_embedding = self._cache.get(text, TEXT_MODEL_NAME)
            if cached_embedding is not None:
                chunks = self.chunk_text(text)
                if metadata is None:
                    metadata = {}
                contextualized_chunks = [
                    self._add_context(chunk, metadata, chunk_idx)
                    for chunk_idx, chunk in enumerate(chunks)
                ]
                chunk_pairs = list(zip(chunks, contextualized_chunks, strict=False))
                return cached_embedding, chunk_pairs

        if not self.is_loaded:
            self._load_model()

        chunks = self.chunk_text(text)

        if metadata is None:
            metadata = {}

        contextualized_chunks = [
            self._add_context(chunk, metadata, chunk_idx) for chunk_idx, chunk in enumerate(chunks)
        ]

        chunk_pairs = list(zip(chunks, contextualized_chunks, strict=False))

        try:
            embeddings = self._model.encode(
                contextualized_chunks,
                batch_size=self._batch_size,
                show_progress_bar=False,
                convert_to_numpy=True,
                normalize_embeddings=False,
            )

            if embeddings.shape[1] > TEXT_EMBEDDING_DIM:
                embeddings = embeddings[:, :TEXT_EMBEDDING_DIM]

            if len(chunks) == 1:
                embedding = embeddings[0]
            else:
                embedding = np.mean(embeddings, axis=0)

            norm = np.linalg.norm(embedding)
            if norm > 0:
                embedding = embedding / norm

            final_embedding = embedding.astype(np.float32)

            validate_embedding_dimension(final_embedding, "TextEmbedder.embed")

            if self._cache:
                self._cache.put(text, TEXT_MODEL_NAME, final_embedding)

            return final_embedding, chunk_pairs

        except torch.cuda.OutOfMemoryError:
            raise OutOfMemoryError("GPU out of memory. Try reducing batch size or using CPU.")
        except Exception as e:
            raise ProcessingError(f"Failed to generate embedding: {e!s}")

    async def embed(
        self, text: str, metadata: dict[str, Any] | None = None
    ) -> tuple[np.ndarray, list[tuple[str, str]]]:
        """Generate text embedding asynchronously using thread pool."""
        return await asyncio.to_thread(self._embed_sync, text, metadata)

    async def embed_batch(
        self,
        texts: list[str],
        metadata_list: list[dict[str, Any]] | None = None,
        progress_callback: ProgressCallback | None = None,
    ) -> list[tuple[np.ndarray, list[tuple[str, str]]]]:
        """Generate batch embeddings asynchronously."""
        if not texts:
            raise InputValidationError("Text list cannot be empty")

        for i, text in enumerate(texts):
            if not text or not text.strip():
                raise InputValidationError(f"Text at index {i} is empty")

        if not self.is_loaded:
            self._load_model()

        if metadata_list is None:
            metadata_list = [{} for _ in texts]

        embeddings = []
        total = len(texts)

        try:
            for i, (text, metadata) in enumerate(zip(texts, metadata_list, strict=False)):
                embedding, chunk_pairs = await self.embed(text, metadata)
                embeddings.append((embedding, chunk_pairs))

                if progress_callback:
                    progress_callback(i + 1, total)

            return embeddings

        except torch.cuda.OutOfMemoryError:
            raise OutOfMemoryError("GPU out of memory. Try reducing batch size or using CPU.")
        except Exception as e:
            if isinstance(e, (InputValidationError, ProcessingError, OutOfMemoryError)):
                raise
            raise ProcessingError(f"Failed to generate batch embeddings: {e!s}")

    def _embed_chunks_batch_sync(
        self, chunks_with_context: list[str], batch_size: int = 32
    ) -> np.ndarray:
        """Generate embeddings for multiple chunks in batches (synchronous).

        This method processes multiple chunks efficiently using the model's
        batch encoding capabilities, reducing inference time by 5-10x.

        Args:
            chunks_with_context: List of contextualized chunk texts
            batch_size: Number of chunks to process per batch

        Returns:
            Array of embeddings, shape (num_chunks, embedding_dim)
        """
        if not self.is_loaded:
            self._load_model()

        all_embeddings = []

        for i in range(0, len(chunks_with_context), batch_size):
            batch = chunks_with_context[i : i + batch_size]

            batch_embeddings = self._model.encode(
                batch,
                batch_size=len(batch),
                show_progress_bar=False,
                convert_to_numpy=True,
                normalize_embeddings=False,
            )

            if batch_embeddings.shape[1] > TEXT_EMBEDDING_DIM:
                batch_embeddings = batch_embeddings[:, :TEXT_EMBEDDING_DIM]

            all_embeddings.append(batch_embeddings)

        return np.vstack(all_embeddings)

    async def embed_chunks_batch(
        self, chunks_with_context: list[str], batch_size: int = 32
    ) -> np.ndarray:
        """Generate embeddings for multiple chunks in batches (async).

        Args:
            chunks_with_context: List of contextualized chunk texts
            batch_size: Number of chunks to process per batch

        Returns:
            Array of embeddings, shape (num_chunks, embedding_dim)
        """
        return await asyncio.to_thread(
            self._embed_chunks_batch_sync, chunks_with_context, batch_size
        )

    def get_cache_stats(self) -> dict[str, Any] | None:
        if self._cache:
            return self._cache.get_stats().to_dict()
        return None

    def clear_cache(self) -> None:
        if self._cache:
            self._cache.clear()

    def persist_cache(self) -> None:
        if self._cache:
            self._cache.persist()
