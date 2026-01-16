#!/bin/bash
# Benchmark script for fast-yaml vs yamlfmt comparison
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BENCH_DIR="$(dirname "$SCRIPT_DIR")"
ROOT_DIR="$(dirname "$(dirname "$BENCH_DIR")")"
FY="$ROOT_DIR/target/release/fy"
CORPUS="$BENCH_DIR/corpus/generated"
RESULTS="$BENCH_DIR/results"

# Check for hyperfine
if ! command -v hyperfine &> /dev/null; then
    echo "Error: hyperfine not found. Install with: brew install hyperfine"
    exit 1
fi

# Check for yamlfmt
if ! command -v yamlfmt &> /dev/null; then
    echo "Error: yamlfmt not found. Install with: brew install yamlfmt"
    exit 1
fi

# Ensure release build exists
if [ ! -f "$FY" ]; then
    echo "Building fast-yaml CLI..."
    cargo build --release -p fast-yaml-cli --manifest-path "$ROOT_DIR/Cargo.toml"
fi

# Create results directory
mkdir -p "$RESULTS"

# Print system info
echo "=== fast-yaml vs yamlfmt Benchmark ==="
echo ""
echo "Platform: $(uname -m) / $(uname -s) $(uname -r)"

# CPU info (cross-platform)
if [ "$(uname -s)" = "Darwin" ]; then
    CPU=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "Unknown")
else
    CPU=$(grep 'model name' /proc/cpuinfo 2>/dev/null | head -1 | cut -d: -f2 | xargs || echo "Unknown")
fi
echo "CPU: $CPU"
echo ""
echo "Tool versions:"
echo "  fast-yaml: $($FY --version 2>&1)"
echo "  yamlfmt: $(yamlfmt --version 2>&1)"
echo "  hyperfine: $(hyperfine --version 2>&1 | head -1)"
echo ""
echo "Configuration:"
echo "  Warmup runs: 3"
echo "  Measured runs: 20"
echo ""

# Generate timestamp for results
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Run benchmarks
for size in small medium large; do
    FILE="$CORPUS/${size}_0.yaml"
    if [ ! -f "$FILE" ]; then
        echo "Warning: $FILE not found, skipping"
        continue
    fi

    # Get file size (cross-platform)
    if [ "$(uname -s)" = "Darwin" ]; then
        FILESIZE=$(stat -f%z "$FILE")
    else
        FILESIZE=$(stat -c%s "$FILE")
    fi

    echo "=== ${size} ($FILESIZE bytes) ==="
    echo ""

    hyperfine --warmup 3 --runs 20 \
        -n "fast-yaml" "$FY format $FILE > /dev/null" \
        -n "yamlfmt" "yamlfmt -dry -in $FILE > /dev/null" \
        --export-json "$RESULTS/${size}_${TIMESTAMP}.json" \
        --export-markdown "$RESULTS/${size}_${TIMESTAMP}.md"

    echo ""
done

# Create summary
SUMMARY="$RESULTS/summary_${TIMESTAMP}.md"
cat > "$SUMMARY" << EOF
# Benchmark Results

**Date:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")
**Platform:** $(uname -m) / $(uname -s) $(uname -r)
**CPU:** $CPU

## Tool versions

- fast-yaml: $($FY --version 2>&1)
- yamlfmt: $(yamlfmt --version 2>&1)

## Results

EOF

for size in small medium large; do
    FILE="$CORPUS/${size}_0.yaml"
    if [ -f "$FILE" ]; then
        if [ "$(uname -s)" = "Darwin" ]; then
            FILESIZE=$(stat -f%z "$FILE")
        else
            FILESIZE=$(stat -c%s "$FILE")
        fi
        echo "### ${size} ($FILESIZE bytes)" >> "$SUMMARY"
        echo "" >> "$SUMMARY"
        cat "$RESULTS/${size}_${TIMESTAMP}.md" >> "$SUMMARY"
        echo "" >> "$SUMMARY"
    fi
done

echo "Results saved to:"
echo "  - $RESULTS/summary_${TIMESTAMP}.md"
echo "  - $RESULTS/*_${TIMESTAMP}.json"
