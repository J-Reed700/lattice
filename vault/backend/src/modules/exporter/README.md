# Export Module

**A self-contained module for exporting knowledge base data in multiple formats.**

## Purpose

Provides comprehensive data export functionality allowing users to export their entire knowledge base or selected documents in various formats with full control over what data is included.

## Public Interface

```python
from modules.exporter import (
    ExportService,
    ExportFormat,
    ExportScope,
    ExportRequest,
    ExportResult
)
```

### Core Functions

#### `ExportService.create_export(request: ExportRequest) -> ExportResult`

Creates a new export job based on the provided request.

**Parameters:**
- `request` (ExportRequest): Export configuration including format, scope, and options

**Returns:**
- `ExportResult`: Export result with job ID and initial status

**Raises:**
- `ValueError`: If request validation fails

**Example:**
```python
async with get_session() as session:
    service = ExportService(session)

    request = ExportRequest(
        format=ExportFormat.JSON,
        scope=ExportScope.FULL,
        compress=True
    )

    result = await service.create_export(request)
    print(f"Export started: {result.export_id}")
```

#### `ExportService.get_export_status(export_id: UUID) -> Optional[ExportResult]`

Retrieves current status of an export job.

**Parameters:**
- `export_id` (UUID): Export job identifier

**Returns:**
- `ExportResult` or `None`: Current export status

**Example:**
```python
status = await service.get_export_status(export_id)
print(f"Progress: {status.progress_percent}%")
print(f"Files: {status.file_count}")
```

#### `ExportService.cancel_export(export_id: UUID) -> bool`

Cancels a pending or in-progress export.

**Parameters:**
- `export_id` (UUID): Export job identifier

**Returns:**
- `bool`: True if cancelled, False if already completed or not found

**Example:**
```python
cancelled = await service.cancel_export(export_id)
if cancelled:
    print("Export cancelled successfully")
```

## Data Models

### ExportFormat

Supported export formats:

- `JSON`: Complete structured export with all metadata
- `CSV`: Tabular format for spreadsheet analysis
- `MARKDOWN`: Human-readable format with folder structure
- `ZIP`: Bundle of original files plus metadata

### ExportScope

Export scope options:

- `FULL`: Export entire knowledge base
- `FILTERED`: Export with filters (date range, file types, tags, folders)
- `SEARCH_RESULTS`: Export current search results
- `SELECTED`: Export user-selected files only

### ExportRequest

```python
ExportRequest(
    format: ExportFormat,              # Required: Export format
    scope: ExportScope,                # Required: Export scope
    filters: Optional[ExportFilters],  # Optional: Filters for filtered scope
    file_ids: Optional[List[UUID]],    # Optional: File IDs for selected scope
    include_embeddings: bool = False,  # Include vector embeddings
    include_original_files: bool = True, # Include original files (ZIP only)
    compress: bool = True              # Compress output
)
```

### ExportResult

```python
ExportResult(
    export_id: UUID,                   # Unique export identifier
    status: ExportStatus,              # Current status
    format: ExportFormat,              # Export format used
    scope: ExportScope,                # Export scope
    file_count: int,                   # Number of files
    total_size_bytes: int,             # Total size
    output_path: Optional[str],        # Path to export file
    progress_percent: float,           # Progress (0-100)
    created_at: datetime,              # Creation timestamp
    completed_at: Optional[datetime],  # Completion timestamp
    error_message: Optional[str]       # Error message if failed
)
```

## Side Effects

- **File Writing**: Creates export files in configured export directory
- **Temporary Files**: Creates temporary files during processing
- **Auto-cleanup**: Deletes export files after 24 hours
- **Database Queries**: Reads file and content data from database
- **Memory Usage**: Loads file data into memory during processing

## Dependencies

- `sqlalchemy>=2.0`: Database access
- `pydantic>=2.0`: Data validation
- `pandas`: CSV generation (optional, using standard csv module instead)
- Built-in modules: `json`, `csv`, `zipfile`, `gzip`, `pathlib`

## Configuration

```python
# Default export directory
export_dir = Path(tempfile.gettempdir()) / "vault_exports"

# Export retention period
retention_hours = 24

# Supported file extensions
SUPPORTED_FORMATS = ["json", "csv", "markdown", "zip"]
```

## Error Handling

| Error Type | Condition | Recovery Strategy |
|------------|-----------|-------------------|
| ValueError | Invalid request (missing file_ids, filters) | Validate request before submission |
| FileNotFoundError | Export file not found for download | Check export status before downloading |
| PermissionError | Cannot write to export directory | Check directory permissions |
| MemoryError | Export too large for memory | Use streaming or split into smaller exports |

## Performance Characteristics

- **Time Complexity**: O(n) for n files being exported
- **Memory Usage**: ~100MB per 1000 files (varies by format)
- **Concurrent Exports**: Unlimited (runs as background tasks)
- **Max Export Size**: Limited by available disk space
- **Processing Speed**: ~100-500 files/second (depends on file size)

## Testing

```bash
# Run unit tests
pytest tests/unit/modules/test_exporter.py -v

# Run with coverage
pytest tests/unit/modules/test_exporter.py --cov=src.modules.exporter
```

## Regeneration Specification

This module can be regenerated from this specification alone.

**Key Invariants:**
- Public function signatures must remain unchanged
- Export file formats must be backward compatible
- Error types and conditions must be preserved
- Auto-cleanup behavior (24 hours) must be maintained

**Regeneration Checklist:**
- [ ] All format handlers implement BaseExportHandler
- [ ] ExportService maintains job tracking
- [ ] All exports include metadata
- [ ] Progress tracking updates every 2 seconds
- [ ] Auto-cleanup runs daily at 2 AM
- [ ] All public types are exported in `__init__.py`

## Usage Examples

### Basic Full Export

```python
from modules.exporter import ExportService, ExportRequest, ExportFormat, ExportScope

async with get_session() as session:
    service = ExportService(session)

    request = ExportRequest(
        format=ExportFormat.JSON,
        scope=ExportScope.FULL
    )

    result = await service.create_export(request)

    # Wait for completion
    while result.status == "in_progress":
        await asyncio.sleep(2)
        result = await service.get_export_status(result.export_id)

    print(f"Export completed: {result.output_path}")
```

### Filtered Export

```python
from datetime import datetime
from modules.exporter import ExportFilters

filters = ExportFilters(
    date_from=datetime(2024, 1, 1),
    date_to=datetime(2024, 12, 31),
    file_types=["pdf", "docx"],
    tags=["important", "work"]
)

request = ExportRequest(
    format=ExportFormat.CSV,
    scope=ExportScope.FILTERED,
    filters=filters,
    compress=True
)

result = await service.create_export(request)
```

### Selected Files Export

```python
# Export specific files
file_ids = [
    UUID("123e4567-e89b-12d3-a456-426614174000"),
    UUID("987e6543-e21b-98d7-b654-321654987000")
]

request = ExportRequest(
    format=ExportFormat.ZIP,
    scope=ExportScope.SELECTED,
    file_ids=file_ids,
    include_original_files=True
)

result = await service.create_export(request)
```

### With Background Task (Celery)

```python
from tasks.export_tasks import process_export_task

# Queue export as background task
request = ExportRequest(
    format=ExportFormat.JSON,
    scope=ExportScope.FULL
)

task = process_export_task.delay(request.model_dump())

# Get task result later
result = task.get(timeout=3600)  # Wait up to 1 hour
```

## API Integration

The export module is exposed via FastAPI endpoints at `/api/v1/export/`:

- `POST /api/v1/export/full` - Full export
- `POST /api/v1/export/filtered` - Filtered export
- `POST /api/v1/export/selected` - Selected files export
- `GET /api/v1/export/{export_id}/status` - Get export status
- `GET /api/v1/export/{export_id}/download` - Download export
- `DELETE /api/v1/export/{export_id}` - Cancel export
- `POST /api/v1/export/cleanup` - Cleanup old exports

See API documentation for detailed endpoint specifications.

## Troubleshooting

### Export fails with "file_ids required"

**Cause**: Using `SELECTED` scope without providing file IDs

**Solution**:
```python
request = ExportRequest(
    format=ExportFormat.JSON,
    scope=ExportScope.SELECTED,
    file_ids=[uuid1, uuid2, uuid3]  # Add file IDs
)
```

### Export file not found after completion

**Cause**: Export file was auto-cleaned after 24 hours

**Solution**: Download exports promptly or increase retention period

### Out of memory during large exports

**Cause**: Too many files loaded into memory at once

**Solution**: Use filtered exports to split into smaller batches:
```python
# Export by date range
for month in range(1, 13):
    filters = ExportFilters(
        date_from=datetime(2024, month, 1),
        date_to=datetime(2024, month, 28)
    )
    # Create export for each month
```

### Slow export performance

**Cause**: Large files or many embeddings

**Solution**: Disable embeddings if not needed:
```python
request = ExportRequest(
    format=ExportFormat.JSON,
    scope=ExportScope.FULL,
    include_embeddings=False  # Skip embeddings
)
```
