# test_view.py — Test near.view() calls
config = near.view("contract.main.burrow.near", "get_config", {})
print(f"Config type: {config}")

version = near.view("wrap.near", "get_version", {})
print(f"wrap.near version: {version}")
