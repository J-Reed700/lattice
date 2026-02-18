import mimetypes
import os
from pathlib import Path

from .types import FileSizeLimitError

MAX_FILE_SIZE = 100 * 1024 * 1024
MAX_PDF_SIZE = 500 * 1024 * 1024

EXTENSION_MIME_MAP = {
    ".txt": "text/plain",
    ".md": "text/markdown",
    ".json": "application/json",
    ".pdf": "application/pdf",
    ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ".pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ".odt": "application/vnd.oasis.opendocument.text",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".png": "image/png",
    ".gif": "image/gif",
    ".bmp": "image/bmp",
    ".webp": "image/webp",
}


def detect_mime_type(file_path: str) -> str:
    """Detect MIME type of a file.

    Uses python-magic if available, falls back to mimetypes stdlib,
    and finally extension mapping as last resort.

    Args:
        file_path: Path to the file

    Returns:
        MIME type string (e.g., 'text/plain', 'application/pdf')

    Example:
        >>> mime_type = detect_mime_type('/path/to/document.pdf')
        >>> assert mime_type == 'application/pdf'
    """
    try:
        import magic

        mime = magic.Magic(mime=True)
        return mime.from_file(file_path)
    except (ImportError, AttributeError):
        pass

    mime_type, _ = mimetypes.guess_type(file_path)
    if mime_type:
        return mime_type

    ext = Path(file_path).suffix.lower()
    return EXTENSION_MIME_MAP.get(ext, "application/octet-stream")


def check_file_size(file_path: str, mime_type: str | None = None) -> int:
    """Validate file size is within limits.

    Args:
        file_path: Path to the file
        mime_type: MIME type for specific limits (PDFs get 500MB, others 100MB)

    Returns:
        File size in bytes

    Raises:
        FileSizeLimitError: If file exceeds size limits
        FileNotFoundError: If file does not exist

    Example:
        >>> size = check_file_size('/path/to/file.txt')
        >>> assert size < MAX_FILE_SIZE
    """
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"File not found: {file_path}")

    file_size = os.path.getsize(file_path)

    max_size = MAX_PDF_SIZE if mime_type == "application/pdf" else MAX_FILE_SIZE

    if file_size > max_size:
        max_mb = max_size / (1024 * 1024)
        actual_mb = file_size / (1024 * 1024)
        raise FileSizeLimitError(
            f"File size {actual_mb:.1f}MB exceeds limit of {max_mb:.1f}MB", file_path=file_path
        )

    return file_size
