#!/bin/bash
# Unwrap Risk Audit Script
# Categorizes .unwrap() calls by risk level

set -e

# Ensure we have ripgrep available
if ! command -v rg &> /dev/null; then
    RG="/Users/joshreed/Library/pnpm/global/5/.pnpm/@anthropic-ai+claude-code@2.0.26/node_modules/@anthropic-ai/claude-code/vendor/ripgrep/arm64-darwin/rg"
    if [ ! -f "$RG" ]; then
        echo "Error: ripgrep (rg) not found"
        exit 1
    fi
else
    RG="rg"
fi

echo "=== UNWRAP RISK AUDIT ==="
echo "Date: $(date)"
echo ""

# CRITICAL: In command handlers (user-facing)
echo "🔴 CRITICAL - Commands (user-facing, high crash risk):"
if [ -d "src/interfaces/commands/" ]; then
    $RG "\.unwrap\(\)" src/interfaces/commands/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20 || echo "No unwraps found"
else
    $RG "\.unwrap\(\)" src/commands/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20 || echo "No commands directory found"
fi
echo ""

# CRITICAL: In public APIs
echo "🔴 CRITICAL - Public APIs:"
$RG "pub.*\.unwrap\(\)" src/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20 || echo "No unwraps found"
echo ""

# HIGH: In services with external I/O
echo "🟡 HIGH - Services (business logic, external I/O):"
if [ -d "src/application/services/" ]; then
    $RG "\.unwrap\(\)" src/application/services/ src/infrastructure/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20 || echo "No unwraps found"
elif [ -d "src/services/" ]; then
    $RG "\.unwrap\(\)" src/services/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20 || echo "No unwraps found"
else
    echo "No services directory found"
fi
echo ""

# HIGH: In loops (repeated crash risk)
echo "🟡 HIGH - Inside loops (sample):"
$RG "for\s" src/ -A 10 2>/dev/null | $RG "\.unwrap\(\)" | head -20 || echo "No unwraps in loops found"
echo ""

# MEDIUM: In initialization code
echo "🟠 MEDIUM - Initialization:"
if [ -d "src/infrastructure/setup/" ]; then
    $RG "\.unwrap\(\)" src/infrastructure/setup/ src/di/ --count-matches 2>/dev/null | sort -t: -k2 -rn || echo "No unwraps found"
elif [ -d "src/di/" ]; then
    $RG "\.unwrap\(\)" src/di/ --count-matches 2>/dev/null | sort -t: -k2 -rn || echo "No unwraps found"
else
    echo "No initialization directories found"
fi
echo ""

# LOW: In tests
echo "🟢 LOW - Tests:"
$RG "\.unwrap\(\)" src/ -g "*test*.rs" --count-matches 2>/dev/null | sort -t: -k2 -rn | head -10 || echo "No test unwraps found"
echo ""

# Summary statistics
echo "=== SUMMARY ==="
TOTAL=$($RG "\.unwrap\(\)" src/ --count-matches 2>/dev/null | awk -F: '{sum+=$2} END {print sum}')
FILES=$($RG "\.unwrap\(\)" src/ -l 2>/dev/null | wc -l | tr -d ' ')
COMMANDS=$($RG "\.unwrap\(\)" src/commands/ --count-matches 2>/dev/null | awk -F: '{sum+=$2} END {print sum}' || echo "0")
TESTS=$($RG "\.unwrap\(\)" src/ -g "*test*.rs" --count-matches 2>/dev/null | awk -F: '{sum+=$2} END {print sum}' || echo "0")

echo "Total unwraps: ${TOTAL:-0}"
echo "Files affected: $FILES"
echo "In commands: ${COMMANDS:-0} (highest priority)"
echo "In tests: ${TESTS:-0} (lowest priority)"
echo "Production code: $((${TOTAL:-0} - ${TESTS:-0}))"
echo ""

# Top offenders
echo "=== TOP 20 FILES ==="
$RG "\.unwrap\(\)" src/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -20
echo ""

# Critical patterns
echo "=== CRITICAL PATTERNS ==="
echo ""
echo "Option::unwrap() without validation:"
$RG "\.get\(.*\)\.unwrap\(\)" src/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -10 || echo "None found"
echo ""

echo "Result::unwrap() without error handling:"
$RG "\.lock\(\)\.unwrap\(\)" src/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -10 || echo "None found"
echo ""

echo "Expect with generic messages:"
$RG '\.expect\(".*"\)' src/ --count-matches 2>/dev/null | sort -t: -k2 -rn | head -10 || echo "None found"
echo ""

echo "=== AUDIT COMPLETE ==="
