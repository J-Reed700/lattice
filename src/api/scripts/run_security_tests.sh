#!/bin/bash
# Security Test Runner Script
# Runs comprehensive security test suite with reporting

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo "======================================"
echo "  Recall/Vault Security Test Suite"
echo "======================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if pytest is installed
if ! command -v pytest &> /dev/null; then
    echo -e "${RED}Error: pytest not found${NC}"
    echo "Install with: pip install pytest pytest-asyncio pytest-cov"
    exit 1
fi

# Create reports directory
REPORTS_DIR="$PROJECT_ROOT/security_reports"
mkdir -p "$REPORTS_DIR"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

echo "Reports will be saved to: $REPORTS_DIR"
echo ""

# Function to run test category
run_test_category() {
    local category=$1
    local test_file=$2
    local description=$3

    echo -e "${YELLOW}Running $category...${NC}"
    echo "Description: $description"

    if pytest "tests/security/$test_file" -v --tb=short \
        --html="$REPORTS_DIR/${category}_${TIMESTAMP}.html" \
        --self-contained-html 2>&1 | tee "$REPORTS_DIR/${category}_${TIMESTAMP}.log"; then
        echo -e "${GREEN}✓ $category PASSED${NC}"
        return 0
    else
        echo -e "${RED}✗ $category FAILED${NC}"
        return 1
    fi
    echo ""
}

# Track results
TOTAL_CATEGORIES=0
PASSED_CATEGORIES=0
FAILED_CATEGORIES=0

# Run each test category
echo "========================================="
echo "1. Authentication Security Tests"
echo "========================================="
if run_test_category "authentication" "test_authentication.py" "Login, token validation, registration security"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "2. Authorization Tests"
echo "========================================="
if run_test_category "authorization" "test_authorization.py" "Access control, privilege escalation prevention"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "3. Path Traversal Tests"
echo "========================================="
if run_test_category "path_traversal" "test_path_traversal.py" "Directory traversal, symlink attacks"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "4. CSRF Protection Tests"
echo "========================================="
if run_test_category "csrf" "test_csrf.py" "Cross-site request forgery protection"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "5. Rate Limiting Tests"
echo "========================================="
if run_test_category "rate_limiting" "test_rate_limiting.py" "Brute force prevention, rate limits"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "6. Input Validation Tests"
echo "========================================="
if run_test_category "input_validation" "test_input_validation.py" "Malformed data, type validation"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "7. File Upload Security Tests"
echo "========================================="
if run_test_category "file_upload" "test_file_upload_security.py" "Malicious files, MIME spoofing, zip bombs"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "8. Injection Attack Tests"
echo "========================================="
if run_test_category "injection" "test_injection.py" "SQL, command, template injection"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "9. API Security Tests"
echo "========================================="
if run_test_category "api_security" "test_api_security.py" "Security headers, CORS, information disclosure"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

echo "========================================="
echo "10. Security Integration Tests"
echo "========================================="
if run_test_category "integration" "test_security_integration.py" "End-to-end security workflows"; then
    ((PASSED_CATEGORIES++))
else
    ((FAILED_CATEGORIES++))
fi
((TOTAL_CATEGORIES++))

# Generate comprehensive coverage report
echo ""
echo "========================================="
echo "Generating Coverage Report"
echo "========================================="
pytest tests/security/ \
    --cov=src \
    --cov-report=html:"$REPORTS_DIR/coverage_${TIMESTAMP}" \
    --cov-report=term \
    --cov-report=json:"$REPORTS_DIR/coverage_${TIMESTAMP}.json" \
    -v

# Summary
echo ""
echo "========================================="
echo "           SECURITY TEST SUMMARY"
echo "========================================="
echo "Total Categories:  $TOTAL_CATEGORIES"
echo -e "${GREEN}Passed:           $PASSED_CATEGORIES${NC}"
if [ $FAILED_CATEGORIES -gt 0 ]; then
    echo -e "${RED}Failed:           $FAILED_CATEGORIES${NC}"
else
    echo "Failed:           $FAILED_CATEGORIES"
fi
echo ""
echo "Reports saved in: $REPORTS_DIR"
echo ""

# Calculate pass rate
PASS_RATE=$((PASSED_CATEGORIES * 100 / TOTAL_CATEGORIES))
echo "Pass Rate: $PASS_RATE%"
echo ""

# Exit with appropriate code
if [ $FAILED_CATEGORIES -eq 0 ]; then
    echo -e "${GREEN}All security tests PASSED! ✓${NC}"
    exit 0
else
    echo -e "${RED}Some security tests FAILED! ✗${NC}"
    echo "Review reports in: $REPORTS_DIR"
    exit 1
fi
