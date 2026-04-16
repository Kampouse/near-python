# test_call.py — Test near.call() with signing
# Note: requires SIGNER_ID and SIGNER_KEY env vars

signer_id = "kampouse.near"
signer_key = "ed25519:YOUR_PRIVATE_KEY"
result = near.call(signer_id, signer_key, "wrap.near", "storage_deposit", "{}", "1000000000000000000000000", "30000000000000")
print(f"Call result: {result}")
