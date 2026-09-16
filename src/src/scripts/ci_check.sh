#!/usr/bin/env bash
set -euo pipefail

# CI Ratchet Check Script
# Enforces Immaculate Baseline standards for production code

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Track overall status
FAILED=0

echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  CI RATCHET: Immaculate Baseline Enforcement${NC}"
echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
echo ""

# Navigate to project root (script is in scripts/)
cd "$(dirname "$0")/.."

# Check 1: Production Code Clippy (Zero Warnings)
echo -e "${BLUE}[1/3] Production Code Clippy (Zero Warnings)${NC}"
echo "Running: cargo clippy --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used"
echo ""

if cargo clippy --lib --bins -- -D warnings -D clippy::unwrap_used -D clippy::expect_used 2>&1; then
    echo -e "${GREEN}✓ Production code clippy: PASS${NC}"
    echo ""
else
    echo -e "${RED}✗ Production code clippy: FAIL${NC}"
    echo -e "${YELLOW}Fix clippy warnings in production code (src/lib.rs, src/main.rs, src/bin/)${NC}"
    echo ""
    FAILED=1
fi

# Check 2: Tests Pass
echo -e "${BLUE}[2/3] Test Suite${NC}"
echo "Running: cargo test"
echo ""

if cargo test --lib --bins 2>&1; then
    echo -e "${GREEN}✓ Tests: PASS${NC}"
    echo ""
else
    echo -e "${RED}✗ Tests: FAIL${NC}"
    echo -e "${YELLOW}Fix failing tests before committing${NC}"
    echo ""
    FAILED=1
fi

# Check 3: Zero-Panic Policy (Production Code Only)
echo -e "${BLUE}[3/3] Zero-Panic Policy (Production Code)${NC}"
echo "Checking for unwrap/expect in src/ (excluding tests)..."
echo ""

# Search for unwrap/expect in production code (exclude test files)
UNWRAP_COUNT=$(rg '\.unwrap\(\)' src/ -g '!*test*.rs' -c 2>/dev/null | awk -F: '{sum += $2} END {print sum+0}')
EXPECT_COUNT=$(rg '\.expect\(' src/ -g '!*test*.rs' -c 2>/dev/null | awk -F: '{sum += $2} END {print sum+0}')
TOTAL=$((UNWRAP_COUNT + EXPECT_COUNT))

if [ "$TOTAL" -eq 0 ]; then
    echo -e "${GREEN}✓ Zero-Panic Policy: PASS (0 unwrap/expect in production code)${NC}"
    echo ""
else
    echo -e "${RED}✗ Zero-Panic Policy: FAIL${NC}"
    echo -e "${YELLOW}Found $UNWRAP_COUNT unwrap() and $EXPECT_COUNT expect() in production code${NC}"
    echo ""
    echo "Top offenders:"
    rg '\.unwrap\(\)|\.expect\(' src/ -g '!*test*.rs' --count-matches 2>/dev/null | sort -t: -k2 -rn | head -10
    echo ""
    echo -e "${YELLOW}unwrap/expect are ALLOWED in tests, but FORBIDDEN in production code${NC}"
    echo ""
    FAILED=1
fi

# Summary
echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
if [ "$FAILED" -eq 0 ]; then
    echo -e "${GREEN}✓ CI RATCHET: ALL CHECKS PASSED${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
    exit 0
else
    echo -e "${RED}✗ CI RATCHET: SOME CHECKS FAILED${NC}"
    echo -e "${BLUE}════════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo -e "${YELLOW}Run 'just ci-quick' to fix issues before committing${NC}"
    exit 1
fi
