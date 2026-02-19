#!/bin/bash
# ================================================================================
# Vault Backend Logs Script
# ================================================================================
# This script shows logs from Vault backend services
# Usage: ./scripts/logs.sh [service] [options]

set -e

# Colors
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Default values
ENV="dev"
SERVICE=""
FOLLOW=false
TAIL="100"

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

show_usage() {
    cat << EOF
Vault Backend Logs Script

Usage: $0 [service] [options]

Services:
    backend             Backend API service
    celery              Celery worker
    celery-beat         Celery beat scheduler
    postgres            PostgreSQL database
    redis               Redis cache
    minio               MinIO object storage
    electric            ElectricSQL sync service
    nginx               Nginx reverse proxy (production only)
    (no service)        Show logs from all services

Options:
    -f, --follow        Follow log output (live tail)
    -n, --tail N        Number of lines to show (default: 100)
    -e, --env ENV       Environment (dev/prod) [default: dev]
    -h, --help          Show this help message

Examples:
    $0                      # Show last 100 lines from all services
    $0 backend -f           # Follow backend logs
    $0 postgres -n 50       # Show last 50 lines from postgres
    $0 -e prod -f           # Follow all production logs

EOF
}

# ================================================================================
# Parse Arguments
# ================================================================================

while [[ $# -gt 0 ]]; do
    case $1 in
        backend|celery|celery-beat|postgres|redis|minio|electric|nginx)
            SERVICE=$1
            shift
            ;;
        -f|--follow)
            FOLLOW=true
            shift
            ;;
        -n|--tail)
            TAIL="$2"
            shift 2
            ;;
        -e|--env)
            ENV="$2"
            shift 2
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            log_warning "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# ================================================================================
# Main
# ================================================================================

case $ENV in
    dev|development)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.dev.yml"
        ;;
    prod|production)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.prod.yml"
        ;;
    *)
        log_warning "Unknown environment: $ENV"
        exit 1
        ;;
esac

# Check if compose file exists
if [ ! -f "$COMPOSE_FILE" ]; then
    log_warning "Docker Compose file not found: $COMPOSE_FILE"
    exit 1
fi

# Build docker-compose command
CMD="docker-compose -f $COMPOSE_FILE logs"

if [ "$FOLLOW" = true ]; then
    CMD="$CMD --follow"
fi

if [ -n "$TAIL" ]; then
    CMD="$CMD --tail=$TAIL"
fi

if [ -n "$SERVICE" ]; then
    # Map friendly names to container names
    case $SERVICE in
        backend)
            if [ "$ENV" = "dev" ] || [ "$ENV" = "development" ]; then
                SERVICE="backend"
            else
                SERVICE="backend"
            fi
            ;;
        celery)
            if [ "$ENV" = "dev" ] || [ "$ENV" = "development" ]; then
                SERVICE="celery-worker"
            else
                SERVICE="celery-worker"
            fi
            ;;
    esac

    log_info "Showing logs for $SERVICE ($ENV environment)..."
    CMD="$CMD $SERVICE"
else
    log_info "Showing logs for all services ($ENV environment)..."
fi

# Execute command
eval $CMD
