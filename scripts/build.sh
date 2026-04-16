#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "🔧 Building near-python WASM component..."

# Check for wasm32-wasip2 target
if ! rustup target list | grep -q "wasm32-wasip2 (installed)"; then
    echo "📦 Installing wasm32-wasip2 target..."
    rustup target add wasm32-wasip2
fi

# Build
cargo build --target wasm32-wasip2 --release

WASM="target/wasm32-wasip2/release/near_python.wasm"

if [ -f "$WASM" ]; then
    SIZE=$(wc -c < "$WASM" | tr -d ' ')
    echo "✅ Built: $WASM ($SIZE bytes)"
    echo ""
    echo "Run via inlayer:"
    echo "  cat src/main.py | inlayer run $WASM"
else
    echo "❌ Build failed - WASM not found"
    exit 1
fi
