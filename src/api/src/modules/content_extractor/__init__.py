"""
Module: ContentExtractor

A self-contained module for extracting text content from various file types.

Supports:
- Text files (plain text, markdown, JSON)
- PDF documents
- Microsoft Office documents (DOCX, PPTX)
- OpenDocument formats (ODT)
- Image files (JPEG, PNG, GIF, BMP, WebP)

Basic Usage:
    >>> from content_extractor import extract_content
    >>> content = extract_content('/path/to/document.pdf')
    >>> print(content.text)
    >>> print(content.metadata)

Error Handling:
    >>> try:
    ...     content = extract_content('/path/to/file.xyz')
    ... except UnsupportedFileTypeError:
    ...     print("File type not supported")
    ... except FileSizeLimitError:
    ...     print("File too large")
    ... except ExtractionError as e:
    ...     print(f"Extraction failed: {e}")
"""

from .detector import check_file_size, detect_mime_type
from .extractors import get_extractor
from .types import (
    CorruptedFileError,
    ExtractedContent,
    ExtractionError,
    FileSizeLimitError,
    PasswordProtectedError,
    UnsupportedFileTypeError,
)

__all__ = [
    "CorruptedFileError",
    "ExtractedContent",
    "ExtractionError",
    "FileSizeLimitError",
    "PasswordProtectedError",
    "UnsupportedFileTypeError",
    "extract_content",
]


def extract_content(file_path: str) -> ExtractedContent:
    """Extract text content from a file.

    This is the main entry point for the ContentExtractor module.
    Automatically detects file type and uses appropriate extractor.

    Args:
        file_path: Path to the file to extract content from

    Returns:
        ExtractedContent object with text, metadata, and file information

    Raises:
        UnsupportedFileTypeError: File type not supported
        FileSizeLimitError: File exceeds size limits
        PasswordProtectedError: File is password protected
        CorruptedFileError: File is corrupted or malformed
        ExtractionError: General extraction failure
        FileNotFoundError: File does not exist

    Examples:
        Basic text file:
        >>> content = extract_content('document.txt')
        >>> print(content.text)
        >>> print(f"Encoding: {content.encoding}")

        PDF with metadata:
        >>> content = extract_content('report.pdf')
        >>> print(f"Pages: {content.metadata['page_count']}")
        >>> print(f"Author: {content.metadata.get('author', 'Unknown')}")

        JSON file:
        >>> content = extract_content('data.json')
        >>> print(f"Keys: {content.metadata.get('keys', [])}")

        Image file:
        >>> content = extract_content('photo.jpg')
        >>> print(f"Size: {content.metadata['width']}x{content.metadata['height']}")
        >>> print(f"EXIF: {content.metadata.get('exif', {})}")

        Error handling:
        >>> try:
        ...     content = extract_content('protected.pdf')
        ... except PasswordProtectedError:
        ...     print("Cannot extract from password-protected PDF")
        ... except FileSizeLimitError as e:
        ...     print(f"File too large: {e}")
        ... except UnsupportedFileTypeError as e:
        ...     print(f"Unsupported type: {e}")
    """
    mime_type = detect_mime_type(file_path)

    extractor = get_extractor(mime_type)

    return extractor(file_path)
