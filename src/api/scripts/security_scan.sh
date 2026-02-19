#!/bin/bash
# Automated Security Scanning Script
# Runs static analysis, dependency scanning, and SAST tools

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo "========================================="
echo "  Automated Security Scanning"
echo "========================================="
echo ""

# Create reports directory
REPORTS_DIR="$PROJECT_ROOT/security_reports"
mkdir -p "$REPORTS_DIR"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Track results
ISSUES_FOUND=0

# 1. Bandit - Python Security Linter
echo -e "${YELLOW}1. Running Bandit (Python Security Linter)...${NC}"
if command -v bandit &> /dev/null; then
    bandit -r src/ -f json -o "$REPORTS_DIR/bandit_${TIMESTAMP}.json" || true
    bandit -r src/ -f txt -o "$REPORTS_DIR/bandit_${TIMESTAMP}.txt" || true

    # Count issues
    if [ -f "$REPORTS_DIR/bandit_${TIMESTAMP}.json" ]; then
        BANDIT_ISSUES=$(python3 -c "import json; data=json.load(open('$REPORTS_DIR/bandit_${TIMESTAMP}.json')); print(len(data.get('results', [])))" 2>/dev/null || echo "0")
        echo "Found $BANDIT_ISSUES security issues"
        ISSUES_FOUND=$((ISSUES_FOUND + BANDIT_ISSUES))
    fi
    echo -e "${GREEN}✓ Bandit scan complete${NC}"
else
    echo -e "${RED}✗ Bandit not installed${NC}"
    echo "Install with: pip install bandit"
fi
echo ""

# 2. Safety - Dependency Vulnerability Scanner
echo -e "${YELLOW}2. Running Safety (Dependency Scanner)...${NC}"
if command -v safety &> /dev/null; then
    safety check --json > "$REPORTS_DIR/safety_${TIMESTAMP}.json" 2>&1 || true
    safety check --full-report > "$REPORTS_DIR/safety_${TIMESTAMP}.txt" 2>&1 || true

    # Count vulnerabilities
    if [ -f "$REPORTS_DIR/safety_${TIMESTAMP}.json" ]; then
        SAFETY_ISSUES=$(python3 -c "import json; data=json.load(open('$REPORTS_DIR/safety_${TIMESTAMP}.json')); print(len(data.get('vulnerabilities', [])))" 2>/dev/null || echo "0")
        echo "Found $SAFETY_ISSUES vulnerable dependencies"
        ISSUES_FOUND=$((ISSUES_FOUND + SAFETY_ISSUES))
    fi
    echo -e "${GREEN}✓ Safety scan complete${NC}"
else
    echo -e "${RED}✗ Safety not installed${NC}"
    echo "Install with: pip install safety"
fi
echo ""

# 3. Semgrep - SAST Scanner
echo -e "${YELLOW}3. Running Semgrep (SAST)...${NC}"
if command -v semgrep &> /dev/null; then
    semgrep --config=auto src/ --json > "$REPORTS_DIR/semgrep_${TIMESTAMP}.json" 2>&1 || true
    semgrep --config=auto src/ > "$REPORTS_DIR/semgrep_${TIMESTAMP}.txt" 2>&1 || true

    # Count findings
    if [ -f "$REPORTS_DIR/semgrep_${TIMESTAMP}.json" ]; then
        SEMGREP_ISSUES=$(python3 -c "import json; data=json.load(open('$REPORTS_DIR/semgrep_${TIMESTAMP}.json')); print(len(data.get('results', [])))" 2>/dev/null || echo "0")
        echo "Found $SEMGREP_ISSUES code patterns"
        ISSUES_FOUND=$((ISSUES_FOUND + SEMGREP_ISSUES))
    fi
    echo -e "${GREEN}✓ Semgrep scan complete${NC}"
else
    echo -e "${RED}✗ Semgrep not installed${NC}"
    echo "Install with: pip install semgrep"
fi
echo ""

# 4. Check for secrets in code
echo -e "${YELLOW}4. Scanning for hardcoded secrets...${NC}"
if command -v gitleaks &> /dev/null; then
    gitleaks detect --source . --report-path "$REPORTS_DIR/gitleaks_${TIMESTAMP}.json" --no-git || true
    echo -e "${GREEN}✓ Secret scan complete${NC}"
else
    echo -e "${YELLOW}⚠ Gitleaks not installed (optional)${NC}"
    echo "Install from: https://github.com/gitleaks/gitleaks"
fi
echo ""

# 5. Check for outdated dependencies
echo -e "${YELLOW}5. Checking for outdated dependencies...${NC}"
pip list --outdated --format=json > "$REPORTS_DIR/outdated_${TIMESTAMP}.json" 2>&1 || true
pip list --outdated > "$REPORTS_DIR/outdated_${TIMESTAMP}.txt" 2>&1 || true
echo -e "${GREEN}✓ Dependency check complete${NC}"
echo ""

# 6. Custom security checks
echo -e "${YELLOW}6. Running custom security checks...${NC}"

# Check for debug mode in production
if grep -r "DEBUG.*=.*True" src/ 2>/dev/null; then
    echo -e "${RED}✗ Found DEBUG=True in source code${NC}"
    ISSUES_FOUND=$((ISSUES_FOUND + 1))
fi

# Check for hardcoded secrets patterns
PATTERNS=(
    "password.*=.*['\"]"
    "secret.*=.*['\"]"
    "api_key.*=.*['\"]"
    "token.*=.*['\"]"
)

for pattern in "${PATTERNS[@]}"; do
    if grep -rE "$pattern" src/ 2>/dev/null | grep -v "SECRET_KEY.*get_env" | grep -v "password.*Field" | head -5; then
        echo -e "${YELLOW}⚠ Potential hardcoded secret found (review manually)${NC}"
    fi
done

echo -e "${GREEN}✓ Custom checks complete${NC}"
echo ""

# Generate summary report
SUMMARY_FILE="$REPORTS_DIR/security_scan_summary_${TIMESTAMP}.txt"

cat > "$SUMMARY_FILE" << EOF
========================================
Security Scan Summary
========================================
Timestamp: $(date)
Project: Recall/Vault Backend

Scan Results:
-------------
1. Bandit (Python Security): $BANDIT_ISSUES issues
2. Safety (Dependencies): $SAFETY_ISSUES vulnerabilities
3. Semgrep (SAST): $SEMGREP_ISSUES findings

Total Issues Found: $ISSUES_FOUND

Detailed Reports:
-----------------
- Bandit: $REPORTS_DIR/bandit_${TIMESTAMP}.json
- Safety: $REPORTS_DIR/safety_${TIMESTAMP}.json
- Semgrep: $REPORTS_DIR/semgrep_${TIMESTAMP}.json

Recommendations:
----------------
1. Review all high/critical severity issues immediately
2. Update vulnerable dependencies
3. Fix hardcoded secrets
4. Implement security fixes
5. Re-run scans after fixes

========================================
EOF

cat "$SUMMARY_FILE"

echo ""
echo "Reports saved to: $REPORTS_DIR"
echo "Summary: $SUMMARY_FILE"
echo ""

# Exit code based on issues found
if [ $ISSUES_FOUND -gt 0 ]; then
    echo -e "${YELLOW}⚠ Security scan found $ISSUES_FOUND issues${NC}"
    echo "Review reports and address issues"
    exit 1
else
    echo -e "${GREEN}✓ No security issues found!${NC}"
    exit 0
fi
