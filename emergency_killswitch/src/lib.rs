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
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    GlobalPaused,
    ModulePaused(Symbol),
    PausedFunctions(Symbol), // Symbol is module_id, maps to Vec of paused functions
    UnpauseSchedule,
}

pub const MAX_PAUSED_FUNCTIONS: u32 = 10;

#[contract]
pub struct EmergencyKillswitch;

#[contractimpl]
impl EmergencyKillswitch {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Pauses the contract globally.
    /// Invariant: A new pause cancels any pending schedule.
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::GlobalPaused, &true);

        // Cancel any pending unpause schedule on new pause
        env.storage().instance().remove(&DataKey::UnpauseSchedule);

        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("paused")),
            (symbol_short!("GLOBAL"), env.ledger().timestamp()),
        );
        Ok(())
    }

    /// Lifts the global pause state.
    /// Invariant: An unpause cannot take effect before the scheduled time.
    /// Enforces env.ledger().timestamp() >= scheduled_time.
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

    /// Records a future unpause time.
    /// Invariant: The timelock cannot be bypassed by re-calling schedule_unpause with a past timestamp.
    /// Rejects past-dated schedules (time < env.ledger().timestamp()).
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

    // --- Issue #501: Per-function pause flags ---

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

#[cfg(test)]
mod transfer_admin_tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{
        symbol_short,
        testutils::{
            Address as _, AuthorizedFunction, AuthorizedInvocation, Ledger, MockAuth,
            MockAuthInvoke,
        },
        IntoVal, Val,
    };

    fn setup(env: &Env) -> (Address, EmergencyKillswitchClient<'_>) {
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(env, &contract_id);
        (contract_id, client)
    }

    fn assert_auth(
        env: &Env,
        contract_id: &Address,
        address: &Address,
        fn_name: &str,
        args: soroban_sdk::Vec<Val>,
    ) {
        assert_eq!(
            env.auths(),
            std::vec![(
                address.clone(),
                AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        contract_id.clone(),
                        Symbol::new(env, fn_name),
                        args
                    )),
                    sub_invocations: std::vec![],
                }
            )]
        );
    }

    fn mock_no_arg_auth(
        env: &Env,
        client: &EmergencyKillswitchClient,
        contract_id: &Address,
        signer: &Address,
        fn_name: &'static str,
    ) {
        client.mock_auths(&[MockAuth {
            address: signer,
            invoke: &MockAuthInvoke {
                contract: contract_id,
                fn_name,
                args: ().into_val(env),
                sub_invokes: &[],
            },
        }]);
    }

    fn stored_admin(env: &Env, contract_id: &Address) -> Address {
        env.as_contract(contract_id, || {
            env.storage()
                .instance()
                .get(&DataKey::Admin)
                .expect("admin must be stored")
        })
    }

    fn disable_mock_all_auths(env: &Env) {
        env.set_auths(&[]);
    }

    /// Verifies transfer_admin fails closed before initialize stores DataKey::Admin.
    #[test]
    fn transfer_admin_before_initialize_returns_not_initialized() {
        let env = Env::default();
        let (_contract_id, client) = setup(&env);
        let new_admin = Address::generate(&env);

        assert_eq!(
            client.try_transfer_admin(&new_admin),
            Err(Ok(Error::NotInitialized))
        );
    }

    /// Verifies only the current DataKey::Admin authorization can rotate the admin slot.
    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn transfer_admin_rejects_non_admin_auth() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        client.mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "transfer_admin",
                args: (&new_admin,).into_val(&env),
                sub_invokes: &[],
            },
        }]);

        client.transfer_admin(&new_admin);
    }

    /// Verifies DataKey::Admin is replaced and the new admin can use every pause surface.
    #[test]
    fn transfer_admin_grants_new_admin_pause_powers_and_updates_storage() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let module = symbol_short!("bill");
        let func = symbol_short!("pay");

        client.initialize(&admin);
        env.mock_all_auths();
        client.transfer_admin(&new_admin);
        assert_auth(
            &env,
            &contract_id,
            &admin,
            "transfer_admin",
            (&new_admin,).into_val(&env),
        );

        let stored_admin = stored_admin(&env, &contract_id);
        assert_eq!(stored_admin, new_admin);

        client.pause();
        assert_auth(&env, &contract_id, &new_admin, "pause", ().into_val(&env));
        assert!(client.is_paused());

        let unpause_at = env.ledger().timestamp() + 10;
        client.schedule_unpause(&unpause_at);
        assert_auth(
            &env,
            &contract_id,
            &new_admin,
            "schedule_unpause",
            (unpause_at,).into_val(&env),
        );

        env.ledger().set_timestamp(unpause_at);
        client.unpause();
        assert_auth(&env, &contract_id, &new_admin, "unpause", ().into_val(&env));
        assert!(!client.is_paused());

        client.pause_module(&module);
        assert_auth(
            &env,
            &contract_id,
            &new_admin,
            "pause_module",
            (module.clone(),).into_val(&env),
        );
        assert!(client.is_function_paused(&module, &func));
    }

    /// Verifies the old admin cannot pause globally after admin authority is transferred.
    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn transfer_admin_revokes_old_admin_pause() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        env.mock_all_auths();
        client.transfer_admin(&new_admin);
        disable_mock_all_auths(&env);

        mock_no_arg_auth(&env, &client, &contract_id, &admin, "pause");
        client.pause();
    }

    /// Verifies the old admin cannot unpause globally after admin authority is transferred.
    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn transfer_admin_revokes_old_admin_unpause() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        env.mock_all_auths();
        client.transfer_admin(&new_admin);

        client.pause();
        let unpause_at = env.ledger().timestamp();
        client.schedule_unpause(&unpause_at);
        disable_mock_all_auths(&env);

        mock_no_arg_auth(&env, &client, &contract_id, &admin, "unpause");
        client.unpause();
    }

    /// Verifies the old admin cannot pause modules after admin authority is transferred.
    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn transfer_admin_revokes_old_admin_pause_module() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let module = symbol_short!("bill");

        client.initialize(&admin);
        env.mock_all_auths();
        client.transfer_admin(&new_admin);
        disable_mock_all_auths(&env);

        client.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "pause_module",
                args: (module.clone(),).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.pause_module(&module);
    }

    /// Verifies transferring to the same admin is a no-op that still requires current admin auth.
    #[test]
    fn transfer_admin_to_self_is_deterministic() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin = Address::generate(&env);

        client.initialize(&admin);
        env.mock_all_auths();
        client.transfer_admin(&admin);
        assert_auth(
            &env,
            &contract_id,
            &admin,
            "transfer_admin",
            (&admin,).into_val(&env),
        );

        let stored_admin = stored_admin(&env, &contract_id);
        assert_eq!(stored_admin, admin);

        client.pause();
        assert_auth(&env, &contract_id, &admin, "pause", ().into_val(&env));
        assert!(client.is_paused());
    }

    /// Verifies double transfer A->B->C revokes B and leaves only C as DataKey::Admin.
    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn transfer_admin_double_transfer_revokes_intermediate_admin() {
        let env = Env::default();
        let (contract_id, client) = setup(&env);
        let admin_a = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        client.initialize(&admin_a);
        env.mock_all_auths();
        client.transfer_admin(&admin_b);
        assert_auth(
            &env,
            &contract_id,
            &admin_a,
            "transfer_admin",
            (&admin_b,).into_val(&env),
        );
        client.transfer_admin(&admin_c);
        assert_auth(
            &env,
            &contract_id,
            &admin_b,
            "transfer_admin",
            (&admin_c,).into_val(&env),
        );

        let stored_admin = stored_admin(&env, &contract_id);
        assert_eq!(stored_admin, admin_c);

        disable_mock_all_auths(&env);
        mock_no_arg_auth(&env, &client, &contract_id, &admin_b, "pause");
        client.pause();
    }
}
