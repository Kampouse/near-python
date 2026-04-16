#!/usr/bin/env python3
"""Test policy engine logic by validating the JSON structures and expected behaviors."""
import json
import sys

def test_policies():
    errors = []

    # Test readonly policy
    with open("policies/readonly.json") as f:
        p = json.load(f)
    if p["tier"] != 1:
        errors.append(f"readonly: tier should be 1, got {p['tier']}")

    # Test defi-agent policy
    with open("policies/defi-agent.json") as f:
        p = json.load(f)
    if p["tier"] != 2:
        errors.append(f"defi-agent: tier should be 2, got {p['tier']}")
    if "v2.ref-finance.near" not in p["allowed_contracts"]:
        errors.append("defi-agent: missing v2.ref-finance.near")
    if "transfer" not in p["blocked_methods"]:
        errors.append("defi-agent: transfer not in blocked_methods")
    if int(p["max_gas"]) != 200000000000000:
        errors.append(f"defi-agent: wrong max_gas")
    if p["max_deposit"] != "0":
        errors.append(f"defi-agent: wrong max_deposit")
    if p["max_calls_per_run"] != 10:
        errors.append(f"defi-agent: wrong max_calls_per_run")

    # Test full-access policy
    with open("policies/full-access.json") as f:
        p = json.load(f)
    if p["tier"] != 3:
        errors.append(f"full-access: tier should be 3, got {p['tier']}")
    if "attested_hashes" not in p:
        errors.append("full-access: missing attested_hashes")

    # Test WIT files exist and have correct functions
    for tier in [1, 2, 3]:
        with open(f"wit/tiers/world-tier{tier}.wit") as f:
            wit = f.read()
        # All tiers should have view
        if "view:" not in wit:
            errors.append(f"tier{tier}: missing view function")
        if "block:" not in wit:
            errors.append(f"tier{tier}: missing block function")
        # Tier 2+ should have call
        if tier >= 2 and "call:" not in wit:
            errors.append(f"tier{tier}: missing call function")
        if tier < 2 and "call:" in wit:
            errors.append(f"tier{tier}: should not have call function")
        # Tier 3 should have transfer and send-tx
        if tier >= 3:
            if "transfer:" not in wit:
                errors.append(f"tier{tier}: missing transfer function")
            if "send-tx:" not in wit:
                errors.append(f"tier{tier}: missing send-tx function")
        if tier < 3:
            if "transfer:" in wit:
                errors.append(f"tier{tier}: should not have transfer function")
            if "send-tx:" in wit:
                errors.append(f"tier{tier}: should not have send-tx function")

    # Test WASM binaries exist and are under 500KB
    import os
    for tier in [1, 2, 3]:
        path = f"dist/near-python-tier{tier}.wasm"
        if not os.path.exists(path):
            errors.append(f"tier{tier}: WASM binary not found at {path}")
        else:
            size = os.path.getsize(path)
            if size > 500_000:
                errors.append(f"tier{tier}: WASM too large ({size} bytes, max 500KB)")
            print(f"  tier{tier}: {size:,} bytes")

    if errors:
        for e in errors:
            print(f"  FAIL: {e}")
        sys.exit(1)
    else:
        print("✅ All policy tests passed")

if __name__ == "__main__":
    test_policies()
