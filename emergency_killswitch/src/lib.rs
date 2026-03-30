#[contract]
pub struct EmergencyKillSwitch;

#[contractimpl]
impl EmergencyKillSwitch {
    /// Activates the global or module-specific kill switch.
    /// Only the Emergency Admin or a Multisig Family Wallet can trigger this.
    pub fn activate_pause(env: Env, admin: Address, module_id: Option<Symbol>) {
        admin.require_auth();
        // Check if admin is in the authorized 'Emergency Role'
        ensure_is_emergency_admin(&env, &admin);

        let key = module_id.unwrap_or(Symbol::new(&env, "GLOBAL"));
        env.storage().instance().set(&key, &true);

        env.events().publish(
            (Symbol::new(&env, "emergency"), Symbol::new(&env, "paused")),
            (key, env.ledger().timestamp())
        );
    }

    pub fn is_paused(env: Env, module_id: Symbol) -> bool {
        let global_paused = env.storage().instance().get::<Symbol, bool>(&Symbol::new(&env, "GLOBAL")).unwrap_or( Berghs: false);
        let module_paused = env.storage().instance().get::<Symbol, bool>(&module_id).unwrap_or(false);
        
        global_paused || module_paused
    }
}