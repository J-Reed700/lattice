"""
Snippet Generator

Key Word In Context (KWIC) snippet extraction with term highlighting.
"""

import re

SNIPPET_LENGTH = 200
CONTEXT_CHARS = 80


class SnippetGenerator:
    """Generate text snippets with highlighted query terms.

    Example:
        >>> text = "The quick brown fox jumps over the lazy dog"
        >>> query = "brown fox"
        >>> snippet = SnippetGenerator.generate_kwic(text, query)
        >>> print(snippet)
        "...quick **brown** **fox** jumps over..."
    """

    @staticmethod
    def generate_kwic(
        text: str,
        query: str,
        snippet_length: int = SNIPPET_LENGTH,
        context_chars: int = CONTEXT_CHARS,
    ) -> str:
        """Generate Key Word In Context snippet.

        Finds first query term match and extracts surrounding context.

        Args:
            text: Full text content
            query: Search query string
            snippet_length: Maximum snippet length
            context_chars: Characters before/after match

        Returns:
            Text excerpt with query terms highlighted

        Example:
            >>> SnippetGenerator.generate_kwic(
            ...     "This is a long document with many words",
            ...     "document"
            ... )
            "...long **document** with many..."
        """
        if not text or not query:
            return ""

        terms = SnippetGenerator.extract_query_terms(query)
        if not terms:
            return text[:snippet_length]

        text_lower = text.lower()

        first_match_pos = None
        for term in terms:
            pos = text_lower.find(term.lower())
            if pos != -1:
                if first_match_pos is None or pos < first_match_pos:
                    first_match_pos = pos

        if first_match_pos is None:
            return text[:snippet_length]

        start = max(0, first_match_pos - context_chars)
        end = min(len(text), first_match_pos + context_chars + max(len(t) for t in terms))

        snippet = text[start:end]

        prefix = "..." if start > 0 else ""
        suffix = "..." if end < len(text) else ""

        snippet = prefix + snippet + suffix

        snippet = SnippetGenerator.highlight_terms(snippet, terms)

        if len(snippet) > snippet_length:
            snippet = snippet[:snippet_length] + "..."

        return snippet

    @staticmethod
    def highlight_terms(text: str, terms: list[str]) -> str:
        """Wrap matching terms in highlight markers.

        Args:
            text: Text to highlight
            terms: List of terms to highlight

        Returns:
            Text with terms wrapped in **markers**

        Example:
            >>> SnippetGenerator.highlight_terms("hello world", ["world"])
            "hello **world**"
        """
        for term in terms:
            pattern = re.compile(re.escape(term), re.IGNORECASE)
            text = pattern.sub(f"**{term}**", text)

        return text

    @staticmethod
    def extract_query_terms(query: str) -> list[str]:
        """Tokenize query into searchable terms.

        Splits on whitespace and removes punctuation.

        Args:
            query: Search query string

        Returns:
            List of individual search terms

        Example:
            >>> SnippetGenerator.extract_query_terms("hello, world!")
            ["hello", "world"]
        """
        query = re.sub(r"[^\w\s]", " ", query)

        terms = query.split()

        terms = [t for t in terms if len(t) > 1]

        return terms
