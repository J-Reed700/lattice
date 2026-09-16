#!/usr/bin/env python3
"""Prepare production SQL against migrations, without executing data mutations.

Rust parsing excludes comments and test-only code. QueryBuilder fragments and
format strings are reported separately; repository tests must execute those paths.
"""
import argparse
import json
from pathlib import Path
import sqlite3
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def prepare(connection, sql):
    try:
        connection.execute('EXPLAIN ' + sql)
    except sqlite3.ProgrammingError as error:
        # SQLite has already prepared and resolved names before checking binds.
        if 'bindings' not in str(error):
            raise

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    extracted = subprocess.run([
        'cargo', 'run', '--quiet', '--locked', '--manifest-path',
        'scripts/data-contract-check/Cargo.toml', '--', 'src-tauri/src'
    ], cwd=ROOT, check=True, capture_output=True, text=True)
    rows = json.loads(extracted.stdout)
    connection = sqlite3.connect(':memory:')
    migrations = sorted((ROOT / 'src-tauri/migrations').glob('*.sql'))
    for migration in migrations:
        connection.executescript(migration.read_text())
    # sqlx creates its own bookkeeping table at runtime; the backup feature
    # reads it to compare schema versions between databases.
    connection.executescript(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations ("
        " version BIGINT PRIMARY KEY, description TEXT NOT NULL,"
        " installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,"
        " success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)"
    )
    # Some repositories initialize optional cache/audit tables on demand.
    # Use their actual DDL rather than a second hand-written schema.
    for row in rows:
        if row['sql'].strip().startswith('CREATE TABLE IF NOT EXISTS'):
            connection.executescript(row['sql'])
    checked, dynamic, failures = [], [], []
    for row in rows:
        sql = row['sql']
        if sql.strip().startswith('CREATE'):
            continue
        if row['fragment'] or '{' in sql:
            dynamic.append(row)
            continue
        try:
            prepare(connection, sql)
            checked.append(row)
        except sqlite3.Error as error:
            failures.append({**row, 'error': str(error)})
    # Verify the guard rejects the exact obsolete-schema class it was added for.
    try:
        prepare(connection, 'SELECT files.name FROM files')
    except sqlite3.OperationalError:
        pass
    else:
        raise AssertionError('Obsolete files.name unexpectedly accepted')
    report = {'migrations': len(migrations), 'checked': checked, 'dynamic': dynamic, 'failures': failures}
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"Prepared {len(checked)} SQL statements against {len(migrations)} migrations; {len(dynamic)} dynamic fragments require repository tests.")
        for failure in failures:
            print(f"{failure['file']}:{failure['line']}: {failure['error']}")
    return bool(failures)

if __name__ == '__main__':
    raise SystemExit(main())
