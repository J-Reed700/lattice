from collections.abc import AsyncIterator
import json
import logging
from typing import Any

import httpx

from .models import (
    OllamaError,
    OllamaGenerateRequest,
    OllamaGenerateResponse,
    OllamaListResponse,
    OllamaStreamResponse,
)

logger = logging.getLogger(__name__)


class OllamaConnectionError(Exception):
    pass


class OllamaAPIError(Exception):
    pass


class OllamaTimeoutError(Exception):
    pass


class OllamaClient:
    def __init__(
        self,
        base_url: str = "http://localhost:11434",
        timeout: int = 120,
        stream_timeout: int = 300,
    ):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.stream_timeout = stream_timeout
        self._client: httpx.AsyncClient | None = None

    async def __aenter__(self) -> "OllamaClient":
        await self.connect()
        return self

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        await self.close()

    async def connect(self) -> None:
        if self._client is None:
            self._client = httpx.AsyncClient(
                base_url=self.base_url,
                timeout=httpx.Timeout(self.timeout),
                follow_redirects=True,
            )
            logger.info(f"Connected to Ollama at {self.base_url}")

    async def close(self) -> None:
        if self._client is not None:
            await self._client.aclose()
            self._client = None
            logger.info("Closed Ollama client connection")

    @property
    def client(self) -> httpx.AsyncClient:
        if self._client is None:
            raise OllamaConnectionError(
                "Client not connected. Use async context manager or call connect()."
            )
        return self._client

    async def health_check(self) -> bool:
        try:
            response = await self.client.get("/")
            return response.status_code == 200
        except httpx.RequestError as e:
            logger.error(f"Health check failed: {e}")
            raise OllamaConnectionError(f"Cannot connect to Ollama at {self.base_url}") from e

    async def list_models(self) -> OllamaListResponse:
        try:
            response = await self.client.get("/api/tags")
            response.raise_for_status()
            return OllamaListResponse(**response.json())
        except httpx.HTTPStatusError as e:
            logger.error(f"HTTP error listing models: {e}")
            error_data = self._parse_error(e.response)
            raise OllamaAPIError(f"Failed to list models: {error_data.error}") from e
        except httpx.RequestError as e:
            logger.error(f"Request error listing models: {e}")
            raise OllamaConnectionError(f"Connection error: {e}") from e

    async def generate(
        self,
        request: OllamaGenerateRequest,
    ) -> OllamaGenerateResponse:
        if request.stream:
            raise ValueError("Use generate_stream() for streaming requests")

        try:
            response = await self.client.post(
                "/api/generate",
                json=request.model_dump(exclude_none=True),
                timeout=httpx.Timeout(self.timeout),
            )
            response.raise_for_status()
            return OllamaGenerateResponse(**response.json())
        except httpx.HTTPStatusError as e:
            logger.error(f"HTTP error during generation: {e}")
            error_data = self._parse_error(e.response)
            raise OllamaAPIError(f"Generation failed: {error_data.error}") from e
        except httpx.TimeoutException as e:
            logger.error(f"Timeout during generation: {e}")
            raise OllamaTimeoutError(f"Request timed out after {self.timeout}s") from e
        except httpx.RequestError as e:
            logger.error(f"Request error during generation: {e}")
            raise OllamaConnectionError(f"Connection error: {e}") from e

    async def generate_stream(
        self,
        request: OllamaGenerateRequest,
    ) -> AsyncIterator[OllamaStreamResponse]:
        if not request.stream:
            request.stream = True

        try:
            async with self.client.stream(
                "POST",
                "/api/generate",
                json=request.model_dump(exclude_none=True),
                timeout=httpx.Timeout(self.stream_timeout),
            ) as response:
                response.raise_for_status()

                async for line in response.aiter_lines():
                    if not line.strip():
                        continue

                    try:
                        chunk_data = json.loads(line)
                        chunk = OllamaStreamResponse(**chunk_data)
                        yield chunk

                        if chunk.done:
                            break
                    except json.JSONDecodeError as e:
                        logger.warning(f"Failed to parse stream chunk: {e}")
                        continue

        except httpx.HTTPStatusError as e:
            logger.error(f"HTTP error during streaming: {e}")
            error_data = self._parse_error(e.response)
            raise OllamaAPIError(f"Streaming failed: {error_data.error}") from e
        except httpx.TimeoutException as e:
            logger.error(f"Timeout during streaming: {e}")
            raise OllamaTimeoutError(f"Stream timed out after {self.stream_timeout}s") from e
        except httpx.RequestError as e:
            logger.error(f"Request error during streaming: {e}")
            raise OllamaConnectionError(f"Connection error: {e}") from e

    def _parse_error(self, response: httpx.Response) -> OllamaError:
        try:
            return OllamaError(**response.json())
        except (json.JSONDecodeError, ValueError):
            return OllamaError(error=response.text or f"HTTP {response.status_code}")
