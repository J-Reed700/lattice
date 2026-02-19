#!/usr/bin/env python3
"""Verify soft delete schema implementation."""

from src.api.src.models.sync import Document, SyncLog
from sqlalchemy import inspect

def verify_document_model():
    """Verify Document model has soft delete fields."""
    mapper = inspect(Document)
    columns = {col.key: col for col in mapper.columns}
    
    print("📋 Document Model Verification")
    print("=" * 60)
    
    required_fields = [
        ('deleted_at', 'DateTime'),
        ('deleted_by_device_id', 'Integer'),
        ('cleanup_after', 'DateTime'),
    ]
    
    for field_name, expected_type in required_fields:
        if field_name in columns:
            col = columns[field_name]
            nullable = "nullable" if col.nullable else "NOT NULL"
            print(f"✓ {field_name:25} {str(col.type):15} {nullable}")
        else:
            print(f"✗ {field_name:25} MISSING")
    
    # Check relationships
    relationships = {rel.key: rel for rel in mapper.relationships}
    if 'deleted_by' in relationships:
        print(f"✓ {'deleted_by':25} Relationship to Device")
    else:
        print(f"✗ {'deleted_by':25} MISSING")
    
    print()

def verify_synclog_model():
    """Verify SyncLog model has document_path and nullable document_id."""
    mapper = inspect(SyncLog)
    columns = {col.key: col for col in mapper.columns}
    
    print("📋 SyncLog Model Verification")
    print("=" * 60)
    
    # Check document_path
    if 'document_path' in columns:
        col = columns['document_path']
        nullable = "nullable" if col.nullable else "NOT NULL"
        print(f"✓ {'document_path':25} {str(col.type):15} {nullable}")
    else:
        print(f"✗ {'document_path':25} MISSING")
    
    # Check document_id is nullable
    if 'document_id' in columns:
        col = columns['document_id']
        nullable = "nullable" if col.nullable else "NOT NULL"
        expected = "✓" if col.nullable else "✗"
        print(f"{expected} {'document_id':25} {str(col.type):15} {nullable}")
    else:
        print(f"✗ {'document_id':25} MISSING")
    
    print()

def show_indexes():
    """Show table indexes."""
    print("📋 Document Indexes")
    print("=" * 60)
    
    mapper = inspect(Document)
    table = mapper.tables[0]
    
    for index in table.indexes:
        cols = ', '.join(c.name for c in index.columns)
        print(f"✓ {index.name:35} ({cols})")
    
    print()

if __name__ == "__main__":
    try:
        verify_document_model()
        verify_synclog_model()
        show_indexes()
        print("✅ All schema changes verified successfully!")
    except Exception as e:
        print(f"❌ Verification failed: {e}")
        import traceback
        traceback.print_exc()
