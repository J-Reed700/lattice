#!/bin/bash
# ================================================================================
# Vault Backend Deployment Script
# ================================================================================
# This script deploys the Vault backend to production
# Usage: ./scripts/deploy.sh [options]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Default values
COMPOSE_FILE="${PROJECT_DIR}/docker/docker-compose.prod.yml"
ENV_FILE="${PROJECT_DIR}/.env"
SKIP_BACKUP=false
SKIP_MIGRATIONS=false
SKIP_BUILD=false

# ================================================================================
# Helper Functions
# ================================================================================

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
Vault Backend Deployment Script

Usage: $0 [options]

Options:
    -h, --help              Show this help message
    -e, --env FILE          Specify environment file (default: .env)
    -f, --file FILE         Specify docker-compose file (default: docker/docker-compose.prod.yml)
    --skip-backup           Skip database backup before deployment
    --skip-migrations       Skip database migrations
    --skip-build            Skip building Docker images
    --pull                  Pull latest images before deployment

Examples:
    $0                      # Deploy with default settings
    $0 --skip-backup        # Deploy without backing up database
    $0 -e .env.prod         # Deploy with custom environment file

EOF
}

check_requirements() {
    log_info "Checking requirements..."

    # Check if docker is installed
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed. Please install Docker first."
        exit 1
    fi

    # Check if docker-compose is installed
    if ! command -v docker-compose &> /dev/null; then
        log_error "Docker Compose is not installed. Please install Docker Compose first."
        exit 1
    fi

    # Check if .env file exists
    if [ ! -f "$ENV_FILE" ]; then
        log_error "Environment file not found: $ENV_FILE"
        log_info "Please copy .env.example to .env and configure it."
        exit 1
    fi

    # Check if docker-compose file exists
    if [ ! -f "$COMPOSE_FILE" ]; then
        log_error "Docker Compose file not found: $COMPOSE_FILE"
        exit 1
    fi

    log_success "All requirements satisfied"
}

load_environment() {
    log_info "Loading environment variables from $ENV_FILE..."
    set -a
    source "$ENV_FILE"
    set +a
    log_success "Environment variables loaded"
}

backup_database() {
    if [ "$SKIP_BACKUP" = true ]; then
        log_warning "Skipping database backup"
        return
    fi

    log_info "Creating database backup..."
    "${SCRIPT_DIR}/backup.sh" || {
        log_error "Database backup failed"
        exit 1
    }
    log_success "Database backup completed"
}

pull_latest_code() {
    log_info "Pulling latest code from repository..."

    if [ -d "${PROJECT_DIR}/.git" ]; then
        cd "$PROJECT_DIR"
        git pull origin main || {
            log_warning "Failed to pull latest code. Continuing with local version..."
        }
    else
        log_warning "Not a git repository. Skipping code update."
    fi
}

build_images() {
    if [ "$SKIP_BUILD" = true ]; then
        log_warning "Skipping image build"
        return
    fi

    log_info "Building Docker images..."
    docker-compose -f "$COMPOSE_FILE" build --no-cache || {
        log_error "Failed to build Docker images"
        exit 1
    }
    log_success "Docker images built successfully"
}

run_migrations() {
    if [ "$SKIP_MIGRATIONS" = true ]; then
        log_warning "Skipping database migrations"
        return
    fi

    log_info "Running database migrations..."
    docker-compose -f "$COMPOSE_FILE" run --rm backend alembic upgrade head || {
        log_error "Database migrations failed"
        exit 1
    }
    log_success "Database migrations completed"
}

start_services() {
    log_info "Starting services..."
    docker-compose -f "$COMPOSE_FILE" up -d || {
        log_error "Failed to start services"
        exit 1
    }
    log_success "Services started successfully"
}

check_health() {
    log_info "Checking service health..."

    local max_attempts=30
    local attempt=1

    while [ $attempt -le $max_attempts ]; do
        if curl -f http://localhost:8000/health &> /dev/null; then
            log_success "Backend is healthy"
            return 0
        fi

        log_info "Waiting for backend to be healthy... (attempt $attempt/$max_attempts)"
        sleep 5
        ((attempt++))
    done

    log_error "Backend health check failed after $max_attempts attempts"
    log_info "Check logs with: docker-compose -f $COMPOSE_FILE logs backend"
    return 1
}

show_status() {
    log_info "Deployment status:"
    docker-compose -f "$COMPOSE_FILE" ps
}

# ================================================================================
# Parse Arguments
# ================================================================================

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -e|--env)
            ENV_FILE="$2"
            shift 2
            ;;
        -f|--file)
            COMPOSE_FILE="$2"
            shift 2
            ;;
        --skip-backup)
            SKIP_BACKUP=true
            shift
            ;;
        --skip-migrations)
            SKIP_MIGRATIONS=true
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# ================================================================================
# Main Deployment Process
# ================================================================================

echo ""
log_info "🚀 Starting Vault Backend Deployment"
echo ""

# Step 1: Check requirements
check_requirements

# Step 2: Load environment
load_environment

# Step 3: Backup database
backup_database

# Step 4: Pull latest code (optional)
# Uncomment if you want to pull latest code
# pull_latest_code

# Step 5: Build images
build_images

# Step 6: Run migrations
run_migrations

# Step 7: Start services
start_services

# Step 8: Check health
check_health

# Step 9: Show status
show_status

echo ""
log_success "✅ Deployment completed successfully!"
echo ""
log_info "Service URLs:"
log_info "  - API: http://localhost:${API_PORT:-8000}"
log_info "  - API Docs: http://localhost:${API_PORT:-8000}/docs"
log_info "  - MinIO Console: http://localhost:9001"
echo ""
log_info "Useful commands:"
log_info "  - View logs: ./scripts/logs.sh"
log_info "  - Stop services: ./scripts/stop.sh"
log_info "  - Backup database: ./scripts/backup.sh"
echo ""
