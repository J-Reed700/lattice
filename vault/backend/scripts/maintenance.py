#!/usr/bin/env python3
import asyncio
import asyncpg
from datetime import datetime


async def run_maintenance():
    conn = await asyncpg.connect("postgresql://localhost/vault")
    
    print(f"[{datetime.now()}] Starting database maintenance...")
    
    print("Running VACUUM ANALYZE...")
    await conn.execute("VACUUM ANALYZE")
    
    print("Running REINDEX...")
    await conn.execute("REINDEX DATABASE vault")
    
    print("Updating statistics...")
    await conn.execute("ANALYZE")
    
    stats = await conn.fetch("""
        SELECT schemaname, tablename,
               pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as size
        FROM pg_tables
        WHERE schemaname = 'public'
        ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
    """)
    
    print("\nTable sizes after maintenance:")
    for row in stats:
        print(f"  {row['tablename']}: {row['size']}")
    
    await conn.close()
    print(f"[{datetime.now()}] Maintenance completed")


if __name__ == "__main__":
    asyncio.run(run_maintenance())
