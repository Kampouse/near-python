#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

WASM="target/wasm32-wasip2/release/near_python.wasm"
SCRIPT="${1:-src/main.py}"

if [ ! -f "$WASM" ]; then
    echo "WASM not found. Run scripts/build.sh first."
    exit 1
fi

if [ ! -f "$SCRIPT" ]; then
    echo "Script not found: $SCRIPT"
    exit 1
fi

echo "🚀 Running $SCRIPT via inlayer..."
cat "$SCRIPT" | ~/.local/bin/inlayer run "$WASM"
