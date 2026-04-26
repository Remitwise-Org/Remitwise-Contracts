#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use remitwise_common::{CoverageType, EventCategory, EventPriority, RemitwiseEvents};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518_400; // ~30 days

// Pagination constants
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

/// Maximum number of active policies a single owner may hold.
pub const MAX_POLICIES_PER_OWNER: u32 = 50;

/// Maximum length for external reference strings
const MAX_EXTERNAL_REF_LEN: u32 = 64;

// Storage keys
const KEY_PAUSE_ADMIN: Symbol = symbol_short!("PAUSE_ADM");
const KEY_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
const KEY_POLICIES: Symbol = symbol_short!("POLICIES");
const KEY_OWNER_INDEX: Symbol = symbol_short!("OWN_IDX");
const KEY_ARCHIVED: Symbol = symbol_short!("ARCH_POL");
const KEY_STATS: Symbol = symbol_short!("STOR_STAT");
const KEY_OWNER_ACTIVE: Symbol = symbol_short!("OWN_ACT");
const KEY_EXT_REF_IDX: Symbol = symbol_short!("EXT_IDX");

/// Errors returned by the Insurance contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    PolicyNotFound = 1,
    Unauthorized = 2,
    PolicyLimitExceeded = 3,
    InvalidExternalRef = 4,
    DuplicateExternalRef = 5,
}

pub const EVT_POLICY_CREATED: Symbol = symbol_short!("created");
pub const EVT_PREMIUM_PAID: Symbol = symbol_short!("paid");
pub const EVT_POLICY_DEACTIVATED: Symbol = symbol_short!("deactive");
pub const EVT_EXT_REF_UPDATED: Symbol = symbol_short!("ext_ref");

#[derive(Clone)]
#[contracttype]
pub struct PolicyCreatedEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct PremiumPaidEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub amount: i128,
    pub next_payment_date: u64,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct PolicyDeactivatedEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct ExternalRefUpdatedEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub external_ref: Option<String>,
    pub timestamp: u64,
}

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
pub struct ArchivedPolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub archived_at: u64,
    pub next_payment_date: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyPage {
    pub items: Vec<InsurancePolicy>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct StorageStats {
    pub active_policies: u32,
    pub archived_policies: u32,
    pub last_updated: u64,
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

    fn read_stats(env: &Env) -> StorageStats {
        env.storage()
            .instance()
            .get(&KEY_STATS)
            .unwrap_or(StorageStats {
                active_policies: 0,
                archived_policies: 0,
                last_updated: 0,
            })
    }

    fn write_stats(env: &Env, stats: StorageStats) {
        env.storage().instance().set(&KEY_STATS, &stats);
    }

    fn owner_active_count(env: &Env, owner: &Address) -> u32 {
        let counts: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_ACTIVE)
            .unwrap_or_else(|| Map::new(env));
        counts.get(owner.clone()).unwrap_or(0)
    }

    fn adjust_owner_active(env: &Env, owner: &Address, delta: i32) {
        let mut counts: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_ACTIVE)
            .unwrap_or_else(|| Map::new(env));
        let current = counts.get(owner.clone()).unwrap_or(0);
        let next = if delta >= 0 {
            current.saturating_add(delta as u32)
        } else {
            current.saturating_sub((-delta) as u32)
        };
        counts.set(owner.clone(), next);
        env.storage().instance().set(&KEY_OWNER_ACTIVE, &counts);
    }

    fn get_external_ref_index(env: &Env) -> Map<(Address, String), u32> {
        env.storage()
            .instance()
            .get(&KEY_EXT_REF_IDX)
            .unwrap_or_else(|| Map::new(env))
    }

    fn validate_external_ref(ext_ref: &String) {
        let len = ext_ref.len();
        if len == 0 || len > MAX_EXTERNAL_REF_LEN {
            panic!("invalid external_ref length");
        }
        let mut buf = [0u8; 64];
        let copy_len = (len as usize).min(buf.len());
        ext_ref.copy_into_slice(&mut buf[..copy_len]);
        if !buf[..copy_len]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
        {
            panic!("invalid external_ref charset");
        }
    }

    fn bind_external_ref(env: &Env, owner: &Address, policy_id: u32, ext_ref: &Option<String>) {
        if let Some(r) = ext_ref {
            let mut index = Self::get_external_ref_index(env);
            if index.contains_key((owner.clone(), r.clone())) {
                panic!("external_ref already in use for owner");
            }
            index.set((owner.clone(), r.clone()), policy_id);
            env.storage().instance().set(&KEY_EXT_REF_IDX, &index);
        }
    }

    fn unbind_external_ref(env: &Env, owner: &Address, _policy_id: u32, ext_ref: &Option<String>) {
        if let Some(r) = ext_ref {
            let mut index = Self::get_external_ref_index(env);
            index.remove((owner.clone(), r.clone()));
            env.storage().instance().set(&KEY_EXT_REF_IDX, &index);
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

        if let Some(ref r) = external_ref {
            Self::validate_external_ref(r);
        }

        let active_count = Self::owner_active_count(&env, &owner);
        if active_count >= MAX_POLICIES_PER_OWNER {
            panic!("Policy limit exceeded");
        }

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
            external_ref: external_ref.clone(),
            coverage_type,
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date: env.ledger().timestamp() + (30 * 86_400),
        };

        Self::bind_external_ref(&env, &owner, next_id, &external_ref);
        policies.set(next_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        let mut index: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));
        let mut ids = index.get(owner.clone()).unwrap_or_else(|| Vec::new(&env));
        ids.push_back(next_id);
        index.set(owner.clone(), ids);
        env.storage().instance().set(&KEY_OWNER_INDEX, &index);

        env.storage().instance().set(&KEY_NEXT_ID, &next_id);

        Self::adjust_owner_active(&env, &owner, 1);
        let mut stats = Self::read_stats(&env);
        stats.active_policies += 1;
        stats.last_updated = env.ledger().timestamp();
        Self::write_stats(&env, stats);

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            EVT_POLICY_CREATED,
            PolicyCreatedEvent {
                policy_id: next_id,
                owner,
                coverage_type,
                monthly_premium,
                coverage_amount,
                timestamp: env.ledger().timestamp(),
            },
        );

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

        if policy.active {
            policy.active = false;
            policies.set(policy_id, policy.clone());
            env.storage().instance().set(&KEY_POLICIES, &policies);

            Self::unbind_external_ref(&env, &caller, policy_id, &policy.external_ref);
            Self::adjust_owner_active(&env, &caller, -1);
            let mut stats = Self::read_stats(&env);
            stats.active_policies = stats.active_policies.saturating_sub(1);
            stats.last_updated = env.ledger().timestamp();
            Self::write_stats(&env, stats);

            RemitwiseEvents::emit(
                &env,
                EventCategory::State,
                EventPriority::Medium,
                EVT_POLICY_DEACTIVATED,
                PolicyDeactivatedEvent {
                    policy_id,
                    owner: caller,
                    timestamp: env.ledger().timestamp(),
                },
            );
        }

        true
    }

    pub fn set_external_ref(
        env: Env,
        caller: Address,
        policy_id: u32,
        external_ref: Option<String>,
    ) -> bool {
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

        if let Some(ref r) = external_ref {
            Self::validate_external_ref(r);
        }

        if policy.external_ref != external_ref {
            Self::unbind_external_ref(&env, &caller, policy_id, &policy.external_ref);
            Self::bind_external_ref(&env, &caller, policy_id, &external_ref);
            policy.external_ref = external_ref.clone();
            policies.set(policy_id, policy);
            env.storage().instance().set(&KEY_POLICIES, &policies);

            RemitwiseEvents::emit(
                &env,
                EventCategory::State,
                EventPriority::Low,
                EVT_EXT_REF_UPDATED,
                ExternalRefUpdatedEvent {
                    policy_id,
                    owner: caller,
                    external_ref,
                    timestamp: env.ledger().timestamp(),
                },
            );
        }

        true
    }

    pub fn archive_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&KEY_ARCHIVED)
            .unwrap_or_else(|| Map::new(&env));

        let policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != caller {
            return false;
        }

        if policy.active {
            Self::unbind_external_ref(&env, &caller, policy_id, &policy.external_ref);
            Self::adjust_owner_active(&env, &caller, -1);
            let mut stats = Self::read_stats(&env);
            stats.active_policies = stats.active_policies.saturating_sub(1);
            Self::write_stats(&env, stats);
        }

        archived.set(
            policy_id,
            ArchivedPolicy {
                id: policy.id,
                owner: policy.owner,
                name: policy.name,
                external_ref: policy.external_ref,
                coverage_type: policy.coverage_type,
                monthly_premium: policy.monthly_premium,
                coverage_amount: policy.coverage_amount,
                archived_at: env.ledger().timestamp(),
                next_payment_date: policy.next_payment_date,
            },
        );
        policies.remove(policy_id);

        env.storage().instance().set(&KEY_POLICIES, &policies);
        env.storage().instance().set(&KEY_ARCHIVED, &archived);

        let mut stats = Self::read_stats(&env);
        stats.archived_policies += 1;
        stats.last_updated = env.ledger().timestamp();
        Self::write_stats(&env, stats);

        true
    }

    pub fn restore_policy(env: Env, caller: Address, policy_id: u32) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&KEY_ARCHIVED)
            .unwrap_or_else(|| Map::new(&env));
        let record = match archived.get(policy_id) {
            Some(r) => r,
            None => return false,
        };
        if record.owner != caller {
            return false;
        }

        let active_count = Self::owner_active_count(&env, &caller);
        if active_count >= MAX_POLICIES_PER_OWNER {
            return false;
        }

        if let Some(ref r) = record.external_ref {
            let index = Self::get_external_ref_index(&env);
            if index.contains_key((caller.clone(), r.clone())) {
                return false;
            }
        }

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        Self::bind_external_ref(&env, &caller, policy_id, &record.external_ref);
        policies.set(
            policy_id,
            InsurancePolicy {
                id: record.id,
                owner: record.owner,
                name: record.name,
                external_ref: record.external_ref,
                coverage_type: record.coverage_type,
                monthly_premium: record.monthly_premium,
                coverage_amount: record.coverage_amount,
                active: true,
                next_payment_date: record.next_payment_date,
            },
        );
        archived.remove(policy_id);

        env.storage().instance().set(&KEY_POLICIES, &policies);
        env.storage().instance().set(&KEY_ARCHIVED, &archived);

        Self::adjust_owner_active(&env, &caller, 1);
        let mut stats = Self::read_stats(&env);
        stats.archived_policies = stats.archived_policies.saturating_sub(1);
        stats.active_policies += 1;
        stats.last_updated = env.ledger().timestamp();
        Self::write_stats(&env, stats);

        true
    }

    pub fn get_archived_policy(env: Env, policy_id: u32) -> Option<ArchivedPolicy> {
        Self::extend_instance_ttl(&env);
        let archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&KEY_ARCHIVED)
            .unwrap_or_else(|| Map::new(&env));
        archived.get(policy_id)
    }

    pub fn get_policy_id_by_external_ref(
        env: Env,
        owner: Address,
        external_ref: String,
    ) -> Option<u32> {
        Self::extend_instance_ttl(&env);
        let index = Self::get_external_ref_index(&env);
        index.get((owner, external_ref))
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

        let amount = policy.monthly_premium;
        policy.next_payment_date = env.ledger().timestamp() + (30 * 86_400);
        let next_payment_date = policy.next_payment_date;
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Low,
            EVT_PREMIUM_PAID,
            PremiumPaidEvent {
                policy_id,
                owner: caller,
                amount,
                next_payment_date,
                timestamp: env.ledger().timestamp(),
            },
        );

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
        let now = env.ledger().timestamp();
        let next_date = now + (30 * 86_400);

        for id in policy_ids.iter() {
            if let Some(mut p) = policies.get(id) {
                if p.owner == caller && p.active {
                    let amount = p.monthly_premium;
                    p.next_payment_date = next_date;
                    policies.set(id, p);

                    RemitwiseEvents::emit(
                        &env,
                        EventCategory::Transaction,
                        EventPriority::Low,
                        EVT_PREMIUM_PAID,
                        PremiumPaidEvent {
                            policy_id: id,
                            owner: caller.clone(),
                            amount,
                            next_payment_date: next_date,
                            timestamp: now,
                        },
                    );
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
                if p.active {
                    items.push_back(p);
                    next_cursor = id;
                    if items.len() >= limit {
                        break;
                    }
                }
            }
        }

        let out_cursor = if items.len() < limit { 0 } else { next_cursor };
        PolicyPage {
            items: items.clone(),
            next_cursor: out_cursor,
            count: items.len(),
        }
    }

    pub fn get_storage_stats(env: Env) -> StorageStats {
        Self::extend_instance_ttl(&env);
        Self::read_stats(&env)
    }
}
