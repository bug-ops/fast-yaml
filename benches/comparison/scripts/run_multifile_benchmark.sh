#!/bin/bash
# Multi-file benchmark: fast-yaml (parallel) vs yamlfmt (sequential)
#
# This benchmark demonstrates fast-yaml's key advantage: parallel file processing.
# yamlfmt processes files sequentially, while fast-yaml can leverage all CPU cores.
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
echo "=== Multi-file Benchmark: fast-yaml vs yamlfmt ==="
echo ""
echo "This benchmark compares parallel vs sequential file processing."
echo "fast-yaml processes files in parallel using all CPU cores."
echo "yamlfmt processes files sequentially (one at a time)."
echo ""
echo "Platform: $(uname -m) / $(uname -s) $(uname -r)"

# CPU info (cross-platform)
if [ "$(uname -s)" = "Darwin" ]; then
    CPU=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "Unknown")
    CORES=$(sysctl -n hw.ncpu 2>/dev/null || echo "Unknown")
else
    CPU=$(grep 'model name' /proc/cpuinfo 2>/dev/null | head -1 | cut -d: -f2 | xargs || echo "Unknown")
    CORES=$(nproc 2>/dev/null || echo "Unknown")
fi
echo "CPU: $CPU"
echo "CPU Cores: $CORES"
echo ""
echo "Tool versions:"
echo "  fast-yaml: $($FY --version 2>&1)"
echo "  yamlfmt: $(yamlfmt --version 2>&1)"
echo "  hyperfine: $(hyperfine --version 2>&1 | head -1)"
echo ""

# Generate timestamp for results
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Check if multi-file corpus exists
if [ ! -d "$CORPUS/multifile_small" ]; then
    echo "Multi-file corpus not found. Generating..."
    python3 "$SCRIPT_DIR/generate_corpus.py"
    echo ""
fi

# Run benchmarks for each multi-file corpus
for corpus_name in multifile_small multifile_medium multifile_large multifile_xl; do
    CORPUS_DIR="$CORPUS/$corpus_name"

    if [ ! -d "$CORPUS_DIR" ]; then
        echo "Warning: $CORPUS_DIR not found, skipping"
        continue
    fi

    # Count files and total size
    FILE_COUNT=$(find "$CORPUS_DIR" -name "*.yaml" | wc -l | tr -d ' ')
    TOTAL_SIZE=$(find "$CORPUS_DIR" -name "*.yaml" -exec cat {} \; | wc -c | tr -d ' ')

    echo "=== $corpus_name ($FILE_COUNT files, $TOTAL_SIZE bytes total) ==="
    echo ""

    # fast-yaml: processes all files (internally can parallelize)
    # yamlfmt: processes files sequentially when given a directory

    # Get CPU count (cross-platform)
    if [ "$(uname -s)" = "Darwin" ]; then
        NCPU=$(sysctl -n hw.ncpu)
    else
        NCPU=$(nproc)
    fi

    hyperfine --warmup 2 --runs 10 \
        -n "fast-yaml (parallel)" "find $CORPUS_DIR -name '*.yaml' -print0 | xargs -0 -P $NCPU -I {} $FY format {} > /dev/null" \
        -n "fast-yaml (sequential)" "find $CORPUS_DIR -name '*.yaml' -print0 | xargs -0 -I {} $FY format {} > /dev/null" \
        -n "yamlfmt" "yamlfmt -dry $CORPUS_DIR > /dev/null 2>&1" \
        --export-json "$RESULTS/${corpus_name}_${TIMESTAMP}.json" \
        --export-markdown "$RESULTS/${corpus_name}_${TIMESTAMP}.md" \
        2>&1 || true

    echo ""
done

# Create summary report
SUMMARY="$RESULTS/multifile_summary_${TIMESTAMP}.md"
cat > "$SUMMARY" << EOF
# Multi-file Benchmark Results

**Date:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")
**Platform:** $(uname -m) / $(uname -s) $(uname -r)
**CPU:** $CPU
**CPU Cores:** $CORES

## Overview

This benchmark compares multi-file processing performance:
- **fast-yaml (parallel)**: Uses xargs with parallel execution
- **fast-yaml (sequential)**: Processes files one at a time
- **yamlfmt**: Processes files sequentially (default behavior)

## Tool versions

- fast-yaml: $($FY --version 2>&1)
- yamlfmt: $(yamlfmt --version 2>&1)

## Results

EOF

for corpus_name in multifile_small multifile_medium multifile_large multifile_xl; do
    CORPUS_DIR="$CORPUS/$corpus_name"
    if [ -d "$CORPUS_DIR" ]; then
        FILE_COUNT=$(find "$CORPUS_DIR" -name "*.yaml" | wc -l | tr -d ' ')
        TOTAL_SIZE=$(find "$CORPUS_DIR" -name "*.yaml" -exec cat {} \; | wc -c | tr -d ' ')

        echo "### $corpus_name ($FILE_COUNT files, $TOTAL_SIZE bytes)" >> "$SUMMARY"
        echo "" >> "$SUMMARY"

        if [ -f "$RESULTS/${corpus_name}_${TIMESTAMP}.md" ]; then
            cat "$RESULTS/${corpus_name}_${TIMESTAMP}.md" >> "$SUMMARY"
        fi
        echo "" >> "$SUMMARY"
    fi
done

cat >> "$SUMMARY" << EOF

## Analysis

The parallel speedup factor depends on:
1. **CPU core count**: More cores = higher speedup
2. **File size**: Larger files benefit more from parallelization
3. **File count**: More files provide better work distribution
4. **I/O characteristics**: SSD vs HDD affects parallel I/O

Expected speedup on multi-core systems:
- 4 cores: ~3-3.5x faster
- 8 cores: ~6-7x faster
- 16 cores: ~10-12x faster
EOF

echo "Results saved to:"
echo "  - $SUMMARY"
echo "  - $RESULTS/*_${TIMESTAMP}.json"
