#!/usr/bin/env python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from sentence_transformers import SentenceTransformer
from src.config.settings import settings


def download_models():
    print("Downloading embedding models...")
    print(f"Cache directory: {settings.MODEL_CACHE_DIR}")

    cache_dir = Path(settings.MODEL_CACHE_DIR)
    cache_dir.mkdir(parents=True, exist_ok=True)

    print(f"\n1. Downloading text embedding model: {settings.EMBEDDING_MODEL}")
    try:
        text_model = SentenceTransformer(
            settings.EMBEDDING_MODEL,
            cache_folder=str(cache_dir),
        )
        print(f"   ✓ Text model downloaded successfully!")
        print(f"   Embedding dimension: {text_model.get_sentence_embedding_dimension()}")
    except Exception as e:
        print(f"   ✗ Failed to download text model: {e}")
        return False

    print(f"\n2. Downloading image embedding model: {settings.IMAGE_EMBEDDING_MODEL}")
    try:
        image_model = SentenceTransformer(
            settings.IMAGE_EMBEDDING_MODEL,
            cache_folder=str(cache_dir),
        )
        print(f"   ✓ Image model downloaded successfully!")
        print(f"   Embedding dimension: {image_model.get_sentence_embedding_dimension()}")
    except Exception as e:
        print(f"   ✗ Failed to download image model: {e}")
        return False

    print("\n✓ All models downloaded successfully!")
    print(f"Models are cached in: {cache_dir.absolute()}")
    return True


if __name__ == "__main__":
    success = download_models()
    sys.exit(0 if success else 1)
