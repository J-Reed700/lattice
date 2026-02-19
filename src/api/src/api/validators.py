from __future__ import annotations

import logging
import os

from fastapi import HTTPException, UploadFile

logger = logging.getLogger(__name__)

MAX_FILE_SIZE = 100 * 1024 * 1024

ALLOWED_MIME_TYPES = {
    "application/pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/msword",
    "application/vnd.ms-excel",
    "application/vnd.ms-powerpoint",
    "text/plain",
    "text/html",
    "text/markdown",
    "text/csv",
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/svg+xml",
    "application/json",
    "application/xml",
    "text/xml",
}


async def validate_upload_file(file: UploadFile) -> UploadFile:
    if not file.filename:
        raise HTTPException(status_code=400, detail="Filename is required")

    content = await file.read()
    file_size = len(content)

    if file_size == 0:
        raise HTTPException(status_code=400, detail="File is empty")

    if file_size > MAX_FILE_SIZE:
        raise HTTPException(
            status_code=413,
            detail=f"File too large. Maximum size is {MAX_FILE_SIZE // (1024 * 1024)}MB",
        )

    if file.content_type not in ALLOWED_MIME_TYPES:
        logger.warning(f"Rejected file upload with unsupported type: {file.content_type}")
        raise HTTPException(
            status_code=415, detail=f"File type '{file.content_type}' not supported"
        )

    await file.seek(0)

    logger.info(f"File validation passed: {file.filename} ({file_size} bytes, {file.content_type})")

    return file


def validate_filename(filename: str) -> str:
    """Validate filename for security.

    Protects against:
    - Path traversal attacks (.., ~)
    - NULL byte injection
    - Control characters
    - Absolute paths
    - Path separators
    - Windows reserved names

    Args:
        filename: The filename to validate

    Returns:
        The validated filename

    Raises:
        HTTPException: If the filename is invalid or contains dangerous patterns
    """
    if not filename or not filename.strip():
        raise HTTPException(status_code=400, detail="Filename is required")

    # Check for leading/trailing whitespace
    if filename.strip() != filename:
        raise HTTPException(status_code=400, detail="Filename has invalid whitespace")

    if len(filename) > 255:
        raise HTTPException(status_code=400, detail="Filename too long (max 255 characters)")

    # Check for path traversal patterns
    if ".." in filename or "~" in filename:
        raise HTTPException(status_code=400, detail="Invalid filename: path traversal detected")

    # Check for path separators
    if "/" in filename or "\\" in filename:
        raise HTTPException(status_code=400, detail="Invalid filename: path separators not allowed")

    # Check for NULL bytes and control characters
    if "\x00" in filename:
        raise HTTPException(
            status_code=400, detail="Invalid characters in filename: null byte detected"
        )

    for char in filename:
        if ord(char) < 32:
            raise HTTPException(
                status_code=400, detail="Invalid characters in filename: control character detected"
            )

    # Verify no absolute path
    if os.path.isabs(filename):
        raise HTTPException(status_code=400, detail="Absolute paths not allowed")

    # Block Windows reserved names
    reserved_names = [
        "CON",
        "PRN",
        "AUX",
        "NUL",
        "COM1",
        "COM2",
        "COM3",
        "COM4",
        "COM5",
        "COM6",
        "COM7",
        "COM8",
        "COM9",
        "LPT1",
        "LPT2",
        "LPT3",
        "LPT4",
        "LPT5",
        "LPT6",
        "LPT7",
        "LPT8",
        "LPT9",
    ]
    name_without_ext = filename.split(".")[0].upper()
    if name_without_ext in reserved_names:
        raise HTTPException(status_code=400, detail="Invalid filename: uses reserved Windows name")

    return filename
