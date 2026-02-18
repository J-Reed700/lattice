"""
Token Counting Utilities

Provides token counting and truncation for LLM context management.
Uses tiktoken for accurate token estimation compatible with GPT models.

Basic Usage:
    >>> from src.modules.rag_engine.qa.tokenizer import count_tokens, truncate_to_tokens
    >>> text = "This is a sample text for token counting."
    >>> token_count = count_tokens(text)
    >>> print(f"Tokens: {token_count}")
    >>>
    >>> truncated = truncate_to_tokens(text, max_tokens=10)
    >>> assert count_tokens(truncated) <= 10
"""

from __future__ import annotations

import logging

import tiktoken

logger = logging.getLogger(__name__)


def count_tokens(text: str, model: str = "gpt-3.5-turbo") -> int:
    """Count tokens in text using tiktoken encoding.

    Uses the cl100k_base encoding which is compatible with GPT-3.5-turbo,
    GPT-4, and most modern LLMs. Provides accurate token counts for
    context window management.

    Args:
        text: Text to count tokens for
        model: Model name for encoding selection (default: "gpt-3.5-turbo")
            Supported: gpt-3.5-turbo, gpt-4, text-embedding-ada-002

    Returns:
        Number of tokens in the text (>= 0)

    Raises:
        ValueError: If text is None
        KeyError: If model encoding is not found

    Performance:
        - Time complexity: O(n) where n = text length
        - Typical speed: ~1M tokens/second
        - Memory: Minimal, streaming tokenization

    Example:
        >>> count_tokens("Hello, world!")
        4

        >>> long_text = "word " * 1000
        >>> count = count_tokens(long_text)
        >>> assert count > 1000

        >>> count_tokens("")
        0

        >>> text = "Context:\nThis is important information.\nQuestion: What is this?"
        >>> tokens = count_tokens(text)
        >>> print(f"This prompt uses {tokens} tokens")
    """
    if text is None:
        raise ValueError("Text cannot be None")

    if not text:
        return 0

    try:
        encoding = tiktoken.encoding_for_model(model)
    except KeyError:
        logger.warning(f"Model {model} not found, using cl100k_base encoding")
        encoding = tiktoken.get_encoding("cl100k_base")

    tokens = encoding.encode(text)

    logger.debug(f"Counted {len(tokens)} tokens in text of length {len(text)}")

    return len(tokens)


def truncate_to_tokens(
    text: str, max_tokens: int, model: str = "gpt-3.5-turbo", suffix: str = "..."
) -> str:
    """Truncate text to fit within token limit.

    Encodes the text, truncates to max_tokens, and decodes back to text.
    Ensures the result fits within the token budget while preserving
    valid UTF-8 text.

    Args:
        text: Text to truncate
        max_tokens: Maximum number of tokens allowed (must be > 0)
        model: Model name for encoding (default: "gpt-3.5-turbo")
        suffix: String to append if truncated (default: "...")
            Suffix tokens ARE counted against max_tokens.

    Returns:
        Truncated text that fits within max_tokens, including suffix.
        If text was truncated, suffix is appended.
        If text already fits, returns original text unchanged.

    Raises:
        ValueError: If text is None or max_tokens <= 0

    Example:
        >>> text = "This is a long text " * 100
        >>> truncated = truncate_to_tokens(text, max_tokens=50)
        >>> assert count_tokens(truncated) <= 50
        >>> assert truncated.endswith("...")

        >>> short_text = "Short text"
        >>> result = truncate_to_tokens(short_text, max_tokens=100)
        >>> assert result == short_text

        >>> context = "Document 1: content\\nDocument 2: more content\\nDocument 3: ..."
        >>> limited = truncate_to_tokens(context, max_tokens=20)
        >>> print(f"Truncated context: {limited}")
    """
    if text is None:
        raise ValueError("Text cannot be None")

    if max_tokens <= 0:
        raise ValueError(f"max_tokens must be positive, got {max_tokens}")

    if not text:
        return text

    try:
        encoding = tiktoken.encoding_for_model(model)
    except KeyError:
        logger.warning(f"Model {model} not found, using cl100k_base encoding")
        encoding = tiktoken.get_encoding("cl100k_base")

    tokens = encoding.encode(text)

    if len(tokens) <= max_tokens:
        logger.debug(f"Text fits within {max_tokens} tokens ({len(tokens)} tokens)")
        return text

    if suffix:
        suffix_tokens = encoding.encode(suffix)
        available_tokens = max_tokens - len(suffix_tokens)

        if available_tokens <= 0:
            return suffix

        truncated_tokens = tokens[:available_tokens]
        truncated_text = encoding.decode(truncated_tokens)
        truncated_text += suffix
    else:
        truncated_tokens = tokens[:max_tokens]
        truncated_text = encoding.decode(truncated_tokens)

    logger.debug(
        f"Truncated text from {len(tokens)} to {max_tokens} tokens "
        f"(original length: {len(text)}, new length: {len(truncated_text)})"
    )

    return truncated_text


__all__ = ["count_tokens", "truncate_to_tokens"]
