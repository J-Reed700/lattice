from dataclasses import dataclass
import os


@dataclass
class MinioConfig:
    endpoint: str
    access_key: str
    secret_key: str
    secure: bool = True
    files_bucket: str = "vault-files"
    thumbnails_bucket: str = "vault-thumbnails"
    region: str | None = None

    @classmethod
    def from_env(cls) -> "MinioConfig":
        endpoint = os.getenv("MINIO_ENDPOINT", "localhost:9000")
        access_key = os.getenv("MINIO_ACCESS_KEY", "minioadmin")
        secret_key = os.getenv("MINIO_SECRET_KEY", "minioadmin")
        secure = os.getenv("MINIO_SECURE", "false").lower() == "true"
        files_bucket = os.getenv("MINIO_FILES_BUCKET", "vault-files")
        thumbnails_bucket = os.getenv("MINIO_THUMBNAILS_BUCKET", "vault-thumbnails")
        region = os.getenv("MINIO_REGION")

        return cls(
            endpoint=endpoint,
            access_key=access_key,
            secret_key=secret_key,
            secure=secure,
            files_bucket=files_bucket,
            thumbnails_bucket=thumbnails_bucket,
            region=region,
        )
