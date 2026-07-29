#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Bill data structure with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    BillNotFound = 1,
    BillAlreadyPaid = 2,
    InvalidAmount = 3,
    InvalidFrequency = 4,
    Unauthorized = 5,
    AdminNotInitialized = 6,
    AdminAlreadyInitialized = 7,
    NoPendingRotation = 8,
    TimelockNotElapsed = 9,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum BillEvent {
    Created,
    Paid,
}

/// Seconds an admin rotation must sit proposed before it can be finalized.
///
/// ## Why a timelock
///
/// Admin rotation is a two-step, delayed process (`propose_admin_rotation`
/// then, once the timelock has elapsed, `finalize_admin_rotation`) rather
/// than an instant one-step handoff. If the current admin's key is ever
/// compromised, an attacker who calls `propose_admin_rotation` does not
/// walk away with control -- the rotation just sits pending, publicly
/// visible via `get_pending_admin_rotation`, for this many seconds before
/// it can take effect. That window gives the legitimate admin (or anyone
/// watching `AdminEvent::RotationProposed`) time to notice the proposal
/// and respond, rather than a single signature being an irreversible,
/// instant takeover.
const ADMIN_ROTATION_TIMELOCK_SECONDS: u64 = 2 * 86400; // 2 days

/// A rotation that has been proposed but not yet finalized.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PendingAdminRotation {
    pub new_admin: Address,
    /// Ledger timestamp at/after which `finalize_admin_rotation` may run.
    pub executable_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum AdminEvent {
    Initialized,
    RotationProposed,
    RotationFinalized,
}

#[contract]
pub struct BillPayments;

#[contractimpl]
impl BillPayments {
    /// Create a new bill
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner (must authorize)
    /// * `name` - Name of the bill (e.g., "Electricity", "School Fees")
    /// * `amount` - Amount to pay (must be positive)
    /// * `due_date` - Due date as Unix timestamp
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Frequency in days for recurring bills (must be > 0 if recurring)
    ///
    /// # Returns
    /// The ID of the created bill
    ///
    /// # Errors
    /// * `InvalidAmount` - If amount is zero or negative
    /// * `InvalidFrequency` - If recurring is true but frequency_days is 0
    pub fn create_bill(
        env: Env,
        owner: Address,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
    ) -> Result<u32, Error> {
        // Access control: require owner authorization
        owner.require_auth();

        // Validate inputs
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        if recurring && frequency_days == 0 {
            return Err(Error::InvalidFrequency);
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        let current_time = env.ledger().timestamp();
        let bill = Bill {
            id: next_id,
            owner: owner.clone(),
            name: name.clone(),
            amount,
            due_date,
            recurring,
            frequency_days,
            paid: false,
            created_at: current_time,
            paid_at: None,
        };

        let bill_owner = bill.owner.clone();
        bills.set(next_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Created),
            (next_id, bill_owner),
        );

        Ok(next_id)
    }

    /// Mark a bill as paid
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the bill owner)
    /// * `bill_id` - ID of the bill
    ///
    /// # Returns
    /// Ok(()) if payment was successful
    ///
    /// # Errors
    /// * `BillNotFound` - If bill with given ID doesn't exist
    /// * `BillAlreadyPaid` - If bill is already marked as paid
    /// * `Unauthorized` - If caller is not the bill owner
    pub fn pay_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), Error> {
        // Access control: require caller authorization
        caller.require_auth();

        // Extend storage TTL
        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).ok_or(Error::BillNotFound)?;

        // Access control: verify caller is the owner
        if bill.owner != caller {
            return Err(Error::Unauthorized);
        }

        if bill.paid {
            return Err(Error::BillAlreadyPaid);
        }

        let current_time = env.ledger().timestamp();
        bill.paid = true;
        bill.paid_at = Some(current_time);

        // If recurring, create next bill
        if bill.recurring {
            let next_due_date = bill.due_date + (bill.frequency_days as u64 * 86400);
            let next_id = env
                .storage()
                .instance()
                .get(&symbol_short!("NEXT_ID"))
                .unwrap_or(0u32)
                + 1;

            let next_bill = Bill {
                id: next_id,
                owner: bill.owner.clone(),
                name: bill.name.clone(),
                amount: bill.amount,
                due_date: next_due_date,
                recurring: true,
                frequency_days: bill.frequency_days,
                paid: false,
                created_at: current_time,
                paid_at: None,
            };
            bills.set(next_id, next_bill);
            env.storage()
                .instance()
                .set(&symbol_short!("NEXT_ID"), &next_id);
        }

        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        // Emit event for audit trail
        env.events()
            .publish((symbol_short!("bill"), BillEvent::Paid), (bill_id, caller));

        Ok(())
    }

    /// Get a bill by ID
    ///
    /// # Arguments
    /// * `bill_id` - ID of the bill
    ///
    /// # Returns
    /// Bill struct or None if not found
    pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        bills.get(bill_id)
    }

    /// Get all unpaid bills for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner
    ///
    /// # Returns
    /// Vec of unpaid Bill structs belonging to the owner
    pub fn get_unpaid_bills(env: Env, owner: Address) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        let max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        for i in 1..=max_id {
            if let Some(bill) = bills.get(i) {
                if !bill.paid && bill.owner == owner {
                    result.push_back(bill);
                }
            }
        }
        result
    }

    /// Get all overdue unpaid bills
    ///
    /// # Returns
    /// Vec of unpaid bills that are past their due date
    pub fn get_overdue_bills(env: Env) -> Vec<Bill> {
        let current_time = env.ledger().timestamp();
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        let max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        for i in 1..=max_id {
            if let Some(bill) = bills.get(i) {
                if !bill.paid && bill.due_date < current_time {
                    result.push_back(bill);
                }
            }
        }
        result
    }

    /// Get total amount of unpaid bills for a specific owner
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner
    ///
    /// # Returns
    /// Total amount of all unpaid bills belonging to the owner
    pub fn get_total_unpaid(env: Env, owner: Address) -> i128 {
        let unpaid = Self::get_unpaid_bills(env, owner);
        let mut total = 0i128;
        for bill in unpaid.iter() {
            total += bill.amount;
        }
        total
    }

    /// Cancel/delete a bill
    ///
    /// # Arguments
    /// * `bill_id` - ID of the bill to cancel
    ///
    /// # Returns
    /// Ok(()) if cancellation was successful
    ///
    /// # Errors
    /// * `BillNotFound` - If bill with given ID doesn't exist
    pub fn cancel_bill(env: Env, bill_id: u32) -> Result<(), Error> {
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        if bills.get(bill_id).is_none() {
            return Err(Error::BillNotFound);
        }

        bills.remove(bill_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        Ok(())
    }

    /// Get all bills (paid and unpaid)
    ///
    /// # Returns
    /// Vec of all Bill structs
    pub fn get_all_bills(env: Env) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        let max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        for i in 1..=max_id {
            if let Some(bill) = bills.get(i) {
                result.push_back(bill);
            }
        }
        result
    }

    /// One-time admin setup.
    ///
    /// # Errors
    /// * `AdminAlreadyInitialized` - If an admin has already been set
    pub fn init_admin(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();

        if env.storage().instance().has(&symbol_short!("ADMIN")) {
            return Err(Error::AdminAlreadyInitialized);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &admin);
        env.events()
            .publish((symbol_short!("admin"), AdminEvent::Initialized), admin);

        Ok(())
    }

    /// Propose rotating the admin to `new_admin`. Does not take effect
    /// immediately -- see `ADMIN_ROTATION_TIMELOCK_SECONDS`. Call
    /// `finalize_admin_rotation` after the timelock elapses to complete it.
    /// A second call before finalization overwrites the still-pending
    /// proposal (and restarts its timelock) rather than stacking.
    ///
    /// # Errors
    /// * `AdminNotInitialized` - If no admin has been set yet
    /// * `Unauthorized` - If caller is not the current admin
    pub fn propose_admin_rotation(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), Error> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(Error::AdminNotInitialized)?;

        if admin != caller {
            return Err(Error::Unauthorized);
        }

        let executable_at = env.ledger().timestamp() + ADMIN_ROTATION_TIMELOCK_SECONDS;
        let pending = PendingAdminRotation {
            new_admin: new_admin.clone(),
            executable_at,
        };
        env.storage()
            .instance()
            .set(&symbol_short!("PENDROT"), &pending);

        env.events().publish(
            (symbol_short!("admin"), AdminEvent::RotationProposed),
            (new_admin, executable_at),
        );

        Ok(())
    }

    /// Finalize a previously proposed admin rotation, once its timelock
    /// has elapsed. Callable by anyone -- the timelock, not the caller
    /// identity, is what gates this taking effect.
    ///
    /// # Errors
    /// * `NoPendingRotation` - If no rotation has been proposed
    /// * `TimelockNotElapsed` - If called before `executable_at`
    pub fn finalize_admin_rotation(env: Env) -> Result<(), Error> {
        let pending: PendingAdminRotation = env
            .storage()
            .instance()
            .get(&symbol_short!("PENDROT"))
            .ok_or(Error::NoPendingRotation)?;

        if env.ledger().timestamp() < pending.executable_at {
            return Err(Error::TimelockNotElapsed);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &pending.new_admin);
        env.storage().instance().remove(&symbol_short!("PENDROT"));

        env.events().publish(
            (symbol_short!("admin"), AdminEvent::RotationFinalized),
            pending.new_admin,
        );

        Ok(())
    }

    /// Get the current admin, or `None` if `init_admin` hasn't run yet.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("ADMIN"))
    }

    /// Get the pending rotation, if one has been proposed and not yet
    /// finalized.
    pub fn get_pending_admin_rotation(env: Env) -> Option<PendingAdminRotation> {
        env.storage().instance().get(&symbol_short!("PENDROT"))
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

mod test;
