"""Security utilities for file path validation and protection against path traversal attacks."""

import os
from pathlib import Path


class SecurityError(Exception):
    """Raised when security validation fails."""


def validate_safe_path(
    file_path: str | Path,
    base_dir: str | Path,
    allow_symlinks: bool = False,
    must_exist: bool = False,
) -> Path:
    """
    Validate that file_path is within base_dir to prevent path traversal attacks.

    This function resolves both paths to their absolute canonical forms and verifies
    that the target path is contained within the base directory. This prevents
    directory traversal attacks using patterns like "../../../etc/passwd".

    Args:
        file_path: Path to validate (relative or absolute)
        base_dir: Base directory that must contain file_path
        allow_symlinks: Whether to allow symbolic links (default: False for security)
        must_exist: Whether the path must exist (default: False)

    Returns:
        Resolved absolute path if validation passes

    Raises:
        SecurityError: If path is outside base_dir, is a symlink when not allowed,
                      or doesn't exist when must_exist=True

    Examples:
        >>> # Safe path
        >>> validate_safe_path("documents/file.pdf", "/app/storage")
        Path('/app/storage/documents/file.pdf')

        >>> # Path traversal attempt - BLOCKED
        >>> validate_safe_path("../../etc/passwd", "/app/storage")
        SecurityError: Path traversal detected: ../../etc/passwd

        >>> # Symlink - BLOCKED by default
        >>> validate_safe_path("link_to_root", "/app/storage", allow_symlinks=False)
        SecurityError: Symbolic links not allowed: /app/storage/link_to_root
    """
    try:
        base = Path(base_dir).resolve()
    except (OSError, RuntimeError) as e:
        raise SecurityError(f"Invalid base directory: {base_dir} - {e}")

    try:
        # Convert to Path and resolve to absolute canonical form
        target = Path(file_path)

        # If relative, make it relative to base_dir
        if not target.is_absolute():
            target = base / target

        # Resolve to canonical absolute path (follows symlinks)
        target = target.resolve()

    except (OSError, RuntimeError) as e:
        raise SecurityError(f"Invalid file path: {file_path} - {e}")

    # Check if target is within base directory
    try:
        target.relative_to(base)
    except ValueError:
        raise SecurityError(
            f"Path traversal detected: '{file_path}' resolves outside base directory '{base_dir}'"
        )

    # Check for symlinks if not allowed
    if not allow_symlinks:
        # Check if any component in the path is a symlink
        try:
            current = Path(file_path)
            if not current.is_absolute():
                current = base / current

            # Walk up the path checking each component
            parts_to_check = []
            check_path = current
            while check_path != base and check_path.parent != check_path:
                parts_to_check.append(check_path)
                check_path = check_path.parent

            for path in parts_to_check:
                if path.is_symlink():
                    raise SecurityError(
                        f"Symbolic links not allowed: '{file_path}' contains symlink at '{path}'"
                    )
        except OSError:
            # If path doesn't exist, we can't check for symlinks
            # This is OK if must_exist=False
            pass

    # Check existence if required
    if must_exist and not target.exists():
        raise SecurityError(f"Path does not exist: {file_path}")

    return target


def validate_filename(filename: str, allow_path_separators: bool = False) -> str:
    r"""
    Validate that a filename is safe and doesn't contain path traversal attempts.

    Args:
        filename: Filename to validate
        allow_path_separators: Whether to allow path separators like / or \ (default: False)

    Returns:
        The filename if validation passes

    Raises:
        SecurityError: If filename contains dangerous characters

    Examples:
        >>> validate_filename("document.pdf")
        'document.pdf'

        >>> validate_filename("../../../etc/passwd")
        SecurityError: Filename contains path traversal: ../../../etc/passwd
    """
    if not filename or not filename.strip():
        raise SecurityError(f"Invalid filename: '{filename}' (empty or whitespace-only)")

    # Check for leading/trailing whitespace
    if filename.strip() != filename:
        raise SecurityError(f"Invalid filename: '{filename}' (has leading/trailing whitespace)")

    # Block null bytes
    if "\x00" in filename:
        raise SecurityError(f"Filename contains null byte: {filename}")

    # Block control characters (ASCII < 32)
    for char in filename:
        if ord(char) < 32:
            raise SecurityError(f"Filename contains control character: {filename}")

    # Block path traversal patterns
    dangerous_patterns = ["..", "~"]
    for pattern in dangerous_patterns:
        if pattern in filename:
            raise SecurityError(f"Filename contains path traversal pattern '{pattern}': {filename}")

    # Block path separators unless explicitly allowed
    if not allow_path_separators:
        if "/" in filename or "\\" in filename:
            raise SecurityError(f"Filename contains path separator: {filename}")

    # Block absolute paths
    if os.path.isabs(filename):
        raise SecurityError(f"Absolute paths not allowed: {filename}")

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
        raise SecurityError(f"Filename uses reserved Windows name: {filename}")

    return filename
