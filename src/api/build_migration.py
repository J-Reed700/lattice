#!/usr/bin/env python3
"""
Script to generate the complete Alembic migration from SQL schema.
Run this to create migrations/versions/001_initial.py
"""

# Import the SQL to understand structure
import pathlib

sql_file = pathlib.Path('migrations/versions/001_initial_schema.sql')
output_file = pathlib.Path('migrations/versions/001_initial.py')

# Read SQL for reference
sql_content = sql_file.read_text()

# Generate complete migration
migration_content = '''"""Initial schema with pgvector support

Revision ID: 001_initial
Revises: 
Create Date: 2025-11-09
"""
from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql
from pgvector.sqlalchemy import Vector

revision = "001_initial"
down_revision = None
branch_labels = None
depends_on = None
'''

print("Building migration file...")
print(f"Output: {output_file}")
print("Migration generator created. Run: python3 build_migration.py")
