"""
Model management for SLM download, caching, and loading.

This module discovers GGUF quantized models directly from Hugging Face,
caches model metadata locally, and manages model lifecycle.
"""

import asyncio
from collections.abc import Callable
from datetime import UTC, datetime, timedelta
import json
from pathlib import Path
import re
from typing import ClassVar

import httpx

from .types import ModelDownloadProgress, ModelInfo, ModelQuantization, SummarizerConfig


class ModelNotFoundError(Exception):
    """Raised when a requested model is not available."""


class ModelDownloadError(Exception):
    """Raised when model download fails."""


class ModelManager:
    """Manages SLM model downloads, caching, and metadata."""

    HF_API_URL: ClassVar[str] = "https://huggingface.co/api/models"
    HF_SEARCH_QUERY: ClassVar[str] = "gguf instruct"
    HF_DEFAULT_LIMIT: ClassVar[int] = 80
    CATALOG_TTL_SECONDS: ClassVar[int] = 3600
    QUANTIZATION_FALLBACK_ORDER: ClassVar[list[ModelQuantization]] = [
        ModelQuantization.Q4_K_M,
        ModelQuantization.Q5_K_M,
        ModelQuantization.Q8_0,
    ]

    def __init__(self, config: SummarizerConfig):
        """Initialize model manager.

        Args:
            config: Summarizer configuration
        """
        self.config = config
        self.cache_dir = Path(config.model_cache_dir)
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self.metadata_file = self.cache_dir / "models.json"

        self.metadata: dict = {}
        self._catalog_cache: dict[str, dict] = {}
        self._catalog_updated_at: datetime | None = None
        self._load_metadata()

    def _load_metadata(self) -> None:
        """Load persisted metadata (downloads + catalog cache) from disk."""
        if not self.metadata_file.exists():
            self.metadata = {}
            self._catalog_cache = {}
            self._catalog_updated_at = None
            return

        with self.metadata_file.open() as f:
            raw = json.load(f)

        # Backward compatibility: older format stored download metadata at top level.
        if "downloads" in raw or "catalog_cache" in raw:
            self.metadata = raw.get("downloads", {})
            self._catalog_cache = raw.get("catalog_cache", {})
            updated_at = raw.get("catalog_updated_at")
        else:
            self.metadata = raw
            self._catalog_cache = {}
            updated_at = None

        if updated_at:
            try:
                self._catalog_updated_at = datetime.fromisoformat(updated_at)
            except ValueError:
                self._catalog_updated_at = None
        else:
            self._catalog_updated_at = None

    def _save_metadata(self) -> None:
        """Save download metadata and discovered catalog to disk."""
        payload = {
            "downloads": self.metadata,
            "catalog_cache": self._catalog_cache,
            "catalog_updated_at": self._catalog_updated_at.isoformat()
            if self._catalog_updated_at
            else None,
        }
        with self.metadata_file.open("w") as f:
            json.dump(payload, f, indent=2, default=str)

    def _catalog_is_stale(self) -> bool:
        if not self._catalog_cache or not self._catalog_updated_at:
            return True
        return datetime.now(UTC) - self._catalog_updated_at > timedelta(
            seconds=self.CATALOG_TTL_SECONDS
        )

    def _normalize_model_name(self, repo_id: str) -> str:
        name = repo_id.split("/")[-1].lower().strip()
        name = re.sub(r"-gguf$", "", name)
        name = re.sub(r"[^a-z0-9._-]+", "-", name)
        name = re.sub(r"-{2,}", "-", name).strip("-")
        return name or "model"

    def _estimate_parameters(self, text: str) -> float:
        match = re.search(r"(\d+(?:\.\d+)?)b", text.lower())
        if not match:
            return 0.0
        try:
            return float(match.group(1))
        except ValueError:
            return 0.0

    def _extract_quantization(self, filename: str) -> ModelQuantization | None:
        lower = filename.lower()
        if "q4_k_m" in lower or "q4_0" in lower:
            return ModelQuantization.Q4_K_M
        if "q5_k_m" in lower:
            return ModelQuantization.Q5_K_M
        if "q8_0" in lower:
            return ModelQuantization.Q8_0
        return None

    def _extract_languages(self, tags: list[str]) -> list[str]:
        tags_lower = {str(tag).lower() for tag in tags}
        if "multilingual" in tags_lower:
            return ["en", "zh", "es", "fr", "de", "ja", "ko", "ar", "ru"]
        return ["en"]

    def _build_catalog_from_hf_payload(self, payload: list[dict]) -> dict[str, dict]:
        catalog: dict[str, dict] = {}
        used_names: set[str] = set()

        for model in payload:
            repo_id = model.get("modelId") or model.get("id")
            if not repo_id:
                continue

            siblings = model.get("siblings") or []
            quantizations: dict[str, dict] = {}

            for sibling in siblings:
                filename = sibling.get("rfilename")
                if not filename or not filename.lower().endswith(".gguf"):
                    continue
                if "/" in filename or "\\" in filename or ".." in filename:
                    continue

                quant = self._extract_quantization(filename)
                if not quant:
                    continue

                size_bytes = int(sibling.get("size") or 0)
                size_gb = round(size_bytes / (1024**3), 3) if size_bytes > 0 else 0.0
                url = f"https://huggingface.co/{repo_id}/resolve/main/{filename}"

                existing = quantizations.get(quant.value)
                if existing is None or (
                    size_bytes > 0
                    and (existing.get("size_bytes", 0) == 0 or size_bytes < existing["size_bytes"])
                ):
                    quantizations[quant.value] = {
                        "size_gb": size_gb,
                        "size_bytes": size_bytes,
                        "url": url,
                        "filename": filename,
                    }

            if not quantizations:
                continue

            base_name = self._normalize_model_name(repo_id)
            model_name = base_name
            suffix = 2
            while model_name in used_names:
                model_name = f"{base_name}-{suffix}"
                suffix += 1
            used_names.add(model_name)

            tags = model.get("tags") or []
            full_name = model.get("name") or repo_id.split("/")[-1]
            parameters = self._estimate_parameters(repo_id)
            description = model.get("description") or f"Hugging Face model: {repo_id}"

            catalog[model_name] = {
                "repo_id": repo_id,
                "full_name": full_name,
                "parameters": parameters,
                "languages": self._extract_languages(tags),
                "description": description,
                "quantizations": quantizations,
            }

        return catalog

    def _fetch_catalog_sync(self) -> dict[str, dict]:
        params = {
            "search": self.HF_SEARCH_QUERY,
            "limit": self.HF_DEFAULT_LIMIT,
            "full": "true",
            "sort": "downloads",
            "direction": "-1",
        }

        with httpx.Client(timeout=20.0, follow_redirects=True) as client:
            response = client.get(self.HF_API_URL, params=params)
            response.raise_for_status()
            payload = response.json()

        if not isinstance(payload, list):
            raise ModelDownloadError("Unexpected Hugging Face response format")

        return self._build_catalog_from_hf_payload(payload)

    async def _fetch_catalog_async(self) -> dict[str, dict]:
        params = {
            "search": self.HF_SEARCH_QUERY,
            "limit": self.HF_DEFAULT_LIMIT,
            "full": "true",
            "sort": "downloads",
            "direction": "-1",
        }

        async with httpx.AsyncClient(timeout=20.0, follow_redirects=True) as client:
            response = await client.get(self.HF_API_URL, params=params)
            response.raise_for_status()
            payload = response.json()

        if not isinstance(payload, list):
            raise ModelDownloadError("Unexpected Hugging Face response format")

        return self._build_catalog_from_hf_payload(payload)

    def _refresh_catalog_cache(self, catalog: dict[str, dict]) -> None:
        self._catalog_cache = catalog
        self._catalog_updated_at = datetime.now(UTC)
        self._save_metadata()

    def _get_catalog_sync(self, force_refresh: bool = False) -> dict[str, dict]:
        if not force_refresh and not self._catalog_is_stale():
            return self._catalog_cache

        try:
            catalog = self._fetch_catalog_sync()
            if catalog:
                self._refresh_catalog_cache(catalog)
                return catalog
        except Exception:
            # Fall back to cached catalog when remote fetch fails.
            if self._catalog_cache:
                return self._catalog_cache

        return self._catalog_cache

    async def _get_catalog_async(self, force_refresh: bool = False) -> dict[str, dict]:
        if not force_refresh and not self._catalog_is_stale():
            return self._catalog_cache

        try:
            catalog = await self._fetch_catalog_async()
            if catalog:
                self._refresh_catalog_cache(catalog)
                return catalog
        except Exception:
            if self._catalog_cache:
                return self._catalog_cache

        return self._catalog_cache

    def _resolve_model_name(self, requested: str, catalog: dict[str, dict]) -> str:
        key = requested.lower().strip()

        if key in catalog:
            return key

        # Backward-compatible fuzzy match for legacy model names.
        candidates = [name for name in catalog if name.startswith(key) or key in name]
        if not candidates:
            raise ModelNotFoundError(f"Unknown model: {requested}")

        # Deterministic tie-break: shortest then alphabetical.
        candidates.sort(key=lambda n: (len(n), n))
        return candidates[0]

    def _build_model_path(self, model_name: str, quantization: ModelQuantization) -> Path:
        filename = f"{model_name}-{quantization.value}.gguf"
        return self.cache_dir / filename

    def _select_quantization_entry(
        self,
        model_info: dict,
        quantization: ModelQuantization | None = None,
    ) -> tuple[ModelQuantization, dict]:
        quantizations = model_info.get("quantizations", {})
        preferred = (quantization or self.config.quantization).value

        if preferred in quantizations:
            selected = ModelQuantization(preferred)
            return selected, quantizations[preferred]

        for fallback in self.QUANTIZATION_FALLBACK_ORDER:
            if fallback.value in quantizations:
                return fallback, quantizations[fallback.value]

        raise ModelNotFoundError("No compatible quantization available for model")

    def list_available_models(self) -> list[ModelInfo]:
        """List available downloadable models discovered from Hugging Face."""
        catalog = self._get_catalog_sync()
        if not catalog:
            return []

        models: list[ModelInfo] = []
        for model_name, info in catalog.items():
            try:
                selected_quant, quant_info = self._select_quantization_entry(
                    info, self.config.quantization
                )
            except ModelNotFoundError:
                continue

            model_path = self._build_model_path(model_name, selected_quant)
            models.append(
                ModelInfo(
                    name=model_name,
                    full_name=info["full_name"],
                    size_gb=quant_info.get("size_gb", 0.0),
                    parameters=info.get("parameters", 0.0),
                    quantization=selected_quant,
                    languages=info.get("languages", ["en"]),
                    downloaded=model_path.exists(),
                    path=str(model_path) if model_path.exists() else None,
                    download_url=quant_info.get("url"),
                    description=info.get("description"),
                )
            )

        return models

    def list_downloaded_models(self) -> list[ModelInfo]:
        """List only downloaded models."""
        return [m for m in self.list_available_models() if m.downloaded]

    def get_model_path(
        self, model_name: str, quantization: ModelQuantization | None = None
    ) -> Path | None:
        """Get local path for a model."""
        catalog = self._get_catalog_sync()
        if not catalog:
            raise ModelNotFoundError(f"Unknown model: {model_name}")

        resolved_name = self._resolve_model_name(model_name, catalog)
        selected_quant, _ = self._select_quantization_entry(
            catalog[resolved_name], quantization
        )
        return self._build_model_path(resolved_name, selected_quant)

    def is_model_downloaded(
        self, model_name: str, quantization: ModelQuantization | None = None
    ) -> bool:
        """Check if model is downloaded."""
        try:
            path = self.get_model_path(model_name, quantization)
            return path is not None and path.exists()
        except ModelNotFoundError:
            return False

    async def download_model(
        self,
        model_name: str,
        quantization: ModelQuantization | None = None,
        progress_callback: Callable[[ModelDownloadProgress], None] | None = None,
    ) -> ModelInfo:
        """Download a model from Hugging Face."""
        catalog = await self._get_catalog_async()
        if not catalog:
            raise ModelNotFoundError(f"Unknown model: {model_name}")

        resolved_name = self._resolve_model_name(model_name, catalog)
        model_info = catalog[resolved_name]
        quant, quant_info = self._select_quantization_entry(model_info, quantization)

        # Check if already downloaded
        model_path = self._build_model_path(resolved_name, quant)
        if model_path.exists():
            return ModelInfo(
                name=resolved_name,
                full_name=model_info["full_name"],
                size_gb=quant_info.get("size_gb", 0.0),
                parameters=model_info.get("parameters", 0.0),
                quantization=quant,
                languages=model_info.get("languages", ["en"]),
                downloaded=True,
                path=str(model_path),
                download_url=quant_info.get("url"),
                description=model_info.get("description"),
            )

        # Download model
        url = quant_info["url"]
        temp_path = model_path.with_suffix(".tmp")

        try:
            loop = asyncio.get_running_loop()
            async with (
                httpx.AsyncClient(timeout=None) as client,
                client.stream("GET", url, follow_redirects=True) as response,
            ):
                response.raise_for_status()

                total_bytes = int(response.headers.get("content-length", 0))
                downloaded_bytes = 0
                start_time = loop.time()

                with temp_path.open("wb") as f:
                    async for chunk in response.aiter_bytes(chunk_size=8192):
                        f.write(chunk)
                        downloaded_bytes += len(chunk)

                        # Update progress
                        if progress_callback and total_bytes > 0:
                            elapsed = loop.time() - start_time
                            speed_mbps = (
                                (downloaded_bytes / elapsed / 1024 / 1024) if elapsed > 0 else 0
                            )
                            remaining_bytes = total_bytes - downloaded_bytes
                            eta = (
                                (remaining_bytes / (speed_mbps * 1024 * 1024))
                                if speed_mbps > 0
                                else 0
                            )

                            progress = ModelDownloadProgress(
                                model_name=resolved_name,
                                bytes_downloaded=downloaded_bytes,
                                total_bytes=total_bytes,
                                progress_percent=(downloaded_bytes / total_bytes * 100),
                                speed_mbps=speed_mbps,
                                eta_seconds=eta,
                            )
                            progress_callback(progress)

            # Move to final location
            temp_path.rename(model_path)

            # Save metadata
            self.metadata[f"{resolved_name}-{quant.value}"] = {
                "downloaded_at": datetime.now(UTC).isoformat(),
                "size_bytes": downloaded_bytes,
                "url": url,
            }
            self._save_metadata()

            return ModelInfo(
                name=resolved_name,
                full_name=model_info["full_name"],
                size_gb=quant_info.get("size_gb", 0.0),
                parameters=model_info.get("parameters", 0.0),
                quantization=quant,
                languages=model_info.get("languages", ["en"]),
                downloaded=True,
                path=str(model_path),
                download_url=url,
                description=model_info.get("description"),
            )

        except Exception as e:
            # Clean up temp file
            if temp_path.exists():
                temp_path.unlink()
            raise ModelDownloadError(f"Failed to download {resolved_name}: {e!s}") from e

    async def delete_model(
        self, model_name: str, quantization: ModelQuantization | None = None
    ) -> bool:
        """Delete a downloaded model."""
        path = self.get_model_path(model_name, quantization)
        if path and path.exists():
            path.unlink()
            quant = quantization or self.config.quantization

            catalog = self._get_catalog_sync()
            try:
                resolved_name = self._resolve_model_name(model_name, catalog)
            except ModelNotFoundError:
                resolved_name = model_name

            self.metadata.pop(f"{resolved_name}-{quant.value}", None)
            self._save_metadata()
            return True
        return False

    def get_model_info(
        self, model_name: str, quantization: ModelQuantization | None = None
    ) -> ModelInfo:
        """Get detailed info about a model."""
        catalog = self._get_catalog_sync()
        if not catalog:
            raise ModelNotFoundError(f"Unknown model: {model_name}")

        resolved_name = self._resolve_model_name(model_name, catalog)
        model_info = catalog[resolved_name]
        selected_quant, quant_info = self._select_quantization_entry(model_info, quantization)
        model_path = self._build_model_path(resolved_name, selected_quant)

        return ModelInfo(
            name=resolved_name,
            full_name=model_info["full_name"],
            size_gb=quant_info.get("size_gb", 0.0),
            parameters=model_info.get("parameters", 0.0),
            quantization=selected_quant,
            languages=model_info.get("languages", ["en"]),
            downloaded=model_path.exists(),
            path=str(model_path) if model_path.exists() else None,
            download_url=quant_info.get("url"),
            description=model_info.get("description"),
        )

    def get_disk_usage(self) -> dict:
        """Get disk usage statistics for downloaded models."""
        total_bytes = 0
        model_count = 0

        for model_file in self.cache_dir.glob("*.gguf"):
            total_bytes += model_file.stat().st_size
            model_count += 1

        return {
            "total_bytes": total_bytes,
            "total_gb": total_bytes / (1024**3),
            "model_count": model_count,
            "cache_dir": str(self.cache_dir),
        }
