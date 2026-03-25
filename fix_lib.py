import re
import sys

def main():
    path = r"c:\Users\HP\Desktop\Northbridge\Remitwise-Contracts\insurance\src\lib.rs"
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    # 1. Remove duplicate Tag Management sections.
    # We find the string "// Tag management" and keep only the first occurrence along with its function implementations up to the line before the next "// -----------------------------------------------------------------------".
    # Actually, lines 479 to 678 are exact duplicates. Let's just remove everything from the second "// Tag management" to the "// Core policy operations (unchanged)".
    tag_mgmt_start = content.find("    // Tag management")
    second_tag_mgmt = content.find("    // Tag management", tag_mgmt_start + 1)
    if second_tag_mgmt != -1:
        core_policy = content.find("    // Core policy operations", second_tag_mgmt)
        if core_policy != -1:
            # find the line before core_policy, which is "    // -----------------------------------------------------------------------"
            dashes = content.rfind("    // -----------------------------------------------------------------------", second_tag_mgmt, core_policy)
            if dashes != -1:
                content = content[:second_tag_mgmt] + content[dashes:]

    # Now let's just do targeted string replacements.

    rep_1 = """        external_ref: Option<String>,
    ) -> u32 {
    ) -> Result<u32, InsuranceError> {"""
    new_1 = """        external_ref: Option<String>,
    ) -> Result<u32, InsuranceError> {"""
    content = content.replace(rep_1, new_1)

    rep_2 = """        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner, policy_external_ref),
            (next_id, owner),
        );"""
    new_2 = """        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PolicyCreated),
            (next_id, policy_owner, policy_external_ref),
        );"""
    content = content.replace(rep_2, new_2)

    rep_3 = """        policies.set(policy_id, policy);
        policies.set(policy_id, policy.clone());"""
    new_3 = """        policies.set(policy_id, policy.clone());"""
    content = content.replace(rep_3, new_3)

    rep_4 = """        env.events().publish(
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
        );"""
    new_4 = """        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::PremiumPaid),
            (policy_id, caller, policy_external_ref),
        );"""
    content = content.replace(rep_4, new_4)

    rep_5 = """        for id in policy_ids.iter() {
            let mut policy = policies.get(id).unwrap_or_else(|| panic!("Policy not found"));
            let mut policy = policies_map.get(id).unwrap();"""
    new_5 = """        for id in policy_ids.iter() {
            let mut policy = policies_map.get(id).unwrap();"""
    content = content.replace(rep_5, new_5)

    rep_6 = """        let mut policy = policies.get(policy_id).unwrap_or_else(|| panic!("Policy not found"));
        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;"""
    new_6 = """        let mut policy = policies
            .get(policy_id)
            .ok_or(InsuranceError::PolicyNotFound)?;"""
    content = content.replace(rep_6, new_6)

    rep_7 = """        let policy_external_ref = policy.external_ref.clone();
        policies.set(policy_id, policy);
        let premium_amount = policy.monthly_premium;
        policies.set(policy_id, policy.clone());"""
    new_7 = """        let policy_external_ref = policy.external_ref.clone();
        let premium_amount = policy.monthly_premium;
        policies.set(policy_id, policy.clone());"""
    content = content.replace(rep_7, new_7)

    rep_8 = """        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ExternalRefUpdated),
            (policy_id, caller, external_ref),
            (symbol_short!("insuranc"), InsuranceEvent::PolicyDeactivated),
            (policy_id, caller),
        );"""
    new_8 = """        env.events().publish(
            (symbol_short!("insure"), InsuranceEvent::ExternalRefUpdated),
            (policy_id, caller, external_ref),
        );"""
    content = content.replace(rep_8, new_8)

    # Now for create_premium_schedule. We need to replace the messy body.
    # The messy body spans from `        let name = String::from_str(&env, "Health Insurance");` 
    # up to `    }` before `        schedules.set(next_schedule_id, schedule);`
    
    messy_start_str = """        Self::require_not_paused(&env, pause_functions::CREATE_SCHED)?;

        let name = String::from_str(&env, "Health Insurance");"""
    
    # We will just write a regex or string extraction to replace between `pause_functions::CREATE_SCHED)?;` and `schedules.set(next_schedule_id, schedule);`

    proper_body = """        Self::require_not_paused(&env, pause_functions::CREATE_SCHED)?;

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

"""
    
    m_start = content.find("        Self::require_not_paused(&env, pause_functions::CREATE_SCHED)?;")
    m_end = content.find("        schedules.set(next_schedule_id, schedule);", m_start)
    if m_start != -1 and m_end != -1:
        content = content[:m_start] + proper_body + content[m_end:]

    
    # Fix modify_premium_schedule panic block
    rep_9 = """        let mut schedule = schedules.get(schedule_id).unwrap_or_else(|| panic!("Schedule not found"));
        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PolicyNotFound)?;"""
    new_9 = """        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PremiumError)?;""" # usually it's PolicyNotFound but let's check
    # Wait, let's just use PolicyNotFound as it was in the duplicate code.
    new_9 = """        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(InsuranceError::PolicyNotFound)?;"""
    content = content.replace(rep_9, new_9)
    
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)
    
    print("Replacements done!")

if __name__ == "__main__":
    main()
