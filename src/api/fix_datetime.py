"""Fix datetime.utcnow to datetime.now(timezone.utc)"""
import re
from pathlib import Path

files_to_fix = [
    "src/auth/models.py",
    "src/models/indexing_job.py",
    "src/schemas/health.py",
    "src/modules/exporter/types.py",
    "src/modules/summarizer/types.py",
    "src/services/llm/agentic_rag.py",
    "src/services/langchain/models.py",
]

for file_path in files_to_fix:
    path = Path(file_path)
    if not path.exists():
        continue
    
    content = path.read_text()
    original = content
    
    # Fix: default=datetime.utcnow
    content = re.sub(
        r'default=datetime\.utcnow',
        r'default=lambda: datetime.now(timezone.utc)',
        content
    )
    
    # Fix: onupdate=datetime.utcnow
    content = re.sub(
        r'onupdate=datetime.utcnow',
        r'onupdate=lambda: datetime.now(timezone.utc)',
        content
    )
    
    # Fix: default_factory=datetime.utcnow
    content = re.sub(
        r'default_factory=datetime\.utcnow',
        r'default_factory=lambda: datetime.now(timezone.utc)',
        content
    )
    
    # Add timezone import if needed
    if 'datetime.now(timezone.utc)' in content and 'from datetime import' in content:
        if ', timezone' not in content:
            content = content.replace('from datetime import', 'from datetime import timezone,', 1)
            content = content.replace('datetime import timezone,', 'datetime import ', 1)
            content = content.replace('from datetime import ', 'from datetime import timezone, ', 1)
    
    if content != original:
        path.write_text(content)
        print(f"✅ Fixed: {file_path}")
    else:
        print(f"⏭️  No changes: {file_path}")

print("\n✅ DateTime modernization complete!")
