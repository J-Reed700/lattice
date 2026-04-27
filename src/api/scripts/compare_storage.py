"""Compare storage metrics before/after UUID migration."""

import json
import sys


def format_bytes(bytes_val: int) -> str:
    """Format bytes as human-readable string."""
    for unit in ["B", "KB", "MB", "GB"]:
        if bytes_val < 1024:
            return f"{bytes_val:.2f} {unit}"
        bytes_val /= 1024
    return f"{bytes_val:.2f} TB"


def compare_storage(pre_file: str, post_file: str) -> None:
    """Compare pre and post migration storage."""

    with open(pre_file) as f:
        pre = json.load(f)

    with open(post_file) as f:
        post = json.load(f)

    print("📊 Storage Comparison: String(36) vs UUID")
    print("=" * 80)
    print()

    # Verify row counts didn't change
    print("Row Count Verification:")
    print(f"{'Table':<30} {'Before':>12} {'After':>12} {'Status':>12}")
    print("-" * 80)

    row_mismatch = False
    for table in pre["row_counts"]:
        pre_count = pre["row_counts"].get(table, 0)
        post_count = post["row_counts"].get(table, 0)
        status = "✅ OK" if pre_count == post_count else "❌ MISMATCH"
        if pre_count != post_count:
            row_mismatch = True
        print(f"{table:<30} {pre_count:>12} {post_count:>12} {status:>12}")

    if row_mismatch:
        print()
        print("⚠️  WARNING: Row count mismatch detected!")
        print()

    print()
    print("-" * 80)
    print()

    # Total database comparison
    pre_total = pre["total_database"]["size_bytes"]
    post_total = post["total_database"]["size_bytes"]
    saved = pre_total - post_total
    percent = (saved / pre_total) * 100 if pre_total > 0 else 0

    print("Total Database:")
    print(f"  Before: {pre['total_database']['size']} ({format_bytes(pre_total)})")
    print(f"  After:  {post['total_database']['size']} ({format_bytes(post_total)})")
    print(f"  Saved:  {format_bytes(saved)} ({percent:.1f}%)")
    print()

    # Index comparison
    pre_index_total = pre.get("total_index_bytes", 0)
    post_index_total = post.get("total_index_bytes", 0)
    index_saved = pre_index_total - post_index_total
    index_percent = (
        (index_saved / pre_index_total) * 100 if pre_index_total > 0 else 0
    )

    print("Total Indexes:")
    print(f"  Before: {format_bytes(pre_index_total)}")
    print(f"  After:  {format_bytes(post_index_total)}")
    print(f"  Saved:  {format_bytes(index_saved)} ({index_percent:.1f}%)")
    print()

    # Per-table comparison
    print("Per-Table Savings:")
    print(
        f"{'Table':<30} {'Before':>15} {'After':>15} {'Saved':>15} {'%':>10}"
    )
    print("-" * 90)

    total_table_saved = 0
    for table in sorted(pre["tables"].keys()):
        if table in post["tables"]:
            pre_size = pre["tables"][table]["size_bytes"]
            post_size = post["tables"][table]["size_bytes"]
            saved_bytes = pre_size - post_size
            percent_saved = (saved_bytes / pre_size * 100) if pre_size > 0 else 0

            if saved_bytes != 0:  # Show all tables, even with negative savings
                total_table_saved += saved_bytes
                saved_str = format_bytes(abs(saved_bytes))
                if saved_bytes < 0:
                    saved_str = f"-{saved_str}"
                print(
                    f"{table:<30} {format_bytes(pre_size):>15} "
                    f"{format_bytes(post_size):>15} {saved_str:>15} "
                    f"{percent_saved:>9.1f}%"
                )

    print()
    print(f"Total table storage saved: {format_bytes(total_table_saved)}")
    print()

    # UUID column type comparison
    if "uuid_columns" in pre and "uuid_columns" in post:
        print("UUID Column Type Changes:")
        print(f"{'Column':<40} {'Before':>20} {'After':>20}")
        print("-" * 85)

        for col in sorted(pre["uuid_columns"].keys()):
            if col in post["uuid_columns"]:
                pre_type = pre["uuid_columns"][col]["type"]
                post_type = post["uuid_columns"][col]["type"]
                pre_len = pre["uuid_columns"][col].get("max_length", "N/A")
                post_len = post["uuid_columns"][col].get("max_length", "N/A")

                pre_str = f"{pre_type}"
                if pre_len != "N/A":
                    pre_str += f"({pre_len})"

                post_str = f"{post_type}"
                if post_len != "N/A":
                    post_str += f"({post_len})"

                print(f"{col:<40} {pre_str:>20} {post_str:>20}")

    print()
    print("=" * 80)
    print()

    # Summary
    if saved > 0:
        print(f"✅ Migration successful!")
        print(f"   Storage reduced by {format_bytes(saved)} ({percent:.1f}%)")
    elif saved < 0:
        print(f"⚠️  Warning: Storage increased by {format_bytes(abs(saved))}")
    else:
        print(f"ℹ️  No storage change detected")

    if not row_mismatch:
        print(f"   All row counts verified ✅")
    else:
        print(f"   ⚠️  Row count mismatches detected - data may be corrupted!")
        sys.exit(1)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python compare_storage.py <pre.json> <post.json>")
        sys.exit(1)

    compare_storage(sys.argv[1], sys.argv[2])
