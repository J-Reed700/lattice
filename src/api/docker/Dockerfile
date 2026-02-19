FROM python:3.11-slim

# Set environment variables
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1

# Set working directory
WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    gcc \
    g++ \
    libpq-dev \
    libmagic1 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy requirements files
COPY requirements/ /app/requirements/

# Install Python dependencies
RUN pip install --upgrade pip && \
    pip install -r requirements/prod.txt

# Copy application code
COPY src/ /app/src/
COPY migrations/ /app/migrations/
COPY scripts/ /app/scripts/
COPY alembic.ini /app/

# Create directories for models and cache
RUN mkdir -p /app/models /app/.cache

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Run the application
CMD ["uvicorn", "src.main:app", "--host", "0.0.0.0", "--port", "8000"]
