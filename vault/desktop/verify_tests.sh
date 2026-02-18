#!/bin/bash
# Database Verification Helper for Operation Silver Bullet

DB_PATH="$HOME/Library/Application Support/com.vault.recall/vault.db"
FILES_PATH="$HOME/Library/Application Support/com.vault.recall/files"

echo "🔍 OPERATION SILVER BULLET - Database Verification Helper"
echo "=========================================================="
echo ""

# Check if database exists
if [ ! -f "$DB_PATH" ]; then
    echo "❌ Database not found at: $DB_PATH"
    echo "   Make sure the app has been run at least once."
    exit 1
fi

echo "✅ Database found: $DB_PATH"
echo ""

# Function to run SQL query
run_query() {
    local query="$1"
    local description="$2"

    echo "📊 $description"
    echo "---"
    sqlite3 "$DB_PATH" "$query"
    echo ""
}

# Menu
echo "Select verification test:"
echo "1) Test 1: Check for duplicate files (Doppelgänger)"
echo "2) Test 2: Check for duplicate tags (Tag Storm)"
echo "3) Test 3: Check file cleanup (Zombie Hunt)"
echo "4) List all files in database"
echo "5) List all files on disk"
echo "6) Custom SQL query"
echo ""
read -p "Enter choice [1-6]: " choice

case $choice in
    1)
        echo ""
        run_query "SELECT checksum, COUNT(*) as count, GROUP_CONCAT(file_name) as files FROM documents GROUP BY checksum HAVING count > 1;" \
            "Checking for duplicate files (same checksum)"

        if [ $? -eq 0 ]; then
            echo "✅ If no rows returned: No duplicates found (PASS)"
            echo "❌ If rows returned: Duplicates exist (FAIL)"
        fi
        ;;

    2)
        echo ""
        read -p "Enter tag name to check (e.g., RC1-Test): " tag_name

        run_query "SELECT name, COUNT(*) as count FROM tags WHERE name = '$tag_name' GROUP BY name;" \
            "Checking for duplicate tags named '$tag_name'"

        run_query "SELECT COUNT(*) FROM document_tags dt JOIN tags t ON dt.tag_id = t.id WHERE t.name = '$tag_name';" \
            "Count of files tagged with '$tag_name'"

        echo "✅ PASS if: 1 tag entry, N files tagged (N = number you tagged)"
        echo "❌ FAIL if: Multiple tag entries or count mismatch"
        ;;

    3)
        echo ""
        read -p "Enter filename of deleted file: " filename

        run_query "SELECT * FROM documents WHERE file_name LIKE '%$filename%';" \
            "Checking if document exists in database"

        run_query "SELECT COUNT(*) FROM chunks WHERE document_id IN (SELECT id FROM documents WHERE file_name LIKE '%$filename%');" \
            "Checking for orphaned chunks"

        # Check file system
        echo "📁 Checking file system:"
        echo "---"
        if ls "$FILES_PATH"/*"$filename"* 2>/dev/null; then
            echo "❌ FAIL: File still exists on disk!"
        else
            echo "✅ PASS: File not found on disk"
        fi
        echo ""
        ;;

    4)
        run_query "SELECT id, file_name, checksum, indexed_at FROM documents ORDER BY indexed_at DESC LIMIT 20;" \
            "Last 20 files in database"
        ;;

    5)
        echo "📁 Files on disk:"
        echo "---"
        ls -lh "$FILES_PATH" 2>/dev/null || echo "No files directory found"
        echo ""
        ;;

    6)
        echo ""
        read -p "Enter SQL query: " query
        run_query "$query" "Custom query result"
        ;;

    *)
        echo "Invalid choice"
        exit 1
        ;;
esac

echo ""
echo "=========================================================="
echo "💡 TIP: Run 'sqlite3 \"$DB_PATH\"' for interactive SQL"
echo ""
