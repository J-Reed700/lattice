from functools import lru_cache
from pathlib import Path
from typing import Literal

from pydantic import Field, field_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env", env_file_encoding="utf-8", case_sensitive=False, extra="ignore"
    )

    app_name: str = "Vault Backend"
    app_version: str = "1.0.0"
    debug: bool = False

    database_url: str = "postgresql+asyncpg://vault:vault@localhost:5432/vault"
    database_pool_size: int = 20
    database_max_overflow: int = 30
    database_pool_timeout: int = 30
    database_pool_recycle: int = 3600
    database_pool_pre_ping: bool = True
    database_pool_use_lifo: bool = True
    database_echo: bool = False

    electric_sync_url: str = "http://localhost:5133"
    electric_database_id: str = "vault"
    electric_token: str = ""
    electric_sync_interval: int = 5

    text_model_name: str = "dunzhang/stella_en_1.5B_v5"
    text_embedding_dim: int = 768
    image_model_name: str = "openai/clip-vit-base-patch32"
    image_embedding_dim: int = 512
    ml_batch_size: int = 32
    ml_device: str = "cpu"
    ml_cache_dir: str = "./models"

    storage_base_path: str = "./storage"
    storage_documents_path: str = "./storage/documents"
    storage_screenshots_path: str = "./storage/screenshots"
    storage_chunks_path: str = "./storage/chunks"
    storage_thumbnails_path: str = "./storage/thumbnails"
    storage_max_file_size: int = 100 * 1024 * 1024

    upload_dir: str = "./data/uploads"
    upload_max_file_size: int = 50 * 1024 * 1024

    search_default_limit: int = 20
    search_max_limit: int = 100
    search_min_score: float = 0.5
    search_chunk_size: int = 512
    search_chunk_overlap: int = 50

    enable_contextual_retrieval: bool = True
    context_prefix_template: str = "From document '{title}' about {topic}: {chunk}"

    indexing_batch_size: int = Field(
        default=100,
        ge=10,
        le=500,
        env="INDEXING_BATCH_SIZE",
        description="Batch size for database insert operations during indexing",
    )
    indexing_embedding_batch_size: int = Field(
        default=32,
        ge=8,
        le=128,
        env="INDEXING_EMBEDDING_BATCH_SIZE",
        description="Batch size for embedding generation during indexing",
    )
    indexing_parallel_workers: int = Field(
        default=4,
        ge=1,
        le=16,
        env="INDEXING_PARALLEL_WORKERS",
        description="Number of parallel workers for concurrent file indexing",
    )
    indexing_use_batch_operations: bool = Field(
        default=True,
        env="INDEXING_USE_BATCH_OPERATIONS",
        description="Enable batch operations for faster indexing (5-10x speedup)",
    )

    hybrid_search_enabled: bool = True
    hybrid_default_strategy: Literal["rrf", "weighted"] = "rrf"
    hybrid_vector_weight: float = 0.7
    hybrid_bm25_weight: float = 0.3
    hybrid_rrf_k: int = 60
    bm25_corpus_auto_build: bool = True
    bm25_top_k: int = 100

    reranking_enabled: bool = Field(
        default=True,
        env="RERANKING_ENABLED",
        description="Enable cross-encoder reranking for improved precision",
    )
    reranking_model: Literal[
        "BAAI/bge-reranker-v2-m3",
        "cross-encoder/ms-marco-MiniLM-L-6-v2",
        "BAAI/bge-reranker-base",
        "BAAI/bge-reranker-large",
    ] = Field(
        default="BAAI/bge-reranker-v2-m3",
        env="RERANKING_MODEL",
        description="Cross-encoder model for reranking (bge-v2-m3 best for multilingual, ms-marco fastest)",
    )
    reranking_top_k_input: int = Field(
        default=100,
        ge=10,
        le=500,
        env="RERANKING_TOP_K_INPUT",
        description="Number of results to rerank (retrieve more, rerank top N)",
    )
    reranking_top_k_output: int = Field(
        default=50,
        ge=5,
        le=100,
        env="RERANKING_TOP_K_OUTPUT",
        description="Number of results to return after reranking",
    )
    reranking_timeout: float = Field(
        default=2.0,
        ge=0.5,
        le=10.0,
        env="RERANKING_TIMEOUT",
        description="Maximum time for reranking in seconds",
    )
    reranking_batch_size: int = Field(
        default=32,
        ge=8,
        le=128,
        env="RERANKING_BATCH_SIZE",
        description="Batch size for cross-encoder inference",
    )
    reranking_cache_enabled: bool = Field(
        default=True,
        env="RERANKING_CACHE_ENABLED",
        description="Cache reranking results for repeated queries",
    )
    reranking_cache_ttl: int = Field(
        default=3600,
        ge=60,
        le=86400,
        env="RERANKING_CACHE_TTL",
        description="Cache TTL in seconds (1 hour default)",
    )
    reranking_max_content_length: int = Field(
        default=2000,
        ge=500,
        le=10000,
        env="RERANKING_MAX_CONTENT_LENGTH",
        description="Maximum characters to read from each document for reranking",
    )

    api_host: str = "0.0.0.0"
    api_port: int = 8000
    api_prefix: str = "/api/v1"
    api_workers: int = 4
    api_reload: bool = False
    enable_docs: bool = True

    cors_origins: list[str] = ["http://localhost:3000", "http://localhost:5173"]
    cors_allow_credentials: bool = True
    cors_allow_methods: list[str] = ["GET", "POST", "PUT", "DELETE"]
    cors_allow_headers: list[str] = [
        "Authorization",
        "Content-Type",
        "X-CSRF-Token",
        "X-Request-ID",
    ]

    log_level: str = "INFO"
    log_format: str = "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    log_file: str = "./logs/vault.log"
    log_rotation: str = "100 MB"
    log_retention: str = "30 days"

    jwt_secret_key: str = Field(..., min_length=32)
    jwt_refresh_secret_key: str = Field(..., min_length=32)
    jwt_algorithm: str = "HS256"
    jwt_access_token_expire_minutes: int = 30
    jwt_refresh_token_expire_days: int = 7

    # Rate Limiting
    rate_limiting_enabled: bool = Field(default=True, env="RATE_LIMITING_ENABLED")
    redis_url: str = Field(default="redis://localhost:6379/1", env="REDIS_URL")

    # Rate limit values
    rate_limit_login_per_minute: int = Field(default=5, env="RATE_LIMIT_LOGIN_PER_MINUTE")
    rate_limit_login_per_hour: int = Field(default=10, env="RATE_LIMIT_LOGIN_PER_HOUR")
    rate_limit_register_per_hour: int = Field(default=3, env="RATE_LIMIT_REGISTER_PER_HOUR")
    rate_limit_file_upload_per_minute: int = Field(
        default=10, env="RATE_LIMIT_FILE_UPLOAD_PER_MINUTE"
    )
    rate_limit_search_per_minute: int = Field(default=60, env="RATE_LIMIT_SEARCH_PER_MINUTE")

    ocr_enabled: bool = True
    ocr_languages: list[str] = ["eng"]
    ocr_dpi: int = 300

    memory_monitoring_enabled: bool = Field(
        default=True,
        env="MEMORY_MONITORING_ENABLED",
        description="Enable automatic memory monitoring and cleanup",
    )
    memory_threshold_mb: int = Field(
        default=8192,
        env="MEMORY_THRESHOLD_MB",
        description="Memory threshold in MB, triggers cleanup if exceeded",
    )
    memory_cleanup_interval: int = Field(
        default=50,
        env="MEMORY_CLEANUP_INTERVAL",
        description="Run cleanup every N operations (e.g., OCR inferences)",
    )

    screenshot_capture_interval: int = 300
    screenshot_quality: int = 85
    screenshot_format: str = "jpeg"

    ollama_base_url: str = "http://localhost:11434"
    ollama_timeout: int = 120
    ollama_default_model: str = "llama2"
    ollama_stream_timeout: int = 300
    ollama_max_parallel_requests: int = Field(
        default=3,
        ge=1,
        le=8,
        env="OLLAMA_MAX_PARALLEL_REQUESTS",
        description="Max concurrent Ollama requests to avoid server overload",
    )
    deep_research_max_parallel_search: int = Field(
        default=4,
        ge=1,
        le=12,
        env="DEEP_RESEARCH_MAX_PARALLEL_SEARCH",
        description="Max concurrent retrieval calls during deep research",
    )

    langchain_ollama_model: str = "llama3.2"
    langchain_ollama_temperature: float = 0.7
    langchain_enable_tracing: bool = False
    langchain_tracing_project: str = "recall-api"
    langchain_verbose: bool = False
    langchain_max_iterations: int = 10
    langchain_max_execution_time: float = 120.0

    file_watcher_enabled: bool = True
    file_watcher_debounce_seconds: float = 0.5
    file_watcher_num_workers: int = 3
    file_watcher_max_file_size: int = 100 * 1024 * 1024
    file_watcher_ignore_patterns: list[str] = [
        ".git",
        ".git/*",
        "node_modules",
        "node_modules/*",
        "__pycache__",
        "__pycache__/*",
        "*.pyc",
        "*.pyo",
        "*.pyd",
        ".DS_Store",
        "Thumbs.db",
        "*.tmp",
        "*.temp",
        "*.swp",
        "*.swx",
        "~*",
        ".~*",
        "*.lock",
        "*.log",
    ]
    file_watcher_file_type_filters: list[str] = [
        "*.txt",
        "*.md",
        "*.pdf",
        "*.docx",
        "*.doc",
        "*.png",
        "*.jpg",
        "*.jpeg",
        "*.gif",
        "*.bmp",
        "*.py",
        "*.js",
        "*.ts",
        "*.jsx",
        "*.tsx",
        "*.html",
        "*.css",
        "*.json",
        "*.xml",
        "*.yaml",
    ]

    data_dir: Path = Path.home() / ".recall"
    watch_dirs: list[str] = []
    chunk_size: int = 512
    chunk_overlap: int = 50
    embedding_model: str = "dunzhang/stella_en_1.5B_v5"
    llm_model: str = "llama3.1:8b"
    file_extensions: list[str] = [
        ".txt",
        ".md",
        ".pdf",
        ".docx",
        ".py",
        ".js",
        ".ts",
        ".html",
        ".css",
    ]

    csrf_enabled: bool = Field(default=True, env="CSRF_ENABLED")
    csrf_token_length: int = Field(default=32, env="CSRF_TOKEN_LENGTH")
    csrf_cookie_secure: bool = Field(default=True, env="CSRF_COOKIE_SECURE")
    csrf_cookie_samesite: str = Field(default="Strict", env="CSRF_COOKIE_SAMESITE")
    csrf_cookie_httponly: bool = Field(default=False, env="CSRF_COOKIE_HTTPONLY")
    csrf_cookie_domain: str | None = Field(default=None, env="CSRF_COOKIE_DOMAIN")
    csrf_exempt_paths: list[str] = Field(
        default=[
            "/api/v1/auth/token",
            "/api/v1/auth/register",
            "/api/v1/health",
            "/api/docs",
            "/api/redoc",
            "/api/openapi.json",
        ],
        env="CSRF_EXEMPT_PATHS",
    )

    @field_validator("jwt_secret_key")
    @classmethod
    def validate_jwt_secret_key(cls, v: str) -> str:
        """Validate JWT secret key with strong security requirements."""
        weak_patterns = [
            "your-secret-key",
            "change-in-production",
            "REPLACE_WITH",
            "test-secret",
            "development-key",
        ]

        for pattern in weak_patterns:
            if pattern.lower() in v.lower():
                raise ValueError(
                    f"JWT_SECRET_KEY contains weak pattern '{pattern}'. "
                    "Generate secure key: python -c 'import secrets; print(secrets.token_urlsafe(32))'"
                )

        if len(v) < 32:
            raise ValueError("JWT_SECRET_KEY must be at least 32 characters")

        if len(set(v)) < 16:
            raise ValueError("JWT_SECRET_KEY has insufficient entropy (too repetitive)")

        return v

    @field_validator("jwt_refresh_secret_key")
    @classmethod
    def validate_jwt_refresh_secret_key(cls, v: str, info) -> str:
        """Validate JWT refresh secret key with strong security requirements."""
        weak_patterns = [
            "your-secret-key",
            "change-in-production",
            "REPLACE_WITH",
            "test-secret",
            "development-key",
        ]

        for pattern in weak_patterns:
            if pattern.lower() in v.lower():
                raise ValueError(
                    f"JWT_REFRESH_SECRET_KEY contains weak pattern '{pattern}'. "
                    "Generate secure key: python -c 'import secrets; print(secrets.token_urlsafe(32))'"
                )

        if len(v) < 32:
            raise ValueError("JWT_REFRESH_SECRET_KEY must be at least 32 characters")

        if len(set(v)) < 16:
            raise ValueError("JWT_REFRESH_SECRET_KEY has insufficient entropy (too repetitive)")

        # Ensure refresh secret is different from access secret
        if "jwt_secret_key" in info.data and v == info.data["jwt_secret_key"]:
            raise ValueError(
                "JWT_REFRESH_SECRET_KEY must be different from JWT_SECRET_KEY. "
                "Using same secret for both tokens is a security vulnerability."
            )

        return v

    @field_validator("database_pool_size")
    @classmethod
    def validate_pool_size(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("Pool size must be positive")
        if v > 100:
            raise ValueError("Pool size too large (max 100)")
        return v

    @field_validator("search_min_score")
    @classmethod
    def validate_search_min_score(cls, v: float) -> float:
        if not 0 <= v <= 1:
            raise ValueError("Search minimum score must be between 0 and 1")
        return v

    @field_validator("storage_max_file_size", "upload_max_file_size")
    @classmethod
    def validate_file_size(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("File size limit must be positive")
        if v > 1024 * 1024 * 1024:
            raise ValueError("File size limit too large (max 1GB)")
        return v

    @field_validator("api_port")
    @classmethod
    def validate_api_port(cls, v: int) -> int:
        if not 1 <= v <= 65535:
            raise ValueError("API port must be between 1 and 65535")
        return v

    @field_validator("api_workers")
    @classmethod
    def validate_api_workers(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("API workers must be positive")
        if v > 32:
            raise ValueError("API workers too large (max 32)")
        return v

    @field_validator("jwt_access_token_expire_minutes")
    @classmethod
    def validate_access_token_expire(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("Access token expiration must be positive")
        if v > 1440:
            raise ValueError("Access token expiration too long (max 24 hours)")
        return v

    @field_validator("file_watcher_debounce_seconds")
    @classmethod
    def validate_debounce_seconds(cls, v: float) -> float:
        if v < 0.1:
            raise ValueError("Debounce seconds must be at least 0.1")
        if v > 10.0:
            raise ValueError("Debounce seconds too large (max 10.0)")
        return v

    @field_validator("file_watcher_num_workers")
    @classmethod
    def validate_num_workers(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("Number of workers must be positive")
        if v > 10:
            raise ValueError("Number of workers too large (max 10)")
        return v

    @field_validator("file_watcher_max_file_size")
    @classmethod
    def validate_watcher_file_size(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("File size limit must be positive")
        if v > 1024 * 1024 * 1024:
            raise ValueError("File size limit too large (max 1GB)")
        return v

    @field_validator("cors_origins")
    @classmethod
    def validate_cors_origins(cls, v: list[str]) -> list[str]:
        if "*" in v:
            raise ValueError(
                "Wildcard CORS origins not allowed. Specify explicit origins for security."
            )
        return v

    @field_validator("log_level")
    @classmethod
    def validate_log_level(cls, v: str) -> str:
        valid_levels = ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]
        if v.upper() not in valid_levels:
            raise ValueError(f"Log level must be one of: {', '.join(valid_levels)}")
        return v.upper()

    @field_validator("csrf_token_length")
    @classmethod
    def validate_csrf_token_length(cls, v: int) -> int:
        if v < 16:
            raise ValueError("CSRF token length must be at least 16")
        if v > 64:
            raise ValueError("CSRF token length too large (max 64)")
        return v

    @field_validator("csrf_cookie_samesite")
    @classmethod
    def validate_csrf_cookie_samesite(cls, v: str) -> str:
        valid_values = ["Strict", "Lax", "None"]
        if v not in valid_values:
            raise ValueError(f"CSRF cookie SameSite must be one of: {', '.join(valid_values)}")
        return v

    @field_validator("ml_batch_size")
    @classmethod
    def validate_ml_batch_size(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("ml_batch_size must be greater than 0")
        return v

    @field_validator("memory_threshold_mb")
    @classmethod
    def validate_memory_threshold(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("memory_threshold_mb must be positive")
        if v < 1024:
            raise ValueError("memory_threshold_mb too low (min 1GB)")
        if v > 64 * 1024:
            raise ValueError("memory_threshold_mb too high (max 64GB)")
        return v

    @field_validator("memory_cleanup_interval")
    @classmethod
    def validate_memory_cleanup_interval(cls, v: int) -> int:
        if v <= 0:
            raise ValueError("memory_cleanup_interval must be positive")
        if v > 1000:
            raise ValueError("memory_cleanup_interval too large (max 1000)")
        return v

    def ensure_storage_paths(self) -> None:
        paths = [
            self.storage_base_path,
            self.storage_documents_path,
            self.storage_screenshots_path,
            self.storage_chunks_path,
            self.storage_thumbnails_path,
            Path(self.log_file).parent,
            self.ml_cache_dir,
            self.upload_dir,
        ]
        for path in paths:
            Path(path).mkdir(parents=True, exist_ok=True)

    @property
    def db_path(self) -> Path:
        """Database file path."""
        return self.data_dir / "recall.db"

    @property
    def vector_index_path(self) -> Path:
        """FAISS index file path."""
        return self.data_dir / "index.faiss"

    def ensure_data_dir(self) -> None:
        """Create data directory if it doesn't exist."""
        self.data_dir.mkdir(parents=True, exist_ok=True)


@lru_cache
def get_settings() -> Settings:
    settings = Settings()
    settings.ensure_storage_paths()
    return settings
