"""Utility modules for the Vault backend."""

from .security import SecurityError, validate_safe_path

__all__ = ["SecurityError", "validate_safe_path"]
