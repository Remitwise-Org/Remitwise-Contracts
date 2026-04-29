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

    pub fn pause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::GlobalPaused, &true);

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

        let schedule: Option<u64> = env.storage().instance().get(&DataKey::UnpauseSchedule);
        if let Some(time) = schedule {
            if env.ledger().timestamp() < time {
                return Err(Error::Unauthorized); // Or a better error code
            }
        }

        env.storage().instance().set(&DataKey::GlobalPaused, &false);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);

        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("unpaused")),
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
