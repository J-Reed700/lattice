"""Result dataclasses for VectorStore module.

This module defines result types returned by vector store operations.
All results use frozen dataclasses for immutability.
"""

from dataclasses import dataclass
from datetime import datetime
from uuid import UUID


@dataclass(frozen=True)
class TextSearchResult:
    """Result from text similarity search.

    Attributes:
        text_content_id: UUID of the text content record
        file_id: UUID of the associated file
        file_path: Path to the file
        filename: Name of the file
        content: The text content
        content_length: Length of the content in characters
        language: Detected language (optional)
        similarity: Cosine similarity score (0-1, higher is better)
        created_at: When the content was created
    """

    text_content_id: UUID
    file_id: UUID
    file_path: str
    filename: str
    content: str
    content_length: int
    language: str | None
    similarity: float
    created_at: datetime


@dataclass(frozen=True)
class ImageSearchResult:
    """Result from image similarity search.

    Attributes:
        image_id: UUID of the image record
        file_id: UUID of the associated file
        file_path: Path to the image file
        filename: Name of the file
        width: Image width in pixels
        height: Image height in pixels
        format: Image format (e.g., 'JPEG', 'PNG')
        color_mode: Color mode (optional)
        thumbnail_path: Path to thumbnail (optional)
        similarity: Cosine similarity score (0-1, higher is better)
        created_at: When the image was indexed
    """

    image_id: UUID
    file_id: UUID
    file_path: str
    filename: str
    width: int
    height: int
    format: str
    color_mode: str | None
    thumbnail_path: str | None
    similarity: float
    created_at: datetime


@dataclass(frozen=True)
class EmbeddingStats:
    """Statistics about stored embeddings.

    Attributes:
        total_text_embeddings: Count of text embeddings
        total_image_embeddings: Count of image embeddings
        text_models: Dictionary mapping model names to count
        image_models: Dictionary mapping model names to count
        oldest_text_embedding: Timestamp of oldest text embedding (optional)
        newest_text_embedding: Timestamp of newest text embedding (optional)
        oldest_image_embedding: Timestamp of oldest image embedding (optional)
        newest_image_embedding: Timestamp of newest image embedding (optional)
    """

    total_text_embeddings: int
    total_image_embeddings: int
    text_models: dict[str, int]
    image_models: dict[str, int]
    oldest_text_embedding: datetime | None
    newest_text_embedding: datetime | None
    oldest_image_embedding: datetime | None
    newest_image_embedding: datetime | None
