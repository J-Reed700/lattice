#!/bin/bash
# ================================================================================
# Vault Backend Start Script
# ================================================================================
# This script starts all Vault backend services
# Usage: ./scripts/start.sh [dev|prod]

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Default environment
ENV=${1:-dev}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# ================================================================================
# Main
# ================================================================================

case $ENV in
    dev|development)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.dev.yml"
        log_info "Starting development environment..."
        ;;
    prod|production)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.prod.yml"
        log_info "Starting production environment..."
        ;;
    *)
        log_warning "Unknown environment: $ENV"
        log_info "Usage: $0 [dev|prod]"
        exit 1
        ;;
esac

# Check if compose file exists
if [ ! -f "$COMPOSE_FILE" ]; then
    log_warning "Docker Compose file not found: $COMPOSE_FILE"
    exit 1
fi

# Start services
log_info "Starting services from $COMPOSE_FILE..."
docker-compose -f "$COMPOSE_FILE" up -d

# Wait for services to be healthy
log_info "Waiting for services to be ready..."
sleep 5

# Show status
log_info "Service status:"
docker-compose -f "$COMPOSE_FILE" ps

log_success "All services started successfully!"

if [ "$ENV" = "dev" ] || [ "$ENV" = "development" ]; then
    echo ""
    log_info "Development URLs:"
    log_info "  - API: http://localhost:8000"
    log_info "  - API Docs: http://localhost:8000/docs"
    log_info "  - MinIO Console: http://localhost:9001"
    log_info "  - PostgreSQL: localhost:5432"
    log_info "  - Redis: localhost:6379"
    log_info "  - ElectricSQL: http://localhost:5133"
else
    echo ""
    log_info "Production URLs:"
    log_info "  - API: http://localhost:${API_PORT:-8000}"
    log_info "  - API Docs: http://localhost:${API_PORT:-8000}/docs (if enabled)"
    log_info "  - MinIO Console: http://localhost:9001"
fi

echo ""
log_info "View logs with: ./scripts/logs.sh"
log_info "Stop services with: ./scripts/stop.sh $ENV"
echo ""
