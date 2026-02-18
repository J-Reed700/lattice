# Vault Sync Server

Multi-device sync server with LLM capabilities for the Vault knowledge management system.

## Purpose

The sync server enables:
- **Multi-device synchronization** via ElectricSQL
- **Server-side search** for mobile/web clients
- **LLM Q&A endpoints** with RAG (for devices without local Ollama)
- **User authentication and isolation**
- **REST API** for all operations

## Architecture

The backend follows a modular "bricks and studs" design philosophy where each module is self-contained and regeneratable.

### Technology Stack

- **Framework:** FastAPI 0.104+
- **Database:** PostgreSQL 15+ with pgvector extension
- **Sync:** ElectricSQL
- **Embeddings:** sentence-transformers
- **LLM:** Ollama (or llama-cpp-python)
- **Migrations:** Alembic
- **Testing:** pytest
- **Package Manager:** Poetry
- **Code Quality:** Ruff + mypy

### Module Structure

```
src/
├── config/              # Application configuration
├── models/              # Database models and schemas
├── api/                 # REST API routes
│   ├── files.py         # File operations
│   ├── search.py        # Search endpoints
│   ├── llm.py           # LLM Q&A endpoints (NEW)
│   └── auth.py          # Authentication
├── services/            # Business logic
│   ├── search_service.py    # Semantic search
│   ├── llm_service.py       # LLM integration (NEW)
│   ├── sync_service.py      # ElectricSQL sync
│   └── embedding_service.py # Vector generation
├── modules/             # Core modules (legacy structure)
│   ├── file_watcher/    # File system monitoring
│   ├── content_extractor/  # Content extraction
│   ├── embedding_generator/  # Vector embeddings
│   ├── vector_store/    # Vector storage operations
│   └── search_engine/   # Semantic search
└── utils/               # Shared utilities
```

## Setup

### Prerequisites

- **Python 3.11+** - Modern Python with performance improvements
- **[Poetry](https://python-poetry.org/docs/#installation) 1.6+** - Dependency management
- **PostgreSQL 15+** - Database with pgvector extension
- **Git** - Version control

### 1. Install Poetry

```bash
# macOS/Linux/WSL
curl -sSL https://install.python-poetry.org | python3 -

# Windows (PowerShell)
(Invoke-WebRequest -Uri https://install.python-poetry.org -UseBasicParsing).Content | py -

# Verify installation
poetry --version
```

### 2. Quick Start (Docker Compose)

```bash
# Start all services (PostgreSQL, backend)
docker compose -f docker/docker-compose.dev.yml up

# Access API at http://localhost:8000
# API docs at http://localhost:8000/docs
```

### 3. Manual Setup

```bash
# Install all dependencies including dev tools
poetry install

# Install specific dependency groups
poetry install --with dev,test     # Development + testing
poetry install --only main,prod    # Production only

# Activate virtual environment (optional - Poetry handles this automatically)
poetry shell

# Copy environment template
cp .env.example .env

# Edit .env with your configuration
# Ensure PostgreSQL is running with pgvector extension

# Run database migrations
poetry run alembic upgrade head

# Download ML models (first time only)
poetry run python scripts/download_models.py

# Start development server with auto-reload
poetry run uvicorn src.main:app --reload --host 0.0.0.0 --port 8000
```

## Development Workflow

### Running Tests

```bash
# Run all tests with coverage
poetry run pytest

# Run specific test categories
poetry run pytest -m unit          # Unit tests only
poetry run pytest -m integration   # Integration tests only
poetry run pytest -m "not slow"    # Skip slow tests

# Run with detailed coverage report
poetry run pytest --cov=src --cov-report=html
open htmlcov/index.html  # View coverage report

# Run specific test file
poetry run pytest tests/unit/test_embeddings.py -v
```

### Code Quality

```bash
# Run Ruff linter (with auto-fix)
poetry run ruff check . --fix

# Run Ruff formatter
poetry run ruff format .

# Run type checking with mypy
poetry run mypy src/

# Run all quality checks
poetry run ruff check . && poetry run ruff format --check . && poetry run mypy src/
```

### Pre-commit Hooks

Set up pre-commit hooks to automatically check code quality before commits:

```bash
# Install pre-commit hooks
poetry run pre-commit install

# Run hooks manually on all files
poetry run pre-commit run --all-files

# Update hooks to latest versions
poetry run pre-commit autoupdate
```

### Database Migrations

```bash
# Create new migration (auto-detect changes)
poetry run alembic revision --autogenerate -m "Add user table"

# Apply all pending migrations
poetry run alembic upgrade head

# Rollback one migration
poetry run alembic downgrade -1

# View migration history
poetry run alembic history

# View current migration
poetry run alembic current
```

### Adding Dependencies

```bash
# Add production dependency
poetry add fastapi

# Add development dependency
poetry add --group dev pytest

# Add with version constraint
poetry add "sqlalchemy>=2.0,<3.0"

# Update dependencies
poetry update

# Update specific package
poetry update fastapi
```

## API Documentation

Once running, visit:
- **Swagger UI:** http://localhost:8000/docs
- **ReDoc:** http://localhost:8000/redoc
- **OpenAPI Schema:** http://localhost:8000/openapi.json

## LLM Features (NEW)

The sync server now includes LLM Q&A capabilities for clients that don't have local Ollama (like mobile devices).

### New API Endpoints

#### Ask Question (Streaming)

Stream LLM response with RAG context.

```bash
POST /api/v1/llm/ask
Content-Type: application/json

{
  "question": "What did I learn about machine learning?",
  "max_context_docs": 5
}
```

**Response:** `text/event-stream` (Server-Sent Events)

```
data: Neural
data:  networks
data:  are
data:  computational
...
data: [DONE]
```

**Parameters:**
- `question` (str): User question
- `max_context_docs` (int): Number of documents to use as context (default: 5)
- `user_id` (str): User identifier (from JWT token)

**How it works:**
1. Generate embedding for question
2. Search user's documents with pgvector
3. Build RAG prompt with top N documents
4. Stream response from Ollama
5. Return chunks to client

#### Health Check

Check if Ollama is available.

```bash
GET /api/v1/llm/health
```

**Response:**
```json
{
  "status": "ok",
  "ollama_available": true,
  "model": "llama2"
}
```

### Setup Ollama

The sync server requires Ollama for LLM features:

```bash
# Install Ollama (see https://ollama.ai)
# macOS/Linux
curl -fsSL https://ollama.ai/install.sh | sh

# Windows
# Download installer from https://ollama.ai/download

# Start Ollama server
ollama serve

# Pull a model (one-time)
ollama pull llama2
# or
ollama pull llama3.1
# or
ollama pull mistral
```

**Recommended models:**
- `llama2` (7B): Fast, good quality
- `llama3.1` (8B): Better quality, slightly slower
- `mistral` (7B): Excellent for RAG tasks
- `gemma` (7B): Good alternative

### Configuration

Add to `.env`:

```bash
# Ollama settings
OLLAMA_BASE_URL=http://localhost:11434
OLLAMA_MODEL=llama2
OLLAMA_TIMEOUT=120

# RAG settings
MAX_CONTEXT_DOCS=5
MAX_CONTEXT_TOKENS=2000
```

### Performance

- **Context retrieval:** <500ms (pgvector search)
- **First token:** <2s (Ollama dependent)
- **Streaming:** 10-50 tokens/second (model dependent)

### Scaling LLM

For production with multiple users:

1. **Multiple Ollama instances:**
```bash
# Start additional instances on different ports
ollama serve --port 11434
ollama serve --port 11435
ollama serve --port 11436

# Round-robin load balancing
OLLAMA_URLS=http://localhost:11434,http://localhost:11435,http://localhost:11436
```

2. **GPU acceleration:**
```bash
# Ollama automatically uses GPU if available
# NVIDIA GPUs: CUDA support built-in
# AMD GPUs: ROCm support on Linux
nvidia-smi  # Check GPU usage
```

3. **Cloud alternatives:**
```bash
# Use OpenAI-compatible API (Anthropic, OpenAI, etc.)
OLLAMA_BASE_URL=https://api.openai.com/v1
OLLAMA_API_KEY=sk-...
OLLAMA_MODEL=gpt-3.5-turbo
```

## Environment Variables

See `.env.example` for all configuration options.

### Key Variables

- `DATABASE_URL` - PostgreSQL connection string
- `ELECTRIC_URL` - ElectricSQL sync service URL
- `EMBEDDING_MODEL` - Model name for sentence-transformers (default: `all-MiniLM-L6-v2`)
- `WATCH_DIRECTORIES` - JSON array of directories to monitor
- `LOG_LEVEL` - Logging level (DEBUG, INFO, WARNING, ERROR)

### LLM Variables (NEW)

- `OLLAMA_BASE_URL` - Ollama server URL (default: `http://localhost:11434`)
- `OLLAMA_MODEL` - Model to use (default: `llama2`)
- `OLLAMA_TIMEOUT` - Request timeout in seconds (default: 120)
- `MAX_CONTEXT_DOCS` - Documents to use in RAG (default: 5)
- `MAX_CONTEXT_TOKENS` - Maximum context length (default: 2000)

## Project Structure

Each module is self-contained following the "bricks and studs" philosophy:

- **Clear public interface** (`__init__.py` with `__all__`)
- **Implementation files** (core logic)
- **Tests** in module-specific test directory
- **Documentation** in module README (where applicable)
- **Type hints** on all public functions
- **Docstrings** with examples

## Troubleshooting

### Poetry Installation Issues

```bash
# If Poetry is not in PATH, add it:
export PATH="$HOME/.local/bin:$PATH"  # Linux/macOS
# or add to ~/.bashrc or ~/.zshrc

# Windows: Poetry should be in %APPDATA%\Python\Scripts
```

### PostgreSQL Connection Issues

Ensure PostgreSQL is running and pgvector extension is installed:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

### Ollama Connection Issues

```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# If not running, start it
ollama serve

# Check logs
tail -f ~/.ollama/logs/server.log

# Test with simple request
curl http://localhost:11434/api/generate -d '{
  "model": "llama2",
  "prompt": "Hello"
}'
```

### Model Download Failures

Run the model download script manually with verbose output:

```bash
poetry run python scripts/download_models.py --verbose
```

### LLM Responses Too Slow

```bash
# Use smaller model
ollama pull llama2:7b-chat

# Or use quantized model (faster, lower quality)
ollama pull llama2:7b-chat-q4_0

# Enable GPU acceleration (if available)
# Ollama uses GPU automatically if detected
```

### Port Already in Use

Change the port in `docker-compose.dev.yml` or specify a different port:

```bash
poetry run uvicorn src.main:app --reload --port 8001
```

### Dependency Conflicts

```bash
# Clear Poetry cache and reinstall
poetry cache clear pypi --all
poetry install --no-cache
```

## Contributing

### Before Submitting

1. **Follow the modular design philosophy** - Keep modules self-contained
2. **Write tests** - Aim for >60% coverage
3. **Add type hints** - Use mypy strict mode
4. **Update documentation** - Keep README and docstrings current
5. **Run quality checks** - `poetry run pre-commit run --all-files`

### Code Style

- **Line length:** 100 characters
- **Formatter:** Ruff (automatic)
- **Import sorting:** Ruff with isort profile
- **Docstrings:** Google style
- **Type hints:** Required for all public functions

### Development Commands Cheatsheet

```bash
poetry install              # Install dependencies
poetry add <package>        # Add dependency
poetry run pytest           # Run tests
poetry run ruff check .     # Lint code
poetry run mypy src/        # Type check
poetry run pre-commit run   # Run all hooks
poetry shell                # Activate virtual environment
```

## License

MIT License - See LICENSE file for details
