# Makefile for near-python — Python runtime for NEAR OutLayer

.PHONY: build run test clean

WASM := target/wasm32-wasip2/release/near_python.wasm

build: $(WASM)

$(WASM): src/lib.rs Cargo.toml wit/world.wit wit/deps/storage.wit
	rustup target add wasm32-wasip2 2>/dev/null || true
	cargo build --target wasm32-wasip2 --release
	@echo "✅ Built $(WASM) ($(shell wc -c < $(WASM)) bytes)"

run: build
	@echo "🚀 Running main.py via inlayer..."
	@cat src/main.py | ~/.local/bin/inlayer run $(WASM)

test-view: build
	@cat tests/test_view.py | ~/.local/bin/inlayer run $(WASM)

test-storage: build
	@cat tests/test_storage.py | ~/.local/bin/inlayer run $(WASM)

test-call: build
	@cat tests/test_call.py | ~/.local/bin/inlayer run $(WASM)

test: test-view test-storage
	@echo "✅ All tests passed"

clean:
	cargo clean
