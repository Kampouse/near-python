# test_view.py — Test near.view() calls
config = near.view("contract.main.burrow.near", "get_config", {})
print(f"Config type: {type(config)}")

# Test account view
account = near.view_account("wrap.near")
print(f"wrap.near account: {type(account)}")

# Test block height
block = near.block_height()
print(f"Block height: {block}")

# Test block query
block_data = near.block("final")
print(f"Block data type: {type(block_data)}")
