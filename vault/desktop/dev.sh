#!/usr/bin/env bash
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Project paths
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_DIR="$PROJECT_ROOT/src"
DB_PATH="$HOME/Library/Application Support/com.vault.recall/vault.db"
MODELS_DIR="$HOME/.cache/recall/models"
LOG_DIR="/tmp"

# Function to print colored output
print_header() {
    echo -e "${CYAN}═══════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}$1${NC}"
    echo -e "${CYAN}═══════════════════════════════════════════════════════${NC}"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Kill all running processes
kill_all() {
    print_header "Killing all processes"

    print_info "Killing Node processes..."
    killall -9 node npm vite 2>/dev/null || true

    print_info "Killing Tauri processes..."
    pkill -9 -f "tauri" 2>/dev/null || true
    killall -9 recall-desktop 2>/dev/null || true

    print_info "Killing Rust processes..."
    pkill -9 -f "cargo" 2>/dev/null || true

    sleep 2
    print_success "All processes killed"
}

# Clean build artifacts
clean_build() {
    print_header "Cleaning build artifacts"

    cd "$PROJECT_ROOT"

    if [ -d "node_modules" ]; then
        print_info "Removing node_modules..."
        # Try standard rm first, fall back to aggressive cleanup if needed
        if ! rm -rf node_modules 2>/dev/null; then
            print_warning "Standard cleanup failed, using aggressive method..."
            # Use find with -delete for stubborn nested directories
            find node_modules -delete 2>/dev/null || true
            # Final cleanup if directory still exists
            if [ -d "node_modules" ]; then
                chmod -R u+w node_modules 2>/dev/null || true
                rm -rf node_modules 2>/dev/null || true
            fi
        fi
        if [ ! -d "node_modules" ]; then
            print_success "node_modules removed"
        else
            print_warning "node_modules partially removed (you may need to run: sudo rm -rf node_modules)"
        fi
    fi

    if [ -d "dist" ]; then
        print_info "Removing dist..."
        rm -rf dist
        print_success "dist removed"
    fi

    if [ -f "package-lock.json" ]; then
        print_info "Removing package-lock.json..."
        rm -f package-lock.json
        print_success "package-lock.json removed"
    fi

    cd "$TAURI_DIR"

    if [ -d "target" ]; then
        print_info "Removing Cargo target directory..."
        rm -rf target
        print_success "Cargo target removed"
    fi

    if [ -f "Cargo.lock" ]; then
        print_info "Removing Cargo.lock..."
        rm -f Cargo.lock
        print_success "Cargo.lock removed"
    fi

    print_success "Build artifacts cleaned"
}

# Clean database
clean_db() {
    print_header "Cleaning database"

    if [ -f "$DB_PATH" ]; then
        print_info "Removing database at $DB_PATH..."
        rm -f "${DB_PATH}"*
        print_success "Database removed"
    else
        print_info "Database not found (already clean)"
    fi
}

# Clean downloaded models
clean_models() {
    print_header "Cleaning downloaded models"

    if [ -d "$MODELS_DIR" ]; then
        print_info "Removing models directory at $MODELS_DIR..."
        rm -rf "$MODELS_DIR"
        print_success "Models directory removed"
    else
        print_info "Models directory not found (already clean)"
    fi
}

# Clean logs
clean_logs() {
    print_header "Cleaning logs"

    print_info "Removing /tmp/tauri_*.log files..."
    rm -f /tmp/tauri_*.log
    print_success "Logs cleaned"
}

# Full clean (everything)
clean_all() {
    print_header "FULL CLEAN (Nuclear Option)"
    print_warning "This will remove everything: builds, database, models, logs"

    kill_all
    clean_build
    clean_db
    clean_models
    clean_logs

    print_success "Full clean complete"
}

# Install dependencies (both npm and Rust)
install_deps() {
    print_header "Installing dependencies"

    cd "$PROJECT_ROOT"

    print_info "Installing npm dependencies..."
    npm install
    print_success "npm dependencies installed"

    cd "$TAURI_DIR"

    print_info "Building Rust dependencies (this may take a while)..."
    cargo build
    print_success "Rust dependencies built"
}

# Install npm dependencies only
install_npm() {
    print_header "Installing npm dependencies"

    cd "$PROJECT_ROOT"

    print_info "Installing npm dependencies..."
    npm install
    print_success "npm dependencies installed"
}

# Build Rust dependencies only
install_rust() {
    print_header "Building Rust dependencies"

    cd "$TAURI_DIR"

    print_info "Building Rust dependencies (this may take a while)..."
    cargo build
    print_success "Rust dependencies built"
}

# Rebuild npm only (clean + install)
rebuild_npm() {
    print_header "Rebuilding npm dependencies"

    cd "$PROJECT_ROOT"

    # Clean npm artifacts
    if [ -d "node_modules" ]; then
        print_info "Removing node_modules..."
        if ! rm -rf node_modules 2>/dev/null; then
            print_warning "Standard cleanup failed, using aggressive method..."
            find node_modules -delete 2>/dev/null || true
            if [ -d "node_modules" ]; then
                chmod -R u+w node_modules 2>/dev/null || true
                rm -rf node_modules 2>/dev/null || true
            fi
        fi
        if [ ! -d "node_modules" ]; then
            print_success "node_modules removed"
        else
            print_warning "node_modules partially removed"
        fi
    fi

    if [ -f "package-lock.json" ]; then
        print_info "Removing package-lock.json..."
        rm -f package-lock.json
        print_success "package-lock.json removed"
    fi

    if [ -d "dist" ]; then
        print_info "Removing dist..."
        rm -rf dist
        print_success "dist removed"
    fi

    # Install
    print_info "Installing npm dependencies..."
    npm install
    print_success "npm dependencies installed"
}

# Rebuild Rust only (clean + build)
rebuild_rust() {
    print_header "Rebuilding Rust dependencies"

    cd "$TAURI_DIR"

    # Clean Rust artifacts
    if [ -d "target" ]; then
        print_info "Removing Cargo target directory..."
        rm -rf target
        print_success "Cargo target removed"
    fi

    if [ -f "Cargo.lock" ]; then
        print_info "Removing Cargo.lock..."
        rm -f Cargo.lock
        print_success "Cargo.lock removed"
    fi

    # Build
    print_info "Building Rust dependencies (this may take a while)..."
    cargo build
    print_success "Rust dependencies built"
}

# Build frontend only
build_frontend() {
    print_header "Building frontend"

    cd "$PROJECT_ROOT"

    print_info "Running vite build..."
    npx vite build
    print_success "Frontend built"
}

# Build Tauri (release)
build_tauri_release() {
    print_header "Building Tauri (Release)"

    cd "$PROJECT_ROOT"

    print_info "Building Tauri release binary (this will take several minutes)..."
    npm run tauri:build
    print_success "Tauri release built"

    print_info "Binary location: $TAURI_DIR/target/release/recall-desktop"
}

# Build Tauri (debug)
build_tauri_debug() {
    print_header "Building Tauri (Debug)"

    cd "$TAURI_DIR"

    print_info "Building Tauri debug binary..."
    cargo build
    print_success "Tauri debug built"

    print_info "Binary location: $TAURI_DIR/target/debug/recall-desktop"
}

# Run dev server (frontend only)
run_frontend() {
    print_header "Running frontend dev server"

    cd "$PROJECT_ROOT"

    print_info "Starting Vite dev server on http://localhost:5173..."
    npm run dev
}

# Run Tauri dev
run_tauri_dev() {
    print_header "Running Tauri dev"

    cd "$PROJECT_ROOT"

    print_info "Starting Tauri dev mode..."
    print_info "This will start both Vite and the Tauri app"
    npm run tauri dev
}

# Quick start for testing (minimal build)
quick_start() {
    print_header "Quick Start (Testing Mode)"

    cd "$PROJECT_ROOT"

    # Check if dist exists, if not build frontend
    if [ ! -d "dist" ]; then
        print_info "No dist folder found, building frontend..."
        npx vite build
        print_success "Frontend built"
    else
        print_info "Using existing dist folder"
    fi

    print_info "Launching Tauri app..."
    npm run tauri dev
}

# Run Tauri dev with logs
run_tauri_dev_logs() {
    print_header "Running Tauri dev with logs"

    local LOG_FILE="/tmp/tauri_dev_$(date +%Y%m%d_%H%M%S).log"

    cd "$PROJECT_ROOT"

    print_info "Starting Tauri dev mode with logging to $LOG_FILE..."
    npm run tauri:dev 2>&1 | tee "$LOG_FILE"
}

# Run tests
run_tests() {
    print_header "Running tests"

    cd "$TAURI_DIR"

    print_info "Running Rust tests..."
    cargo test
    print_success "Rust tests complete"

    # Add frontend tests if you have them
    # cd "$PROJECT_ROOT"
    # npm test
}

# Run linter
run_lint() {
    print_header "Running linters"

    cd "$TAURI_DIR"

    print_info "Running cargo clippy..."
    cargo clippy -- -W clippy::all

    cd "$PROJECT_ROOT"

    if [ -f ".eslintrc.js" ] || [ -f ".eslintrc.json" ]; then
        print_info "Running ESLint..."
        npm run lint
    fi

    print_success "Linting complete"
}

# Format code
format_code() {
    print_header "Formatting code"

    cd "$TAURI_DIR"

    print_info "Running cargo fmt..."
    cargo fmt
    print_success "Rust code formatted"

    cd "$PROJECT_ROOT"

    if command -v prettier &> /dev/null; then
        print_info "Running Prettier..."
        npx prettier --write "src/**/*.{ts,tsx,js,jsx,css,json}"
        print_success "Frontend code formatted"
    fi
}

# Check system health
health_check() {
    print_header "System Health Check"

    # Check Node version
    if command -v node &> /dev/null; then
        NODE_VERSION=$(node --version)
        print_success "Node: $NODE_VERSION"
    else
        print_error "Node not found"
    fi

    # Check npm version
    if command -v npm &> /dev/null; then
        NPM_VERSION=$(npm --version)
        print_success "npm: $NPM_VERSION"
    else
        print_error "npm not found"
    fi

    # Check Rust version
    if command -v rustc &> /dev/null; then
        RUST_VERSION=$(rustc --version)
        print_success "Rust: $RUST_VERSION"
    else
        print_error "Rust not found"
    fi

    # Check Cargo version
    if command -v cargo &> /dev/null; then
        CARGO_VERSION=$(cargo --version)
        print_success "Cargo: $CARGO_VERSION"
    else
        print_error "Cargo not found"
    fi

    # Check for running processes
    echo ""
    print_info "Checking for running processes..."

    if pgrep -f "vite" > /dev/null; then
        print_warning "Vite is running (PID: $(pgrep -f 'vite'))"
    else
        print_success "No Vite processes running"
    fi

    if pgrep -f "recall-desktop" > /dev/null; then
        print_warning "Tauri app is running (PID: $(pgrep -f 'recall-desktop'))"
    else
        print_success "No Tauri processes running"
    fi

    # Check disk space
    echo ""
    print_info "Disk space:"
    df -h "$PROJECT_ROOT" | tail -1 | awk '{print "  Available: " $4 " / " $2 " (" $5 " used)"}'

    # Check database
    echo ""
    if [ -f "$DB_PATH" ]; then
        DB_SIZE=$(du -h "$DB_PATH" | cut -f1)
        print_success "Database exists: $DB_SIZE"
    else
        print_info "Database not found (will be created on first run)"
    fi

    # Check models directory
    if [ -d "$MODELS_DIR" ]; then
        MODEL_SIZE=$(du -sh "$MODELS_DIR" | cut -f1)
        MODEL_COUNT=$(find "$MODELS_DIR" -type d -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
        print_success "Models directory: $MODEL_SIZE ($MODEL_COUNT models)"
    else
        print_info "Models directory not found"
    fi
}

# Watch logs
watch_logs() {
    print_header "Watching logs"

    local LOG_PATTERN="${1:-tauri}"
    local LATEST_LOG=$(ls -t /tmp/${LOG_PATTERN}_*.log 2>/dev/null | head -1)

    if [ -n "$LATEST_LOG" ]; then
        print_info "Watching $LATEST_LOG"
        print_info "Press Ctrl+C to stop"
        echo ""
        tail -f "$LATEST_LOG"
    else
        print_error "No log files found matching pattern: ${LOG_PATTERN}_*.log"
    fi
}

# Full rebuild
full_rebuild() {
    print_header "FULL REBUILD"

    kill_all
    clean_build

    # Install npm dependencies first
    cd "$PROJECT_ROOT"
    print_info "Installing npm dependencies..."
    npm install
    print_success "npm dependencies installed"

    # Build frontend to create dist directory (required by Tauri)
    print_info "Building frontend with Vite (creating dist directory)..."
    npx vite build
    print_success "Frontend built"

    # Now build Rust dependencies
    cd "$TAURI_DIR"
    print_info "Building Rust dependencies (this may take a while)..."
    cargo build
    print_success "Rust dependencies built"

    print_success "Full rebuild complete"
    print_info "You can now run: ./dev.sh dev"
}

# Quick restart (keep deps)
quick_restart() {
    print_header "Quick Restart"

    kill_all
    sleep 1
    run_tauri_dev
}

# Show menu
show_menu() {
    print_header "Recall Desktop - Development Script"
    echo ""
    echo "Usage: ./dev.sh [command] [options]"
    echo ""
    echo -e "${CYAN}Build Commands:${NC}"
    echo "  build-frontend       Build frontend only (Vite)"
    echo "  build-debug          Build Tauri in debug mode"
    echo "  build-release        Build Tauri in release mode"
    echo "  rebuild              Full rebuild (clean + install npm + Rust)"
    echo "  rebuild-npm          Rebuild npm only (fast, no Rust)"
    echo "  rebuild-rust         Rebuild Rust only (slow, keeps npm)"
    echo ""
    echo -e "${CYAN}Run Commands:${NC}"
    echo "  dev                  Run Tauri dev mode (frontend + backend)"
    echo "  dev-logs             Run Tauri dev with logging"
    echo "  quick-start          Quick start for testing (builds dist if needed)"
    echo "  frontend             Run frontend dev server only"
    echo "  restart              Quick restart (kill + restart)"
    echo ""
    echo -e "${CYAN}Clean Commands:${NC}"
    echo "  clean                Clean build artifacts"
    echo "  clean-db             Clean database only"
    echo "  clean-models         Clean downloaded models"
    echo "  clean-logs           Clean log files"
    echo "  clean-all            Nuclear clean (everything)"
    echo "  kill                 Kill all running processes"
    echo ""
    echo -e "${CYAN}Install Commands:${NC}"
    echo "  install              Install all dependencies (npm + Rust)"
    echo "  install-npm          Install npm dependencies only (fast)"
    echo "  install-rust         Build Rust dependencies only (slow)"
    echo ""
    echo -e "${CYAN}Maintenance Commands:${NC}"
    echo "  test                 Run tests"
    echo "  lint                 Run linters"
    echo "  format               Format code"
    echo "  health               System health check"
    echo "  logs [pattern]       Watch logs (default: tauri)"
    echo ""
    echo -e "${CYAN}Examples:${NC}"
    echo "  ./dev.sh dev                    # Start development"
    echo "  ./dev.sh quick-start            # Quick launch for testing"
    echo "  ./dev.sh clean-all && ./dev.sh rebuild  # Fresh start"
    echo "  ./dev.sh health                 # Check system"
    echo "  ./dev.sh logs tauri             # Watch Tauri logs"
}

# Main command dispatcher
main() {
    cd "$PROJECT_ROOT"

    case "${1:-}" in
        # Build commands
        build-frontend)
            build_frontend
            ;;
        build-debug)
            build_tauri_debug
            ;;
        build-release)
            build_tauri_release
            ;;
        rebuild)
            full_rebuild
            ;;
        rebuild-npm)
            rebuild_npm
            ;;
        rebuild-rust)
            rebuild_rust
            ;;

        # Run commands
        dev)
            run_tauri_dev
            ;;
        dev-logs)
            run_tauri_dev_logs
            ;;
        quick-start)
            quick_start
            ;;
        frontend)
            run_frontend
            ;;
        restart)
            quick_restart
            ;;

        # Clean commands
        clean)
            clean_build
            ;;
        clean-db)
            clean_db
            ;;
        clean-models)
            clean_models
            ;;
        clean-logs)
            clean_logs
            ;;
        clean-all)
            clean_all
            ;;
        kill)
            kill_all
            ;;

        # Install commands
        install)
            install_deps
            ;;
        install-npm)
            install_npm
            ;;
        install-rust)
            install_rust
            ;;

        # Maintenance commands
        test)
            run_tests
            ;;
        lint)
            run_lint
            ;;
        format)
            format_code
            ;;
        health)
            health_check
            ;;
        logs)
            watch_logs "${2:-tauri}"
            ;;

        # Help
        help|--help|-h)
            show_menu
            ;;

        # Default
        *)
            if [ -n "${1:-}" ]; then
                print_error "Unknown command: $1"
                echo ""
            fi
            show_menu
            exit 1
            ;;
    esac
}

# Run main
main "$@"
