import asyncio
from collections.abc import AsyncIterator
from dataclasses import dataclass
import hashlib
import logging
import time
from typing import Any

from .models import (
    OllamaGenerateRequest,
    OllamaGenerateResponse,
    OllamaModel,
    OllamaStreamResponse,
)
from .ollama_client import (
    OllamaAPIError,
    OllamaClient,
    OllamaConnectionError,
    OllamaTimeoutError,
)

logger = logging.getLogger(__name__)


@dataclass
class CacheMetrics:
    hits: int = 0
    misses: int = 0
    total_time_saved_ms: float = 0.0

    def record_hit(self, time_saved_ms: float = 0.0):
        self.hits += 1
        self.total_time_saved_ms += time_saved_ms

    def record_miss(self):
        self.misses += 1

    @property
    def hit_rate(self) -> float:
        total = self.hits + self.misses
        return self.hits / total if total > 0 else 0.0


@dataclass
class PromptCache:
    cached_context: list[int] | None = None
    context_hash: str | None = None
    last_system_prompt: str | None = None
    last_context_text: str | None = None


class OllamaService:
    def __init__(
        self,
        base_url: str = "http://localhost:11434",
        default_model: str = "llama2",
        timeout: int = 120,
        stream_timeout: int = 300,
        enable_prompt_caching: bool = True,
        cache_keep_alive: str = "5m",
        max_parallel_requests: int = 3,
    ):
        self.base_url = base_url
        self.default_model = default_model
        self.timeout = timeout
        self.stream_timeout = stream_timeout
        self.enable_prompt_caching = enable_prompt_caching
        self.cache_keep_alive = cache_keep_alive
        self.max_parallel_requests = max(1, max_parallel_requests)
        self._client: OllamaClient | None = None
        self._prompt_cache = PromptCache()
        self._cache_metrics = CacheMetrics()
        self._request_semaphore = asyncio.Semaphore(self.max_parallel_requests)

    async def __aenter__(self) -> "OllamaService":
        await self.initialize()
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        await self.cleanup()

    async def initialize(self) -> None:
        if self._client is None:
            self._client = OllamaClient(
                base_url=self.base_url,
                timeout=self.timeout,
                stream_timeout=self.stream_timeout,
            )
            await self._client.connect()
            logger.info(f"OllamaService initialized with model {self.default_model}")

    async def cleanup(self) -> None:
        if self._client is not None:
            await self._client.close()
            self._client = None
            logger.info("OllamaService cleaned up")

    @property
    def client(self) -> OllamaClient:
        if self._client is None:
            raise RuntimeError(
                "Service not initialized. Use async context manager or call initialize()."
            )
        return self._client

    async def health_check(self) -> bool:
        try:
            return await self.client.health_check()
        except OllamaConnectionError:
            return False

    async def list_available_models(self) -> list[str]:
        try:
            response = await self.client.list_models()
            return [model.name for model in response.models]
        except (OllamaConnectionError, OllamaAPIError) as e:
            logger.error(f"Failed to list models: {e}")
            raise

    async def get_model_info(self, model_name: str | None = None) -> OllamaModel | None:
        model_name = model_name or self.default_model
        try:
            response = await self.client.list_models()
            for model in response.models:
                if model.name == model_name:
                    return model
            return None
        except (OllamaConnectionError, OllamaAPIError) as e:
            logger.error(f"Failed to get model info: {e}")
            raise

    def _compute_context_hash(self, system: str | None, context_text: str) -> str:
        content = f"{system or ''}\n{context_text}"
        return hashlib.md5(content.encode()).hexdigest()

    def _is_cache_valid(self, system: str | None, context_text: str) -> bool:
        if not self.enable_prompt_caching or self._prompt_cache.cached_context is None:
            return False
        new_hash = self._compute_context_hash(system, context_text)
        return self._prompt_cache.context_hash == new_hash

    def invalidate_cache(self):
        logger.info("Invalidating prompt cache")
        self._prompt_cache.cached_context = None
        self._prompt_cache.context_hash = None
        self._prompt_cache.last_system_prompt = None
        self._prompt_cache.last_context_text = None

    def get_cache_metrics(self) -> dict[str, Any]:
        return {
            "hits": self._cache_metrics.hits,
            "misses": self._cache_metrics.misses,
            "hit_rate": self._cache_metrics.hit_rate,
            "total_time_saved_ms": self._cache_metrics.total_time_saved_ms,
            "cache_enabled": self.enable_prompt_caching,
        }

    async def generate(
        self,
        prompt: str,
        model: str | None = None,
        system: str | None = None,
        context: list[int] | None = None,
        options: dict[str, Any] | None = None,
        format: str | None = None,
        raw: bool = False,
        keep_alive: str | None = None,
        context_text: str | None = None,
    ) -> OllamaGenerateResponse:
        model = model or self.default_model
        start_time = time.time()

        cache_hit = False
        if context is None and context_text is not None and self.enable_prompt_caching:
            if self._is_cache_valid(system, context_text):
                context = self._prompt_cache.cached_context
                cache_hit = True
                logger.info(f"Prompt cache HIT (hash: {self._prompt_cache.context_hash[:8]})")
            else:
                logger.info("Prompt cache MISS")
                self._cache_metrics.record_miss()

        if keep_alive is None and self.enable_prompt_caching:
            keep_alive = self.cache_keep_alive

        request = OllamaGenerateRequest(
            model=model,
            prompt=prompt,
            system=system,
            context=context,
            stream=False,
            raw=raw,
            format=format,
            options=options,
            keep_alive=keep_alive,
        )

        try:
            logger.info(
                f"Generating with model {model}, prompt length: {len(prompt)}, cache_hit: {cache_hit}"
            )
            async with self._request_semaphore:
                response = await self.client.generate(request)

            elapsed_ms = (time.time() - start_time) * 1000
            logger.info(
                f"Generation complete in {elapsed_ms:.0f}ms. Response length: {len(response.response)}"
            )

            if self.enable_prompt_caching and response.context:
                if cache_hit:
                    estimated_time_saved = elapsed_ms * 4
                    self._cache_metrics.record_hit(estimated_time_saved)
                    logger.info(f"Cache hit saved ~{estimated_time_saved:.0f}ms (estimated)")
                elif context_text is not None:
                    self._prompt_cache.cached_context = response.context
                    self._prompt_cache.context_hash = self._compute_context_hash(
                        system, context_text
                    )
                    self._prompt_cache.last_system_prompt = system
                    self._prompt_cache.last_context_text = context_text
                    logger.info(
                        f"Cached context for future requests (hash: {self._prompt_cache.context_hash[:8]})"
                    )

            return response
        except OllamaTimeoutError as e:
            logger.error(f"Generation timeout: {e}")
            raise
        except OllamaAPIError as e:
            logger.error(f"Generation API error: {e}")
            raise
        except OllamaConnectionError as e:
            logger.error(f"Generation connection error: {e}")
            raise

    async def generate_stream(
        self,
        prompt: str,
        model: str | None = None,
        system: str | None = None,
        context: list[int] | None = None,
        options: dict[str, Any] | None = None,
        format: str | None = None,
        raw: bool = False,
        keep_alive: str | None = None,
        context_text: str | None = None,
    ) -> AsyncIterator[OllamaStreamResponse]:
        model = model or self.default_model
        start_time = time.time()

        cache_hit = False
        if context is None and context_text is not None and self.enable_prompt_caching:
            if self._is_cache_valid(system, context_text):
                context = self._prompt_cache.cached_context
                cache_hit = True
                logger.info(
                    f"Prompt cache HIT for streaming (hash: {self._prompt_cache.context_hash[:8]})"
                )
            else:
                logger.info("Prompt cache MISS for streaming")
                self._cache_metrics.record_miss()

        if keep_alive is None and self.enable_prompt_caching:
            keep_alive = self.cache_keep_alive

        request = OllamaGenerateRequest(
            model=model,
            prompt=prompt,
            system=system,
            context=context,
            stream=True,
            raw=raw,
            format=format,
            options=options,
            keep_alive=keep_alive,
        )

        try:
            logger.info(
                f"Streaming generation with model {model}, prompt length: {len(prompt)}, cache_hit: {cache_hit}"
            )
            chunk_count = 0
            last_context = None

            async with self._request_semaphore:
                async for chunk in self.client.generate_stream(request):
                    chunk_count += 1
                    if hasattr(chunk, "context") and chunk.context:
                        last_context = chunk.context
                    yield chunk

            elapsed_ms = (time.time() - start_time) * 1000
            logger.info(f"Streaming complete in {elapsed_ms:.0f}ms. Received {chunk_count} chunks")

            if self.enable_prompt_caching and last_context:
                if cache_hit:
                    estimated_time_saved = elapsed_ms * 4
                    self._cache_metrics.record_hit(estimated_time_saved)
                    logger.info(f"Cache hit saved ~{estimated_time_saved:.0f}ms (estimated)")
                elif context_text is not None:
                    self._prompt_cache.cached_context = last_context
                    self._prompt_cache.context_hash = self._compute_context_hash(
                        system, context_text
                    )
                    self._prompt_cache.last_system_prompt = system
                    self._prompt_cache.last_context_text = context_text
                    logger.info(
                        f"Cached context for future requests (hash: {self._prompt_cache.context_hash[:8]})"
                    )

        except OllamaTimeoutError as e:
            logger.error(f"Streaming timeout: {e}")
            raise
        except OllamaAPIError as e:
            logger.error(f"Streaming API error: {e}")
            raise
        except OllamaConnectionError as e:
            logger.error(f"Streaming connection error: {e}")
            raise

    async def generate_text(
        self,
        prompt: str,
        model: str | None = None,
        system: str | None = None,
        max_tokens: int | None = None,
        temperature: float | None = None,
        top_p: float | None = None,
        top_k: int | None = None,
        context_text: str | None = None,
    ) -> str:
        options: dict[str, Any] = {}
        if max_tokens is not None:
            options["num_predict"] = max_tokens
        if temperature is not None:
            options["temperature"] = temperature
        if top_p is not None:
            options["top_p"] = top_p
        if top_k is not None:
            options["top_k"] = top_k

        response = await self.generate(
            prompt=prompt,
            model=model,
            system=system,
            options=options if options else None,
            context_text=context_text,
        )
        return response.response

    async def generate_text_stream(
        self,
        prompt: str,
        model: str | None = None,
        system: str | None = None,
        max_tokens: int | None = None,
        temperature: float | None = None,
        top_p: float | None = None,
        top_k: int | None = None,
        context_text: str | None = None,
    ) -> AsyncIterator[str]:
        options: dict[str, Any] = {}
        if max_tokens is not None:
            options["num_predict"] = max_tokens
        if temperature is not None:
            options["temperature"] = temperature
        if top_p is not None:
            options["top_p"] = top_p
        if top_k is not None:
            options["top_k"] = top_k

        async for chunk in self.generate_stream(
            prompt=prompt,
            model=model,
            system=system,
            options=options if options else None,
            context_text=context_text,
        ):
            if chunk.response:
                yield chunk.response

    async def chat(
        self,
        messages: list[dict[str, str]],
        model: str | None = None,
        options: dict[str, Any] | None = None,
    ) -> str:
        prompt_parts = []
        system_prompt = None

        for message in messages:
            role = message.get("role", "user")
            content = message.get("content", "")

            if role == "system":
                system_prompt = content
            elif role == "user":
                prompt_parts.append(f"User: {content}")
            elif role == "assistant":
                prompt_parts.append(f"Assistant: {content}")

        prompt = "\n".join(prompt_parts)
        if prompt_parts:
            prompt += "\nAssistant:"

        response = await self.generate(
            prompt=prompt,
            model=model,
            system=system_prompt,
            options=options,
        )
        return response.response

    async def generate_many(
        self,
        prompts: list[str],
        model: str | None = None,
        system: str | None = None,
        options: dict[str, Any] | None = None,
        max_parallel_requests: int | None = None,
    ) -> list[OllamaGenerateResponse]:
        """
        Generate responses for many prompts with bounded parallelism.

        This enables 3-4 concurrent Ollama calls safely while applying backpressure.
        """
        if not prompts:
            return []

        local_parallelism = max_parallel_requests or self.max_parallel_requests
        semaphore = asyncio.Semaphore(max(1, local_parallelism))
        ordered_results: list[OllamaGenerateResponse | None] = [None] * len(prompts)

        async def _generate_single(index: int, prompt: str) -> None:
            async with semaphore:
                ordered_results[index] = await self.generate(
                    prompt=prompt,
                    model=model,
                    system=system,
                    options=options,
                )

        await asyncio.gather(
            *(_generate_single(index, prompt) for index, prompt in enumerate(prompts))
        )

        return [result for result in ordered_results if result is not None]

    async def generate_text_many(
        self,
        prompts: list[str],
        model: str | None = None,
        system: str | None = None,
        max_tokens: int | None = None,
        temperature: float | None = None,
        top_p: float | None = None,
        top_k: int | None = None,
        max_parallel_requests: int | None = None,
    ) -> list[str]:
        """Text-only wrapper for batch generation."""
        options: dict[str, Any] = {}
        if max_tokens is not None:
            options["num_predict"] = max_tokens
        if temperature is not None:
            options["temperature"] = temperature
        if top_p is not None:
            options["top_p"] = top_p
        if top_k is not None:
            options["top_k"] = top_k

        responses = await self.generate_many(
            prompts=prompts,
            model=model,
            system=system,
            options=options if options else None,
            max_parallel_requests=max_parallel_requests,
        )
        return [response.response for response in responses]

    async def is_model_available(self, model_name: str) -> bool:
        try:
            available_models = await self.list_available_models()
            return model_name in available_models
        except (OllamaConnectionError, OllamaAPIError):
            return False
