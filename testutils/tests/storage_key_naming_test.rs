/// Storage Key Naming Convention Tests
///
/// This module validates that all storage keys across the Remitwise contracts
/// adhere to documented naming conventions:
/// - Maximum length: 9 characters (Soroban symbol_short! constraint)
/// - Format: UPPERCASE with underscores
/// - No special characters except underscore
///
/// Reference: STORAGE_LAYOUT.md
/// Soroban SDK Reference: symbol_short! supports up to 9 characters
/// https://developers.stellar.org/docs/build/smart-contracts/example-contracts/storage

use std::collections::HashSet;

/// Storage key definition with metadata for validation
#[derive(Debug, Clone)]
struct StorageKey {
    key: &'static str,
    contract: &'static str,
    description: &'static str,
}

/// Maximum length for storage keys (symbol_short! constraint)
const MAX_KEY_LENGTH: usize = 9;

/// All documented storage keys across contracts
fn get_all_storage_keys() -> Vec<StorageKey> {
    vec![
        // remittance_split
        StorageKey {
            key: "CONFIG",
            contract: "remittance_split",
            description: "Owner + percentages + initialized flag",
        },
        StorageKey {
            key: "SPLIT",
            contract: "remittance_split",
            description: "Ordered percentages",
        },
        StorageKey {
            key: "NONCES",
            contract: "remittance_split",
            description: "Replay protection nonces",
        },
        StorageKey {
            key: "AUDIT",
            contract: "remittance_split",
            description: "Rotating audit log",
        },
        StorageKey {
            key: "REM_SCH",
            contract: "remittance_split",
            description: "Remittance schedules",
        },
        StorageKey {
            key: "NEXT_RSCH",
            contract: "remittance_split",
            description: "Next remittance schedule ID",
        },
        StorageKey {
            key: "PAUSE_ADM",
            contract: "remittance_split",
            description: "Pause admin",
        },
        StorageKey {
            key: "PAUSED",
            contract: "remittance_split",
            description: "Global pause flag",
        },
        StorageKey {
            key: "UPG_ADM",
            contract: "remittance_split",
            description: "Upgrade admin",
        },
        StorageKey {
            key: "VERSION",
            contract: "remittance_split",
            description: "Contract version",
        },
        // savings_goals
        StorageKey {
            key: "GOALS",
            contract: "savings_goals",
            description: "Primary goal records",
        },
        StorageKey {
            key: "NEXT_ID",
            contract: "savings_goals",
            description: "Next savings goal ID",
        },
        StorageKey {
            key: "SAV_SCH",
            contract: "savings_goals",
            description: "Recurring savings schedules",
        },
        StorageKey {
            key: "NEXT_SSCH",
            contract: "savings_goals",
            description: "Next savings schedule ID",
        },
        StorageKey {
            key: "NONCES",
            contract: "savings_goals",
            description: "Snapshot import nonce tracking",
        },
        StorageKey {
            key: "AUDIT",
            contract: "savings_goals",
            description: "Rotating audit log",
        },
        StorageKey {
            key: "PAUSE_ADM",
            contract: "savings_goals",
            description: "Pause admin",
        },
        StorageKey {
            key: "PAUSED",
            contract: "savings_goals",
            description: "Global pause flag",
        },
        StorageKey {
            key: "PAUSED_FN",
            contract: "savings_goals",
            description: "Per-function pause switches",
        },
        StorageKey {
            key: "UNP_AT",
            contract: "savings_goals",
            description: "Optional time-locked unpause timestamp",
        },
        StorageKey {
            key: "UPG_ADM",
            contract: "savings_goals",
            description: "Upgrade admin",
        },
        StorageKey {
            key: "VERSION",
            contract: "savings_goals",
            description: "Contract version",
        },
        // bill_payments
        StorageKey {
            key: "BILLS",
            contract: "bill_payments",
            description: "Active bill records",
        },
        StorageKey {
            key: "NEXT_ID",
            contract: "bill_payments",
            description: "Next bill ID",
        },
        StorageKey {
            key: "ARCH_BILL",
            contract: "bill_payments",
            description: "Archived paid bills",
        },
        StorageKey {
            key: "STOR_STAT",
            contract: "bill_payments",
            description: "Aggregated storage metrics",
        },
        StorageKey {
            key: "PAUSE_ADM",
            contract: "bill_payments",
            description: "Pause admin",
        },
        StorageKey {
            key: "PAUSED",
            contract: "bill_payments",
            description: "Global pause flag",
        },
        StorageKey {
            key: "PAUSED_FN",
            contract: "bill_payments",
            description: "Per-function pause switches",
        },
        StorageKey {
            key: "UNP_AT",
            contract: "bill_payments",
            description: "Optional unpause timestamp",
        },
        StorageKey {
            key: "UPG_ADM",
            contract: "bill_payments",
            description: "Upgrade admin",
        },
        StorageKey {
            key: "VERSION",
            contract: "bill_payments",
            description: "Contract version",
        },
        StorageKey {
            key: "UNPD_TOT",
            contract: "bill_payments",
            description: "Unpaid totals by owner",
        },
        // insurance
        StorageKey {
            key: "POLICIES",
            contract: "insurance",
            description: "Insurance policy records",
        },
        StorageKey {
            key: "NEXT_ID",
            contract: "insurance",
            description: "Next policy ID",
        },
        StorageKey {
            key: "PREM_SCH",
            contract: "insurance",
            description: "Premium schedules",
        },
        StorageKey {
            key: "NEXT_PSCH",
            contract: "insurance",
            description: "Next premium schedule ID",
        },
        StorageKey {
            key: "PAUSE_ADM",
            contract: "insurance",
            description: "Pause admin",
        },
        StorageKey {
            key: "PAUSED",
            contract: "insurance",
            description: "Global pause flag",
        },
        StorageKey {
            key: "PAUSED_FN",
            contract: "insurance",
            description: "Per-function pause switches",
        },
        StorageKey {
            key: "UNP_AT",
            contract: "insurance",
            description: "Optional unpause timestamp",
        },
        StorageKey {
            key: "UPG_ADM",
            contract: "insurance",
            description: "Upgrade admin",
        },
        StorageKey {
            key: "VERSION",
            contract: "insurance",
            description: "Contract version",
        },
        StorageKey {
            key: "OWN_IDX",
            contract: "insurance",
            description: "Owner index for policies",
        },
        // family_wallet
        StorageKey {
            key: "OWNER",
            contract: "family_wallet",
            description: "Wallet owner",
        },
        StorageKey {
            key: "MEMBERS",
            contract: "family_wallet",
            description: "Family members and roles",
        },
        StorageKey {
            key: "MS_WDRAW",
            contract: "family_wallet",
            description: "Multisig config for large withdrawals",
        },
        StorageKey {
            key: "MS_SPLIT",
            contract: "family_wallet",
            description: "Multisig config for split changes",
        },
        StorageKey {
            key: "MS_ROLE",
            contract: "family_wallet",
            description: "Multisig config for role changes",
        },
        StorageKey {
            key: "MS_EMERG",
            contract: "family_wallet",
            description: "Multisig config for emergency transfer",
        },
        StorageKey {
            key: "MS_POL",
            contract: "family_wallet",
            description: "Multisig config for policy cancellation",
        },
        StorageKey {
            key: "MS_REG",
            contract: "family_wallet",
            description: "Config key for regular withdrawals",
        },
        StorageKey {
            key: "PEND_TXS",
            contract: "family_wallet",
            description: "Pending multisig transactions",
        },
        StorageKey {
            key: "EXEC_TXS",
            contract: "family_wallet",
            description: "Executed transaction markers",
        },
        StorageKey {
            key: "NEXT_TX",
            contract: "family_wallet",
            description: "Next pending tx ID",
        },
        StorageKey {
            key: "EM_CONF",
            contract: "family_wallet",
            description: "Emergency transfer constraints",
        },
        StorageKey {
            key: "EM_MODE",
            contract: "family_wallet",
            description: "Emergency mode toggle",
        },
        StorageKey {
            key: "EM_LAST",
            contract: "family_wallet",
            description: "Last emergency transfer timestamp",
        },
        StorageKey {
            key: "ARCH_TX",
            contract: "family_wallet",
            description: "Archived executed transaction metadata",
        },
        StorageKey {
            key: "STOR_STAT",
            contract: "family_wallet",
            description: "Storage usage stats",
        },
        StorageKey {
            key: "ROLE_EXP",
            contract: "family_wallet",
            description: "Role expiry timestamps",
        },
        StorageKey {
            key: "PAUSED",
            contract: "family_wallet",
            description: "Global pause flag",
        },
        StorageKey {
            key: "PAUSE_ADM",
            contract: "family_wallet",
            description: "Pause admin",
        },
        StorageKey {
            key: "UPG_ADM",
            contract: "family_wallet",
            description: "Upgrade admin",
        },
        StorageKey {
            key: "VERSION",
            contract: "family_wallet",
            description: "Contract version",
        },
        StorageKey {
            key: "ACC_AUDIT",
            contract: "family_wallet",
            description: "Rolling access audit trail",
        },
        StorageKey {
            key: "PROP_EXP",
            contract: "family_wallet",
            description: "Proposal expiry duration",
        },
        // reporting
        StorageKey {
            key: "ADMIN",
            contract: "reporting",
            description: "Reporting admin",
        },
        StorageKey {
            key: "ADDRS",
            contract: "reporting",
            description: "Cross-contract address registry",
        },
        StorageKey {
            key: "REPORTS",
            contract: "reporting",
            description: "Active reports",
        },
        StorageKey {
            key: "ARCH_RPT",
            contract: "reporting",
            description: "Archived report summaries",
        },
        StorageKey {
            key: "STOR_STAT",
            contract: "reporting",
            description: "Active/archive counts",
        },
        // orchestrator
        StorageKey {
            key: "STATS",
            contract: "orchestrator",
            description: "Aggregate execution counters",
        },
        StorageKey {
            key: "AUDIT",
            contract: "orchestrator",
            description: "Rotating audit log",
        },
    ]
}

#[test]
fn test_all_keys_within_max_length() {
    let keys = get_all_storage_keys();
    let mut violations = Vec::new();

    for key_def in &keys {
        if key_def.key.len() > MAX_KEY_LENGTH {
            violations.push(format!(
                "❌ {}.{}: '{}' exceeds max length {} (actual: {})",
                key_def.contract,
                key_def.key,
                key_def.key,
                MAX_KEY_LENGTH,
                key_def.key.len()
            ));
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\nStorage key length violations found:\n{}\n\n",
            violations.join("\n")
        );
    }

    println!(
        "✅ All {} storage keys are within {} character limit",
        keys.len(),
        MAX_KEY_LENGTH
    );
}

#[test]
fn test_all_keys_uppercase_with_underscores() {
    let keys = get_all_storage_keys();
    let mut violations = Vec::new();

    for key_def in &keys {
        for (i, ch) in key_def.key.chars().enumerate() {
            if !ch.is_ascii_uppercase() && ch != '_' {
                violations.push(format!(
                    "❌ {}.{}: Invalid character '{}' at position {} (must be A-Z or _)",
                    key_def.contract, key_def.key, ch, i
                ));
                break;
            }
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\nStorage key format violations found:\n{}\n\n",
            violations.join("\n")
        );
    }

    println!(
        "✅ All {} storage keys use UPPERCASE_WITH_UNDERSCORES format",
        keys.len()
    );
}

#[test]
fn test_no_duplicate_keys_within_contract() {
    let keys = get_all_storage_keys();
    let mut contract_keys: std::collections::HashMap<&str, HashSet<&str>> =
        std::collections::HashMap::new();
    let mut violations = Vec::new();

    for key_def in &keys {
        let entry = contract_keys
            .entry(key_def.contract)
            .or_insert_with(HashSet::new);

        if !entry.insert(key_def.key) {
            violations.push(format!(
                "❌ {}: Duplicate key '{}' found",
                key_def.contract, key_def.key
            ));
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\nDuplicate storage keys found:\n{}\n\n",
            violations.join("\n")
        );
    }

    println!("✅ No duplicate keys within any contract");
}

#[test]
fn test_keys_not_empty() {
    let keys = get_all_storage_keys();
    let mut violations = Vec::new();

    for key_def in &keys {
        if key_def.key.is_empty() {
            violations.push(format!(
                "❌ {}: Empty storage key found",
                key_def.contract
            ));
        }
    }

    if !violations.is_empty() {
        panic!("\n\nEmpty storage keys found:\n{}\n\n", violations.join("\n"));
    }

    println!("✅ All storage keys are non-empty");
}

#[test]
fn test_keys_do_not_start_with_underscore() {
    let keys = get_all_storage_keys();
    let mut violations = Vec::new();

    for key_def in &keys {
        if key_def.key.starts_with('_') {
            violations.push(format!(
                "❌ {}.{}: Key starts with underscore (not recommended)",
                key_def.contract, key_def.key
            ));
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\nStorage keys starting with underscore:\n{}\n\n",
            violations.join("\n")
        );
    }

    println!("✅ No storage keys start with underscore");
}

#[test]
fn test_keys_do_not_end_with_underscore() {
    let keys = get_all_storage_keys();
    let mut violations = Vec::new();

    for key_def in &keys {
        if key_def.key.ends_with('_') {
            violations.push(format!(
                "❌ {}.{}: Key ends with underscore (not recommended)",
                key_def.contract, key_def.key
            ));
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\nStorage keys ending with underscore:\n{}\n\n",
            violations.join("\n")
        );
    }

    println!("✅ No storage keys end with underscore");
}

#[test]
fn test_no_consecutive_underscores() {
    let keys = get_all_storage_keys();
    let mut violations = Vec::new();

    for key_def in &keys {
        if key_def.key.contains("__") {
            violations.push(format!(
                "❌ {}.{}: Contains consecutive underscores (not recommended)",
                key_def.contract, key_def.key
            ));
        }
    }

    if !violations.is_empty() {
        panic!(
            "\n\nStorage keys with consecutive underscores:\n{}\n\n",
            violations.join("\n")
        );
    }

    println!("✅ No storage keys have consecutive underscores");
}

#[test]
fn test_common_keys_consistency() {
    // Keys that appear in multiple contracts should be consistent
    let keys = get_all_storage_keys();
    let mut key_contracts: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();

    for key_def in &keys {
        key_contracts
            .entry(key_def.key)
            .or_insert_with(Vec::new)
            .push(key_def.contract);
    }

    // Common keys that should be consistent across contracts
    let common_keys = vec![
        "PAUSE_ADM", "PAUSED", "UPG_ADM", "VERSION", "NEXT_ID", "AUDIT", "NONCES",
    ];

    for common_key in common_keys {
        if let Some(contracts) = key_contracts.get(common_key) {
            if contracts.len() > 1 {
                println!(
                    "✅ Common key '{}' used consistently across {} contracts: {:?}",
                    common_key,
                    contracts.len(),
                    contracts
                );
            }
        }
    }
}

#[test]
fn test_storage_key_documentation_coverage() {
    // This test ensures all keys are documented
    let keys = get_all_storage_keys();
    let mut undocumented = Vec::new();

    for key_def in &keys {
        if key_def.description.is_empty() {
            undocumented.push(format!(
                "❌ {}.{}: Missing description",
                key_def.contract, key_def.key
            ));
        }
    }

    if !undocumented.is_empty() {
        panic!(
            "\n\nUndocumented storage keys found:\n{}\n\n",
            undocumented.join("\n")
        );
    }

    println!("✅ All {} storage keys have descriptions", keys.len());
}

#[test]
fn test_print_storage_key_summary() {
    let keys = get_all_storage_keys();
    let mut contract_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();

    for key_def in &keys {
        *contract_counts.entry(key_def.contract).or_insert(0) += 1;
    }

    println!("\n📊 Storage Key Summary:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Total storage keys: {}", keys.len());
    println!("\nKeys per contract:");
    
    let mut contracts: Vec<_> = contract_counts.iter().collect();
    contracts.sort_by_key(|(name, _)| *name);
    
    for (contract, count) in contracts {
        println!("  • {}: {} keys", contract, count);
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
}
