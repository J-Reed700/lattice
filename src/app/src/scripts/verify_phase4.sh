#!/bin/bash
# verify_phase4.sh - Verify Phase 4 API Documentation Completion
#
# This script verifies that Phase 4 (API Documentation) meets all success criteria:
# - 0 compilation errors
# - 0 compilation warnings
# - 0 clippy warnings
# - Documentation builds without warnings
# - All tests pass
# - 100% command documentation coverage (aspirational)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=========================================="
echo "Phase 4 Verification Script"
echo "API Documentation Quality Checks"
echo "=========================================="
echo ""

ERRORS=0

# Function to print success
success() {
    echo -e "${GREEN}✅ PASSED${NC}: $1"
}

# Function to print failure
failure() {
    echo -e "${RED}❌ FAILED${NC}: $1"
    ERRORS=$((ERRORS + 1))
}

# Function to print warning
warning() {
    echo -e "${YELLOW}⚠️  WARNING${NC}: $1"
}

# 1. Check compilation (errors and warnings)
echo "1. Checking compilation..."
if OUTPUT=$(cargo build 2>&1); then
    if echo "$OUTPUT" | grep -q "warning:"; then
        failure "Compilation warnings found"
        echo "$OUTPUT" | grep "warning:"
    else
        success "0 compilation errors, 0 warnings"
    fi
else
    failure "Compilation errors found"
    echo "$OUTPUT"
fi
echo ""

# 2. Check documentation build
echo "2. Checking documentation build..."
if OUTPUT=$(cargo doc --no-deps 2>&1); then
    if echo "$OUTPUT" | grep -q "warning:"; then
        failure "Documentation warnings found"
        echo "$OUTPUT" | grep "warning:"
    else
        success "Documentation builds without warnings"
    fi
else
    failure "Documentation build failed"
    echo "$OUTPUT"
fi
echo ""

# 3. Check clippy
echo "3. Checking clippy..."
if OUTPUT=$(cargo clippy -- -D warnings 2>&1); then
    success "0 clippy warnings"
else
    failure "Clippy warnings found"
    echo "$OUTPUT"
fi
echo ""

# 4. Run tests
echo "4. Running tests..."
if OUTPUT=$(cargo test --lib 2>&1); then
    success "All tests pass"
else
    failure "Test failures found"
    echo "$OUTPUT"
fi
echo ""

# 5. Count documented commands
echo "5. Analyzing documentation coverage..."

# Count total commands (exclude mod.rs, consolidated.rs, command_tests.rs)
TOTAL_COMMANDS=$(find src/interfaces/commands -name "*.rs" \
    ! -name "mod.rs" \
    ! -name "consolidated.rs" \
    ! -name "command_tests.rs" \
    -exec grep -c "^#\[tauri::command\]" {} + | \
    awk '{sum += $1} END {print sum}')

# Count commands with rustdoc (/// comment immediately before #[tauri::command])
DOCUMENTED=0
for file in src/interfaces/commands/*.rs; do
    # Skip excluded files
    if [[ "$file" =~ (mod|consolidated|command_tests)\.rs$ ]]; then
        continue
    fi

    # Count commands with doc comments in this file
    # This is a simplified check - it counts /// comments before #[tauri::command]
    count=$(grep -B1 "^#\[tauri::command\]" "$file" 2>/dev/null | grep -c "^///" || true)
    DOCUMENTED=$((DOCUMENTED + count))
done

PERCENTAGE=$((DOCUMENTED * 100 / TOTAL_COMMANDS))

echo "Total commands found: $TOTAL_COMMANDS"
echo "Commands with rustdoc: $DOCUMENTED"
echo "Documentation coverage: ${PERCENTAGE}%"

if [ "$DOCUMENTED" -eq "$TOTAL_COMMANDS" ]; then
    success "100% documentation coverage achieved"
elif [ "$PERCENTAGE" -ge 90 ]; then
    warning "Documentation coverage ${PERCENTAGE}% (target: 100%)"
else
    warning "Documentation coverage ${PERCENTAGE}% - needs improvement"
fi
echo ""

# 6. Check for TypeScript examples (sample check)
echo "6. Checking for TypeScript examples..."
EXAMPLE_COUNT=$(grep -r "# Example" src/interfaces/commands/*.rs 2>/dev/null | wc -l | tr -d ' ')
TYPESCRIPT_COUNT=$(grep -r '```typescript' src/interfaces/commands/*.rs 2>/dev/null | wc -l | tr -d ' ')

echo "Commands with '# Example' section: $EXAMPLE_COUNT"
echo "Commands with TypeScript examples: $TYPESCRIPT_COUNT"

if [ "$TYPESCRIPT_COUNT" -ge "$((TOTAL_COMMANDS * 80 / 100))" ]; then
    success "Good TypeScript example coverage (${TYPESCRIPT_COUNT}/${TOTAL_COMMANDS})"
else
    warning "TypeScript examples need improvement (${TYPESCRIPT_COUNT}/${TOTAL_COMMANDS})"
fi
echo ""

# 7. Check for security documentation
echo "7. Checking for security documentation..."
SECURITY_SECTIONS=$(grep -r "# Security" src/interfaces/commands/*.rs 2>/dev/null | wc -l | tr -d ' ')

echo "Commands with '# Security' section: $SECURITY_SECTIONS"

if [ "$SECURITY_SECTIONS" -ge 30 ]; then
    success "Good security documentation coverage"
else
    warning "More security documentation recommended (found $SECURITY_SECTIONS sections)"
fi
echo ""

# Summary
echo "=========================================="
echo "Verification Summary"
echo "=========================================="

if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}✅ All critical checks passed!${NC}"
    echo ""
    echo "Phase 4 Status:"
    echo "  - Compilation: PASSING (0 errors, 0 warnings)"
    echo "  - Documentation build: PASSING"
    echo "  - Clippy: PASSING (0 warnings)"
    echo "  - Tests: PASSING"
    echo "  - Documentation coverage: ${PERCENTAGE}%"
    echo ""
    exit 0
else
    echo -e "${RED}❌ $ERRORS critical check(s) failed${NC}"
    echo ""
    echo "Please fix the failures before proceeding."
    exit 1
fi
