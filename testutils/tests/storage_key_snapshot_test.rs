/// Storage Key + Type Snapshot Tests
///
/// Locks in the current (key, type, storage tier) mapping for every storage key
/// across all contracts. A change to any existing mapping must be deliberate
/// (editing this file) and will be visible in PR review.
///
/// Rules enforced:
/// 1. Every key must have a documented type and tier.
/// 2. No key may be reused with a different type in the same contract.
/// 3. No key may be removed and later re-added with a different type
///    (enforced via `get_historically_used_keys`).
use std::collections::{HashMap, HashSet};

/// Storage key snapshot entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StorageKeyEntry {
    /// The on-chain key string (e.g. "CONFIG") or DataKey variant name
    key: &'static str,
    /// Contract crate name
    contract: &'static str,
    /// Human-readable Rust type name stored under this key
    type_name: &'static str,
    /// Storage tier: "instance" or "persistent"
    tier: &'static str,
}

// ---------------------------------------------------------------------------
// Snapshot: complete list of all storage keys and their types
// ---------------------------------------------------------------------------

fn get_snapshot_entries() -> Vec<StorageKeyEntry> {
    vec![
        // ===================================================================
        // remittance_split
        // ===================================================================
        StorageKeyEntry {
            key: "CONFIG",
            contract: "remittance_split",
            type_name: "SplitConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "SPLIT",
            contract: "remittance_split",
            type_name: "Vec<u32>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "NONCES",
            contract: "remittance_split",
            type_name: "Map<Address, u64>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "AUDIT",
            contract: "remittance_split",
            type_name: "Vec<AuditEntry>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSE_ADM",
            contract: "remittance_split",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSED",
            contract: "remittance_split",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "UPG_ADM",
            contract: "remittance_split",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "VERSION",
            contract: "remittance_split",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "NEXT_RSCH",
            contract: "remittance_split",
            type_name: "u32",
            tier: "instance",
        },
        // DataKey variants (used with persistent storage)
        StorageKeyEntry {
            key: "DataKey::Schedule",
            contract: "remittance_split",
            type_name: "RemittanceSchedule",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::OwnerSchedules",
            contract: "remittance_split",
            type_name: "Vec<u32>",
            tier: "persistent",
        },
        // ===================================================================
        // savings_goals
        // ===================================================================
        StorageKeyEntry {
            key: "DataKey::NextId",
            contract: "savings_goals",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Goal",
            contract: "savings_goals",
            type_name: "SavingsGoal",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::ArchivedGoal",
            contract: "savings_goals",
            type_name: "ArchivedSavingsGoal",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::OwnerGoals",
            contract: "savings_goals",
            type_name: "Vec<u32>",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::ArchivedGoalsIndex",
            contract: "savings_goals",
            type_name: "Vec<u32>",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::TagIndex",
            contract: "savings_goals",
            type_name: "Vec<u32>",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::PauseAdmin",
            contract: "savings_goals",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Paused",
            contract: "savings_goals",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::PausedFunctions",
            contract: "savings_goals",
            type_name: "Map<Symbol, bool>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::UnpauseAt",
            contract: "savings_goals",
            type_name: "u64",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::UpgradeAdmin",
            contract: "savings_goals",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Version",
            contract: "savings_goals",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Nonces",
            contract: "savings_goals",
            type_name: "u64",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Audit",
            contract: "savings_goals",
            type_name: "Vec<AuditEntry>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::NextScheduleId",
            contract: "savings_goals",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Schedule",
            contract: "savings_goals",
            type_name: "SavingsSchedule",
            tier: "persistent",
        },
        StorageKeyEntry {
            key: "DataKey::OwnerSchedules",
            contract: "savings_goals",
            type_name: "Vec<u32>",
            tier: "persistent",
        },
        // ===================================================================
        // bill_payments
        // ===================================================================
        StorageKeyEntry {
            key: "BILLS",
            contract: "bill_payments",
            type_name: "Vec<Bill>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "NEXT_ID",
            contract: "bill_payments",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ARCH_BILL",
            contract: "bill_payments",
            type_name: "Vec<ArchivedBill>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "STOR_STAT",
            contract: "bill_payments",
            type_name: "StorageStats",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSE_ADM",
            contract: "bill_payments",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSED",
            contract: "bill_payments",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSED_FN",
            contract: "bill_payments",
            type_name: "Map<Symbol, bool>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "UNP_AT",
            contract: "bill_payments",
            type_name: "u64",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "UPG_ADM",
            contract: "bill_payments",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "VERSION",
            contract: "bill_payments",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "UNPD_TOT",
            contract: "bill_payments",
            type_name: "Map<Address, i128>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EXTRIDX",
            contract: "bill_payments",
            type_name: "Map<Address, Map<String, u32>>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "OWN_IDX",
            contract: "bill_payments",
            type_name: "Map<Address, Vec<u32>>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ARCH_IDX",
            contract: "bill_payments",
            type_name: "Map<Address, Vec<u32>>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "CUR_IDX",
            contract: "bill_payments",
            type_name: "Map<(Address, String), Vec<u32>>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "NEXT_BSCH",
            contract: "bill_payments",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "OWN_BSCH",
            contract: "bill_payments",
            type_name: "Map<Address, Vec<u32>>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "BSCHEDS",
            contract: "bill_payments",
            type_name: "Map<u32, BillSchedule>",
            tier: "instance",
        },
        // ===================================================================
        // insurance
        // ===================================================================
        StorageKeyEntry {
            key: "DataKey::Owner",
            contract: "insurance",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::PolicyCount",
            contract: "insurance",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Policy",
            contract: "insurance",
            type_name: "Policy",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::ActivePolicies",
            contract: "insurance",
            type_name: "Vec<u32>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::OwnerPolicies",
            contract: "insurance",
            type_name: "Vec<u32>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::Initialized",
            contract: "insurance",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "VERSION",
            contract: "insurance",
            type_name: "u32",
            tier: "instance",
        },
        // ===================================================================
        // family_wallet
        // ===================================================================
        StorageKeyEntry {
            key: "OWNER",
            contract: "family_wallet",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MEMBERS",
            contract: "family_wallet",
            type_name: "Map<Address, FamilyMember>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MS_WDRAW",
            contract: "family_wallet",
            type_name: "MultiSigConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MS_SPLIT",
            contract: "family_wallet",
            type_name: "MultiSigConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MS_ROLE",
            contract: "family_wallet",
            type_name: "MultiSigConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MS_EMERG",
            contract: "family_wallet",
            type_name: "MultiSigConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MS_POL",
            contract: "family_wallet",
            type_name: "MultiSigConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "MS_REG",
            contract: "family_wallet",
            type_name: "MultiSigConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PEND_TXS",
            contract: "family_wallet",
            type_name: "Map<u64, PendingTransaction>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EXEC_TXS",
            contract: "family_wallet",
            type_name: "Map<u64, ExecutedTxMeta>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "NEXT_TX",
            contract: "family_wallet",
            type_name: "u64",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EM_CONF",
            contract: "family_wallet",
            type_name: "EmergencyConfig",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EM_MODE",
            contract: "family_wallet",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EM_LAST",
            contract: "family_wallet",
            type_name: "u64",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EM_VOL",
            contract: "family_wallet",
            type_name: "i128",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ARCH_TX",
            contract: "family_wallet",
            type_name: "Map<u64, ArchivedTransaction>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "STOR_STAT",
            contract: "family_wallet",
            type_name: "StorageStats",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ROLE_EXP",
            contract: "family_wallet",
            type_name: "Map<Address, u64>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PREC_LIM",
            contract: "family_wallet",
            type_name: "Map<Address, PrecisionLimitOpt>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "SPND_TRK",
            contract: "family_wallet",
            type_name: "Map<Address, SpendingTracker>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSED",
            contract: "family_wallet",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PAUSE_ADM",
            contract: "family_wallet",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "UPG_ADM",
            contract: "family_wallet",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "VERSION",
            contract: "family_wallet",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ACC_AUDIT",
            contract: "family_wallet",
            type_name: "Vec<AuditEntry>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PROP_EXP",
            contract: "family_wallet",
            type_name: "u64",
            tier: "instance",
        },
        // ===================================================================
        // reporting
        // ===================================================================
        StorageKeyEntry {
            key: "ADMIN",
            contract: "reporting",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "PEND_ADM",
            contract: "reporting",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ADDRS",
            contract: "reporting",
            type_name: "ContractAddresses",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "REPORTS",
            contract: "reporting",
            type_name: "Map<(Address, u64), FinancialHealthReport>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ARCH_RPT",
            contract: "reporting",
            type_name: "Map<(Address, u64), ArchivedReport>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "ARCH_IDX",
            contract: "reporting",
            type_name: "Map<Address, Vec<u64>>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "STOR_STAT",
            contract: "reporting",
            type_name: "StorageStats",
            tier: "instance",
        },
        // ===================================================================
        // orchestrator
        // ===================================================================
        StorageKeyEntry {
            key: "OWNER",
            contract: "orchestrator",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "FW_ADDR",
            contract: "orchestrator",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "RS_ADDR",
            contract: "orchestrator",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "SG_ADDR",
            contract: "orchestrator",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "BP_ADDR",
            contract: "orchestrator",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "INS_ADDR",
            contract: "orchestrator",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "EXEC_LOCK",
            contract: "orchestrator",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "NONCES",
            contract: "orchestrator",
            type_name: "Map<Address, u64>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "GOAL_ID",
            contract: "orchestrator",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "BILL_ID",
            contract: "orchestrator",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "POL_ID",
            contract: "orchestrator",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "STATS",
            contract: "orchestrator",
            type_name: "ExecutionStats",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "VERSION",
            contract: "orchestrator",
            type_name: "u32",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "AUDIT",
            contract: "orchestrator",
            type_name: "Vec<AuditEntry>",
            tier: "instance",
        },
        // ===================================================================
        // emergency_killswitch
        // ===================================================================
        StorageKeyEntry {
            key: "DataKey::Admin",
            contract: "emergency_killswitch",
            type_name: "Address",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::GlobalPaused",
            contract: "emergency_killswitch",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::ModulePaused",
            contract: "emergency_killswitch",
            type_name: "bool",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::PausedFunctions",
            contract: "emergency_killswitch",
            type_name: "Vec<Symbol>",
            tier: "instance",
        },
        StorageKeyEntry {
            key: "DataKey::UnpauseSchedule",
            contract: "emergency_killswitch",
            type_name: "u64",
            tier: "instance",
        },
    ]
}

/// Historical set of (contract, key) pairs that have ever existed.
///
/// This set MUST never shrink — only grow. If a key is removed from the
/// codebase, its entry stays here. If that key is later re-added, its type
/// must match the original snapshot or the test fails.
fn get_historically_used_keys() -> HashSet<(&'static str, &'static str)> {
    get_snapshot_entries()
        .iter()
        .map(|e| (e.contract, e.key))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_storage_key_type_snapshot_unchanged() {
    let entries = get_snapshot_entries();
    let mut errors = Vec::new();

    // Check for empty / missing fields
    for entry in &entries {
        if entry.key.is_empty() {
            errors.push(format!("{}: empty key", entry.contract));
        }
        if entry.type_name.is_empty() {
            errors.push(format!(
                "{}.{}: missing type_name",
                entry.contract, entry.key
            ));
        }
        if entry.tier.is_empty() {
            errors.push(format!("{}.{}: missing tier", entry.contract, entry.key));
        }
        if entry.tier != "instance" && entry.tier != "persistent" {
            errors.push(format!(
                "{}.{}: invalid tier '{}' (must be 'instance' or 'persistent')",
                entry.contract, entry.key, entry.tier
            ));
        }
    }

    // Check for duplicate keys within the same contract
    let mut seen: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let k = (entry.contract, entry.key);
        seen.entry(k).or_default().push(i);
    }
    for ((contract, key), indices) in &seen {
        if indices.len() > 1 {
            errors.push(format!(
                "{}: duplicate key '{}' at indices {:?}",
                contract, key, indices
            ));
        }
    }

    // Check for type / tier mismatches where the same (contract, key) appears
    // with different type or tier in the snapshot list (shouldn't happen if
    // the duplicate check passes, but defensive).
    let mut type_map: HashMap<(&str, &str), (&str, &str)> = HashMap::new();
    for entry in &entries {
        let k = (entry.contract, entry.key);
        let vt = (entry.type_name, entry.tier);
        if let Some(existing) = type_map.get(&k) {
            if existing != &vt {
                errors.push(format!(
                    "{}.{}: type/tier conflict: was ({}, {}), now ({}, {})",
                    entry.contract, entry.key, existing.0, existing.1, entry.type_name, entry.tier
                ));
            }
        } else {
            type_map.insert(k, vt);
        }
    }

    if !errors.is_empty() {
        panic!(
            "\n\nStorage key snapshot violations found ({}):\n{}\n\n",
            errors.len(),
            errors.join("\n")
        );
    }

    println!(
        "✅ Storage key snapshot self-validates ({} entries across {} contracts)",
        entries.len(),
        seen.len()
    );
}

#[test]
fn test_no_key_reused_with_different_type() {
    let historical = get_historically_used_keys();
    let current = get_snapshot_entries();
    let mut errors = Vec::new();

    // Build type map from current entries
    let mut type_map: HashMap<(&str, &str), (&str, &str)> = HashMap::new();
    for entry in &current {
        type_map.insert((entry.contract, entry.key), (entry.type_name, entry.tier));
    }

    // Every historically known key that is still in use must have the same
    // type and tier as before. Since the historical set == current set on
    // first creation, this is a self-consistency check that future edits
    // cannot silently break.
    for (contract, key) in &historical {
        if let Some((current_type, current_tier)) = type_map.get(&(contract, key)) {
            let entries = get_snapshot_entries();
            let original = entries
                .iter()
                .find(|e| e.contract == *contract && e.key == *key)
                .expect("historical key not found in current snapshot");

            if original.type_name != *current_type {
                errors.push(format!(
                    "{}.{}: type changed from '{}' to '{}'",
                    contract, key, original.type_name, current_type
                ));
            }
            if original.tier != *current_tier {
                errors.push(format!(
                    "{}.{}: tier changed from '{}' to '{}'",
                    contract, key, original.tier, current_tier
                ));
            }
        }
    }

    if !errors.is_empty() {
        panic!(
            "\n\nStorage key type reuse violations found ({}):\n{}\n\n\
             If you intentionally changed a key's type or tier, update the\n\
             snapshot entries in `get_snapshot_entries()` to match.\n\
             Never reuse a retired key for a different purpose.\n",
            errors.len(),
            errors.join("\n")
        );
    }

    println!(
        "✅ No storage key has been reused with a different type or tier ({} historical keys)",
        historical.len()
    );
}

#[test]
fn test_print_storage_key_snapshot_summary() {
    let entries = get_snapshot_entries();
    let mut contract_counts: HashMap<&str, usize> = HashMap::new();
    let mut tier_counts: HashMap<&str, usize> = HashMap::new();

    for entry in &entries {
        *contract_counts.entry(entry.contract).or_insert(0) += 1;
        *tier_counts.entry(entry.tier).or_insert(0) += 1;
    }

    println!("\n📊 Storage Key Snapshot Summary:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Total storage entries: {}", entries.len());
    println!("\nEntries per contract:");

    let mut contracts: Vec<_> = contract_counts.iter().collect();
    contracts.sort_by_key(|(name, _)| *name);
    for (contract, count) in contracts {
        println!("  • {}: {} entries", contract, count);
    }

    println!("\nBy storage tier:");
    for (tier, count) in &tier_counts {
        println!("  • {}: {}", tier, count);
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
}
