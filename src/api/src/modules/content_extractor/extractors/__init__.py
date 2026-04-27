from collections.abc import Callable
import fnmatch
from typing import Dict, Optional

from ..types import ExtractedContent, UnsupportedFileTypeError

EXTRACTORS: dict[str, Callable[[str], ExtractedContent]] = {}


def register_extractor(*mime_types: str):
    """Decorator to register an extractor function for specific MIME types.

    Supports exact matching and wildcards (e.g., 'text/*').

    Args:
        *mime_types: One or more MIME type patterns to register

    Returns:
        Decorator function

    Example:
        >>> @register_extractor('text/plain', 'text/markdown')
        ... def extract_text(file_path: str) -> ExtractedContent:
        ...     pass
        >>>
        >>> @register_extractor('text/*')
        ... def extract_any_text(file_path: str) -> ExtractedContent:
        ...     pass
    """

    def decorator(func: Callable[[str], ExtractedContent]):
        for mime_type in mime_types:
            EXTRACTORS[mime_type] = func
        return func

    return decorator


def get_extractor(mime_type: str) -> Callable[[str], ExtractedContent] | None:
    """Get the appropriate extractor function for a MIME type.

    First tries exact match, then tries wildcard patterns.

    Args:
        mime_type: MIME type to find extractor for

    Returns:
        Extractor function or None if not found

    Raises:
        UnsupportedFileTypeError: If no extractor found for MIME type

    Example:
        >>> extractor = get_extractor('text/plain')
        >>> content = extractor('/path/to/file.txt')
    """
    if mime_type in EXTRACTORS:
        return EXTRACTORS[mime_type]

    for pattern, extractor in EXTRACTORS.items():
        if "*" in pattern and fnmatch.fnmatch(mime_type, pattern):
            return extractor

    raise UnsupportedFileTypeError(f"No extractor available for MIME type: {mime_type}")


from . import document, image, pdf, text
