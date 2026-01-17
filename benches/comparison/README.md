# fast-yaml vs yamlfmt Benchmark

Reproducible performance comparison between fast-yaml and [google/yamlfmt](https://github.com/google/yamlfmt).

## Prerequisites

### Required tools

```bash
# Install hyperfine (benchmark runner)
brew install hyperfine

# Install yamlfmt
brew install yamlfmt

# Verify installations
hyperfine --version
yamlfmt --version
```

### Build fast-yaml

```bash
# From repository root
cargo build --release -p fast-yaml-cli

# Verify
./target/release/fy --version
```

## Corpus

### Single-file corpus

The benchmark corpus consists of three YAML files of varying sizes:

| File | Size | Description |
|------|------|-------------|
| `small_0.yaml` | 480 B | Simple key-value pairs |
| `medium_0.yaml` | 45 KB | Nested structures, lists |
| `large_0.yaml` | 459 KB | Complex document with deep nesting |

### Multi-file corpus

For parallel processing benchmarks, additional corpus directories simulate real projects:

| Directory | Files | Size per file | Total | Use case |
|-----------|-------|---------------|-------|----------|
| `multifile_small/` | 50 | 500 B | ~25 KB | Small project |
| `multifile_medium/` | 200 | 1 KB | ~200 KB | Medium project |
| `multifile_large/` | 500 | 2 KB | ~1 MB | Large project |
| `multifile_xl/` | 1000 | 1 KB | ~1 MB | Enterprise project |

### Generate corpus

If corpus files are missing, generate them:

```bash
cd benches/comparison
python3 scripts/generate_corpus.py
```

<details>
<summary><b>Corpus generator script</b></summary>

Create `scripts/generate_corpus.py`:

```python
#!/usr/bin/env python3
"""Generate benchmark corpus files."""

import os
import random
import string

def random_string(length: int) -> str:
    return ''.join(random.choices(string.ascii_lowercase, k=length))

def generate_yaml(target_size: int) -> str:
    """Generate YAML content of approximately target_size bytes."""
    lines = []
    current_size = 0
    item_count = 0

    while current_size < target_size:
        if item_count % 3 == 0:
            key = random_string(8)
            value = random_string(random.randint(10, 50))
            line = f"{key}: {value}"
        elif item_count % 3 == 1:
            key = random_string(8)
            subkey = random_string(6)
            value = random.randint(1, 10000)
            line = f"{key}:\n  {subkey}: {value}"
        else:
            key = random_string(8)
            items = [random_string(10) for _ in range(3)]
            line = f"{key}:\n" + "\n".join(f"  - {item}" for item in items)

        lines.append(line)
        current_size += len(line) + 1
        item_count += 1

    return "\n".join(lines)

def main():
    os.makedirs("corpus/generated", exist_ok=True)

    sizes = {
        "small": 480,
        "medium": 45_000,
        "large": 460_000,
    }

    for name, size in sizes.items():
        content = generate_yaml(size)
        filename = f"corpus/generated/{name}_0.yaml"
        with open(filename, "w") as f:
            f.write(content)
        print(f"Generated {filename}: {len(content)} bytes")

if __name__ == "__main__":
    main()
```

</details>

## Running benchmarks

### Single-file benchmark

Run single-file benchmarks with default settings:

```bash
cd benches/comparison
./scripts/run_benchmark.sh
```

### Batch mode benchmark (native parallel processing)

Run batch mode benchmarks to compare fast-yaml's native batch processing vs yamlfmt:

```bash
cd benches/comparison
./scripts/run_batch_benchmark.sh
```

This benchmark demonstrates fast-yaml's key advantage: **native batch mode with parallel workers**.
- fast-yaml uses built-in batch mode with `-j` flag for parallel processing
- yamlfmt processes files sequentially

Expected speedup on multi-core systems:
- **50-200 files**: ~2-6x faster
- **500-1000 files**: ~13-16x faster
- **Scales with CPU cores**: More cores = higher speedup

### Manual benchmark

Run individual file benchmarks:

```bash
# Small file
hyperfine --warmup 3 --runs 20 \
  -n "fast-yaml" "../../target/release/fy format corpus/generated/small_0.yaml > /dev/null" \
  -n "yamlfmt" "yamlfmt -dry -in corpus/generated/small_0.yaml > /dev/null"

# Medium file
hyperfine --warmup 3 --runs 20 \
  -n "fast-yaml" "../../target/release/fy format corpus/generated/medium_0.yaml > /dev/null" \
  -n "yamlfmt" "yamlfmt -dry -in corpus/generated/medium_0.yaml > /dev/null"

# Large file
hyperfine --warmup 3 --runs 20 \
  -n "fast-yaml" "../../target/release/fy format corpus/generated/large_0.yaml > /dev/null" \
  -n "yamlfmt" "yamlfmt -dry -in corpus/generated/large_0.yaml > /dev/null"
```

### Export results

Export benchmark results to JSON or markdown:

```bash
hyperfine --warmup 3 --runs 20 \
  -n "fast-yaml" "../../target/release/fy format corpus/generated/large_0.yaml > /dev/null" \
  -n "yamlfmt" "yamlfmt -dry -in corpus/generated/large_0.yaml > /dev/null" \
  --export-json results/large.json \
  --export-markdown results/large.md
```

## Methodology

### Benchmark configuration

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Warmup runs | 3 | Eliminate cold-start effects |
| Measured runs | 20 | Statistical significance |
| Output | `/dev/null` | Exclude I/O from measurement |
| Shell | Default | Include process startup overhead |

### What is measured

- **Total wall-clock time** from process start to exit
- Includes: argument parsing, file reading, formatting, output writing
- Excludes: shell startup (calibrated by hyperfine)

### Fair comparison

Both tools are configured for equivalent operations:

| Setting | fast-yaml | yamlfmt |
|---------|-----------|---------|
| Mode | Format only | Format only (`-dry`) |
| Input | File path | File path (`-in`) |
| Output | stdout | stdout |
| Indent | 2 spaces (default) | 2 spaces (default) |

## Interpreting results

### Metrics

| Metric | Description |
|--------|-------------|
| Mean | Average execution time |
| σ (sigma) | Standard deviation |
| Min/Max | Fastest/slowest run |
| Relative | Ratio compared to fastest tool |

### Example output

```
Benchmark 1: fast-yaml
  Time (mean ± σ):       2.7 ms ±   0.3 ms
  Range (min … max):     2.5 ms …   3.5 ms

Benchmark 2: yamlfmt
  Time (mean ± σ):       2.9 ms ±   0.3 ms
  Range (min … max):     2.7 ms …   3.6 ms

Summary
  fast-yaml ran 1.08 ± 0.15 times faster than yamlfmt
```

### Performance factors

Results may vary based on:

- **CPU architecture** — ARM (Apple Silicon) vs x86
- **File system** — SSD vs HDD, cached vs uncached
- **System load** — Background processes affect timing
- **Tool versions** — Performance changes between releases

## Latest results

**Platform:** Apple M3 Pro (12 cores), macOS 25.2
**Date:** 2026-01-17
**Versions:** fast-yaml 0.3.3, yamlfmt 0.21.0

### Single-file benchmarks

| File Size | fast-yaml | yamlfmt | Result |
|-----------|-----------|---------|--------|
| Small (502 bytes) | **1.7 ms** | 3.1 ms | **fast-yaml 1.80x faster** ✓ |
| Medium (45 KB) | **2.5 ms** | 2.9 ms | **fast-yaml 1.19x faster** ✓ |
| Large (460 KB) | 8.4 ms | **2.9 ms** | yamlfmt 2.88x faster |

> [!NOTE]
> yamlfmt is optimized for large single files. fast-yaml excels at batch processing multiple files.

### Batch mode benchmarks (native parallel processing)

> [!TIP]
> Batch mode is where fast-yaml truly shines with parallel workers.

| Workload | fast-yaml (parallel -j 12) | yamlfmt (sequential) | Speedup |
|----------|---------------------------|----------------------|---------|
| 50 files (26 KB total) | **4.3 ms** | 10.3 ms | **2.40x faster** ✓ |
| 200 files (204 KB total) | **8.0 ms** | 52.7 ms | **6.63x faster** ✓ |
| 500 files (1 MB total) | **15.5 ms** | 244.7 ms | **15.77x faster** ⚡ |
| 1000 files (1 MB total) | **23.4 ms** | 323.4 ms | **13.80x faster** ⚡ |

**Key takeaway:** Native batch mode with parallel workers provides 6-15x speedup on multi-file operations, making fast-yaml ideal for formatting entire codebases.

### Python API benchmarks

Comparison of Python bindings for YAML parsing and serialization.

**Setup and run:**

```bash
cd benches/comparison

# Setup (one-time)
bash scripts/setup_python_benchmarks.sh

# Run benchmarks
bash scripts/run_python_benchmark.sh
```

See [scripts/README.md](scripts/README.md) for detailed instructions.

**Results (example):**

| Operation | File Size | fast-yaml | PyYAML (C) | PyYAML (pure) | Speedup (vs C) | Speedup (vs pure) |
|-----------|-----------|-----------|------------|---------------|----------------|-------------------|
| Parse | Small (502B) | **125 μs** | 286 μs | 1,848 μs | **2.3x** | **14.7x** |
| Parse | Medium (44KB) | **2.34 ms** | 5.43 ms | 28.76 ms | **2.3x** | **12.3x** |
| Parse | Large (449KB) | **23.8 ms** | 54.2 ms | 287.4 ms | **2.3x** | **12.1x** |
| Dump | Small (502B) | **99 μs** | 235 μs | 1,524 μs | **2.4x** | **15.4x** |
| Dump | Medium (44KB) | **1.87 ms** | 4.78 ms | 24.32 ms | **2.6x** | **13.0x** |
| Dump | Large (449KB) | **18.9 ms** | 47.6 ms | 243.7 ms | **2.5x** | **12.9x** |

**Key findings:**
- **2.3-2.6x faster** than PyYAML with C LibYAML extension
- **12-15x faster** than pure Python PyYAML
- Consistent performance across file sizes
- Both parsing and serialization operations show similar speedups

See [results/python_summary_EXAMPLE.md](results/python_summary_EXAMPLE.md) for detailed analysis.

## Scripts

<details>
<summary><b>Full benchmark script</b></summary>

Create `scripts/run_benchmark.sh`:

```bash
#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BENCH_DIR="$(dirname "$SCRIPT_DIR")"
ROOT_DIR="$(dirname "$(dirname "$BENCH_DIR")")"
FY="$ROOT_DIR/target/release/fy"
CORPUS="$BENCH_DIR/corpus/generated"
RESULTS="$BENCH_DIR/results"

# Ensure release build exists
if [ ! -f "$FY" ]; then
    echo "Building fast-yaml CLI..."
    cargo build --release -p fast-yaml-cli --manifest-path "$ROOT_DIR/Cargo.toml"
fi

# Create results directory
mkdir -p "$RESULTS"

# Print system info
echo "=== Benchmark Configuration ==="
echo "Platform: $(uname -m) / $(uname -s) $(uname -r)"
echo "CPU: $(sysctl -n machdep.cpu.brand_string 2>/dev/null || cat /proc/cpuinfo | grep 'model name' | head -1 | cut -d: -f2)"
echo ""
echo "Tool versions:"
echo "  fast-yaml: $($FY --version)"
echo "  yamlfmt: $(yamlfmt --version)"
echo "  hyperfine: $(hyperfine --version)"
echo ""

# Run benchmarks
for size in small medium large; do
    FILE="$CORPUS/${size}_0.yaml"
    if [ ! -f "$FILE" ]; then
        echo "Warning: $FILE not found, skipping"
        continue
    fi

    FILESIZE=$(stat -f%z "$FILE" 2>/dev/null || stat -c%s "$FILE")
    echo "=== ${size} ($FILESIZE bytes) ==="

    hyperfine --warmup 3 --runs 20 \
        -n "fast-yaml" "$FY format $FILE > /dev/null" \
        -n "yamlfmt" "yamlfmt -dry -in $FILE > /dev/null" \
        --export-json "$RESULTS/${size}.json" \
        --export-markdown "$RESULTS/${size}.md"

    echo ""
done

echo "Results saved to $RESULTS/"
```

</details>

## Directory structure

```
benches/comparison/
├── README.md                    # This file
├── corpus/
│   └── generated/               # Benchmark input files
│       ├── small_0.yaml         # Single-file corpus
│       ├── medium_0.yaml
│       ├── large_0.yaml
│       ├── multifile_small/     # Multi-file corpus (50 files)
│       ├── multifile_medium/    # Multi-file corpus (200 files)
│       ├── multifile_large/     # Multi-file corpus (500 files)
│       └── multifile_xl/        # Multi-file corpus (1000 files)
├── results/                     # Benchmark output
│   ├── small_*.json
│   ├── medium_*.json
│   ├── large_*.json
│   ├── multifile_*_*.json       # Multi-file results
│   └── *.md
└── scripts/
    ├── README.md                # Script documentation
    ├── generate_corpus.py       # Generates both single and multi-file corpus
    ├── run_benchmark.sh         # Single-file benchmarks (CLI)
    ├── run_batch_benchmark.sh   # Batch mode benchmarks (native parallel processing)
    ├── run_multifile_benchmark.sh  # Multi-file benchmarks (xargs parallelization)
    ├── setup_python_benchmarks.sh  # Python environment setup
    └── run_python_benchmark.sh  # Python API benchmarks
```

## Contributing

To add new benchmarks:

1. Add corpus files to `corpus/generated/`
2. Update `scripts/run_benchmark.sh`
3. Run benchmarks and commit results
4. Update this README with new results
