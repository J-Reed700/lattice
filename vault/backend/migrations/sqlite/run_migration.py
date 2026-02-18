#!/usr/bin/env python3
"""
SQLite migration runner for Vault.

Usage:
    python run_migration.py <database_path> [migration_file]
    
Examples:
    python run_migration.py vault.db
    python run_migration.py vault.db 001_initial_schema.sql
"""

import sqlite3
import sys
from pathlib import Path
from datetime import datetime


def load_sqlite_vec(conn: sqlite3.Connection):
    """Load sqlite-vec extension."""
    try:
        conn.enable_load_extension(True)
        conn.load_extension("vec0")
        print("✓ sqlite-vec extension loaded")
    except Exception as e:
        print(f"✗ Failed to load sqlite-vec: {e}")
        print("  Install from: https://github.com/asg017/sqlite-vec")
        sys.exit(1)


def run_migration(db_path: str, migration_file: str):
    """Run a migration file against the database."""
    migration_path = Path(__file__).parent / migration_file
    
    if not migration_path.exists():
        print(f"✗ Migration file not found: {migration_path}")
        sys.exit(1)
    
    print(f"Running migration: {migration_file}")
    print(f"Database: {db_path}")
    print("-" * 60)
    
    # Read migration SQL
    with open(migration_path, 'r', encoding='utf-8') as f:
        migration_sql = f.read()
    
    # Connect to database
    conn = sqlite3.connect(db_path)
    conn.execute("PRAGMA foreign_keys = ON")
    conn.execute("PRAGMA journal_mode = WAL")
    
    # Load sqlite-vec extension
    load_sqlite_vec(conn)
    
    try:
        # Execute migration in a transaction
        cursor = conn.cursor()
        cursor.executescript(migration_sql)
        conn.commit()
        
        print("✓ Migration completed successfully")
        
        # Show statistics
        tables = cursor.execute(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        ).fetchall()
        
        print(f"\n✓ Created {len(tables)} tables:")
        for (table,) in tables:
            if not table.startswith('sqlite_'):
                count = cursor.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                print(f"  - {table}: {count} rows")
        
        # Show views
        views = cursor.execute(
            "SELECT name FROM sqlite_master WHERE type='view' ORDER BY name"
        ).fetchall()
        if views:
            print(f"\n✓ Created {len(views)} views:")
            for (view,) in views:
                print(f"  - {view}")
        
        # Show indexes
        indexes = cursor.execute(
            "SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name"
        ).fetchall()
        if indexes:
            print(f"\n✓ Created {len(indexes)} indexes")
        
    except sqlite3.Error as e:
        print(f"\n✗ Migration failed: {e}")
        conn.rollback()
        sys.exit(1)
    finally:
        conn.close()


def verify_schema(db_path: str):
    """Verify the schema is correct."""
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    
    print("\nSchema Verification:")
    print("-" * 60)
    
    # Check critical tables
    required_tables = [
        'documents', 'embeddings', 'topics', 'file_metadata',
        'vec_text_embeddings', 'fts_documents'
    ]
    
    for table in required_tables:
        result = cursor.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name=?",
            (table,)
        ).fetchone()
        
        status = "✓" if result else "✗"
        print(f"{status} {table}")
    
    conn.close()


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python run_migration.py <database_path> [migration_file]")
        sys.exit(1)
    
    db_path = sys.argv[1]
    migration_file = sys.argv[2] if len(sys.argv) > 2 else "001_initial_schema.sql"
    
    print(f"Vault SQLite Migration Runner")
    print(f"Started: {datetime.now().isoformat()}")
    print("=" * 60)
    
    run_migration(db_path, migration_file)
    verify_schema(db_path)
    
    print("\n" + "=" * 60)
    print(f"Completed: {datetime.now().isoformat()}")
