# Storage Manager Usage Guide

Complete guide for using the storage management module in the Vault system.

## Table of Contents

- [Quick Start](#quick-start)
- [API Endpoints](#api-endpoints)
- [Python Usage](#python-usage)
- [Frontend Integration](#frontend-integration)
- [Background Tasks](#background-tasks)
- [Configuration](#configuration)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

## Quick Start

### 1. Get Storage Statistics

```python
from src.modules.storage_manager import get_storage_stats

# Get current storage statistics
stats = await get_storage_stats(refresh=True)

print(f"Total Storage: {stats.total_bytes / 1024**3:.2f} GB")
print(f"Original Files: {stats.breakdown.original_files / 1024**3:.2f} GB")
print(f"Embeddings: {stats.breakdown.embeddings / 1024**3:.2f} GB")
print(f"Database: {stats.breakdown.database / 1024**3:.2f} GB")
```

### 2. Cleanup Operations

```python
from src.modules.storage_manager import cleanup_orphaned_files

# Preview what would be cleaned (dry run)
result = await cleanup_orphaned_files(dry_run=True, older_than_days=30)
print(f"Would remove {result.files_affected} files")
print(f"Would reclaim {result.space_reclaimed_bytes / 1024**2:.2f} MB")

# Actually perform cleanup
result = await cleanup_orphaned_files(dry_run=False, older_than_days=30)
print(f"Removed {result.files_affected} files")
print(f"Reclaimed {result.space_reclaimed_bytes / 1024**2:.2f} MB")
```

### 3. Manage Storage Quota

```python
from src.modules.storage_manager import set_storage_quota, check_quota_status

# Set a 50GB quota
status = await set_storage_quota(
    max_bytes=50 * 1024**3,
    warning_at_percent=80,
    critical_at_percent=95,
    auto_cleanup_enabled=True
)

print(f"Quota Status: {status.status}")
print(f"Usage: {status.percent_used:.1f}%")
print(f"Available: {status.available_bytes / 1024**3:.2f} GB")

# Check current quota status
status = await check_quota_status()
if status.needs_cleanup:
    print("⚠️ Storage cleanup recommended")
```

## API Endpoints

### GET /api/v1/storage/stats

Get comprehensive storage statistics.

**Query Parameters:**
- `refresh` (boolean): Force refresh instead of cache
- `include_breakdown` (boolean): Include category breakdown
- `include_trends` (boolean): Include growth trends
- `include_largest` (boolean): Include largest files

**Example Request:**
```bash
curl "http://localhost:8000/api/v1/storage/stats?refresh=true"
```

**Example Response:**
```json
{
  "total_bytes": 10737418240,
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
    "png": 2147483648
  },
  "by_date": {
    "this_week": 1073741824,
    "this_month": 3221225472
  },
  "largest_files": [...],
  "growth_trend": [...],
  "last_calculated": "2025-11-10T12:00:00Z",
  "cached": false
}
```

### POST /api/v1/storage/cleanup

Perform cleanup operation.

**Request Body:**
```json
{
  "operation": "orphaned_files",
  "dry_run": true,
  "older_than_days": 30,
  "file_types": ["tmp", "cache"],
  "min_size_bytes": 1048576
}
```

**Operations:**
- `orphaned_files`: Remove files in storage but not in database
- `cache`: Clear cache directory
- `thumbnails`: Delete generated thumbnails
- `old_logs`: Remove old log files
- `vacuum_db`: Vacuum database
- `old_embeddings`: Delete old embeddings

**Example Request:**
```bash
curl -X POST "http://localhost:8000/api/v1/storage/cleanup" \
  -H "Content-Type: application/json" \
  -d '{
    "operation": "orphaned_files",
    "dry_run": true,
    "older_than_days": 30
  }'
```

**Example Response:**
```json
{
  "operation": "orphaned_files",
  "dry_run": true,
  "files_affected": 42,
  "space_reclaimed_bytes": 536870912,
  "errors": [],
  "duration_seconds": 2.5,
  "timestamp": "2025-11-10T12:00:00Z"
}
```

### POST /api/v1/storage/quota

Set storage quota.

**Request Body:**
```json
{
  "max_bytes": 53687091200,
  "warning_at_percent": 80,
  "critical_at_percent": 95,
  "auto_cleanup_enabled": true,
  "auto_cleanup_policy": "oldest_first",
  "auto_cleanup_target_percent": 70
}
```

**Example Request:**
```bash
curl -X POST "http://localhost:8000/api/v1/storage/quota" \
  -H "Content-Type: application/json" \
  -d '{
    "max_bytes": 53687091200,
    "warning_at_percent": 80,
    "critical_at_percent": 95
  }'
```

### GET /api/v1/storage/quota/status

Get current quota status.

**Example Response:**
```json
{
  "quota": {
    "max_bytes": 53687091200,
    "warning_at_percent": 80,
    "critical_at_percent": 95
  },
  "current_bytes": 43046721536,
  "percent_used": 80.2,
  "status": "warning",
  "available_bytes": 10640369664,
  "needs_cleanup": true
}
```

## Python Usage

### Using StorageAnalyzer Directly

```python
from src.config import get_settings
from src.db import get_session
from src.modules.storage_manager import StorageAnalyzer
from src.modules.storage_manager.config import DEFAULT_CONFIG

async def analyze_storage():
    settings = get_settings()
    async with get_session() as session:
        analyzer = StorageAnalyzer(settings, DEFAULT_CONFIG, session)

        # Get full statistics
        stats = await analyzer.get_storage_stats(
            refresh=True,
            include_breakdown=True,
            include_trends=True,
            include_largest=True
        )

        # Print breakdown
        print("Storage Breakdown:")
        print(f"  Original Files: {stats.breakdown.original_files / 1024**3:.2f} GB")
        print(f"  Embeddings: {stats.breakdown.embeddings / 1024**3:.2f} GB")
        print(f"  Database: {stats.breakdown.database / 1024**3:.2f} GB")

        # Print largest files
        print("\nLargest Files:")
        for file in stats.largest_files[:10]:
            print(f"  {file.path}: {file.size_bytes / 1024**2:.2f} MB")

        return stats
```

### Using StorageCleanup Directly

```python
from src.modules.storage_manager import StorageCleanup, CleanupRequest, CleanupOperation

async def cleanup_old_cache():
    settings = get_settings()
    async with get_session() as session:
        cleanup = StorageCleanup(settings, DEFAULT_CONFIG, session)

        # First do a dry run
        dry_request = CleanupRequest(
            operation=CleanupOperation.CACHE,
            dry_run=True,
            older_than_days=30
        )
        dry_result = await cleanup.cleanup(dry_request)

        print(f"Preview: Would remove {dry_result.files_affected} files")
        print(f"Preview: Would reclaim {dry_result.space_reclaimed_bytes / 1024**2:.2f} MB")

        # Confirm and execute
        if dry_result.files_affected > 0:
            real_request = CleanupRequest(
                operation=CleanupOperation.CACHE,
                dry_run=False,
                older_than_days=30
            )
            real_result = await cleanup.cleanup(real_request)

            print(f"Removed {real_result.files_affected} files")
            print(f"Reclaimed {real_result.space_reclaimed_bytes / 1024**2:.2f} MB")

        return real_result
```

### Using StorageQuotaManager

```python
from src.modules.storage_manager import StorageQuotaManager

async def manage_quota():
    settings = get_settings()
    async with get_session() as session:
        quota_mgr = StorageQuotaManager(settings, DEFAULT_CONFIG, session)

        # Set quota
        quota = await quota_mgr.set_quota(
            max_bytes=50 * 1024**3,
            warning_at_percent=80,
            critical_at_percent=95,
            auto_cleanup_enabled=True,
            auto_cleanup_policy="oldest_first",
            auto_cleanup_target_percent=70
        )

        # Check status
        status = await quota_mgr.get_quota_status()

        print(f"Quota: {quota.max_bytes / 1024**3:.2f} GB")
        print(f"Current: {status.current_bytes / 1024**3:.2f} GB")
        print(f"Usage: {status.percent_used:.1f}%")
        print(f"Status: {status.status}")

        if status.needs_cleanup:
            print("⚠️ Cleanup recommended!")

        # Check before indexing
        new_file_size = 100 * 1024**2  # 100MB
        can_index = await quota_mgr.check_quota_before_index(new_file_size)

        if can_index:
            print("✓ Space available for new file")
        else:
            print("✗ Quota would be exceeded")

        return status
```

## Frontend Integration

### Using the StorageDashboard Component

```tsx
import StorageDashboard from './components/StorageDashboard';

function App() {
  return (
    <div>
      <StorageDashboard />
    </div>
  );
}
```

### Custom API Integration

```tsx
import { useEffect, useState } from 'react';

interface StorageStats {
  total_bytes: number;
  breakdown: {
    original_files: number;
    embeddings: number;
    database: number;
  };
}

function MyStorageComponent() {
  const [stats, setStats] = useState<StorageStats | null>(null);

  useEffect(() => {
    async function fetchStats() {
      const response = await fetch('http://localhost:8000/api/v1/storage/stats');
      const data = await response.json();
      setStats(data);
    }

    fetchStats();
  }, []);

  if (!stats) return <div>Loading...</div>;

  return (
    <div>
      <h1>Storage: {(stats.total_bytes / 1024**3).toFixed(2)} GB</h1>
      <p>Files: {(stats.breakdown.original_files / 1024**3).toFixed(2)} GB</p>
      <p>Embeddings: {(stats.breakdown.embeddings / 1024**3).toFixed(2)} GB</p>
    </div>
  );
}
```

## Background Tasks

### Scheduled Tasks

The storage manager includes several scheduled Celery tasks:

1. **Daily Storage Calculation** (2:00 AM daily)
   ```python
   from src.tasks.storage_tasks import calculate_daily_storage_stats

   # Manually trigger
   result = calculate_daily_storage_stats.delay()
   ```

2. **Weekly Orphan Detection** (Sunday 3:00 AM)
   ```python
   from src.tasks.storage_tasks import detect_orphaned_files_task

   # Manually trigger
   result = detect_orphaned_files_task.delay()
   ```

3. **Monthly Cleanup** (1st of month, 4:00 AM)
   ```python
   from src.tasks.storage_tasks import monthly_cleanup_task

   # Manually trigger
   result = monthly_cleanup_task.delay()
   ```

4. **Hourly Quota Check**
   ```python
   from src.tasks.storage_tasks import check_storage_quota

   # Manually trigger
   result = check_storage_quota.delay()
   ```

### Configure Celery Beat

Add to your Celery configuration:

```python
from celery.schedules import crontab

CELERY_BEAT_SCHEDULE = {
    'calculate-daily-storage': {
        'task': 'storage.calculate_daily_stats',
        'schedule': crontab(hour=2, minute=0),
    },
    'detect-orphaned-files': {
        'task': 'storage.detect_orphaned_files',
        'schedule': crontab(day_of_week=0, hour=3, minute=0),
    },
    'monthly-cleanup': {
        'task': 'storage.monthly_cleanup',
        'schedule': crontab(day_of_month=1, hour=4, minute=0),
    },
    'check-storage-quota': {
        'task': 'storage.check_quota',
        'schedule': crontab(minute=0),  # Every hour
    },
}
```

## Configuration

### Environment Variables

```bash
# Storage paths
STORAGE_BASE_PATH=./storage
STORAGE_DOCUMENTS_PATH=./storage/documents
STORAGE_SCREENSHOTS_PATH=./storage/screenshots
STORAGE_THUMBNAILS_PATH=./storage/thumbnails
STORAGE_CHUNKS_PATH=./storage/chunks

# Storage limits
STORAGE_MAX_FILE_SIZE=104857600  # 100MB
STORAGE_DEFAULT_QUOTA=53687091200  # 50GB

# Cleanup settings
STORAGE_CACHE_TTL_SECONDS=300
STORAGE_CLEANUP_BATCH_SIZE=1000
STORAGE_TREND_RETENTION_DAYS=365
```

### Python Configuration

```python
from src.modules.storage_manager.config import StorageManagerConfig

config = StorageManagerConfig(
    cache_ttl_seconds=300,
    max_files_per_scan=1000000,
    cleanup_batch_size=1000,
    trend_retention_days=365,
    default_quota_bytes=50 * 1024**3,
    warning_threshold_percent=80,
    critical_threshold_percent=95,
    auto_cleanup_enabled=False,
    backup_before_cleanup=True,
    audit_log_retention_days=90,
)
```

## Best Practices

### 1. Always Use Dry Run First

```python
# BAD: Directly delete files
result = await cleanup_orphaned_files(dry_run=False)

# GOOD: Preview first, then confirm
preview = await cleanup_orphaned_files(dry_run=True)
if preview.files_affected > 0:
    print(f"Will remove {preview.files_affected} files")
    confirm = input("Continue? (y/n): ")
    if confirm.lower() == 'y':
        result = await cleanup_orphaned_files(dry_run=False)
```

### 2. Monitor Quota Status Regularly

```python
async def check_and_alert():
    status = await check_quota_status()

    if status.status == 'exceeded':
        send_alert("Storage quota exceeded!")
    elif status.status == 'critical':
        send_alert(f"Storage at {status.percent_used:.1f}%")
    elif status.status == 'warning':
        send_notification(f"Storage at {status.percent_used:.1f}%")
```

### 3. Use Auto-Cleanup Carefully

```python
# Enable auto-cleanup only with conservative settings
await set_storage_quota(
    max_bytes=100 * 1024**3,
    warning_at_percent=80,
    critical_at_percent=90,  # Higher threshold
    auto_cleanup_enabled=True,
    auto_cleanup_policy="oldest_first",
    auto_cleanup_target_percent=70  # Clean to 70%
)
```

### 4. Schedule Regular Maintenance

```python
# Run cleanup during off-peak hours
async def nightly_maintenance():
    # Cleanup old cache
    await cleanup_cache(dry_run=False, older_than_days=30)

    # Cleanup old logs
    await cleanup_old_logs(dry_run=False, older_than_days=90)

    # Vacuum database monthly
    if datetime.now().day == 1:
        await vacuum_database(dry_run=False)
```

## Troubleshooting

### Issue: Stats Not Updating

**Solution:** Force refresh the cache
```python
stats = await get_storage_stats(refresh=True)
```

### Issue: Cleanup Not Removing Files

**Possible Causes:**
1. Dry run mode enabled
2. Filters too restrictive
3. Permission issues

**Solution:**
```python
# Check dry_run flag
result = await cleanup_orphaned_files(dry_run=False)

# Adjust filters
result = await cleanup_orphaned_files(
    dry_run=False,
    older_than_days=7,  # Less restrictive
    min_size_bytes=None  # No size filter
)
```

### Issue: Quota Exceeded Errors

**Solution:** Increase quota or enable auto-cleanup
```python
await set_storage_quota(
    max_bytes=100 * 1024**3,  # Increase to 100GB
    auto_cleanup_enabled=True
)
```

### Issue: Slow Stats Calculation

**Solution:** Use cached stats or reduce scope
```python
# Use cache
stats = await get_storage_stats(refresh=False)

# Reduce scope
stats = await get_storage_stats(
    include_trends=False,  # Skip trends
    include_largest=False  # Skip largest files
)
```

### Issue: Database Lock During Vacuum

**Solution:** Run vacuum during maintenance windows
```python
# Schedule for low-traffic periods
async def weekend_vacuum():
    if datetime.now().weekday() == 6:  # Sunday
        await vacuum_database(dry_run=False)
```
