#!/bin/bash
# ================================================================================
# Vault Backend Database Backup Script
# ================================================================================
# This script creates backups of the Vault PostgreSQL database
# Usage: ./scripts/backup.sh [options]

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Default values
BACKUP_DIR="${PROJECT_DIR}/backups"
ENV="prod"
KEEP_BACKUPS=7
COMPRESS=true

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

show_usage() {
    cat << EOF
Vault Backend Database Backup Script

Usage: $0 [options]

Options:
    -d, --dir DIR           Backup directory (default: ./backups)
    -e, --env ENV           Environment (dev/prod) [default: prod]
    -k, --keep N            Keep last N backups (default: 7)
    --no-compress           Don't compress backup file
    -h, --help              Show this help message

Examples:
    $0                      # Create compressed production backup
    $0 -e dev               # Backup development database
    $0 -k 30                # Keep last 30 backups
    $0 --no-compress        # Create uncompressed backup

EOF
}

# ================================================================================
# Parse Arguments
# ================================================================================

while [[ $# -gt 0 ]]; do
    case $1 in
        -d|--dir)
            BACKUP_DIR="$2"
            shift 2
            ;;
        -e|--env)
            ENV="$2"
            shift 2
            ;;
        -k|--keep)
            KEEP_BACKUPS="$2"
            shift 2
            ;;
        --no-compress)
            COMPRESS=false
            shift
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# ================================================================================
# Main
# ================================================================================

log_info "🔄 Starting database backup process..."

# Create backup directory if it doesn't exist
mkdir -p "$BACKUP_DIR"

# Set compose file based on environment
case $ENV in
    dev|development)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.dev.yml"
        CONTAINER_NAME="vault-postgres-dev"
        ;;
    prod|production)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.prod.yml"
        CONTAINER_NAME="vault-postgres"
        ;;
    *)
        log_error "Unknown environment: $ENV"
        exit 1
        ;;
esac

# Load environment variables
ENV_FILE="${PROJECT_DIR}/.env"
if [ -f "$ENV_FILE" ]; then
    set -a
    source "$ENV_FILE"
    set +a
fi

# Get database credentials
DB_USER=${POSTGRES_USER:-vault}
DB_NAME=${POSTGRES_DB:-vault}
DB_PASSWORD=${POSTGRES_PASSWORD:-vault}

# Check if container is running
if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    log_error "PostgreSQL container is not running: $CONTAINER_NAME"
    log_info "Start services with: ./scripts/start.sh $ENV"
    exit 1
fi

# Generate backup filename
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILENAME="vault_backup_${ENV}_${TIMESTAMP}.sql"

if [ "$COMPRESS" = true ]; then
    BACKUP_FILENAME="${BACKUP_FILENAME}.gz"
fi

BACKUP_PATH="${BACKUP_DIR}/${BACKUP_FILENAME}"

log_info "Creating backup: $BACKUP_FILENAME"
log_info "Database: $DB_NAME (user: $DB_USER)"

# Create backup
if [ "$COMPRESS" = true ]; then
    # Compressed backup
    docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        pg_dump -U "$DB_USER" -d "$DB_NAME" --clean --if-exists \
        | gzip > "$BACKUP_PATH"
else
    # Uncompressed backup
    docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        pg_dump -U "$DB_USER" -d "$DB_NAME" --clean --if-exists \
        > "$BACKUP_PATH"
fi

if [ $? -eq 0 ]; then
    BACKUP_SIZE=$(du -h "$BACKUP_PATH" | cut -f1)
    log_success "Backup created successfully: $BACKUP_PATH ($BACKUP_SIZE)"
else
    log_error "Backup failed"
    exit 1
fi

# Cleanup old backups
log_info "Cleaning up old backups (keeping last $KEEP_BACKUPS)..."
cd "$BACKUP_DIR"
ls -t vault_backup_${ENV}_*.sql* 2>/dev/null | tail -n +$((KEEP_BACKUPS + 1)) | xargs -r rm -f
REMAINING_BACKUPS=$(ls -1 vault_backup_${ENV}_*.sql* 2>/dev/null | wc -l)
log_info "Current backups: $REMAINING_BACKUPS"

# Show backup info
echo ""
log_success "✅ Backup completed successfully!"
echo ""
log_info "Backup details:"
log_info "  - File: $BACKUP_PATH"
log_info "  - Size: $BACKUP_SIZE"
log_info "  - Environment: $ENV"
log_info "  - Database: $DB_NAME"
echo ""
log_info "Restore with:"
log_info "  ./scripts/restore.sh $BACKUP_FILENAME"
echo ""

# Optional: Upload to cloud storage
# Uncomment and configure for cloud backup
# if [ -n "$BACKUP_S3_BUCKET" ]; then
#     log_info "Uploading to S3: $BACKUP_S3_BUCKET"
#     aws s3 cp "$BACKUP_PATH" "s3://${BACKUP_S3_BUCKET}/vault-backups/"
#     log_success "Backup uploaded to S3"
# fi
