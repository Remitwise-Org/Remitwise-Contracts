#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use remitwise_common::CoverageType;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Map, String, Vec,
};

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518_400; // ~30 days
const MAX_PAGE_LIMIT: u32 = 50;
const MAX_BATCH_SIZE: u32 = 50;

const SECONDS_PER_DAY: u64 = 86_400;
const DEFAULT_BILLING_DAYS: u64 = 30;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoliciesPage {
    pub count: u32,
    pub next_cursor: u32,
    pub items: Vec<InsurancePolicy>,
}

#[contract]
pub struct Insurance;

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn get_policies(env: &Env) -> Map<u32, InsurancePolicy> {
    env.storage()
        .instance()
        .get(&symbol_short!("POLICY"))
        .unwrap_or_else(|| Map::new(env))
}

fn set_policies(env: &Env, policies: &Map<u32, InsurancePolicy>) {
    env.storage()
        .instance()
        .set(&symbol_short!("POLICY"), policies);
}

fn get_next_id(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&symbol_short!("NEXTID"))
        .unwrap_or(0u32)
}

fn set_next_id(env: &Env, next_id: u32) {
    env.storage()
        .instance()
        .set(&symbol_short!("NEXTID"), &next_id);
}

fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        0
    } else if limit > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        limit
    }
}

#[contractimpl]
impl Insurance {
    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
    ) -> u32 {
        owner.require_auth();
        extend_instance_ttl(&env);

        if monthly_premium <= 0 || coverage_amount <= 0 {
            panic!("Invalid parameters");
        }

        let mut policies = get_policies(&env);
        let next_id = get_next_id(&env) + 1;
        let next_payment_date =
            env.ledger().timestamp() + (DEFAULT_BILLING_DAYS * SECONDS_PER_DAY);

        let policy = InsurancePolicy {
            id: next_id,
            owner: owner.clone(),
            name,
            coverage_type,
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date,
        };

        policies.set(next_id, policy);
        set_policies(&env, &policies);
        set_next_id(&env, next_id);
        next_id
    }

    pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
        let policies = get_policies(&env);
        policies.get(policy_id)
    }

    pub fn deactivate_policy(env: Env, owner: Address, policy_id: u32) -> bool {
        owner.require_auth();
        extend_instance_ttl(&env);

        let mut policies = get_policies(&env);
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != owner {
            return false;
        }
        policy.active = false;
        policies.set(policy_id, policy);
        set_policies(&env, &policies);
        true
    }

    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        let policies = get_policies(&env);
        let max_id = get_next_id(&env);

        let mut total: i128 = 0;
        let mut id = 1u32;
        while id <= max_id {
            if let Some(p) = policies.get(id) {
                if p.active && p.owner == owner {
                    total = total
                        .checked_add(p.monthly_premium)
                        .unwrap_or_else(|| panic!("overflow"));
                }
            }
            id += 1;
        }
        total
    }

    pub fn get_active_policies(env: Env, owner: Address, cursor: u32, limit: u32) -> PoliciesPage {
        let policies = get_policies(&env);
        let max_id = get_next_id(&env);
        let cap = clamp_limit(limit);

        let mut items: Vec<InsurancePolicy> = Vec::new(&env);
        let mut count: u32 = 0;
        let mut last_id: u32 = 0;

        if cap == 0 {
            return PoliciesPage {
                count: 0,
                next_cursor: 0,
                items,
            };
        }

        let mut id = cursor.saturating_add(1);
        while id <= max_id && count < cap {
            if let Some(p) = policies.get(id) {
                if p.active && p.owner == owner {
                    items.push_back(p);
                    count += 1;
                    last_id = id;
                }
            }
            id += 1;
        }

        let next_cursor = if count == 0 { 0 } else { last_id };
        PoliciesPage {
            count,
            next_cursor,
            items,
        }
    }

    pub fn pay_premium(env: Env, owner: Address, policy_id: u32) -> bool {
        owner.require_auth();
        extend_instance_ttl(&env);

        let mut policies = get_policies(&env);
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != owner || !policy.active {
            return false;
        }

        policy.next_payment_date =
            env.ledger().timestamp() + (DEFAULT_BILLING_DAYS * SECONDS_PER_DAY);
        policies.set(policy_id, policy);
        set_policies(&env, &policies);
        true
    }

    pub fn batch_pay_premiums(env: Env, owner: Address, policy_ids: Vec<u32>) -> u32 {
        owner.require_auth();
        extend_instance_ttl(&env);

        if policy_ids.len() > MAX_BATCH_SIZE {
            panic!("Batch too large");
        }

        let mut paid: u32 = 0;
        let mut policies = get_policies(&env);

        for id in policy_ids.iter() {
            if let Some(mut policy) = policies.get(id) {
                if policy.owner == owner && policy.active {
                    policy.next_payment_date =
                        env.ledger().timestamp() + (DEFAULT_BILLING_DAYS * SECONDS_PER_DAY);
                    policies.set(id, policy);
                    paid += 1;
                }
            }
        }

        set_policies(&env, &policies);
        paid
    }
}