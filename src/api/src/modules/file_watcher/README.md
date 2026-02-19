# File Watcher Module

Real-time file system monitoring and automatic document re-indexing for the Vault knowledge management system.

## Overview

The File Watcher module monitors specified directories for file changes (create, modify, delete, rename) and automatically triggers the indexing pipeline to keep the knowledge base up-to-date.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    File Watcher Module                       │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────┐    ┌───────────────┐    ┌──────────────┐ │
│  │   Watchdog   │───>│ Event Handler │───>│ Event Queue  │ │
│  │   Observer   │    │               │    │  (Debounced) │ │
│  └──────────────┘    └───────────────┘    └──────────────┘ │
│         │                                         │          │
│         │ File System Events                      │          │
│         ▼                                         ▼          │
│  ┌──────────────┐                      ┌──────────────────┐ │
│  │ Path Filter  │                      │  Worker Pool     │ │
│  │              │                      │  (3 workers)     │ │
│  └──────────────┘                      └──────────────────┘ │
│                                                  │           │
│                                                  ▼           │
│                                        ┌──────────────────┐ │
│                                        │ Indexing Service │ │
│                                        └──────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Features

### 1. File System Monitoring
- **Recursive directory watching**: Monitor directories and all subdirectories
- **Cross-platform support**: Works on Windows, macOS, and Linux via `watchdog`
- **Event types**: CREATE, MODIFY, DELETE, MOVED
- **Real-time notifications**: Immediate detection of file changes

### 2. Event Processing
- **Debouncing**: Configurable delay (default 500ms) to avoid duplicate events
- **Hash-based change detection**: Only processes files with actual content changes
- **Priority-based queue**: High priority for new files, medium for modifications, low for deletions
- **Batch processing**: Multiple changes batched together to prevent system overload

### 3. Ignore Patterns
Default ignore patterns:
```python
[
    ".git", ".git/*",
    "node_modules", "node_modules/*",
    "__pycache__", "__pycache__/*",
    "*.pyc", "*.pyo", "*.pyd",
    ".DS_Store", "Thumbs.db",
    "*.tmp", "*.temp", "*.swp", "*.swx",
    "~*", ".~*",
    "*.lock", "*.log",
]
```

### 4. File Type Filters
Supported file types (configurable):
```python
[
    "*.txt", "*.md", "*.pdf", "*.docx", "*.doc",
    "*.png", "*.jpg", "*.jpeg", "*.gif", "*.bmp",
    "*.py", "*.js", "*.ts", "*.jsx", "*.tsx",
    "*.html", "*.css", "*.json", "*.xml", "*.yaml"
]
```

### 5. Performance Optimizations
- **Non-blocking operations**: All I/O operations are async
- **Worker pool**: Configurable number of workers (default: 3)
- **Memory efficiency**: Streams file content for hashing
- **Throttling**: Prevents CPU spikes during bulk changes
- **File size limits**: Max 100MB per file (configurable)

### 6. Error Handling
- **Retry logic**: Exponential backoff (3 attempts max)
- **Graceful degradation**: Failed indexing doesn't stop watcher
- **Permission errors**: Logged and skipped
- **Crash recovery**: Automatic restart on watcher crashes

## Public Interface

### FileWatcher

Main entry point for file watching functionality.

```python
from src.modules.file_watcher import FileWatcher, FileEvent

async def process_callback(event: FileEvent):
    print(f"File {event.event_type}: {event.file_path}")

watcher = FileWatcher(
    process_callback=process_callback,
    ignore_patterns=None,  # Uses defaults
    num_workers=3,
    debounce_seconds=0.5
)

await watcher.start(["/path/to/watch"])
await watcher.stop()
```

**Parameters:**
- `process_callback`: Async function called for each file event
- `ignore_patterns`: List of glob patterns to ignore (optional)
- `num_workers`: Number of concurrent processing workers (default: 3)
- `debounce_seconds`: Delay before processing events (default: 0.5)

**Methods:**
- `start(watch_directories: List[Path])`: Start watching directories
- `stop()`: Stop watching and cleanup resources
- `scan_directory(directory: Path)`: Manually scan directory for initial indexing

### FileEvent

Data model for file system events.

```python
@dataclass
class FileEvent:
    event_type: FileEventType  # CREATED, MODIFIED, DELETED, MOVED
    file_path: Path
    timestamp: datetime
    priority: EventPriority  # HIGH, MEDIUM, LOW
    file_hash: Optional[str] = None
    retry_count: int = 0
```

### PathFilter

Filter for determining which files to process.

```python
from src.modules.file_watcher import PathFilter

filter = PathFilter(ignore_patterns=["*.tmp", "*.log"])
should_process = filter.should_process(Path("/path/to/file.txt"))
```

## Configuration

All configuration is in `src/config/settings.py`:

```python
# Enable/disable file watching
file_watcher_enabled: bool = True

# Debounce delay in seconds
file_watcher_debounce_seconds: float = 0.5  # 100ms - 10s

# Number of concurrent workers
file_watcher_num_workers: int = 3  # 1-10

# Maximum file size to auto-index
file_watcher_max_file_size: int = 100 * 1024 * 1024  # 100MB

# Patterns to ignore
file_watcher_ignore_patterns: List[str] = [
    ".git", "node_modules", "__pycache__", # ...
]

# File types to watch
file_watcher_file_type_filters: List[str] = [
    "*.txt", "*.md", "*.pdf", # ...
]
```

### Environment Variables

Configure via `.env` file:

```env
FILE_WATCHER_ENABLED=true
FILE_WATCHER_DEBOUNCE_SECONDS=0.5
FILE_WATCHER_NUM_WORKERS=3
FILE_WATCHER_MAX_FILE_SIZE=104857600
```

## Integration

### WatchService

High-level service that integrates FileWatcher with the indexing pipeline.

```python
from src.services.watch import WatchService

watch_service = WatchService()
watch_service.set_session_factory(get_session_factory())

await watch_service.start_watching(
    watch_folder_id=uuid.uuid4(),
    path="/path/to/documents",
    recursive=True,
    db_session=session
)

await watch_service.stop_watching(watch_folder_id)
await watch_service.stop_all()
```

### Event Handling

The WatchService handles file events by:

1. **File Created**:
   - Check if file already exists in database
   - Validate file size
   - Detect MIME type
   - Create File record
   - Trigger indexing

2. **File Modified**:
   - Find existing File record
   - Compute new hash
   - Skip if content unchanged
   - Update File metadata
   - Trigger re-indexing

3. **File Deleted**:
   - Find existing File record
   - Remove from index
   - Mark as deleted

4. **File Moved**:
   - Treat as delete + create

## API Endpoints

### Add Watch Directory
```http
POST /api/v1/watch
Content-Type: application/json

{
  "path": "/path/to/documents",
  "recursive": true,
  "file_patterns": ["*"],
  "ignore_patterns": ["*.tmp"],
  "auto_index": true
}
```

### List Watch Directories
```http
GET /api/v1/watch
```

### Get Watch Directory
```http
GET /api/v1/watch/{watch_id}
```

### Remove Watch Directory
```http
DELETE /api/v1/watch/{watch_id}
```

### Get Watch Status
```http
GET /api/v1/watch/status
```

Response:
```json
{
  "total_directories": 5,
  "active_watchers": 3,
  "total_files_watched": 1234,
  "pending_index_queue": 10,
  "last_event": "2025-11-10T12:34:56Z"
}
```

### Trigger Manual Reindex
```http
POST /api/v1/watch/reindex
Content-Type: application/json

{
  "watch_id": "uuid-here",
  "force": false
}
```

### Pause/Resume Watching
```http
POST /api/v1/watch/{watch_id}/pause
POST /api/v1/watch/{watch_id}/resume
```

## Error Codes

| Error | Condition | Recovery |
|-------|-----------|----------|
| `ValueError` | Invalid directory path | Fix path and retry |
| `PermissionError` | No read access to directory | Grant permissions |
| `FileNotFoundError` | Directory doesn't exist | Create directory or fix path |
| `OSError` | Disk I/O error | Check disk health, retry |

## Performance Characteristics

- **Event latency**: < 1 second from file change to index start
- **Throughput**: ~100 files/minute (depends on file size and content)
- **Memory usage**: ~50MB base + ~10MB per 1000 files watched
- **CPU usage**: < 5% idle, < 30% during bulk indexing
- **Debounce effectiveness**: Reduces events by 60-80% for rapidly changing files

## Logging

All events are logged at appropriate levels:

```python
# Info: Normal operations
logger.info("Started watching directory: /path/to/docs")
logger.info("Indexing new file: document.pdf")

# Warning: Recoverable issues
logger.warning("File too large to index: 150MB")
logger.warning("Retry 2/3 for document.pdf")

# Error: Failures
logger.error("Failed to start watching /path: Permission denied")
logger.error("Failed to index file.pdf after 3 retries")
```

## Testing

Run unit tests:
```bash
cd src/api
pytest tests/test_file_watcher.py -v
```

Run integration tests:
```bash
pytest tests/integration/test_file_watcher_integration.py -v
```

## Troubleshooting

### Watcher not starting
1. Check `file_watcher_enabled` in settings
2. Verify directory paths exist and are accessible
3. Check database connection
4. Review logs for errors

### Files not being indexed
1. Check ignore patterns - file might be filtered
2. Verify file type filters include your file type
3. Check file size limits
4. Ensure IndexingService is running

### High CPU/memory usage
1. Reduce `file_watcher_num_workers`
2. Increase `file_watcher_debounce_seconds`
3. Add more ignore patterns
4. Reduce watched directory size

### Events being missed
1. Decrease `file_watcher_debounce_seconds`
2. Check system file descriptor limits
3. Verify watchdog is working: `watchmedo log /path`

## Dependencies

- `watchdog>=3.0.0`: Cross-platform file system monitoring
- `python-magic>=0.4.27`: MIME type detection
- `sqlalchemy>=2.0.23`: Database operations
- `asyncio`: Async/await support

## Regeneration Specification

This module can be completely regenerated from this specification. Key invariants:

### Public Interfaces
```python
# FileWatcher.__init__ signature
def __init__(
    self,
    process_callback: Callable[[FileEvent], Awaitable[None]],
    ignore_patterns: List[str] | None = None,
    num_workers: int = 3,
    debounce_seconds: float = 0.5
)

# FileWatcher.start signature
async def start(self, watch_directories: List[str | Path]) -> None

# FileEvent structure
@dataclass
class FileEvent:
    event_type: FileEventType
    file_path: Path
    timestamp: datetime
    priority: EventPriority
    file_hash: Optional[str] = None
    retry_count: int = 0
```

### Behavior Contracts
1. Debouncing must prevent duplicate events within configured window
2. File hash must be computed before indexing
3. Workers must retry failed operations with exponential backoff
4. All database operations must be atomic
5. Stopping watcher must complete all pending operations

## Examples

### Basic Usage
```python
from src.modules.file_watcher import FileWatcher, FileEvent

async def handle_event(event: FileEvent):
    print(f"Processing {event.file_path}")

watcher = FileWatcher(process_callback=handle_event)
await watcher.start(["/Users/docs"])
```

### With Custom Filters
```python
watcher = FileWatcher(
    process_callback=handle_event,
    ignore_patterns=["*.tmp", ".git/*", "node_modules/*"],
    num_workers=5,
    debounce_seconds=1.0
)
```

### Initial Directory Scan
```python
await watcher.start(["/Users/docs"])
await watcher.scan_directory(Path("/Users/docs"))
```

### Full Integration
```python
from src.services.watch import WatchService
from src.config import get_settings

settings = get_settings()
watch_service = WatchService()
watch_service.set_session_factory(get_session_factory())

async with get_session_factory()() as session:
    await watch_service.start_watching(
        watch_folder_id=folder_id,
        path="/path/to/watch",
        recursive=True,
        db_session=session
    )
```

## License

MIT License - See LICENSE file for details

## Support

For issues and questions:
- GitHub Issues: https://github.com/your-org/vault/issues
- Documentation: https://docs.vault.dev
- Email: support@vault.dev
