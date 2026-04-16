# dao_monitor.py — Watch DAO proposals and notify
# Monitors near-daos for new proposals

dao_account = "ref-dao.sputnik-dao.near"

# Get DAO config
config = near.view(dao_account, "get_config", {})
if config:
    print(f"DAO Config: {config}")

# Get last proposal ID
last_id_result = near.view(dao_account, "get_last_proposal_id", {})
if last_id_result:
    last_id = json.loads(last_id_result)
    print(f"Last proposal ID: {last_id}")
    
    # Get recent proposals (last 5)
    start = 0
    if last_id > 5:
        start = last_id - 5
    
    proposals = near.view(
        dao_account,
        "get_proposals",
        json.dumps({"from_index": start, "limit": 5})
    )
    
    if proposals:
        proposal_list = json.loads(proposals)
        print(f"Recent proposals:")
        for p in proposal_list:
            if isinstance(p, dict):
                pid = p["id"] if "id" in p else "?"
                status = p["status"] if "status" in p else "?"
                print(f"  #{pid}: {status}")
            else:
                print(f"  {p}")

# Store last check
block = near.block_height()
near.storage.put("last_dao_check", json.dumps({"height": block}))
print(f"DAO check complete at block {block}")
