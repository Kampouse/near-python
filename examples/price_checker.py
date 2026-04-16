# price_checker.py — Check NEAR price from on-chain DEX
# Uses Ref Finance DCL pool to get NEAR/USDT price

# Get the latest block
block = near.block_height()
print(f"Block: {block}")

# Query Ref Finance DCL pool for NEAR/USDT price
# Pool: usdt.tether-token.near|wrap.near|100 on dclv2.ref-labs.near
pool_data = near.view(
    "dclv2.ref-labs.near",
    "get_pool",
    json.dumps({"pool_id": "usdt.tether-token.near|wrap.near|100"})
)

if pool_data:
    info = json.loads(pool_data)
    print(f"Pool info: {info}")
else:
    # Fallback: check wrap.near for basic info
    account = near.view_account("wrap.near")
    print(f"wrap.near account: {account}")

# Check account balance
my_account = near.view_account("kampouse.near")
print(f"Account: {my_account}")
