#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token::TokenClient, vec, Address, Env,
    Symbol, Vec,
};

#[derive(Clone)]
#[contracttype]
pub struct Allocation {
    pub category: Symbol,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AccountGroup {
    pub spending: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
}

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Split configuration with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub initialized: bool,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone)]
pub enum SplitEvent {
    Initialized,
    Updated,
    Calculated,
}

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
    ///
    /// # Arguments
    /// * `owner` - Address of the split owner (must authorize)
    /// * `spending_percent` - Percentage for spending (0-100)
    /// * `savings_percent` - Percentage for savings (0-100)
    /// * `bills_percent` - Percentage for bills (0-100)
    /// * `insurance_percent` - Percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if initialization was successful
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If percentages don't sum to 100
    /// - If split is already initialized (use update_split instead)
    pub fn initialize_split(
        env: Env,
        owner: Address,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        // Access control: require owner authorization
        owner.require_auth();

        // Check if already initialized
        let existing: Option<SplitConfig> = env.storage().instance().get(&symbol_short!("CONFIG"));

        if existing.is_some() {
            panic!("Split already initialized. Use update_split to modify.");
        }

        // Input validation: percentages must sum to 100
        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            panic!("Percentages must sum to 100");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        let config = SplitConfig {
            owner: owner.clone(),
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            initialized: true,
        };

        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &config);

        // Also store the split vector for backward compatibility
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        // Emit event for audit trail
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Initialized), owner);

        true
    }

    /// Update an existing split configuration
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the owner)
    /// * `spending_percent` - New percentage for spending (0-100)
    /// * `savings_percent` - New percentage for savings (0-100)
    /// * `bills_percent` - New percentage for bills (0-100)
    /// * `insurance_percent` - New percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if update was successful
    ///
    /// # Panics
    /// - If caller is not the owner
    /// - If percentages don't sum to 100
    /// - If split is not initialized
    pub fn update_split(
        env: Env,
        caller: Address,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        // Access control: require caller authorization
        caller.require_auth();

        // Get existing config
        let mut config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .expect("Split not initialized");

        // Access control: verify caller is the owner
        if config.owner != caller {
            panic!("Only the owner can update the split configuration");
        }

        // Input validation: percentages must sum to 100
        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            panic!("Percentages must sum to 100");
        }

        // Extend storage TTL
        Self::extend_instance_ttl(&env);

        // Update config
        config.spending_percent = spending_percent;
        config.savings_percent = savings_percent;
        config.bills_percent = bills_percent;
        config.insurance_percent = insurance_percent;

        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &config);

        // Also update the split vector for backward compatibility
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        // Emit event for audit trail
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Updated), caller);

        true
    }

    /// Get the current split configuration
    ///
    /// # Returns
    /// Vec containing [spending, savings, bills, insurance] percentages
    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![env, 50, 30, 15, 5])
    }

    /// Get the full split configuration including owner
    ///
    /// # Returns
    /// SplitConfig or None if not initialized
    pub fn get_config(env: Env) -> Option<SplitConfig> {
        env.storage().instance().get(&symbol_short!("CONFIG"))
    }

    /// Calculate split amounts from a total remittance amount
    ///
    /// # Arguments
    /// * `total_amount` - The total amount to split (must be positive)
    ///
    /// # Returns
    /// Vec containing [spending, savings, bills, insurance] amounts
    ///
    /// # Panics
    /// - If total_amount is not positive
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        // Input validation
        if total_amount <= 0 {
            panic!("Total amount must be positive");
        }

        let split = Self::get_split(&env);

        let spending = (total_amount * split.get(0).unwrap() as i128) / 100;
        let savings = (total_amount * split.get(1).unwrap() as i128) / 100;
        let bills = (total_amount * split.get(2).unwrap() as i128) / 100;
        // Insurance gets the remainder to handle rounding
        let insurance = total_amount - spending - savings - bills;

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("split"), SplitEvent::Calculated),
            total_amount,
        );

        vec![&env, spending, savings, bills, insurance]
    }

    /// Distribute USDC according to the configured split
    pub fn distribute_usdc(
        env: Env,
        usdc_contract: Address,
        from: Address,
        accounts: AccountGroup,
        total_amount: i128,
    ) -> bool {
        if total_amount <= 0 {
            return false;
        }

        from.require_auth();

        let amounts = Self::calculate_split(env.clone(), total_amount);
        let recipients = [
            accounts.spending,
            accounts.savings,
            accounts.bills,
            accounts.insurance,
        ];
        let token = TokenClient::new(&env, &usdc_contract);

        for (amount, recipient) in amounts.into_iter().zip(recipients.iter()) {
            if amount > 0 {
                token.transfer(&from, recipient, &amount);
            }
        }

        true
    }

    /// Query USDC balance for an address
    pub fn get_usdc_balance(env: &Env, usdc_contract: Address, account: Address) -> i128 {
        TokenClient::new(env, &usdc_contract).balance(&account)
    }

    /// Returns a breakdown of the split by category and resulting amount
    pub fn get_split_allocations(env: &Env, total_amount: i128) -> Vec<Allocation> {
        let amounts = Self::calculate_split(env.clone(), total_amount);
        let categories = [
            symbol_short!("SPENDING"),
            symbol_short!("SAVINGS"),
            symbol_short!("BILLS"),
            symbol_short!("INSURANCE"),
        ];

        let mut result = Vec::new(env);
        for (category, amount) in categories.into_iter().zip(amounts.into_iter()) {
            result.push_back(Allocation { category, amount });
        }
        result
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

mod test;
