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
    /// Set or update the split percentages used to allocate remittances.
    ///
    /// # Arguments
    /// * `spending_percent` - Percent allocated to spending
    /// * `savings_percent` - Percent allocated to savings
    /// * `bills_percent` - Percent allocated to bills
    /// * `insurance_percent` - Percent allocated to insurance
    ///
    /// # Returns
    /// `true` when the inputs are valid and stored, `false` otherwise.
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
        if !Self::is_valid_split(
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
        ) {
            panic!("Percentages must sum to 100 and be valid");
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
        if !Self::is_valid_split(
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
        ) {
            panic!("Percentages must sum to 100 and be valid");
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
            .unwrap_or_else(|| vec![&env, 50, 30, 15, 5])
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

        let spending = Self::split_amount(total_amount, split.get(0).unwrap());
        let savings = Self::split_amount(total_amount, split.get(1).unwrap());
        let bills = Self::split_amount(total_amount, split.get(2).unwrap());
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

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        token::{StellarAssetClient, TokenClient},
        Env,
    };

    #[test]
    fn distribute_usdc_apportions_tokens_to_recipients() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
        let payer = Address::generate(&env);
        let amount = 1_000i128;

        StellarAssetClient::new(&env, &token_contract.address()).mint(&payer, &amount);

        let spending = Address::generate(&env);
        let savings = Address::generate(&env);
        let bills = Address::generate(&env);
        let insurance = Address::generate(&env);

        let accounts = AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        };

        let distributed =
            client.distribute_usdc(&token_contract.address(), &payer, &accounts, &amount);

        assert!(distributed);

        let token_client = TokenClient::new(&env, &token_contract.address());
        assert_eq!(token_client.balance(&spending), 500);
        assert_eq!(token_client.balance(&savings), 300);
        assert_eq!(token_client.balance(&bills), 150);
        assert_eq!(token_client.balance(&insurance), 50);
        assert_eq!(token_client.balance(&payer), 0);
    }

    #[test]
    fn split_allocations_report_categories_and_amounts() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        let total_amount = 2000i128;
        let allocations = client.get_split_allocations(&total_amount);

        assert_eq!(allocations.len(), 4);
        let expected_amounts = [1000, 600, 300, 100];
        let categories = [
            symbol_short!("SPENDING"),
            symbol_short!("SAVINGS"),
            symbol_short!("BILLS"),
            symbol_short!("INSURANCE"),
        ];

        for i in 0..4 {
            let allocation = allocations.get(i).unwrap();
            let idx = i as usize;
            assert_eq!(allocation.amount, expected_amounts[idx]);
            assert_eq!(allocation.category, categories[idx]);
        }
    }
}
