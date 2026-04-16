# main.py — Example NEAR script for near-python runtime
# Demonstrates near.view(), near.block_height(), and near.storage

# Query Burrow config
config = near.view("contract.main.burrow.near", "get_config", {})
print(f"Burrow config: {config}")

# Get current block height
block = near.block_height()
print(f"Current block: {block}")

# Query a specific contract
status = near.view("wrap.near", "get_version", {})
print(f"wrap.near version: {status}")

# Store the analysis result
near.storage.put("last_check", json.dumps({"block": block, "status": "ok"}))

# Retrieve it back
stored = near.storage.get("last_check")
print(f"Stored result: {stored}")
