#!/bin/bash
# Safe dev server starter - prevents multiple Vite instances

echo "🔍 Checking for running dev servers..."

# Kill any existing dev servers
pkill -9 -f "vite" 2>/dev/null
pkill -9 -f "tauri" 2>/dev/null
sleep 2

# Verify port is free
if lsof -ti:5173 > /dev/null 2>&1; then
    echo "❌ Port 5173 still in use! Killing..."
    kill -9 $(lsof -ti:5173)
    sleep 1
fi

echo "✓ Port 5173 is free"
echo "🚀 Starting dev server..."

# Change to the desktop directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

npm run tauri:dev
