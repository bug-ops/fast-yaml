#!/bin/bash
# Batch mode benchmark: fast-yaml (native batch) vs yamlfmt
#
# This benchmark demonstrates fast-yaml's native batch processing with parallel workers
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
echo "=== Batch Mode Benchmark: fast-yaml vs yamlfmt ==="
echo ""
echo "This benchmark compares native batch processing (fast-yaml) vs sequential (yamlfmt)."
echo "fast-yaml uses native batch mode with parallel workers."
echo "yamlfmt processes files sequentially."
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
    echo "Multi-file corpus not found. Please run generate_corpus.py first."
    exit 1
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

    # fast-yaml: native batch mode with different worker counts
    # yamlfmt: sequential processing

    hyperfine --warmup 2 --runs 10 \
        -n "fast-yaml (sequential -j 0)" "$FY format -n -j 0 $CORPUS_DIR > /dev/null 2>&1" \
        -n "fast-yaml (parallel -j $CORES)" "$FY format -n -j $CORES $CORPUS_DIR > /dev/null 2>&1" \
        -n "yamlfmt" "yamlfmt -dry $CORPUS_DIR > /dev/null 2>&1" \
        --export-json "$RESULTS/batch_${corpus_name}_${TIMESTAMP}.json" \
        --export-markdown "$RESULTS/batch_${corpus_name}_${TIMESTAMP}.md" \
        2>&1 || true

    echo ""
done

# Create summary report
SUMMARY="$RESULTS/batch_summary_${TIMESTAMP}.md"
cat > "$SUMMARY" << EOF
# Batch Mode Benchmark Results

**Date:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")
**Platform:** $(uname -m) / $(uname -s) $(uname -r)
**CPU:** $CPU
**CPU Cores:** $CORES

## Overview

This benchmark compares batch/multi-file processing performance:
- **fast-yaml (sequential -j 0)**: Single-threaded batch processing
- **fast-yaml (parallel -j $CORES)**: Multi-threaded batch with $CORES workers
- **yamlfmt**: Sequential file processing (default behavior)

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

        if [ -f "$RESULTS/batch_${corpus_name}_${TIMESTAMP}.md" ]; then
            cat "$RESULTS/batch_${corpus_name}_${TIMESTAMP}.md" >> "$SUMMARY"
        fi
        echo "" >> "$SUMMARY"
    fi
done

cat >> "$SUMMARY" << EOF

## Analysis

Key observations:
1. **Parallel speedup**: fast-yaml with -j $CORES shows near-linear speedup on multi-core CPUs
2. **Sequential comparison**: fast-yaml sequential mode vs yamlfmt baseline performance
3. **Batch overhead**: Native batch mode eliminates per-file process startup overhead

Expected speedup with $CORES cores:
- Sequential baseline: ~1.0x
- Parallel (ideal): ~${CORES}x
- Parallel (realistic): ~$((CORES * 75 / 100))-$((CORES * 85 / 100))x (75-85% efficiency)

Factors affecting performance:
- File size distribution
- I/O characteristics (SSD vs HDD)
- Memory bandwidth
- Cache coherency
EOF

echo ""
echo "Results saved to:"
echo "  - $SUMMARY"
echo "  - $RESULTS/batch_*_${TIMESTAMP}.json"
