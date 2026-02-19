# Storage Manager Module

A self-contained module for comprehensive disk usage tracking and cleanup operations in the Vault knowledge management system.

## Purpose

Provides users with full visibility and control over their data storage, including:
- Total disk usage analysis
- Breakdown by category (files, embeddings, database, thumbnails, cache, logs)
- Cleanup operations with safety mechanisms
- Storage quotas and alerts
- Historical trend tracking

## Contract

### Inputs

1. **Storage Analysis**
   - Type: `StorageAnalysisRequest`
   - Optional filters: date range, file type, category
   - Returns: `StorageStats` with detailed breakdown

2. **Cleanup Operations**
   - Type: `CleanupRequest`
   - Parameters: operation type, dry_run flag, filters
   - Returns: `CleanupResult` with files affected and space reclaimed

3. **Quota Management**
   - Type: `StorageQuota`
   - Parameters: max_bytes, warning_thresholds, auto_cleanup_policy
   - Side effects: Stores quota settings in database

### Outputs

**StorageStats:**
```python
{
    "total_bytes": 10737418240,  # 10GB
    "breakdown": {
        "original_files": 5368709120,
        "embeddings": 2147483648,
        "database": 1073741824,
        "thumbnails": 536870912,
        "cache": 268435456,
        "logs": 104857600
    },
    "by_file_type": {
        "pdf": 3221225472,
        "png": 2147483648,
        ...
    },
    "by_date": {
        "this_week": 1073741824,
        "this_month": 3221225472,
        ...
    },
    "largest_files": [
        {"path": "...", "size": 104857600, ...},
        ...
    ],
    "growth_trend": [
        {"date": "2025-11-03", "size": 9663676416},
        ...
    ],
    "last_calculated": "2025-11-10T12:00:00Z"
}
```

**CleanupResult:**
```python
{
    "operation": "remove_orphaned_files",
    "dry_run": false,
    "files_affected": 42,
    "space_reclaimed_bytes": 536870912,
    "errors": [],
    "duration_seconds": 2.5,
    "timestamp": "2025-11-10T12:00:00Z"
}
```

## Side Effects

- **File System**: Cleanup operations modify/delete files
- **Database**: Stores historical stats, quota settings, cleanup audit logs
- **Cache**: Updates cached statistics every 5 minutes
- **Background Jobs**: Triggers periodic storage calculations

## Dependencies

- **External**:
  - `psutil>=5.9.0`: Disk usage operations
  - `sqlalchemy>=2.0.23`: Database operations
  - `celery>=5.3.4`: Background tasks

- **Internal**:
  - `src.config.settings`: Configuration
  - `src.models`: Database models
  - `src.db`: Database session management

## Public Interface

```python
from storage_manager import (
    StorageAnalyzer,
    StorageCleanup,
    StorageQuotaManager,
    get_storage_stats,
    cleanup_orphaned_files,
    set_storage_quota
)

analyzer = StorageAnalyzer(settings)
stats = await analyzer.get_storage_stats(
    refresh=True,
    include_breakdown=True,
    include_trends=True
)

cleanup = StorageCleanup(settings)
result = await cleanup.remove_orphaned_files(
    dry_run=True,
    older_than_days=30
)

quota_manager = StorageQuotaManager(settings, db_session)
await quota_manager.set_quota(
    max_bytes=50 * 1024 * 1024 * 1024,  # 50GB
    warning_at_percent=80,
    auto_cleanup_enabled=True
)
```

## Error Handling

| Error Type | Condition | Recovery Strategy |
|------------|-----------|-------------------|
| `StorageAnalysisError` | Cannot calculate storage | Return cached stats with warning |
| `CleanupError` | Cleanup operation fails | Rollback, log error, return partial result |
| `QuotaExceededError` | Storage exceeds quota | Block new indexing, trigger auto-cleanup |
| `PermissionError` | Cannot access file/directory | Skip file, log warning, continue |
| `DatabaseError` | Cannot store stats | Cache in memory, retry later |

## Performance Characteristics

- **Storage Analysis**: O(n) where n = number of files, ~1-5 seconds for 100k files
- **Cached Lookups**: O(1), <100ms
- **Cleanup Operations**: O(m) where m = files to clean, ~0.1s per 1000 files
- **Database Queries**: Indexed, <500ms for stats retrieval
- **Memory Usage**: ~50MB for 100k file records
- **Concurrent Requests**: Thread-safe, supports up to 10 parallel analyses

## Configuration

```python
STORAGE_MANAGER_CONFIG = {
    "cache_ttl_seconds": 300,
    "max_files_per_scan": 1000000,
    "cleanup_batch_size": 1000,
    "trend_retention_days": 365,
    "default_quota_bytes": 50 * 1024 * 1024 * 1024,  # 50GB
    "warning_threshold_percent": 80,
    "critical_threshold_percent": 95,
    "auto_cleanup_enabled": False,
    "backup_before_cleanup": True,
    "audit_log_retention_days": 90
}
```

## Testing

```bash
# Run all storage manager tests
pytest src/api/src/modules/storage_manager/tests/

# Run contract validation
pytest src/api/src/modules/storage_manager/tests/test_contract.py

# Run documentation accuracy tests
pytest src/api/src/modules/storage_manager/tests/test_documentation.py

# Run with coverage
pytest --cov=storage_manager --cov-report=html
```

## Regeneration Specification

This module can be regenerated from this specification alone.

**Key Invariants:**
1. Public function signatures must remain stable
2. StorageStats and CleanupResult structures are versioned
3. Error types and conditions are documented
4. All cleanup operations support dry_run mode
5. Audit logging is mandatory for destructive operations
6. Quota checks happen before any file indexing

## Usage Examples

### Basic Storage Analysis

```python
from storage_manager import get_storage_stats

stats = await get_storage_stats()
print(f"Total storage: {stats.total_bytes / 1024**3:.2f} GB")
print(f"Original files: {stats.breakdown['original_files'] / 1024**3:.2f} GB")
```

### Cleanup Orphaned Files

```python
from storage_manager import cleanup_orphaned_files

result = await cleanup_orphaned_files(dry_run=False)
print(f"Removed {result.files_affected} files")
print(f"Reclaimed {result.space_reclaimed_bytes / 1024**2:.2f} MB")
```

### Set Storage Quota

```python
from storage_manager import set_storage_quota

quota = await set_storage_quota(
    max_bytes=50 * 1024**3,  # 50GB
    warning_at_percent=80,
    auto_cleanup_policy="oldest_first"
)
```

### Monitor Storage Trends

```python
from storage_manager import StorageAnalyzer

analyzer = StorageAnalyzer(settings)
stats = await analyzer.get_storage_stats(include_trends=True)

for trend in stats.growth_trend[-7:]:
    print(f"{trend.date}: {trend.size / 1024**3:.2f} GB")
```

## Safety Mechanisms

1. **Dry Run Mode**: All cleanup operations support preview mode
2. **Confirmation Required**: Destructive actions require explicit confirmation
3. **Backup Support**: Optional backup before cleanup
4. **Audit Logging**: All operations logged with timestamp, user, and result
5. **Rollback Support**: Failed operations rollback automatically
6. **Rate Limiting**: Cleanup operations throttled to prevent system overload

## Background Jobs

### Daily Storage Calculation
- **Schedule**: Daily at 2:00 AM
- **Duration**: 1-5 minutes
- **Purpose**: Update storage statistics and trends

### Weekly Orphan Detection
- **Schedule**: Weekly on Sunday at 3:00 AM
- **Duration**: 5-15 minutes
- **Purpose**: Identify files in storage but not in database

### Monthly Cleanup
- **Schedule**: Monthly on 1st at 4:00 AM
- **Duration**: 10-30 minutes
- **Purpose**: Vacuum database, remove old cache, cleanup logs

### Real-time Usage Tracking
- **Trigger**: After each file indexed/deleted
- **Duration**: <100ms
- **Purpose**: Incremental stats update for instant feedback
