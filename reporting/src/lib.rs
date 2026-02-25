#![no_std]
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, Address, Env, Map, Vec,
};

// Storage TTL constants for active data
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

// Storage TTL constants for archived data (longer retention, less frequent access)
const ARCHIVE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const ARCHIVE_BUMP_AMOUNT: u32 = 2592000; // ~180 days (6 months)

/// Category for financial breakdown
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Category {
    Spending = 1,
    Savings = 2,
    Bills = 3,
    Insurance = 4,
}

/// Financial health score (0-100)
#[contracttype]
#[derive(Clone)]
pub struct HealthScore {
    pub score: u32,
    pub savings_score: u32,
    pub bills_score: u32,
    pub insurance_score: u32,
}

/// Category breakdown with amount and percentage
#[contracttype]
#[derive(Clone)]
pub struct CategoryBreakdown {
    pub category: Category,
    pub amount: i128,
    pub percentage: u32,
}

/// Trend data comparing two periods
#[contracttype]
#[derive(Clone)]
pub struct TrendData {
    pub current_amount: i128,
    pub previous_amount: i128,
    pub change_amount: i128,
    pub change_percentage: i32, // Can be negative
}

/// Remittance summary report
#[contracttype]
#[derive(Clone)]
pub struct RemittanceSummary {
    pub total_received: i128,
    pub total_allocated: i128,
    pub category_breakdown: Vec<CategoryBreakdown>,
    pub period_start: u64,
    pub period_end: u64,
}

/// Savings progress report
#[contracttype]
#[derive(Clone)]
pub struct SavingsReport {
    pub total_goals: u32,
    pub completed_goals: u32,
    pub total_target: i128,
    pub total_saved: i128,
    pub completion_percentage: u32,
    pub period_start: u64,
    pub period_end: u64,
}

/// Bill payment compliance report
#[contracttype]
#[derive(Clone)]
pub struct BillComplianceReport {
    pub total_bills: u32,
    pub paid_bills: u32,
    pub unpaid_bills: u32,
    pub overdue_bills: u32,
    pub total_amount: i128,
    pub paid_amount: i128,
    pub unpaid_amount: i128,
    pub compliance_percentage: u32,
    pub period_start: u64,
    pub period_end: u64,
}

/// Insurance coverage report
#[contracttype]
#[derive(Clone)]
pub struct InsuranceReport {
    pub active_policies: u32,
    pub total_coverage: i128,
    pub monthly_premium: i128,
    pub annual_premium: i128,
    pub coverage_to_premium_ratio: u32,
    pub period_start: u64,
    pub period_end: u64,
}

/// Family spending report
#[contracttype]
#[derive(Clone)]
pub struct FamilySpendingReport {
    pub total_members: u32,
    pub total_spending: i128,
    pub average_per_member: i128,
    pub period_start: u64,
    pub period_end: u64,
}

/// Overall financial health report
#[contracttype]
#[derive(Clone)]
pub struct FinancialHealthReport {
    pub health_score: HealthScore,
    pub remittance_summary: RemittanceSummary,
    pub savings_report: SavingsReport,
    pub bill_compliance: BillComplianceReport,
    pub insurance_report: InsuranceReport,
    pub generated_at: u64,
}

/// Contract addresses configuration
#[contracttype]
#[derive(Clone)]
pub struct ContractAddresses {
    pub remittance_split: Address,
    pub savings_goals: Address,
    pub bill_payments: Address,
    pub insurance: Address,
    pub family_wallet: Address,
}

/// Events emitted by the reporting contract
#[contracttype]
#[derive(Clone, Copy)]
pub enum ReportingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AddressesNotConfigured = 4,
}

impl From<ReportingError> for soroban_sdk::Error {
    fn from(err: ReportingError) -> Self {
        match err {
            ReportingError::AlreadyInitialized => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidAction,
            )),
            ReportingError::NotInitialized => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::MissingValue,
            )),
            ReportingError::Unauthorized => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::InvalidAction,
            )),
            ReportingError::AddressesNotConfigured => soroban_sdk::Error::from((
                soroban_sdk::xdr::ScErrorType::Contract,
                soroban_sdk::xdr::ScErrorCode::MissingValue,
            )),
        }
    }
}

impl From<&ReportingError> for soroban_sdk::Error {
    fn from(err: &ReportingError) -> Self {
        (*err).into()
    }
}

impl From<soroban_sdk::Error> for ReportingError {
    fn from(_err: soroban_sdk::Error) -> Self {
        ReportingError::Unauthorized
    }
}

#[contracttype]
#[derive(Clone)]
pub enum ReportEvent {
    ReportGenerated,
    ReportStored,
    AddressesConfigured,
    ReportsArchived,
    ArchivesCleaned,
}

/// Archived report - compressed summary
#[contracttype]
#[derive(Clone)]
pub struct ArchivedReport {
    pub user: Address,
    pub period_key: u64,
    pub health_score: u32,
    pub generated_at: u64,
    pub archived_at: u64,
}

/// Storage statistics for monitoring
#[contracttype]
#[derive(Clone)]
pub struct StorageStats {
    pub active_reports: u32,
    pub archived_reports: u32,
    pub last_updated: u64,
}

// Client traits for cross-contract calls

#[contractclient(name = "RemittanceSplitClient")]
pub trait RemittanceSplitTrait {
    fn get_split(env: &Env) -> Vec<u32>;
    fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>;
}

#[contractclient(name = "SavingsGoalsClient")]
pub trait SavingsGoalsTrait {
    fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal>;
    fn is_goal_completed(env: Env, goal_id: u32) -> bool;
}

#[contractclient(name = "BillPaymentsClient")]
pub trait BillPaymentsTrait {
    fn get_unpaid_bills(env: Env, owner: Address) -> Vec<Bill>;
    fn get_total_unpaid(env: Env, owner: Address) -> i128;
    fn get_all_bills(env: Env) -> Vec<Bill>;
}

#[contractclient(name = "InsuranceClient")]
pub trait InsuranceTrait {
    fn get_active_policies(env: Env, owner: Address) -> Vec<InsurancePolicy>;
    fn get_total_monthly_premium(env: Env, owner: Address) -> i128;
}

// Data structures from other contracts (needed for client traits)

#[contracttype]
#[derive(Clone)]
pub struct SavingsGoal {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64,
    pub locked: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
}

#[contracttype]
#[derive(Clone)]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub coverage_type: soroban_sdk::String,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub active: bool,
    pub next_payment_date: u64,
}

#[contract]
pub struct ReportingContract;

#[contractimpl]
impl ReportingContract {
    /// Initialize the reporting contract with an admin address.
    ///
    /// # Arguments
    /// * `admin` - Address of the contract administrator (must authorize)
    ///
    /// # Returns
    /// `Ok(())` on successful initialization
    ///
    /// # Errors
    /// * `AlreadyInitialized` - If the contract has already been initialized
    ///
    /// # Panics
    /// * If `admin` does not authorize the transaction
    pub fn init(env: Env, admin: Address) -> Result<(), ReportingError> {
        admin.require_auth();

        let existing: Option<Address> = env.storage().instance().get(&symbol_short!("ADMIN"));
        if existing.is_some() {
            return Err(ReportingError::AlreadyInitialized);
        }

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &admin);

        Ok(())
    }

    /// Configure addresses for all related contracts (admin only).
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `remittance_split` - Address of the remittance split contract
    /// * `savings_goals` - Address of the savings goals contract
    /// * `bill_payments` - Address of the bill payments contract
    /// * `insurance` - Address of the insurance contract
    /// * `family_wallet` - Address of the family wallet contract
    ///
    /// # Returns
    /// `Ok(())` on successful configuration
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    ///
    /// # Panics
    /// * If `caller` does not authorize the transaction
    pub fn configure_addresses(
        env: Env,
        caller: Address,
        remittance_split: Address,
        savings_goals: Address,
        bill_payments: Address,
        insurance: Address,
        family_wallet: Address,
    ) -> Result<(), ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);

        let addresses = ContractAddresses {
            remittance_split,
            savings_goals,
            bill_payments,
            insurance,
            family_wallet,
        };

        env.storage()
            .instance()
            .set(&symbol_short!("ADDRS"), &addresses);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::AddressesConfigured),
            caller,
        );

        Ok(())
    }

    /// Generate remittance summary report
    pub fn get_remittance_summary(
        env: Env,
        _user: Address,
        total_amount: i128,
        period_start: u64,
        period_end: u64,
    ) -> RemittanceSummary {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .expect("Contract addresses not configured");

        let split_client = RemittanceSplitClient::new(&env, &addresses.remittance_split);
        let split_percentages = split_client.get_split();
        let split_amounts = split_client.calculate_split(&total_amount);

        let mut breakdown = Vec::new(&env);
        let categories = [
            Category::Spending,
            Category::Savings,
            Category::Bills,
            Category::Insurance,
        ];

        for (i, &category) in categories.iter().enumerate() {
            breakdown.push_back(CategoryBreakdown {
                category,
                amount: split_amounts.get(i as u32).unwrap_or(0),
                percentage: split_percentages.get(i as u32).unwrap_or(0),
            });
        }

        RemittanceSummary {
            total_received: total_amount,
            total_allocated: total_amount,
            category_breakdown: breakdown,
            period_start,
            period_end,
        }
    }

    /// Generate savings progress report
    pub fn get_savings_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> SavingsReport {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .expect("Contract addresses not configured");

        let savings_client = SavingsGoalsClient::new(&env, &addresses.savings_goals);
        let goals = savings_client.get_all_goals(&user);

        let mut total_target = 0i128;
        let mut total_saved = 0i128;
        let mut completed_count = 0u32;
        let total_goals = goals.len();

        for goal in goals.iter() {
            total_target += goal.target_amount;
            total_saved += goal.current_amount;
            if goal.current_amount >= goal.target_amount {
                completed_count += 1;
            }
        }

        let completion_percentage = if total_target > 0 {
            ((total_saved * 100) / total_target) as u32
        } else {
            0
        };

        SavingsReport {
            total_goals,
            completed_goals: completed_count,
            total_target,
            total_saved,
            completion_percentage,
            period_start,
            period_end,
        }
    }

    /// Generate bill payment compliance report
    pub fn get_bill_compliance_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> BillComplianceReport {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .expect("Contract addresses not configured");

        let bill_client = BillPaymentsClient::new(&env, &addresses.bill_payments);
        let all_bills = bill_client.get_all_bills();

        let mut total_bills = 0u32;
        let mut paid_bills = 0u32;
        let mut unpaid_bills = 0u32;
        let mut overdue_bills = 0u32;
        let mut total_amount = 0i128;
        let mut paid_amount = 0i128;
        let mut unpaid_amount = 0i128;

        let current_time = env.ledger().timestamp();

        for bill in all_bills.iter() {
            if bill.owner != user {
                continue;
            }

            // Filter by period
            if bill.created_at < period_start || bill.created_at > period_end {
                continue;
            }

            total_bills += 1;
            total_amount += bill.amount;

            if bill.paid {
                paid_bills += 1;
                paid_amount += bill.amount;
            } else {
                unpaid_bills += 1;
                unpaid_amount += bill.amount;
                if bill.due_date < current_time {
                    overdue_bills += 1;
                }
            }
        }

        let compliance_percentage = if total_bills > 0 {
            (paid_bills * 100) / total_bills
        } else {
            100
        };

        BillComplianceReport {
            total_bills,
            paid_bills,
            unpaid_bills,
            overdue_bills,
            total_amount,
            paid_amount,
            unpaid_amount,
            compliance_percentage,
            period_start,
            period_end,
        }
    }

    /// Generate insurance coverage report
    pub fn get_insurance_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> InsuranceReport {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .expect("Contract addresses not configured");

        let insurance_client = InsuranceClient::new(&env, &addresses.insurance);
        let policies = insurance_client.get_active_policies(&user);
        let monthly_premium = insurance_client.get_total_monthly_premium(&user);

        let mut total_coverage = 0i128;
        let active_policies = policies.len();

        for policy in policies.iter() {
            total_coverage += policy.coverage_amount;
        }

        let annual_premium = monthly_premium * 12;
        let coverage_to_premium_ratio = if annual_premium > 0 {
            ((total_coverage * 100) / annual_premium) as u32
        } else {
            0
        };

        InsuranceReport {
            active_policies,
            total_coverage,
            monthly_premium,
            annual_premium,
            coverage_to_premium_ratio,
            period_start,
            period_end,
        }
    }

    /// Calculate financial health score
    pub fn calculate_health_score(env: Env, user: Address, _total_remittance: i128) -> HealthScore {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .expect("Contract addresses not configured");

        // Savings score (0-40 points)
        let savings_client = SavingsGoalsClient::new(&env, &addresses.savings_goals);
        let goals = savings_client.get_all_goals(&user);
        let mut total_target = 0i128;
        let mut total_saved = 0i128;
        for goal in goals.iter() {
            total_target += goal.target_amount;
            total_saved += goal.current_amount;
        }
        let savings_score = if total_target > 0 {
            let progress = ((total_saved * 100) / total_target) as u32;
            if progress > 100 {
                40
            } else {
                (progress * 40) / 100
            }
        } else {
            20 // Default score if no goals
        };

        // Bills score (0-40 points)
        let bill_client = BillPaymentsClient::new(&env, &addresses.bill_payments);
        let unpaid_bills = bill_client.get_unpaid_bills(&user);
        let bills_score = if unpaid_bills.is_empty() {
            40
        } else {
            let overdue_count = unpaid_bills
                .iter()
                .filter(|b| b.due_date < env.ledger().timestamp())
                .count();
            if overdue_count == 0 {
                35 // Has unpaid but none overdue
            } else {
                20 // Has overdue bills
            }
        };

        // Insurance score (0-20 points)
        let insurance_client = InsuranceClient::new(&env, &addresses.insurance);
        let policies = insurance_client.get_active_policies(&user);
        let insurance_score = if !policies.is_empty() { 20 } else { 0 };

        let total_score = savings_score + bills_score + insurance_score;

        HealthScore {
            score: total_score,
            savings_score,
            bills_score,
            insurance_score,
        }
    }

    /// Generate comprehensive financial health report
    pub fn get_financial_health_report(
        env: Env,
        user: Address,
        total_remittance: i128,
        period_start: u64,
        period_end: u64,
    ) -> FinancialHealthReport {
        let health_score =
            Self::calculate_health_score(env.clone(), user.clone(), total_remittance);
        let remittance_summary = Self::get_remittance_summary(
            env.clone(),
            user.clone(),
            total_remittance,
            period_start,
            period_end,
        );
        let savings_report =
            Self::get_savings_report(env.clone(), user.clone(), period_start, period_end);
        let bill_compliance =
            Self::get_bill_compliance_report(env.clone(), user.clone(), period_start, period_end);
        let insurance_report =
            Self::get_insurance_report(env.clone(), user, period_start, period_end);

        let generated_at = env.ledger().timestamp();

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ReportGenerated),
            generated_at,
        );

        FinancialHealthReport {
            health_score,
            remittance_summary,
            savings_report,
            bill_compliance,
            insurance_report,
            generated_at,
        }
    }

    /// Generate trend analysis comparing two periods
    pub fn get_trend_analysis(
        _env: Env,
        _user: Address,
        current_amount: i128,
        previous_amount: i128,
    ) -> TrendData {
        let change_amount = current_amount - previous_amount;
        let change_percentage = if previous_amount > 0 {
            ((change_amount * 100) / previous_amount) as i32
        } else if current_amount > 0 {
            100
        } else {
            0
        };

        TrendData {
            current_amount,
            previous_amount,
            change_amount,
            change_percentage,
        }
    }

    /// Store a financial health report for a user
    pub fn store_report(
        env: Env,
        user: Address,
        report: FinancialHealthReport,
        period_key: u64,
    ) -> bool {
        user.require_auth();

        Self::extend_instance_ttl(&env);

        let mut reports: Map<(Address, u64), FinancialHealthReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("REPORTS"))
            .unwrap_or_else(|| Map::new(&env));

        reports.set((user.clone(), period_key), report);
        env.storage()
            .instance()
            .set(&symbol_short!("REPORTS"), &reports);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ReportStored),
            (user, period_key),
        );

        true
    }

    /// Retrieve a stored report
    pub fn get_stored_report(
        env: Env,
        user: Address,
        period_key: u64,
    ) -> Option<FinancialHealthReport> {
        let reports: Map<(Address, u64), FinancialHealthReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("REPORTS"))
            .unwrap_or_else(|| Map::new(&env));

        reports.get((user, period_key))
    }

    /// Get configured contract addresses
    pub fn get_addresses(env: Env) -> Option<ContractAddresses> {
        env.storage().instance().get(&symbol_short!("ADDRS"))
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("ADMIN"))
    }

    /// Archive old reports before the specified timestamp
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be admin)
    /// * `before_timestamp` - Archive reports generated before this timestamp
    ///
    /// # Returns
    /// Number of reports archived
    pub fn archive_old_reports(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .expect("Contract not initialized");

        if caller != admin {
            panic!("Only admin can archive reports");
        }

        Self::extend_instance_ttl(&env);

        let mut reports: Map<(Address, u64), FinancialHealthReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("REPORTS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut archived: Map<(Address, u64), ArchivedReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_RPT"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut archived_count = 0u32;
        let mut to_remove: Vec<(Address, u64)> = Vec::new(&env);

        for ((user, period_key), report) in reports.iter() {
            if report.generated_at < before_timestamp {
                let archived_report = ArchivedReport {
                    user: user.clone(),
                    period_key,
                    health_score: report.health_score.score,
                    generated_at: report.generated_at,
                    archived_at: current_time,
                };
                archived.set((user.clone(), period_key), archived_report);
                to_remove.push_back((user, period_key));
                archived_count += 1;
            }
        }

        for i in 0..to_remove.len() {
            if let Some(key) = to_remove.get(i) {
                reports.remove(key);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("REPORTS"), &reports);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_RPT"), &archived);

        Self::extend_archive_ttl(&env);
        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ReportsArchived),
            (archived_count, caller),
        );

        archived_count
    }

    /// Get archived reports for a user
    ///
    /// # Arguments
    /// * `user` - Address of the user
    ///
    /// # Returns
    /// Vec of ArchivedReport structs
    pub fn get_archived_reports(env: Env, user: Address) -> Vec<ArchivedReport> {
        let archived: Map<(Address, u64), ArchivedReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_RPT"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for ((addr, _), report) in archived.iter() {
            if addr == user {
                result.push_back(report);
            }
        }
        result
    }

    /// Permanently delete old archives before specified timestamp
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be admin)
    /// * `before_timestamp` - Delete archives created before this timestamp
    ///
    /// # Returns
    /// Number of archives deleted
    pub fn cleanup_old_reports(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .expect("Contract not initialized");

        if caller != admin {
            panic!("Only admin can cleanup reports");
        }

        Self::extend_instance_ttl(&env);

        let mut archived: Map<(Address, u64), ArchivedReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_RPT"))
            .unwrap_or_else(|| Map::new(&env));

        let mut deleted_count = 0u32;
        let mut to_remove: Vec<(Address, u64)> = Vec::new(&env);

        for ((user, period_key), report) in archived.iter() {
            if report.archived_at < before_timestamp {
                to_remove.push_back((user, period_key));
                deleted_count += 1;
            }
        }

        for i in 0..to_remove.len() {
            if let Some(key) = to_remove.get(i) {
                archived.remove(key);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_RPT"), &archived);

        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ArchivesCleaned),
            (deleted_count, caller),
        );

        deleted_count
    }

    /// Get storage usage statistics
    ///
    /// # Returns
    /// StorageStats struct with current storage metrics
    pub fn get_storage_stats(env: Env) -> StorageStats {
        env.storage()
            .instance()
            .get(&symbol_short!("STOR_STAT"))
            .unwrap_or(StorageStats {
                active_reports: 0,
                archived_reports: 0,
                last_updated: 0,
            })
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Extend the TTL of archive storage with longer duration
    fn extend_archive_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
    }

    /// Update storage statistics
    fn update_storage_stats(env: &Env) {
        let reports: Map<(Address, u64), FinancialHealthReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("REPORTS"))
            .unwrap_or_else(|| Map::new(env));

        let archived: Map<(Address, u64), ArchivedReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_RPT"))
            .unwrap_or_else(|| Map::new(env));

        let mut active_count = 0u32;
        for _ in reports.iter() {
            active_count += 1;
        }

        let mut archived_count = 0u32;
        for _ in archived.iter() {
            archived_count += 1;
        }

        let stats = StorageStats {
            active_reports: active_count,
            archived_reports: archived_count,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&symbol_short!("STOR_STAT"), &stats);
    }
}

#[cfg(test)]
mod tests;
