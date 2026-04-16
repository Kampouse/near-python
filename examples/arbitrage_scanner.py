# arbitrage_scanner.py — Scan DEX prices for arbitrage opportunities
# Compares prices across Ref Finance pools

# Get NEAR/USDT price from DCL
pool_info = near.view(
    "dclv2.ref-labs.near",
    "get_pool",
    json.dumps({"pool_id": "usdt.tether-token.near|wrap.near|100"})
)

if pool_info:
    print(f"DCL NEAR/USDT: {pool_info}")

# Check multiple pools for arb opportunities
pools = [
    "usdt.tether-token.near|wrap.near|100",
    "usdt.tether-token.near|wrap.near|500",
    "usdt.tether-token.near|wrap.near|1000"
]

prices = {}
for pool_id in pools:
    data = near.view(
        "dclv2.ref-labs.near",
        "get_pool",
        json.dumps({"pool_id": pool_id})
    )
    if data:
        prices[pool_id] = data
        print(f"Pool {pool_id}: {data}")

# Store scan results
block = near.block_height()
near.storage.put("last_arb_scan", json.dumps({"block": block, "pools": len(pools)}))
print(f"Scanned {len(pools)} pools at block {block}")
