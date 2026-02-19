from datetime import datetime
import json

from ..detector import check_file_size, detect_mime_type
from ..types import CorruptedFileError, ExtractedContent, ExtractionError
from ..utils import detect_encoding, detect_language, normalize_text
from . import register_extractor


@register_extractor("text/*", "text/plain", "text/markdown")
def extract_text(file_path: str) -> ExtractedContent:
    """Extract content from plain text files.

    Handles all text/* MIME types including plain, markdown, csv, etc.
    Automatically detects encoding and normalizes whitespace.

    Args:
        file_path: Path to the text file

    Returns:
        ExtractedContent with extracted text and metadata

    Raises:
        ExtractionError: If file cannot be read
        FileSizeLimitError: If file exceeds size limit

    Example:
        >>> content = extract_text('/path/to/document.txt')
        >>> assert content.mime_type.startswith('text/')
        >>> assert content.encoding in ['utf-8', 'iso-8859-1']
    """
    try:
        mime_type = detect_mime_type(file_path)
        file_size = check_file_size(file_path, mime_type)

        encoding = detect_encoding(file_path)

        try:
            with open(file_path, encoding=encoding) as f:
                text = f.read()
        except UnicodeDecodeError:
            with open(file_path, encoding="utf-8", errors="replace") as f:
                text = f.read()
            encoding = "utf-8"

        text = normalize_text(text)
        language = detect_language(text)

        return ExtractedContent(
            text=text,
            mime_type=mime_type,
            file_path=file_path,
            metadata={},
            encoding=encoding,
            language=language,
            extraction_time=datetime.now(),
            file_size=file_size,
        )

    except OSError as e:
        raise ExtractionError(f"Failed to read text file: {e!s}", file_path=file_path) from e


@register_extractor("application/json")
def extract_json(file_path: str) -> ExtractedContent:
    """Extract content from JSON files.

    Parses JSON and converts to pretty-printed text.
    Validates JSON structure.

    Args:
        file_path: Path to the JSON file

    Returns:
        ExtractedContent with formatted JSON text

    Raises:
        CorruptedFileError: If JSON is malformed
        ExtractionError: If file cannot be read
        FileSizeLimitError: If file exceeds size limit

    Example:
        >>> content = extract_json('/path/to/data.json')
        >>> assert content.mime_type == 'application/json'
        >>> assert 'keys' in content.metadata
    """
    try:
        mime_type = detect_mime_type(file_path)
        file_size = check_file_size(file_path, mime_type)

        encoding = detect_encoding(file_path)

        with open(file_path, encoding=encoding) as f:
            try:
                data = json.load(f)
            except json.JSONDecodeError as e:
                raise CorruptedFileError(f"Invalid JSON format: {e!s}", file_path=file_path) from e

        text = json.dumps(data, indent=2, ensure_ascii=False)

        metadata = {}
        if isinstance(data, dict):
            metadata["keys"] = list(data.keys())
            metadata["key_count"] = len(data.keys())
        elif isinstance(data, list):
            metadata["item_count"] = len(data)

        return ExtractedContent(
            text=text,
            mime_type=mime_type,
            file_path=file_path,
            metadata=metadata,
            encoding=encoding,
            language=None,
            extraction_time=datetime.now(),
            file_size=file_size,
        )

    except OSError as e:
        raise ExtractionError(f"Failed to read JSON file: {e!s}", file_path=file_path) from e
