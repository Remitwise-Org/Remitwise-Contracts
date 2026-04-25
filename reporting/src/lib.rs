#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    Env, Map, Vec,
};

pub use remitwise_common::{Category, CoverageType};

// Storage TTL constants
const DAY_IN_LEDGERS: u32 = 17280;

pub const PERSISTENT_BUMP_AMOUNT: u32 = 60 * DAY_IN_LEDGERS; // 60 days
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 15 * DAY_IN_LEDGERS; // 15 days

pub const INSTANCE_BUMP_AMOUNT: u32 = PERSISTENT_BUMP_AMOUNT;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = PERSISTENT_LIFETIME_THRESHOLD;

pub const ARCHIVE_BUMP_AMOUNT: u32 = 150 * DAY_IN_LEDGERS; // ~150 days
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = 1 * DAY_IN_LEDGERS; // 1 day

/// Maximum number of pages fetched from any single dependency per report call.
/// Loops that reach this cap mark the result `DataAvailability::Partial` so
/// callers know the aggregate may be incomplete.
pub const MAX_DEP_PAGES: u32 = 20;

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

/// Indicates the completeness of the data retrieved from external contracts
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataAvailability {
    /// All external calls succeeded and data is complete
    Complete = 0,
    /// Some external calls failed or returned partial data
    Partial = 1,
    /// Critical external calls failed or addresses not configured, data is missing/default
    Missing = 2,
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
    pub data_availability: DataAvailability,
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
    pub data_availability: DataAvailability,
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
    pub data_availability: DataAvailability,
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

/// Errors returned by the reporting contract (`Result` arms and `try_` client helpers).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReportingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AddressesNotConfigured = 4,
    NotAdminProposed = 5,
    /// Dependency address set is not usable: duplicates or self-reference to this reporting contract.
    InvalidDependencyAddressConfiguration = 6,
    /// Report period range is invalid (`period_start` is greater than `period_end`).
    InvalidPeriod = 7,
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

/// Dependency health status for monitoring
#[contracttype]
#[derive(Clone)]
pub struct DependencyStatus {
    pub name: soroban_sdk::String,
    pub ok: bool,
    pub error_category: Option<soroban_sdk::String>,
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
    fn get_unpaid_bills(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage;
    fn get_total_unpaid(env: Env, owner: Address) -> i128;
    fn get_all_bills_for_owner(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage;
}

#[contractclient(name = "InsuranceClient")]
pub trait InsuranceTrait {
    fn get_active_policies(env: Env, owner: Address, cursor: u32, limit: u32) -> PolicyPage;
    fn get_total_monthly_premium(env: Env, owner: Address) -> i128;
}

#[contractclient(name = "FamilyWalletClient")]
pub trait FamilyWalletTrait {
    fn get_owner(env: Env) -> Address;
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
    pub unlock_date: Option<u64>,
}

#[contracttype]
#[derive(Clone)]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub external_ref: Option<soroban_sdk::String>,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
    pub schedule_id: Option<u32>,
    pub tags: Vec<soroban_sdk::String>,
    pub currency: soroban_sdk::String,
}

#[contracttype]
#[derive(Clone)]
pub struct BillPage {
    pub items: Vec<Bill>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub external_ref: Option<soroban_sdk::String>,
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
pub struct ReportingContract;

#[contractimpl]
impl ReportingContract {
    // ---------------------------------------------------------------------
    // Dependency address integrity
    // ---------------------------------------------------------------------

    /// Validates the five downstream contract addresses before they are persisted or used.
    ///
    /// # Security assumptions
    ///
    /// - **Self-reference**: No slot may equal [`Env::current_contract_address`]. Routing a role
    ///   back to this reporting contract would make cross-contract calls ambiguous and can break
    ///   tooling that assumes unique callees.
    /// - **Pairwise uniqueness**: Each of `remittance_split`, `savings_goals`, `bill_payments`,
    ///   `insurance`, and `family_wallet` must refer to a **different** contract ID. Duplicate IDs
    ///   mean two logical roles silently talk to the same deployment (data integrity / audit risk).
    /// Complexity: constant time (five slots, fixed number of equality checks).
    fn validate_dependency_address_set(
        env: &Env,
        addrs: &ContractAddresses,
    ) -> Result<(), ReportingError> {
        let reporting = env.current_contract_address();
        let slots = [
            &addrs.remittance_split,
            &addrs.savings_goals,
            &addrs.bill_payments,
            &addrs.insurance,
            &addrs.family_wallet,
        ];

        for slot in slots {
            if *slot == reporting {
                return Err(ReportingError::InvalidDependencyAddressConfiguration);
            }
        }

        for i in 0..slots.len() {
            for j in (i + 1)..slots.len() {
                if *slots[i] == *slots[j] {
                    return Err(ReportingError::InvalidDependencyAddressConfiguration);
                }
            }
        }

        Ok(())
    }

    /// Validates that a requested report period is logically ordered.
    fn validate_period(period_start: u64, period_end: u64) -> Result<(), ReportingError> {
        if period_start > period_end {
            return Err(ReportingError::InvalidPeriod);
        }
        Ok(())
    }

    /// Verify a [`ContractAddresses`] bundle using the same rules as [`ReportingContract::configure_addresses`].
    ///
    /// Does **not** write storage and does **not** require authorization. Intended for admin UIs and
    /// offline checks before submitting a configuration transaction.
    ///
    /// # Errors
    ///
    /// * [`ReportingError::InvalidDependencyAddressConfiguration`] — duplicates or self-reference.
    pub fn verify_dependency_address_set(
        env: Env,
        addrs: ContractAddresses,
    ) -> Result<(), ReportingError> {
        Self::validate_dependency_address_set(&env, &addrs)
    }

    /// Initialize the reporting contract with an admin address.
    ///
    /// This function must be called only once. The provided admin address will
    /// have full control over contract configuration and maintenance.
    ///
    /// # Arguments
    /// * `admin` - Address of the initial contract administrator
    ///
    /// # Returns
    /// `Ok(())` on successful initialization
    ///
    /// # Errors
    /// * `AlreadyInitialized` - If the contract has already been initialized
    pub fn init(env: Env, admin: Address) -> Result<(), ReportingError> {
        let existing: Option<Address> = env.storage().instance().get(&symbol_short!("ADMIN"));
        if existing.is_some() {
            return Err(ReportingError::AlreadyInitialized);
        }

        admin.require_auth();

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &admin);

        Ok(())
    }

    /// Propose a new administrator for the contract.
    ///
    /// This is the first step of a two-step admin rotation process. The proposed
    /// admin must then call `accept_admin_rotation` to complete the transfer.
    ///
    /// # Arguments
    /// * `caller` - Current administrator (must authorize)
    /// * `new_admin` - Address of the proposed successor
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the current admin
    pub fn propose_new_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
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
        env.storage()
            .instance()
            .set(&symbol_short!("PEND_ADM"), &new_admin);

        Ok(())
    }

    /// Accept the role of contract administrator.
    ///
    /// This is the second step of a two-step admin rotation process. Only the
    /// address currently proposed via `propose_new_admin` can call this.
    ///
    /// # Arguments
    /// * `caller` - The proposed administrator (must authorize)
    ///
    /// # Errors
    /// * `NotAdminProposed` - If no admin rotation is currently in progress
    /// * `Unauthorized` - If caller is not the proposed admin
    pub fn accept_admin_rotation(env: Env, caller: Address) -> Result<(), ReportingError> {
        caller.require_auth();

        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_ADM"))
            .ok_or(ReportingError::NotAdminProposed)?;

        if caller != pending_admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &pending_admin);
        env.storage().instance().remove(&symbol_short!("PEND_ADM"));

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
    /// * [`ReportingError::InvalidDependencyAddressConfiguration`] - Duplicate addresses or
    ///   self-reference (this reporting contract used as a dependency).
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

        Self::validate_dependency_address_set(&env, &addresses)?;

        env.storage()
            .instance()
            .set(&symbol_short!("ADDRS"), &addresses);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::AddressesConfigured),
            caller,
        );

        Ok(())
    }

    /// Check health of all configured dependencies (admin only).
    ///
    /// Performs minimal try_* calls against each configured contract to verify
    /// they are responsive and properly configured. Returns a status list for
    /// monitoring and debugging.
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    ///
    /// # Returns
    /// Vec of DependencyStatus for each configured contract
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    /// * `AddressesNotConfigured` - If dependency addresses have not been configured
    pub fn check_dependencies(
        env: Env,
        caller: Address,
    ) -> Result<Vec<DependencyStatus>, ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let mut statuses = Vec::new(&env);

        // Check remittance_split
        let split_client = RemittanceSplitClient::new(&env, &addresses.remittance_split);
        let split_ok = match split_client.try_get_split() {
            Ok(Ok(_)) => true,
            _ => false,
        };
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "remittance_split"),
            ok: split_ok,
            error_category: if split_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(&env, "get_split_failed"))
            },
        });

        // Check savings_goals
        let savings_client = SavingsGoalsClient::new(&env, &addresses.savings_goals);
        let savings_ok = match savings_client.try_get_all_goals(&env.current_contract_address()) {
            Ok(Ok(_)) => true,
            _ => false,
        };
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "savings_goals"),
            ok: savings_ok,
            error_category: if savings_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(&env, "get_all_goals_failed"))
            },
        });

        // Check bill_payments
        let bill_client = BillPaymentsClient::new(&env, &addresses.bill_payments);
        let bill_ok = match bill_client.try_get_total_unpaid(&env.current_contract_address()) {
            Ok(Ok(_)) => true,
            _ => false,
        };
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "bill_payments"),
            ok: bill_ok,
            error_category: if bill_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(
                    &env,
                    "get_total_unpaid_failed",
                ))
            },
        });

        // Check insurance
        let insurance_client = InsuranceClient::new(&env, &addresses.insurance);
        let insurance_ok =
            match insurance_client.try_get_total_monthly_premium(&env.current_contract_address()) {
                Ok(Ok(_)) => true,
                _ => false,
            };
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "insurance"),
            ok: insurance_ok,
            error_category: if insurance_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(
                    &env,
                    "get_total_monthly_premium_failed",
                ))
            },
        });

        // Check family_wallet
        let family_client = FamilyWalletClient::new(&env, &addresses.family_wallet);
        let family_ok = match family_client.try_get_owner() {
            Ok(Ok(_)) => true,
            _ => false,
        };
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "family_wallet"),
            ok: family_ok,
            error_category: if family_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(&env, "get_owner_failed"))
            },
        });

        Ok(statuses)
    }

    /// Generate remittance summary report.
    ///
    /// Fetches split configuration and calculates amounts for a specific period.
    pub fn get_remittance_summary(
        env: Env,
        user: Address,
        total_amount: i128,
        period_start: u64,
        period_end: u64,
    ) -> Result<RemittanceSummary, ReportingError> {
        Self::validate_period(period_start, period_end)?;
        user.require_auth();
        Ok(Self::get_remittance_summary_internal(
            &env,
            total_amount,
            period_start,
            period_end,
        ))
    }

    fn get_remittance_summary_internal(
        env: &Env,
        total_amount: i128,
        period_start: u64,
        period_end: u64,
    ) -> RemittanceSummary {
        let addresses: Option<ContractAddresses> =
            env.storage().instance().get(&symbol_short!("ADDRS"));

        if addresses.is_none() {
            return RemittanceSummary {
                total_received: total_amount,
                total_allocated: total_amount,
                category_breakdown: Vec::new(env),
                period_start,
                period_end,
                data_availability: DataAvailability::Missing,
            };
        }

        let addresses = addresses.unwrap();
        let split_client = RemittanceSplitClient::new(env, &addresses.remittance_split);
        let mut availability = DataAvailability::Complete;

        let split_percentages = match split_client.try_get_split() {
            Ok(Ok(res)) => res,
            _ => {
                availability = DataAvailability::Partial;
                Vec::new(env)
            }
        };

        let split_amounts = match split_client.try_calculate_split(&total_amount) {
            Ok(Ok(res)) => res,
            _ => {
                availability = DataAvailability::Partial;
                Vec::new(env)
            }
        };

        let mut breakdown = Vec::new(env);
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
            data_availability: availability,
        }
    }

    /// Generate savings progress report.
    ///
    /// Aggregates all goals for a user and calculates overall completion progress.
    pub fn get_savings_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<SavingsReport, ReportingError> {
        Self::validate_period(period_start, period_end)?;
        user.require_auth();
        Ok(Self::get_savings_report_internal(
            &env,
            user,
            period_start,
            period_end,
        ))
    }

    fn get_savings_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> SavingsReport {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .unwrap_or_else(|| panic!("Contract addresses not configured"));

        let savings_client = SavingsGoalsClient::new(env, &addresses.savings_goals);
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

    /// Generate bill payment compliance report.
    ///
    /// Analyzes bill statuses and payment deadlines for a specific period.
    pub fn get_bill_compliance_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<BillComplianceReport, ReportingError> {
        Self::validate_period(period_start, period_end)?;
        user.require_auth();
        Ok(Self::get_bill_compliance_report_internal(
            &env,
            user,
            period_start,
            period_end,
        ))
    }

    fn get_bill_compliance_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> BillComplianceReport {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .unwrap_or_else(|| panic!("Contract addresses not configured"));

        let bill_client = BillPaymentsClient::new(env, &addresses.bill_payments);

        let mut total_bills = 0u32;
        let mut paid_bills = 0u32;
        let mut unpaid_bills = 0u32;
        let mut overdue_bills = 0u32;
        let mut total_amount = 0i128;
        let mut paid_amount = 0i128;
        let mut unpaid_amount = 0i128;
        let current_time = env.ledger().timestamp();
        let mut data_availability = DataAvailability::Complete;

        let mut cursor = 0u32;
        let mut pages_fetched = 0u32;
        loop {
            let page = bill_client.get_all_bills_for_owner(&user, &cursor, &50u32);
            for bill in page.items.iter() {
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
            pages_fetched += 1;
            if page.next_cursor == 0 {
                break;
            }
            if pages_fetched >= MAX_DEP_PAGES {
                data_availability = DataAvailability::Partial;
                break;
            }
            cursor = page.next_cursor;
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
            data_availability,
        }
    }

    /// Generate insurance coverage report.
    ///
    /// Summarizes active policies, coverage amounts, and premium ratios.
    pub fn get_insurance_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<InsuranceReport, ReportingError> {
        Self::validate_period(period_start, period_end)?;
        user.require_auth();
        Ok(Self::get_insurance_report_internal(
            &env,
            user,
            period_start,
            period_end,
        ))
    }

    fn get_insurance_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> InsuranceReport {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .unwrap_or_else(|| panic!("Contract addresses not configured"));

        let insurance_client = InsuranceClient::new(env, &addresses.insurance);
        let monthly_premium = insurance_client.get_total_monthly_premium(&user);

        let mut total_coverage = 0i128;
        let mut active_policies = 0u32;
        let mut data_availability = DataAvailability::Complete;

        let mut cursor = 0u32;
        let mut pages_fetched = 0u32;
        loop {
            let page = insurance_client.get_active_policies(&user, &cursor, &50);
            for policy in page.items.iter() {
                active_policies += 1;
                total_coverage += policy.coverage_amount;
            }
            pages_fetched += 1;
            if page.next_cursor == 0 {
                break;
            }
            if pages_fetched >= MAX_DEP_PAGES {
                data_availability = DataAvailability::Partial;
                break;
            }
            cursor = page.next_cursor;
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
            data_availability,
        }
    }

    /// Calculate financial health score
    pub fn calculate_health_score(env: Env, user: Address, total_remittance: i128) -> HealthScore {
        user.require_auth();
        Self::calculate_health_score_internal(&env, user, total_remittance)
    }

    fn calculate_health_score_internal(
        env: &Env,
        user: Address,
        _total_remittance: i128,
    ) -> HealthScore {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .unwrap_or_else(|| panic!("Contract addresses not configured"));

        // Savings score (0-40 points)
        let savings_client = SavingsGoalsClient::new(env, &addresses.savings_goals);
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
        let bill_client = BillPaymentsClient::new(env, &addresses.bill_payments);
        let unpaid_bills = bill_client.get_unpaid_bills(&user, &0u32, &50u32).items;
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
        let insurance_client = InsuranceClient::new(env, &addresses.insurance);
        let policy_page = insurance_client.get_active_policies(&user, &0, &1);
        let insurance_score = if !policy_page.items.is_empty() { 20 } else { 0 };

        let total_score = savings_score + bills_score + insurance_score;

        HealthScore {
            score: total_score,
            savings_score,
            bills_score,
            insurance_score,
        }
    }

    /// Generate comprehensive financial health report combining all metrics.
    ///
    /// This is the primary reporting entry point for users.
    pub fn get_financial_health_report(
        env: Env,
        user: Address,
        total_remittance: i128,
        period_start: u64,
        period_end: u64,
    ) -> Result<FinancialHealthReport, ReportingError> {
        Self::validate_period(period_start, period_end)?;
        user.require_auth();
        let health_score =
            Self::calculate_health_score_internal(&env, user.clone(), total_remittance);
        let remittance_summary =
            Self::get_remittance_summary_internal(&env, total_remittance, period_start, period_end);
        let savings_report =
            Self::get_savings_report_internal(&env, user.clone(), period_start, period_end);
        let bill_compliance =
            Self::get_bill_compliance_report_internal(&env, user.clone(), period_start, period_end);
        let insurance_report =
            Self::get_insurance_report_internal(&env, user, period_start, period_end);

        let generated_at = env.ledger().timestamp();

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ReportGenerated),
            generated_at,
        );

        Ok(FinancialHealthReport {
            health_score,
            remittance_summary,
            savings_report,
            bill_compliance,
            insurance_report,
            generated_at,
        })
    }

    /// Generate trend analysis comparing two data points.
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

    /// Compute trend analysis over a window of historical data points.
    ///
    /// Aggregates a Vec of (period_key, amount) pairs ordered by period_key and
    /// returns one `TrendData` per adjacent pair, producing deterministic output
    /// for identical inputs regardless of call order or ledger state.
    ///
    /// # Arguments
    /// * `_env`      - Contract environment
    /// * `_user`     - Address of the user (reserved for future auth scoping)
    /// * `history`   - Vec of `(period_key: u64, amount: i128)` tuples, at least 2 elements
    ///
    /// # Returns
    /// `Vec<TrendData>` with `history.len() - 1` elements.  Empty when fewer than
    /// two data points are supplied.
    pub fn get_trend_analysis_multi(
        env: Env,
        user: Address,
        history: Vec<(u64, i128)>,
    ) -> Vec<TrendData> {
        user.require_auth();
        let mut result = Vec::new(&env);
        let len = history.len();
        if len < 2 {
            return result;
        }
        for i in 1..len {
            let (_, prev_amount) = history.get(i - 1).unwrap_or((0, 0));
            let (_, curr_amount) = history.get(i).unwrap_or((0, 0));
            let change_amount = curr_amount - prev_amount;
            let change_percentage = if prev_amount > 0 {
                ((change_amount * 100) / prev_amount) as i32
            } else if curr_amount > 0 {
                100
            } else {
                0
            };
            result.push_back(TrendData {
                current_amount: curr_amount,
                previous_amount: prev_amount,
                change_amount,
                change_percentage,
            });
        }
        result
    }

    /// Store a financial health report for a user (must authorize).
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

        Self::update_storage_stats(&env);

        true
    }

    /// Retrieve a previously stored report.
    pub fn get_stored_report(
        env: Env,
        user: Address,
        period_key: u64,
    ) -> Option<FinancialHealthReport> {
        user.require_auth();
        let reports: Map<(Address, u64), FinancialHealthReport> = env
            .storage()
            .instance()
            .get(&symbol_short!("REPORTS"))
            .unwrap_or_else(|| Map::new(&env));

        reports.get((user, period_key))
    }

    /// Get configured contract addresses.
    pub fn get_addresses(env: Env) -> Option<ContractAddresses> {
        env.storage().instance().get(&symbol_short!("ADDRS"))
    }

    /// Get current administrator address.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("ADMIN"))
    }

    /// Archive old reports before the specified timestamp (admin only).
    ///
    /// Moves report data from the primary `REPORTS` storage to the `ARCH_RPT`
    /// storage, potentially reducing gas costs for active users.
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `before_timestamp` - Archive reports generated before this ledger timestamp
    ///
    /// # Returns
    /// `Ok(u32)` containing the number of reports archived
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    pub fn archive_old_reports(
        env: Env,
        caller: Address,
        before_timestamp: u64,
    ) -> Result<u32, ReportingError> {
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

        Ok(archived_count)
    }

    /// Get archived reports for a user
    ///
    /// # Arguments
    /// * `user` - Address of the user
    ///
    /// # Returns
    /// Vec of ArchivedReport structs
    pub fn get_archived_reports(env: Env, user: Address) -> Vec<ArchivedReport> {
        user.require_auth();
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

    /// Permanently delete old archives before specified timestamp (admin only).
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `before_timestamp` - Delete archives created before this ledger timestamp
    ///
    /// # Returns
    /// `Ok(u32)` containing the number of archives deleted
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    pub fn cleanup_old_reports(
        env: Env,
        caller: Address,
        before_timestamp: u64,
    ) -> Result<u32, ReportingError> {
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

        Ok(deleted_count)
    }

    /// Returns aggregate counts of active and archived reports for observability.
    ///
    /// This is intentionally callable without authentication: it exposes only
    /// non-sensitive counters (not report contents or user-identifying detail).
    /// Callers must not treat these values as authorization signals.
    ///
    /// # Returns
    /// [`StorageStats`] with `active_reports`, `archived_reports`, and `last_updated`
    /// (ledger timestamp when stats were last recomputed).
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
