"""
Main summarization service using llama.cpp for on-device inference.

This service handles text summarization using quantized SLMs with
100% local processing for privacy.
"""

import asyncio
from collections.abc import AsyncIterator
from datetime import UTC, datetime
import time
import uuid

try:
    from llama_cpp import Llama

    LLAMA_CPP_AVAILABLE = True
except ImportError:
    LLAMA_CPP_AVAILABLE = False
    Llama = None

try:
    from langdetect import detect

    LANGDETECT_AVAILABLE = True
except ImportError:
    LANGDETECT_AVAILABLE = False

from .model_manager import ModelManager, ModelNotFoundError
from .prompts import PromptBuilder
from .types import (
    BatchSummaryResponse,
    SummarizerConfig,
    Summary,
    SummaryType,
)


class SummarizationError(Exception):
    """Raised when summarization fails."""


class LanguageNotSupportedError(Exception):
    """Raised when model doesn't support the text language."""


class SummarizerService:
    """Main service for document summarization.

    This service provides on-device text summarization using SLMs.
    All processing happens locally with no external API calls.

    Example:
        >>> config = SummarizerConfig()
        >>> service = SummarizerService(config)
        >>> await service.initialize()
        >>> summary = await service.summarize(
        ...     text="Long document...",
        ...     summary_type=SummaryType.BULLET_POINTS
        ... )
    """

    def __init__(self, config: SummarizerConfig):
        """Initialize summarization service.

        Args:
            config: Service configuration

        Raises:
            ImportError: If llama-cpp-python is not installed
        """
        if not LLAMA_CPP_AVAILABLE:
            raise ImportError(
                "llama-cpp-python is required. Install with: pip install llama-cpp-python"
            )

        self.config = config
        self.model_manager = ModelManager(config)
        self.prompt_builder = PromptBuilder()
        self._model_cache: dict[str, Llama] = {}
        self._lock = asyncio.Lock()

    async def initialize(self) -> None:
        """Initialize the service and load default model.

        This should be called before using the service.
        Downloads default model if not present.

        Example:
            >>> await service.initialize()
        """
        # Ensure default model is downloaded.
        # If configured default is no longer present in the remote catalog,
        # fall back to the first discovered model.
        try:
            if not self.model_manager.is_model_downloaded(self.config.default_model):
                await self.model_manager.download_model(self.config.default_model)
        except ModelNotFoundError:
            available = self.model_manager.list_available_models()
            if not available:
                raise
            fallback = available[0].name
            self.config.default_model = fallback
            if not self.model_manager.is_model_downloaded(fallback):
                await self.model_manager.download_model(fallback)

    def _load_model(self, model_name: str) -> Llama:
        """Load model into memory.

        Args:
            model_name: Model to load

        Returns:
            Loaded Llama model

        Raises:
            ModelNotFoundError: If model not downloaded
            SummarizationError: If model loading fails
        """
        # Check cache
        if model_name in self._model_cache:
            return self._model_cache[model_name]

        # Get model path
        model_path = self.model_manager.get_model_path(model_name)
        if not model_path or not model_path.exists():
            raise ModelNotFoundError(
                f"Model {model_name} not downloaded. Download it first using download_model()"
            )

        try:
            # Load model with llama.cpp
            model = Llama(
                model_path=str(model_path),
                n_ctx=self.config.max_context_length,
                n_threads=self.config.threads,
                n_gpu_layers=self.config.gpu_layers,
                n_batch=self.config.batch_size,
                verbose=False,
            )
        except Exception as e:
            raise SummarizationError(f"Failed to load model {model_name}: {e!s}") from e
        else:
            # Cache model
            self._model_cache[model_name] = model
            return model

    def _detect_language(self, text: str) -> str:
        """Detect language of text.

        Args:
            text: Input text

        Returns:
            ISO 639-1 language code (e.g., 'en', 'es')
        """
        if not LANGDETECT_AVAILABLE:
            return "en"  # Default to English

        try:
            # Use first 500 chars for detection
            sample = text[:500]
            return detect(sample)
        except Exception:
            return "en"

    def _validate_language_support(self, model_name: str, language: str) -> None:
        """Validate that model supports the language.

        Args:
            model_name: Model to check
            language: Language code

        Raises:
            LanguageNotSupportedError: If language not supported
        """
        model_info = self.model_manager.get_model_info(model_name)

        # Skip validation for multilingual models
        if len(model_info.languages) > 5:
            return

        if language not in model_info.languages:
            raise LanguageNotSupportedError(
                f"Model {model_name} does not support language '{language}'. "
                f"Supported: {', '.join(model_info.languages)}"
            )

    def _ensure_valid_summary(self, summary_text: str, summary_type: SummaryType) -> None:
        """Validate generated summary content."""
        if not self.prompt_builder.validate_summary(summary_text, summary_type):
            raise SummarizationError("Generated summary failed validation")

    async def summarize(
        self,
        text: str,
        summary_type: SummaryType = SummaryType.ABSTRACTIVE,
        max_words: int = 150,
        model: str | None = None,
        language: str | None = None,
        stream: bool = False,
        cache_key: str | None = None,
    ) -> Summary:
        """Generate summary of text.

        Args:
            text: Input text (100-50000 chars)
            summary_type: Type of summary
            max_words: Target length in words
            model: Model to use (defaults to config.default_model)
            language: Language code (auto-detect if None)
            stream: Stream response chunks
            cache_key: Optional cache key

        Returns:
            Summary object with generated text and metadata

        Raises:
            ValueError: Invalid input
            ModelNotFoundError: Model not downloaded
            SummarizationError: Generation failed
            LanguageNotSupportedError: Language not supported

        Example:
            >>> summary = await service.summarize(
            ...     text="Long document...",
            ...     summary_type=SummaryType.BULLET_POINTS,
            ...     max_words=100
            ... )
        """
        # Validate input
        if not text or len(text.strip()) < 100:
            raise ValueError("Text must be at least 100 characters")
        if len(text) > 50000:
            raise ValueError("Text exceeds maximum length of 50000 characters")
        # Kept for API compatibility; streaming/caching are handled by dedicated paths.
        _ = (stream, cache_key)

        # Use default model if not specified
        model_name = model or self.config.default_model

        # Detect language if not provided
        detected_language = language or self._detect_language(text)

        # Validate language support
        self._validate_language_support(model_name, detected_language)

        # Optimize text for context window
        optimized_text = self.prompt_builder.optimize_for_context(
            text, self.config.max_context_length, self.config.max_tokens
        )

        # Build prompt
        _, user_prompt = self.prompt_builder.build_prompt(
            optimized_text, summary_type, max_words, detected_language
        )

        # Load model
        async with self._lock:
            llm = self._load_model(model_name)

        # Generate summary
        start_time = time.time()

        try:
            # Run inference in thread pool (llama.cpp is blocking)
            loop = asyncio.get_event_loop()
            response = await loop.run_in_executor(
                None,
                lambda: llm(
                    user_prompt,
                    max_tokens=self.config.max_tokens,
                    temperature=self.config.temperature,
                    stop=["</s>", "<|end|>", "<|endoftext|>"],
                    echo=False,
                ),
            )

            generation_time = time.time() - start_time

            # Extract summary text
            summary_text = response["choices"][0]["text"].strip()
            summary_text = self.prompt_builder.extract_summary_from_response(
                summary_text, summary_type
            )

            # Validate summary
            self._ensure_valid_summary(summary_text, summary_type)

            # Calculate metrics
            tokens_generated = response["usage"]["completion_tokens"]
            tokens_per_second = tokens_generated / generation_time if generation_time > 0 else 0

            # Create summary object
            result = Summary(
                id=str(uuid.uuid4()),
                text=summary_text,
                summary_type=summary_type,
                source_length=len(text),
                summary_length=len(summary_text),
                compression_ratio=len(text) / len(summary_text) if len(summary_text) > 0 else 0,
                model=model_name,
                language=detected_language,
                generation_time=generation_time,
                tokens_per_second=tokens_per_second,
                created_at=datetime.now(UTC),
                cache_hit=False,
                metadata={"tokens_generated": tokens_generated, "max_words_requested": max_words},
            )

        except Exception as e:
            raise SummarizationError(f"Summarization failed: {e!s}") from e
        else:
            return result

    async def batch_summarize(
        self,
        texts: list[str],
        summary_type: SummaryType = SummaryType.ABSTRACTIVE,
        max_words: int = 150,
        model: str | None = None,
        parallel: bool = True,
    ) -> BatchSummaryResponse:
        """Summarize multiple texts.

        Args:
            texts: List of texts to summarize
            summary_type: Type of summary for all
            max_words: Target length for all
            model: Model to use
            parallel: Process in parallel (limited by model loading)

        Returns:
            BatchSummaryResponse with all summaries and stats

        Example:
            >>> response = await service.batch_summarize(
            ...     texts=["Doc 1...", "Doc 2..."],
            ...     summary_type=SummaryType.TLDR
            ... )
        """
        start_time = time.time()
        summaries = []
        errors = []

        if parallel:
            # Process in parallel (note: model loading is still serialized)
            tasks = [
                self.summarize(
                    text=text, summary_type=summary_type, max_words=max_words, model=model
                )
                for text in texts
            ]

            results = await asyncio.gather(*tasks, return_exceptions=True)

            for i, result in enumerate(results):
                if isinstance(result, Exception):
                    errors.append({"index": i, "error": str(result)})
                else:
                    summaries.append(result)
        else:
            # Process sequentially
            for i, text in enumerate(texts):
                try:
                    summary = await self.summarize(
                        text=text, summary_type=summary_type, max_words=max_words, model=model
                    )
                    summaries.append(summary)
                except Exception as e:
                    errors.append({"index": i, "error": str(e)})

        total_time = time.time() - start_time

        return BatchSummaryResponse(
            summaries=summaries,
            total_count=len(texts),
            success_count=len(summaries),
            failed_count=len(errors),
            total_time=total_time,
            errors=errors,
        )

    async def stream_summarize(
        self,
        text: str,
        summary_type: SummaryType = SummaryType.ABSTRACTIVE,
        max_words: int = 150,
        model: str | None = None,
        language: str | None = None,
    ) -> AsyncIterator[str]:
        """Stream summary generation token by token.

        Args:
            text: Input text
            summary_type: Type of summary
            max_words: Target length
            model: Model to use
            language: Language code

        Yields:
            Text chunks as they're generated

        Example:
            >>> async for chunk in service.stream_summarize(text="..."):
            ...     print(chunk, end="", flush=True)
        """
        # Validate input
        if not text or len(text.strip()) < 100:
            raise ValueError("Text must be at least 100 characters")

        model_name = model or self.config.default_model
        detected_language = language or self._detect_language(text)

        # Optimize and build prompt
        optimized_text = self.prompt_builder.optimize_for_context(
            text, self.config.max_context_length, self.config.max_tokens
        )

        _, user_prompt = self.prompt_builder.build_prompt(
            optimized_text, summary_type, max_words, detected_language
        )

        # Load model
        async with self._lock:
            llm = self._load_model(model_name)

        # Stream generation
        loop = asyncio.get_event_loop()

        def generate_stream():
            """Generator function for streaming."""
            stream = llm(
                user_prompt,
                max_tokens=self.config.max_tokens,
                temperature=self.config.temperature,
                stop=["</s>", "<|end|>", "<|endoftext|>"],
                stream=True,
            )
            for chunk in stream:
                if "choices" in chunk and len(chunk["choices"]) > 0:
                    text = chunk["choices"][0].get("text", "")
                    if text:
                        yield text

        # Yield chunks
        for chunk in await loop.run_in_executor(None, lambda: list(generate_stream())):
            yield chunk

    def list_models(self):
        """List all available models.

        Returns:
            List of ModelInfo objects
        """
        return self.model_manager.list_available_models()

    def list_downloaded_models(self):
        """List downloaded models.

        Returns:
            List of ModelInfo for downloaded models
        """
        return self.model_manager.list_downloaded_models()

    async def download_model(self, model_name: str, progress_callback=None):
        """Download a model.

        Args:
            model_name: Model to download
            progress_callback: Progress callback

        Returns:
            ModelInfo
        """
        return await self.model_manager.download_model(
            model_name, progress_callback=progress_callback
        )

    def get_model_info(self, model_name: str):
        """Get model information.

        Args:
            model_name: Model identifier

        Returns:
            ModelInfo
        """
        return self.model_manager.get_model_info(model_name)

    async def cleanup(self) -> None:
        """Clean up resources.

        Unloads all cached models from memory.
        """
        self._model_cache.clear()
