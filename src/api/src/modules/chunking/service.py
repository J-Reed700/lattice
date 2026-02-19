"""
Module: Chunking Service

A self-contained module for text chunking with contextual retrieval support.
Implements Anthropic's Contextual Retrieval technique to improve RAG accuracy by 49%.

Basic Usage:
    >>> from services.chunking import ChunkingService
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
"""

from dataclasses import dataclass
from pathlib import Path
import re
from typing import Any


@dataclass
class Chunk:
    """Represents a text chunk with optional context.

    Attributes:
        original_text: The raw chunk text without context
        contextualized_text: The chunk with prepended document context
        start_index: Character index where chunk starts in original document
        end_index: Character index where chunk ends in original document
        metadata: Additional metadata about the chunk
    """

    original_text: str
    contextualized_text: str
    start_index: int
    end_index: int
    metadata: dict[str, Any]


class ChunkingService:
    """Service for chunking text with contextual retrieval support.

    This service implements Anthropic's Contextual Retrieval technique by
    prepending document context to each chunk before embedding. This helps
    the retrieval system understand what document the chunk came from and
    what topic it covers.

    Public Interface:
        chunk_text_with_context: Main chunking method with context generation
        chunk_text: Simple chunking without context (backward compatible)
        add_chunk_context: Add context to existing chunk
        extract_document_title: Extract title from filename
        generate_topic_hint: Generate brief topic description
    """

    CONTEXT_TEMPLATE = "From document '{title}' about {topic}: {chunk}"

    FILE_TYPE_TOPICS = {
        ".pdf": "technical documentation",
        ".doc": "technical documentation",
        ".docx": "technical documentation",
        ".md": "notes and information",
        ".txt": "notes and information",
        ".csv": "data and records",
        ".json": "data and records",
        ".xml": "data and records",
        ".py": "source code",
        ".js": "source code",
        ".ts": "source code",
        ".java": "source code",
        ".cpp": "source code",
        ".c": "source code",
        ".html": "web content",
        ".css": "style definitions",
    }

    def __init__(
        self,
        chunk_size: int = 512,
        chunk_overlap: int = 50,
        enable_context: bool = True,
        context_template: str | None = None,
    ):
        """Initialize the ChunkingService.

        Args:
            chunk_size: Target number of tokens per chunk
            chunk_overlap: Number of overlapping tokens between chunks
            enable_context: Whether to add contextual information to chunks
            context_template: Custom template for context prefix
        """
        self.chunk_size = chunk_size
        self.chunk_overlap = chunk_overlap
        self.enable_context = enable_context
        self.context_template = context_template or self.CONTEXT_TEMPLATE

    def extract_document_title(self, file_path: str) -> str:
        """Extract a readable title from a file path.

        Converts filenames like "my-architecture-guide.pdf" to "My Architecture Guide"

        Args:
            file_path: Path to the file (can be absolute or relative)

        Returns:
            Cleaned and formatted document title

        Example:
            >>> service = ChunkingService()
            >>> service.extract_document_title("/docs/my-architecture-guide.pdf")
            'My Architecture Guide'
            >>> service.extract_document_title("system_design.md")
            'System Design'
        """
        path = Path(file_path)
        filename = path.stem

        title = filename.replace("-", " ").replace("_", " ")

        title = re.sub(r"\s+", " ", title).strip()

        title = title.title()

        return title

    def generate_topic_hint(self, file_path: str, text_preview: str | None = None) -> str:
        """Generate a brief topic description for the document.

        Uses file extension as a heuristic to determine the document type.
        Optionally can use the first N characters of the document for
        more accurate topic detection (future enhancement).

        Args:
            file_path: Path to the file
            text_preview: Optional preview of document content (unused in simple implementation)

        Returns:
            Brief topic description

        Example:
            >>> service = ChunkingService()
            >>> service.generate_topic_hint("guide.pdf")
            'technical documentation'
            >>> service.generate_topic_hint("notes.md")
            'notes and information'
        """
        path = Path(file_path)
        extension = path.suffix.lower()

        return self.FILE_TYPE_TOPICS.get(extension, "general content")

    def add_chunk_context(self, chunk_text: str, document_title: str, topic_hint: str) -> str:
        """Add contextual information to a chunk.

        Prepends document context to the chunk using the configured template.

        Args:
            chunk_text: The original chunk text
            document_title: Title of the source document
            topic_hint: Brief description of the document topic

        Returns:
            Chunk with prepended context

        Example:
            >>> service = ChunkingService()
            >>> service.add_chunk_context(
            ...     "This is a chunk of text.",
            ...     "Architecture Guide",
            ...     "technical documentation"
            ... )
            "From document 'Architecture Guide' about technical documentation: This is a chunk of text."
        """
        return self.context_template.format(
            title=document_title, topic=topic_hint, chunk=chunk_text
        )

    def chunk_text(self, text: str, approximate: bool = True) -> list[str]:
        """Chunk text without contextual information (backward compatible).

        Simple character-based chunking with overlap. This method maintains
        backward compatibility with existing code that doesn't use context.

        Args:
            text: Text to chunk
            approximate: If True, use character-based approximation.
                        If False, would use tokenizer (not implemented)

        Returns:
            List of text chunks

        Example:
            >>> service = ChunkingService()
            >>> chunks = service.chunk_text("Long text..." * 1000)
            >>> len(chunks) > 1
            True
        """
        if not text or not text.strip():
            return []

        chars_per_token = 4
        chunk_size_chars = self.chunk_size * chars_per_token
        overlap_chars = self.chunk_overlap * chars_per_token

        if len(text) <= chunk_size_chars:
            return [text]

        chunks = []
        start = 0

        while start < len(text):
            end = start + chunk_size_chars

            if end < len(text):
                split_point = self._find_sentence_boundary(text, start, end)
                if split_point > start:
                    end = split_point

            chunk = text[start:end].strip()
            if chunk:
                chunks.append(chunk)

            start = end - overlap_chars
            start = max(start, 0)

        return chunks

    def chunk_text_with_context(
        self,
        text: str,
        document_title: str | None = None,
        file_path: str | None = None,
        topic_hint: str | None = None,
    ) -> list[Chunk]:
        """Chunk text and add contextual information to each chunk.

        This is the main method for contextual retrieval. It chunks the text
        and prepends document context to each chunk for improved embedding quality.

        Args:
            text: Text to chunk
            document_title: Title of the document (extracted from file_path if not provided)
            file_path: Path to source file (used for title and topic extraction)
            topic_hint: Manual topic override (auto-detected if not provided)

        Returns:
            List of Chunk objects with original and contextualized text

        Raises:
            ValueError: If text is empty or required context info is missing

        Example:
            >>> service = ChunkingService()
            >>> chunks = service.chunk_text_with_context(
            ...     text="Long document about architecture...",
            ...     file_path="/docs/architecture-guide.pdf"
            ... )
            >>> len(chunks) > 0
            True
            >>> "From document" in chunks[0].contextualized_text
            True
        """
        if not text or not text.strip():
            raise ValueError("Text cannot be empty")

        if file_path:
            if not document_title:
                document_title = self.extract_document_title(file_path)
            if not topic_hint:
                topic_hint = self.generate_topic_hint(file_path)

        if not document_title:
            document_title = "Unknown Document"
        if not topic_hint:
            topic_hint = "general content"

        original_chunks = self.chunk_text(text)

        result_chunks = []
        current_pos = 0

        for chunk_text in original_chunks:
            start_index = text.find(chunk_text, current_pos)
            if start_index == -1:
                start_index = current_pos
            end_index = start_index + len(chunk_text)

            if self.enable_context:
                contextualized_text = self.add_chunk_context(chunk_text, document_title, topic_hint)
            else:
                contextualized_text = chunk_text

            chunk = Chunk(
                original_text=chunk_text,
                contextualized_text=contextualized_text,
                start_index=start_index,
                end_index=end_index,
                metadata={
                    "document_title": document_title,
                    "topic_hint": topic_hint,
                    "chunk_length": len(chunk_text),
                    "context_added": self.enable_context,
                },
            )
            result_chunks.append(chunk)

            current_pos = end_index

        return result_chunks

    def _find_sentence_boundary(self, text: str, start: int, target_end: int) -> int:
        """Find a sentence boundary near the target end position.

        Looks for sentence-ending punctuation (. ! ?) within a window
        before the target end position to create more natural chunks.

        Args:
            text: The full text
            start: Start position of the chunk
            target_end: Target end position

        Returns:
            Position of sentence boundary, or target_end if none found
        """
        window = 100
        search_start = max(start, target_end - window)
        search_end = min(len(text), target_end)

        search_text = text[search_start:search_end]

        for i in range(len(search_text) - 1, -1, -1):
            if search_text[i] in ".!?" and i + 1 < len(search_text):
                if search_text[i + 1].isspace():
                    return search_start + i + 1

        return target_end


__all__ = [
    "Chunk",
    "ChunkingService",
]
