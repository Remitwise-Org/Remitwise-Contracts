#![no_std]
use soroban_sdk::{
    testutils::{Address as AddressTrait, Ledger, LedgerInfo},
    Address, Env,
};

pub fn set_ledger_time(env: &Env, sequence_number: u32, timestamp: u64) {
    let proto = env.ledger().protocol_version();

    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        // Must exceed any contract bump TTL used in tests (e.g. 518,400).
        max_entry_ttl: 3_000_000,
    });
}

pub fn generate_test_address(env: &Env) -> Address {
    Address::generate(env)
}

/// Returns `true` when `a` and `b` represent the same on-ledger address.
///
/// Prefer this helper over a naked `a == b` comparison in tests so that
/// address-equality checks are grep-able and self-documenting.  Any future
/// change to how addresses are compared (e.g. normalisation) can be
/// implemented in one place.
///
/// # Examples
/// ```rust,ignore
/// assert!(same_address(&alice, &alice));
/// assert!(!same_address(&alice, &bob));
/// ```
pub fn same_address(a: &Address, b: &Address) -> bool {
    a == b
}

#[macro_export]
macro_rules! setup_test_env {
    ($env:ident, $contract:ident, $client_struct:ident, $client:ident, $owner:ident) => {
        let $env = Env::default();
        $env.mock_all_auths();
        let contract_id = $env.register_contract(None, $contract);
        let $client = $client_struct::new(&$env, &contract_id);
        let $owner = $crate::generate_test_address(&$env);
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests for same_address
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    /// same_address returns true when both arguments point to the identical address.
    #[test]
    fn same_address_returns_true_for_identical_address() {
        let env = Env::default();
        let alice = Address::generate(&env);
        assert!(same_address(&alice, &alice));
    }

    /// same_address returns true when the value is cloned (same bytes, different Rust binding).
    #[test]
    fn same_address_returns_true_for_clone_of_same_address() {
        let env = Env::default();
        let alice = Address::generate(&env);
        let alice_clone = alice.clone();
        assert!(same_address(&alice, &alice_clone));
    }

    /// same_address returns false when both arguments are distinct generated addresses.
    #[test]
    fn same_address_returns_false_for_distinct_addresses() {
        let env = Env::default();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        assert!(!same_address(&alice, &bob));
    }

    /// same_address is commutative: same_address(a, b) == same_address(b, a).
    #[test]
    fn same_address_is_commutative() {
        let env = Env::default();
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        assert_eq!(same_address(&alice, &bob), same_address(&bob, &alice));
    }

    /// Three distinct addresses are all pairwise unequal (transitivity check).
    #[test]
    fn same_address_three_distinct_addresses_all_pairwise_unequal() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        assert!(!same_address(&a, &b));
        assert!(!same_address(&b, &c));
        assert!(!same_address(&a, &c));
    }
}
