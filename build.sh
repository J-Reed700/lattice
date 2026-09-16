#!/usr/bin/env bash

################################################################################
# Advanced Build Script for Lattice Desktop (Unix/Linux/macOS)
#
# Usage:
#   ./build.sh [options]
#
# Options:
#   --mode <dev|release|production>    Build mode (default: release)
#   --platform <mac|linux|current>     Target platform (default: current)
#   --clean                            Clean build artifacts before building
#   --skip-tests                       Skip running tests
#   --no-bundle                        Skip creating installers
#   --features <feat1,feat2>           Enable specific features
#   --target <triple>                  Specific Rust target triple
#   --verbose                          Enable verbose logging
#   --checksums                        Generate SHA256 checksums
#   --parallel                         Enable parallel builds (default)
#   --config <path>                    Custom config file path
#   --help                             Show this help message
#
# Examples:
#   ./build.sh                                    # Simple release build
#   ./build.sh --clean --mode release            # Clean release build
#   ./build.sh --mode dev --no-bundle            # Dev build without bundling
#   ./build.sh --features embeddings,file_watcher # Build with specific features
#   ./build.sh --platform mac --checksums        # macOS build with checksums
################################################################################

set -e  # Exit on error
set -u  # Exit on undefined variable

################################################################################
# Configuration & Constants
################################################################################

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TAURI_DIR="${SCRIPT_DIR}/src-tauri"
LOG_DIR="${SCRIPT_DIR}/build-logs"
DIST_DIR="${SCRIPT_DIR}/dist"
START_TIME=$(date +%s)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
RESET='\033[0m'

# Default options
MODE="release"
PLATFORM=""
CLEAN=false
SKIP_TESTS=false
NO_BUNDLE=false
FEATURES=""
TARGET=""
VERBOSE=false
CHECKSUMS=false
PARALLEL=true
CONFIG_FILE="build.config.json"

################################################################################
# Logging Functions
################################################################################

# Create log directory
mkdir -p "${LOG_DIR}"
LOG_FILE="${LOG_DIR}/build-$(date +%Y%m%d-%H%M%S).log"

log() {
    local level="$1"
    shift
    local message="$*"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    
    # Write to log file
    echo "[${timestamp}] [${level}] ${message}" >> "${LOG_FILE}"
    
    # Write to console with colors
    case "${level}" in
        INFO)
            echo -e "${CYAN}ℹ  ${message}${RESET}"
            ;;
        SUCCESS)
            echo -e "${GREEN}✓ ${message}${RESET}"
            ;;
        WARNING)
            echo -e "${YELLOW}⚠  ${message}${RESET}"
            ;;
        ERROR)
            echo -e "${RED}✗ ${message}${RESET}"
            ;;
        HEADER)
            echo -e "\n${MAGENTA}============================================================${RESET}"
            echo -e "${WHITE}${message}${RESET}"
            echo -e "${MAGENTA}============================================================${RESET}\n"
            ;;
    esac
}

################################################################################
# Helper Functions
################################################################################

detect_platform() {
    case "$(uname -s)" in
        Darwin*)
            echo "macos"
            ;;
        Linux*)
            echo "linux"
            ;;
        *)
            log ERROR "Unsupported platform: $(uname -s)"
            exit 1
            ;;
    esac
}

check_command() {
    local cmd="$1"
    local name="$2"
    
    if command -v "${cmd}" &> /dev/null; then
        local version=$(${cmd} --version 2>&1 | head -n 1)
        log SUCCESS "${name}: ${version}"
        return 0
    else
        log ERROR "${name}: NOT FOUND"
        return 1
    fi
}

run_command() {
    local cmd="$1"
    local work_dir="${2:-$SCRIPT_DIR}"
    
    if [[ "${VERBOSE}" == true ]]; then
        log INFO "Executing: ${cmd}"
    fi
    
    if [[ "${VERBOSE}" == true ]]; then
        (cd "${work_dir}" && eval "${cmd}")
    else
        (cd "${work_dir}" && eval "${cmd}" >> "${LOG_FILE}" 2>&1)
    fi
    
    return $?
}

################################################################################
# Build Steps
################################################################################

check_prerequisites() {
    log HEADER "Checking Prerequisites"
    
    local all_found=true
    
    check_command "node" "Node.js" || all_found=false
    check_command "npm" "npm" || all_found=false
    check_command "rustc" "Rust" || all_found=false
    check_command "cargo" "Cargo" || all_found=false
    
    if [[ "${all_found}" == false ]]; then
        log ERROR "Missing required tools. Please install them first."
        exit 1
    fi
    
    # Platform-specific checks
    if [[ "${PLATFORM:-$(detect_platform)}" == "macos" ]]; then
        if xcode-select -p &> /dev/null; then
            log SUCCESS "Xcode Command Line Tools: Found"
        else
            log WARNING "Xcode Command Line Tools: Not found (may cause build issues)"
        fi
    elif [[ "${PLATFORM:-$(detect_platform)}" == "linux" ]]; then
        if command -v gcc &> /dev/null; then
            log SUCCESS "GCC: Found"
        else
            log WARNING "GCC: Not found (may cause build issues)"
        fi
    fi
}

clean_build() {
    if [[ "${CLEAN}" != true ]]; then
        return
    fi
    
    log HEADER "Cleaning Build Artifacts"
    
    local paths_to_clean=(
        "${TAURI_DIR}/target"
        "${DIST_DIR}"
        "${SCRIPT_DIR}/.build-cache"
        "${SCRIPT_DIR}/node_modules/.vite"
    )
    
    for path in "${paths_to_clean[@]}"; do
        if [[ -d "${path}" ]] || [[ -f "${path}" ]]; then
            log INFO "Removing ${path}..."
            rm -rf "${path}"
            log SUCCESS "Cleaned $(basename ${path})"
        fi
    done
}

install_dependencies() {
    log HEADER "Installing Dependencies"
    
    # Install Node dependencies
    log INFO "Installing Node.js dependencies..."
    if run_command "npm install" "${SCRIPT_DIR}"; then
        log SUCCESS "Node.js dependencies installed"
    else
        log ERROR "Failed to install Node.js dependencies"
        exit 1
    fi
    
    # Fetch Rust dependencies
    log INFO "Fetching Rust dependencies..."
    if run_command "cargo fetch" "${TAURI_DIR}"; then
        log SUCCESS "Rust dependencies ready"
    else
        log WARNING "Failed to fetch Rust dependencies (continuing anyway)"
    fi
}

run_tests() {
    if [[ "${SKIP_TESTS}" == true ]]; then
        log WARNING "Skipping tests (--skip-tests flag)"
        return
    fi
    
    log HEADER "Running Tests"
    
    # Frontend tests
    log INFO "Running frontend tests..."
    if run_command "npm run test -- --run" "${SCRIPT_DIR}"; then
        log SUCCESS "Frontend tests passed"
    else
        log ERROR "Frontend tests failed"
        exit 1
    fi
    
    # Backend tests
    log INFO "Running backend tests..."
    if run_command "cargo test" "${TAURI_DIR}"; then
        log SUCCESS "Backend tests passed"
    else
        log ERROR "Backend tests failed"
        exit 1
    fi
}

build_application() {
    log HEADER "Building Lattice Desktop Application"
    
    # Prepare build command
    local build_cmd=""
    
    if [[ "${NO_BUNDLE}" == true ]]; then
        # Build without bundling (faster)
        log INFO "Building without bundler..."
        
        build_cmd="cargo build"
        
        if [[ "${MODE}" != "dev" ]]; then
            build_cmd="${build_cmd} --release"
        fi
        
        if [[ -n "${FEATURES}" ]]; then
            build_cmd="${build_cmd} --features ${FEATURES}"
        fi
        
        if [[ -n "${TARGET}" ]]; then
            log INFO "Installing Rust target: ${TARGET}"
            run_command "rustup target add ${TARGET}" "${TAURI_DIR}"
            build_cmd="${build_cmd} --target ${TARGET}"
        fi
        
        if run_command "${build_cmd}" "${TAURI_DIR}"; then
            log SUCCESS "Build completed successfully"
        else
            log ERROR "Build failed"
            exit 1
        fi
    else
        # Build with Tauri bundler
        log INFO "Building with Tauri bundler..."
        
        build_cmd="npm run tauri:build"
        
        local tauri_args=""
        
        if [[ "${MODE}" == "dev" ]]; then
            tauri_args="${tauri_args} --debug"
        fi
        
        if [[ -n "${FEATURES}" ]]; then
            tauri_args="${tauri_args} --features ${FEATURES}"
        fi
        
        if [[ -n "${TARGET}" ]]; then
            log INFO "Installing Rust target: ${TARGET}"
            run_command "rustup target add ${TARGET}" "${TAURI_DIR}"
            tauri_args="${tauri_args} --target ${TARGET}"
        fi
        
        if [[ -n "${tauri_args}" ]]; then
            build_cmd="${build_cmd} -- ${tauri_args}"
        fi
        
        # Tauri expects CI=true/false, not CI=1. Unset so we don't pass invalid --ci 1.
        [[ "${CI:-}" == "1" ]] && unset CI
        if run_command "${build_cmd}" "${SCRIPT_DIR}"; then
            log SUCCESS "Build completed successfully"
        else
            log ERROR "Build failed"
            exit 1
        fi
    fi
}

verify_bundle() {
    if [[ "${NO_BUNDLE}" == true ]] || [[ "$(detect_platform)" != "macos" ]]; then
        return
    fi

    log HEADER "Verifying macOS Release Artifacts"

    local bundle_subdir
    if [[ "${MODE}" == "dev" ]]; then
        bundle_subdir="debug"
    else
        bundle_subdir="release"
    fi

    local bundle_dir="${TAURI_DIR}/target/${bundle_subdir}/bundle"
    local app_bundles=("${bundle_dir}/macos/"*.app)
    local dmg_bundles=("${bundle_dir}/dmg/"*.dmg)

    if [[ ! -d "${app_bundles[0]}" ]]; then
        log ERROR "No macOS app bundle found in ${bundle_dir}/macos"
        exit 1
    fi

    if [[ ! -f "${dmg_bundles[0]}" ]]; then
        log ERROR "No macOS disk image found in ${bundle_dir}/dmg"
        exit 1
    fi

    for app_bundle in "${app_bundles[@]}"; do
        log INFO "Checking code signature: $(basename "${app_bundle}")"
        if codesign --verify --deep --strict --verbose=2 "${app_bundle}" >> "${LOG_FILE}" 2>&1; then
            log SUCCESS "Code signature is valid: $(basename "${app_bundle}")"
        else
            log ERROR "Invalid code signature: ${app_bundle}"
            exit 1
        fi
    done

    for dmg_bundle in "${dmg_bundles[@]}"; do
        log INFO "Checking disk image: $(basename "${dmg_bundle}")"
        if hdiutil verify "${dmg_bundle}" >> "${LOG_FILE}" 2>&1; then
            log SUCCESS "Disk image is valid: $(basename "${dmg_bundle}")"
        else
            log ERROR "Invalid disk image: ${dmg_bundle}"
            exit 1
        fi
    done
}

generate_checksums() {
    if [[ "${CHECKSUMS}" != true ]]; then
        return
    fi
    
    log HEADER "Generating Checksums"
    
    local bundle_subdir
    if [[ "${MODE}" == "dev" ]]; then
        bundle_subdir="debug"
    else
        bundle_subdir="release"
    fi
    
    local bundle_dir="${TAURI_DIR}/target/${bundle_subdir}/bundle"
    
    if [[ ! -d "${bundle_dir}" ]]; then
        log WARNING "Bundle directory not found, skipping checksums"
        return
    fi
    
    mkdir -p "${DIST_DIR}"
    local checksum_file="${DIST_DIR}/checksums.txt"
    
    > "${checksum_file}"  # Clear file
    
    # Find all installer files
    local installer_patterns=()
    case "$(detect_platform)" in
        macos)
            installer_patterns=("*.dmg" "*.app")
            ;;
        linux)
            installer_patterns=("*.deb" "*.appimage" "*.rpm" "*.tar.gz")
            ;;
    esac
    
    for pattern in "${installer_patterns[@]}"; do
        while IFS= read -r -d '' file; do
            if [[ -f "${file}" ]]; then
                local hash=$(shasum -a 256 "${file}" | awk '{print $1}')
                local relative_path=$(realpath --relative-to="${bundle_dir}" "${file}" 2>/dev/null || \
                                     python3 -c "import os.path; print(os.path.relpath('${file}', '${bundle_dir}'))")
                echo "${hash}  ${relative_path}" >> "${checksum_file}"
                log INFO "$(basename ${file}): ${hash}"
            fi
        done < <(find "${bundle_dir}" -name "${pattern}" -print0 2>/dev/null)
    done
    
    if [[ -s "${checksum_file}" ]]; then
        log SUCCESS "Checksums written to ${checksum_file}"
    else
        log WARNING "No installer files found for checksum generation"
    fi
}

show_summary() {
    log HEADER "Build Summary"
    
    local end_time=$(date +%s)
    local duration=$((end_time - START_TIME))
    local minutes=$((duration / 60))
    local seconds=$((duration % 60))
    
    log SUCCESS "Total build time: ${minutes}m ${seconds}s"
    
    # Find and display output locations
    local target_subdir
    if [[ "${MODE}" == "dev" ]]; then
        target_subdir="debug"
    else
        target_subdir="release"
    fi
    
    local target_dir="${TAURI_DIR}/target/${target_subdir}"
    
    if [[ -d "${target_dir}" ]]; then
        log INFO "Build artifacts location: ${target_dir}"
        
        # List bundle files
        local bundle_dir="${target_dir}/bundle"
        if [[ -d "${bundle_dir}" ]]; then
            log INFO "Generated installers:"
            
            case "$(detect_platform)" in
                macos)
                    find "${bundle_dir}" \( -name "*.dmg" -o -name "*.app" \) -prune | while read -r file; do
                        local size=$(du -sh "${file}" | cut -f1)
                        log INFO "  - $(basename ${file}) (${size})"
                    done
                    ;;
                linux)
                    find "${bundle_dir}" -name "*.deb" -o -name "*.appimage" -o -name "*.rpm" | while read -r file; do
                        local size=$(du -h "${file}" | cut -f1)
                        log INFO "  - $(basename ${file}) (${size})"
                    done
                    ;;
            esac
        fi
    fi
    
    log INFO "Build log saved to: ${LOG_FILE}"
}

################################################################################
# Argument Parsing
################################################################################

show_help() {
    cat << EOF

${CYAN}Lattice Desktop - Advanced Build System (Unix/Linux/macOS)${RESET}

${WHITE}Usage:${RESET}
  ./build.sh [options]

${WHITE}Options:${RESET}
  ${GREEN}--mode <dev|release|production>${RESET}
      Build mode (default: release)
      
  ${GREEN}--platform <mac|linux|current>${RESET}
      Target platform (default: current)
      
  ${GREEN}--clean${RESET}
      Clean build artifacts before building
      
  ${GREEN}--skip-tests${RESET}
      Skip running tests
      
  ${GREEN}--no-bundle${RESET}
      Skip creating installers (faster builds)
      
  ${GREEN}--features <feat1,feat2>${RESET}
      Enable specific features (comma-separated)
      
  ${GREEN}--target <triple>${RESET}
      Specific Rust target triple
      Examples: x86_64-apple-darwin, aarch64-apple-darwin
      
  ${GREEN}--verbose${RESET}
      Enable verbose logging
      
  ${GREEN}--checksums${RESET}
      Generate SHA256 checksums for build artifacts
      
  ${GREEN}--parallel${RESET}
      Enable parallel builds (default)
      
  ${GREEN}--config <path>${RESET}
      Custom config file path (default: build.config.json)
      
  ${GREEN}--help${RESET}
      Show this help message

${WHITE}Examples:${RESET}
  # Simple release build
  ./build.sh
  
  # Clean build with tests
  ./build.sh --clean --mode release
  
  # Development build without bundling
  ./build.sh --mode dev --no-bundle
  
  # Build with specific features
  ./build.sh --features embeddings,file_watcher
  
  # macOS universal binary (Intel + Apple Silicon)
  ./build.sh --platform mac --target aarch64-apple-darwin
  
  # Verbose build with checksums
  ./build.sh --verbose --checksums

${WHITE}Available Features:${RESET}
  embeddings      - ML embeddings and semantic search
  file_watcher    - Automatic file watching
  sync            - Cloud sync (experimental)
  telemetry       - OpenTelemetry observability

EOF
}

parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --mode)
                MODE="$2"
                shift 2
                ;;
            --platform)
                PLATFORM="$2"
                shift 2
                ;;
            --clean)
                CLEAN=true
                shift
                ;;
            --skip-tests)
                SKIP_TESTS=true
                shift
                ;;
            --no-bundle)
                NO_BUNDLE=true
                shift
                ;;
            --features)
                FEATURES="$2"
                shift 2
                ;;
            --target)
                TARGET="$2"
                shift 2
                ;;
            --verbose)
                VERBOSE=true
                shift
                ;;
            --checksums)
                CHECKSUMS=true
                shift
                ;;
            --parallel)
                PARALLEL=true
                shift
                ;;
            --config)
                CONFIG_FILE="$2"
                shift 2
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log WARNING "Unknown option: $1"
                shift
                ;;
        esac
    done
    
    # Set platform if not specified
    if [[ -z "${PLATFORM}" ]]; then
        PLATFORM=$(detect_platform)
    fi
}

################################################################################
# Main Build Process
################################################################################

main() {
    parse_arguments "$@"
    
    # Show banner
    cat << "EOF"

╔══════════════════════════════════════════════════════════╗
║                                                          ║
║       Lattice Desktop Build System (Unix/Linux/macOS)     ║
║                                                          ║
╚══════════════════════════════════════════════════════════╝

EOF
    
    log HEADER "Lattice Desktop Build System - ${PLATFORM}"
    log INFO "Mode: ${MODE}"
    log INFO "Platform: ${PLATFORM}"
    log INFO "Bundle: $([[ "${NO_BUNDLE}" == true ]] && echo "false" || echo "true")"
    [[ -n "${FEATURES}" ]] && log INFO "Features: ${FEATURES}"
    [[ -n "${TARGET}" ]] && log INFO "Target: ${TARGET}"
    log INFO "Log file: ${LOG_FILE}"
    
    # Execute build steps
    check_prerequisites
    clean_build
    install_dependencies
    run_tests
    build_application
    verify_bundle
    generate_checksums
    show_summary
    
    log HEADER "Build Completed Successfully! 🎉"
    echo -e "\n${GREEN}✨ All done! ✨${RESET}\n"
}

# Run main function
main "$@"
