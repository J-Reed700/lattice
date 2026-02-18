#!/usr/bin/env python3
import asyncio
import asyncpg
import time
from typing import List, Tuple


async def benchmark_query(conn: asyncpg.Connection, name: str, query: str, 
                          iterations: int = 100) -> Tuple[str, float, float]:
    times = []
    for _ in range(iterations):
        start = time.perf_counter()
        await conn.fetch(query)
        elapsed = (time.perf_counter() - start) * 1000
        times.append(elapsed)
    
    avg_time = sum(times) / len(times)
    min_time = min(times)
    return name, avg_time, min_time


async def run_benchmarks():
    conn = await asyncpg.connect("postgresql://localhost/vault")
    
    queries = [
        ("List recent files", 
         "SELECT * FROM files ORDER BY created_at DESC LIMIT 100"),
        
        ("Filter by MIME type",
         "SELECT * FROM files WHERE mime_type = 'application/pdf' ORDER BY created_at DESC LIMIT 50"),
        
        ("Search by filename",
         "SELECT * FROM files WHERE filename ILIKE '%document%' LIMIT 50"),
        
        ("Get file with text content",
         "SELECT f.*, t.content FROM files f JOIN text_content t ON f.id = t.file_id WHERE f.id IN (SELECT id FROM files LIMIT 10)"),
        
        ("Count files by type",
         "SELECT mime_type, COUNT(*) FROM files GROUP BY mime_type"),
        
        ("Get tagged files",
         "SELECT f.* FROM files f JOIN file_tags ft ON f.id = ft.file_id WHERE ft.tag_id IN (SELECT id FROM tags LIMIT 5)"),
        
        ("Search history",
         "SELECT * FROM search_history ORDER BY created_at DESC LIMIT 100"),
    ]
    
    print("=" * 80)
    print("DATABASE PERFORMANCE BENCHMARKS")
    print("=" * 80)
    print(f"\n{'Query':<40} {'Avg (ms)':<12} {'Min (ms)':<12} {'Target (ms)':<12}")
    print("-" * 80)
    
    for name, query in queries:
        query_name, avg_ms, min_ms = await benchmark_query(conn, name, query, 50)
        target_ms = 10
        status = "✓" if avg_ms < target_ms else "✗"
        print(f"{status} {query_name:<38} {avg_ms:>10.2f}   {min_ms:>10.2f}   {target_ms:>10.2f}")
    
    await conn.close()
    print("\n" + "=" * 80)


if __name__ == "__main__":
    asyncio.run(run_benchmarks())
