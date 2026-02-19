# Storage Manager API Reference

Complete API documentation for the storage management module.

## Core Classes

### StorageAnalyzer

Analyzes disk usage and provides detailed storage statistics.

#### `__init__(settings, config, db_session)`

Initialize the storage analyzer.

**Parameters:**
- `settings` (Settings): Application settings
- `config` (StorageManagerConfig, optional): Storage manager configuration
- `db_session` (AsyncSession, optional): Database session for querying metadata

**Example:**
```python
from src.modules.storage_manager import StorageAnalyzer
from src.modules.storage_manager.config import DEFAULT_CONFIG

analyzer = StorageAnalyzer(settings, DEFAULT_CONFIG, db_session)
```

#### `async get_storage_stats(refresh=False, include_breakdown=True, include_trends=False, include_largest=True) -> StorageStats`

Get comprehensive storage statistics.

**Parameters:**
- `refresh` (bool): Force recalculation instead of using cache
- `include_breakdown` (bool): Include detailed category breakdown
- `include_trends` (bool): Include historical growth data
- `include_largest` (bool): Include list of largest files

**Returns:**
- `StorageStats`: Complete storage statistics

**Raises:**
- `StorageAnalysisError`: If analysis fails

**Example:**
```python
stats = await analyzer.get_storage_stats(
    refresh=True,
    include_trends=True
)
print(f"Total: {stats.total_bytes / 1024**3:.2f} GB")
```

---

### StorageCleanup

Performs cleanup operations to reclaim disk space.

#### `__init__(settings, config, db_session)`

Initialize the storage cleanup manager.

**Parameters:**
- `settings` (Settings): Application settings
- `config` (StorageManagerConfig, optional): Storage manager configuration
- `db_session` (AsyncSession, optional): Database session

#### `async cleanup(request: CleanupRequest) -> CleanupResult`

Perform cleanup operation.

**Parameters:**
- `request` (CleanupRequest): Cleanup request with operation type and filters

**Returns:**
- `CleanupResult`: Result with files affected and space reclaimed

**Raises:**
- `CleanupError`: If cleanup operation fails

**Example:**
```python
request = CleanupRequest(
    operation=CleanupOperation.CACHE,
    dry_run=True,
    older_than_days=30
)
result = await cleanup.cleanup(request)
```

---

### StorageQuotaManager

Manages storage quotas and alerts.

#### `__init__(settings, config, db_session)`

Initialize the quota manager.

**Parameters:**
- `settings` (Settings): Application settings
- `config` (StorageManagerConfig, optional): Storage manager configuration
- `db_session` (AsyncSession, optional): Database session

#### `async set_quota(max_bytes, warning_at_percent=80, critical_at_percent=95, auto_cleanup_enabled=False, auto_cleanup_policy="oldest_first", auto_cleanup_target_percent=70) -> StorageQuota`

Set storage quota.

**Parameters:**
- `max_bytes` (int): Maximum storage allowed in bytes
- `warning_at_percent` (int): Warning threshold percentage (default: 80)
- `critical_at_percent` (int): Critical threshold percentage (default: 95)
- `auto_cleanup_enabled` (bool): Enable automatic cleanup (default: False)
- `auto_cleanup_policy` (str): Cleanup policy (default: "oldest_first")
- `auto_cleanup_target_percent` (int): Target usage after auto cleanup (default: 70)

**Returns:**
- `StorageQuota`: Configured storage quota

**Example:**
```python
quota = await quota_mgr.set_quota(
    max_bytes=50 * 1024**3,
    warning_at_percent=80,
    auto_cleanup_enabled=True
)
```

#### `async get_quota_status(current_bytes=None) -> QuotaStatus`

Get current quota status.

**Parameters:**
- `current_bytes` (int, optional): Current storage usage (calculated if not provided)

**Returns:**
- `QuotaStatus`: Quota status with usage and recommendations

**Example:**
```python
status = await quota_mgr.get_quota_status()
print(f"Status: {status.status}")  # "ok", "warning", "critical", "exceeded"
```

#### `async check_quota_before_index(file_size: int) -> bool`

Check if indexing a file would exceed quota.

**Parameters:**
- `file_size` (int): Size of file to be indexed in bytes

**Returns:**
- `bool`: True if file can be indexed, False if would exceed quota

**Raises:**
- `QuotaExceededError`: If quota already exceeded and auto-cleanup disabled

**Example:**
```python
can_index = await quota_mgr.check_quota_before_index(1024000)
if can_index:
    # Proceed with indexing
    pass
```

---

## Data Models

### StorageStats

Complete storage statistics.

**Attributes:**
- `total_bytes` (int): Total storage used across all categories
- `breakdown` (StorageBreakdown): Breakdown by storage category
- `by_file_type` (dict[str, int]): Storage grouped by file extension
- `by_date` (dict[str, int]): Storage grouped by indexing date ranges
- `largest_files` (list[FileInfo]): Top files by size
- `growth_trend` (list[TrendPoint]): Historical storage growth data points
- `last_calculated` (datetime): When these stats were calculated
- `cached` (bool): Whether stats are from cache

**Example:**
```python
stats = StorageStats(
    total_bytes=10737418240,
    breakdown=StorageBreakdown(...),
    by_file_type={"pdf": 3221225472},
    by_date={"this_week": 1073741824},
    largest_files=[],
    growth_trend=[],
    last_calculated=datetime.now(),
    cached=False
)
```

---

### StorageBreakdown

Storage breakdown by category.

**Attributes:**
- `original_files` (int): Space used by original uploaded/indexed files
- `embeddings` (int): Space used by vector embeddings
- `database` (int): Database file size
- `thumbnails` (int): Generated thumbnails and previews
- `cache` (int): Temporary cache files
- `logs` (int): Log files

**Properties:**
- `total` (int): Sum of all categories

**Example:**
```python
breakdown = StorageBreakdown(
    original_files=5368709120,
    embeddings=2147483648,
    database=1073741824,
    thumbnails=536870912,
    cache=268435456,
    logs=104857600
)
print(f"Total: {breakdown.total} bytes")
```

---

### CleanupRequest

Request to perform cleanup operation.

**Attributes:**
- `operation` (CleanupOperation): Type of cleanup to perform
- `dry_run` (bool): If True, only preview without deleting (default: True)
- `older_than_days` (int, optional): Only clean files older than N days
- `file_types` (list[str], optional): Only clean specific file types
- `min_size_bytes` (int, optional): Only clean files larger than threshold

**Example:**
```python
request = CleanupRequest(
    operation=CleanupOperation.ORPHANED_FILES,
    dry_run=True,
    older_than_days=30,
    min_size_bytes=1048576
)
```

---

### CleanupResult

Result of cleanup operation.

**Attributes:**
- `operation` (CleanupOperation): The cleanup operation performed
- `dry_run` (bool): Whether this was a preview
- `files_affected` (int): Number of files processed
- `space_reclaimed_bytes` (int): Space freed in bytes
- `errors` (list[str]): List of errors encountered
- `duration_seconds` (float): Operation duration
- `timestamp` (datetime): When operation completed
- `files` (list[FileInfo], optional): Detailed file list (dry_run mode)

**Example:**
```python
result = CleanupResult(
    operation=CleanupOperation.CACHE,
    dry_run=False,
    files_affected=42,
    space_reclaimed_bytes=536870912,
    errors=[],
    duration_seconds=2.5,
    timestamp=datetime.now()
)
```

---

### StorageQuota

Storage quota configuration.

**Attributes:**
- `max_bytes` (int): Maximum storage allowed in bytes
- `warning_at_percent` (int): Trigger warning at this percentage (default: 80)
- `critical_at_percent` (int): Trigger critical alert at this percentage (default: 95)
- `auto_cleanup_enabled` (bool): Enable automatic cleanup (default: False)
- `auto_cleanup_policy` (str): Policy for auto cleanup (default: "oldest_first")
- `auto_cleanup_target_percent` (int): Clean until usage is at this percentage (default: 70)

**Validation:**
- `max_bytes` must be > 0
- `critical_at_percent` must be > `warning_at_percent`
- `auto_cleanup_target_percent` must be < `warning_at_percent`

**Example:**
```python
quota = StorageQuota(
    max_bytes=50 * 1024**3,
    warning_at_percent=80,
    critical_at_percent=95,
    auto_cleanup_enabled=True
)
```

---

### QuotaStatus

Current quota status.

**Attributes:**
- `quota` (StorageQuota): The configured quota settings
- `current_bytes` (int): Current storage usage
- `percent_used` (float): Percentage of quota used
- `status` (str): Current status ("ok", "warning", "critical", "exceeded")
- `available_bytes` (int): Space remaining
- `needs_cleanup` (bool): Whether cleanup is recommended

**Example:**
```python
status = QuotaStatus(
    quota=quota,
    current_bytes=43046721536,
    percent_used=80.2,
    status="warning",
    available_bytes=10640369664,
    needs_cleanup=True
)
```

---

### FileInfo

Information about a single file.

**Attributes:**
- `path` (str): Full path to the file
- `size_bytes` (int): File size in bytes
- `modified_at` (datetime): Last modification timestamp
- `file_type` (str): File extension without dot
- `category` (str): Storage category (e.g., 'original_files', 'thumbnails')

**Example:**
```python
file = FileInfo(
    path="/storage/documents/report.pdf",
    size_bytes=1024000,
    modified_at=datetime.now(),
    file_type="pdf",
    category="original_files"
)
```

---

### CleanupOperation

Available cleanup operations (Enum).

**Values:**
- `ORPHANED_FILES`: Remove files in storage but not in database
- `CACHE`: Clear temporary cache files
- `THUMBNAILS`: Delete generated thumbnails
- `OLD_LOGS`: Remove old log files
- `VACUUM_DB`: Reclaim database space
- `OLD_EMBEDDINGS`: Delete old embedding vectors

**Example:**
```python
from src.modules.storage_manager import CleanupOperation

operation = CleanupOperation.ORPHANED_FILES
```

---

## Utility Functions

### `async get_storage_stats(settings=None, config=None, db_session=None, refresh=False) -> StorageStats`

Get storage statistics (convenience function).

**Parameters:**
- `settings` (Settings, optional): Application settings (uses default if not provided)
- `config` (StorageManagerConfig, optional): Storage manager config
- `db_session` (AsyncSession, optional): Database session
- `refresh` (bool): Force refresh instead of using cache

**Returns:**
- `StorageStats`: Complete storage statistics

**Example:**
```python
from src.modules.storage_manager import get_storage_stats

stats = await get_storage_stats(refresh=True)
```

---

### `async cleanup_orphaned_files(settings=None, config=None, db_session=None, dry_run=True, older_than_days=None) -> CleanupResult`

Remove orphaned files (convenience function).

**Parameters:**
- `settings` (Settings, optional): Application settings
- `config` (StorageManagerConfig, optional): Storage manager config
- `db_session` (AsyncSession, optional): Database session
- `dry_run` (bool): If True, only preview (default: True)
- `older_than_days` (int, optional): Only remove files older than N days

**Returns:**
- `CleanupResult`: Cleanup result with files affected and space reclaimed

**Example:**
```python
from src.modules.storage_manager import cleanup_orphaned_files

result = await cleanup_orphaned_files(dry_run=True, older_than_days=30)
```

---

### `async cleanup_cache(settings=None, config=None, db_session=None, dry_run=True, older_than_days=None) -> CleanupResult`

Clear cache directory (convenience function).

**Example:**
```python
from src.modules.storage_manager import cleanup_cache

result = await cleanup_cache(dry_run=False, older_than_days=7)
```

---

### `async vacuum_database(settings=None, config=None, db_session=None, dry_run=True) -> CleanupResult`

Vacuum database to reclaim space (convenience function).

**Example:**
```python
from src.modules.storage_manager import vacuum_database

result = await vacuum_database(dry_run=False)
```

---

### `async set_storage_quota(max_bytes, settings=None, config=None, db_session=None, warning_at_percent=80, critical_at_percent=95, auto_cleanup_enabled=False) -> QuotaStatus`

Set storage quota and get current status (convenience function).

**Example:**
```python
from src.modules.storage_manager import set_storage_quota

status = await set_storage_quota(
    max_bytes=50 * 1024**3,
    warning_at_percent=80
)
```

---

### `async check_quota_status(settings=None, config=None, db_session=None) -> QuotaStatus`

Check current quota status (convenience function).

**Example:**
```python
from src.modules.storage_manager import check_quota_status

status = await check_quota_status()
if status.needs_cleanup:
    print("Cleanup recommended")
```

---

### `format_bytes(bytes_value: int) -> str`

Format bytes as human-readable string.

**Parameters:**
- `bytes_value` (int): Number of bytes

**Returns:**
- `str`: Formatted string (e.g., "1.5 GB", "500 MB")

**Example:**
```python
from src.modules.storage_manager.utils import format_bytes

size_str = format_bytes(1536000000)  # "1.43 GB"
```

---

## Exceptions

### StorageError

Base exception for all storage manager errors.

**Example:**
```python
from src.modules.storage_manager import StorageError

raise StorageError("Generic storage error")
```

---

### StorageAnalysisError

Raised when storage analysis fails.

**Example:**
```python
from src.modules.storage_manager import StorageAnalysisError

raise StorageAnalysisError("Cannot access storage directory")
```

---

### CleanupError

Raised when cleanup operation fails.

**Example:**
```python
from src.modules.storage_manager import CleanupError

raise CleanupError("Permission denied while deleting file")
```

---

### QuotaExceededError

Raised when storage quota is exceeded.

**Attributes:**
- `current_bytes` (int): Current storage usage in bytes
- `quota_bytes` (int): Maximum allowed storage in bytes
- `overage_bytes` (int): Amount over quota in bytes

**Example:**
```python
from src.modules.storage_manager import QuotaExceededError

raise QuotaExceededError(
    "Storage quota exceeded",
    current_bytes=60 * 1024**3,
    quota_bytes=50 * 1024**3
)
```

---

## Configuration

### StorageManagerConfig

Storage manager configuration settings.

**Attributes:**
- `cache_ttl_seconds` (int): How long to cache storage stats (default: 300)
- `max_files_per_scan` (int): Maximum files to scan in single operation (default: 1000000)
- `cleanup_batch_size` (int): Number of files to process per batch (default: 1000)
- `trend_retention_days` (int): How many days of historical data to keep (default: 365)
- `default_quota_bytes` (int): Default storage quota (default: 50GB)
- `warning_threshold_percent` (int): Default warning threshold (default: 80)
- `critical_threshold_percent` (int): Default critical threshold (default: 95)
- `auto_cleanup_enabled` (bool): Enable automatic cleanup by default (default: False)
- `backup_before_cleanup` (bool): Create backup before destructive operations (default: True)
- `audit_log_retention_days` (int): How long to keep audit logs (default: 90)
- `parallel_scan_workers` (int): Number of parallel workers for scanning (default: 4)
- `skip_patterns` (list[str]): File patterns to skip during scanning

**Example:**
```python
from src.modules.storage_manager.config import StorageManagerConfig

config = StorageManagerConfig(
    cache_ttl_seconds=600,
    cleanup_batch_size=500,
    default_quota_bytes=100 * 1024**3
)
```
