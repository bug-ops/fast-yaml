#!/usr/bin/env python3
"""Generate benchmark corpus files for fast-yaml vs yamlfmt comparison."""

import random
import string
from pathlib import Path


def random_string(length: int) -> str:
    """Generate a random lowercase string."""
    return "".join(random.choices(string.ascii_lowercase, k=length))


def generate_yaml(target_size: int, file_index: int = 0) -> str:
    """Generate YAML content of approximately target_size bytes."""
    lines = [f"# Generated file {file_index}"]
    current_size = len(lines[0]) + 1
    item_count = 0

    while current_size < target_size:
        if item_count % 3 == 0:
            # Simple key-value
            key = random_string(8)
            value = random_string(random.randint(10, 50))
            line = f"{key}: {value}"
        elif item_count % 3 == 1:
            # Nested mapping
            key = random_string(8)
            subkey = random_string(6)
            value = random.randint(1, 10000)
            line = f"{key}:\n  {subkey}: {value}"
        else:
            # List
            key = random_string(8)
            items = [random_string(10) for _ in range(3)]
            line = f"{key}:\n" + "\n".join(f"  - {item}" for item in items)

        lines.append(line)
        current_size += len(line) + 1
        item_count += 1

    return "\n".join(lines)


def generate_single_file_corpus(corpus_dir: Path):
    """Generate single-file corpus for basic benchmarks."""
    sizes = {
        "small": 480,
        "medium": 45_000,
        "large": 460_000,
    }

    for name, size in sizes.items():
        content = generate_yaml(size)
        filename = corpus_dir / f"{name}_0.yaml"
        filename.write_text(content)
        print(f"Generated {filename}: {len(content)} bytes")


def generate_multifile_corpus(corpus_dir: Path):
    """Generate multi-file corpus for parallel processing benchmarks.

    This simulates real-world scenarios:
    - Small project: 50 files × 500 bytes (~25KB total)
    - Medium project: 200 files × 1KB (~200KB total)
    - Large project: 500 files × 2KB (~1MB total)
    - XL project: 1000 files × 1KB (~1MB total)
    """
    multifile_configs = {
        "multifile_small": (50, 500),       # 50 files × 500 bytes
        "multifile_medium": (200, 1000),    # 200 files × 1KB
        "multifile_large": (500, 2000),     # 500 files × 2KB
        "multifile_xl": (1000, 1000),       # 1000 files × 1KB
    }

    for name, (file_count, bytes_per_file) in multifile_configs.items():
        dir_path = corpus_dir / name
        dir_path.mkdir(parents=True, exist_ok=True)

        total_bytes = 0
        for i in range(file_count):
            content = generate_yaml(bytes_per_file, file_index=i)
            filename = dir_path / f"config_{i:04d}.yaml"
            filename.write_text(content)
            total_bytes += len(content)

        print(f"Generated {dir_path}: {file_count} files, {total_bytes:,} bytes total")


def main():
    # Determine output directory
    script_dir = Path(__file__).parent
    corpus_dir = script_dir.parent / "corpus" / "generated"
    corpus_dir.mkdir(parents=True, exist_ok=True)

    # Set seed for reproducibility
    random.seed(42)

    print("=== Single-file corpus ===")
    generate_single_file_corpus(corpus_dir)

    print("\n=== Multi-file corpus ===")
    generate_multifile_corpus(corpus_dir)


if __name__ == "__main__":
    main()
