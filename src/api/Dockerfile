# Multi-stage Dockerfile for Vault Backend
# Optimized for production with minimal image size

# ================================================================================
# Stage 1: Builder - Install dependencies and build packages
# ================================================================================
FROM python:3.11-slim as builder

# Set working directory
WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    libpq-dev \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Poetry
ENV POETRY_VERSION=1.7.1 \
    POETRY_HOME=/opt/poetry \
    POETRY_NO_INTERACTION=1 \
    POETRY_VIRTUALENVS_CREATE=false

RUN curl -sSL https://install.python-poetry.org | python3 - && \
    ln -s /opt/poetry/bin/poetry /usr/local/bin/poetry

# Copy dependency files
COPY pyproject.toml poetry.lock* ./

# Install Python dependencies (production only)
RUN poetry export -f requirements.txt --output requirements.txt --without-hashes --without dev,test,docs && \
    pip install --no-cache-dir --upgrade pip setuptools wheel && \
    pip install --no-cache-dir --user -r requirements.txt

# ================================================================================
# Stage 2: Runtime - Minimal production image
# ================================================================================
FROM python:3.11-slim

# Set environment variables
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PATH=/home/vault/.local/bin:$PATH

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    # PostgreSQL client library
    libpq5 \
    # Tesseract OCR
    tesseract-ocr \
    tesseract-ocr-eng \
    # FFmpeg for video processing
    ffmpeg \
    # Image processing libraries
    libjpeg62-turbo \
    libpng16-16 \
    # For file type detection
    libmagic1 \
    # Utilities
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r vault --gid=1000 && \
    useradd -r -g vault --uid=1000 --home-dir=/home/vault --shell=/bin/bash vault && \
    mkdir -p /home/vault /app /data /app/models && \
    chown -R vault:vault /home/vault /app /data

# Set working directory
WORKDIR /app

# Copy Python packages from builder
COPY --from=builder /root/.local /home/vault/.local

# Copy application code
COPY --chown=vault:vault . .

# Create necessary directories
RUN mkdir -p /app/logs /app/tmp && \
    chown -R vault:vault /app/logs /app/tmp

# Switch to non-root user
USER vault

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Default command (can be overridden in docker-compose)
CMD ["uvicorn", "src.main:app", "--host", "0.0.0.0", "--port", "8000"]
