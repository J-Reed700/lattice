from datetime import timedelta
from io import BytesIO
import logging
from typing import BinaryIO

from minio import Minio
from minio.error import S3Error

from .config import MinioConfig

logger = logging.getLogger(__name__)


class MinioStorageClient:
    def __init__(self, config: MinioConfig):
        self.config = config
        self.client = Minio(
            endpoint=config.endpoint,
            access_key=config.access_key,
            secret_key=config.secret_key,
            secure=config.secure,
            region=config.region,
        )
        self.files_bucket = config.files_bucket
        self.thumbnails_bucket = config.thumbnails_bucket

        self._ensure_buckets()

    def _ensure_buckets(self):
        try:
            if not self.client.bucket_exists(self.files_bucket):
                self.client.make_bucket(self.files_bucket)
                logger.info(f"Created bucket: {self.files_bucket}")

            if not self.client.bucket_exists(self.thumbnails_bucket):
                self.client.make_bucket(self.thumbnails_bucket)
                logger.info(f"Created bucket: {self.thumbnails_bucket}")
        except S3Error as e:
            logger.error(f"Failed to ensure buckets exist: {e}")
            raise

    async def upload_file(
        self, file_hash: str, file_data: BinaryIO, content_type: str, file_size: int
    ) -> str:
        if await self.file_exists(file_hash):
            logger.info(f"File already exists, skipping upload: {file_hash}")
            return f"s3://{self.files_bucket}/files/{file_hash}"

        object_name = f"files/{file_hash}"

        try:
            self.client.put_object(
                bucket_name=self.files_bucket,
                object_name=object_name,
                data=file_data,
                length=file_size,
                content_type=content_type,
                metadata={"x-amz-server-side-encryption": "AES256"},
            )

            logger.info(f"Uploaded file: {object_name}")
            return f"s3://{self.files_bucket}/{object_name}"
        except S3Error as e:
            logger.error(f"Failed to upload file {file_hash}: {e}")
            raise

    async def upload_thumbnail(self, file_hash: str, thumbnail_data: bytes) -> str:
        thumbnail_hash = f"{file_hash}_thumb"
        object_name = f"thumbnails/{thumbnail_hash}.jpg"

        try:
            self.client.put_object(
                bucket_name=self.thumbnails_bucket,
                object_name=object_name,
                data=BytesIO(thumbnail_data),
                length=len(thumbnail_data),
                content_type="image/jpeg",
                metadata={"x-amz-server-side-encryption": "AES256"},
            )

            logger.info(f"Uploaded thumbnail: {object_name}")
            return f"s3://{self.thumbnails_bucket}/{object_name}"
        except S3Error as e:
            logger.error(f"Failed to upload thumbnail for {file_hash}: {e}")
            raise

    async def download_file(self, cloud_url: str) -> bytes:
        bucket, key = self._parse_s3_url(cloud_url)

        try:
            response = self.client.get_object(bucket_name=bucket, object_name=key)
            data = response.read()
            response.close()
            response.release_conn()
            return data
        except S3Error as e:
            logger.error(f"Failed to download file {cloud_url}: {e}")
            raise

    async def stream_file(self, cloud_url: str):
        bucket, key = self._parse_s3_url(cloud_url)

        try:
            response = self.client.get_object(bucket_name=bucket, object_name=key)
            return response
        except S3Error as e:
            logger.error(f"Failed to stream file {cloud_url}: {e}")
            raise

    async def file_exists(self, file_hash: str) -> bool:
        try:
            self.client.stat_object(bucket_name=self.files_bucket, object_name=f"files/{file_hash}")
            return True
        except S3Error:
            return False

    async def get_presigned_url(self, cloud_url: str, expires_in: int = 3600) -> str:
        bucket, key = self._parse_s3_url(cloud_url)

        try:
            url = self.client.presigned_get_object(
                bucket_name=bucket, object_name=key, expires=timedelta(seconds=expires_in)
            )
            return url
        except S3Error as e:
            logger.error(f"Failed to generate presigned URL for {cloud_url}: {e}")
            raise

    async def delete_file(self, cloud_url: str) -> None:
        bucket, key = self._parse_s3_url(cloud_url)

        try:
            self.client.remove_object(bucket_name=bucket, object_name=key)
            logger.info(f"Deleted file: {cloud_url}")
        except S3Error as e:
            logger.error(f"Failed to delete file {cloud_url}: {e}")
            raise

    def _parse_s3_url(self, cloud_url: str) -> tuple[str, str]:
        if not cloud_url.startswith("s3://"):
            raise ValueError(f"Invalid S3 URL format: {cloud_url}")

        parts = cloud_url.replace("s3://", "").split("/", 1)
        if len(parts) != 2:
            raise ValueError(f"Invalid S3 URL format: {cloud_url}")

        bucket = parts[0]
        key = parts[1]
        return bucket, key
