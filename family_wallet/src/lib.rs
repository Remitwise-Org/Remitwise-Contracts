#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::TokenClient, Address, Env, Map, Symbol, Vec,
};

use remitwise_common::{
    EventCategory, EventPriority, FamilyRole, RemitwiseEvents, CONTRACT_VERSION,
};

// Storage TTL constants for active data
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;

// Storage TTL constants for archived data
const ARCHIVE_LIFETIME_THRESHOLD: u32 = 17280;
const ARCHIVE_BUMP_AMOUNT: u32 = 2592000;

// Signature expiration time constants
const DEFAULT_PROPOSAL_EXPIRY: u64 = 86400; // 24 hours
const MAX_PROPOSAL_EXPIRY: u64 = 604_800; // 7 days

// Multisig configuration bounds
const MIN_THRESHOLD: u32 = 1;
const MAX_SIGNERS: u32 = 20;

// Batch bounds
const MAX_BATCH_MEMBERS: u32 = 30;
const MAX_FAMILY_MEMBERS: u32 = MAX_BATCH_MEMBERS;

// Access audit bounds
const MAX_ACCESS_AUDIT_ENTRIES: u32 = 200;
const MAX_AUDIT_PAGE_LIMIT: u32 = 50;
const DEFAULT_AUDIT_PAGE_LIMIT: u32 = 20;
const MAX_PENDING_PAGE_LIMIT: u32 = 100;
const DEFAULT_PENDING_PAGE_LIMIT: u32 = 20;
const MAX_MEMBER_PAGE_LIMIT: u32 = 100;
const DEFAULT_MEMBER_PAGE_LIMIT: u32 = 20;

/// Hard cap on the number of entries retained in `ARCH_TX`.
/// When the archive reaches this limit the oldest entry (lowest `tx_id`) is
/// evicted before the new one is inserted, keeping instance-storage rent bounded.
const MAX_ARCHIVE_ENTRIES: u32 = 500;
/// Default page size for `get_archived_transactions` when `limit == 0`.
const DEFAULT_ARCHIVE_PAGE_LIMIT: u32 = 20;
/// Maximum page size for `get_archived_transactions`.
const MAX_ARCHIVE_PAGE_LIMIT: u32 = 100;

#[contracttype]
#[derive(Clone)]
pub struct AccessAuditEntry {
    pub operation: Symbol,
    pub caller: Address,
    pub target: Option<Address>,
    pub success: bool,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct AccessAuditPage {
    pub items: Vec<AccessAuditEntry>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TransactionType {
    LargeWithdrawal = 1,
    SplitConfigChange = 2,
    RoleChange = 3,
    EmergencyTransfer = 4,
    PolicyCancellation = 5,
    RegularWithdrawal = 6,
}

#[contracttype]
#[derive(Clone)]
pub struct MultiSigConfig {
    pub threshold: u32,
    pub signers: Vec<Address>,
    pub spending_limit: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct PendingTransaction {
    pub tx_id: u64,
    pub tx_type: TransactionType,
    pub proposer: Address,
    pub signatures: Vec<Address>,
    pub created_at: u64,
    pub expires_at: u64,
    pub data: TransactionData,
}

#[contracttype]
#[derive(Clone)]
pub struct PendingTxPage {
    pub items: Vec<PendingTransaction>,
    pub next_cursor: u64,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum TransactionData {
    Withdrawal(Address, Address, i128),
    SplitConfigChange(u32, u32, u32, u32),
    RoleChange(Address, FamilyRole),
    EmergencyTransfer(Address, Address, i128),
    PolicyCancellation(u32),
}

/// Spending period configuration for rollover behavior
#[contracttype]
#[derive(Clone)]
pub struct SpendingPeriod {
    /// Period type: 0=Daily, 1=Weekly, 2=Monthly
    pub period_type: u32,
    /// Period start timestamp (aligned to period boundary)
    pub period_start: u64,
    /// Period duration in seconds
    pub period_duration: u64,
}

/// Cumulative spending tracking for precision validation
#[contracttype]
#[derive(Clone)]
pub struct SpendingTracker {
    pub current_spent: i128,
    pub last_tx_timestamp: u64,
    pub tx_count: u32,
    pub period: SpendingPeriod,
}

#[contracttype]
#[derive(Clone)]
pub struct MemberAddressPage {
    pub items: Vec<Address>,
    pub next_cursor: u32,
    pub count: u32,
}

/// Enhanced spending limit with precision controls
#[contracttype]
#[derive(Clone)]
pub struct PrecisionSpendingLimit {
    pub limit: i128,
    pub min_precision: i128,
    pub max_single_tx: i128,
    pub enable_rollover: bool,
}

/// Soroban `contracttype` does not support `Option<CustomStruct>`; use this instead of `Option`.
#[contracttype]
#[derive(Clone)]
pub enum PrecisionLimitOpt {
    None,
    Some(PrecisionSpendingLimit),
}

#[contracttype]
#[derive(Clone)]
pub struct FamilyMember {
    pub address: Address,
    pub role: FamilyRole,
    /// Legacy per-transaction cap in stroops. 0 = unlimited.
    pub spending_limit: i128,
    /// Optional precision spending guardrails for cumulative/rollover enforcement.
    pub precision_limit: PrecisionLimitOpt,
    pub added_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct EmergencyConfig {
    pub max_amount: i128,
    pub cooldown: u64,
    pub min_balance: i128,
    pub daily_limit: i128,
}

#[contracttype]
#[derive(Clone)]
pub enum EmergencyEvent {
    ModeOn,
    ModeOff,
    TransferInit,
    TransferExec,
}

#[contracttype]
#[derive(Clone)]
pub struct MemberAddedEvent {
    pub member: Address,
    pub role: FamilyRole,
    pub spending_limit: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct SpendingLimitUpdatedEvent {
    pub member: Address,
    pub old_limit: i128,
    pub new_limit: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ProposalInvalidatedEvent {
    pub tx_id: u64,
    pub reason: Symbol,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ArchivedTransaction {
    pub tx_id: u64,
    pub tx_type: TransactionType,
    pub proposer: Address,
    pub executed_at: u64,
    pub archived_at: u64,
}

/// Metadata for multisig-completed executions retained in `EXEC_TXS` until archived.
///
/// **Security:** `tx_id` must match the map key; mismatch indicates storage corruption
/// and must abort archiving (`archive_old_transactions`).
#[contracttype]
#[derive(Clone)]
pub struct ExecutedTxMeta {
    pub tx_id: u64,
    pub tx_type: TransactionType,
    pub proposer: Address,
    pub executed_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct StorageStats {
    pub pending_transactions: u32,
    pub archived_transactions: u32,
    pub total_members: u32,
    pub last_updated: u64,
}

const MAX_THRESHOLD: u32 = 100;

#[contracttype]
#[derive(Clone)]
pub struct BatchMemberItem {
    pub address: Address,
    pub role: FamilyRole,
}

#[contracttype]
#[derive(Clone)]
pub enum ArchiveEvent {
    TransactionsArchived,
    ExpiredCleaned,
    TransactionCancelled,
}

/// @title Family Wallet Multisig Proposal Expiry
/// @notice Manages the lifecycle of multisig proposals with deterministic expiry.
///
/// Security Assumptions:
/// 1. Proposer Authorization: Only authenticated family members can propose.
/// 2. Deterministic Expiry: Expiry is set at proposal time based on contract configuration.
/// 3. Signer Authorization: Only designated signers for a transaction type can sign.
/// 4. Cancellation Safety: Proposers can cancel their own proposals; Admins can cancel any.
/// 5. Expiry Enforcement: Expired proposals cannot be signed or executed.
/// 6. Storage Bounds: Expired proposals can be pruned by Admins to manage storage costs.
#[contract]
pub struct FamilyWallet;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    InvalidThreshold = 2,
    InvalidSigner = 3,
    TransactionNotFound = 4,
    TransactionExpired = 5,
    InsufficientSignatures = 6,
    DuplicateSignature = 7,
    InvalidTransactionType = 8,
    InvalidAmount = 9,
    InvalidRole = 10,
    MemberNotFound = 11,
    TransactionAlreadyExecuted = 12,
    InvalidSpendingLimit = 13,
    ThresholdBelowMinimum = 14,
    ThresholdAboveMaximum = 15,
    SignersListEmpty = 16,
    SignerNotMember = 17,
    DuplicateSigner = 18,
    TooManySigners = 19,
    InvalidPrecisionConfig = 20,
    InvalidProposalExpiry = 21,
    MemberAlreadyExists = 22,
    QuorumUnachievable = 23,
    /// An emergency transfer was rejected because the resulting balance would
    /// fall below `EmergencyConfig.min_balance`.
    MinBalanceViolation = 24,
}

#[contractimpl]
impl FamilyWallet {
    pub fn init(env: Env, owner: Address, initial_members: Vec<Address>) -> bool {
        owner.require_auth();
        let existing: Option<Address> = env.storage().instance().get(&symbol_short!("OWNER"));
        if existing.is_some() {
            panic!("Wallet already initialized");
        }
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("OWNER"), &owner);

        let mut members: Map<Address, FamilyMember> = Map::new(&env);
        let timestamp = env.ledger().timestamp();
        members.set(
            owner.clone(),
            FamilyMember {
                address: owner.clone(),
                role: FamilyRole::Owner,
                spending_limit: 0,
                precision_limit: PrecisionLimitOpt::None,
                added_at: timestamp,
            },
        );
        for member_addr in initial_members.iter() {
            members.set(
                member_addr.clone(),
                FamilyMember {
                    address: member_addr.clone(),
                    role: FamilyRole::Member,
                    spending_limit: 0,
                    precision_limit: PrecisionLimitOpt::None,
                    added_at: timestamp,
                },
            );
        }
        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members);

        let default_config = MultiSigConfig {
            threshold: 2,
            signers: Vec::new(&env),
            spending_limit: 1000_0000000,
        };

        for tx_type in [
            TransactionType::LargeWithdrawal,
            TransactionType::SplitConfigChange,
            TransactionType::RoleChange,
            TransactionType::EmergencyTransfer,
            TransactionType::PolicyCancellation,
        ] {
            env.storage()
                .instance()
                .set(&Self::get_config_key(tx_type), &default_config.clone());
        }

        env.storage().instance().set(
            &symbol_short!("PEND_TXS"),
            &Map::<u64, PendingTransaction>::new(&env),
        );
        env.storage().instance().set(
            &symbol_short!("EXEC_TXS"),
            &Map::<u64, ExecutedTxMeta>::new(&env),
        );

        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_TX"), &1u64);
        let em_config = EmergencyConfig {
            max_amount: 10000_0000000,
            cooldown: 3600,
            min_balance: 0,
            daily_limit: 100000_0000000,
        };
        env.storage()
            .instance()
            .set(&symbol_short!("EM_CONF"), &em_config);

        env.storage()
            .instance()
            .set(&symbol_short!("EM_MODE"), &false);

        env.storage()
            .instance()
            .set(&symbol_short!("EM_LAST"), &0u64);

        true
    }

    pub fn add_member(
        env: Env,
        admin: Address,
        member_address: Address,
        role: FamilyRole,
        spending_limit: i128,
    ) -> Result<bool, Error> {
        admin.require_auth();
        Self::require_not_paused(&env);
        if role == FamilyRole::Owner {
            return Err(Error::InvalidRole);
        }
        if !Self::is_owner_or_admin(&env, &admin) {
            return Err(Error::Unauthorized);
        }
        if spending_limit < 0 {
            return Err(Error::InvalidSpendingLimit);
        }

        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        if members.get(member_address.clone()).is_some() {
            return Err(Error::MemberAlreadyExists);
        }

        Self::extend_instance_ttl(&env);

        let now = env.ledger().timestamp();
        members.set(
            member_address.clone(),
            FamilyMember {
                address: member_address.clone(),
                role,
                spending_limit,
                precision_limit: PrecisionLimitOpt::None,
                added_at: now,
            },
        );
        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members);

        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::High,
            symbol_short!("member"),
            MemberAddedEvent {
                member: member_address,
                role,
                spending_limit,
                timestamp: now,
            },
        );

        Ok(true)
    }

    pub fn get_member(env: Env, member_address: Address) -> Option<FamilyMember> {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        members.get(member_address)
    }

    /// Update the spending limit for an existing family member.
    ///
    /// # Authorization
    /// Only Owner or Admin can update spending limits.
    ///
    /// # Arguments
    /// * `caller` - The address performing the update (must be Owner or Admin)
    /// * `member_address` - The member whose limit to update (must exist)
    /// * `new_limit` - New spending limit in stroops (>= 0)
    ///
    /// # Returns
    /// `bool` - true on successful update
    ///
    /// # Security
    /// - Validates caller is Owner or Admin
    /// - Ensures member exists
    /// - Enforces non-negative limits
    /// - Emits SpendingLimitUpdatedEvent on success
    pub fn update_spending_limit(
        env: Env,
        caller: Address,
        member_address: Address,
        new_limit: i128,
    ) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can update spending limits");
        }
        if new_limit < 0 {
            panic!("InvalidSpendingLimit");
        }

        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        let mut record = members
            .get(member_address.clone())
            .ok_or(Error::MemberNotFound)
            .unwrap_or_else(|_| panic!("MemberNotFound"));

        let old_limit = record.spending_limit;
        record.spending_limit = new_limit;
        members.set(member_address.clone(), record);

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members);

        let now = env.ledger().timestamp();
        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::Medium,
            symbol_short!("limit"),
            SpendingLimitUpdatedEvent {
                member: member_address,
                old_limit,
                new_limit,
                timestamp: now,
            },
        );

        true
    }

    /// Check if `caller` is allowed to spend `amount`.
    ///
    /// Rules (checked in order):
    /// 1. Unknown address → false
    /// 2. Negative amount → false
    /// 3. Owner / Admin → always true (unlimited)
    /// 4. Member with `spending_limit == 0` → unlimited → true
    /// 5. Member with `spending_limit > 0` → true iff `amount <= spending_limit`
    pub fn check_spending_limit(env: Env, caller: Address, amount: i128) -> bool {
        if amount < 0 {
            return false;
        }

        let members: Map<Address, FamilyMember> =
            match env.storage().instance().get(&symbol_short!("MEMBERS")) {
                Some(m) => m,
                None => return false,
            };

        let member = match members.get(caller) {
            Some(m) => m,
            None => return false,
        };

        // Expired roles are treated as having no permissions.
        if Self::role_has_expired(&env, &member.address) {
            return false;
        }

        // Owner and Admin are never restricted
        if member.role == FamilyRole::Owner || member.role == FamilyRole::Admin {
            return true;
        }

        // 0 means unlimited for regular members too
        if member.spending_limit == 0 {
            return true;
        }

        amount <= member.spending_limit
    }

    pub fn validate_precision_spending(
        env: Env,
        caller: Address,
        amount: i128,
    ) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if !Self::check_spending_limit(env.clone(), caller.clone(), amount) {
            return Err(Error::Unauthorized);
        }

        Ok(())
    }

    /// @notice Configure multisig parameters for a given transaction type.
    /// @dev Validates threshold bounds, signer membership, and uniqueness.
    ///      Returns `Result<bool, Error>` instead of panicking on invalid input.
    /// @param caller Owner or Admin authorizing the configuration.
    /// @param tx_type The transaction type to configure.
    /// @param threshold Number of signatures required (MIN_THRESHOLD..=min(MAX_THRESHOLD, signer_count)).
    /// @param signers List of authorized signers (must be family members, no duplicates).
    /// @param spending_limit Non-negative spending cap for the configuration.
    /// @return Ok(true) on success, or a specific Error variant on failure.
    pub fn configure_multisig(
        env: Env,
        caller: Address,
        tx_type: TransactionType,
        threshold: u32,
        signers: Vec<Address>,
        spending_limit: i128,
    ) -> Result<bool, Error> {
        caller.require_auth();
        Self::require_not_paused(&env);

        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        if !Self::is_owner_or_admin_in_members(&env, &members, &caller) {
            return Err(Error::Unauthorized);
        }

        let signer_count = signers.len();

        if signer_count == 0 {
            return Err(Error::SignersListEmpty);
        }

        if signer_count > MAX_SIGNERS {
            return Err(Error::TooManySigners);
        }

        if threshold < MIN_THRESHOLD {
            return Err(Error::ThresholdBelowMinimum);
        }

        if threshold > MAX_THRESHOLD {
            return Err(Error::ThresholdAboveMaximum);
        }

        if threshold > signer_count {
            // Must return a typed error for `configure_multisig`'s `Result` API.
            return Err(Error::InvalidThreshold);
        }





        // Check signer membership and uniqueness in a single pass
        let mut checked: Map<Address, bool> = Map::new(&env);
        for signer in signers.iter() {
            if members.get(signer.clone()).is_none() {
                return Err(Error::SignerNotMember);
            }
            if checked.get(signer.clone()).is_some() {
                return Err(Error::DuplicateSigner);
            }
            checked.set(signer.clone(), true);
        }

        if spending_limit < 0 {
            return Err(Error::InvalidSpendingLimit);
        }

        Self::extend_instance_ttl(&env);

        let config = MultiSigConfig {
            threshold,
            signers,
            spending_limit,
        };

        env.storage()
            .instance()
            .set(&Self::get_config_key(tx_type), &config);

        Ok(true)
    }

    pub fn propose_transaction(
        env: Env,
        proposer: Address,
        tx_type: TransactionType,
        data: TransactionData,
    ) -> u64 {
        proposer.require_auth();
        Self::require_not_paused(&env);
        Self::require_role_at_least(&env, &proposer, FamilyRole::Member);

        if !Self::is_family_member(&env, &proposer) {
            panic!("Only family members can propose transactions");
        }

        let config_key = match tx_type {
            TransactionType::RegularWithdrawal => {
                Self::get_config_key(TransactionType::LargeWithdrawal)
            }
            _ => Self::get_config_key(tx_type),
        };

        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&config_key)
            .unwrap_or_else(|| panic!("Multi-sig config not found"));

        let requires_multisig = match (&tx_type, &data) {
            (TransactionType::RegularWithdrawal, TransactionData::Withdrawal(_, _, amount)) => {
                *amount > config.spending_limit
            }
            (TransactionType::LargeWithdrawal, _) => true,
            (TransactionType::RegularWithdrawal, _) => false,
            _ => true,
        };

        if !requires_multisig {
            return Self::execute_transaction_internal(&env, &proposer, &tx_type, &data, false);
        }

        Self::extend_instance_ttl(&env);

        let mut next_tx_id: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_TX"))
            .unwrap_or(1);

        let tx_id = next_tx_id;
        next_tx_id += 1;

        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_TX"), &next_tx_id);

        let timestamp = env.ledger().timestamp();
        let mut signatures = Vec::new(&env);
        signatures.push_back(proposer.clone());

        let expiry_duration: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("PROP_EXP"))
            .unwrap_or(DEFAULT_PROPOSAL_EXPIRY);

        // If duration is 0, expiry is disabled — set expires_at to u64::MAX so the guard never trips.
        let expires_at = if expiry_duration == 0 {
            u64::MAX
        } else {
            timestamp + expiry_duration
        };

        let pending_tx = PendingTransaction {
            tx_id,
            tx_type,
            proposer: proposer.clone(),
            signatures,
            created_at: timestamp,
            expires_at,
            data: data.clone(),
        };

        let mut pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| panic!("Pending transactions map not initialized"));

        pending_txs.set(tx_id, pending_tx);
        env.storage()
            .instance()
            .set(&symbol_short!("PEND_TXS"), &pending_txs);

        tx_id
    }
    /// Sign a pending multisig transaction.
    ///
    /// Idempotency: repeated calls by the same `signer` for the same `tx_id` are
    /// treated as a no-op and do not increase the recorded approval count. The
    /// proposer's implicit approval (added when the proposal is created) is
    /// respected and will not be double-counted if the proposer calls this
    /// method again.
    pub fn sign_transaction(env: Env, signer: Address, tx_id: u64) -> Result<bool, Error> {
        signer.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_family_member(&env, &signer) {
            return Err(Error::SignerNotMember);
        }
        Self::require_role_at_least(&env, &signer, FamilyRole::Member);

        Self::extend_instance_ttl(&env);

        let mut pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| panic!("Pending transactions map not initialized"));

        let mut pending_tx = pending_txs
            .get(tx_id)
            .unwrap_or_else(|| panic!("Transaction not found"));

        let current_time = env.ledger().timestamp();
        if current_time > pending_tx.expires_at {
            return Err(Error::TransactionExpired);
        }

        // If signer already recorded, no-op (idempotent).
        for sig in pending_tx.signatures.iter() {
            if sig.clone() == signer {
                return Ok(false);
            }
        }

        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&Self::get_config_key(pending_tx.tx_type))
            .unwrap_or_else(|| panic!("Multi-sig config not found"));

        let mut is_authorized = false;
        for authorized_signer in config.signers.iter() {
            if authorized_signer.clone() == signer {
                is_authorized = true;
                break;
            }
        }

        if !is_authorized {
            return Err(Error::SignerNotMember);
        }

        pending_tx.signatures.push_back(signer.clone());

        // Count only signatures whose signer is still authorized in the CURRENT
        // config. Signatures collected from signers that were rotated out before
        // this call must not contribute to quorum.
        let mut valid_signatures: u32 = 0;
        for sig in pending_tx.signatures.iter() {
            for authorized_signer in config.signers.iter() {
                if authorized_signer.clone() == sig {
                    valid_signatures += 1;
                    break;
                }
            }
        }

        if valid_signatures >= config.threshold {
            let executed = Self::execute_transaction_internal(
                &env,
                &pending_tx.proposer,
                &pending_tx.tx_type,
                &pending_tx.data,
                true,
            );

            if executed == 0 {
                pending_txs.remove(tx_id);
                env.storage()
                    .instance()
                    .set(&symbol_short!("PEND_TXS"), &pending_txs);

                let mut executed_txs: Map<u64, ExecutedTxMeta> = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("EXEC_TXS"))
                    .unwrap_or_else(|| panic!("Executed transactions map not initialized"));

                let executed_at = env.ledger().timestamp();
                executed_txs.set(
                    tx_id,
                    ExecutedTxMeta {
                        tx_id,
                        tx_type: pending_tx.tx_type,
                        proposer: pending_tx.proposer.clone(),
                        executed_at,
                    },
                );
                env.storage()
                    .instance()
                    .set(&symbol_short!("EXEC_TXS"), &executed_txs);
            }

            return Ok(true);
        }

        pending_txs.set(tx_id, pending_tx);
        env.storage()
            .instance()
            .set(&symbol_short!("PEND_TXS"), &pending_txs);

        Ok(true)
    }

    /// Withdraw funds using the appropriate spending limit and multi-sig configuration.
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn withdraw(
        env: Env,
        proposer: Address,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> u64 {
        Self::require_not_paused(&env);
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        if !Self::check_spending_limit(env.clone(), proposer.clone(), amount) {
            panic!("Spending limit exceeded");
        }

        let config: MultiSigConfig = env
            .storage()
            .instance()
            .get(&Self::get_config_key(TransactionType::LargeWithdrawal))
            .unwrap_or_else(|| panic!("Multi-sig config not found"));

        let tx_type = if amount > config.spending_limit {
            TransactionType::LargeWithdrawal
        } else {
            TransactionType::RegularWithdrawal
        };

        Self::propose_transaction(
            env,
            proposer,
            tx_type,
            TransactionData::Withdrawal(token, recipient, amount),
        )
    }

    /// Propose a split configuration change.
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn propose_split_config_change(
        env: Env,
        proposer: Address,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> u64 {
        Self::require_not_paused(&env);
        if spending_percent + savings_percent + bills_percent + insurance_percent != 100 {
            panic!("Percentages must sum to 100");
        }

        Self::propose_transaction(
            env,
            proposer,
            TransactionType::SplitConfigChange,
            TransactionData::SplitConfigChange(
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ),
        )
    }

    /// Propose a family member role change.
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn propose_role_change(
        env: Env,
        proposer: Address,
        member: Address,
        new_role: FamilyRole,
    ) -> u64 {
        Self::require_not_paused(&env);
        Self::propose_transaction(
            env,
            proposer,
            TransactionType::RoleChange,
            TransactionData::RoleChange(member, new_role),
        )
    }

    /// Propose or execute an emergency transfer.
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn propose_emergency_transfer(
        env: Env,
        proposer: Address,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> u64 {
        Self::require_not_paused(&env);
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let em_mode: bool = env
            .storage()
            .instance()
            .get(&symbol_short!("EM_MODE"))
            .unwrap_or(false);

        if em_mode {
            return Self::execute_emergency_transfer_now(env, proposer, token, recipient, amount);
        }

        let pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut active_proposals = 0;
        for (_, tx) in pending_txs.iter() {
            if tx.proposer == proposer && tx.tx_type == TransactionType::EmergencyTransfer {
                if let TransactionData::EmergencyTransfer(t, r, a) = &tx.data {
                    if t == &token && r == &recipient && *a == amount {
                        panic!("Identical emergency transfer proposal already pending");
                    }
                }
                active_proposals += 1;
            }
        }

        if active_proposals >= 1 {
            panic!("Maximum pending emergency proposals reached");
        }

        let tx_id = Self::propose_transaction(
            env.clone(),
            proposer.clone(),
            TransactionType::EmergencyTransfer,
            TransactionData::EmergencyTransfer(token.clone(), recipient.clone(), amount),
        );

        Self::append_access_audit(
            &env,
            symbol_short!("em_prop"),
            &proposer,
            Some(recipient.clone()),
            true,
        );

        tx_id
    }

    /// Propose a policy cancellation.
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn propose_policy_cancellation(env: Env, proposer: Address, policy_id: u32) -> u64 {
        Self::require_not_paused(&env);
        Self::propose_transaction(
            env,
            proposer,
            TransactionType::PolicyCancellation,
            TransactionData::PolicyCancellation(policy_id),
        )
    }

    /// Configure emergency transfer guardrails.
    ///
    /// Only `Owner` or `Admin` may update emergency settings.
    /// Successful configuration is recorded in the access audit trail.
    pub fn configure_emergency(
        env: Env,
        caller: Address,
        max_amount: i128,
        cooldown: u64,
        min_balance: i128,
        daily_limit: i128,
    ) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can configure emergency settings");
        }
        if max_amount <= 0 {
            panic!("Emergency max amount must be positive");
        }
        if min_balance < 0 {
            panic!("Emergency min balance must be non-negative");
        }

        Self::extend_instance_ttl(&env);

        env.storage().instance().set(
            &symbol_short!("EM_CONF"),
            &EmergencyConfig {
                max_amount,
                cooldown,
                min_balance,
                daily_limit,
            },
        );

        Self::append_access_audit(&env, symbol_short!("em_conf"), &caller, None, true);

        true
    }

    /// Enable or disable emergency mode.
    ///
    /// This operation is restricted to `Owner` or `Admin` and is recorded in the access audit trail.
    pub fn set_emergency_mode(env: Env, caller: Address, enabled: bool) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can change emergency mode");
        }

        Self::extend_instance_ttl(&env);

        env.storage()
            .instance()
            .set(&symbol_short!("EM_MODE"), &enabled);

        let event = if enabled {
            EmergencyEvent::ModeOn
        } else {
            EmergencyEvent::ModeOff
        };
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("em_mode"),
            event,
        );

        Self::append_access_audit(&env, symbol_short!("em_mode"), &caller, None, true);

        true
    }

    pub fn add_family_member(env: Env, caller: Address, member: Address, role: FamilyRole) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);
        if role == FamilyRole::Owner {
            panic!("Cannot add Owner via add_family_member");
        }
        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can add family members");
        }

        Self::extend_instance_ttl(&env);

        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        let timestamp = env.ledger().timestamp();
        members.set(
            member.clone(),
            FamilyMember {
                address: member.clone(),
                role,
                spending_limit: 0,
                precision_limit: PrecisionLimitOpt::None,
                added_at: timestamp,
            },
        );

        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members);

        Self::append_access_audit(&env, symbol_short!("add_mem"), &caller, Some(member), true);
        true
    }

    /// Remove a family member from the wallet.
    ///
    /// # Authorization
    /// Only Owner can remove family members.
    ///
    /// # Arguments
    /// * `caller` - The address performing the removal (must be Owner)
    /// * `member` - The member address to remove
    ///
    /// # Returns
    /// `bool` - true on successful removal
    ///
    /// # Security
    /// - Validates caller is Owner
    /// - Prevents removing the Owner
    /// - Silently succeeds if member doesn't exist
    /// - Records access audit entry
    pub fn remove_family_member(env: Env, caller: Address, member: Address) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);

        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        if Self::role_has_expired(&env, &caller) {
            panic!("Role has expired");
        }
        if caller != owner {
            panic!("Only Owner can remove family members");
        }
        if member == owner {
            panic!("Cannot remove owner");
        }

        Self::extend_instance_ttl(&env);

        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        members.remove(member.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members);

        // Re-validate in-flight proposals: strip signatures from the removed
        // member and invalidate any proposal that can no longer reach quorum.
        Self::revalidate_proposals_after_membership_change(&env);

        Self::append_access_audit(&env, symbol_short!("rem_mem"), &caller, Some(member), true);
        true
    }

    pub fn get_pending_transaction(env: Env, tx_id: u64) -> Option<PendingTransaction> {
        let pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| panic!("Pending transactions map not initialized"));

        pending_txs.get(tx_id)
    }

    /// Paginated listing of pending multisig proposals.
    ///
    /// - `caller` must be authenticated.
    /// - Owner/Admin may list all pending proposals.
    /// - Regular members may only list proposals they proposed.
    ///
    /// Cursor is the last-seen `tx_id`. Pass `0` for the first page.
    pub fn get_pending_transactions_page(
        env: Env,
        caller: Address,
        cursor: u64,
        limit: u32,
    ) -> PendingTxPage {
        caller.require_auth();

        let capped_limit = if limit == 0 {
            DEFAULT_PENDING_PAGE_LIMIT
        } else {
            limit.min(MAX_PENDING_PAGE_LIMIT)
        };

        let pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_tx: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_TX"))
            .unwrap_or(1u64);

        let mut items: Vec<PendingTransaction> = Vec::new(&env);

        let mut id = cursor.saturating_add(1);
        let mut last_returned: u64 = 0;
        let is_admin = Self::is_owner_or_admin(&env, &caller);

        while id < next_tx && items.len() < capped_limit {
            if let Some(tx) = pending_txs.get(id) {
                if is_admin || tx.proposer == caller {
                    items.push_back(tx.clone());
                    last_returned = id;
                }
            }
            id = id.saturating_add(1);
        }

        let next_cursor = if id < next_tx && last_returned != 0 {
            last_returned
        } else {
            0u64
        };
        let count = items.len();

        PendingTxPage {
            items,
            next_cursor,
            count,
        }
    }

    pub fn get_multisig_config(env: Env, tx_type: TransactionType) -> Option<MultiSigConfig> {
        env.storage().instance().get(&Self::get_config_key(tx_type))
    }

    pub fn get_family_member(env: Env, member: Address) -> Option<FamilyMember> {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        members.get(member)
    }

    pub fn get_owner(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .unwrap_or_else(|| panic!("Wallet not initialized"))
    }

    pub fn get_emergency_config(env: Env) -> Option<EmergencyConfig> {
        env.storage().instance().get(&symbol_short!("EM_CONF"))
    }

    pub fn is_emergency_mode(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("EM_MODE"))
            .unwrap_or(false)
    }

    pub fn get_last_emergency_at(env: Env) -> Option<u64> {
        let ts: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("EM_LAST"))
            .unwrap_or(0u64);
        if ts == 0 {
            None
        } else {
            Some(ts)
        }
    }

    /// Moves **eligible** multisig-executed transactions from `EXEC_TXS` into `ARCH_TX`.
    ///
    /// # Semantics
    /// - `before_timestamp` is a **retention cutoff** (ledger seconds): a row is archived iff
    ///   `executed_at < before_timestamp` (strictly less-than — entries executed *at* the cutoff
    ///   are **not** archived, preserving the most recent boundary entry in `EXEC_TXS`).
    /// - The cutoff must satisfy `before_timestamp <= ledger timestamp`. A future cutoff would
    ///   treat recent executions as "old" relative to an incorrect clock and could archive too much.
    ///
    /// # Bounded growth invariant
    /// `ARCH_TX` is capped at `MAX_ARCHIVE_ENTRIES`. Before inserting each new entry, if the
    /// archive is already at capacity the entry with the **lowest `tx_id`** (oldest) is evicted.
    /// This keeps instance-storage rent bounded regardless of how many transactions are executed
    /// over the contract's lifetime.
    ///
    /// # Authorization
    /// Owner or Admin only (`caller.require_auth()`).
    ///
    /// # Data integrity
    /// Archived rows copy **proposer**, **tx_type**, and **executed_at** from `ExecutedTxMeta`.
    /// If `meta.tx_id != map_key`, the contract panics to avoid corrupting the archive.
    ///
    /// # Returns
    /// The number of transactions moved from `EXEC_TXS` to `ARCH_TX` in this call.
    pub fn archive_old_transactions(env: Env, caller: Address, before_timestamp: u64) -> u32 {
        caller.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can archive transactions");
        }

        Self::extend_instance_ttl(&env);

        let now = env.ledger().timestamp();
        if before_timestamp > now {
            panic!("Archive retention cutoff must not exceed ledger time");
        }

        let mut executed_txs: Map<u64, ExecutedTxMeta> = env
            .storage()
            .instance()
            .get(&symbol_short!("EXEC_TXS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut archived: Map<u64, ArchivedTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_TX"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut archived_count = 0u32;
        let mut to_remove: Vec<u64> = Vec::new(&env);

        // Pre-compute archive length and oldest tx_id once, before the main loop.
        // This avoids O(n²) nested iteration which exhausts the Soroban WASM budget.
        let mut arch_len = 0u32;
        let mut oldest_arch_id: Option<u64> = None;
        for (aid, _) in archived.iter() {
            arch_len += 1;
            oldest_arch_id = Some(match oldest_arch_id {
                None => aid,
                Some(prev) => {
                    if aid < prev {
                        aid
                    } else {
                        prev
                    }
                }
            });
        }

        for (tx_id, meta) in executed_txs.iter() {
            if meta.tx_id != tx_id {
                panic!("Inconsistent executed transaction metadata");
            }
            // Strictly less-than: entries executed AT before_timestamp are retained.
            if meta.executed_at < before_timestamp {
                // Enforce the archive size cap: evict the oldest entry (lowest tx_id)
                // before inserting so ARCH_TX never exceeds MAX_ARCHIVE_ENTRIES.
                if arch_len >= MAX_ARCHIVE_ENTRIES {
                    if let Some(oid) = oldest_arch_id {
                        archived.remove(oid);
                        // After eviction, find the new oldest from the remaining entries.
                        let mut new_oldest: Option<u64> = None;
                        for (aid, _) in archived.iter() {
                            new_oldest = Some(match new_oldest {
                                None => aid,
                                Some(prev) => {
                                    if aid < prev {
                                        aid
                                    } else {
                                        prev
                                    }
                                }
                            });
                        }
                        oldest_arch_id = new_oldest;
                        // arch_len stays the same: we removed one and will add one below.
                    }
                } else {
                    arch_len += 1;
                    // Update oldest_arch_id if this new entry is older (lower tx_id).
                    oldest_arch_id = Some(match oldest_arch_id {
                        None => tx_id,
                        Some(prev) => {
                            if tx_id < prev {
                                tx_id
                            } else {
                                prev
                            }
                        }
                    });
                }

                let archived_tx = ArchivedTransaction {
                    tx_id: meta.tx_id,
                    tx_type: meta.tx_type,
                    proposer: meta.proposer.clone(),
                    executed_at: meta.executed_at,
                    archived_at: current_time,
                };
                archived.set(tx_id, archived_tx);
                to_remove.push_back(tx_id);
                archived_count += 1;
            }
        }

        for i in 0..to_remove.len() {
            if let Some(id) = to_remove.get(i) {
                executed_txs.remove(id);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_TXS"), &executed_txs);

        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_TX"), &archived);

        Self::extend_archive_ttl(&env);
        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("archive"), ArchiveEvent::TransactionsArchived),
            (archived_count, caller),
        );

        archived_count
    }

    /// Returns a page of archived transactions ordered by ascending `tx_id`.
    ///
    /// # Parameters
    /// - `limit`: entries to return; `0` → `DEFAULT_ARCHIVE_PAGE_LIMIT`; clamped to
    ///   `MAX_ARCHIVE_PAGE_LIMIT`. Ordering follows the map's natural key order (ascending `tx_id`).
    ///
    /// # Authorization
    /// Only Owner or Admin. Requires `caller.require_auth()` to prevent unauthenticated reads
    /// of historical transaction metadata (ownership / privacy leakage).
    pub fn get_archived_transactions(
        env: Env,
        caller: Address,
        limit: u32,
    ) -> Vec<ArchivedTransaction> {
        caller.require_auth();
        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can view archived transactions");
        }

        // Clamp limit: 0 → default, >max → max.
        let effective_limit = if limit == 0 {
            DEFAULT_ARCHIVE_PAGE_LIMIT
        } else if limit > MAX_ARCHIVE_PAGE_LIMIT {
            MAX_ARCHIVE_PAGE_LIMIT
        } else {
            limit
        };

        let archived: Map<u64, ArchivedTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_TX"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (count, (_, tx)) in archived.iter().enumerate() {
            if count as u32 >= effective_limit {
                break;
            }
            result.push_back(tx);
        }
        result
    }

    /// Removes pending proposals whose `expires_at` is strictly before the ledger time.
    ///
    /// # Authorization
    /// Owner or Admin only.
    ///
    /// # Integrity
    /// Aborts if `pending.tx_id` does not match the map key (prevents silent corruption during cleanup).
    pub fn cleanup_expired_pending(env: Env, caller: Address) -> u32 {
        caller.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_owner_or_admin(&env, &caller) {
            panic!("Only Owner or Admin can cleanup expired transactions");
        }

        Self::extend_instance_ttl(&env);

        let mut pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut removed_count = 0u32;
        let mut to_remove: Vec<u64> = Vec::new(&env);

        for (tx_id, tx) in pending_txs.iter() {
            if tx.tx_id != tx_id {
                panic!("Inconsistent pending transaction data");
            }
            if tx.expires_at < current_time {
                to_remove.push_back(tx_id);
                removed_count += 1;
            }
        }

        for i in 0..to_remove.len() {
            if let Some(id) = to_remove.get(i) {
                pending_txs.remove(id);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("PEND_TXS"), &pending_txs);

        Self::update_storage_stats(&env);

        env.events().publish(
            (symbol_short!("archive"), ArchiveEvent::ExpiredCleaned),
            (removed_count, caller),
        );
        removed_count
    }

    pub fn get_storage_stats(env: Env) -> StorageStats {
        env.storage()
            .instance()
            .get(&symbol_short!("STOR_STAT"))
            .unwrap_or(StorageStats {
                pending_transactions: 0,
                archived_transactions: 0,
                total_members: 0,
                last_updated: 0,
            })
    }

    /// @notice Set or clear a role-expiry timestamp for an existing family member.
    /// @dev Expiry is inclusive: at `ledger.timestamp() >= expires_at` the member is treated as expired.
    /// @param caller Admin/Owner authorizing the change.
    /// @param member Target family member.
    /// @param expires_at Unix timestamp in seconds; `None` clears expiry.
    pub fn set_role_expiry(
        env: Env,
        caller: Address,
        member: Address,
        expires_at: Option<u64>,
    ) -> bool {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Admin);
        Self::require_not_paused(&env);
        Self::extend_instance_ttl(&env);

        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));
        if members.get(member.clone()).is_none() {
            panic!("Member not found");
        }

        let mut m: Map<Address, u64> = env
            .storage()
            .instance()
            .get(&symbol_short!("ROLE_EXP"))
            .unwrap_or_else(|| Map::new(&env));
        match expires_at {
            Some(t) => m.set(member.clone(), t),
            None => {
                m.remove(member.clone());
            }
        }
        env.storage().instance().set(&symbol_short!("ROLE_EXP"), &m);
        Self::append_access_audit(&env, symbol_short!("role_exp"), &caller, Some(member), true);
        true
    }

    pub fn get_role_expiry_public(env: Env, address: Address) -> Option<u64> {
        Self::get_role_expiry(&env, &address)
    }

    /// Configure withdrawal precision limits for an existing member.
    ///
    /// Only the owner or an admin may set limits. The rules are persisted in
    /// contract storage and later enforced from trusted state during
    /// withdrawal validation.
    pub fn set_precision_spending_limit(
        env: Env,
        caller: Address,
        member: Address,
        limit: PrecisionSpendingLimit,
    ) -> Result<bool, Error> {
        caller.require_auth();
        Self::require_not_paused(&env);

        if !Self::is_owner_or_admin(&env, &caller) {
            return Err(Error::Unauthorized);
        }

        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));
        if members.get(member.clone()).is_none() {
            return Err(Error::MemberNotFound);
        }

        if limit.limit < 0
            || limit.min_precision <= 0
            || limit.max_single_tx <= 0
            || limit.max_single_tx > limit.limit
        {
            return Err(Error::InvalidPrecisionConfig);
        }

        Self::extend_instance_ttl(&env);

        let mut limits: Map<Address, PrecisionSpendingLimit> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREC_LIM"))
            .unwrap_or_else(|| Map::new(&env));
        limits.set(member.clone(), limit.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("PREC_LIM"), &limits);

        if !limit.enable_rollover {
            let mut trackers: Map<Address, SpendingTracker> = env
                .storage()
                .instance()
                .get(&symbol_short!("SPND_TRK"))
                .unwrap_or_else(|| Map::new(&env));
            trackers.remove(member);
            env.storage()
                .instance()
                .set(&symbol_short!("SPND_TRK"), &trackers);
        }

        Ok(true)
    }

    /// Get the persisted cumulative spending tracker for a member, if any.
    pub fn get_spending_tracker(env: Env, member: Address) -> Option<SpendingTracker> {
        env.storage()
            .instance()
            .get::<_, Map<Address, SpendingTracker>>(&symbol_short!("SPND_TRK"))
            .unwrap_or_else(|| Map::new(&env))
            .get(member)
    }

    /// Paginated listing of family-member addresses for downstream readers.
    ///
    /// Cursor is the number of members already returned. Pass `0` for the
    /// first page.
    pub fn get_member_addresses_page(env: Env, cursor: u32, limit: u32) -> MemberAddressPage {
        let capped_limit = if limit == 0 {
            DEFAULT_MEMBER_PAGE_LIMIT
        } else {
            limit.min(MAX_MEMBER_PAGE_LIMIT)
        };

        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        let mut items: Vec<Address> = Vec::new(&env);
        let mut seen = 0u32;
        let mut has_more = false;

        for (address, _) in members.iter() {
            if seen < cursor {
                seen = seen.saturating_add(1);
                continue;
            }

            if items.len() < capped_limit {
                items.push_back(address);
                seen = seen.saturating_add(1);
            } else {
                has_more = true;
                break;
            }
        }

        let next_cursor = if has_more {
            cursor.saturating_add(items.len())
        } else {
            0
        };

        MemberAddressPage {
            count: items.len(),
            items,
            next_cursor,
        }
    }

    /// Cancel a pending transaction.
    ///
    /// The original proposer may cancel their own transaction. Owners and
    /// admins may cancel any pending transaction.
    pub fn cancel_transaction(env: Env, caller: Address, tx_id: u64) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);

        let mut pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| panic!("Pending transactions map not initialized"));

        let pending_tx = pending_txs.get(tx_id).unwrap_or_else(|| {
            panic_with_error!(&env, Error::TransactionNotFound);
        });

        if caller != pending_tx.proposer && !Self::is_owner_or_admin(&env, &caller) {
            panic_with_error!(&env, Error::Unauthorized);
        }

        Self::extend_instance_ttl(&env);
        pending_txs.remove(tx_id);
        env.storage()
            .instance()
            .set(&symbol_short!("PEND_TXS"), &pending_txs);

        env.events().publish(
            (symbol_short!("archive"), ArchiveEvent::TransactionCancelled),
            (tx_id, caller),
        );

        true
    }

    pub fn pause(env: Env, caller: Address) -> bool {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Admin);
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| {
            env.storage()
                .instance()
                .get(&symbol_short!("OWNER"))
                .unwrap_or_else(|| panic!("Wallet not initialized"))
        });
        if admin != caller {
            panic!("Only pause admin can pause");
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        env.events()
            .publish((symbol_short!("wallet"), symbol_short!("paused")), ());
        true
    }

    pub fn unpause(env: Env, caller: Address) -> bool {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).unwrap_or_else(|| {
            env.storage()
                .instance()
                .get(&symbol_short!("OWNER"))
                .unwrap_or_else(|| panic!("Wallet not initialized"))
        });
        if admin != caller {
            panic!("Only pause admin can unpause");
        }
        if Self::role_has_expired(&env, &caller) {
            panic!("Role has expired");
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &false);
        env.events()
            .publish((symbol_short!("wallet"), symbol_short!("unpaused")), ());
        true
    }

    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) -> bool {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Owner);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        true
    }

    pub fn is_paused(env: Env) -> bool {
        Self::get_global_paused(&env)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(CONTRACT_VERSION)
    }

    /// Set the multisig proposal expiry window in seconds.
    ///
    /// # Security
    /// Only the Owner can set this value, and their role must not be expired.
    ///
    /// A value of `0` disables expiry (proposals never expire).
    /// Values greater than `MAX_PROPOSAL_EXPIRY` are rejected.
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn set_proposal_expiry(env: Env, caller: Address, expiry: u64) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);
        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        // Verify caller is owner AND role is not expired
        if caller != owner {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if Self::role_has_expired(&env, &caller) {
            panic!("Role has expired");
        }

        if expiry > MAX_PROPOSAL_EXPIRY {
            panic_with_error!(&env, Error::InvalidProposalExpiry);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("PROP_EXP"), &expiry);
        true
    }

    /// Return the configured proposal expiry window, or the default if unset.
    pub fn get_proposal_expiry_public(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&symbol_short!("PROP_EXP"))
            .unwrap_or(DEFAULT_PROPOSAL_EXPIRY)
    }

    fn get_upgrade_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("UPG_ADM"))
    }

    fn current_spending_tracker(env: &Env, proposer: &Address) -> SpendingTracker {
        let current_time = env.ledger().timestamp();
        let period_duration = 86_400u64;
        let period_start = (current_time / period_duration) * period_duration;

        let mut trackers: Map<Address, SpendingTracker> = env
            .storage()
            .instance()
            .get(&symbol_short!("SPND_TRK"))
            .unwrap_or_else(|| Map::new(env));

        let tracker = if let Some(existing) = trackers.get(proposer.clone()) {
            if existing.period.period_start == period_start {
                existing
            } else {
                SpendingTracker {
                    current_spent: 0,
                    last_tx_timestamp: 0,
                    tx_count: 0,
                    period: SpendingPeriod {
                        period_type: 0,
                        period_start,
                        period_duration,
                    },
                }
            }
        } else {
            SpendingTracker {
                current_spent: 0,
                last_tx_timestamp: 0,
                tx_count: 0,
                period: SpendingPeriod {
                    period_type: 0,
                    period_start,
                    period_duration,
                },
            }
        };

        trackers.set(proposer.clone(), tracker.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("SPND_TRK"), &trackers);

        tracker
    }

    fn record_precision_spending(env: &Env, proposer: &Address, amount: i128) {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));
        let Some(member) = members.get(proposer.clone()) else {
            return;
        };

        if matches!(member.role, FamilyRole::Owner | FamilyRole::Admin) {
            return;
        }

        let limits: Map<Address, PrecisionSpendingLimit> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREC_LIM"))
            .unwrap_or_else(|| Map::new(env));
        let Some(limit) = limits.get(proposer.clone()) else {
            return;
        };
        if !limit.enable_rollover {
            return;
        }

        let mut trackers: Map<Address, SpendingTracker> = env
            .storage()
            .instance()
            .get(&symbol_short!("SPND_TRK"))
            .unwrap_or_else(|| Map::new(env));
        let mut tracker = Self::current_spending_tracker(env, proposer);
        // Overflow-safe tracker accumulation
        tracker.current_spent = tracker
            .current_spent
            .checked_add(amount)
            .unwrap_or(i128::MAX);
        tracker.last_tx_timestamp = env.ledger().timestamp();
        tracker.tx_count = tracker.tx_count.saturating_add(1);
        trackers.set(proposer.clone(), tracker);
        env.storage()
            .instance()
            .set(&symbol_short!("SPND_TRK"), &trackers);
    }

    fn validate_precision_spending_internal(
        env: Env,
        proposer: Address,
        amount: i128,
    ) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));
        let member = members.get(proposer.clone()).ok_or(Error::MemberNotFound)?;

        if matches!(member.role, FamilyRole::Owner | FamilyRole::Admin) {
            return Ok(());
        }

        let limits: Map<Address, PrecisionSpendingLimit> = env
            .storage()
            .instance()
            .get(&symbol_short!("PREC_LIM"))
            .unwrap_or_else(|| Map::new(&env));

        if let Some(limit) = limits.get(proposer.clone()) {
            if amount < limit.min_precision || amount > limit.max_single_tx {
                return Err(Error::InvalidPrecisionConfig);
            }

            if limit.enable_rollover {
                let tracker = Self::current_spending_tracker(&env, &proposer);
                // Overflow-safe addition to prevent DoS via integer overflow in accumulated spend
                let new_spent = tracker
                    .current_spent
                    .checked_add(amount)
                    .ok_or(Error::InvalidSpendingLimit)?;
                if new_spent > limit.limit {
                    return Err(Error::InvalidSpendingLimit);
                }
            }

            return Ok(());
        }

        if member.spending_limit > 0 && amount > member.spending_limit {
            return Err(Error::InvalidSpendingLimit);
        }

        Ok(())
    }

    /// Set or transfer the upgrade admin role.
    ///
    /// # Security Requirements
    /// - Only wallet owners can set or transfer upgrade admin role
    /// - Caller must be authenticated via require_auth()
    /// - Caller must have at least Owner role in the family wallet
    ///
    /// # Parameters
    /// - `caller`: The address attempting to set the upgrade admin
    /// - `new_admin`: The address to become the new upgrade admin
    ///
    /// # Returns
    /// - `true` on successful admin transfer
    ///
    /// # Panics
    /// - If caller lacks Owner role or higher
    /// - If the contract is paused
    pub fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) -> bool {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Owner);
        Self::require_not_paused(&env);

        let current_upgrade_admin = Self::get_upgrade_admin(&env);

        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);

        // Emit admin transfer event for audit trail
        env.events().publish(
            (symbol_short!("family"), symbol_short!("adm_xfr")),
            (current_upgrade_admin.clone(), new_admin.clone()),
        );

        true
    }

    /// Get the current upgrade admin address.
    ///
    /// # Returns
    /// - `Some(Address)` if upgrade admin is set
    /// - `None` if no upgrade admin has been configured
    pub fn get_upgrade_admin_public(env: Env) -> Option<Address> {
        Self::get_upgrade_admin(&env)
    }

    /// Set the contract version (upgrade support).
    ///
    /// # Errors
    /// Panics if the contract is paused.
    pub fn set_version(env: Env, caller: Address, new_version: u32) -> bool {
        caller.require_auth();
        Self::require_not_paused(&env);
        let admin = Self::get_upgrade_admin(&env).unwrap_or_else(|| {
            env.storage()
                .instance()
                .get(&symbol_short!("OWNER"))
                .unwrap_or_else(|| panic!("Wallet not initialized"))
        });
        if admin != caller {
            panic!("Only upgrade admin can set version");
        }
        if Self::role_has_expired(&env, &caller) {
            panic!("Role has expired");
        }
        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);
        env.events().publish(
            (symbol_short!("wallet"), symbol_short!("upgraded")),
            (prev, new_version),
        );
        true
    }

    /// Add a batch of family members atomically.
    ///
    /// Semantics:
    /// - The whole batch succeeds or the whole batch fails.
    /// - Empty batches are accepted and return `0`.
    /// - Any duplicate address in the batch, pre-existing member, owner-role item,
    ///   or batch that would exceed the family-member cap aborts the entire call.
    /// - On success, the return value is the number of members added.
    pub fn batch_add_family_members(
        env: Env,
        caller: Address,
        members: Vec<BatchMemberItem>,
    ) -> u32 {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Admin);
        Self::require_not_paused(&env);
        if members.len() > MAX_BATCH_MEMBERS {
            panic!("Batch too large");
        }
        Self::extend_instance_ttl(&env);

        let mut members_map: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        let mut current_member_count = 0u32;
        for _ in members_map.iter() {
            current_member_count += 1;
        }

        let mut seen_addrs: Map<Address, bool> = Map::new(&env);
        let mut additions = 0u32;
        for item in members.iter() {
            if item.role == FamilyRole::Owner {
                panic!("Cannot add Owner via batch");
            }
            if seen_addrs.get(item.address.clone()).is_some() {
                panic!("Duplicate member in batch");
            }
            seen_addrs.set(item.address.clone(), true);
            if members_map.get(item.address.clone()).is_some() {
                panic!("Member already exists");
            }
            additions += 1;
        }

        if current_member_count + additions > MAX_FAMILY_MEMBERS {
            panic!("Member cap exceeded");
        }

        let timestamp = env.ledger().timestamp();
        let mut count = 0u32;
        for item in members.iter() {
            members_map.set(
                item.address.clone(),
                FamilyMember {
                    address: item.address.clone(),
                    role: item.role,
                    spending_limit: 0,
                    precision_limit: PrecisionLimitOpt::None,
                    added_at: timestamp,
                },
            );
            Self::append_access_audit(
                &env,
                symbol_short!("add_mem"),
                &caller,
                Some(item.address.clone()),
                true,
            );
            count += 1;
        }
        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members_map);
        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::Medium,
            symbol_short!("batch_mem"),
            count,
        );
        Self::update_storage_stats(&env);
        count
    }

    /// Remove a batch of family members atomically.
    ///
    /// Semantics:
    /// - The whole batch succeeds or the whole batch fails.
    /// - Empty batches are accepted and return `0`.
    /// - Any duplicate address in the batch, missing member, or attempt to remove
    ///   the owner aborts the entire call.
    /// - On success, the return value is the number of members removed.
    pub fn batch_remove_family_members(env: Env, caller: Address, addresses: Vec<Address>) -> u32 {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Owner);
        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));
        if caller != owner {
            panic!("Only Owner can remove members");
        }
        Self::require_not_paused(&env);
        if addresses.len() > MAX_BATCH_MEMBERS {
            panic!("Batch too large");
        }
        Self::extend_instance_ttl(&env);
        let mut members_map: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));

        let mut seen_addrs: Map<Address, bool> = Map::new(&env);
        for addr in addresses.iter() {
            if addr.clone() == owner {
                panic!("Cannot remove owner");
            }
            if seen_addrs.get(addr.clone()).is_some() {
                panic!("Duplicate member in batch");
            }
            seen_addrs.set(addr.clone(), true);
            if members_map.get(addr.clone()).is_none() {
                panic!("Member not found");
            }
        }

        let mut count = 0u32;
        for addr in addresses.iter() {
            members_map.remove(addr.clone());
            Self::append_access_audit(
                &env,
                symbol_short!("rem_mem"),
                &caller,
                Some(addr.clone()),
                true,
            );
            count += 1;
        }
        env.storage()
            .instance()
            .set(&symbol_short!("MEMBERS"), &members_map);
        Self::update_storage_stats(&env);

        // Re-validate in-flight proposals after batch removal: strip signatures
        // from removed members and invalidate proposals that can no longer reach quorum.
        Self::revalidate_proposals_after_membership_change(&env);

        count
    }

    pub fn get_access_audit(env: Env, limit: u32) -> Vec<AccessAuditEntry> {
        let entries: Vec<AccessAuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("ACC_AUDIT"))
            .unwrap_or_else(|| Vec::new(&env));
        let n = entries.len().min(limit);
        let mut out = Vec::new(&env);
        for i in (entries.len().saturating_sub(n))..entries.len() {
            if let Some(e) = entries.get(i) {
                out.push_back(e);
            }
        }
        out
    }

    // Owner/Admin only: audit data is privacy-sensitive — reveals who accessed
    // what and when, so Members are excluded from reading the full trail.
    //
    // ## Pagination cursor semantics
    //
    // `from_index` is the **inclusive** zero-based index of the first entry to
    // return.  `next_cursor` in the returned page is the index to pass as
    // `from_index` on the next call.
    //
    // **Sentinel value:** when `next_cursor == total` (i.e. equals the length
    // of the log at the time of the call) there are no more entries.  Callers
    // MUST stop iterating when `next_cursor >= count` returned by a previous
    // page, or when the returned page is empty.
    //
    // **Clamping rules (no panic on adversarial input):**
    // - `limit == 0`            → silently promoted to `DEFAULT_AUDIT_PAGE_LIMIT`.
    // - `limit > MAX_AUDIT_PAGE_LIMIT` → clamped to `MAX_AUDIT_PAGE_LIMIT`.
    // - `from_index >= total`   → returns an empty page with
    //                             `next_cursor = total` (end-of-log sentinel).
    // - `from_index = u32::MAX` → handled by the `>= total` check above; no
    //                             arithmetic overflow is possible.
    pub fn get_access_audit_page(
        env: Env,
        caller: Address,
        from_index: u32,
        limit: u32,
    ) -> AccessAuditPage {
        caller.require_auth();
        Self::require_role_at_least(&env, &caller, FamilyRole::Admin);

        let entries: Vec<AccessAuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("ACC_AUDIT"))
            .unwrap_or_else(|| Vec::new(&env));

        // Clamp limit: 0 → default, oversized → max.
        let capped_limit = if limit == 0 {
            DEFAULT_AUDIT_PAGE_LIMIT
        } else {
            limit.min(MAX_AUDIT_PAGE_LIMIT)
        };

        let total = entries.len();

        // Out-of-range offset: return empty page with end-of-log sentinel so
        // callers can detect exhaustion without a separate length query.
        if from_index >= total {
            return AccessAuditPage {
                items: Vec::new(&env),
                next_cursor: total, // sentinel: no more entries
                count: 0,
            };
        }

        let mut items = Vec::new(&env);
        // `i` is bounded by `total` (u32), so no overflow risk.
        let mut i = from_index;
        while i < total && items.len() < capped_limit {
            if let Some(e) = entries.get(i) {
                items.push_back(e);
            }
            i += 1;
        }
        let count = items.len();
        // `next_cursor == total` is the end-of-log sentinel.
        // Callers iterate while `next_cursor < total` (or while `count > 0`).
        let next_cursor = i; // equals `total` when the log is exhausted
        AccessAuditPage {
            items,
            next_cursor,
            count,
        }
    }

    /// Manually trigger quorum re-validation for all in-flight proposals.
    ///
    /// This is useful after any membership or multisig-config change to ensure
    /// proposals that can no longer reach quorum are invalidated immediately.
    ///
    /// # Authorization
    /// Owner or Admin only.
    ///
    /// # Returns
    /// The number of proposals that were invalidated (expired early).
    pub fn revalidate_proposals(env: Env, caller: Address) -> u32 {
        caller.require_auth();
        Self::require_not_paused(&env);
        if !Self::is_owner_or_admin(&env, &caller) {
            panic_with_error!(&env, Error::Unauthorized);
        }
        Self::extend_instance_ttl(&env);
        Self::revalidate_proposals_after_membership_change(&env)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Re-validate every in-flight proposal against the current membership and
    /// multisig configuration.
    ///
    /// For each pending proposal this function:
    /// 1. Strips signatures from addresses that are no longer active members.
    /// 2. Checks whether the remaining eligible signers in the multisig config
    ///    can still satisfy the threshold.
    /// 3. If quorum is unachievable, the proposal is invalidated by setting its
    ///    `expires_at` to the current ledger timestamp (effectively expired) and
    ///    emitting a `ProposalInvalidatedEvent`.
    ///
    /// Returns the count of proposals that were invalidated.
    fn revalidate_proposals_after_membership_change(env: &Env) -> u32 {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(env));

        let mut pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| Map::new(env));

        let now = env.ledger().timestamp();
        let mut invalidated_count = 0u32;
        let mut updated_txs: Vec<(u64, PendingTransaction)> = Vec::new(env);

        for (tx_id, mut tx) in pending_txs.iter() {
            // Skip already-expired proposals — they will be cleaned up separately.
            if tx.expires_at <= now {
                continue;
            }

            // --- Step 1: strip signatures from addresses no longer in the wallet ---
            let mut valid_sigs: Vec<Address> = Vec::new(env);
            for sig in tx.signatures.iter() {
                if members.get(sig.clone()).is_some() && !Self::role_has_expired(env, &sig) {
                    valid_sigs.push_back(sig);
                }
            }
            tx.signatures = valid_sigs;

            // --- Step 2: count eligible signers in the multisig config ---
            let config_key = Self::get_config_key(tx.tx_type);
            let config: MultiSigConfig = match env.storage().instance().get(&config_key) {
                Some(c) => c,
                None => {
                    // No config means the proposal can never execute — invalidate it.
                    tx.expires_at = now;
                    invalidated_count += 1;
                    RemitwiseEvents::emit(
                        env,
                        EventCategory::System,
                        EventPriority::High,
                        symbol_short!("inv_prop"),
                        ProposalInvalidatedEvent {
                            tx_id,
                            reason: symbol_short!("no_cfg"),
                            timestamp: now,
                        },
                    );
                    updated_txs.push_back((tx_id, tx));
                    continue;
                }
            };

            // Count how many configured signers are still active members.
            let mut eligible_signers = 0u32;
            for signer in config.signers.iter() {
                if members.get(signer.clone()).is_some() && !Self::role_has_expired(env, &signer) {
                    eligible_signers += 1;
                }
            }

            // --- Step 3: invalidate if quorum is now unachievable ---
            // Quorum is unachievable when the total number of eligible signers
            // (including those who already signed) is less than the threshold.
            // We use `eligible_signers` from the config list because only
            // configured signers are allowed to sign (see `sign_transaction`).
            if eligible_signers < config.threshold {
                tx.expires_at = now;
                invalidated_count += 1;
                RemitwiseEvents::emit(
                    env,
                    EventCategory::System,
                    EventPriority::High,
                    symbol_short!("inv_prop"),
                    ProposalInvalidatedEvent {
                        tx_id,
                        reason: symbol_short!("no_qrm"),
                        timestamp: now,
                    },
                );
            }

            updated_txs.push_back((tx_id, tx));
        }

        // Persist all modified proposals back to storage.
        for i in 0..updated_txs.len() {
            if let Some((tx_id, tx)) = updated_txs.get(i) {
                pending_txs.set(tx_id, tx);
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("PEND_TXS"), &pending_txs);

        invalidated_count
    }

    /// Enforces the emergency transfer daily volume cap and persists the updated `EM_VOL`.
    ///
    /// # Day-boundary rollover
    ///
    /// The window is anchored to **UTC midnight** boundaries derived from `EM_LAST`
    /// (the timestamp of the most-recently completed emergency transfer):
    ///
    /// ```text
    /// is_new_day = (now / 86_400) > (EM_LAST / 86_400)
    /// ```
    ///
    /// When `is_new_day` is true `EM_VOL` is reset to zero before adding `amount`.
    /// This prevents the sliding-window attack where an attacker splits transfers
    /// across an artificial 24-hour boundary to reset the counter early and
    /// effectively drain up to `2 × daily_limit` across two adjacent calls.
    ///
    /// # Checked arithmetic
    ///
    /// `checked_add` is used instead of `saturating_add`.  An `i128` overflow would
    /// silently wrap to a value that passes the cap comparison; `checked_add` panics
    /// instead, treating overflow as a hard protocol error rather than masking it.
    ///
    /// # Panics
    /// - `"Emergency volume arithmetic overflow"` — `current_vol + amount` overflows `i128`.
    /// - `"Emergency daily limit exceeded"` — accumulated volume would exceed `daily_limit`.
    fn check_and_update_emergency_volume(env: &Env, now: u64, amount: i128, daily_limit: i128) {
        const DAY: u64 = 86_400;

        // EM_LAST: timestamp of the last recorded emergency transfer.
        // Initialized to 0 in `init`; 0 places the last transfer at the Unix epoch
        // (day 0), so any transfer at timestamp >= 86_400 triggers a fresh window.
        let last_ts: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("EM_LAST"))
            .unwrap_or(0u64);

        // EM_VOL: accumulated volume for the current UTC day.
        let stored_vol: i128 = env
            .storage()
            .instance()
            .get(&symbol_short!("EM_VOL"))
            .unwrap_or(0i128);

        // Integer division truncates to the start of each UTC day.
        // e.g. 86_399 / 86_400 = 0  (day 0)
        //      86_400 / 86_400 = 1  (day 1) ← triggers reset
        let current_vol = if (now / DAY) > (last_ts / DAY) {
            0i128 // new UTC day — discard previous window's volume
        } else {
            stored_vol
        };

        // checked_add: overflow is a hard error, not a user-correctable condition.
        let new_vol = current_vol
            .checked_add(amount)
            .unwrap_or_else(|| panic!("Emergency volume arithmetic overflow"));

        if new_vol > daily_limit {
            panic!("Emergency daily limit exceeded");
        }

        // Persist updated volume. EM_LAST is written by the caller *after* the
        // token transfer succeeds, so on the next call this helper sees the correct
        // "last transfer day" for rollover detection.
        env.storage()
            .instance()
            .set(&symbol_short!("EM_VOL"), &new_vol);
    }

    fn execute_emergency_transfer_now(
        env: Env,
        proposer: Address,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> u64 {
        let config: EmergencyConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("EM_CONF"))
            .unwrap_or_else(|| panic!("Emergency config not set"));

        if amount > config.max_amount {
            panic!("Emergency amount exceeds maximum allowed");
        }

        let now = env.ledger().timestamp();
        let last_ts: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("EM_LAST"))
            .unwrap_or(0u64);
        if last_ts != 0 && now < last_ts.saturating_add(config.cooldown) {
            panic!("Emergency transfer cooldown period not elapsed");
        }

        // Enforce daily volume cap — correct day-boundary rollover + checked arithmetic.
        Self::check_and_update_emergency_volume(&env, now, amount, config.daily_limit);

        // --- Minimum balance floor -------------------------------------------------
        //
        // Invariant: an emergency transfer must never drain the proposer's balance
        // below `EmergencyConfig.min_balance`. This floor exists so a wallet stays
        // solvent for recurring obligations (bills, premiums) even during an
        // emergency drain; if it were unenforced it would be a purely decorative
        // setting.
        //
        // `min_balance == 0` intentionally disables the floor (any non-negative
        // post-transfer balance is allowed), matching `configure_emergency`'s
        // validation that only rejects *negative* `min_balance` values.
        //
        // TOCTOU safety: this reads `current_balance` from the same `token_client`
        // (same token address) that `execute_transaction_internal` uses to perform
        // the actual transfer below, and no external/cross-contract call happens
        // between this read and that transfer — so there is no window in which the
        // balance could change between the check and the transfer.
        //
        // `checked_sub` (rather than plain `-`) mirrors the daily-volume cap's
        // checked-arithmetic discipline: an overflow/underflow here must surface as
        // a hard error rather than silently wrapping and bypassing the floor.
        let token_client = TokenClient::new(&env, &token);
        let current_balance = token_client.balance(&proposer);
        let post_transfer_balance = current_balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, Error::MinBalanceViolation));
        if post_transfer_balance < config.min_balance {
            panic_with_error!(&env, Error::MinBalanceViolation);
        }

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("em_init"),
            (proposer.clone(), recipient.clone(), amount),
        );

        proposer.require_auth();
        let _ = Self::execute_transaction_internal(
            &env,
            &proposer,
            &TransactionType::EmergencyTransfer,
            &TransactionData::EmergencyTransfer(token.clone(), recipient.clone(), amount),
            false,
        );

        // Avoid storing 0: `get_last_emergency_at` treats 0 as "none", and cooldown logic uses `last_ts != 0`.
        let ts = env.ledger().timestamp();
        let store_ts: u64 = if ts == 0 { 1u64 } else { ts };
        env.storage()
            .instance()
            .set(&symbol_short!("EM_LAST"), &store_ts);

        env.events().publish(
            (symbol_short!("emerg"), EmergencyEvent::TransferExec),
            (proposer.clone(), recipient.clone(), amount),
        );

        Self::append_access_audit(
            &env,
            symbol_short!("em_exec"),
            &proposer,
            Some(recipient.clone()),
            true,
        );

        0
    }

    fn execute_transaction_internal(
        env: &Env,
        proposer: &Address,
        tx_type: &TransactionType,
        data: &TransactionData,
        require_auth: bool,
    ) -> u64 {
        match (tx_type, data) {
            (
                TransactionType::RegularWithdrawal,
                TransactionData::Withdrawal(token, recipient, amount),
            )
            | (
                TransactionType::LargeWithdrawal,
                TransactionData::Withdrawal(token, recipient, amount),
            ) => {
                if require_auth {
                    proposer.require_auth();
                }
                if let Err(e) = Self::validate_precision_spending_internal(
                    env.clone(),
                    proposer.clone(),
                    *amount,
                ) {
                    panic_with_error!(env, e);
                }
                Self::record_precision_spending(env, proposer, *amount);
                let token_client = TokenClient::new(env, token);
                token_client.transfer(proposer, recipient, amount);
                0
            }
            (TransactionType::SplitConfigChange, TransactionData::SplitConfigChange(..)) => 0,
            (TransactionType::RoleChange, TransactionData::RoleChange(member, new_role)) => {
                let mut members: Map<Address, FamilyMember> = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("MEMBERS"))
                    .unwrap_or_else(|| panic!("Wallet not initialized"));

                if let Some(mut member_data) = members.get(member.clone()) {
                    member_data.role = *new_role;
                    members.set(member.clone(), member_data);
                    env.storage()
                        .instance()
                        .set(&symbol_short!("MEMBERS"), &members);
                    Self::append_access_audit(
                        env,
                        symbol_short!("role_chg"),
                        proposer,
                        Some(member.clone()),
                        true,
                    );
                }
                0
            }
            (
                TransactionType::EmergencyTransfer,
                TransactionData::EmergencyTransfer(token, recipient, amount),
            ) => {
                if require_auth {
                    proposer.require_auth();
                }
                let token_client = TokenClient::new(env, token);
                token_client.transfer(proposer, recipient, amount);
                0
            }
            (TransactionType::PolicyCancellation, TransactionData::PolicyCancellation(..)) => 0,
            _ => panic!("Invalid transaction type or data mismatch"),
        }
    }

    fn get_config_key(tx_type: TransactionType) -> Symbol {
        match tx_type {
            TransactionType::LargeWithdrawal => symbol_short!("MS_WDRAW"),
            TransactionType::SplitConfigChange => symbol_short!("MS_SPLIT"),
            TransactionType::RoleChange => symbol_short!("MS_ROLE"),
            TransactionType::EmergencyTransfer => symbol_short!("MS_EMERG"),
            TransactionType::PolicyCancellation => symbol_short!("MS_POL"),
            TransactionType::RegularWithdrawal => symbol_short!("MS_REG"),
        }
    }

    fn is_family_member(env: &Env, address: &Address) -> bool {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(env));

        members.get(address.clone()).is_some()
    }

    fn is_owner_or_admin(env: &Env, address: &Address) -> bool {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(env));

        Self::is_owner_or_admin_in_members(env, &members, address)
    }

    fn is_owner_or_admin_in_members(
        env: &Env,
        members: &Map<Address, FamilyMember>,
        address: &Address,
    ) -> bool {
        if let Some(member) = members.get(address.clone()) {
            if Self::role_has_expired(env, address) {
                false
            } else {
                matches!(member.role, FamilyRole::Owner | FamilyRole::Admin)
            }
        } else {
            false
        }
    }

    fn role_ordinal(role: FamilyRole) -> u32 {
        role as u32
    }

    fn get_role_expiry(env: &Env, address: &Address) -> Option<u64> {
        env.storage()
            .instance()
            .get::<_, Map<Address, u64>>(&symbol_short!("ROLE_EXP"))
            .unwrap_or_else(|| Map::new(env))
            .get(address.clone())
    }

    fn role_has_expired(env: &Env, address: &Address) -> bool {
        if let Some(exp) = Self::get_role_expiry(env, address) {
            env.ledger().timestamp() >= exp
        } else {
            false
        }
    }

    fn require_role_at_least(env: &Env, caller: &Address, min_role: FamilyRole) {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| panic!("Wallet not initialized"));
        let member = members
            .get(caller.clone())
            .unwrap_or_else(|| panic!("Not a family member"));
        if Self::role_has_expired(env, caller) {
            panic!("Role has expired");
        }
        if Self::role_ordinal(member.role) > Self::role_ordinal(min_role) {
            panic!("Insufficient role");
        }
    }

    /// Helper to enforce role expiry on admin-level operations.
    ///
    /// Combines authorization check with expiry validation in a single call,
    /// ensuring expired admins cannot perform privileged operations.
    /// This helper is documented as a pattern for future admin-gated operations.
    #[allow(dead_code)]
    fn require_not_expired_admin(env: &Env, caller: &Address) {
        if !Self::is_owner_or_admin(env, caller) {
            panic!("Only Owner or Admin can perform this operation");
        }
        if Self::role_has_expired(env, caller) {
            panic!("Role has expired");
        }
    }

    fn append_access_audit(
        env: &Env,
        operation: Symbol,
        caller: &Address,
        target: Option<Address>,
        success: bool,
    ) {
        let mut entries: Vec<AccessAuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("ACC_AUDIT"))
            .unwrap_or_else(|| Vec::new(env));
        entries.push_back(AccessAuditEntry {
            operation,
            caller: caller.clone(),
            target,
            timestamp: env.ledger().timestamp(),
            success,
        });
        let n = entries.len();
        if n > MAX_ACCESS_AUDIT_ENTRIES {
            let mut v = Vec::new(env);
            let start = n - MAX_ACCESS_AUDIT_ENTRIES;
            for i in start..n {
                v.push_back(entries.get(i).unwrap_or_else(|| panic!("Item not found")));
            }
            entries = v;
        }
        env.storage()
            .instance()
            .set(&symbol_short!("ACC_AUDIT"), &entries);
    }

    fn get_pause_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
    }

    fn get_global_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("PAUSED"))
            .unwrap_or(false)
    }

    fn require_not_paused(env: &Env) {
        if Self::get_global_paused(env) {
            panic!("Contract is paused");
        }
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn extend_archive_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
    }

    fn update_storage_stats(env: &Env) {
        let pending_txs: Map<u64, PendingTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_TXS"))
            .unwrap_or_else(|| Map::new(env));

        let archived: Map<u64, ArchivedTransaction> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_TX"))
            .unwrap_or_else(|| Map::new(env));

        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(env));

        let mut pending_count = 0u32;
        for _ in pending_txs.iter() {
            pending_count += 1;
        }

        let mut archived_count = 0u32;
        for _ in archived.iter() {
            archived_count += 1;
        }

        let mut member_count = 0u32;
        for _ in members.iter() {
            member_count += 1;
        }

        let stats = StorageStats {
            pending_transactions: pending_count,
            archived_transactions: archived_count,
            total_members: member_count,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&symbol_short!("STOR_STAT"), &stats);
    }
}

#[cfg(test)]
mod events_schema_test;
#[cfg(test)]
mod test;
