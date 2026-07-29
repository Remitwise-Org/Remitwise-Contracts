import re
import os

repo_dir = r"c:\Users\Dell\OneDrive\Documents\Drip\Remitwise-Contracts\reporting\src"
lib_path = os.path.join(repo_dir, "lib.rs")

with open(lib_path, "r", encoding="utf-8") as f:
    lib_code = f.read()

# Add DataKey, MAX_VIEWERS_PER_USER
lib_code = lib_code.replace(
    'pub enum ReportingError {',
    '''#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Viewers(Address),
}

const MAX_VIEWERS_PER_USER: u32 = 5;

/// Events emitted by the reporting contract
#[contracttype]
#[derive(Clone, Copy)]
pub enum ReportingError {'''
)

lib_code = lib_code.replace(
    'AddressesNotConfigured = 4,',
    'AddressesNotConfigured = 4,\n    MaxViewersReached = 5,'
)

# Add MaxViewersReached mapping
lib_code = lib_code.replace(
    '''            ReportingError::AddressesNotConfigured => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::MissingValue,
            )),
        }''',
    '''            ReportingError::AddressesNotConfigured => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::MissingValue,
            )),
            ReportingError::MaxViewersReached => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidAction,
            )),
        }'''
)

# Add access functions and verify_read_access
lib_code = lib_code.replace(
    '''#[contractimpl]
impl ReportingContract {
    /// Initialize the reporting contract with an admin address.''',
    '''#[contractimpl]
impl ReportingContract {
    fn verify_read_access(env: &Env, caller: &Address, user: &Address) {
        caller.require_auth();
        if caller != user {
            let viewers: Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::Viewers(user.clone()))
                .unwrap_or_else(|| Vec::new(env));
            
            if !viewers.contains(caller) {
                panic!("unauthorized viewer");
            }
        }
    }

    pub fn grant_viewer(env: Env, user: Address, viewer: Address) -> Result<(), ReportingError> {
        user.require_auth();
        
        let mut viewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Viewers(user.clone()))
            .unwrap_or_else(|| Vec::new(&env));
            
        if !viewers.contains(&viewer) {
            if viewers.len() >= MAX_VIEWERS_PER_USER {
                return Err(ReportingError::MaxViewersReached);
            }
            viewers.push_back(viewer.clone());
            env.storage().persistent().set(&DataKey::Viewers(user), &viewers);
        }
        
        Ok(())
    }

    pub fn revoke_viewer(env: Env, user: Address, viewer: Address) -> Result<(), ReportingError> {
        user.require_auth();
        
        let mut viewers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Viewers(user.clone()))
            .unwrap_or_else(|| Vec::new(&env));
            
        if let Some(pos) = viewers.first_index_of(&viewer) {
            viewers.remove(pos);
            env.storage().persistent().set(&DataKey::Viewers(user), &viewers);
        }
        
        Ok(())
    }

    /// Initialize the reporting contract with an admin address.'''
)

# Add caller to read APIs
def replace_func(name, old_args, new_args, verify_str):
    global lib_code
    pattern = r"pub fn " + name + r"\s*\(\s*" + old_args.replace("(", r"\(").replace(")", r"\)") + r"\s*"
    repl = f"pub fn {name}(\n{new_args}\n" + verify_str
    lib_code = re.sub(pattern, repl, lib_code, count=1)

func_list = [
    ("get_remittance_summary", "env: Env,\n        _user: Address,", "        env: Env,\n        caller: Address,\n        user: Address,", ") -> Result<RemittanceSummary, ReportingError> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("get_savings_report", "env: Env,\n        user: Address,", "        env: Env,\n        caller: Address,\n        user: Address,", ") -> Result<SavingsReport, ReportingError> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("get_bill_compliance_report", "env: Env,\n        user: Address,", "        env: Env,\n        caller: Address,\n        user: Address,", ") -> Result<BillComplianceReport, ReportingError> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("get_insurance_report", "env: Env,\n        user: Address,", "        env: Env,\n        caller: Address,\n        user: Address,", ") -> Result<InsuranceReport, ReportingError> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("calculate_health_score", "env: Env, user: Address, _total_remittance: i128", "        env: Env, caller: Address, user: Address, _total_remittance: i128", ") -> Result<HealthScore, ReportingError> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("get_financial_health_report", "env: Env,\n        user: Address,", "        env: Env,\n        caller: Address,\n        user: Address,", ") -> Result<FinancialHealthReport, ReportingError> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("get_trend_analysis", "_env: Env,\n        _user: Address,", "        _env: Env,\n        caller: Address,\n        user: Address,", ") -> TrendData {\n        Self::verify_read_access(&_env, &caller, &user);\n"),
    ("get_stored_report", "env: Env,\n        user: Address,", "        env: Env,\n        caller: Address,\n        user: Address,", ") -> Option<FinancialHealthReport> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
    ("get_archived_reports", "env: Env, user: Address", "        env: Env, caller: Address, user: Address", ") -> Vec<ArchivedReport> {\n        Self::verify_read_access(&env, &caller, &user);\n"),
]

for name, old_args, new_args, verify_str in func_list:
    lib_code = re.sub(
        r"pub fn " + name + r"\s*\(\s*" + old_args.replace("(", r"\(").replace(")", r"\)") + r"(.*?\))\s*(->.*?\{)",
        r"pub fn " + name + r"(\n" + new_args + r"\1 \2\n" + verify_str,
        lib_code, flags=re.DOTALL
    )

# Fix inner calls in get_financial_health_report
lib_code = lib_code.replace(
    'Self::calculate_health_score(env.clone(), user.clone(), total_remittance)?',
    'Self::calculate_health_score(env.clone(), caller.clone(), user.clone(), total_remittance)?'
)
lib_code = lib_code.replace(
    'Self::get_remittance_summary(\n            env.clone(),\n            user.clone(),',
    'Self::get_remittance_summary(\n            env.clone(),\n            caller.clone(),\n            user.clone(),'
)
lib_code = lib_code.replace(
    'Self::get_savings_report(env.clone(), user.clone(),',
    'Self::get_savings_report(env.clone(), caller.clone(), user.clone(),'
)
lib_code = lib_code.replace(
    'Self::get_bill_compliance_report(env.clone(), user.clone(),',
    'Self::get_bill_compliance_report(env.clone(), caller.clone(), user.clone(),'
)
lib_code = lib_code.replace(
    'Self::get_insurance_report(env.clone(), user,',
    'Self::get_insurance_report(env.clone(), caller, user,'
)

with open(lib_path, "w", encoding="utf-8") as f:
    f.write(lib_code)

print("lib.rs modified successfully.")

# Now update tests.rs
tests_path = os.path.join(repo_dir, "tests.rs")
with open(tests_path, "r", encoding="utf-8") as f:
    test_code = f.read()

# Replace test calls: try_get_remittance_summary(&user, ...) -> try_get_remittance_summary(&user, &user, ...)
replace_patterns = [
    (r"try_get_remittance_summary\(&user,", r"try_get_remittance_summary(&user, &user,"),
    (r"try_get_savings_report\(&user,", r"try_get_savings_report(&user, &user,"),
    (r"try_get_bill_compliance_report\(&user,", r"try_get_bill_compliance_report(&user, &user,"),
    (r"try_get_insurance_report\(&user,", r"try_get_insurance_report(&user, &user,"),
    (r"try_calculate_health_score\(&user,", r"try_calculate_health_score(&user, &user,"),
    (r"try_get_financial_health_report\(&user,", r"try_get_financial_health_report(&user, &user,"),
    (r"get_trend_analysis\(&user,", r"get_trend_analysis(&user, &user,"),
    (r"get_stored_report\(&user,", r"get_stored_report(&user, &user,"),
    (r"get_archived_reports\(&user\)", r"get_archived_reports(&user, &user)"),
]

for p, r in replace_patterns:
    test_code = re.sub(p, r, test_code)

# Add tests for ACL functionality
acl_tests = """
#[test]
fn test_acl_delegation() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 1, 1704067200);
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let viewer = Address::generate(&env);

    client.init(&admin);

    let remittance_split_id = env.register_contract(None, remittance_split::RemittanceSplit);
    let savings_goals_id = env.register_contract(None, savings_goals::SavingsGoalsContract);
    let bill_payments_id = env.register_contract(None, bill_payments::BillPayments);
    let insurance_id = env.register_contract(None, insurance::Insurance);
    let family_wallet = Address::generate(&env);

    client.configure_addresses(
        &admin,
        &remittance_split_id,
        &savings_goals_id,
        &bill_payments_id,
        &insurance_id,
        &family_wallet,
    );

    // Default: viewer cannot read
    let res = client.try_get_savings_report(&viewer, &user, &1704067200u64, &1706745600u64);
    assert!(res.is_err(), "Viewer without ACL should fail");

    // Grant viewer
    client.grant_viewer(&user, &viewer);

    // Viewer can read
    let res = client.try_get_savings_report(&viewer, &user, &1704067200u64, &1706745600u64);
    assert!(res.is_ok(), "Viewer with ACL should succeed");

    // Revoke viewer
    client.revoke_viewer(&user, &viewer);

    // Viewer cannot read
    let res = client.try_get_savings_report(&viewer, &user, &1704067200u64, &1706745600u64);
    assert!(res.is_err(), "Viewer after revoke should fail");
}
"""

test_code += acl_tests

with open(tests_path, "w", encoding="utf-8") as f:
    f.write(test_code)

print("tests.rs modified successfully.")
