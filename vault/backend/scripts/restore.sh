#!/bin/bash
# ================================================================================
# Vault Backend Database Restore Script
# ================================================================================
# This script restores a PostgreSQL database backup
# Usage: ./scripts/restore.sh <backup_file> [options]

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
BACKUP_FILE=""

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
Vault Backend Database Restore Script

Usage: $0 <backup_file> [options]

Arguments:
    backup_file         Backup file to restore (filename or full path)

Options:
    -e, --env ENV       Environment (dev/prod) [default: prod]
    -h, --help          Show this help message

Examples:
    $0 vault_backup_prod_20240101_120000.sql.gz
    $0 /path/to/backup.sql -e dev
    $0 latest           # Restore latest backup

EOF
}

list_backups() {
    log_info "Available backups in $BACKUP_DIR:"
    if [ -d "$BACKUP_DIR" ] && [ "$(ls -A $BACKUP_DIR/vault_backup_*.sql* 2>/dev/null)" ]; then
        ls -lh "$BACKUP_DIR"/vault_backup_*.sql* | awk '{print "  - " $9 " (" $5 ")"}'
    else
        log_warning "No backups found in $BACKUP_DIR"
    fi
}

# ================================================================================
# Parse Arguments
# ================================================================================

if [ $# -eq 0 ]; then
    show_usage
    echo ""
    list_backups
    exit 1
fi

while [[ $# -gt 0 ]]; do
    case $1 in
        -e|--env)
            ENV="$2"
            shift 2
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        --list)
            list_backups
            exit 0
            ;;
        *)
            if [ -z "$BACKUP_FILE" ]; then
                BACKUP_FILE="$1"
                shift
            else
                log_error "Unknown option: $1"
                show_usage
                exit 1
            fi
            ;;
    esac
done

if [ -z "$BACKUP_FILE" ]; then
    log_error "No backup file specified"
    show_usage
    exit 1
fi

# ================================================================================
# Main
# ================================================================================

log_warning "⚠️  WARNING: Database restore will overwrite existing data!"

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

# Handle "latest" keyword
if [ "$BACKUP_FILE" = "latest" ]; then
    BACKUP_FILE=$(ls -t "$BACKUP_DIR"/vault_backup_${ENV}_*.sql* 2>/dev/null | head -1)
    if [ -z "$BACKUP_FILE" ]; then
        log_error "No backups found for environment: $ENV"
        exit 1
    fi
    log_info "Using latest backup: $(basename $BACKUP_FILE)"
fi

# Determine full path to backup file
if [ -f "$BACKUP_FILE" ]; then
    BACKUP_PATH="$BACKUP_FILE"
elif [ -f "${BACKUP_DIR}/${BACKUP_FILE}" ]; then
    BACKUP_PATH="${BACKUP_DIR}/${BACKUP_FILE}"
else
    log_error "Backup file not found: $BACKUP_FILE"
    list_backups
    exit 1
fi

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

log_info "Restore details:"
log_info "  - Environment: $ENV"
log_info "  - Database: $DB_NAME"
log_info "  - Backup file: $BACKUP_PATH"
log_info "  - Size: $(du -h "$BACKUP_PATH" | cut -f1)"

echo ""
read -p "Are you sure you want to restore? This will OVERWRITE the database. Type 'RESTORE' to confirm: " -r
echo
if [[ $REPLY != "RESTORE" ]]; then
    log_info "Aborted"
    exit 0
fi

# Check if container is running
if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    log_error "PostgreSQL container is not running: $CONTAINER_NAME"
    log_info "Start services with: ./scripts/start.sh $ENV"
    exit 1
fi

log_info "Creating backup of current database before restore..."
"${SCRIPT_DIR}/backup.sh" -e "$ENV" || {
    log_warning "Failed to create pre-restore backup"
}

log_info "Restoring database from backup..."

# Determine if backup is compressed
if [[ "$BACKUP_PATH" == *.gz ]]; then
    # Compressed backup
    gunzip -c "$BACKUP_PATH" | docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        psql -U "$DB_USER" -d "$DB_NAME"
else
    # Uncompressed backup
    docker exec -i -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        psql -U "$DB_USER" -d "$DB_NAME" < "$BACKUP_PATH"
fi

if [ $? -eq 0 ]; then
    log_success "✅ Database restored successfully!"

    # Verify restoration
    log_info "Verifying database..."
    TABLE_COUNT=$(docker exec -e PGPASSWORD="$DB_PASSWORD" "$CONTAINER_NAME" \
        psql -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';")
    log_info "Tables in database: $(echo $TABLE_COUNT | xargs)"

    echo ""
    log_success "Restore completed successfully!"
    log_info "You may need to restart services: ./scripts/stop.sh $ENV && ./scripts/start.sh $ENV"
else
    log_error "Restore failed"
    exit 1
fi
