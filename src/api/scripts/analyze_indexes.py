#!/usr/bin/env python3
import asyncio
import asyncpg
from tabulate import tabulate


async def analyze_indexes():
    conn = await asyncpg.connect("postgresql://localhost/vault")
    
    print("\n=== ALL INDEXES ===")
    indexes = await conn.fetch("""
        SELECT tablename, indexname, 
               pg_size_pretty(pg_relation_size(indexrelid)) as size,
               idx_scan as scans
        FROM pg_stat_user_indexes
        WHERE schemaname = 'public'
        ORDER BY pg_relation_size(indexrelid) DESC
    """)
    print(tabulate(indexes, headers='keys'))
    
    print("\n=== UNUSED INDEXES ===")
    unused = await conn.fetch("""
        SELECT tablename, indexname,
               pg_size_pretty(pg_relation_size(indexrelid)) as wasted_size
        FROM pg_stat_user_indexes
        WHERE idx_scan = 0 AND schemaname = 'public'
          AND indexrelid::regclass::text NOT LIKE '%_pkey'
    """)
    print(tabulate(unused, headers='keys'))
    
    await conn.close()


if __name__ == "__main__":
    asyncio.run(analyze_indexes())
