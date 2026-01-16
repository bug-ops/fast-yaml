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

The benchmark corpus consists of three YAML files of varying sizes:

| File | Size | Description |
|------|------|-------------|
| `small_0.yaml` | 480 B | Simple key-value pairs |
| `medium_0.yaml` | 45 KB | Nested structures, lists |
| `large_0.yaml` | 459 KB | Complex document with deep nesting |

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

### Quick benchmark

Run all benchmarks with default settings:

```bash
cd benches/comparison
./scripts/run_benchmark.sh
```

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

**Platform:** Apple M3 Pro, macOS 26.2
**Date:** 2026-01-16
**Versions:** fast-yaml 0.3.3, yamlfmt 0.21.0

| File Size | fast-yaml | yamlfmt | Winner |
|-----------|-----------|---------|--------|
| Small (480B) | **1.8 ms** | 2.9 ms | fast-yaml (1.6x) |
| Medium (45KB) | **2.7 ms** | 2.9 ms | fast-yaml (1.1x) |
| Large (459KB) | 9.1 ms | **3.0 ms** | yamlfmt (3.0x) |

> [!NOTE]
> fast-yaml uses streaming formatter for files >1KB. Large file performance improved from 4.7x to 3.0x slower through Phase 1-3 optimizations.

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
├── README.md              # This file
├── corpus/
│   └── generated/         # Benchmark input files
│       ├── small_0.yaml
│       ├── medium_0.yaml
│       └── large_0.yaml
├── results/               # Benchmark output
│   ├── small.json
│   ├── medium.json
│   ├── large.json
│   └── *.md
└── scripts/
    ├── generate_corpus.py
    └── run_benchmark.sh
```

## Contributing

To add new benchmarks:

1. Add corpus files to `corpus/generated/`
2. Update `scripts/run_benchmark.sh`
3. Run benchmarks and commit results
4. Update this README with new results
