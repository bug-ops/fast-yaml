#!/usr/bin/env python3
"""Generate benchmark corpus files for fast-yaml vs yamlfmt comparison."""

import random
import string
from pathlib import Path


def random_string(length: int) -> str:
    """Generate a random lowercase string."""
    return "".join(random.choices(string.ascii_lowercase, k=length))


def generate_yaml(target_size: int) -> str:
    """Generate YAML content of approximately target_size bytes."""
    lines = []
    current_size = 0
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


def main():
    # Determine output directory
    script_dir = Path(__file__).parent
    corpus_dir = script_dir.parent / "corpus" / "generated"
    corpus_dir.mkdir(parents=True, exist_ok=True)

    # Define target sizes
    sizes = {
        "small": 480,
        "medium": 45_000,
        "large": 460_000,
    }

    # Set seed for reproducibility
    random.seed(42)

    for name, size in sizes.items():
        content = generate_yaml(size)
        filename = corpus_dir / f"{name}_0.yaml"
        filename.write_text(content)
        print(f"Generated {filename}: {len(content)} bytes")


if __name__ == "__main__":
    main()
