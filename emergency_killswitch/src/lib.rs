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
