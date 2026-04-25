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

/// Maximum number of **active** policies a single owner may hold at one time.
///
/// Scope: active-only (deactivated and archived policies do not count toward
/// this limit). This prevents unbounded storage growth and mitigates DoS via
/// policy spam. The value matches `MAX_PAGE_LIMIT` so a single page always
/// covers the full active set for any owner.
pub const MAX_POLICIES_PER_OWNER: u32 = 50;

// Storage keys
const KEY_PAUSE_ADMIN: Symbol = symbol_short!("PAUSE_ADM");
const KEY_NEXT_ID: Symbol = symbol_short!("NEXT_ID");
const KEY_POLICIES: Symbol = symbol_short!("POLICIES");
const KEY_OWNER_INDEX: Symbol = symbol_short!("OWN_IDX");
const KEY_ARCHIVED: Symbol = symbol_short!("ARCH_POL");
const KEY_STATS: Symbol = symbol_short!("STOR_STAT");

// Per-owner active-policy counter map key
const KEY_OWNER_ACTIVE: Symbol = symbol_short!("OWN_ACT");

/// Errors returned by the Insurance contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    /// Policy with the given ID does not exist.
    PolicyNotFound = 1,
    /// Caller is not the policy owner.
    Unauthorized = 2,
    /// Owner has reached `MAX_POLICIES_PER_OWNER` active policies.
    PolicyLimitExceeded = 3,
}

// ---------------------------------------------------------------------------
// Event topic symbols — locked; changing these is a breaking change for indexers
// ---------------------------------------------------------------------------

/// Topic action for PolicyCreated: (Remitwise, Transaction=0, Medium=1, "created")
pub const EVT_POLICY_CREATED: Symbol = symbol_short!("created");
/// Topic action for PremiumPaid: (Remitwise, Transaction=0, Low=0, "paid")
pub const EVT_PREMIUM_PAID: Symbol = symbol_short!("paid");
/// Topic action for PolicyDeactivated: (Remitwise, State=1, Medium=1, "deactive")
pub const EVT_POLICY_DEACTIVATED: Symbol = symbol_short!("deactive");
/// Topic action for ExternalRefUpdated: (Remitwise, State=1, Low=0, "ext_ref")
pub const EVT_EXT_REF_UPDATED: Symbol = symbol_short!("ext_ref");

// ---------------------------------------------------------------------------
// Event payload structs
// ---------------------------------------------------------------------------

/// Payload emitted when a new insurance policy is created.
///
/// Topics: `("Remitwise", Transaction=0, Medium=1, "created")`
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

/// Payload emitted when a premium payment is recorded.
///
/// Topics: `("Remitwise", Transaction=0, Low=0, "paid")`
#[derive(Clone)]
#[contracttype]
pub struct PremiumPaidEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub amount: i128,
    pub next_payment_date: u64,
    pub timestamp: u64,
}

/// Payload emitted when a policy is deactivated.
///
/// Topics: `("Remitwise", State=1, Medium=1, "deactive")`
#[derive(Clone)]
#[contracttype]
pub struct PolicyDeactivatedEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub timestamp: u64,
}

/// Payload emitted when a policy's external reference is updated or cleared.
///
/// Topics: `("Remitwise", State=1, Low=0, "ext_ref")`
#[derive(Clone)]
#[contracttype]
pub struct ExternalRefUpdatedEvent {
    pub policy_id: u32,
    pub owner: Address,
    pub external_ref: Option<String>,
    pub timestamp: u64,
}

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

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

/// Compact record stored in the archive after a policy is deactivated and
/// explicitly archived via `archive_policies`.
#[contracttype]
#[derive(Clone)]
pub struct ArchivedPolicy {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub archived_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct PolicyPage {
    pub items: Vec<InsurancePolicy>,
    pub next_cursor: u32,
    pub count: u32,
}

/// Snapshot of contract storage usage.
///
/// Counters are updated deterministically on every lifecycle transition:
/// - `create_policy`   → `active_policies` +1
/// - `deactivate_policy` → `active_policies` -1
/// - `archive_policies`  → `archived_policies` +N, `active_policies` unchanged
///   (deactivated policies are already excluded from the active count)
/// - `restore_policy`    → `archived_policies` -1, `active_policies` +1
/// - `cleanup_policies`  → `archived_policies` -N
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
    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

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

    /// Read the current `StorageStats`, defaulting to zeroes if not yet written.
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

    /// Return the number of active policies for `owner`.
    fn owner_active_count(env: &Env, owner: &Address) -> u32 {
        let counts: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&KEY_OWNER_ACTIVE)
            .unwrap_or_else(|| Map::new(env));
        counts.get(owner.clone()).unwrap_or(0)
    }

    /// Adjust the per-owner active-policy counter by `delta` (+1 or -1).
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

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);
        env.storage().instance().set(&KEY_PAUSE_ADMIN, &new_admin);
        true
    }

    // -----------------------------------------------------------------------
    // Core policy lifecycle
    // -----------------------------------------------------------------------

    /// Create a new insurance policy for `owner`.
    ///
    /// # Errors
    /// - `PolicyLimitExceeded` if the owner already holds `MAX_POLICIES_PER_OWNER`
    ///   active policies.
    pub fn create_policy(
        env: Env,
        owner: Address,
        name: String,
        coverage_type: CoverageType,
        monthly_premium: i128,
        coverage_amount: i128,
        external_ref: Option<String>,
    ) -> Result<u32, InsuranceError> {
        owner.require_auth();
        Self::extend_instance_ttl(&env);

        // Enforce per-owner active-policy cap.
        let active_count = Self::owner_active_count(&env, &owner);
        if active_count >= MAX_POLICIES_PER_OWNER {
            return Err(InsuranceError::PolicyLimitExceeded);
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
        policies.set(next_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        // Update owner index.
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

        // Update counters.
        Self::adjust_owner_active(&env, &owner, 1);
        let mut stats = Self::read_stats(&env);
        stats.active_policies = stats.active_policies.saturating_add(1);
        stats.last_updated = env.ledger().timestamp();
        Self::write_stats(&env, stats);

        Ok(next_id)
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

    /// Deactivate a policy owned by `caller`.
    ///
    /// Decrements the owner's active-policy counter and the global
    /// `active_policies` stat so the slot becomes available for a new policy.
    pub fn deactivate_policy(
        env: Env,
        caller: Address,
        policy_id: u32,
    ) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return Err(InsuranceError::PolicyNotFound),
        };
        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        // Only decrement if the policy was still active.
        let was_active = policy.active;
        policy.active = false;
        policies.set(policy_id, policy.clone());
        env.storage().instance().set(&KEY_POLICIES, &policies);

        if was_active {
            Self::adjust_owner_active(&env, &caller, -1);
            let mut stats = Self::read_stats(&env);
            stats.active_policies = stats.active_policies.saturating_sub(1);
            stats.last_updated = env.ledger().timestamp();
            Self::write_stats(&env, stats);
        }

        Ok(true)
    }

    pub fn set_external_ref(
        env: Env,
        caller: Address,
        policy_id: u32,
        external_ref: Option<String>,
    ) -> bool {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        if let Some(ref_value) = external_ref.clone() {
            Self::validate_external_ref(&ref_value);
        }

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

        let old_external_ref = policy.external_ref.clone();

        if external_ref == old_external_ref {
            return true;
        }

        if let Some(new_ref) = external_ref.clone() {
            Self::ensure_external_ref_available(&env, &policy.owner, &new_ref, Some(policy_id));
        }

        Self::unbind_external_ref(&env, &policy.owner, policy_id, &old_external_ref);
        Self::bind_external_ref(&env, &policy.owner, policy_id, &external_ref);

        policy.external_ref = external_ref.clone();
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);

        env.events().publish(
            (EVENT_EXTERNAL_REF_UPDATED,),
            ExternalRefUpdatedEvent {
                policy_id,
                owner: caller,
                external_ref,
            },
        );

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
            .get(&KEY_ARCHIVED_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if policy.owner != caller {
            return false;
        }

        Self::unbind_external_ref(&env, &policy.owner, policy_id, &policy.external_ref);

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
                next_payment_date: policy.next_payment_date,
            },
        );
        policies.remove(policy_id);

        env.storage().instance().set(&KEY_POLICIES, &policies);
        env.storage()
            .instance()
            .set(&KEY_ARCHIVED_POLICIES, &archived);
        true
    }

    pub fn restore_policy(env: Env, caller: Address, policy_id: u32) -> bool {
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
            .get(&KEY_ARCHIVED_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let archived_policy = match archived.get(policy_id) {
            Some(p) => p,
            None => return false,
        };
        if archived_policy.owner != caller {
            return false;
        }

        if let Some(ref_value) = archived_policy.external_ref.clone() {
            let index = Self::get_external_ref_index(&env);
            if let Some(existing_id) = index.get((archived_policy.owner.clone(), ref_value)) {
                if existing_id != policy_id {
                    return false;
                }
            }
        }

        Self::bind_external_ref(
            &env,
            &archived_policy.owner,
            policy_id,
            &archived_policy.external_ref,
        );

        policies.set(
            policy_id,
            InsurancePolicy {
                id: archived_policy.id,
                owner: archived_policy.owner,
                name: archived_policy.name,
                external_ref: archived_policy.external_ref,
                coverage_type: archived_policy.coverage_type,
                monthly_premium: archived_policy.monthly_premium,
                coverage_amount: archived_policy.coverage_amount,
                active: true,
                next_payment_date: archived_policy.next_payment_date,
            },
        );
        archived.remove(policy_id);

        env.storage().instance().set(&KEY_POLICIES, &policies);
        env.storage()
            .instance()
            .set(&KEY_ARCHIVED_POLICIES, &archived);
        true
    }

    pub fn get_archived_policy(env: Env, policy_id: u32) -> Option<ArchivedPolicy> {
        Self::extend_instance_ttl(&env);
        let archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&KEY_ARCHIVED_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        archived.get(policy_id)
    }

    pub fn get_policy_id_by_external_ref(
        env: Env,
        owner: Address,
        external_ref: String,
    ) -> Option<u32> {
        Self::extend_instance_ttl(&env);

        let len = external_ref.len();
        if len == 0 || len > MAX_EXTERNAL_REF_LEN {
            return None;
        }
        let copy_len = len as usize;
        let mut buf = [0u8; MAX_EXTERNAL_REF_LEN as usize];
        external_ref.copy_into_slice(&mut buf[..copy_len]);
        if !buf[..copy_len]
            .iter()
            .all(|&b| Self::is_allowed_external_ref_byte(b))
        {
            return None;
        }

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

    /// Update or clear the external reference for a policy.
    ///
    /// Emits an `ExternalRefUpdatedEvent` with topics
    /// `("Remitwise", State=1, Low=0, "ext_ref")`.
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
        let ts = env.ledger().timestamp();

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
                            timestamp: ts,
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

        let out_cursor = if items.len() < limit { 0 } else { next_cursor };
        let count = items.len();
        PolicyPage {
            items,
            next_cursor: out_cursor,
            count,
        }
    }

    // -----------------------------------------------------------------------
    // Archive / restore / cleanup
    // -----------------------------------------------------------------------

    /// Move all **inactive** policies owned by `caller` into the archive.
    ///
    /// Returns the number of policies archived. Increments
    /// `StorageStats::archived_policies` by that count. The `active_policies`
    /// counter is unaffected because `deactivate_policy` already decremented it.
    pub fn archive_policies(env: Env, caller: Address) -> u32 {
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

        let now = env.ledger().timestamp();
        let mut to_remove: Vec<u32> = Vec::new(&env);
        let mut count: u32 = 0;

        for (id, policy) in policies.iter() {
            if policy.owner == caller && !policy.active {
                archived.set(
                    id,
                    ArchivedPolicy {
                        id: policy.id,
                        owner: policy.owner.clone(),
                        name: policy.name.clone(),
                        coverage_type: policy.coverage_type,
                        monthly_premium: policy.monthly_premium,
                        coverage_amount: policy.coverage_amount,
                        archived_at: now,
                    },
                );
                to_remove.push_back(id);
                count += 1;
            }
        }

        for id in to_remove.iter() {
            policies.remove(id);
        }

        env.storage().instance().set(&KEY_POLICIES, &policies);
        env.storage().instance().set(&KEY_ARCHIVED, &archived);

        if count > 0 {
            let mut stats = Self::read_stats(&env);
            stats.archived_policies = stats.archived_policies.saturating_add(count);
            stats.last_updated = now;
            Self::write_stats(&env, stats);
        }

        count
    }

    /// Restore an archived policy back to active storage.
    ///
    /// The restored policy is marked **inactive** (the caller must explicitly
    /// reactivate it via a future mechanism if needed). Decrements
    /// `archived_policies` and increments `active_policies`.
    ///
    /// # Errors
    /// - `PolicyNotFound` if no archived policy with `policy_id` exists.
    /// - `Unauthorized` if `caller` is not the policy owner.
    pub fn restore_policy(env: Env, caller: Address, policy_id: u32) -> Result<(), InsuranceError> {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&KEY_ARCHIVED)
            .unwrap_or_else(|| Map::new(&env));
        let record = match archived.get(policy_id) {
            Some(r) => r,
            None => return Err(InsuranceError::PolicyNotFound),
        };
        if record.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }

        // Enforce cap on restore as well — restoring counts as gaining an active slot.
        let active_count = Self::owner_active_count(&env, &caller);
        if active_count >= MAX_POLICIES_PER_OWNER {
            return Err(InsuranceError::PolicyLimitExceeded);
        }

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        policies.set(
            policy_id,
            InsurancePolicy {
                id: record.id,
                owner: record.owner.clone(),
                name: record.name.clone(),
                external_ref: None,
                coverage_type: record.coverage_type,
                monthly_premium: record.monthly_premium,
                coverage_amount: record.coverage_amount,
                // Restored as inactive; owner decides whether to reactivate.
                active: false,
                next_payment_date: env.ledger().timestamp() + (30 * 86_400),
            },
        );
        archived.remove(policy_id);

        env.storage().instance().set(&KEY_POLICIES, &policies);
        env.storage().instance().set(&KEY_ARCHIVED, &archived);

        let mut stats = Self::read_stats(&env);
        stats.archived_policies = stats.archived_policies.saturating_sub(1);
        // Restored policy is inactive, so active_policies is unchanged here.
        stats.last_updated = env.ledger().timestamp();
        Self::write_stats(&env, stats);

        Ok(())
    }

    /// Permanently delete archived policies with `archived_at < before_timestamp`.
    ///
    /// Returns the number of records deleted. Decrements `archived_policies`
    /// by that count.
    pub fn cleanup_policies(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedPolicy> = env
            .storage()
            .instance()
            .get(&KEY_ARCHIVED)
            .unwrap_or_else(|| Map::new(&env));

        let mut to_remove: Vec<u32> = Vec::new(&env);
        let mut count: u32 = 0;

        for (id, record) in archived.iter() {
            if record.archived_at < before_timestamp {
                to_remove.push_back(id);
                count += 1;
            }
        }

        for id in to_remove.iter() {
            archived.remove(id);
        }

        env.storage().instance().set(&KEY_ARCHIVED, &archived);

        if count > 0 {
            let mut stats = Self::read_stats(&env);
            stats.archived_policies = stats.archived_policies.saturating_sub(count);
            stats.last_updated = env.ledger().timestamp();
            Self::write_stats(&env, stats);
        }

        count
    }

    /// Return a snapshot of contract storage usage.
    pub fn get_storage_stats(env: Env) -> StorageStats {
        Self::extend_instance_ttl(&env);
        Self::read_stats(&env)
    }

    /// Set or clear the `external_ref` on a policy owned by `caller`.
    pub fn set_external_ref(
        env: Env,
        caller: Address,
        policy_id: u32,
        external_ref: Option<String>,
    ) -> Result<(), InsuranceError> {
        caller.require_auth();
        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut policy = match policies.get(policy_id) {
            Some(p) => p,
            None => return Err(InsuranceError::PolicyNotFound),
        };
        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }
        policy.external_ref = external_ref;
        policies.set(policy_id, policy);
        env.storage().instance().set(&KEY_POLICIES, &policies);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Insurance, InsuranceClient};
    use remitwise_common::CoverageType;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String};

    fn setup() -> (Env, InsuranceClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);
        (env, client, owner_a, owner_b)
    }

    fn policy_name(env: &Env) -> String {
        String::from_str(env, "Policy")
    }

    fn ext(env: &Env, v: &str) -> Option<String> {
        Some(String::from_str(env, v))
    }

    #[test]
    #[should_panic(expected = "external_ref already in use for owner")]
    fn duplicate_external_ref_same_owner_panics() {
        let (env, client, owner, _) = setup();
        let name = policy_name(&env);
        client.create_policy(
            &owner,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "INV-001"),
        );
        client.create_policy(
            &owner,
            &name,
            &CoverageType::Life,
            &200,
            &20_000,
            &ext(&env, "INV-001"),
        );
    }

    #[test]
    fn duplicate_external_ref_different_owners_allowed() {
        let (env, client, owner_a, owner_b) = setup();
        let name = policy_name(&env);

        let id_a = client.create_policy(
            &owner_a,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "INV-001"),
        );
        let id_b = client.create_policy(
            &owner_b,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "INV-001"),
        );

        assert_eq!(
            client.get_policy_id_by_external_ref(&owner_a, &String::from_str(&env, "INV-001")),
            Some(id_a)
        );
        assert_eq!(
            client.get_policy_id_by_external_ref(&owner_b, &String::from_str(&env, "INV-001")),
            Some(id_b)
        );
    }

    #[test]
    #[should_panic(expected = "invalid external_ref charset")]
    fn invalid_external_ref_charset_panics() {
        let (env, client, owner, _) = setup();
        let name = policy_name(&env);
        client.create_policy(
            &owner,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "bad ref"),
        );
    }

    #[test]
    fn clearing_external_ref_allows_reuse() {
        let (env, client, owner, _) = setup();
        let name = policy_name(&env);

        let id1 = client.create_policy(
            &owner,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "INV-002"),
        );

        assert!(client.set_external_ref(&owner, &id1, &None));
        assert_eq!(
            client.get_policy_id_by_external_ref(&owner, &String::from_str(&env, "INV-002")),
            None
        );

        let id2 = client.create_policy(
            &owner,
            &name,
            &CoverageType::Life,
            &100,
            &10_000,
            &ext(&env, "INV-002"),
        );
        assert_eq!(
            client.get_policy_id_by_external_ref(&owner, &String::from_str(&env, "INV-002")),
            Some(id2)
        );
    }

    #[test]
    fn deactivate_clears_external_ref_mapping_allows_reuse() {
        let (env, client, owner, _) = setup();
        let name = policy_name(&env);

        let id1 = client.create_policy(
            &owner,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "INV-003"),
        );
        assert!(client.deactivate_policy(&owner, &id1));

        let id2 = client.create_policy(
            &owner,
            &name,
            &CoverageType::Property,
            &100,
            &10_000,
            &ext(&env, "INV-003"),
        );
        assert_eq!(
            client.get_policy_id_by_external_ref(&owner, &String::from_str(&env, "INV-003")),
            Some(id2)
        );
    }

    #[test]
    fn restore_with_conflicting_external_ref_fails_closed() {
        let (env, client, owner, _) = setup();
        let name = policy_name(&env);

        let id1 = client.create_policy(
            &owner,
            &name,
            &CoverageType::Health,
            &100,
            &10_000,
            &ext(&env, "INV-004"),
        );
        assert!(client.archive_policy(&owner, &id1));

        let id2 = client.create_policy(
            &owner,
            &name,
            &CoverageType::Auto,
            &120,
            &12_000,
            &ext(&env, "INV-004"),
        );

        assert!(!client.restore_policy(&owner, &id1));
        assert_eq!(
            client.get_policy_id_by_external_ref(&owner, &String::from_str(&env, "INV-004")),
            Some(id2)
        );
        assert!(client.get_archived_policy(&id1).is_some());
    }
}

#[cfg(test)]
mod test;
