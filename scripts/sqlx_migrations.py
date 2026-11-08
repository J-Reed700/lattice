"""The app's migrations, applied to a plain SQLite connection the way sqlx applies them.

sqlx creates `_sqlx_migrations` before the first migration and records each one in
the transaction that applies it, so a migration can read when an earlier one ran.
"""
import hashlib
from pathlib import Path

MIGRATIONS = Path(__file__).resolve().parent.parent / "crates/lattice-desktop/migrations"

BOOKKEEPING = (
    "CREATE TABLE IF NOT EXISTS _sqlx_migrations ("
    " version BIGINT PRIMARY KEY, description TEXT NOT NULL,"
    " installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,"
    " success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)"
)


def all_migrations():
    return sorted(MIGRATIONS.glob("*.sql"))


def apply(db, migration):
    """Apply one migration file in its own transaction and record it, as sqlx does."""
    version, _, description = migration.stem.partition("_")
    sql = migration.read_bytes()
    db.executescript(
        f"{BOOKKEEPING};\nBEGIN;\n{sql.decode('utf-8')}\n"
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) "
        f"VALUES ({int(version)}, '{description.replace('_', ' ')}', 1, X'{hashlib.sha384(sql).hexdigest()}', 0);\n"
        "COMMIT;"
    )
