# liquidation_bot.py — Find and report liquidatable positions
# Scans Burrow for positions that could be liquidated

# Get all assets in Burrow
assets = near.view(
    "contract.main.burrow.near",
    "get_assets_paged",
    json.dumps({"from_index": 0, "limit": 100})
)

if assets:
    asset_data = json.loads(assets)
    print(f"Found {len(asset_data)} assets")
    
    # Check each asset for health
    for asset in asset_data:
        token_id = asset["token_id"]
        print(f"Token: {token_id}")
else:
    print("Could not fetch assets")

# Get specific margin account
account_id = "kampouse.near"
margin = near.view(
    "contract.main.burrow.near",
    "get_margin_account",
    json.dumps({"account_id": account_id})
)

if margin:
    print(f"Margin account: {margin}")
    
# Store liquidation scan results
block = near.block_height()
near.storage.put("last_liquidation_scan", json.dumps({"block": block}))
print(f"Scan complete at block {block}")
