# test_storage.py — Test near.storage operations (in-memory)

# Put a value
near.storage.put("test_key", json.dumps({"hello": "world", "count": 42}))
print("Stored test_key")

# Get it back
value = near.storage.get("test_key")
print(f"Got: {value}")

# Store a block height
block = near.block_height()
near.storage.put("last_block", json.dumps({"height": block}))
print(f"Stored block {block}")

# Verify retrieval
stored = near.storage.get("last_block")
print(f"Retrieved: {stored}")
