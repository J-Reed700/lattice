"""Embedding dimension validation to prevent sync crashes."""

from .types import TEXT_EMBEDDING_DIM, TEXT_MODEL_NAME


class DimensionMismatchError(Exception):
    """Raised when embedding dimensions don't match between systems."""


def validate_embedding_dimension(embedding, context: str = ""):
    """Validate that an embedding has the correct dimension.

    Args:
        embedding: NumPy array or list to validate
        context: Additional context for error message

    Raises:
        DimensionMismatchError: If dimension doesn't match TEXT_EMBEDDING_DIM
    """
    try:
        actual_dim = len(embedding) if hasattr(embedding, "__len__") else embedding.shape[0]
    except (AttributeError, IndexError):
        raise DimensionMismatchError(f"Invalid embedding format: {type(embedding)}")

    if actual_dim != TEXT_EMBEDDING_DIM:
        error_msg = (
            f"Embedding dimension mismatch{': ' + context if context else ''}. "
            f"Expected {TEXT_EMBEDDING_DIM} dimensions (model: {TEXT_MODEL_NAME}), "
            f"but got {actual_dim} dimensions. "
            f"This indicates a configuration mismatch between desktop and backend."
        )
        raise DimensionMismatchError(error_msg)


def validate_embedding_compatibility(desktop_dim: int, backend_dim: int):
    """Validate that desktop and backend embedding dimensions match.

    Args:
        desktop_dim: Embedding dimension from desktop system
        backend_dim: Embedding dimension from backend system

    Raises:
        DimensionMismatchError: If dimensions don't match
    """
    if desktop_dim != backend_dim:
        raise DimensionMismatchError(
            f"CRITICAL: Embedding dimension mismatch detected!\n"
            f"Desktop uses {desktop_dim}-dimensional embeddings.\n"
            f"Backend uses {backend_dim}-dimensional embeddings.\n"
            f"Sync operations will crash until this is fixed.\n"
            f"Both systems must use the same embedding model and dimension.\n"
            f"Current expected: {TEXT_EMBEDDING_DIM} dimensions with model {TEXT_MODEL_NAME}"
        )


def get_expected_dimension() -> int:
    """Get the expected embedding dimension for this system."""
    return TEXT_EMBEDDING_DIM


def get_expected_model() -> str:
    """Get the expected embedding model name for this system."""
    return TEXT_MODEL_NAME
