from dataclasses import dataclass
import hashlib
import logging
from pathlib import Path
import shutil

from PIL import Image

from src.config.settings import get_settings

logger = logging.getLogger(__name__)


@dataclass
class StoredFileInfo:
    file_path: Path
    file_hash: str
    size_bytes: int
    thumbnail_path: Path | None = None


class FileStorageService:
    def __init__(self):
        self.settings = get_settings()
        self.storage_base = Path(self.settings.model_cache_dir).parent / "storage"
        self.documents_path = self.storage_base / "documents"
        self.thumbnails_path = self.storage_base / "thumbnails"

        self.documents_path.mkdir(parents=True, exist_ok=True)
        self.thumbnails_path.mkdir(parents=True, exist_ok=True)

    async def store_file(self, file_data: bytes, filename: str, mime_type: str) -> StoredFileInfo:
        file_hash = hashlib.sha256(file_data).hexdigest()

        extension = Path(filename).suffix or ".bin"
        file_path = self._get_storage_path(file_hash, extension)

        file_path.parent.mkdir(parents=True, exist_ok=True)

        with open(file_path, "wb") as f:
            f.write(file_data)

        logger.info(f"Stored file: {file_path} ({len(file_data)} bytes)")

        thumbnail_path = None
        if mime_type.startswith("image/"):
            try:
                thumbnail_path = await self.generate_thumbnail(file_path, file_hash)
            except Exception as e:
                logger.warning(f"Failed to generate thumbnail: {e}")

        return StoredFileInfo(
            file_path=file_path,
            file_hash=file_hash,
            size_bytes=len(file_data),
            thumbnail_path=thumbnail_path,
        )

    async def store_file_from_path(
        self, source_path: Path, filename: str | None = None
    ) -> StoredFileInfo:
        file_hash = await self.get_file_hash(source_path)

        extension = source_path.suffix
        file_path = self._get_storage_path(file_hash, extension)

        if file_path.exists():
            logger.info(f"File already exists: {file_path}")
            return StoredFileInfo(
                file_path=file_path, file_hash=file_hash, size_bytes=file_path.stat().st_size
            )

        file_path.parent.mkdir(parents=True, exist_ok=True)

        shutil.copy2(source_path, file_path)

        logger.info(f"Copied file: {source_path} → {file_path}")

        thumbnail_path = None
        if source_path.suffix.lower() in [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp"]:
            try:
                thumbnail_path = await self.generate_thumbnail(file_path, file_hash)
            except Exception as e:
                logger.warning(f"Failed to generate thumbnail: {e}")

        return StoredFileInfo(
            file_path=file_path,
            file_hash=file_hash,
            size_bytes=file_path.stat().st_size,
            thumbnail_path=thumbnail_path,
        )

    async def delete_file(self, file_path: Path) -> bool:
        try:
            if file_path.exists():
                file_path.unlink()
                logger.info(f"Deleted file: {file_path}")

            file_hash = file_path.stem
            thumbnail_path = (
                self.thumbnails_path / file_hash[:2] / file_hash[2:4] / f"{file_hash}.jpg"
            )
            if thumbnail_path.exists():
                thumbnail_path.unlink()
                logger.info(f"Deleted thumbnail: {thumbnail_path}")

            return True
        except Exception as e:
            logger.error(f"Failed to delete file: {e}")
            return False

    async def get_file_hash(self, file_path: Path) -> str:
        hasher = hashlib.sha256()
        with open(file_path, "rb") as f:
            while chunk := f.read(8192):
                hasher.update(chunk)
        return hasher.hexdigest()

    async def generate_thumbnail(
        self, image_path: Path, file_hash: str, size: tuple[int, int] = (256, 256)
    ) -> Path:
        thumbnail_path = self.thumbnails_path / file_hash[:2] / file_hash[2:4] / f"{file_hash}.jpg"
        thumbnail_path.parent.mkdir(parents=True, exist_ok=True)

        with Image.open(image_path) as img:
            if img.mode in ("RGBA", "LA", "P"):
                background = Image.new("RGB", img.size, (255, 255, 255))
                if img.mode == "P":
                    img = img.convert("RGBA")
                background.paste(img, mask=img.getchannel("A") if "A" in img.getbands() else None)
                img = background
            elif img.mode != "RGB":
                img = img.convert("RGB")

            img.thumbnail(size, Image.Resampling.LANCZOS)
            img.save(thumbnail_path, "JPEG", quality=85, optimize=True)

        logger.info(f"Generated thumbnail: {thumbnail_path}")
        return thumbnail_path

    def _get_storage_path(self, file_hash: str, extension: str) -> Path:
        prefix1 = file_hash[:2]
        prefix2 = file_hash[2:4]
        filename = f"{file_hash}{extension}"
        return self.documents_path / prefix1 / prefix2 / filename
