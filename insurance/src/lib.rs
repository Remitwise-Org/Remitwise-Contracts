#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use remitwise_common::CoverageType;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Symbol, Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518_400; // ~30 days

// Pagination constants (used by tests)
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

// Storage keys
const KEY_PAUSE_ADMIN: Symbol = symbol_short!("PAUSE_ADM");
const KEY_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
const KEY_POLICIES: Symbol = symbol_short!("POLICIES");
const KEY_OWNER_INDEX: Symbol = symbol_short!("OWN_IDX");

#[contracttype]
#[derive(Clone)]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyPage {
    pub items: Vec<InsurancePolicy>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contract]
pub struct Insurance;

#[contractimpl]
impl Insurance {
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        }
    }

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);
        env.storage().instance().set(&KEY_PAUSE_ADMIN, &new_admin);
        true
    }

    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        external_ref: Option<String>,
    ) -> u32 {
        owner.require_auth();
        Self::extend_instance_ttl(&env);

        let mut next_id: u32 = env.storage().instance().get(&KEY_NEXT_ID).unwrap_or(0);
        next_id += 1;

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let policy = InsurancePolicy {
            id: next_id,
            owner: owner.clone(),
            name,
            external_ref,
            coverage_type,
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date: env.ledger().timestamp() + (30 * 86_400),
        };
        policies.set(next_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        let mut index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));
        let mut ids = index.get(owner.clone()).unwrap_or_else(|| Vec::new(&env));
        ids.push_back(next_id);
        index.set(owner, ids);
        env.storage().instance().set(&KEY_OWNER_INDEX, &index);

        env.storage().instance().set(&KEY_NEXT_ID, &next_id);
        next_id
    }

    pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
        Self::extend_instance_ttl(&env);
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        policies.get(policy_id)
    }

    pub fn deactivate_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != caller {
            return false;
        }
        policy.active = false;
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);
        true
    }

    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != caller || !policy.active {
            return false;
        }
        policy.next_payment_date = env.ledger().timestamp() + (30 * 86_400);
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);
        true
    }

    pub fn batch_pay_premiums(env: Env, caller: Address, policy_ids: Vec<u32>) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let mut count: u32 = 0;
        let next_date = env.ledger().timestamp() + (30 * 86_400);
        for id in policy_ids.iter() {
            if let Some(mut p) = policies.get(id) {
                if p.owner == caller && p.active {
                    p.next_payment_date = next_date;
                    policies.set(id, p);
                    count += 1;
                }
            }
        }
        env.storage().instance().set(&KEY_POLICIES, &policies);
        count
    }

    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        Self::extend_instance_ttl(&env);

        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));

        let ids = index.get(owner).unwrap_or_else(|| Vec::new(&env));
        let mut total: i128 = 0;
        for id in ids.iter() {
            if let Some(p) = policies.get(id) {
                if p.active {
                    total += p.monthly_premium;
                }
            }
        }
        total
    }

    /// Returns a stable, cursor-based page of active policies for an owner.
    pub fn get_active_policies(env: Env, owner: Address, cursor: u32, limit: u32) -> PolicyPage {
        Self::extend_instance_ttl(&env);
        let limit = Self::clamp_limit(limit);

        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));
        let ids = index.get(owner).unwrap_or_else(|| Vec::new(&env));

        let mut items: Vec<InsurancePolicy> = Vec::new(&env);
        let mut next_cursor: u32 = 0;

        for id in ids.iter() {
            if id <= cursor {
                continue;
            }
            if let Some(p) = policies.get(id) {
                if !p.active {
                    continue;
                }
                items.push_back(p);
                next_cursor = id;
                if items.len() >= limit {
                    break;
                }
            }
        }

        // If we returned a full page, we may or may not have more items;
        // keep the cursor as the last returned id (caller can continue).
        // If we returned less than a full page, no more data -> cursor 0.
        let out_cursor = if items.len() < limit { 0 } else { next_cursor };

        let count = items.len();
        PolicyPage {
            items,
            next_cursor: out_cursor,
            count,
        }
    }
}
