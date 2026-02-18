# File Watcher Usage Examples

This document provides practical examples for using the File Watcher module in various scenarios.

## Table of Contents
1. [Basic Usage](#basic-usage)
2. [API Integration](#api-integration)
3. [Custom Callbacks](#custom-callbacks)
4. [Configuration Examples](#configuration-examples)
5. [Production Deployment](#production-deployment)
6. [Troubleshooting Examples](#troubleshooting-examples)

---

## Basic Usage

### Example 1: Simple File Watching

```python
import asyncio
from pathlib import Path
from src.modules.file_watcher import FileWatcher, FileEvent

async def handle_file_change(event: FileEvent):
    print(f"[{event.event_type}] {event.file_path} at {event.timestamp}")
    print(f"Priority: {event.priority}, Hash: {event.file_hash}")

async def main():
    watcher = FileWatcher(
        process_callback=handle_file_change,
        num_workers=3,
        debounce_seconds=0.5
    )

    await watcher.start(["/Users/documents"])

    print("Watching for changes... Press Ctrl+C to stop")
    try:
        while True:
            await asyncio.sleep(1)
    except KeyboardInterrupt:
        await watcher.stop()

if __name__ == "__main__":
    asyncio.run(main())
```

### Example 2: Multiple Directories

```python
async def watch_multiple_directories():
    watcher = FileWatcher(process_callback=handle_file_change)

    directories = [
        "/Users/documents",
        "/Users/downloads",
        "/Users/projects"
    ]

    await watcher.start(directories)
    await asyncio.sleep(3600)
    await watcher.stop()
```

### Example 3: Initial Directory Scan

```python
async def scan_and_watch():
    watcher = FileWatcher(
        process_callback=handle_file_change,
        debounce_seconds=0.5
    )

    watch_path = Path("/Users/documents")

    await watcher.start([watch_path])

    print("Performing initial scan...")
    await watcher.scan_directory(watch_path)
    print("Initial scan complete")

    await asyncio.sleep(3600)
    await watcher.stop()
```

---

## API Integration

### Example 4: Add Watch Directory via API

```bash
curl -X POST http://localhost:8000/api/v1/watch \
  -H "Content-Type: application/json" \
  -d '{
    "path": "/Users/documents",
    "recursive": true,
    "file_patterns": ["*"],
    "ignore_patterns": ["*.tmp", ".git/*"],
    "auto_index": true
  }'
```

Response:
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "path": "/Users/documents",
  "recursive": true,
  "file_patterns": ["*"],
  "ignore_patterns": ["*.tmp", ".git/*"],
  "auto_index": true,
  "status": "active",
  "files_watched": 0,
  "last_scan": null,
  "created_at": "2025-11-10T12:34:56Z"
}
```

### Example 5: List All Watch Directories

```bash
curl http://localhost:8000/api/v1/watch
```

Response:
```json
{
  "items": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "path": "/Users/documents",
      "recursive": true,
      "status": "active",
      "files_watched": 150,
      "last_scan": "2025-11-10T12:30:00Z"
    }
  ],
  "total": 1
}
```

### Example 6: Get Watch Status

```bash
curl http://localhost:8000/api/v1/watch/status
```

Response:
```json
{
  "total_directories": 3,
  "active_watchers": 2,
  "total_files_watched": 450,
  "pending_index_queue": 5,
  "last_event": "2025-11-10T12:35:00Z"
}
```

### Example 7: Pause/Resume Watching

```bash
# Pause watching
curl -X POST http://localhost:8000/api/v1/watch/550e8400-e29b-41d4-a716-446655440000/pause

# Resume watching
curl -X POST http://localhost:8000/api/v1/watch/550e8400-e29b-41d4-a716-446655440000/resume
```

### Example 8: Trigger Manual Reindex

```bash
curl -X POST http://localhost:8000/api/v1/watch/reindex \
  -H "Content-Type: application/json" \
  -d '{
    "watch_id": "550e8400-e29b-41d4-a716-446655440000",
    "force": true
  }'
```

---

## Custom Callbacks

### Example 9: Database Integration

```python
from sqlalchemy.ext.asyncio import AsyncSession
from src.models import File
from src.services.indexing import IndexingService

class FileIndexingHandler:
    def __init__(self, db_session: AsyncSession):
        self.db_session = db_session
        self.indexing_service = IndexingService()

    async def handle_event(self, event: FileEvent):
        if event.event_type == FileEventType.CREATED:
            await self._index_new_file(event)
        elif event.event_type == FileEventType.MODIFIED:
            await self._reindex_file(event)
        elif event.event_type == FileEventType.DELETED:
            await self._remove_file(event)

    async def _index_new_file(self, event: FileEvent):
        file_obj = File(
            path=str(event.file_path),
            filename=event.file_path.name,
            hash=event.file_hash
        )
        self.db_session.add(file_obj)
        await self.db_session.flush()

        await self.indexing_service.index_file(file_obj.id, self.db_session)
        await self.db_session.commit()

async def main():
    async with get_session() as session:
        handler = FileIndexingHandler(session)
        watcher = FileWatcher(process_callback=handler.handle_event)
        await watcher.start(["/path/to/watch"])
```

### Example 10: Notification System

```python
import smtplib
from email.message import EmailMessage

class NotificationHandler:
    def __init__(self, smtp_config):
        self.smtp_config = smtp_config

    async def handle_event(self, event: FileEvent):
        if event.event_type == FileEventType.CREATED:
            await self._send_notification(
                f"New file detected: {event.file_path.name}"
            )

    async def _send_notification(self, message: str):
        msg = EmailMessage()
        msg['Subject'] = 'File Watcher Alert'
        msg['From'] = self.smtp_config['from']
        msg['To'] = self.smtp_config['to']
        msg.set_content(message)

        with smtplib.SMTP(self.smtp_config['host']) as server:
            server.send_message(msg)

async def main():
    smtp_config = {
        'host': 'smtp.gmail.com',
        'from': 'alerts@example.com',
        'to': 'admin@example.com'
    }

    handler = NotificationHandler(smtp_config)
    watcher = FileWatcher(process_callback=handler.handle_event)
    await watcher.start(["/critical/files"])
```

### Example 11: Webhook Integration

```python
import httpx

async def webhook_callback(event: FileEvent):
    async with httpx.AsyncClient() as client:
        payload = {
            "event_type": event.event_type.value,
            "file_path": str(event.file_path),
            "timestamp": event.timestamp.isoformat(),
            "file_hash": event.file_hash
        }

        await client.post(
            "https://webhook.site/your-webhook-id",
            json=payload
        )

async def main():
    watcher = FileWatcher(process_callback=webhook_callback)
    await watcher.start(["/monitored/directory"])
```

---

## Configuration Examples

### Example 12: Custom Ignore Patterns

```python
# Watch only specific file types
custom_ignore = [
    "*",  # Ignore everything by default
    "!*.txt",  # Except text files
    "!*.md",   # And markdown files
    "!*.pdf"   # And PDFs
]

watcher = FileWatcher(
    process_callback=handle_event,
    ignore_patterns=custom_ignore
)
```

### Example 13: Environment-Based Configuration

```python
import os
from src.config import get_settings

settings = get_settings()

# Development: Fast debounce, more workers
if os.getenv("ENV") == "development":
    watcher = FileWatcher(
        process_callback=handle_event,
        num_workers=5,
        debounce_seconds=0.1
    )

# Production: Slower debounce, fewer workers
else:
    watcher = FileWatcher(
        process_callback=handle_event,
        num_workers=3,
        debounce_seconds=1.0
    )
```

### Example 14: Performance Tuning

```python
# High-throughput configuration
high_throughput_watcher = FileWatcher(
    process_callback=handle_event,
    num_workers=10,              # More workers
    debounce_seconds=0.1         # Quick processing
)

# Resource-constrained configuration
low_resource_watcher = FileWatcher(
    process_callback=handle_event,
    num_workers=1,               # Single worker
    debounce_seconds=2.0         # Longer debounce
)
```

---

## Production Deployment

### Example 15: Systemd Service

Create `/etc/systemd/system/vault-watcher.service`:

```ini
[Unit]
Description=Vault File Watcher Service
After=network.target postgresql.service

[Service]
Type=simple
User=vault
Group=vault
WorkingDirectory=/opt/vault
Environment="FILE_WATCHER_ENABLED=true"
Environment="FILE_WATCHER_NUM_WORKERS=3"
ExecStart=/opt/vault/venv/bin/python -m src.main
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Start service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable vault-watcher
sudo systemctl start vault-watcher
sudo systemctl status vault-watcher
```

### Example 16: Docker Compose

```yaml
version: '3.8'

services:
  vault-backend:
    image: vault-backend:latest
    environment:
      - FILE_WATCHER_ENABLED=true
      - FILE_WATCHER_DEBOUNCE_SECONDS=0.5
      - FILE_WATCHER_NUM_WORKERS=3
      - DATABASE_URL=postgresql://vault:pass@db:5432/vault
    volumes:
      - /path/to/documents:/watch/documents:ro
    depends_on:
      - db
    restart: unless-stopped

  db:
    image: postgres:15
    environment:
      - POSTGRES_DB=vault
      - POSTGRES_USER=vault
      - POSTGRES_PASSWORD=pass
    volumes:
      - vault-data:/var/lib/postgresql/data

volumes:
  vault-data:
```

### Example 17: Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: vault-backend
spec:
  replicas: 1
  selector:
    matchLabels:
      app: vault-backend
  template:
    metadata:
      labels:
        app: vault-backend
    spec:
      containers:
      - name: vault-backend
        image: vault-backend:latest
        env:
        - name: FILE_WATCHER_ENABLED
          value: "true"
        - name: FILE_WATCHER_NUM_WORKERS
          value: "3"
        volumeMounts:
        - name: documents
          mountPath: /watch/documents
          readOnly: true
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
      volumes:
      - name: documents
        persistentVolumeClaim:
          claimName: documents-pvc
```

---

## Troubleshooting Examples

### Example 18: Debug Logging

```python
import logging

# Enable debug logging
logging.basicConfig(
    level=logging.DEBUG,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)

logger = logging.getLogger('src.modules.file_watcher')
logger.setLevel(logging.DEBUG)

async def debug_callback(event: FileEvent):
    logger.debug(f"Event received: {event}")
    logger.debug(f"File exists: {event.file_path.exists()}")
    logger.debug(f"File size: {event.file_path.stat().st_size if event.file_path.exists() else 'N/A'}")

watcher = FileWatcher(process_callback=debug_callback)
await watcher.start(["/debug/path"])
```

### Example 19: Health Check Script

```python
import asyncio
from datetime import datetime, timedelta
from src.config import get_settings
from src.db import get_session_factory
from src.models import WatchFolder
from sqlalchemy import select

async def check_watcher_health():
    settings = get_settings()

    if not settings.file_watcher_enabled:
        print("❌ File watcher is disabled")
        return False

    async with get_session_factory()() as session:
        result = await session.execute(
            select(WatchFolder).where(WatchFolder.active == True)
        )
        active_folders = result.scalars().all()

        print(f"✓ {len(active_folders)} active watch folders")

        for folder in active_folders:
            age = datetime.utcnow() - folder.last_scan_at if folder.last_scan_at else None
            if age and age > timedelta(hours=1):
                print(f"⚠ {folder.path} last scanned {age} ago")
            else:
                print(f"✓ {folder.path} is healthy")

    return True

if __name__ == "__main__":
    asyncio.run(check_watcher_health())
```

### Example 20: Performance Monitoring

```python
import time
from collections import defaultdict

class PerformanceMonitor:
    def __init__(self):
        self.event_counts = defaultdict(int)
        self.processing_times = []
        self.start_time = time.time()

    async def monitored_callback(self, event: FileEvent):
        start = time.time()

        # Process event
        await self.actual_callback(event)

        # Record metrics
        duration = time.time() - start
        self.event_counts[event.event_type] += 1
        self.processing_times.append(duration)

        # Log statistics every 100 events
        total_events = sum(self.event_counts.values())
        if total_events % 100 == 0:
            self.print_stats()

    def print_stats(self):
        uptime = time.time() - self.start_time
        total_events = sum(self.event_counts.values())
        avg_time = sum(self.processing_times) / len(self.processing_times)

        print(f"\n=== Performance Stats ===")
        print(f"Uptime: {uptime:.1f}s")
        print(f"Total events: {total_events}")
        print(f"Events/min: {(total_events / uptime) * 60:.1f}")
        print(f"Avg processing time: {avg_time*1000:.2f}ms")
        print(f"Event breakdown: {dict(self.event_counts)}")
        print("========================\n")

async def main():
    monitor = PerformanceMonitor()
    watcher = FileWatcher(process_callback=monitor.monitored_callback)
    await watcher.start(["/monitored/path"])
```

---

## Advanced Examples

### Example 21: Rate Limiting

```python
import asyncio
from datetime import datetime, timedelta

class RateLimitedHandler:
    def __init__(self, max_per_minute=60):
        self.max_per_minute = max_per_minute
        self.events = []

    async def handle_event(self, event: FileEvent):
        now = datetime.now()
        cutoff = now - timedelta(minutes=1)

        # Remove old events
        self.events = [e for e in self.events if e > cutoff]

        # Check rate limit
        if len(self.events) >= self.max_per_minute:
            print(f"Rate limit exceeded, dropping event: {event.file_path}")
            return

        self.events.append(now)
        await self.process_event(event)

    async def process_event(self, event: FileEvent):
        print(f"Processing: {event.file_path}")

async def main():
    handler = RateLimitedHandler(max_per_minute=100)
    watcher = FileWatcher(process_callback=handler.handle_event)
    await watcher.start(["/high/traffic/path"])
```

### Example 22: Graceful Shutdown

```python
import signal

class GracefulWatcher:
    def __init__(self):
        self.watcher = None
        self.running = True

        signal.signal(signal.SIGINT, self.signal_handler)
        signal.signal(signal.SIGTERM, self.signal_handler)

    def signal_handler(self, signum, frame):
        print(f"\nReceived signal {signum}, shutting down gracefully...")
        self.running = False

    async def run(self):
        self.watcher = FileWatcher(process_callback=self.handle_event)
        await self.watcher.start(["/path/to/watch"])

        print("Watcher started. Press Ctrl+C to stop.")

        while self.running:
            await asyncio.sleep(1)

        print("Stopping watcher...")
        await self.watcher.stop()
        print("Watcher stopped successfully")

    async def handle_event(self, event: FileEvent):
        if self.running:
            print(f"Processing: {event.file_path}")

if __name__ == "__main__":
    watcher = GracefulWatcher()
    asyncio.run(watcher.run())
```

---

## Testing Examples

### Example 23: Integration Test

```python
import pytest
from pathlib import Path

@pytest.mark.asyncio
async def test_file_watcher_integration(tmp_path):
    events_received = []

    async def test_callback(event: FileEvent):
        events_received.append(event)

    watcher = FileWatcher(
        process_callback=test_callback,
        debounce_seconds=0.1
    )

    test_dir = tmp_path / "test_watch"
    test_dir.mkdir()

    await watcher.start([test_dir])
    await asyncio.sleep(0.2)

    # Create test file
    test_file = test_dir / "test.txt"
    test_file.write_text("Hello World")

    await asyncio.sleep(0.5)

    assert len(events_received) > 0
    assert any(e.file_path.name == "test.txt" for e in events_received)

    await watcher.stop()
```

---

For more examples and documentation, visit:
- [Main Documentation](README.md)
- [API Reference](../../api/v1/watch.py)
- [Configuration Guide](../../config/settings.py)
