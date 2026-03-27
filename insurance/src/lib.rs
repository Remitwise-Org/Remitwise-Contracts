#![no_std]
#![allow(dead_code)]

#[cfg(test)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

use remitwise_common::{clamp_limit, CoverageType, MAX_BATCH_SIZE};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InsuranceError {
    Unauthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    PolicyNotFound = 4,
    PolicyInactive = 5,
    InvalidName = 6,
    InvalidPremium = 7,
    InvalidCoverage = 8,
    UnsupportedCombination = 9,
    InvalidExternalRef = 10,
    MaxPoliciesReached = 11,
    ScheduleNotFound = 12,
}

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
    pub external_ref: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyPage {
    pub items: Vec<InsurancePolicy>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PremiumSchedule {
    pub id: u32,
    pub owner: Address,
    pub policy_id: u32,
    pub next_due: u64,
    pub interval: u64,
    pub active: bool,
    pub missed_count: u32,
}

#[contract]
pub struct Insurance;

const DATA_KEY_OWNER: Symbol = symbol_short!("OWNER");
const DATA_KEY_COUNTER: Symbol = symbol_short!("COUNTER");
const DATA_KEY_POLICIES: Symbol = symbol_short!("POL");
const DATA_KEY_SCHEDULE_COUNTER: Symbol = symbol_short!("SCH_CTR");
const DATA_KEY_SCHEDULES: Symbol = symbol_short!("SCH");

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

const MAX_POLICIES: u32 = 1000;

#[contractimpl]
impl Insurance {
    pub fn init(env: Env, owner: Address) -> Result<(), InsuranceError> {
        if env.storage().instance().has(&DATA_KEY_OWNER) {
            return Err(InsuranceError::AlreadyInitialized);
        }
        env.storage().instance().set(&DATA_KEY_OWNER, &owner);
        env.storage().instance().set(&DATA_KEY_COUNTER, &0u32);
        env.storage()
            .instance()
            .set(&DATA_KEY_SCHEDULE_COUNTER, &0u32);
        Ok(())
    }

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
        Self::require_initialized(&env)?;

        if name.is_empty() {
            return Err(InsuranceError::InvalidName);
        }
        if name.len() > 64 {
            return Err(InsuranceError::InvalidName);
        }
        if monthly_premium <= 0 {
            return Err(InsuranceError::InvalidPremium);
        }
        if coverage_amount <= 0 {
            return Err(InsuranceError::InvalidCoverage);
        }

        // Validate ranges based on CoverageType (from README)
        let (min_p, max_p, min_c, max_c) = match coverage_type {
            CoverageType::Health => (1_000_000, 500_000_000, 10_000_000, 100_000_000_000i128),
            CoverageType::Life => (500_000, 1_000_000_000, 50_000_000, 500_000_000_000i128),
            CoverageType::Property => {
                (2_000_000, 2_000_000_000, 100_000_000, 1_000_000_000_000i128)
            }
            CoverageType::Auto => (1_500_000, 750_000_000, 20_000_000, 200_000_000_000i128),
            CoverageType::Liability => (800_000, 400_000_000, 5_000_000, 50_000_000_000i128),
        };

        if monthly_premium < min_p || monthly_premium > max_p {
            return Err(InsuranceError::InvalidPremium);
        }
        if coverage_amount < min_c || coverage_amount > max_c {
            return Err(InsuranceError::InvalidCoverage);
        }

        // Apply ratio guard: coverage_amount <= monthly_premium * 12 * 500
        let max_leverage = monthly_premium
            .checked_mul(12)
            .and_then(|v| v.checked_mul(500))
            .ok_or(InsuranceError::UnsupportedCombination)?;
        if coverage_amount > max_leverage {
            return Err(InsuranceError::UnsupportedCombination);
        }

        if let Some(ref ext) = external_ref {
            if ext.is_empty() || ext.len() > 128 {
                return Err(InsuranceError::InvalidExternalRef);
            }
        }

        let mut counter: u32 = env.storage().instance().get(&DATA_KEY_COUNTER).unwrap_or(0);
        if counter >= MAX_POLICIES {
            return Err(InsuranceError::MaxPoliciesReached);
        }

        Self::extend_instance_ttl(&env);

        counter += 1;
        env.storage().instance().set(&DATA_KEY_COUNTER, &counter);

        let policy = InsurancePolicy {
            id: counter,
            owner: owner.clone(),
            name,
            coverage_type,
            monthly_premium,
            coverage_amount,
            active: true,
            next_payment_date: env.ledger().timestamp() + (30 * 86400),
            external_ref,
        };

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        policies.set(counter, policy);
        env.storage().instance().set(&DATA_KEY_POLICIES, &policies);

        Ok(counter)
    }

    pub fn get_policy(env: Env, id: u32) -> Option<InsurancePolicy> {
        let policies: Map<u32, InsurancePolicy> =
            env.storage().instance().get(&DATA_KEY_POLICIES)?;
        policies.get(id)
    }

    pub fn pay_premium(env: Env, caller: Address, policy_id: u32) -> Result<bool, InsuranceError> {
        caller.require_auth();
        Self::require_initialized(&env)?;

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .ok_or(InsuranceError::PolicyNotFound)?;
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }
        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }

        Self::extend_instance_ttl(&env);

        policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);
        policies.set(policy_id, policy);
        env.storage().instance().set(&DATA_KEY_POLICIES, &policies);

        Ok(true)
    }

    pub fn deactivate_policy(
        env: Env,
        owner: Address,
        policy_id: u32,
    ) -> Result<bool, InsuranceError> {
        owner.require_auth();
        Self::require_initialized(&env)?;

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .ok_or(InsuranceError::PolicyNotFound)?;
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        let contract_owner: Address = env
            .storage()
            .instance()
            .get(&DATA_KEY_OWNER)
            .ok_or(InsuranceError::NotInitialized)?;
        if owner != contract_owner {
            return Err(InsuranceError::Unauthorized);
        }

        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }

        policy.active = false;
        policies.set(policy_id, policy);
        env.storage().instance().set(&DATA_KEY_POLICIES, &policies);

        Ok(true)
    }

    pub fn get_active_policies(env: Env, owner: Address, cursor: u32, limit: u32) -> PolicyPage {
        let limit = clamp_limit(limit);
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let mut items = Vec::new(&env);
        let mut next_cursor = 0u32;
        let mut count = 0u32;

        for (id, policy) in policies.iter() {
            if id > cursor && policy.owner == owner && policy.active {
                if count < limit {
                    items.push_back(policy);
                    count += 1;
                    next_cursor = id;
                } else {
                    break;
                }
            }
        }

        PolicyPage {
            items,
            next_cursor,
            count,
        }
    }

    pub fn get_all_policies_for_owner(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> PolicyPage {
        let limit = clamp_limit(limit);
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));

        let mut items = Vec::new(&env);
        let mut next_cursor = 0u32;
        let mut count = 0u32;

        for (id, policy) in policies.iter() {
            if id > cursor && policy.owner == owner {
                if count < limit {
                    items.push_back(policy);
                    count += 1;
                    next_cursor = id;
                } else {
                    break;
                }
            }
        }

        PolicyPage {
            items,
            next_cursor,
            count,
        }
    }

    pub fn get_total_monthly_premium(env: Env, owner: Address) -> i128 {
        let policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .unwrap_or_else(|| Map::new(&env));
        let mut total = 0i128;
        for (_, policy) in policies.iter() {
            if policy.owner == owner && policy.active {
                total = total.saturating_add(policy.monthly_premium);
            }
        }
        total
    }

    pub fn batch_pay_premiums(env: Env, caller: Address, policy_ids: Vec<u32>) -> u32 {
        caller.require_auth();
        let mut paid_count = 0u32;
        let limit = policy_ids.len().min(MAX_BATCH_SIZE);

        for i in 0..limit {
            let Some(id) = policy_ids.get(i) else {
                continue;
            };
            // We do the logic inline to avoid nested require_auth and panics
            if let Ok(true) = Self::internal_pay_premium(&env, caller.clone(), id) {
                paid_count += 1;
            }
        }
        paid_count
    }

    fn internal_pay_premium(
        env: &Env,
        caller: Address,
        policy_id: u32,
    ) -> Result<bool, InsuranceError> {
        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&DATA_KEY_POLICIES)
            .ok_or(InsuranceError::PolicyNotFound)?;
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != caller {
            return Err(InsuranceError::Unauthorized);
        }
        if !policy.active {
            return Err(InsuranceError::PolicyInactive);
        }

        policy.next_payment_date = env.ledger().timestamp() + (30 * 86400);
        policies.set(policy_id, policy);
        env.storage().instance().set(&DATA_KEY_POLICIES, &policies);

        Ok(true)
    }

    pub fn create_premium_schedule(
        env: Env,
        owner: Address,
        policy_id: u32,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, InsuranceError> {
        owner.require_auth();
        Self::require_initialized(&env)?;

        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DATA_KEY_SCHEDULE_COUNTER)
            .unwrap_or(0);
        counter += 1;
        env.storage()
            .instance()
            .set(&DATA_KEY_SCHEDULE_COUNTER, &counter);

        let schedule = PremiumSchedule {
            id: counter,
            owner: owner.clone(),
            policy_id,
            next_due,
            interval,
            active: true,
            missed_count: 0,
        };

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DATA_KEY_SCHEDULES)
            .unwrap_or_else(|| Map::new(&env));
        schedules.set(counter, schedule);
        env.storage()
            .instance()
            .set(&DATA_KEY_SCHEDULES, &schedules);

        Ok(counter)
    }

    pub fn modify_premium_schedule(
        env: Env,
        owner: Address,
        schedule_id: u32,
        next_due: u64,
        interval: u64,
    ) -> Result<(), InsuranceError> {
        owner.require_auth();
        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DATA_KEY_SCHEDULES)
            .ok_or(InsuranceError::ScheduleNotFound)?;
        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::ScheduleNotFound)?;

        if schedule.owner != owner {
            return Err(InsuranceError::Unauthorized);
        }

        schedule.next_due = next_due;
        schedule.interval = interval;
        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&DATA_KEY_SCHEDULES, &schedules);
        Ok(())
    }

    pub fn cancel_premium_schedule(
        env: Env,
        owner: Address,
        schedule_id: u32,
    ) -> Result<(), InsuranceError> {
        owner.require_auth();
        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DATA_KEY_SCHEDULES)
            .ok_or(InsuranceError::ScheduleNotFound)?;
        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::ScheduleNotFound)?;

        if schedule.owner != owner {
            return Err(InsuranceError::Unauthorized);
        }

        schedule.active = false;
        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&DATA_KEY_SCHEDULES, &schedules);
        Ok(())
    }

    pub fn get_premium_schedule(env: Env, id: u32) -> Option<PremiumSchedule> {
        let schedules: Map<u32, PremiumSchedule> =
            env.storage().instance().get(&DATA_KEY_SCHEDULES)?;
        schedules.get(id)
    }

    pub fn execute_due_premium_schedules(env: Env) -> Vec<u32> {
        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&DATA_KEY_SCHEDULES)
            .unwrap_or_else(|| Map::new(&env));
        let mut executed = Vec::new(&env);
        let now = env.ledger().timestamp();

        let mut updated_schedules = Vec::new(&env);

        for (id, mut schedule) in schedules.iter() {
            if schedule.active && schedule.next_due <= now {
                if let Ok(true) =
                    Self::internal_pay_premium(&env, schedule.owner.clone(), schedule.policy_id)
                {
                    executed.push_back(id);
                    if schedule.interval > 0 {
                        schedule.next_due += schedule.interval;
                    } else {
                        schedule.active = false;
                    }
                } else {
                    schedule.missed_count += 1;
                    if schedule.interval > 0 {
                        schedule.next_due += schedule.interval;
                    } else {
                        schedule.active = false;
                    }
                }
                updated_schedules.push_back((id, schedule));
            }
        }

        for (id, schedule) in updated_schedules.iter() {
            schedules.set(id, schedule);
        }
        env.storage()
            .instance()
            .set(&DATA_KEY_SCHEDULES, &schedules);

        executed
    }

    fn require_initialized(env: &Env) -> Result<(), InsuranceError> {
        if !env.storage().instance().has(&DATA_KEY_OWNER) {
            Err(InsuranceError::NotInitialized)
        } else {
            Ok(())
        }
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}
