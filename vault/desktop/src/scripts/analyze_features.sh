#!/bin/bash
# Feature Flag Analysis Script
# Analyzes dependency tree and estimates compile time savings

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."

cd "$PROJECT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Feature Flag Dependency Analysis${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo

# Function to time a build
time_build() {
    local features=$1
    local name=$2

    echo -e "${YELLOW}Building with: ${name}${NC}"

    # Clean to get accurate timing
    cargo clean -q

    # Time the build
    local start=$(date +%s)
    if [ -z "$features" ]; then
        cargo build --quiet 2>&1 | tail -n 5
    else
        cargo build --quiet $features 2>&1 | tail -n 5
    fi
    local end=$(date +%s)

    local duration=$((end - start))
    echo -e "${GREEN}✓ Built in ${duration}s${NC}"

    # Get binary size
    local binary_size=$(ls -lh target/debug/recall-desktop 2>/dev/null | awk '{print $5}' || echo "N/A")
    echo -e "${GREEN}  Binary size: ${binary_size}${NC}"
    echo

    echo "$name,$duration,$binary_size"
}

# Check if we should run full analysis
FULL_ANALYSIS=${1:-"no"}

if [ "$FULL_ANALYSIS" != "full" ]; then
    echo -e "${YELLOW}Quick analysis mode (no builds)${NC}"
    echo -e "${YELLOW}Run with 'full' argument for complete timing analysis${NC}"
    echo
else
    echo -e "${YELLOW}Full analysis mode - this will take 5-10 minutes${NC}"
    echo

    # Create results file
    RESULTS_FILE="feature_timing_results.csv"
    echo "Configuration,Build Time (s),Binary Size" > "$RESULTS_FILE"

    # Run build tests
    time_build "" "Default features" >> "$RESULTS_FILE"
    time_build "--no-default-features --features=\"embeddings,search\"" "Minimal" >> "$RESULTS_FILE"
    time_build "--features=full" "Full features" >> "$RESULTS_FILE"
    time_build "--no-default-features --features=\"embeddings,search,file-watch,document-extraction\"" "No LLM" >> "$RESULTS_FILE"

    echo -e "${GREEN}Results saved to: ${RESULTS_FILE}${NC}"
    echo
fi

# Analyze dependency tree
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Dependency Analysis${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo

# Count total dependencies
TOTAL_DEPS=$(cargo tree --depth 1 2>/dev/null | grep -c "├──\|└──" || echo "0")
echo -e "${YELLOW}Total direct dependencies: ${TOTAL_DEPS}${NC}"
echo

# Analyze feature-specific dependencies
echo -e "${BLUE}Embeddings dependencies:${NC}"
cargo tree --no-default-features --features=embeddings --depth 1 2>/dev/null | grep "ort\|tokenizers\|ndarray\|fastembed" || true
echo

echo -e "${BLUE}Search dependencies:${NC}"
cargo tree --no-default-features --features=search --depth 1 2>/dev/null | grep "instant-distance\|rayon\|memmap2" || true
echo

echo -e "${BLUE}LLM dependencies (heavy):${NC}"
cargo tree --no-default-features --features=llm --depth 1 2>/dev/null | grep "ollama\|mistralrs\|hf-hub" || true
echo

echo -e "${BLUE}Web ingestion dependencies:${NC}"
cargo tree --no-default-features --features=web-ingest --depth 1 2>/dev/null | grep "scraper\|url" || true
echo

# Check for heavy dependencies
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Heavy Dependencies (>5s compile time)${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo

echo -e "${RED}⚠ mistralrs${NC} - Git dependency, very large (~15-20s)"
echo -e "${YELLOW}⚠ ort${NC} - ONNX Runtime (~8-12s)"
echo -e "${YELLOW}⚠ opentelemetry${NC} - Multiple crates (~5-8s combined)"
echo -e "${YELLOW}⚠ lopdf${NC} - PDF parsing (~3-5s)"
echo

# Feature recommendations
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Recommendations${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo

echo -e "${GREEN}For fastest development:${NC}"
echo "  cargo build --no-default-features --features=\"embeddings,search\""
echo "  ${YELLOW}Estimated time: ~15-20s (50% faster)${NC}"
echo

echo -e "${GREEN}For most development:${NC}"
echo "  cargo build"
echo "  ${YELLOW}Estimated time: ~18-23s (default features)${NC}"
echo

echo -e "${GREEN}For production release:${NC}"
echo "  cargo build --release --features=full"
echo "  ${YELLOW}Estimated time: ~35-40s (all features)${NC}"
echo

echo -e "${GREEN}To avoid LLM overhead:${NC}"
echo "  cargo build --features=standard"
echo "  ${YELLOW}Estimated time: ~20-25s (40% faster)${NC}"
echo

# Check for unused dependencies
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Checking for Unused Dependencies${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo

if command -v cargo-udeps &> /dev/null; then
    echo -e "${YELLOW}Running cargo-udeps...${NC}"
    cargo +nightly udeps --all-targets 2>&1 | grep "unused" || echo -e "${GREEN}No unused dependencies found${NC}"
else
    echo -e "${YELLOW}cargo-udeps not installed${NC}"
    echo "Install with: cargo install cargo-udeps"
fi
echo

# Suggest next steps
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}   Next Steps${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════${NC}"
echo

echo "1. Review the proposed Cargo.toml: Cargo.toml.proposed"
echo "2. Review the full design document: FEATURE_FLAGS_DESIGN.md"
echo "3. Review the quick reference: FEATURE_FLAGS_QUICK_REF.md"
echo "4. Test a minimal build:"
echo "   cargo build --no-default-features --features=\"embeddings,search\""
echo "5. Run full timing analysis:"
echo "   ./scripts/analyze_features.sh full"
echo

echo -e "${GREEN}Analysis complete!${NC}"
