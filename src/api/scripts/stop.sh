#!/bin/bash
# ================================================================================
# Vault Backend Stop Script
# ================================================================================
# This script stops all Vault backend services
# Usage: ./scripts/stop.sh [dev|prod] [--remove-volumes]

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

# Default environment
ENV=${1:-dev}
REMOVE_VOLUMES=false

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

# ================================================================================
# Parse Arguments
# ================================================================================

while [[ $# -gt 0 ]]; do
    case $1 in
        dev|development|prod|production)
            ENV=$1
            shift
            ;;
        --remove-volumes|-v)
            REMOVE_VOLUMES=true
            shift
            ;;
        -h|--help)
            cat << EOF
Vault Backend Stop Script

Usage: $0 [environment] [options]

Arguments:
    environment         Environment to stop (dev, prod) [default: dev]

Options:
    --remove-volumes    Remove volumes when stopping (WARNING: deletes data!)
    -h, --help          Show this help message

Examples:
    $0                          # Stop development environment
    $0 prod                     # Stop production environment
    $0 dev --remove-volumes     # Stop dev and remove volumes

EOF
            exit 0
            ;;
        *)
            log_error "Unknown argument: $1"
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
        log_info "Stopping development environment..."
        ;;
    prod|production)
        COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.prod.yml"
        log_info "Stopping production environment..."
        ;;
    *)
        log_error "Unknown environment: $ENV"
        log_info "Usage: $0 [dev|prod]"
        exit 1
        ;;
esac

# Check if compose file exists
if [ ! -f "$COMPOSE_FILE" ]; then
    log_warning "Docker Compose file not found: $COMPOSE_FILE"
    exit 1
fi

# Warning for production
if [ "$ENV" = "prod" ] || [ "$ENV" = "production" ]; then
    log_warning "Stopping PRODUCTION environment!"
    read -p "Are you sure? (yes/no): " -r
    echo
    if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
        log_info "Aborted"
        exit 0
    fi
fi

# Warning for volume removal
if [ "$REMOVE_VOLUMES" = true ]; then
    log_warning "WARNING: This will remove all volumes and DELETE ALL DATA!"
    read -p "Are you absolutely sure? Type 'DELETE' to confirm: " -r
    echo
    if [[ $REPLY != "DELETE" ]]; then
        log_info "Aborted"
        exit 0
    fi
fi

# Stop services
log_info "Stopping services..."
if [ "$REMOVE_VOLUMES" = true ]; then
    docker-compose -f "$COMPOSE_FILE" down -v
    log_warning "Services stopped and volumes removed"
else
    docker-compose -f "$COMPOSE_FILE" down
    log_success "Services stopped (volumes preserved)"
fi

# Show remaining containers
CONTAINERS=$(docker-compose -f "$COMPOSE_FILE" ps -q)
if [ -n "$CONTAINERS" ]; then
    log_warning "Some containers are still running:"
    docker-compose -f "$COMPOSE_FILE" ps
else
    log_success "All services stopped successfully"
fi

echo ""
log_info "Start services again with: ./scripts/start.sh $ENV"
echo ""
