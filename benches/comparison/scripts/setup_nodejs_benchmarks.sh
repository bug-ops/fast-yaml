#!/bin/bash
# Setup script for Node.js benchmarks
# Run this before executing run_nodejs_benchmark.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$(dirname "$(dirname "$SCRIPT_DIR")")")"
BENCH_DIR="$(dirname "$SCRIPT_DIR")"
NODEJS_DIR="$ROOT_DIR/nodejs"
BENCH_NODE_DIR="$BENCH_DIR/node_bench"

echo "=== Setting up Node.js benchmarks ==="
echo ""

# Check Node.js version
if ! command -v node &> /dev/null; then
    echo "Error: Node.js not found. Please install Node.js 20+"
    exit 1
fi

NODE_VERSION=$(node --version)
echo "Node.js version: $NODE_VERSION"

# Check if Node.js version is 20+
NODE_MAJOR=$(node --version | cut -d. -f1 | sed 's/v//')
if [ "$NODE_MAJOR" -lt 20 ]; then
    echo "Error: Node.js 20+ required, found v$NODE_MAJOR"
    exit 1
fi

# Create benchmark Node.js directory
echo "Creating benchmark Node.js directory..."
mkdir -p "$BENCH_NODE_DIR"
cd "$BENCH_NODE_DIR"

# Create package.json if it doesn't exist
if [ ! -f package.json ]; then
    cat > package.json << 'EOFJSON'
{
  "name": "fast-yaml-nodejs-benchmarks",
  "version": "1.0.0",
  "private": true,
  "description": "Node.js benchmarks for fast-yaml",
  "dependencies": {
    "js-yaml": "^4.1.0"
  }
}
EOFJSON
fi

# Install js-yaml for benchmarking
echo "Installing js-yaml..."
npm install

# Build fast-yaml Node.js bindings
echo "Building fast-yaml Node.js bindings..."
cd "$NODEJS_DIR"
npm install
npm run build

# Verify installations
echo ""
echo "Verifying installations..."
cd "$BENCH_NODE_DIR"

if node -e "require('js-yaml')" 2>/dev/null; then
    JSYAML_VERSION=$(node -e "console.log(require('js-yaml/package.json').version)")
    echo "  js-yaml: $JSYAML_VERSION"
else
    echo "Error: js-yaml installation failed"
    exit 1
fi

cd "$NODEJS_DIR"

if node -e "require('$NODEJS_DIR')" 2>/dev/null; then
    FASTYAML_VERSION=$(node -e "console.log(require('$NODEJS_DIR/package.json').version)")
    echo "  fastyaml-rs: $FASTYAML_VERSION"
else
    echo "Error: fastyaml-rs installation failed"
    exit 1
fi

echo ""
echo "Setup complete! You can now run:"
echo "  bash $SCRIPT_DIR/run_nodejs_benchmark.sh"
echo ""
