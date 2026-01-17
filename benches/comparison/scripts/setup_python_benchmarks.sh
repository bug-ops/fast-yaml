#!/bin/bash
# Setup script for Python benchmarks
# Run this before executing run_python_benchmark.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$(dirname "$(dirname "$SCRIPT_DIR")")")"

echo "=== Setting up Python benchmarks ==="
echo ""

# Check if venv exists
if [ ! -d "$ROOT_DIR/.venv" ]; then
    echo "Creating Python virtual environment..."
    python3 -m venv "$ROOT_DIR/.venv"
fi

echo "Installing PyYAML with uv..."
cd "$ROOT_DIR"
uv pip install pyyaml

echo "Building fast-yaml Python bindings..."
cd "$ROOT_DIR"
uv run maturin develop --manifest-path python/Cargo.toml

echo ""
echo "Setup complete! You can now run:"
echo "  bash $SCRIPT_DIR/run_python_benchmark.sh"
echo ""
