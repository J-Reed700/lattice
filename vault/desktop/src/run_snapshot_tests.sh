#!/bin/bash

# Snapshot Testing Runner
# Comprehensive test runner for insta snapshot tests

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "========================================="
echo "Recall Desktop Snapshot Tests"
echo "========================================="
echo ""

# Color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to run a test suite
run_test() {
    local test_name=$1
    echo -e "${YELLOW}Running: $test_name${NC}"
    if cargo test --test "$test_name" -- --nocapture; then
        echo -e "${GREEN}✓ $test_name passed${NC}"
        echo ""
        return 0
    else
        echo -e "${RED}✗ $test_name failed${NC}"
        echo ""
        return 1
    fi
}

# Parse command line arguments
COMMAND=${1:-"test"}
TEST_FILTER=${2:-""}

case $COMMAND in
    test)
        echo "Running all snapshot tests..."
        echo ""

        if [ -n "$TEST_FILTER" ]; then
            echo "Filter: $TEST_FILTER"
            cargo test --test search_results_test "$TEST_FILTER" -- --nocapture
            cargo test --test indexing_output_test "$TEST_FILTER" -- --nocapture
            cargo test --test audit_events_test "$TEST_FILTER" -- --nocapture
        else
            run_test "search_results_test"
            run_test "indexing_output_test"
            run_test "audit_events_test"
        fi
        ;;

    review)
        echo "Reviewing snapshot changes..."
        echo ""
        if ! command -v cargo-insta &> /dev/null; then
            echo -e "${YELLOW}cargo-insta not found. Installing...${NC}"
            cargo install cargo-insta
        fi

        if [ -n "$TEST_FILTER" ]; then
            cargo insta review --test "$TEST_FILTER"
        else
            cargo insta review
        fi
        ;;

    accept)
        echo "Accepting all snapshot changes..."
        echo ""
        if ! command -v cargo-insta &> /dev/null; then
            echo -e "${RED}Error: cargo-insta not found${NC}"
            echo "Install with: cargo install cargo-insta"
            exit 1
        fi
        cargo insta accept
        echo -e "${GREEN}✓ All snapshots accepted${NC}"
        ;;

    reject)
        echo "Rejecting all snapshot changes..."
        echo ""
        if ! command -v cargo-insta &> /dev/null; then
            echo -e "${RED}Error: cargo-insta not found${NC}"
            echo "Install with: cargo install cargo-insta"
            exit 1
        fi
        cargo insta reject
        echo -e "${GREEN}✓ All snapshots rejected${NC}"
        ;;

    check)
        echo "Checking for pending snapshots..."
        echo ""
        if ! command -v cargo-insta &> /dev/null; then
            echo -e "${RED}Error: cargo-insta not found${NC}"
            echo "Install with: cargo install cargo-insta"
            exit 1
        fi
        cargo insta test --check
        ;;

    clean)
        echo "Cleaning snapshot artifacts..."
        echo ""
        find tests/snapshots -name "*.snap.new" -delete
        find tests/snapshots -name "*.snap.pending" -delete
        echo -e "${GREEN}✓ Cleaned snapshot artifacts${NC}"
        ;;

    list)
        echo "Snapshot test files:"
        echo ""
        ls -1 tests/*_test.rs | grep -E "(search_results|indexing_output|audit_events)" || true
        echo ""
        echo "Snapshot files:"
        echo ""
        ls -1 tests/snapshots/*.snap 2>/dev/null || echo "No snapshots generated yet"
        ;;

    help|*)
        cat << EOF
Usage: ./run_snapshot_tests.sh [COMMAND] [OPTIONS]

Commands:
    test [FILTER]    Run all snapshot tests (or filtered tests)
    review [TEST]    Review snapshot changes interactively
    accept           Accept all pending snapshot changes
    reject           Reject all pending snapshot changes
    check            Check if any snapshots are pending
    clean            Remove .snap.new and .snap.pending files
    list             List all snapshot test files and snapshots
    help             Show this help message

Examples:
    ./run_snapshot_tests.sh test
        Run all snapshot tests

    ./run_snapshot_tests.sh test hybrid_search
        Run tests matching "hybrid_search"

    ./run_snapshot_tests.sh review
        Review all pending snapshots interactively

    ./run_snapshot_tests.sh review search_results_test
        Review snapshots for specific test file

    ./run_snapshot_tests.sh accept
        Accept all pending snapshot changes

    ./run_snapshot_tests.sh check
        Check for pending snapshots (useful in CI)

Requirements:
    - cargo-insta: Install with 'cargo install cargo-insta'

For more information, see tests/snapshots/README.md
EOF
        ;;
esac

echo ""
echo "========================================="
echo "Done!"
echo "========================================="
