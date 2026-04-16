# monitor_burrow.py — Monitor Burrow margin positions
# Queries contract.main.burrow.near for account positions

account_id = "kampouse.near"

# Get margin account info
result = near.view(
    "contract.main.burrow.near",
    "get_margin_account",
    json.dumps({"account_id": account_id})
)

if result:
    account = json.loads(result)
    print(f"Margin account for {account_id}:")
    print(json.dumps(account))
    
    # Store last check in storage
    block = near.block_height()
    near.storage.put("last_burrow_check", json.dumps({"height": block, "account": account_id}))
    print(f"Stored check at block {block}")
else:
    print(f"No margin account found for {account_id}")

# Check Burrow config
config = near.view("contract.main.burrow.near", "get_config", {})
if config:
    print(f"Burrow config: {config}")
