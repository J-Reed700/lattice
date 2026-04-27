"""
Prompt templates for different summarization types.

This module contains optimized prompts for various summary formats.
"""

from .types import SummaryType


class PromptBuilder:
    """Builds optimized prompts for different summarization types.

    This class generates prompts tailored for each summary type,
    ensuring consistent output format and quality.

    Example:
        >>> builder = PromptBuilder()
        >>> prompt = builder.build_prompt(
        ...     text="Long document...",
        ...     summary_type=SummaryType.BULLET_POINTS,
        ...     max_words=100
        ... )
    """

    # System prompts for different summary types
    SYSTEM_PROMPTS = {
        SummaryType.EXTRACTIVE: """You are a precise document summarizer. Extract the most important sentences and paragraphs directly from the text without modification. Select key sentences that capture the essential information.""",
        SummaryType.ABSTRACTIVE: """You are a skilled summarizer. Create a concise, natural language summary that captures the main points in your own words. Focus on clarity and completeness.""",
        SummaryType.BULLET_POINTS: """You are a summarizer that creates bullet point lists. Extract the key points and present them as a clean, organized bullet list. Each bullet should be concise and informative.""",
        SummaryType.TLDR: """You are a concise summarizer. Create a very brief "TL;DR" (Too Long; Didn't Read) summary in 1-2 sentences that captures the absolute core message.""",
        SummaryType.DETAILED: """You are a comprehensive summarizer. Create a detailed, multi-paragraph summary that thoroughly covers all important aspects while remaining more concise than the original.""",
        SummaryType.CUSTOM: """You are a flexible summarizer. Create a summary that captures the key information in a natural, readable format.""",
    }

    # User prompt templates
    USER_PROMPTS = {
        SummaryType.EXTRACTIVE: """Extract the most important sentences from the following text. Select {max_words} words of key content directly from the text:

{text}

Important sentences:""",
        SummaryType.ABSTRACTIVE: """Summarize the following text in approximately {max_words} words. Create a natural, flowing summary in your own words:

{text}

Summary:""",
        SummaryType.BULLET_POINTS: """Create a bullet point summary of the following text. Use approximately {max_words} words total across all bullet points:

{text}

Key Points:
•""",
        SummaryType.TLDR: """Create a TL;DR (1-2 sentence) summary of the following text:

{text}

TL;DR:""",
        SummaryType.DETAILED: """Create a detailed, comprehensive summary of the following text in approximately {max_words} words. Cover all major points thoroughly:

{text}

Detailed Summary:""",
        SummaryType.CUSTOM: """Summarize the following text in approximately {max_words} words:

{text}

Summary:""",
    }

    @staticmethod
    def build_prompt(
        text: str, summary_type: SummaryType, max_words: int = 150, language: str = "en"
    ) -> tuple[str, str]:
        """Build system and user prompts for summarization.

        Args:
            text: Text to summarize
            summary_type: Type of summary to generate
            max_words: Target word count
            language: Language code for multilingual models

        Returns:
            Tuple of (system_prompt, user_prompt)

        Example:
            >>> system, user = PromptBuilder.build_prompt(
            ...     text="Long document...",
            ...     summary_type=SummaryType.BULLET_POINTS,
            ...     max_words=100
            ... )
        """
        system_prompt = PromptBuilder.SYSTEM_PROMPTS[summary_type]
        user_template = PromptBuilder.USER_PROMPTS[summary_type]

        # Add language instruction for non-English
        if language and language != "en":
            system_prompt += f"\n\nRespond in {language}."

        # Format user prompt
        user_prompt = user_template.format(text=text.strip(), max_words=max_words)

        return system_prompt, user_prompt

    @staticmethod
    def build_chat_messages(
        text: str, summary_type: SummaryType, max_words: int = 150, language: str = "en"
    ) -> list[dict]:
        """Build chat-format messages for models that use chat templates.

        Args:
            text: Text to summarize
            summary_type: Type of summary
            max_words: Target word count
            language: Language code

        Returns:
            List of message dicts in chat format

        Example:
            >>> messages = PromptBuilder.build_chat_messages(
            ...     text="Document...",
            ...     summary_type=SummaryType.TLDR
            ... )
        """
        system_prompt, user_prompt = PromptBuilder.build_prompt(
            text, summary_type, max_words, language
        )

        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ]

    @staticmethod
    def optimize_for_context(
        text: str, max_context_length: int = 4096, reserve_output_tokens: int = 512
    ) -> str:
        """Optimize text to fit within context window.

        Args:
            text: Input text
            max_context_length: Model's max context
            reserve_output_tokens: Tokens to reserve for output

        Returns:
            Truncated text if necessary

        Example:
            >>> optimized = PromptBuilder.optimize_for_context(
            ...     text=very_long_text,
            ...     max_context_length=4096
            ... )
        """
        # Rough estimate: 1 token ≈ 4 characters
        max_chars = (max_context_length - reserve_output_tokens) * 4

        if len(text) <= max_chars:
            return text

        # Truncate at sentence boundary if possible
        truncated = text[:max_chars]
        last_period = truncated.rfind(". ")
        last_newline = truncated.rfind("\n\n")

        if last_period > max_chars * 0.8:
            return truncated[: last_period + 1]
        if last_newline > max_chars * 0.8:
            return truncated[:last_newline]
        return truncated + "..."

    @staticmethod
    def chunk_long_text(text: str, chunk_size: int = 2000, overlap: int = 200) -> list[str]:
        """Chunk very long text for incremental summarization.

        Args:
            text: Long input text
            chunk_size: Characters per chunk
            overlap: Overlap between chunks

        Returns:
            List of text chunks

        Example:
            >>> chunks = PromptBuilder.chunk_long_text(
            ...     text=very_long_document,
            ...     chunk_size=2000
            ... )
        """
        chunks = []
        start = 0

        while start < len(text):
            end = start + chunk_size

            # Try to break at paragraph boundary
            if end < len(text):
                next_paragraph = text.find("\n\n", end - 200, end + 200)
                if next_paragraph != -1:
                    end = next_paragraph

            chunk = text[start:end].strip()
            if chunk:
                chunks.append(chunk)

            start = end - overlap

        return chunks

    @staticmethod
    def validate_summary(summary: str, summary_type: SummaryType, min_words: int = 10) -> bool:
        """Validate that summary meets basic requirements.

        Args:
            summary: Generated summary
            summary_type: Expected type
            min_words: Minimum word count

        Returns:
            True if valid

        Example:
            >>> valid = PromptBuilder.validate_summary(
            ...     summary="Key points: ...",
            ...     summary_type=SummaryType.BULLET_POINTS
            ... )
        """
        if not summary or not summary.strip():
            return False

        word_count = len(summary.split())
        if word_count < min_words:
            return False

        # Type-specific validation
        if summary_type == SummaryType.BULLET_POINTS:
            # Should contain bullet markers
            has_bullets = any(marker in summary for marker in ["•", "-", "*", "1.", "2."])
            if not has_bullets:
                return False

        elif summary_type == SummaryType.TLDR:
            # Should be very short (< 50 words typically)
            if word_count > 100:
                return False

        return True

    @staticmethod
    def extract_summary_from_response(response: str, summary_type: SummaryType) -> str:
        """Extract clean summary from model response.

        Args:
            response: Raw model output
            summary_type: Type of summary

        Returns:
            Cleaned summary text

        Example:
            >>> clean = PromptBuilder.extract_summary_from_response(
            ...     response="Summary: The key points are...",
            ...     summary_type=SummaryType.ABSTRACTIVE
            ... )
        """
        # Remove common prefixes
        prefixes = [
            "Summary:",
            "TL;DR:",
            "Key Points:",
            "Important sentences:",
            "Detailed Summary:",
            "Here is a summary:",
            "Here are the key points:",
        ]

        result = response.strip()
        for prefix in prefixes:
            if result.startswith(prefix):
                result = result[len(prefix) :].strip()

        return result
