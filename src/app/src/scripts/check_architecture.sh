#!/bin/bash
# Local architecture quality checks
# Mirrors the GitHub Actions workflow for local validation

set -euo pipefail

ARCH_CHECK_TMP_DIR="$(mktemp -d)"
trap 'rm -rf -- "$ARCH_CHECK_TMP_DIR"' EXIT

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_DIR"

echo "🏗️  Running Architecture Quality Checks"
echo "======================================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

FAILED_CHECKS=0
PASSED_CHECKS=0

# Check 1: Mock Isolation
echo "📋 Check 1: Mock Isolation"
echo "--------------------------"
MOCK_FILES=$(find src -type f -name "mock_*.rs" -o -path "*/mocks/*.rs" 2>/dev/null || true)

if [ -n "$MOCK_FILES" ]; then
    VIOLATIONS=0
    while IFS= read -r file; do
        if ! grep -q "#\[cfg(test)\]" "$file" && ! grep -q "#\[cfg(any(test" "$file"; then
            echo -e "${RED}❌ $file missing #[cfg(test)]${NC}"
            VIOLATIONS=$((VIOLATIONS + 1))
        fi
    done <<< "$MOCK_FILES"

    if [ $VIOLATIONS -eq 0 ]; then
        echo -e "${GREEN}✅ All mock files properly isolated${NC}"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        echo -e "${RED}❌ Found $VIOLATIONS mock files without #[cfg(test)]${NC}"
        FAILED_CHECKS=$((FAILED_CHECKS + 1))
    fi
else
    echo -e "${YELLOW}⚠️  No mock files found${NC}"
    PASSED_CHECKS=$((PASSED_CHECKS + 1))
fi
echo ""

# Check 2: Production Build
echo "📋 Check 2: Production Build"
echo "----------------------------"
if cargo build --release --lib --quiet; then
    echo -e "${GREEN}✅ Release build succeeded${NC}"
    PASSED_CHECKS=$((PASSED_CHECKS + 1))

    # Check for test dependencies
    if cargo tree --edges normal --no-dev-dependencies 2>/dev/null | grep -q "mockall\|proptest\|criterion"; then
        echo -e "${RED}❌ Test dependencies found in production build${NC}"
        FAILED_CHECKS=$((FAILED_CHECKS + 1))
    else
        echo -e "${GREEN}✅ No test dependencies in production${NC}"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    fi
else
    echo -e "${RED}❌ Release build failed${NC}"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi
echo ""

# Check 3: Clippy Architecture Lints
echo "📋 Check 3: Clippy Architecture Lints"
echo "-------------------------------------"
CLIPPY_LOG="$ARCH_CHECK_TMP_DIR/clippy.log"
if cargo clippy --all-targets --all-features --quiet -- \
    -D warnings \
    -W clippy::all \
    -W clippy::pedantic \
    -A clippy::missing-errors-doc \
    -A clippy::missing-panics-doc \
    -A clippy::module-name-repetitions \
    -D clippy::unwrap-used \
    -D clippy::expect-used \
    -D clippy::panic \
    -D clippy::todo \
    -D clippy::unimplemented \
    >"$CLIPPY_LOG" 2>&1; then
    echo -e "${GREEN}✅ Clippy checks passed${NC}"
    PASSED_CHECKS=$((PASSED_CHECKS + 1))
else
    echo -e "${RED}❌ Clippy checks failed${NC}"
    head -n 20 "$CLIPPY_LOG"
    echo "See errors above. Common fixes:"
    echo "  - Replace .unwrap() with proper error handling"
    echo "  - Replace .expect() with Result propagation"
    echo "  - Remove panic!(), todo!(), unimplemented!()"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi
echo ""

# Check 4: Documentation
echo "📋 Check 4: Documentation"
echo "------------------------"
if [ -f "../ARCHITECTURE.md" ]; then
    echo -e "${GREEN}✅ ARCHITECTURE.md exists${NC}"
    PASSED_CHECKS=$((PASSED_CHECKS + 1))

    # Check required sections
    DOC="../ARCHITECTURE.md"
    REQUIRED=("Domain-Driven Design" "SOLID Principles" "Service Container" "Dependency Injection" "Security")
    MISSING=0

    for section in "${REQUIRED[@]}"; do
        if ! grep -qi "$section" "$DOC"; then
            echo -e "${YELLOW}⚠️  Missing section: $section${NC}"
            MISSING=$((MISSING + 1))
        fi
    done

    if [ $MISSING -eq 0 ]; then
        echo -e "${GREEN}✅ All required sections present${NC}"
        PASSED_CHECKS=$((PASSED_CHECKS + 1))
    else
        echo -e "${YELLOW}⚠️  $MISSING sections missing or incomplete${NC}"
    fi
else
    echo -e "${RED}❌ ARCHITECTURE.md not found${NC}"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi
echo ""

# Check 5: Feature Combinations
echo "📋 Check 5: Feature Combinations"
echo "--------------------------------"
FEATURE_ERRORS=0

echo "  Checking default features..."
if cargo check --release --quiet 2>/dev/null; then
    echo -e "  ${GREEN}✓${NC} Default"
else
    echo -e "  ${RED}✗${NC} Default"
    FEATURE_ERRORS=$((FEATURE_ERRORS + 1))
fi

echo "  Checking no-default-features..."
if cargo check --release --no-default-features --quiet 2>/dev/null; then
    echo -e "  ${GREEN}✓${NC} No defaults"
else
    echo -e "  ${RED}✗${NC} No defaults"
    FEATURE_ERRORS=$((FEATURE_ERRORS + 1))
fi

for feature in search indexing qa extraction; do
    echo "  Checking feature: $feature..."
    if cargo check --release --no-default-features --features "$feature" --quiet 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} $feature"
    else
        echo -e "  ${RED}✗${NC} $feature"
        FEATURE_ERRORS=$((FEATURE_ERRORS + 1))
    fi
done

if [ $FEATURE_ERRORS -eq 0 ]; then
    echo -e "${GREEN}✅ All feature combinations compile${NC}"
    PASSED_CHECKS=$((PASSED_CHECKS + 1))
else
    echo -e "${RED}❌ $FEATURE_ERRORS feature combination(s) failed${NC}"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi
echo ""

# Summary
echo "======================================="
echo "🎯 Architecture Quality Summary"
echo "======================================="
echo -e "Passed: ${GREEN}$PASSED_CHECKS${NC}"
echo -e "Failed: ${RED}$FAILED_CHECKS${NC}"
echo ""

if [ $FAILED_CHECKS -eq 0 ]; then
    echo -e "${GREEN}✅ All architecture checks passed!${NC}"
    echo "Ready to push to CI."
    exit 0
else
    echo -e "${RED}❌ Some architecture checks failed${NC}"
    echo "Please fix the issues above before pushing."
    exit 1
fi
