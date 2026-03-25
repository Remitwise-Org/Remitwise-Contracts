const fs = require('fs');
const path = require('path');

const libPath = path.join('c:', 'Users', 'HP', 'Desktop', 'Northbridge', 'Remitwise-Contracts', 'insurance', 'src', 'lib.rs');

let content = fs.readFileSync(libPath, 'utf8');

// 1. Tag management duplicates
const tagMgmtStr = '    // Tag management';
let tagIdx1 = content.indexOf(tagMgmtStr);
if (tagIdx1 !== -1) {
    let tagIdx2 = content.indexOf(tagMgmtStr, tagIdx1 + 1);
    if (tagIdx2 !== -1) {
        let coreStr = '    // Core policy operations';
        let coreIdx = content.indexOf(coreStr, tagIdx2);
        if (coreIdx !== -1) {
            let dashes = content.lastIndexOf('    // -----------------------------------------------------------------------', coreIdx);
            if (dashes !== -1 && dashes > tagIdx2) {
                content = content.substring(0, tagIdx2) + content.substring(dashes);
            }
        }
    }
}

// Fixed replacements
function rep(search, replaceStr) {
    content = content.replace(search, replaceStr);
}

rep(`        external_ref: Option<String>,
    ) -> u32 {
    ) -> Result<u32, InsuranceError> {`, 
`        external_ref: Option<String>,
    ) -> Result<u32, InsuranceError> {`);

rep(`        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner, policy_external_ref),
            (next_id, owner),
        );`,
`        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner, policy_external_ref),
        );`);

rep(`        policies.set(policy_id, policy);
        policies.set(policy_id, policy.clone());`,
`        policies.set(policy_id, policy.clone());`);

rep(`        env.events().publish(
            (PREMIUM_PAID,),
            PremiumPaidEvent {
                policy_id,
                name: policy.name,
                amount: policy.monthly_premium,
                next_payment_date: policy.next_payment_date,
                timestamp: env.ledger().timestamp(),
            },
        );

        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
            (policy_id, caller, policy_external_ref),
        );`,
`        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
            (policy_id, caller, policy_external_ref),
        );`);

rep(`        for id in policy_ids.iter() {
            let mut policy = policies.get(id).unwrap_or_else(|| panic!("Policy not found"));
            let mut policy = policies_map.get(id).unwrap();`,
`        for id in policy_ids.iter() {
            let mut policy = policies_map.get(id).unwrap();`);

rep(`        let mut policy = policies.get(policy_id).unwrap_or_else(|| panic!("Policy not found"));
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;`,
`        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;`);

rep(`        let policy_external_ref = policy.external_ref.clone();
        policies.set(policy_id, policy);
        let premium_amount = policy.monthly_premium;
        policies.set(policy_id, policy.clone());`,
`        let policy_external_ref = policy.external_ref.clone();
        let premium_amount = policy.monthly_premium;
        policies.set(policy_id, policy.clone());`);

rep(`        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ExternalRefUpdated),
            (policy_id, caller, external_ref),
            (symbol_short!("insuranc"), InsuranceEvent::PolicyDeactivated),
            (policy_id, caller),
        );`,
`        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ExternalRefUpdated),
            (policy_id, caller, external_ref),
        );`);

// the create_premium_schedule replacement
const searchStart = `        Self::require_not_paused(&env, pause_functions::CREATE_SCHED)?;`;
const searchEnd = `        schedules.set(next_schedule_id, schedule);`;
const properBody = `        Self::require_not_paused(&env, pause_functions::CREATE_SCHED)?;

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(InsuranceError::InvalidTimestamp);
        }

        Self::extend_instance_ttl(&env);

        let mut policies: Map<u32, InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Map::new(&env));

        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;

        if policy.owner != owner {
            return Err(InsuranceError::Unauthorized);
        }

        let mut schedules: Map<u32, PremiumSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let next_schedule_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_PSCH"))
            .unwrap_or(0u32)
            + 1;

        let schedule = PremiumSchedule {
            id: next_schedule_id,
            owner: owner.clone(),
            policy_id,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: current_time,
            last_executed: None,
            missed_count: 0,
        };

        policy.schedule_id = Some(next_schedule_id);

`;

let mStart = content.indexOf(searchStart);
if (mStart !== -1) {
    let mEnd = content.indexOf(searchEnd, mStart);
    if (mEnd !== -1) {
        content = content.substring(0, mStart) + properBody + content.substring(mEnd);
    }
}

rep(`        let mut schedule = schedules.get(schedule_id).unwrap_or_else(|| panic!("Schedule not found"));
        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PolicyNotFound)?;`,
`        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PolicyNotFound)?;`);

fs.writeFileSync(libPath, content, 'utf8');
console.log("Replacements done!");
