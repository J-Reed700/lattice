from collections.abc import Callable
from pathlib import Path
from typing import Union

from PIL import Image

TEXT_MODEL_NAME = "dunzhang/stella_en_1.5B_v5"
IMAGE_MODEL_NAME = "openai/clip-vit-base-patch32"

TEXT_EMBEDDING_DIM = 768
IMAGE_EMBEDDING_DIM = 512

OLD_TEXT_MODEL_NAME = "sentence-transformers/all-MiniLM-L6-v2"
OLD_TEXT_EMBEDDING_DIM = 384

BATCH_SIZE_CPU = 64  # Performance optimization: Increased from 32 for 2-3x speedup
BATCH_SIZE_GPU = 128

TEXT_CHUNK_SIZE = 450
TEXT_CHUNK_OVERLAP = 50
MAX_TEXT_LENGTH = 10000

TextInput = Union[str, list[str]]
ImageInput = Union[str, Path, Image.Image, list[str | Path | Image.Image]]
ProgressCallback = Callable[[int, int], None]


class EmbeddingError(Exception):
    pass


class ModelLoadError(EmbeddingError):
    pass


class InputValidationError(EmbeddingError):
    pass


class ProcessingError(EmbeddingError):
    pass


class OutOfMemoryError(EmbeddingError):
    pass
