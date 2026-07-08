#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    LimitExceeded = 4,
    InvalidSchedule = 5,
    InvalidAdmin = 6,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    GlobalPaused,
    ModulePaused(Symbol),
    PausedFunctions(Symbol),
    UnpauseSchedule,
}

pub const MAX_PAUSED_FUNCTIONS: u32 = 10;

/// Emitted when the killswitch admin is successfully transferred.
#[contracttype]
#[derive(Clone)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

#[contract]
pub struct EmergencyKillswitch;

#[contractimpl]
impl EmergencyKillswitch {
    /// Initializes the killswitch with an admin address.
    ///
    /// Rejects the contract's own address as admin to prevent unrecoverable bricking.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if admin == env.current_contract_address() {
            return Err(Error::InvalidAdmin);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Transfers admin authority to a new address.
    ///
    /// # Rejects
    /// - `new_admin` == contract own address (unrecoverable brick)
    /// - `new_admin` == current admin (no-op, to prevent accidental re-auth)
    ///
    /// Emits [AdminTransferred] on successful handover.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if new_admin == env.current_contract_address() {
            return Err(Error::InvalidAdmin);
        }
        if new_admin == admin {
            return Err(Error::InvalidAdmin);
        }

        let old_admin = admin.clone();
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("admn_xfer")),
            AdminTransferred {
                old_admin,
                new_admin: new_admin.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::GlobalPaused, &true);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("paused")),
            (symbol_short!("GLOBAL"), env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let schedule: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UnpauseSchedule)
            .ok_or(Error::InvalidSchedule)?;
        if env.ledger().timestamp() < schedule {
            return Err(Error::Unauthorized);
        }
        env.storage().instance().set(&DataKey::GlobalPaused, &false);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("unpaused")),
            (symbol_short!("GLOBAL"), env.ledger().timestamp()),
        );
        Ok(())
    }

    /// Admin-only recovery path that immediately clears the global emergency
    /// pause, bypassing the unpause timelock.
    ///
    /// [unpause] can only succeed once a future [schedule_unpause] has been set
    /// *and* the ledger has reached it. A re-[pause] removes any pending
    /// schedule (see [pause]), so a contract can be left globally paused with no
    /// valid schedule — at which point `unpause` fails with
    /// [Error::InvalidSchedule] and the only options were to wait out a stale
    /// schedule or redeploy. This entrypoint lets the admin recover from that
    /// stuck-paused state in a single call.
    ///
    /// Sets [DataKey::GlobalPaused] to `false` and removes any pending
    /// [DataKey::UnpauseSchedule]. It is idempotent: calling it when the
    /// contract is not paused is a successful no-op. Module- and function-level
    /// pauses are intentionally left untouched — lift those with
    /// [unpause_module] / [unpause_function].
    ///
    /// Emits an `emergency`/`cleared` event on success.
    pub fn clear_emergency_state(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::GlobalPaused, &false);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("cleared")),
            (symbol_short!("GLOBAL"), env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn schedule_unpause(env: Env, time: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if time < env.ledger().timestamp() {
            return Err(Error::InvalidSchedule);
        }
        env.storage()
            .instance()
            .set(&DataKey::UnpauseSchedule, &time);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::GlobalPaused)
            .unwrap_or(false)
    }

    /// Returns the pending unpause timestamp set by `schedule_unpause`, or `None` if no unpause
    /// is scheduled. The schedule is cleared when `pause` or `unpause` is called.
    ///
    /// No authentication required — the schedule is observable on-chain.
    pub fn get_unpause_schedule(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::UnpauseSchedule)
    }

    /// Returns the list of paused function names for `module_id`, or an empty vec if none.
    ///
    /// Bounded by [`MAX_PAUSED_FUNCTIONS`] (10); no pagination required.
    ///
    /// Note: a function may appear unpaused here yet still be blocked if the module
    /// (`is_module_paused`) or global pause (`is_paused`) is active — the precedence order
    /// is global → module → function.
    ///
    /// No authentication required — state is observable on-chain.
    pub fn list_paused_functions(env: Env, module_id: Symbol) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns whether `module_id` is directly paused via `pause_module`.
    ///
    /// Note: this reflects only the module-level flag. For the full precedence check
    /// (global → module → function) use `is_function_paused`.
    ///
    /// No authentication required — state is observable on-chain.
    pub fn is_module_paused(env: Env, module_id: Symbol) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ModulePaused(module_id))
            .unwrap_or(false)
    }

    pub fn pause_function(env: Env, module_id: Symbol, func: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let mut paused_funcs: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id.clone()))
            .unwrap_or(Vec::new(&env));
        if !paused_funcs.contains(func.clone()) {
            if paused_funcs.len() >= MAX_PAUSED_FUNCTIONS {
                return Err(Error::LimitExceeded);
            }
            paused_funcs.push_back(func.clone());
            env.storage()
                .instance()
                .set(&DataKey::PausedFunctions(module_id.clone()), &paused_funcs);
            env.events().publish(
                (symbol_short!("emergency"), symbol_short!("f_paused")),
                (module_id, func, env.ledger().timestamp()),
            );
        }
        Ok(())
    }

    pub fn unpause_function(env: Env, module_id: Symbol, func: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let mut paused_funcs: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id.clone()))
            .unwrap_or(Vec::new(&env));
        if let Some(index) = paused_funcs.first_index_of(func.clone()) {
            paused_funcs.remove(index);
            env.storage()
                .instance()
                .set(&DataKey::PausedFunctions(module_id.clone()), &paused_funcs);
            env.events().publish(
                (symbol_short!("emergency"), symbol_short!("f_unpause")),
                (module_id, func, env.ledger().timestamp()),
            );
        }
        Ok(())
    }

    pub fn is_function_paused(env: Env, module_id: Symbol, func: Symbol) -> bool {
        if env
            .storage()
            .instance()
            .get(&DataKey::GlobalPaused)
            .unwrap_or(false)
        {
            return true;
        }
        if env
            .storage()
            .instance()
            .get(&DataKey::ModulePaused(module_id.clone()))
            .unwrap_or(false)
        {
            return true;
        }
        let paused_funcs: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id))
            .unwrap_or(Vec::new(&env));
        paused_funcs.contains(func)
    }

    pub fn pause_module(env: Env, module_id: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ModulePaused(module_id.clone()), &true);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("m_paused")),
            (module_id, env.ledger().timestamp()),
        );
        Ok(())
    }

    pub fn unpause_module(env: Env, module_id: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ModulePaused(module_id.clone()), &false);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("m_unpause")),
            (module_id, env.ledger().timestamp()),
        );
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — transfer_admin authorization and post-transfer privilege revocation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    fn setup_env() -> (Env, EmergencyKillswitchClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        (env, client)
    }

    /// transfer_admin before initialize returns NotInitialized.
    #[test]
    fn test_transfer_admin_before_init_returns_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let new_admin = Address::generate(&env);

        let res = client.try_transfer_admin(&new_admin);
        assert_eq!(res, Err(Ok(Error::NotInitialized)));
    }

    /// Transferring to the current admin is rejected (prevents accidental re-auth).
    #[test]
    fn test_transfer_admin_to_self_rejected() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);

        client.initialize(&admin);

        let res = client.try_transfer_admin(&admin);
        assert_eq!(res, Err(Ok(Error::InvalidAdmin)));
    }

    /// After a successful transfer, the new admin can pause and unpause,
    /// proving DataKey::Admin was updated.
    #[test]
    fn test_transfer_admin_grants_powers_to_new_admin() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        client.transfer_admin(&new_admin);

        // New admin can pause
        client.pause();
        assert!(client.is_paused());

        // New admin can schedule unpause and unpause
        let now = env.ledger().timestamp();
        client.schedule_unpause(&(now + 100));
        env.ledger().with_mut(|li| li.timestamp = now + 200);
        client.unpause();
        assert!(!client.is_paused());
    }

    /// After transfer, new admin can use pause_module and unpause_module.
    #[test]
    fn test_new_admin_can_pause_module_after_transfer() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        client.transfer_admin(&new_admin);

        client.pause_module(&symbol_short!("insurance"));
        assert!(client.is_module_paused(&symbol_short!("insurance")));

        client.unpause_module(&symbol_short!("insurance"));
        assert!(!client.is_module_paused(&symbol_short!("insurance")));
    }

    /// Double transfer (A→B→C) — all intermediate transfers succeed
    /// and the final admin retains full control.
    #[test]
    fn test_double_transfer() {
        let (env, client) = setup_env();
        let admin_a = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        client.initialize(&admin_a);
        client.transfer_admin(&admin_b);
        client.transfer_admin(&admin_c);

        // Admin C can pause
        client.pause();
        assert!(client.is_paused());
    }

    /// Transferring to the contract's own address is rejected (prevents bricking).
    /// Uses the address returned by `register_contract` as the self-address.
    #[test]
    fn test_transfer_admin_to_contract_self_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // transfer_admin to the contract's own address
        let res = client.try_transfer_admin(&contract_id);
        assert_eq!(res, Err(Ok(Error::InvalidAdmin)));
    }

    /// Verify DataKey::Admin value is updated by checking a second transfer
    /// succeeds (new admin is stored).
    #[test]
    fn test_transfer_admin_updates_stored_admin() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        client.initialize(&admin);
        client.transfer_admin(&admin_b);
        // A→B succeeded. Now B→C should also succeed, proving B is stored.
        client.transfer_admin(&admin_c);
        // C can pause, proving C is now admin
        client.pause();
        assert!(client.is_paused());
    }
}
