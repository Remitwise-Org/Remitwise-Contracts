#!/bin/bash
# Reset the indexer database

set -e

DB_PATH=${DB_PATH:-"./data/remitwise.db"}

echo "=== Reset Indexer Database ==="
echo ""
echo "This will delete all indexed data!"
echo "Database: $DB_PATH"
echo ""
read -p "Are you sure? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    echo "Cancelled"
    exit 0
fi

# Remove database files
if [ -f "$DB_PATH" ]; then
    rm "$DB_PATH"
    echo "✓ Removed $DB_PATH"
fi

if [ -f "${DB_PATH}-shm" ]; then
    rm "${DB_PATH}-shm"
    echo "✓ Removed ${DB_PATH}-shm"
fi

if [ -f "${DB_PATH}-wal" ]; then
    rm "${DB_PATH}-wal"
    echo "✓ Removed ${DB_PATH}-wal"
fi

echo ""
echo "Database reset complete!"
echo "Run 'npm start' to reinitialize and start indexing"
