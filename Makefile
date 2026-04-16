# Makefile for near-python — Python runtime for NEAR OutLayer
#
# 3-tier security model:
#   make tier1  — Read-only (view, block, gas_price, status)
#   make tier2  — Tier 1 + whitelisted contract calls
#   make tier3  — Tier 2 + transfers and raw transactions

.PHONY: build run test clean tier1 tier2 tier3 all-tiers dist

WASM := target/wasm32-wasip2/release/near_python.wasm
DIST  := dist

build: tier1

# Tier builds — copy the right WIT + set cargo feature
tier1:
	@mkdir -p $(DIST)
	cp wit/tiers/world-tier1.wit wit/world.wit
	cargo build --target wasm32-wasip2 --release --no-default-features --features tier1
	cp target/wasm32-wasip2/release/near_python.wasm $(DIST)/near-python-tier1.wasm
	@echo "✅ Tier 1 (read-only): $(DIST)/near-python-tier1.wasm ($$(wc -c < $(DIST)/near-python-tier1.wasm) bytes)"

tier2:
	@mkdir -p $(DIST)
	cp wit/tiers/world-tier2.wit wit/world.wit
	cargo build --target wasm32-wasip2 --release --no-default-features --features tier2
	cp target/wasm32-wasip2/release/near_python.wasm $(DIST)/near-python-tier2.wasm
	@echo "✅ Tier 2 (whitelisted calls): $(DIST)/near-python-tier2.wasm ($$(wc -c < $(DIST)/near-python-tier2.wasm) bytes)"

tier3:
	@mkdir -p $(DIST)
	cp wit/tiers/world-tier3.wit wit/world.wit
	cargo build --target wasm32-wasip2 --release --no-default-features --features tier3
	cp target/wasm32-wasip2/release/near_python.wasm $(DIST)/near-python-tier3.wasm
	@echo "✅ Tier 3 (full access): $(DIST)/near-python-tier3.wasm ($$(wc -c < $(DIST)/near-python-tier3.wasm) bytes)"

all-tiers: tier1 tier2 tier3
	@echo "✅ All tiers built in $(DIST)/"

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
	rm -rf $(DIST)
