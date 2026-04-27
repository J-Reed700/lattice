"""
Module: Chunking

A self-contained module for text chunking with contextual retrieval support.
Implements Anthropic's Contextual Retrieval technique to improve RAG accuracy by 49%.

Basic Usage:
    >>> from modules.chunking import ChunkingService
    >>> chunker = ChunkingService()
    >>> chunks = chunker.chunk_text_with_context(
    ...     text="Long document text...",
    ...     document_title="Architecture Guide",
    ...     file_path="docs/architecture.pdf"
    ... )

Configuration:
    >>> chunker = ChunkingService(
    ...     chunk_size=512,
    ...     chunk_overlap=50,
    ...     enable_context=True
    ... )

For detailed documentation, see: docs/CONTEXTUAL_RETRIEVAL.md
"""

from .service import Chunk, ChunkingService

__all__ = [
    "Chunk",
    "ChunkingService",
]
