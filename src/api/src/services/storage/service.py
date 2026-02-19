import asyncio
import logging
from pathlib import Path

from .client import MinioStorageClient
from .config import MinioConfig
from .thumbnails import ThumbnailGenerator

logger = logging.getLogger(__name__)


class StorageService:
    def __init__(self, config: MinioConfig | None = None):
        if config is None:
            config = MinioConfig.from_env()

        self.client = MinioStorageClient(config)
        self.thumbnail_gen = ThumbnailGenerator()

    async def store_file(
        self, file_path: str, file_hash: str, mime_type: str
    ) -> tuple[str, str | None]:
        file_url = None
        thumbnail_url = None

        try:
            path = Path(file_path)
            if not path.exists():
                raise FileNotFoundError(f"File not found: {file_path}")

            file_size = path.stat().st_size

            with open(file_path, "rb") as f:
                file_url = await self.client.upload_file(
                    file_hash=file_hash,
                    file_data=f,
                    content_type=mime_type,
                    file_size=file_size,
                )

            if self._should_generate_thumbnail(mime_type):
                thumbnail_url = await self._generate_and_upload_thumbnail(
                    file_path, file_hash, mime_type
                )

            logger.info(f"Successfully stored file: {file_hash}")
            return file_url, thumbnail_url

        except Exception as e:
            logger.error(f"Failed to store file {file_path}: {e}")
            raise

    async def store_file_with_retry(
        self,
        file_path: str,
        file_hash: str,
        mime_type: str,
        max_retries: int = 3,
        retry_delay: float = 1.0,
    ) -> tuple[str, str | None]:
        last_error = None

        for attempt in range(max_retries):
            try:
                return await self.store_file(file_path, file_hash, mime_type)
            except Exception as e:
                last_error = e
                if attempt < max_retries - 1:
                    logger.warning(
                        f"Upload attempt {attempt + 1} failed, retrying in {retry_delay}s: {e}"
                    )
                    await asyncio.sleep(retry_delay)
                    retry_delay *= 2
                else:
                    logger.error(f"All {max_retries} upload attempts failed")

        raise last_error

    async def get_file(self, cloud_url: str) -> bytes:
        try:
            return await self.client.download_file(cloud_url)
        except Exception as e:
            logger.error(f"Failed to get file {cloud_url}: {e}")
            raise

    async def stream_file(self, cloud_url: str):
        try:
            return await self.client.stream_file(cloud_url)
        except Exception as e:
            logger.error(f"Failed to stream file {cloud_url}: {e}")
            raise

    async def file_exists(self, file_hash: str) -> bool:
        try:
            return await self.client.file_exists(file_hash)
        except Exception as e:
            logger.error(f"Failed to check file existence {file_hash}: {e}")
            return False

    async def get_presigned_url(self, cloud_url: str, expires_in: int = 3600) -> str:
        try:
            return await self.client.get_presigned_url(cloud_url, expires_in)
        except Exception as e:
            logger.error(f"Failed to get presigned URL for {cloud_url}: {e}")
            raise

    async def delete_file(self, cloud_url: str) -> None:
        try:
            await self.client.delete_file(cloud_url)
        except Exception as e:
            logger.error(f"Failed to delete file {cloud_url}: {e}")
            raise

    async def _generate_and_upload_thumbnail(
        self, file_path: str, file_hash: str, mime_type: str
    ) -> str | None:
        try:
            thumbnail_data = await self.thumbnail_gen.generate(file_path, mime_type)

            if thumbnail_data is None:
                logger.debug(f"No thumbnail generated for {file_path}")
                return None

            thumbnail_url = await self.client.upload_thumbnail(file_hash, thumbnail_data)
            return thumbnail_url

        except Exception as e:
            logger.warning(f"Failed to generate/upload thumbnail for {file_path}: {e}")
            return None

    def _should_generate_thumbnail(self, mime_type: str) -> bool:
        return (
            mime_type.startswith("image/")
            or mime_type.startswith("video/")
            or mime_type == "application/pdf"
        )
