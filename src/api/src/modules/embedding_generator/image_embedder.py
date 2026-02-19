import asyncio
from pathlib import Path
import threading
from typing import Any

import numpy as np
from PIL import Image
import torch
from transformers import CLIPModel, CLIPProcessor

from .model_manager import ModelManager
from .types import (
    BATCH_SIZE_CPU,
    BATCH_SIZE_GPU,
    IMAGE_EMBEDDING_DIM,
    IMAGE_MODEL_NAME,
    InputValidationError,
    ModelLoadError,
    OutOfMemoryError,
    ProcessingError,
    ProgressCallback,
)

_image_model = None
_image_processor = None
_image_model_lock = threading.Lock()


class ImageEmbedder:
    def __init__(
        self,
        device: str | None = None,
        batch_size: int | None = None,
    ):
        if device is None:
            device = "cuda" if torch.cuda.is_available() else "cpu"
        self._device = device

        if batch_size is None:
            batch_size = BATCH_SIZE_GPU if device == "cuda" else BATCH_SIZE_CPU
        self._batch_size = batch_size

        self._model = None
        self._processor = None

    def _load_model(self) -> None:
        global _image_model, _image_processor

        if not ModelManager.is_model_cached(IMAGE_MODEL_NAME):
            ModelManager.download_model(IMAGE_MODEL_NAME)

        with _image_model_lock:
            if _image_model is None:
                try:
                    cache_dir = str(ModelManager.get_model_cache_dir())

                    _image_model = CLIPModel.from_pretrained(
                        IMAGE_MODEL_NAME, cache_dir=cache_dir
                    ).to(self._device)

                    _image_processor = CLIPProcessor.from_pretrained(
                        IMAGE_MODEL_NAME, cache_dir=cache_dir
                    )

                    _image_model.eval()

                except Exception as e:
                    raise ModelLoadError(f"Failed to load image model: {e!s}")

        self._model = _image_model
        self._processor = _image_processor

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    @property
    def embedding_dim(self) -> int:
        return IMAGE_EMBEDDING_DIM

    @property
    def device(self) -> str:
        return self._device

    def preprocess_image(self, image_input: str | Path | Image.Image) -> Image.Image:
        try:
            if isinstance(image_input, Image.Image):
                image = image_input
            elif isinstance(image_input, (str, Path)):
                image_path = Path(image_input)
                if not image_path.exists():
                    raise InputValidationError(f"Image file not found: {image_path}")
                image = Image.open(image_path)
            else:
                raise InputValidationError(f"Invalid image input type: {type(image_input)}")

            if image.mode not in ("RGB", "L"):
                image = image.convert("RGB")

            return image

        except InputValidationError:
            raise
        except Exception as e:
            raise ProcessingError(f"Failed to preprocess image: {e!s}")

    def _build_context_metadata(self, metadata: dict[str, Any]) -> dict[str, Any]:
        context_meta = {}

        if metadata.get("file_name"):
            context_meta["file_name"] = metadata["file_name"]

        if metadata.get("file_path"):
            context_meta["file_path"] = metadata["file_path"]

        if metadata.get("title"):
            context_meta["title"] = metadata["title"]

        if metadata.get("width"):
            context_meta["width"] = metadata["width"]

        if metadata.get("height"):
            context_meta["height"] = metadata["height"]

        if metadata.get("format"):
            context_meta["format"] = metadata["format"]

        return context_meta

    def _embed_sync(
        self, image_input: str | Path | Image.Image, metadata: dict[str, Any] | None = None
    ) -> tuple[np.ndarray, dict[str, Any]]:
        """Synchronous image embedding generation (for use with asyncio.to_thread)."""
        if not self.is_loaded:
            self._load_model()

        image = self.preprocess_image(image_input)

        if metadata is None:
            metadata = {}

        context_metadata = self._build_context_metadata(metadata)

        try:
            inputs = self._processor(images=image, return_tensors="pt").to(self._device)

            with torch.no_grad():
                image_features = self._model.get_image_features(**inputs)

            embedding = image_features.cpu().numpy()[0]

            norm = np.linalg.norm(embedding)
            if norm > 0:
                embedding = embedding / norm

            return embedding.astype(np.float32), context_metadata

        except torch.cuda.OutOfMemoryError:
            raise OutOfMemoryError("GPU out of memory. Try reducing batch size or using CPU.")
        except Exception as e:
            raise ProcessingError(f"Failed to generate image embedding: {e!s}")

    async def embed(
        self, image_input: str | Path | Image.Image, metadata: dict[str, Any] | None = None
    ) -> tuple[np.ndarray, dict[str, Any]]:
        """Generate image embedding asynchronously using thread pool."""
        return await asyncio.to_thread(self._embed_sync, image_input, metadata)

    async def embed_batch(
        self,
        image_inputs: list[str | Path | Image.Image],
        metadata_list: list[dict[str, Any]] | None = None,
        progress_callback: ProgressCallback | None = None,
    ) -> list[tuple[np.ndarray, dict[str, Any]]]:
        """Generate batch image embeddings asynchronously."""
        if not image_inputs:
            raise InputValidationError("Image list cannot be empty")

        if not self.is_loaded:
            self._load_model()

        if metadata_list is None:
            metadata_list = [{} for _ in image_inputs]

        embeddings = []
        total = len(image_inputs)

        try:
            for i, (img_input, metadata) in enumerate(
                zip(image_inputs, metadata_list, strict=False)
            ):
                embedding, context_metadata = await self.embed(img_input, metadata)
                embeddings.append((embedding, context_metadata))

                if progress_callback:
                    progress_callback(i + 1, total)

            return embeddings

        except torch.cuda.OutOfMemoryError:
            raise OutOfMemoryError("GPU out of memory. Try reducing batch size or using CPU.")
        except Exception as e:
            raise ProcessingError(f"Failed to generate batch embeddings: {e!s}")
