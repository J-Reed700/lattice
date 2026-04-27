from pathlib import Path
import shutil

from huggingface_hub import snapshot_download

from .types import ModelLoadError


class ModelManager:
    @staticmethod
    def get_model_cache_dir() -> Path:
        cache_dir = Path.home() / ".vault" / "models"
        cache_dir.mkdir(parents=True, exist_ok=True)
        return cache_dir

    @staticmethod
    def download_model(model_name: str, force: bool = False) -> Path:
        try:
            cache_dir = ModelManager.get_model_cache_dir()
            model_path = cache_dir / model_name.replace("/", "--")

            if model_path.exists() and not force:
                return model_path

            if force and model_path.exists():
                shutil.rmtree(model_path)

            downloaded_path = snapshot_download(
                repo_id=model_name,
                cache_dir=str(cache_dir),
                local_dir=str(model_path),
                local_dir_use_symlinks=False,
            )

            return Path(downloaded_path)

        except Exception as e:
            raise ModelLoadError(f"Failed to download model {model_name}: {e!s}")

    @staticmethod
    def is_model_cached(model_name: str) -> bool:
        cache_dir = ModelManager.get_model_cache_dir()
        model_path = cache_dir / model_name.replace("/", "--")
        return model_path.exists() and any(model_path.iterdir())

    @staticmethod
    def clear_model_cache(model_name: str | None = None) -> None:
        cache_dir = ModelManager.get_model_cache_dir()

        if model_name is None:
            if cache_dir.exists():
                shutil.rmtree(cache_dir)
                cache_dir.mkdir(parents=True, exist_ok=True)
        else:
            model_path = cache_dir / model_name.replace("/", "--")
            if model_path.exists():
                shutil.rmtree(model_path)
