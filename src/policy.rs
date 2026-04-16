//! Policy engine for 3-tier security model
//!
//! Tier 1: Read-only (view, block, gas_price, status, view_account, etc.)
//! Tier 2: Tier 1 + whitelisted contract calls (no transfers)
//! Tier 3: Tier 2 + transfers and raw tx sending

use std::collections::HashMap;

/// Runtime policy loaded from JSON
#[derive(Clone, Debug)]
pub struct Policy {
    pub tier: u8,
    /// contract_id → list of allowed method names (empty = all methods allowed for that contract)
    pub allowed_contracts: Option<HashMap<String, Vec<String>>>,
    /// Always-blocked method names regardless of contract
    pub blocked_methods: Vec<String>,
    /// Max gas per call (as u128 string in JSON)
    pub max_gas: Option<u128>,
    /// Max deposit per call (as u128 string in JSON)
    pub max_deposit: Option<u128>,
    /// Max number of call() invocations per script run
    pub max_calls_per_run: Option<u32>,
    /// Attested WASM hashes allowed (tier 3 only)
    pub attested_hashes: Option<Vec<String>>,
    /// Runtime counter
    pub call_count: u32,
}

impl Policy {
    /// Parse policy from a serde_json::Value (the "policy" field from stdin JSON)
    pub fn from_json(v: &serde_json::Value) -> Self {
        let tier = v.get("tier").and_then(|t| t.as_u64()).unwrap_or(1) as u8;

        let allowed_contracts = v.get("allowed_contracts").and_then(|ac| {
            let mut map = HashMap::new();
            if let Some(obj) = ac.as_object() {
                for (k, vals) in obj {
                    let methods: Vec<String> = vals.as_array()
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                        .unwrap_or_default();
                    map.insert(k.clone(), methods);
                }
            }
            if map.is_empty() { None } else { Some(map) }
        });

        let blocked_methods = v.get("blocked_methods")
            .and_then(|bm| bm.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let max_gas = v.get("max_gas")
            .and_then(|g| g.as_str())
            .and_then(|s| s.parse::<u128>().ok());

        let max_deposit = v.get("max_deposit")
            .and_then(|d| d.as_str())
            .and_then(|s| s.parse::<u128>().ok());

        let max_calls_per_run = v.get("max_calls_per_run")
            .and_then(|c| c.as_u64())
            .map(|c| c as u32);

        let attested_hashes = v.get("attested_hashes")
            .and_then(|ah| ah.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());

        Policy {
            tier,
            allowed_contracts,
            blocked_methods,
            max_gas,
            max_deposit,
            max_calls_per_run,
            attested_hashes,
            call_count: 0,
        }
    }

    /// Default policy for the given tier (no restrictions beyond tier level)
    pub fn default_for_tier(tier: u8) -> Self {
        Policy {
            tier,
            allowed_contracts: None,
            blocked_methods: vec![],
            max_gas: None,
            max_deposit: None,
            max_calls_per_run: None,
            attested_hashes: None,
            call_count: 0,
        }
    }

    /// Check if a `near.call()` is allowed. Returns Err(error_msg) if blocked.
    pub fn check_call(
        &mut self,
        receiver_id: &str,
        method_name: &str,
        gas: &str,
        deposit: &str,
    ) -> Result<(), String> {
        // Tier check
        if self.tier < 2 {
            return Err("Tier 1: write operations not allowed".to_string());
        }

        // Check blocked methods
        if self.blocked_methods.iter().any(|m| m == method_name) {
            return Err(format!("Policy: method '{}' is blocked", method_name));
        }

        // Check allowed contracts
        if let Some(ref contracts) = self.allowed_contracts {
            if let Some(allowed_methods) = contracts.get(receiver_id) {
                // Contract is whitelisted, check method
                if !allowed_methods.is_empty() && !allowed_methods.iter().any(|m| m == method_name) {
                    return Err(format!(
                        "Policy: method '{}' not allowed for contract '{}'",
                        method_name, receiver_id
                    ));
                }
            } else {
                return Err(format!(
                    "Policy: contract '{}' not in allowed list",
                    receiver_id
                ));
            }
        }

        // Check gas limit
        if let Some(max_gas) = self.max_gas {
            if let Ok(g) = gas.parse::<u128>() {
                if g > max_gas {
                    return Err(format!(
                        "Policy: gas {} exceeds max {}",
                        gas, max_gas
                    ));
                }
            }
        }

        // Check deposit limit
        if let Some(max_deposit) = self.max_deposit {
            if let Ok(d) = deposit.parse::<u128>() {
                if d > max_deposit {
                    return Err(format!(
                        "Policy: deposit {} exceeds max {}",
                        deposit, max_deposit
                    ));
                }
            }
        }

        // Rate limit
        self.call_count += 1;
        if let Some(max_calls) = self.max_calls_per_run {
            if self.call_count > max_calls {
                return Err(format!(
                    "Policy: exceeded max {} calls per run",
                    max_calls
                ));
            }
        }

        Ok(())
    }

    /// Check if a `near.transfer()` is allowed.
    pub fn check_transfer(&mut self, amount_yocto: &str) -> Result<(), String> {
        if self.tier < 3 {
            return Err(format!("Tier {}: transfers not allowed", self.tier));
        }

        // Check deposit/spending limit
        if let Some(max_deposit) = self.max_deposit {
            if let Ok(a) = amount_yocto.parse::<u128>() {
                if a > max_deposit {
                    return Err(format!(
                        "Policy: transfer amount {} exceeds max {}",
                        amount_yocto, max_deposit
                    ));
                }
            }
        }

        // Rate limit (transfers count toward call count)
        self.call_count += 1;
        if let Some(max_calls) = self.max_calls_per_run {
            if self.call_count > max_calls {
                return Err(format!(
                    "Policy: exceeded max {} calls per run",
                    max_calls
                ));
            }
        }

        Ok(())
    }

    /// Check if `near.send_tx()` is allowed.
    pub fn check_send_tx(&mut self) -> Result<(), String> {
        if self.tier < 3 {
            return Err(format!("Tier {}: raw transactions not allowed", self.tier));
        }

        self.call_count += 1;
        if let Some(max_calls) = self.max_calls_per_run {
            if self.call_count > max_calls {
                return Err(format!(
                    "Policy: exceeded max {} calls per run",
                    max_calls
                ));
            }
        }

        Ok(())
    }
}
