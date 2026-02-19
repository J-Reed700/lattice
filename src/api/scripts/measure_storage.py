"""Measure database storage metrics before/after UUID migration."""

import argparse
import asyncio
import json
import os
import sys
from pathlib import Path

# Add src to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from sqlalchemy import text
from sqlalchemy.ext.asyncio import create_async_engine

from config.settings import get_settings


async def measure_storage(output_file: str) -> None:
    """Measure storage metrics for all tables."""

    settings = get_settings()
    database_url = os.environ.get("DATABASE_URL", settings.database_url)

    engine = create_async_engine(database_url, echo=False)

    metrics = {
        "database_url": database_url.split("@")[-1],  # Hide credentials
        "timestamp": asyncio.get_event_loop().time(),
    }

    async with engine.connect() as conn:
        # Row counts for each table
        print("Counting rows...")
        tables = [
            "watch_folders",
            "files",
            "text_content",
            "text_embeddings",
            "images",
            "image_embeddings",
            "tags",
            "file_tags",
            "search_history",
        ]

        row_counts = {}
        for table in tables:
            result = await conn.execute(text(f"SELECT COUNT(*) FROM {table}"))
            count = result.scalar()
            row_counts[table] = count
            print(f"   {table}: {count} rows")

        metrics["row_counts"] = row_counts

        # Table sizes
        print("\nMeasuring table sizes...")
        result = await conn.execute(
            text("""
            SELECT 
                schemaname,
                tablename,
                pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size,
                pg_total_relation_size(schemaname||'.'||tablename) AS size_bytes
            FROM pg_tables
            WHERE schemaname = 'public'
            ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
        """)
        )

        table_sizes = {}
        for row in result:
            table_sizes[row.tablename] = {
                "size": row.size,
                "size_bytes": row.size_bytes,
            }
            if row.tablename in tables:
                print(f"   {row.tablename}: {row.size}")

        metrics["tables"] = table_sizes

        # Index sizes
        print("\nMeasuring index sizes...")
        result = await conn.execute(
            text("""
            SELECT 
                indexname,
                tablename,
                pg_size_pretty(pg_relation_size(indexname::regclass)) AS size,
                pg_relation_size(indexname::regclass) AS size_bytes
            FROM pg_indexes
            WHERE schemaname = 'public'
            ORDER BY pg_relation_size(indexname::regclass) DESC;
        """)
        )

        index_sizes = {}
        total_index_bytes = 0
        for row in result:
            index_sizes[row.indexname] = {
                "table": row.tablename,
                "size": row.size,
                "size_bytes": row.size_bytes,
            }
            total_index_bytes += row.size_bytes

        metrics["indexes"] = index_sizes
        metrics["total_index_bytes"] = total_index_bytes

        # Total database size
        result = await conn.execute(
            text("""
            SELECT pg_size_pretty(pg_database_size(current_database())) AS size,
                   pg_database_size(current_database()) AS size_bytes;
        """)
        )

        row = result.fetchone()
        metrics["total_database"] = {"size": row.size, "size_bytes": row.size_bytes}

        # Column statistics for UUID columns
        print("\nAnalyzing UUID columns...")
        uuid_columns = [
            ("watch_folders", "id"),
            ("files", "id"),
            ("files", "watch_folder_id"),
            ("text_content", "id"),
            ("text_content", "file_id"),
            ("text_embeddings", "id"),
            ("text_embeddings", "text_content_id"),
            ("images", "id"),
            ("images", "file_id"),
            ("image_embeddings", "id"),
            ("image_embeddings", "image_id"),
            ("tags", "id"),
            ("file_tags", "id"),
            ("file_tags", "file_id"),
            ("file_tags", "tag_id"),
            ("search_history", "id"),
        ]

        uuid_stats = {}
        for table, column in uuid_columns:
            try:
                # Get column type
                result = await conn.execute(
                    text(f"""
                    SELECT data_type, character_maximum_length
                    FROM information_schema.columns
                    WHERE table_name = '{table}' AND column_name = '{column}'
                """)
                )
                row = result.fetchone()
                if row:
                    uuid_stats[f"{table}.{column}"] = {
                        "type": row.data_type,
                        "max_length": row.character_maximum_length,
                    }
            except Exception as e:
                print(f"   Warning: Could not analyze {table}.{column}: {e}")

        metrics["uuid_columns"] = uuid_stats

    await engine.dispose()

    # Save metrics
    with open(output_file, "w") as f:
        json.dump(metrics, f, indent=2)

    print()
    print(f"✅ Storage metrics saved to {output_file}")
    print(f"   Total database size: {metrics['total_database']['size']}")
    print(
        f"   Total index size: {metrics['total_index_bytes'] / (1024**2):.2f} MB"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Measure database storage metrics")
    parser.add_argument(
        "--output", required=True, help="Output JSON file for metrics"
    )
    args = parser.parse_args()

    asyncio.run(measure_storage(args.output))
